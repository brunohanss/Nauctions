use super::*;
use std::io::Write;

#[derive(clap::Args)]
pub struct Options {
    #[arg(long)]
    realm: String,
    #[arg(long)]
    house: String,
    #[arg(long)]
    dry_run: bool,
    /// Select one account directory name; omit to sync every discovered account.
    #[arg(long)]
    account: Option<String>,
}

#[cfg(windows)]
fn closed() -> Result<()> {
    use windows_sys::Win32::{
        Foundation::{CloseHandle, ERROR_NO_MORE_FILES, INVALID_HANDLE_VALUE},
        System::Diagnostics::ToolHelp::*,
    };
    unsafe {
        let handle = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if handle == INVALID_HANDLE_VALUE {
            return Err(std::io::Error::last_os_error().into());
        }
        let result = (|| {
            let mut entry: PROCESSENTRY32W = std::mem::zeroed();
            entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
            if Process32FirstW(handle, &mut entry) == 0 {
                bail!("cannot enumerate processes; sync refused");
            }
            loop {
                let len = entry
                    .szExeFile
                    .iter()
                    .position(|x| *x == 0)
                    .unwrap_or(entry.szExeFile.len());
                let name = String::from_utf16_lossy(&entry.szExeFile[..len]).to_ascii_lowercase();
                if name.starts_with("wow") && name.ends_with(".exe") {
                    bail!("close all WoW clients before sync");
                }
                if Process32NextW(handle, &mut entry) == 0 {
                    if std::io::Error::last_os_error().raw_os_error()
                        != Some(ERROR_NO_MORE_FILES as i32)
                    {
                        bail!("process enumeration failed; sync refused");
                    }
                    break;
                }
            }
            Ok(())
        })();
        CloseHandle(handle);
        result
    }
}
#[cfg(not(windows))]
fn closed() -> Result<()> {
    bail!("sync requires Windows")
}

fn safe_path(root: &Path, path: &Path) -> Result<()> {
    let root = root.canonicalize()?;
    if !path.canonicalize()?.starts_with(&root) {
        bail!("path is outside selected installation");
    }
    let mut cursor = path.to_path_buf();
    loop {
        if is_link(&cursor) {
            bail!("links and reparse points are refused");
        }
        if cursor.canonicalize()? == root {
            break;
        }
        if !cursor.pop() {
            bail!("invalid source path");
        }
    }
    Ok(())
}

