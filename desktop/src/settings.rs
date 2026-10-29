use crate::Config;
use std::{
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

const PRIMARY: &str = "desktop.json";
const BACKUP: &str = "desktop.backup.json";

fn decode(bytes: &[u8]) -> Option<Config> {
    if bytes.len() > 64 * 1024 {
        return None;
    }
    let config: Config = serde_json::from_slice(bytes).ok()?;
    if let Some(server) = &config.server {
        crate::transport::server_origin(server).ok()?;
    }
    Some(config)
}

fn read(path: &Path) -> Result<Option<Vec<u8>>, String> {
    match fs::File::open(path) {
        Ok(file) => {
            let mut bytes = Vec::new();
            file.take(64 * 1024 + 1)
                .read_to_end(&mut bytes)
                .map_err(|_| "Cannot read desktop settings.")?;
            Ok(Some(bytes))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(_) => {
            Err("Cannot read desktop settings. Check access to the application data folder.".into())
        }
    }
}

fn unique(path: &Path, extension: &str) -> PathBuf {
    path.with_extension(format!(
        "{}-{}.{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos(),
        extension
    ))
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let temporary = unique(path, "tmp");
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|_| "Cannot create temporary desktop settings.")?;
        file.write_all(bytes)
            .and_then(|_| file.sync_all())
            .map_err(|_| "Cannot write desktop settings.")?;
        drop(file);
        #[cfg(windows)]
        {
            use std::os::windows::ffi::OsStrExt;
            use windows_sys::Win32::Storage::FileSystem::{
                MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
            };
            let source: Vec<u16> = temporary.as_os_str().encode_wide().chain(Some(0)).collect();
            let target: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
            if unsafe {
                MoveFileExW(
                    source.as_ptr(),
                    target.as_ptr(),
                    MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
                )
            } == 0
            {
                return Err("Cannot commit desktop settings.".into());
            }
        }
        #[cfg(not(windows))]
        fs::rename(&temporary, path).map_err(|_| "Cannot commit desktop settings.")?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(temporary);
    }
    result
}

fn preserve(path: &Path) -> Result<(), String> {
    if path.exists() {
        fs::rename(path, unique(path, "corrupt.json"))
            .map_err(|_| "Cannot preserve damaged desktop settings. The original file was kept.")?;
    }
    Ok(())
}

pub fn save(data: &Path, config: &Config) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(config).map_err(|_| "Cannot encode desktop settings.")?;
    if decode(&bytes).is_none() {
        return Err("Cannot save invalid desktop settings.".into());
    }
    let previous = read(&data.join(PRIMARY))?.filter(|bytes| decode(bytes).is_some());
    let backup = read(&data.join(BACKUP))?.filter(|bytes| decode(bytes).is_some());
    // Keep a valid recovery copy before changing the primary file. Never back up
    // a truncated primary over a valid backup.
    if let Some(previous) = previous.as_ref() {
        atomic_write(&data.join(BACKUP), previous)?;
    } else if backup.is_none() {
        preserve(&data.join(BACKUP))?;
        atomic_write(&data.join(BACKUP), &bytes)?;
    }
    atomic_write(&data.join(PRIMARY), &bytes)
}

pub fn load(data: &Path) -> Result<(Config, Option<String>), String> {
    let primary = read(&data.join(PRIMARY))?;
    if let Some(config) = primary.as_deref().and_then(decode) {
        let backup = read(&data.join(BACKUP))?;
        if backup.as_deref().and_then(decode).is_none() {
            preserve(&data.join(BACKUP))?;
            atomic_write(&data.join(BACKUP), primary.as_deref().unwrap())?;
        }
        return Ok((config, None));
    }
    let backup = read(&data.join(BACKUP))?;
    if let Some(config) = backup.as_deref().and_then(decode) {
        preserve(&data.join(PRIMARY))?;
        atomic_write(&data.join(PRIMARY), backup.as_deref().unwrap())?;
        return Ok((config, Some("Desktop settings were restored from the last valid backup. Check your preferences. Damaged settings were kept in the application data folder.".into())));
    }
    if primary.is_none() && backup.is_none() {
        return Ok((Config::default(), None));
    }
    preserve(&data.join(PRIMARY))?;
    preserve(&data.join(BACKUP))?;
    let config = Config {
        launch_at_login: false,
        automatic_collection: false,
        ..Config::default()
    };
    save(data, &config)?;
    Ok((config, Some("Desktop settings could not be restored. Collection and Windows startup are disabled. Damaged settings were kept in the application data folder. Reconnect the server and check your preferences.".into())))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interrupted_settings_recover_without_losing_pairing_or_queue_files() {
        let dir = tempfile::tempdir().unwrap();
        let mut config = Config {
            server: Some("https://example.com".into()),
            launch_at_login: false,
            automatic_collection: false,
            ..Config::default()
        };
        save(dir.path(), &config).unwrap();
        config.automatic_collection = true;
        save(dir.path(), &config).unwrap();
        fs::write(dir.path().join("credential.dpapi"), b"membership sentinel").unwrap();
        fs::write(dir.path().join("spool.sqlite"), b"queue sentinel").unwrap();
        fs::write(dir.path().join(PRIMARY), b"{truncated").unwrap();
        let (restored, warning) = load(dir.path()).unwrap();
        assert_eq!(restored.server, config.server);
        assert!(!restored.launch_at_login && !restored.automatic_collection);
        assert!(warning.is_some());
        assert!(load(dir.path()).unwrap().1.is_none());
        assert_eq!(
            fs::read(dir.path().join("credential.dpapi")).unwrap(),
            b"membership sentinel"
        );
        assert_eq!(
            fs::read(dir.path().join("spool.sqlite")).unwrap(),
            b"queue sentinel"
        );
        assert!(fs::read_dir(dir.path()).unwrap().any(|entry| entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .ends_with("corrupt.json")));
    }

    #[test]
    fn missing_primary_uses_backup_and_two_bad_copies_start_paused() {
        let dir = tempfile::tempdir().unwrap();
        save(dir.path(), &Config::default()).unwrap();
        fs::remove_file(dir.path().join(PRIMARY)).unwrap();
        assert!(load(dir.path()).unwrap().1.is_some());
        fs::write(dir.path().join(PRIMARY), b"{").unwrap();
        fs::write(dir.path().join(BACKUP), b"{").unwrap();
        let (config, warning) = load(dir.path()).unwrap();
        assert!(!config.launch_at_login && !config.automatic_collection);
        assert!(config.server.is_none() && warning.is_some());
        assert_eq!(
            fs::read_dir(dir.path())
                .unwrap()
                .filter(|entry| entry
                    .as_ref()
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .ends_with("corrupt.json"))
                .count(),
            2
        );
    }

    #[test]
    fn failed_primary_commit_keeps_the_previous_settings() {
        let dir = tempfile::tempdir().unwrap();
        save(dir.path(), &Config::default()).unwrap();
        // A directory cannot be replaced by a settings file.
        let blocked = dir.path().join("blocked");
        fs::create_dir(&blocked).unwrap();
        assert!(atomic_write(&blocked, b"new settings").is_err());
        assert!(load(dir.path()).unwrap().1.is_none());
        assert!(!fs::read_dir(dir.path()).unwrap().any(|entry| entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .ends_with(".tmp")));
    }
}
