use crate::Desktop;
use rusqlite::{Connection, OpenFlags};
use serde_json::{json, Value};
use std::{
    fs,
    io::{Read, Write},
    path::Path,
};
use tauri::Manager;
use tauri_plugin_dialog::DialogExt;

#[tauri::command]
pub async fn diagnostics_snapshot(app: tauri::AppHandle) -> Result<Value, String> {
    tauri::async_runtime::spawn_blocking(move || snapshot(&app))
        .await
        .map_err(|_| "Setup check interrupted.".to_string())?
}

fn snapshot(app: &tauri::AppHandle) -> Result<Value, String> {
    let state = app.state::<Desktop>();
    let config = state
        .connection
        .lock()
        .map_err(|_| "Settings are busy.")?
        .config
        .clone();
    let status = crate::collector::collector_state(app.state())?;
    // Use a separate client without session cookies. Never include the server address in the bundle.
    let api_status = config.server.as_ref().and_then(|origin| {
        reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .ok()?
            .get(format!("{origin}/healthz"))
            .send()
            .ok()
            .map(|r| r.status().as_u16())
    });
    let probe = state
        .data
        .join(format!(".diagnostics-{}", std::process::id()));
    let writable = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&probe)
        .is_ok();
    if writable {
        let _ = fs::remove_file(probe);
    }
    let queue = crate::collector::data_dir(&state, &config).ok().and_then(|dir| {
        let db = Connection::open_with_flags(dir.join("spool.sqlite"), OpenFlags::SQLITE_OPEN_READ_ONLY).ok()?;
        db.busy_timeout(std::time::Duration::from_millis(500)).ok()?;
        let counts: Vec<Value> = db.prepare("SELECT state,count(*),max(attempts) FROM queue GROUP BY state").ok()?
            .query_map([], |r| Ok(json!({"state": r.get::<_,String>(0)?, "count":r.get::<_,i64>(1)?, "max_attempts":r.get::<_,i64>(2)?}))).ok()?
            .filter_map(Result::ok).collect();
        let timestamps = db.query_row("SELECT max(staged_at),max(accepted_at) FROM source_health", [], |r| Ok(json!({"last_staged_at":r.get::<_,Option<i64>>(0)?,"last_accepted_at":r.get::<_,Option<i64>>(1)?}))).ok();
        Some(json!({"counts":counts,"timestamps":timestamps}))
    });
    let mut collector = serde_json::to_value(status).map_err(|_| "Cannot inspect collector.")?;
    // Native errors may include local paths. Export state flags and fixed checks only.
    collector.as_object_mut().unwrap().remove("last_exit");
    collector.as_object_mut().unwrap().remove("startup_error");
    let addon_version = config.game_root.as_ref().and_then(|root| {
        let contents = read_bounded(&root.join("Interface/AddOns/AuctionDB/AuctionDB.toc"));
        String::from_utf8(contents).ok()?.lines().find_map(|line| {
            line.strip_prefix("## Version:")
                .map(str::trim)
                .filter(|v| {
                    v.len() <= 64
                        && v.chars()
                            .all(|c| c.is_ascii_alphanumeric() || ".-_".contains(c))
                })
                .map(str::to_owned)
        })
    });
    Ok(json!({
        "release": market_telemetry::RELEASE, "revision": market_telemetry::REVISION,
        "incident_id": incident_id(), "os": std::env::consts::OS, "architecture": std::env::consts::ARCH,
        "webview2": tauri::webview_version().ok(),
        "free_disk_bytes": free_disk_bytes(&state.data),
        "addon_version": addon_version,
        "api_status": api_status, "server_configured": config.server.is_some(),
        "game_folder_exists": config.game_root.as_ref().is_some_and(|p| p.is_dir()),
        "wtf_folder_exists": config.game_root.as_ref().is_some_and(|p| p.join("WTF/Account").is_dir()),
        "game_executable_exists": config.game_exe.as_ref().is_some_and(|p| p.is_file()),
        "data_folder_writable": writable, "collector_binary_exists": state.binary.is_file(),
        "collector":collector, "queue":queue,
        "remote_reporting_enabled": !state.data.join("telemetry-disabled").exists() && market_telemetry::enabled(),
    }))
}

