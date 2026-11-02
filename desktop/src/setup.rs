//! Local installation discovery and recoverable installation of the bundled addon.
use crate::{Config, Desktop};
use std::{
    fs,
    path::{Path, PathBuf},
};
use tauri::Manager;

include!(concat!(env!("OUT_DIR"), "/bundled_addon.rs"));

fn valid_root(path: &Path) -> bool {
    path.join("WTF/Account").is_dir()
        && fs::read_dir(path).is_ok_and(|entries| {
            entries.flatten().any(|entry| {
                let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
                name.starts_with("wow") && name.ends_with(".exe") && entry.path().is_file()
            })
        })
}

fn candidates(seeds: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut found = Vec::new();
    for seed in seeds {
        // A launcher can point at the product directory above the actual client.
        for path in std::iter::once(seed.clone())
            .chain(["_classic_", "_classic_era_", "_retail_"].map(|name| seed.join(name)))
        {
            if valid_root(&path) {
                if let Ok(path) = path.canonicalize() {
                    if !found.contains(&path) {
                        found.push(path);
                    }
                }
            }
        }
    }
    found.sort();
    found
}

fn discovery_seeds() -> Vec<PathBuf> {
    let mut seeds = Vec::new();
    #[cfg(windows)]
    {
        use windows_registry::{CURRENT_USER, LOCAL_MACHINE};
        for hive in [CURRENT_USER, LOCAL_MACHINE] {
            for branch in [
                r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall",
                r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall",
            ] {
                if let Ok(key) = hive.open(branch) {
                    if let Ok(names) = key.keys() {
                        for name in names {
                            if let Ok(product) = key.open(name) {
                                let title = product
                                    .get_string("DisplayName")
                                    .unwrap_or_default()
                                    .to_lowercase();
                                if title.contains("warcraft") || title.contains("wow") {
                                    if let Ok(path) = product.get_string("InstallLocation") {
                                        seeds.push(PathBuf::from(path));
                                    }
                                }
                            }
                        }
                    }
                }
            }
            for branch in [
                r"SOFTWARE\Blizzard Entertainment\World of Warcraft",
                r"SOFTWARE\WOW6432Node\Blizzard Entertainment\World of Warcraft",
            ] {
                if let Ok(key) = hive.open(branch) {
                    for field in ["InstallPath", "GamePath"] {
                        if let Ok(path) = key.get_string(field) {
                            seeds.push(PathBuf::from(path));
                        }
                    }
                }
            }
        }
        // Process image paths also find portable clients in nonstandard directories.
        use windows_sys::Win32::{
            Foundation::*,
            System::{Diagnostics::ToolHelp::*, Threading::*},
        };
        unsafe {
            let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
            if snapshot != INVALID_HANDLE_VALUE {
                let mut entry: PROCESSENTRY32W = std::mem::zeroed();
                entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
                let mut next = Process32FirstW(snapshot, &mut entry);
                while next != 0 {
                    let len = entry
                        .szExeFile
                        .iter()
                        .position(|c| *c == 0)
                        .unwrap_or(entry.szExeFile.len());
                    let name = String::from_utf16_lossy(&entry.szExeFile[..len]).to_lowercase();
                    if name.starts_with("wow") && name.ends_with(".exe") {
                        let handle =
                            OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, entry.th32ProcessID);
                        if !handle.is_null() {
                            let mut image = vec![0u16; 32768];
                            let mut size = image.len() as u32;
                            if QueryFullProcessImageNameW(handle, 0, image.as_mut_ptr(), &mut size)
                                != 0
                            {
                                if let Some(parent) =
                                    PathBuf::from(String::from_utf16_lossy(&image[..size as usize]))
                                        .parent()
                                {
                                    seeds.push(parent.to_path_buf());
                                }
                            }
                            CloseHandle(handle);
                        }
                    }
                    next = Process32NextW(snapshot, &mut entry);
                }
                CloseHandle(snapshot);
            }
        }
        let mut locations: Vec<PathBuf> = ["ProgramFiles", "ProgramFiles(x86)", "USERPROFILE"]
            .iter()
            .filter_map(std::env::var_os)
            .map(PathBuf::from)
            .collect();
        if let Some(home) = std::env::var_os("USERPROFILE") {
            for name in ["Downloads", "Desktop", "Documents", "Games"] {
                locations.push(PathBuf::from(&home).join(name));
            }
        }
        for drive in b'C'..=b'Z' {
            let root = PathBuf::from(format!("{}:\\", drive as char));
            if root.is_dir() {
                locations.push(root.clone());
                for name in ["Games", "Blizzard", "Battle.net"] {
                    locations.push(root.join(name));
                }
            }
        }
        for location in locations {
            seeds.push(location.clone());
            if let Ok(entries) = fs::read_dir(location) {
                for entry in entries.take(1000).flatten() {
                    let name = entry.file_name().to_string_lossy().to_lowercase();
                    if name.contains("wow") || name.contains("warcraft") || name.contains("forever")
                    {
                        seeds.push(entry.path());
                        // Portable distributions sometimes contain one nested client folder.
                        if let Ok(children) = fs::read_dir(entry.path()) {
                            seeds.extend(children.take(100).flatten().map(|e| e.path()));
                        }
                    }
                }
            }
        }
    }
    seeds
}

