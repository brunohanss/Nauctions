use serde::Serialize;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use tauri::{ipc::Channel, Manager};
use tauri_plugin_updater::{Update, UpdaterExt};

#[derive(Default)]
pub struct Updates {
    operation: Arc<AtomicBool>,
    pub installing: Arc<AtomicBool>,
    pending: Mutex<Option<Update>>,
}

// The guard also releases the operation when a task fails or is cancelled.
struct Busy(Arc<AtomicBool>);
impl Busy {
    fn acquire(flag: &Arc<AtomicBool>) -> Result<Self, String> {
        flag.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .map_err(|_| "An update operation is already running.")?;
        Ok(Self(flag.clone()))
    }
}
impl Drop for Busy {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

#[derive(Serialize)]
pub struct UpdateInfo {
    current_version: String,
    version: Option<String>,
    notes: Option<String>,
}
#[derive(Clone, Serialize)]
pub struct Progress {
    phase: &'static str,
    downloaded: u64,
    total: Option<u64>,
}

#[tauri::command]
pub async fn check_update(app: tauri::AppHandle) -> Result<UpdateInfo, String> {
    let state = app.state::<Updates>();
    let _busy = Busy::acquire(&state.operation)?;
    *state
        .pending
        .lock()
        .map_err(|_| "Update state is unavailable.")? = None;
    let update = app
        .updater_builder()
        .timeout(std::time::Duration::from_secs(120))
        .configure_client(|client| client.https_only(true))
        .build()
        .map_err(|_| "Cannot configure app updates.")?
        .check()
        .await
        .map_err(|_| "Cannot check for updates. Check your connection or try again later.")?;
    if let Some(update) = &update {
        if update.download_url.scheme() != "https" {
            return Err("The update download must use HTTPS.".into());
        }
    }
    let info = UpdateInfo {
        current_version: app.package_info().version.to_string(),
        version: update.as_ref().map(|u| u.version.clone()),
        notes: update.as_ref().and_then(|u| u.body.clone()),
    };
    *state
        .pending
        .lock()
        .map_err(|_| "Update state is unavailable.")? = update;
    Ok(info)
}

#[tauri::command]
pub async fn install_update(
    app: tauri::AppHandle,
    progress: Channel<Progress>,
) -> Result<(), String> {
    let state = app.state::<Updates>();
    let busy = Busy::acquire(&state.operation)?;
    let update = state
        .pending
        .lock()
        .map_err(|_| "Update state is unavailable.")?
        .clone()
        .ok_or("Check for an update before installing.")?;
    let mut downloaded = 0;
    let bytes = update
        .download(
            |chunk, total| {
                downloaded += chunk as u64;
                let _ = progress.send(Progress {
                    phase: "downloading",
                    downloaded,
                    total,
                });
            },
            || {},
        )
        .await
        .map_err(|_| {
            "The update could not be downloaded or verified. Collection continues. Try again later."
        })?;

    // Download and signature verification finish before the collector is stopped.
    // The installer exits Windows directly, so do not rely on Tauri Exit events.
    tauri::async_runtime::spawn_blocking(move || {
        let _busy = busy;
        let updates = app.state::<Updates>();
        let _installing = Busy::acquire(&updates.installing)?;
        let desktop = app.state::<crate::Desktop>();
        let mut collector = desktop.collector.try_lock().map_err(|_| {
            "A collector action is running. Wait for it to finish, then install the update."
        })?;
        collector.stop()?;
        let _ = progress.send(Progress {
            phase: "installing",
            downloaded: bytes.len() as u64,
            total: Some(bytes.len() as u64),
        });
        update.install(bytes).map_err(|_| {
            "Cannot start the update installer. Automatic collection will resume if enabled."
                .to_string()
        })?;
        Ok(())
    })
    .await
    .map_err(|_| "The update task failed. Try again later.".to_string())?
}

pub fn installation_active(app: &tauri::AppHandle) -> bool {
    app.state::<Updates>().installing.load(Ordering::SeqCst)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "requires NAUCTIONS_RELEASE_INSTALLER and a signed release package"]
    fn release_installer_matches_the_shipped_key_and_manifest() {
        let config: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        let path =
            std::env::var("NAUCTIONS_RELEASE_INSTALLER").expect("set NAUCTIONS_RELEASE_INSTALLER");
        let installer = std::path::Path::new(&path);
        let bytes = std::fs::read(installer).unwrap();
        let manifest: serde_json::Value = serde_json::from_slice(
            &std::fs::read(installer.parent().unwrap().join("latest.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(manifest["version"], config["version"]);
        let signature = std::fs::read_to_string(format!("{path}.sig")).unwrap();
        assert_eq!(
            manifest["platforms"]["windows-x86_64"]["signature"]
                .as_str()
                .unwrap(),
            signature.trim()
        );
        let version = config["version"].as_str().unwrap();
        let expected_url = format!("https://github.com/brunohanss/Nauctions/releases/download/v{version}/Nauctions_{version}_x64-setup.exe");
        assert_eq!(
            manifest["platforms"]["windows-x86_64"]["url"]
                .as_str()
                .unwrap(),
            expected_url
        );
        let mut context = tauri::test::mock_context(tauri::test::noop_assets());
        context
            .config_mut()
            .plugins
            .0
            .insert("updater".into(), config["plugins"]["updater"].clone());
        let app = tauri::test::mock_builder()
            .plugin(tauri_plugin_updater::Builder::new().build())
            .build(context)
            .unwrap();
        // Select the package independently of the mock app's current version.
        let mut selection = manifest.clone();
        selection["version"] = "99.0.0".into();
        let mut update = app
            .updater_builder()
            .target("windows-x86_64")
            .build()
            .unwrap()
            .restore_update(selection)
            .unwrap()
            .unwrap();
        update.version = version.into();
        update
            .verify(&bytes)
            .expect("installer must match the shipped release key and announced version");
        update.version = "99.0.0".into();
        assert!(update.verify(&bytes).is_err());
        let mut changed = bytes;
        changed[0] ^= 1;
        update.version = version.into();
        assert!(update.verify(&changed).is_err());
    }
    #[test]
    fn signed_updates_reject_modified_bytes_wrong_keys_and_inflated_versions() {
        let key = include_str!("../tests/fixtures/update-test.pub").trim();
        let signature = include_str!("../tests/fixtures/update-payload.txt.sig").trim();
        let payload = include_bytes!("../tests/fixtures/update-payload.txt");
        let mut context = tauri::test::mock_context(tauri::test::noop_assets());
        context.config_mut().plugins.0.insert(
            "updater".into(),
            serde_json::json!({
            "pubkey": key, "requireSignedVersion": true,
            "endpoints": ["https://example.invalid/latest.json"]
            }),
        );
        let app = tauri::test::mock_builder()
            .plugin(tauri_plugin_updater::Builder::new().build())
            .build(context)
            .unwrap();
        let manifest = |version| {
            serde_json::json!({
                "version": version,
                "platforms": {"windows-x86_64": {
                    "url": "https://example.invalid/test.exe", "signature": signature
                }}
            })
        };
        let updater = app
            .updater_builder()
            .target("windows-x86_64")
            .build()
            .unwrap();
        let update = updater.restore_update(manifest("99.0.0")).unwrap().unwrap();
        assert!(update.verify(payload).is_ok());
        assert!(update.verify(b"modified download").is_err());
        let inflated = updater.restore_update(manifest("99.0.1")).unwrap().unwrap();
        assert!(inflated.verify(payload).is_err());
        let wrong_key = app
            .updater_builder()
            .target("windows-x86_64")
            .pubkey("invalid-key")
            .build()
            .unwrap()
            .restore_update(manifest("99.0.0"))
            .unwrap()
            .unwrap();
        assert!(wrong_key.verify(payload).is_err());
        assert!(updater.restore_update(manifest("0.0.0")).unwrap().is_none());
    }
    #[test]
    fn operation_excludes_duplicates_and_releases_on_failure() {
        let flag = Arc::new(AtomicBool::new(false));
        let operation = Busy::acquire(&flag).unwrap();
        assert!(Busy::acquire(&flag).is_err());
        drop(operation);
        assert!(Busy::acquire(&flag).is_ok());
    }
}