#[cfg(windows)]
fn free_disk_bytes(path: &Path) -> Option<u64> {
    use std::os::windows::ffi::OsStrExt;
    let path: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let mut available = 0;
    let result = unsafe {
        windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW(
            path.as_ptr(),
            &mut available,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    (result != 0).then_some(available)
}
#[cfg(not(windows))]
fn free_disk_bytes(_: &Path) -> Option<u64> {
    None
}

fn incident_id() -> String {
    // Unique enough for a local support export, without a machine or account identifier.
    format!(
        "support-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    )
}

#[tauri::command]
pub async fn export_diagnostics(
    app: tauri::AppHandle,
    frontend: Value,
) -> Result<Option<String>, String> {
    if frontend.to_string().len() > 128 * 1024 {
        return Err("Diagnostics exceed the size limit.".into());
    }
    tauri::async_runtime::spawn_blocking(move || {
        let report = snapshot(&app)?;
        let Some(destination) = app.dialog().file().add_filter("Diagnostics ZIP", &["zip"])
            .set_file_name("nauctions-diagnostics.zip").blocking_save_file() else { return Ok(None); };
        let destination = destination.into_path().map_err(|_| "Select a local file.")?;
        let state = app.state::<Desktop>();
        let mut entries = Vec::new();
        let mut add = |name: &'static str, bytes: &[u8]| -> Result<(), String> {
            entries.push((name, bytes.to_vec()));
            Ok(())
        };
        add("setup.json", serde_json::to_string_pretty(&report).unwrap().as_bytes())?;
        add("interface.json", serde_json::to_string_pretty(&safe_frontend(frontend)).unwrap().as_bytes())?;
        for name in ["desktop-diagnostics.jsonl", "desktop-diagnostics.jsonl.1"] {
            add(name, &read_bounded(&state.data.join(name)))?;
        }
        let config = state.connection.lock().map_err(|_| "Settings are busy.")?.config.clone();
        if let Ok(dir) = crate::collector::data_dir(&state, &config) {
            for name in ["collector-diagnostics.jsonl", "collector-diagnostics.jsonl.1"] {
                add(name, &read_bounded(&dir.join(name)))?;
            }
        }
        add("README.txt", b"Share this file with the Nauctions maintainer. Raw collector logs, credentials, saved settings, game files, and database contents are excluded. Queue counts and last transfer timestamps are included.\n")?;
        write_archive(fs::File::create(&destination).map_err(|_| "Cannot create diagnostics file.")?, entries)?;
        market_telemetry::record("diagnostics_exported", json!({}));
        Ok(Some(destination.to_string_lossy().into_owned()))
    }).await.map_err(|_| "Diagnostics export interrupted.".to_string())?
}

fn write_archive<W: Write + std::io::Seek>(
    writer: W,
    entries: Vec<(&str, Vec<u8>)>,
) -> Result<W, String> {
    let mut zip = zip::ZipWriter::new(writer);
    let options = zip::write::SimpleFileOptions::default();
    for (name, bytes) in entries {
        zip.start_file(name, options)
            .map_err(|_| "Cannot create ZIP entry.")?;
        zip.write_all(&bytes)
            .map_err(|_| "Cannot write diagnostics file.")?;
    }
    zip.finish()
        .map_err(|_| "Cannot finish diagnostics ZIP.".into())
}

fn read_bounded(path: &Path) -> Vec<u8> {
    let mut bytes = Vec::new();
    if let Ok(file) = fs::File::open(path) {
        let _ = file.take(1024 * 1024).read_to_end(&mut bytes);
    }
    bytes
}

fn safe_frontend(frontend: Value) -> Value {
    let entries = frontend
        .as_array()
        .into_iter()
        .flatten()
        .rev()
        .take(100)
        .map(|entry| {
            let mut result = json!({});
            for key in ["at", "code", "level"] {
                if let Some(value) = entry[key].as_str().filter(|s| {
                    s.len() <= 100
                        && s.chars()
                            .all(|c| c.is_ascii_alphanumeric() || "_-.:TZ".contains(c))
                }) {
                    result[key] = json!(value);
                }
            }
            let mut properties = json!({});
            for key in [
                "release",
                "revision",
                "component",
                "screen",
                "action",
                "outcome",
                "operation",
                "status",
                "duration_ms",
                "incident_id",
                "request_id",
            ] {
                let value = &entry["properties"][key];
                if value.is_number()
                    || value.is_boolean()
                    || value.as_str().is_some_and(|s| {
                        s.len() <= 100
                            && s.chars()
                                .all(|c| c.is_ascii_alphanumeric() || "_-./:".contains(c))
                    })
                {
                    properties[key] = value.clone();
                }
            }
            result["properties"] = properties;
            result
        })
        .collect::<Vec<_>>();
    json!(entries)
}

#[tauri::command]
pub fn observability_preference(state: tauri::State<'_, Desktop>) -> bool {
    !state.data.join("telemetry-disabled").exists()
}

#[tauri::command]
pub fn set_observability(enabled: bool, state: tauri::State<'_, Desktop>) -> Result<(), String> {
    let path = state.data.join("telemetry-disabled");
    if enabled {
        if path.exists() {
            fs::remove_file(path).map_err(|_| "Cannot save reporting preference.")?;
        }
    } else {
        fs::write(path, b"off").map_err(|_| "Cannot save reporting preference.")?;
    }
    market_telemetry::set_enabled(enabled);
    // Apply the preference to the next collector process. The supervisor resumes it when enabled.
    state
        .collector
        .lock()
        .map_err(|_| "Collector is busy.")?
        .stop()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn support_zip_roundtrip_uses_the_patched_writer() {
        let safe = serde_json::to_vec(&safe_frontend(
            json!([{"code":"test", "properties":{"token":"private","status":503}}]),
        ))
        .unwrap();
        let archive = write_archive(
            std::io::Cursor::new(Vec::new()),
            vec![
                ("interface.json", safe.clone()),
                ("README.txt", b"Synthetic support export".to_vec()),
            ],
        )
        .unwrap();
        let mut zip = zip::ZipArchive::new(archive).unwrap();
        assert_eq!(zip.len(), 2);
        let mut bytes = Vec::new();
        zip.by_name("interface.json")
            .unwrap()
            .read_to_end(&mut bytes)
            .unwrap();
        assert_eq!(bytes, safe);
        assert!(!String::from_utf8(bytes).unwrap().contains("private"));
    }
    #[test]
    fn support_export_does_not_copy_arbitrary_interface_data() {
        let result = safe_frontend(
            json!([{"code":"api_request_failed","token":"secret","properties":{"body":"private","password":"secret","operation":"C:\\Users\\Alice","status":503}}]),
        );
        assert_eq!(result[0]["properties"]["status"], 503);
        let text = result.to_string();
        assert!(!text.contains("secret"));
        assert!(!text.contains("Alice"));
        assert!(!text.contains("private"));
    }
}
