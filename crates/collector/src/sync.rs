use super::*;

#[derive(clap::Args)]
pub struct Options {
    #[arg(long)]
    realm: String,
    #[arg(long)]
    house: String,
    #[arg(long)]
    dry_run: bool,
    /// Select exactly one account directory. Never defaults to all accounts.
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

fn adapter(root: &Path) -> Result<()> {
    // The first writable adapter is the exact bundled fork, not merely a version label.
    let expected: &[(&str, &[u8])] = &[
        (
            "AuctionDB.toc",
            include_bytes!("../../../addon/AuctionDB/AuctionDB.toc"),
        ),
        (
            "AuctionDB.lua",
            include_bytes!("../../../addon/AuctionDB/AuctionDB.lua"),
        ),
        (
            "Bug.lua",
            include_bytes!("../../../addon/AuctionDB/Bug.lua"),
        ),
        (
            "Chat.lua",
            include_bytes!("../../../addon/AuctionDB/Chat.lua"),
        ),
        (
            "Comm.lua",
            include_bytes!("../../../addon/AuctionDB/Comm.lua"),
        ),
        (
            "Graph.lua",
            include_bytes!("../../../addon/AuctionDB/Graph.lua"),
        ),
        (
            "History.lua",
            include_bytes!("../../../addon/AuctionDB/History.lua"),
        ),
        (
            "Live.lua",
            include_bytes!("../../../addon/AuctionDB/Live.lua"),
        ),
        (
            "Minimap.lua",
            include_bytes!("../../../addon/AuctionDB/Minimap.lua"),
        ),
        (
            "NauctionsAuctionRecipes.lua",
            include_bytes!("../../../addon/AuctionDB/NauctionsAuctionRecipes.lua"),
        ),
        (
            "NauctionsProfessions.lua",
            include_bytes!("../../../addon/AuctionDB/NauctionsProfessions.lua"),
        ),
        (
            "Options.lua",
            include_bytes!("../../../addon/AuctionDB/Options.lua"),
        ),
        (
            "Record.lua",
            include_bytes!("../../../addon/AuctionDB/Record.lua"),
        ),
        (
            "Scan.lua",
            include_bytes!("../../../addon/AuctionDB/Scan.lua"),
        ),
        (
            "Sell.lua",
            include_bytes!("../../../addon/AuctionDB/Sell.lua"),
        ),
        (
            "Tooltip.lua",
            include_bytes!("../../../addon/AuctionDB/Tooltip.lua"),
        ),
        (
            "Vendor.lua",
            include_bytes!("../../../addon/AuctionDB/Vendor.lua"),
        ),
        (
            "NauctionsForecasts.lua",
            include_bytes!("../../../addon/AuctionDB/NauctionsForecasts.lua"),
        ),
    ];
    for (name, bytes) in expected {
        let path = root.join("Interface/AddOns/AuctionDB").join(name);
        safe_path(root, &path).context("installed addon is missing or unsafe")?;
        if hash(&bounded_read(&path)?) != hash(bytes) {
            bail!("installed addon fingerprint is unsupported: {name}");
        }
    }
    Ok(())
}
fn settle(path: &Path, guard: &dyn Fn() -> Result<()>) -> Result<Vec<u8>> {
    let start = std::time::Instant::now();
    let mut stable_since = start;
    let mut previous = stable_read(path)?;
    loop {
        guard()?;
        if start.elapsed() > Duration::from_secs(60) {
            bail!("source did not settle while the game was closed");
        }
        let current = stable_read(path)?;
        if current != previous {
            stable_since = std::time::Instant::now();
            previous = current;
        }
        let required = if cfg!(test) {
            Duration::ZERO
        } else {
            Duration::from_secs(5)
        };
        if stable_since.elapsed() >= required {
            return Ok(previous);
        }
    }
}
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
    sync_safety::legacy_pending(db)?;
    let account = options
        .account
        .as_deref()
        .filter(|s| !s.is_empty() && !s.contains(['/', '\\', ':']) && *s != "." && *s != "..")
        .context("select exactly one account with --account; run discover to list local paths")?;
    let (config, credential) = crate::load_pairing(db, dir)?;
    validate_server(&config.server, config.allow_localhost_http)?;
    let root = config.root.canonicalize()?;
    safe_path(&config.root, &config.root)?;
    if dir.canonicalize()?.starts_with(&root) || is_link(dir) {
        bail!("use a private collector data directory outside WoW");
    }
    let coordinator = sync_safety::Coordinator::open(&config.root, !options.dry_run)?;
    if let Some(c) = &coordinator {
        if options.dry_run {
            if c.pending()? {
                bail!("unfinished local operation; run recover --root before preview");
            }
        } else {
            c.recover(db, guard)?;
        }
    }
    let path = root
        .join("WTF/Account")
        .join(account)
        .join("SavedVariables/AuctionDB.lua");
    safe_path(&root, &path)?;
    adapter(&root)?;
    let original = settle(&path, guard)?;
    let source = hash(path.strip_prefix(&root)?.to_string_lossy().as_bytes());
    let game = exe
        .map(|p| -> Result<PathBuf> {
            safe_path(&root, &p)?;
            let name = p
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            if !matches!(
                name.as_str(),
                "wow.exe" | "wowclassic.exe" | "wow-64.exe" | "wowclassic_t.exe"
            ) {
                bail!("select a supported WoW executable");
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
    let snapshot_started = std::time::Instant::now();
    let snapshot_clock = now();
    let token = String::from_utf8(protect(&credential, true)?)?;
    guard()?;
    let response = client()?
        .get(format!(
            "{}/v1/collector/history",
            config.server.trim_end_matches('/')
        ))
        .query(&[
            ("realm", options.realm.as_str()),
            ("house", options.house.as_str()),
            ("protocol", "2"),
        ])
        .bearer_auth(token)
        .send()?;
    if matches!(response.status().as_u16(), 401 | 403) {
        db.execute(
            "INSERT OR REPLACE INTO settings VALUES('paused','unauthorized')",
            [],
        )?;
        bail!("device credential refused; pair again");
    }
    let mut bytes = Vec::new();
    response
        .error_for_status()?
        .take(MAX_FILE + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_FILE {
        bail!("history export exceeds 32 MiB");
    }
    let snapshot: market_domain::HistorySnapshot = serde_json::from_slice(&bytes)?;
    if !snapshot.verify(now()) {
        bail!("invalid, stale, expired, or incomplete history snapshot; source retained");
    }
    let bundle = &snapshot.history;
    validate_export_time(bundle.generated_at, now())?;
    let merged = ahdb_format::merge_history(&original, bundle, &options.realm, &options.house)?;
    if merged.len() as u64 > MAX_FILE {
        bail!("merged source exceeds 32 MiB");
    }
    println!(
        "account={account} path={} realm={} house={} changed={}",
        path.display(),
        options.realm,
        options.house,
        original != merged
    );
    if options.dry_run {
        return Ok(());
    }
    let c = coordinator.context("installation coordinator is required")?;
    if original == merged {
        return Ok(());
    }
    guard()?;
    let op = c.prepare(dir, &path, &original, &merged, "sync")?;
    // One explicitly selected account: all preparation completes before the source changes.
    enqueue(db, &source, &original)?;
    adapter(&root)?;
    validate_export_time(bundle.generated_at, now())?;
    if (now() - snapshot_clock - snapshot_started.elapsed().as_secs() as i64).abs() > 5
        || !snapshot.verify(now())
    {
        bail!("snapshot expired or local clock changed; prepared files retained");
    }
    c.commit(&op, db, guard)?;
    println!(
        "operation={} outcome=committed original_sha256={} output_sha256={}",
        op.id, op.original_hash, op.output_hash
    );
    if let Some(game) = game {
        if c.pending()? {
            bail!("unfinished recovery blocks game launch");
        }
        guard()?;
        safe_path(&root, &game)?;
        std::process::Command::new(&game)
            .current_dir(game.parent().unwrap())
            .spawn()
            .context("cannot start selected WoW executable")?;
    }
    Ok(())
}
fn validate_export_time(generated: i64, current: i64) -> Result<()> {
    if generated <= 0 || generated.abs_diff(current) > 300 {
        bail!(
            "history snapshot is stale or clocks differ by more than five minutes; source retained"
        );
    }
    Ok(())
}

pub fn local(
    dir: &Path,
    db: &mut Connection,
    root: &Path,
    action: &str,
    id: Option<Uuid>,
    expected: Option<&str>,
) -> Result<()> {
    sync_safety::legacy_pending(db)?;
    let Some(c) = sync_safety::Coordinator::open(root, false)? else {
        println!("{{\"operations\":[]}}");
        return Ok(());
    };
    match action {
        "recovery-status" => {}
        "recover" => {
            closed()?;
            c.recover(db, &closed)?;
        }
        "restore" => {
            closed()?;
            let id = id.context("select a recorded operation")?;
            let expected = expected.context(
                "preview recovery-status and supply --expected-current-sha256 (or missing)",
            )?;
            c.restore(dir, db, id, expected, &closed)?;
        }
        _ => bail!("unknown local recovery action"),
    }
    println!("{}", c.status()?);
    Ok(())
}

// Test-only wrappers retain direct Win32 and last-moment source-guard checks.
#[cfg(test)]
fn replace(source: &Path, temp: &Path) -> Result<()> {
    sync_safety::replace_with_backup(
        source,
        temp,
        &source.with_file_name(format!(".test-{}.backup", Uuid::new_v4())),
    )
}
#[cfg(test)]
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
        bail!("source changed before replacement");
    }
    replace(path, temp)
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
    use std::io::Write;
    const ORIGINAL: &[u8] = br#"Unknown={flag=true}
AuctionDBForeverSaved={settings={recordScans=true},houses={Alliance={items={},daily={},scans={}}}}"#;
    fn options(dry_run: bool) -> Options {
        Options {
            realm: "Realm".into(),
            house: "Alliance".into(),
            dry_run,
            account: Some("A".into()),
        }
    }
    fn serve(realm: &str) -> (String, std::thread::JoinHandle<String>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let bundle:market_domain::HistoryExport = serde_json::from_value(serde_json::json!({"schema_version":1,"market":{"realm":realm,"house":"Alliance"},"generated_at":now(),"points":[{"item_id":"1","observed_at":now().div_euclid(3600)*3600,"min_unit_copper":"99","median_unit_copper":"99","quantity":"3"}],"daily":[]})).unwrap();
        let body = serde_json::to_string(&market_domain::HistorySnapshot::new(bundle)).unwrap();
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
        let addon = root.join("Interface/AddOns/AuctionDB");
        fs::create_dir_all(&addon)?;
        for name in [
            "AuctionDB.toc",
            "AuctionDB.lua",
            "Bug.lua",
            "Chat.lua",
            "Comm.lua",
            "Graph.lua",
            "History.lua",
            "Live.lua",
            "Minimap.lua",
            "NauctionsAuctionRecipes.lua",
            "NauctionsProfessions.lua",
            "NauctionsForecasts.lua",
            "Options.lua",
            "Record.lua",
            "Scan.lua",
            "Sell.lua",
            "Tooltip.lua",
            "Vendor.lua",
        ] {
            fs::copy(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../addon/AuctionDB")
                    .join(name),
                addon.join(name),
            )?;
        }
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
        assert!(
            request.starts_with("GET /v1/collector/history?realm=Realm&house=Alliance&protocol=2 ")
        );
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
        assert_eq!(db.query_row("SELECT 0", [], |r| r.get::<_, i64>(0))?, 0);
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