#[tauri::command]
pub async fn discover_wow(app: tauri::AppHandle) -> Result<Vec<PathBuf>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<Desktop>();
        let config = state
            .connection
            .lock()
            .map_err(|_| "Desktop settings are unavailable.")?
            .config
            .clone();
        if let Some(root) = config.game_root.filter(|root| valid_root(root)) {
            return Ok(vec![root]);
        }
        Ok(candidates(discovery_seeds()))
    })
    .await
    .map_err(|_| "WoW search failed.".to_string())?
}

fn ordinary_directory(path: &Path) -> Result<(), String> {
    if let Ok(meta) = fs::symlink_metadata(path) {
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if meta.file_attributes() & 0x400 != 0 {
                return Err("The addon path contains a link. Select a regular WoW folder.".into());
            }
        }
        if !meta.is_dir() || meta.file_type().is_symlink() {
            return Err("The addon path is not a regular directory.".into());
        }
    }
    Ok(())
}

fn addon_current(target: &Path) -> bool {
    ADDON.iter().all(|(name, bytes)| {
        let path = target.join(name);
        fs::symlink_metadata(&path).is_ok_and(|meta| {
            #[cfg(windows)]
            {
                use std::os::windows::fs::MetadataExt;
                if meta.file_attributes() & 0x400 != 0 {
                    return false;
                }
            }
            meta.is_file()
                && !meta.file_type().is_symlink()
                && fs::read(path).is_ok_and(|actual| actual == *bytes)
        })
    })
}

fn install_addon(root: &Path, game: market_game_process::GameState) -> Result<(), String> {
    let interface = root.join("Interface");
    let addons = interface.join("AddOns");
    let target = addons.join("AuctionDB");
    for path in [&interface, &addons, &target] {
        ordinary_directory(path)?;
    }
    if addon_current(&target) {
        return Ok(());
    }
    if game != market_game_process::GameState::Closed {
        return Err("Close WoW, then select Retry setup to install the addon.".into());
    }
    fs::create_dir_all(&addons)
        .map_err(|_| "Cannot create the addon folder. Check folder permissions.")?;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    let staging = addons.join(format!(".nauctions-install-{stamp}"));
    fs::create_dir(&staging).map_err(|_| "Cannot prepare addon installation.")?;
    let result = (|| {
        for (name, bytes) in ADDON {
            fs::write(staging.join(name), bytes).map_err(|_| "Cannot write the bundled addon.")?;
        }
        let backup_root = root.join("NauctionsBackups");
        ordinary_directory(&backup_root)?;
        let backup = backup_root.join(format!("AuctionDB-{stamp}"));
        let existed = target.exists();
        if existed {
            fs::create_dir_all(&backup_root)
                .map_err(|_| "Cannot create the addon backup folder.")?;
            fs::rename(&target, &backup).map_err(|_| "Cannot back up the existing addon.")?;
        }
        if fs::rename(&staging, &target).is_err() {
            if existed && fs::rename(&backup, &target).is_err() {
                return Err(format!(
                    "Cannot install the addon. Restore the original addon from {}.",
                    backup.display()
                ));
            }
            return Err("Cannot install the addon. The original addon was retained.".into());
        }
        Ok(())
    })();
    // This is our newly created staging directory and contains only bundled files.
    if staging.canonicalize().is_ok_and(|path| {
        path.starts_with(root) && path.parent() == addons.canonicalize().ok().as_deref()
    }) {
        let _ = fs::remove_dir_all(staging);
    }
    result
}