#[cfg(windows)]
fn replace(source: &Path, temp: &Path) -> Result<()> {
    use std::os::windows::ffi::OsStrExt;
    let wide = |p: &Path| {
        p.as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect::<Vec<_>>()
    };
    let source = wide(source);
    let temp = wide(temp);
    if unsafe {
        windows_sys::Win32::Storage::FileSystem::ReplaceFileW(
            source.as_ptr(),
            temp.as_ptr(),
            std::ptr::null(),
            0,
            std::ptr::null(),
            std::ptr::null(),
        )
    } == 0
    {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(())
}
#[cfg(not(windows))]
fn replace(_: &Path, _: &Path) -> Result<()> {
    bail!("sync requires Windows")
}

fn guarded_replace(
    root: &Path,
    path: &Path,
    temp: &Path,
    original: &[u8],
    modified: SystemTime,
    guard: impl Fn() -> Result<()>,
) -> Result<()> {
    guard()?;
    safe_path(root, path)?;
    safe_path(root, temp)?;
    if fs::metadata(path)?.modified()? != modified || bounded_read(path)? != original {
        bail!("source changed before replacement; source retained");
    }
    replace(path, temp)
}

// SQLite's exclusive transaction releases the lock automatically if the process exits.
pub fn run(dir: &Path, db: &mut Connection, options: Options, exe: Option<PathBuf>) -> Result<()> {
    if !options.dry_run {
        bail!("shared-history writes are disabled pending recovery and game compatibility validation; use sync --dry-run");
    }
    run_with_guard(dir, db, options, exe, &closed)
}
fn run_with_guard(
    dir: &Path,
    db: &mut Connection,
    options: Options,
    exe: Option<PathBuf>,
    guard: &dyn Fn() -> Result<()>,
) -> Result<()> {
    guard()?;
    let config: Config = serde_json::from_slice(
        &fs::read(dir.join("config.json")).context("pair this device first")?,
    )?;
    validate_server(&config.server, config.allow_localhost_http)?;
    let root = config.root.canonicalize()?;
    safe_path(&root, &root)?;
    if dir.canonicalize()?.starts_with(&root) || is_link(dir) {
        bail!("use a private collector data directory outside WoW");
    }
    let game = exe
        .map(|p| -> Result<PathBuf> {
            safe_path(&root, &p)?;
            let p = p.canonicalize()?;
            let name = p
                .file_name()
                .and_then(|x| x.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            if !p.is_file()
                || !matches!(
                    name.as_str(),
                    "wow.exe" | "wowclassic.exe" | "wow-64.exe" | "wowclassic_t.exe"
                )
            {
                bail!("select a WoW executable within the paired installation");
            }
            Ok(p)
        })
        .transpose()?;
    if db
        .query_row("SELECT value FROM settings WHERE key='paused'", [], |r| {
            r.get::<_, String>(0)
        })
        .optional()?
        .is_some()
    {
        bail!("authentication paused; pair again");
    }
    let mut lock = Connection::open(dir.join("sync-lock.sqlite"))?;
    lock.busy_timeout(Duration::from_millis(1))?;
    let _exclusive = lock
        .transaction_with_behavior(rusqlite::TransactionBehavior::Exclusive)
        .context("another sync is active")?;
    db.execute_batch("CREATE TABLE IF NOT EXISTS sync_journal(source TEXT PRIMARY KEY,new_hash TEXT NOT NULL,backup TEXT NOT NULL)")?;
    let mut files = discover(&root)?;
    if let Some(account) = &options.account {
        files.retain(|p| {
            p.parent()
                .and_then(Path::parent)
                .and_then(Path::file_name)
                .is_some_and(|x| x == account.as_str())
        });
    }
    if files.is_empty() {
        bail!("no matching existing AuctionDB.lua files");
    }
    let token = String::from_utf8(protect(&fs::read(dir.join("credential.dpapi"))?, true)?)?;
    guard()?;
    let response = client()?
        .get(format!(
            "{}/v1/collector/history",
            config.server.trim_end_matches('/')
        ))
        .query(&[("realm", &options.realm), ("house", &options.house)])
        .bearer_auth(token)
        .send()?;
    if matches!(response.status().as_u16(), 401 | 403) {
        db.execute(
            "INSERT OR REPLACE INTO settings VALUES('paused','unauthorized')",
            [],
        )?;
        bail!("device credential refused; pair again");
    }
    let response = response.error_for_status()?;
    let mut bytes = Vec::new();
    response.take(MAX_FILE + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_FILE {
        bail!("history export exceeds 32 MiB");
    }
    let bundle: market_domain::HistoryExport = serde_json::from_slice(&bytes)?;
    let mut failures = 0;
    for path in files {
        let result = (|| -> Result<()> {
            guard()?;
            safe_path(&root, &path)?;
            let source = hash(path.strip_prefix(&root)?.to_string_lossy().as_bytes());
            let original = stable_read(&path)?;
            let modified = fs::metadata(&path)?.modified()?;
            let pending: Option<(String, String)> = db
                .query_row(
                    "SELECT new_hash,backup FROM sync_journal WHERE source=?",
                    [&source],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .optional()?;
            if let Some((new_hash, backup)) = pending {
                if !Path::new(&backup).is_file() {
                    bail!("recovery backup is missing; manual review required");
                }
                if !options.dry_run && hash(&original) == new_hash {
                    db.execute("INSERT INTO sources VALUES(?,?) ON CONFLICT(source) DO UPDATE SET checksum=excluded.checksum", params![source,new_hash])?;
                }
                if !options.dry_run {
                    db.execute("DELETE FROM sync_journal WHERE source=?", [&source])?;
                }
            }
            let merged =
                ahdb_format::merge_history(&original, &bundle, &options.realm, &options.house)?;
            if merged.len() as u64 > MAX_FILE {
                bail!("merged source exceeds 32 MiB limit");
            }
            if options.dry_run {
                println!("dry-run source={source} changed={}", original != merged);
                return Ok(());
            }
            enqueue(db, &source, &original)?;
            if merged == original {
                return Ok(());
            }
            guard()?;
            safe_path(&root, &path)?;
            let backup = dir.join(format!("sync-{}-{}.bak", now(), Uuid::new_v4()));
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&backup)?;
            file.write_all(&original)?;
            file.sync_all()?;
            let temp = path.with_file_name(format!(".AuctionDB-{}.tmp", Uuid::new_v4()));
            let write_result = (|| -> Result<()> {
                guard()?;
                safe_path(&root, path.parent().unwrap())?;
                let mut file = fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&temp)?;
                file.write_all(&merged)?;
                file.sync_all()?;
                drop(file);
                db.execute(
                    "INSERT OR REPLACE INTO sync_journal VALUES(?,?,?)",
                    params![source, hash(&merged), backup.to_string_lossy()],
                )?;
                guarded_replace(&root, &path, &temp, &original, modified, guard)?;

                let tx = db.transaction()?;
                tx.execute("INSERT INTO sources VALUES(?,?) ON CONFLICT(source) DO UPDATE SET checksum=excluded.checksum",params![source,hash(&merged)])?;
                tx.execute("DELETE FROM sync_journal WHERE source=?", [&source])?;
                tx.commit()?;
                Ok(())
            })();
            if temp.exists() {
                let _ = fs::remove_file(&temp);
            }
            write_result?;
            println!("synced source={source} backup={}", backup.display());
            Ok(())
        })();
        if let Err(error) = result {
            failures += 1;
            eprintln!(
                "sync failed for source={}: {error:#}",
                hash(path.strip_prefix(&root)?.to_string_lossy().as_bytes())
            );
        }
    }
    if failures != 0 {
        bail!("{failures} account sync(s) failed; game was not started; successful accounts remain synced");
    }
    if let Some(game) = game {
        if !options.dry_run {
            guard()?;
            safe_path(&root, &game)?;
            std::process::Command::new(&game)
                .current_dir(game.parent().unwrap())
                .spawn()
                .context("cannot start selected WoW executable")?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn release_commands_refuse_writes_before_accessing_game_or_network() -> Result<()> {
        let mut db = Connection::open_in_memory()?;
        for executable in [None, Some(PathBuf::from("missing/WoW.exe"))] {
            let error = run(
                Path::new("missing"),
                &mut db,
                Options {
                    realm: "Realm".into(),
                    house: "Alliance".into(),
                    dry_run: false,
                    account: None,
                },
                executable,
            )
            .unwrap_err();
            assert!(error
                .to_string()
                .contains("shared-history writes are disabled"));
        }
        Ok(())
    }
    #[test]
    fn strict_source_path_rejects_escape() -> Result<()> {
        let root =
            tempfile::tempdir_in(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target"))?;
        let outside =
            tempfile::tempdir_in(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target"))?;
        assert!(safe_path(root.path(), outside.path()).is_err());
        assert!(safe_path(root.path(), root.path()).is_ok());
        Ok(())
    }
    #[test]
    fn exclusive_sync_lock_releases_after_exit() -> Result<()> {
        let dir = tempfile::tempdir_in(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target"))?;
        let path = dir.path().join("lock.sqlite");
        let mut a = Connection::open(&path)?;
        let mut b = Connection::open(&path)?;
        b.busy_timeout(Duration::from_millis(1))?;
        let tx = a.transaction_with_behavior(rusqlite::TransactionBehavior::Exclusive)?;
        assert!(b
            .transaction_with_behavior(rusqlite::TransactionBehavior::Exclusive)
            .is_err());
        drop(tx);
        assert!(b
            .transaction_with_behavior(rusqlite::TransactionBehavior::Exclusive)
            .is_ok());
        Ok(())
    }
    #[cfg(windows)]
    #[test]
    fn atomic_replace_preserves_source_on_failure_and_consumes_temp() -> Result<()> {
        let dir = tempfile::tempdir_in(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target"))?;
        let source = dir.path().join("AuctionDB.lua");
        let temp = dir.path().join("temp");
        fs::write(&source, b"original")?;
        assert!(replace(&source, &temp).is_err());
        assert_eq!(fs::read(&source)?, b"original");
        fs::write(&temp, b"merged")?;
        replace(&source, &temp)?;
        assert_eq!(fs::read(&source)?, b"merged");
        assert!(!temp.exists());
        Ok(())
    }
    #[test]
    fn process_guard_and_changed_source_preserve_original() -> Result<()> {
        let dir = tempfile::tempdir_in(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target"))?;
        let source = dir.path().join("AuctionDB.lua");
        let temp = dir.path().join("temp");
        fs::write(&source, b"original")?;
        fs::write(&temp, b"merged")?;
        let modified = fs::metadata(&source)?.modified()?;
        assert!(
            guarded_replace(dir.path(), &source, &temp, b"original", modified, || bail!(
                "fake running game"
            ))
            .is_err()
        );
        assert_eq!(fs::read(&source)?, b"original");
        fs::write(&source, b"game rewrite")?;
        assert!(
            guarded_replace(dir.path(), &source, &temp, b"original", modified, || Ok(())).is_err()
        );
        assert_eq!(fs::read(&source)?, b"game rewrite");
        assert_eq!(fs::read(&temp)?, b"merged");
        Ok(())
    }
}

#[cfg(all(test, windows))]
mod integration {
    use super::*;
    const ORIGINAL: &[u8] = br#"Unknown={flag=true}
AuctionDBForeverSaved={settings={recordScans=true},houses={Alliance={items={},daily={}}}}"#;
    fn options(dry_run: bool) -> Options {
        Options {
            realm: "Realm".into(),
            house: "Alliance".into(),
            dry_run,
            account: None,
        }
    }
    fn serve(realm: &str) -> (String, std::thread::JoinHandle<String>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let body = serde_json::json!({"schema_version":1,"market":{"realm":realm,"house":"Alliance"},"generated_at":17280000,"points":[{"item_id":"1","observed_at":17280000,"min_unit_copper":"99","median_unit_copper":"99","quantity":"3"}],"daily":[]}).to_string();
        let handle = thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut request = Vec::new();
            let mut buf = [0; 1024];
            while !request.windows(4).any(|x| x == b"\r\n\r\n") {
                let n = socket.read(&mut buf).unwrap();
                if n == 0 {
                    break;
                }
                request.extend_from_slice(&buf[..n]);
            }
            write!(socket,"HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{}",body.len(),body).unwrap();
            String::from_utf8(request).unwrap()
        });
        (url, handle)
    }
    fn configure(dir: &Path, root: &Path, url: String) -> Result<()> {
        fs::write(
            dir.join("config.json"),
            serde_json::to_vec(&Config {
                root: root.to_path_buf(),
                server: url,
                device_id: "test".into(),
                allow_localhost_http: true,
            })?,
        )?;
        fs::write(
            dir.join("credential.dpapi"),
            protect(b"fixture-token", false)?,
        )?;
        Ok(())
    }
    #[test]
    fn network_merge_backup_queue_repeat_dry_run_and_invalid_market() -> Result<()> {
        let fixture =
            tempfile::tempdir_in(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target"))?;
        let root = fixture.path().join("game");
        let dir = fixture.path().join("private");
        let source = root.join("WTF/Account/A/SavedVariables/AuctionDB.lua");
        fs::create_dir_all(source.parent().unwrap())?;
        fs::create_dir_all(&dir)?;
        fs::write(&source, ORIGINAL)?;
        let mut db = database(&dir)?;
        let (url, server) = serve("Realm");
        configure(&dir, &root, url)?;
        run_with_guard(&dir, &mut db, options(true), None, &|| Ok(()))?;
        let request = server.join().unwrap();
        assert!(request.starts_with("GET /v1/collector/history?realm=Realm&house=Alliance "));
        assert!(request
            .to_ascii_lowercase()
            .contains("authorization: bearer fixture-token"));
        assert_eq!(fs::read(&source)?, ORIGINAL);
        assert_eq!(
            db.query_row("SELECT COUNT(*) FROM queue", [], |r| r.get::<_, i64>(0))?,
            0
        );
        assert_eq!(
            fs::read_dir(&dir)?
                .filter_map(Result::ok)
                .filter(|x| x.path().extension().is_some_and(|x| x == "bak"))
                .count(),
            0
        );
        let (url, server) = serve("Realm");
        configure(&dir, &root, url)?;
        run_with_guard(&dir, &mut db, options(false), None, &|| Ok(()))?;
        server.join().unwrap();
        let merged = fs::read(&source)?;
        assert_ne!(merged, ORIGINAL);
        assert!(String::from_utf8_lossy(&merged).contains("nauctionsSync"));
        let backups = fs::read_dir(&dir)?
            .filter_map(Result::ok)
            .filter(|x| x.path().extension().is_some_and(|x| x == "bak"))
            .collect::<Vec<_>>();
        assert_eq!(backups.len(), 1);
        assert_eq!(fs::read(backups[0].path())?, ORIGINAL);
        assert_eq!(
            db.query_row("SELECT payload FROM queue", [], |r| r.get::<_, Vec<u8>>(0))?,
            ORIGINAL
        );
        assert_eq!(
            db.query_row("SELECT checksum FROM sources", [], |r| r
                .get::<_, String>(0))?,
            hash(&merged)
        );
        assert_eq!(
            db.query_row("SELECT COUNT(*) FROM sync_journal", [], |r| r
                .get::<_, i64>(0))?,
            0
        );
        let (url, server) = serve("Realm");
        configure(&dir, &root, url)?;
        run_with_guard(&dir, &mut db, options(false), None, &|| Ok(()))?;
        server.join().unwrap();
        assert_eq!(fs::read(&source)?, merged);
        assert_eq!(
            db.query_row("SELECT COUNT(*) FROM queue", [], |r| r.get::<_, i64>(0))?,
            1
        );
        let (url, server) = serve("Wrong Realm");
        configure(&dir, &root, url)?;
        assert!(run_with_guard(&dir, &mut db, options(false), None, &|| Ok(())).is_err());
        server.join().unwrap();
        assert_eq!(fs::read(&source)?, merged);
        // Fail exactly at the last process check, after durable backup and temp writes.
        fs::write(&source, ORIGINAL)?;
        let (url, server) = serve("Realm");
        configure(&dir, &root, url)?;
        let count = std::cell::Cell::new(0);
        let guard = || {
            count.set(count.get() + 1);
            if count.get() >= 6 {
                bail!("fake game started before replace")
            }
            Ok(())
        };
        assert!(run_with_guard(&dir, &mut db, options(false), None, &guard).is_err());
        server.join().unwrap();
        assert_eq!(fs::read(&source)?, ORIGINAL);
        assert!(!fs::read_dir(source.parent().unwrap())?
            .filter_map(Result::ok)
            .any(|x| x.path().extension().is_some_and(|x| x == "tmp")));
        Ok(())
    }
}