#[tauri::command]
pub async fn prepare_wow(root: String, app: tauri::AppHandle) -> Result<Config, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let root = PathBuf::from(root)
            .canonicalize()
            .map_err(|_| "The WoW folder is unavailable.")?;
        if !valid_root(&root) {
            return Err("Select a WoW folder with a WoW executable and WTF\\Account.".into());
        }
        let state = app.state::<Desktop>();
        let mut runner = state.collector.lock().map_err(|_| "Collector is busy.")?;
        if crate::updates::installation_active(&app) {
            return Err("Wait for the app update to finish.".into());
        }
        let mut conn = state
            .connection
            .lock()
            .map_err(|_| "Desktop settings are unavailable.")?;
        if state.data.starts_with(&root) {
            return Err("The WoW folder cannot contain Nauctions application data.".into());
        }
        {
            let _activity = crate::tray_activity::begin(&app);
            install_addon(&root, market_game_process::detect(&root, None))?;
        }
        let mut config = conn.config.clone();
        if config.game_root.as_ref() != Some(&root) {
            runner.stop()?;
            config.game_root = Some(root);
            config.game_exe = None;
            config.automatic_collection = false;
            state.save(&config)?;
            conn.config = config.clone();
        }
        Ok(config)
    })
    .await
    .map_err(|_| "WoW setup failed.".to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn discovery_validates_deduplicates_and_finds_product_clients() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("_classic_era_");
        fs::create_dir_all(root.join("WTF/Account")).unwrap();
        assert!(candidates(vec![dir.path().into()]).is_empty());
        fs::write(root.join("WoW.exe"), b"test").unwrap();
        assert_eq!(
            candidates(vec![dir.path().into(), root.clone()]),
            vec![root.canonicalize().unwrap()]
        );
    }
    #[test]
    fn addon_install_keeps_original_and_saved_variables_and_is_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let target = root.join("Interface/AddOns/AuctionDB");
        fs::create_dir_all(&target).unwrap();
        fs::write(target.join("custom.lua"), b"original addon").unwrap();
        fs::create_dir_all(root.join("WTF/Account")).unwrap();
        fs::write(root.join("WTF/Account/AuctionDB.lua"), b"saved data").unwrap();
        assert!(install_addon(root, market_game_process::GameState::Running).is_err());
        assert!(target.join("custom.lua").exists());
        install_addon(root, market_game_process::GameState::Closed).unwrap();
        assert!(addon_current(&target));
        let backups: Vec<_> = fs::read_dir(root.join("NauctionsBackups"))
            .unwrap()
            .flatten()
            .collect();
        assert_eq!(backups.len(), 1);
        assert_eq!(
            fs::read(backups[0].path().join("custom.lua")).unwrap(),
            b"original addon"
        );
        assert_eq!(
            fs::read(root.join("WTF/Account/AuctionDB.lua")).unwrap(),
            b"saved data"
        );
        install_addon(root, market_game_process::GameState::Running).unwrap();
        assert_eq!(
            fs::read_dir(root.join("NauctionsBackups")).unwrap().count(),
            1
        );
    }
    #[test]
    fn addon_install_rejects_file_in_place_of_directory() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("Interface"), b"keep").unwrap();
        assert!(install_addon(dir.path(), market_game_process::GameState::Closed).is_err());
        assert_eq!(fs::read(dir.path().join("Interface")).unwrap(), b"keep");
    }
}
