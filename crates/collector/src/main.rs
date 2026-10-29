mod maintenance;
mod sync;
mod sync_safety;
use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use reqwest::blocking::Client;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use uuid::Uuid;

#[derive(Parser)]
#[command(
    about = "AuctionDB collector with explicit optional sync. Raw files can contain character identifiers."
)]
struct Cli {
    #[arg(long, default_value = "collector-data")]
    data_dir: PathBuf,
    #[command(subcommand)]
    command: Command,
}
#[derive(Subcommand)]
enum Command {
    Discover {
        #[arg(long)]
        root: PathBuf,
    },
    Pair {
        #[arg(long)]
        root: PathBuf,
        #[arg(long)]
        server: String,
        #[arg(long)]
        code: String,
        #[arg(long, default_value = "Windows collector")]
        display_name: String,
        #[arg(long)]
        allow_localhost_http: bool,
    },
    Run {
        #[arg(long)]
        once: bool,
        /// Read saved game files only while WoW is active and during final-save grace periods.
        #[arg(long)]
        game_aware: bool,
        #[arg(long, requires = "game_aware")]
        game_exe: Option<PathBuf>,
    },
    Sync {
        #[command(flatten)]
        options: sync::Options,
    },
    SyncAndStart {
        #[command(flatten)]
        options: sync::Options,
        #[arg(long)]
        game_exe: PathBuf,
    },
    /// Inspect local recovery records without a server connection or a game-file write.
    RecoveryStatus {
        #[arg(long)]
        root: PathBuf,
    },
    /// Recover interrupted operations while every WoW client is closed. Works offline.
    Recover {
        #[arg(long)]
        root: PathBuf,
    },
    /// Restore a recorded original after checking the exact current-file hash from preview.
    Restore {
        #[arg(long)]
        root: PathBuf,
        #[arg(long)]
        operation: Uuid,
        #[arg(long)]
        expected_current_sha256: String,
    },
    Status,
    /// List retained rejected payloads for review. No payload is changed.
    Rejections {
        #[arg(long, default_value_t = 50)]
        limit: u32,
    },
    /// Discard one reviewed rejected payload. Pending uploads cannot be removed.
    DiscardRejected {
        #[arg(long)]
        upload_id: Uuid,
        #[arg(long)]
        sha256: String,
        #[arg(long)]
        confirm: bool,
    },
    Retry,
    Heartbeat,
}
#[derive(Clone, Serialize, Deserialize)]
struct Config {
    root: PathBuf,
    server: String,
    device_id: String,
    allow_localhost_http: bool,
}
#[derive(Deserialize)]
struct Enrollment {
    device_id: String,
    token: String,
}
#[derive(Deserialize)]
struct Receipt {
    upload_id: String,
    status: String,
    error_code: Option<String>,
}
const MAX_FILE: u64 = 32 * 1024 * 1024;
const QUOTA: i64 = 256 * 1024 * 1024;
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}
fn database(dir: &Path) -> Result<Connection> {
    fs::create_dir_all(dir)?;
    let mut db = Connection::open(dir.join("spool.sqlite"))?;
    db.busy_timeout(Duration::from_secs(5))?;
    db.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL;
        CREATE TABLE IF NOT EXISTS sources(source TEXT PRIMARY KEY, checksum TEXT NOT NULL);
        CREATE TABLE IF NOT EXISTS queue(id TEXT PRIMARY KEY, source TEXT NOT NULL, checksum TEXT NOT NULL,
        payload BLOB, state TEXT NOT NULL DEFAULT 'queued', error TEXT, attempts INTEGER NOT NULL DEFAULT 0,
        next_at INTEGER NOT NULL DEFAULT 0);
        CREATE TABLE IF NOT EXISTS settings(key TEXT PRIMARY KEY, value TEXT NOT NULL);")?;
    db.execute_batch("CREATE TABLE IF NOT EXISTS source_health(source TEXT PRIMARY KEY, modified_at INTEGER, staged_at INTEGER, accepted_at INTEGER, acquisition_error TEXT);")?;
    maintenance::migrate(&mut db, now())?;
    Ok(db)
}
fn discover(root: &Path) -> Result<Vec<PathBuf>> {
    let accounts = root.join("WTF").join("Account");
    let mut files = Vec::new();
    if !accounts.exists() {
        return Ok(files);
    }
    if is_link(&root.join("WTF")) || is_link(&accounts) {
        bail!("source directory must not be a link or junction");
    }
    for entry in fs::read_dir(accounts)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() || is_link(&entry.path()) {
            continue;
        }
        let sv = entry.path().join("SavedVariables");
        if is_link(&sv) {
            continue;
        }
        let path = sv.join("AuctionDB.lua");
        if let Ok(meta) = fs::symlink_metadata(&path) {
            if meta.is_file()
                && !is_link(&path)
                && path.canonicalize()?.starts_with(root.canonicalize()?)
            {
                files.push(path);
            }
        }
    }
    files.sort();
    Ok(files)
}
fn is_link(path: &Path) -> bool {
    let Ok(meta) = fs::symlink_metadata(path) else {
        return false;
    };
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        meta.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        meta.file_type().is_symlink()
    }
}
fn bounded_read(path: &Path) -> Result<Vec<u8>> {
    let file = fs::File::open(path)?;
    let mut bytes = Vec::new();
    file.take(MAX_FILE + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_FILE {
        bail!("source exceeds 32 MiB limit");
    }
    Ok(bytes)
}
fn stable_read(path: &Path) -> Result<Vec<u8>> {
    let before = fs::metadata(path)?;
    let first = bounded_read(path)?;
    thread::sleep(Duration::from_millis(250));
    let second = bounded_read(path)?;
    let after = fs::metadata(path)?;
    if before.len() != after.len()
        || before.modified()? != after.modified()?
        || hash(&first) != hash(&second)
    {
        bail!("source changed during read; retry on next reconciliation");
    }
    Ok(second)
}
fn enqueue(db: &mut Connection, source: &str, bytes: &[u8]) -> Result<bool> {
    let checksum = hash(bytes);
    let tx = db.transaction()?;
    let previous: Option<String> = tx
        .query_row(
            "SELECT checksum FROM sources WHERE source=?",
            [source],
            |r| r.get(0),
        )
        .optional()?;
    if previous.as_deref() == Some(&checksum) {
        return Ok(false);
    }
    let used: i64 = tx.query_row(
        "SELECT COALESCE(SUM(length(payload)),0) FROM queue",
        [],
        |r| r.get(0),
    )?;
    if used + bytes.len() as i64 > QUOTA {
        bail!(
            "spool quota reached; pending bytes retained; clear terminal rejections after review"
        );
    }
    tx.execute(
        "INSERT INTO queue(id,source,checksum,payload,created_at) VALUES(?,?,?,?,?)",
        params![Uuid::new_v4().to_string(), source, checksum, bytes, now()],
    )?;
    tx.execute("INSERT INTO sources VALUES(?,?) ON CONFLICT(source) DO UPDATE SET checksum=excluded.checksum", params![source,checksum])?;
    tx.execute("INSERT INTO source_health(source,staged_at) VALUES(?,?) ON CONFLICT(source) DO UPDATE SET staged_at=excluded.staged_at",params![source,now()])?;
    tx.commit()?;
    Ok(true)
}
use rusqlite::OptionalExtension;
fn validate_server(server: &str, local: bool) -> Result<()> {
    let url = reqwest::Url::parse(server)?;
    if !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        bail!("server URL must not contain credentials, query, or fragment");
    }
    if url.scheme() != "https"
        && !(local
            && url.scheme() == "http"
            && matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]")))
    {
        bail!("HTTPS required; HTTP is allowed only for explicit localhost development");
    }
    Ok(())
}
fn client() -> Result<Client> {
    Ok(Client::builder()
        .timeout(Duration::from_secs(5))
        .redirect(reqwest::redirect::Policy::none())
        .build()?)
}
fn transport(db: &Connection, config: &Config, token: &str) -> Result<()> {
    if db
        .query_row("SELECT value FROM settings WHERE key='paused'", [], |r| {
            r.get::<_, String>(0)
        })
        .optional()?
        .is_some()
    {
        bail!("authentication paused; pair again to resume");
    }
    let http = client()?;
    let mut stmt = db.prepare("SELECT id,source,checksum,payload,state,attempts FROM queue WHERE state IN ('queued','received') AND next_at<=? ORDER BY rowid LIMIT 1")?;
    let jobs = stmt
        .query_map([now()], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, Vec<u8>>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, u32>(5)?,
            ))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    for (id, source, checksum, bytes, state, attempts) in jobs {
        let base = config.server.trim_end_matches('/');
        let response = if state == "received" {
            http.get(format!("{base}/v1/uploads/{id}"))
                .bearer_auth(token)
                .send()
        } else {
            http.post(format!("{base}/v1/uploads"))
                .bearer_auth(token)
                .header("Content-Type", "application/octet-stream")
                .header("X-Upload-ID", &id)
                .header("X-Content-SHA256", checksum)
                .header("X-Source-File-ID", &source)
                .body(bytes)
                .send()
        };
        let delay = (2_i64.pow(attempts.min(10)) + (Uuid::new_v4().as_u128() % 7) as i64).min(1800);
        db.execute(
            "UPDATE queue SET attempts=attempts+1,next_at=? WHERE id=?",
            params![now() + delay, id],
        )?;
        match response {
            Ok(response) if matches!(response.status().as_u16(), 401 | 403) => {
                db.execute(
                    "INSERT OR REPLACE INTO settings VALUES('paused','unauthorized')",
                    [],
                )?;
                bail!("device credential refused; pair again");
            }
            Ok(response) if response.status().is_success() => {
                let receipt: Receipt = response
                    .json()
                    .context("invalid receipt; queued payload retained")?;
                if receipt.upload_id != id {
                    bail!("receipt ID mismatch; queued payload retained");
                }
                match receipt.status.as_str() {
                    "accepted" => {
                        market_telemetry::record("upload_accepted", serde_json::json!({}));
                        db.execute(
                            "UPDATE queue SET state='accepted',payload=NULL,error=NULL,finished_at=? WHERE id=?",
                            params![now(),id],
                        )?;
                        db.execute("INSERT INTO source_health(source,accepted_at) VALUES(?,?) ON CONFLICT(source) DO UPDATE SET accepted_at=excluded.accepted_at",params![source,now()])?;
                    }
                    "received" => {
                        db.execute(
                            "UPDATE queue SET state='received',error=NULL WHERE id=?",
                            [&id],
                        )?;
                    }
                    "rejected" => {
                        market_telemetry::error("upload_rejected", serde_json::json!({}));
                        db.execute(
                            "UPDATE queue SET state='rejected',error=?,finished_at=? WHERE id=?",
                            params![
                                receipt.error_code.unwrap_or_else(|| "rejected".into()),
                                now(),
                                id
                            ],
                        )?;
                    }
                    _ => bail!("unknown receipt state; queued payload retained"),
                }
            }
            Ok(response) if matches!(response.status().as_u16(), 400 | 409 | 413 | 415 | 422) => {
                db.execute(
                    "UPDATE queue SET state='rejected',error=?,finished_at=? WHERE id=?",
                    params![format!("HTTP {}", response.status().as_u16()), now(), id],
                )?;
            }
            Ok(response) => {
                db.execute(
                    "UPDATE queue SET error=? WHERE id=?",
                    params![format!("HTTP {}", response.status().as_u16()), id],
                )?;
            }
            Err(_) => {
                db.execute(
                    "UPDATE queue SET error='network unavailable' WHERE id=?",
                    [id],
                )?;
            }
        }
    }
    Ok(())
}
fn health_snapshot(db: &Connection) -> Result<serde_json::Value> {
    let queue_count: i64 = db.query_row(
        "SELECT COUNT(*) FROM queue WHERE state IN ('queued','received')",
        [],
        |r| r.get(0),
    )?;
    let modified: Option<i64> =
        db.query_row("SELECT MAX(modified_at) FROM source_health", [], |r| {
            r.get(0)
        })?;
    let age = modified.map(|value| (now() - value).max(0));
    Ok(
        serde_json::json!({"authentication_paused":db.query_row("SELECT EXISTS(SELECT 1 FROM settings WHERE key='paused')", [], |r| r.get::<_,bool>(0))?,"rejected_count":db.query_row("SELECT COUNT(*) FROM queue WHERE state='rejected'", [], |r| r.get::<_,i64>(0))?,"acquisition_errors":db.query_row("SELECT COUNT(*) FROM source_health WHERE acquisition_error IS NOT NULL", [], |r| r.get::<_,i64>(0))?,"oldest_pending_seconds":db.query_row("SELECT ? - MIN(created_at) FROM queue WHERE state IN ('queued','received')", [now()], |r| r.get::<_,Option<i64>>(0))?,"collector_version":env!("CARGO_PKG_VERSION"),"queue_count":queue_count,"source_file_age_seconds":age,"scan_age_seconds":null}),
    )
}
fn heartbeat(db: &Connection, config: &Config, token: &str) -> Result<()> {
    if db
        .query_row("SELECT value FROM settings WHERE key='paused'", [], |r| {
            r.get::<_, String>(0)
        })
        .optional()?
        .is_some()
    {
        bail!("authentication paused; pair again");
    }
    let response = client()?
        .post(format!(
            "{}/v1/collector/health",
            config.server.trim_end_matches('/')
        ))
        .bearer_auth(token)
        .json(&health_snapshot(db)?)
        .send()?;
    if matches!(response.status().as_u16(), 401 | 403) {
        db.execute(
            "INSERT OR REPLACE INTO settings VALUES('paused','unauthorized')",
            [],
        )?;
        bail!("device credential refused; pair again");
    }
    response.error_for_status()?;
    println!("Collector health sent. Source modification age does not prove a fresh game scan.");
    Ok(())
}
#[cfg(windows)]
fn protect(bytes: &[u8], decrypt: bool) -> Result<Vec<u8>> {
    use windows_sys::Win32::{
        Foundation::LocalFree,
        Security::Cryptography::{
            CryptProtectData, CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
        },
    };
    let input = CRYPT_INTEGER_BLOB {
        cbData: bytes.len() as u32,
        pbData: bytes.as_ptr() as *mut u8,
    };
    let mut output = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: std::ptr::null_mut(),
    };
    unsafe {
        let ok = if decrypt {
            CryptUnprotectData(
                &input,
                std::ptr::null_mut(),
                std::ptr::null(),
                std::ptr::null_mut(),
                std::ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        } else {
            CryptProtectData(
                &input,
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null_mut(),
                std::ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        };
        if ok == 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        let result = std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec();
        LocalFree(output.pbData as *mut _);
        Ok(result)
    }
}
#[cfg(not(windows))]
fn protect(_: &[u8], _: bool) -> Result<Vec<u8>> {
    bail!("device credentials require Windows DPAPI")
}
fn acquire(db: &mut Connection, root: &Path) -> Result<bool> {
    // A configured coordinator serializes capture with replacement across all data dirs.
    // A normal read-only collector never creates this installation directory.
    let coordinator = if root.is_dir() {
        match sync_safety::Coordinator::open(root, false) {
            Ok(c) => c,
            Err(_) => {
                eprintln!("Source acquisition deferred: installation is busy or recovery coordination is unavailable. Pending uploads can continue.");
                return Ok(true);
            }
        }
    } else {
        None
    };
    let paths = discover(root);
    let source_unavailable = !root.is_dir() || paths.is_err();
    let paths = paths.unwrap_or_default();
    for path in paths {
        let relative = path.strip_prefix(root)?;
        let source = hash(relative.to_string_lossy().as_bytes());
        let modified = fs::metadata(&path)
            .ok()
            .and_then(|meta| meta.modified().ok())
            .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64);
        db.execute("INSERT INTO source_health(source,modified_at) VALUES(?,?) ON CONFLICT(source) DO UPDATE SET modified_at=excluded.modified_at",params![source,modified])?;
        let result = stable_read(&path).and_then(|bytes| {
            if let Some(c) = &coordinator {
                if c.capture(db, relative, &bytes)? {
                    return Ok(false);
                }
            }
            enqueue(db, &source, &bytes)
        });
        let acquisition_error = result
            .as_ref()
            .err()
            .map(|_| "unstable_unreadable_oversized_or_quota_full");
        db.execute(
            "UPDATE source_health SET acquisition_error=? WHERE source=?",
            params![acquisition_error, source],
        )?;
        if result.is_err() {
            eprintln!(
                "Source acquisition deferred (unstable, unreadable, oversized, or quota full)."
            );
            market_telemetry::error("source_acquisition_deferred", serde_json::json!({}));
        }
    }
    Ok(source_unavailable)
}
fn save_pairing(db: &mut Connection, config: &Config, credential: &[u8]) -> Result<()> {
    let tx = db.transaction()?;
    // New device credentials cannot poll old device receipts. New upload IDs avoid
    // device-scoped idempotency conflicts; canonical scans are deduplicated by the server.
    let ids = {
        let mut stmt = tx.prepare(
            "SELECT id FROM queue WHERE state IN ('queued','received') AND payload IS NOT NULL",
        )?;
        let rows = stmt
            .query_map([], |r| r.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows
    };
    for id in ids {
        tx.execute(
            "UPDATE queue SET id=?,state='queued',attempts=0,next_at=0,error=NULL WHERE id=?",
            params![Uuid::new_v4().to_string(), id],
        )?;
    }
    tx.execute(
        "INSERT OR REPLACE INTO settings VALUES('paired_config',?)",
        [serde_json::to_string(config)?],
    )?;
    tx.execute(
        "INSERT OR REPLACE INTO settings VALUES('paired_credential',?)",
        [credential],
    )?;
    tx.execute("DELETE FROM settings WHERE key='paused'", [])?;
    tx.commit()?;
    Ok(())
}
fn load_pairing(db: &Connection, data: &Path) -> Result<(Config, Vec<u8>)> {
    let config: Option<String> = db
        .query_row(
            "SELECT value FROM settings WHERE key='paired_config'",
            [],
            |r| r.get(0),
        )
        .optional()?;
    if let Some(config) = config {
        return Ok((
            serde_json::from_str(&config)?,
            db.query_row(
                "SELECT value FROM settings WHERE key='paired_credential'",
                [],
                |r| r.get(0),
            )?,
        ));
    }
    Ok((
        serde_json::from_slice(&fs::read(data.join("config.json"))?)?,
        fs::read(data.join("credential.dpapi"))?,
    ))
}
fn configured_root(data: &Path) -> Result<Option<PathBuf>> {
    if data.join("spool.sqlite").exists() {
        let db = Connection::open_with_flags(
            data.join("spool.sqlite"),
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )?;
        let config: Option<String> = db
            .query_row(
                "SELECT value FROM settings WHERE key='paired_config'",
                [],
                |r| r.get(0),
            )
            .optional()?;
        if let Some(config) = config {
            return Ok(Some(serde_json::from_str::<Config>(&config)?.root));
        }
    }
    let path = data.join("config.json");
    if !path.exists() {
        return Ok(None);
    }
    Ok(Some(
        serde_json::from_slice::<Config>(&fs::read(path)?)?.root,
    ))
}
fn main() -> Result<()> {
    let cli = Cli::parse();
    let result = run(cli);
    if result.is_err() {
        market_telemetry::error("collector_command_failed", serde_json::json!({}));
    }
    market_telemetry::flush(Duration::from_secs(2));
    result
}
fn run(cli: Cli) -> Result<()> {
    // Validate the destination before creating a SQLite file or a directory.
    let selected_root = match &cli.command {
        Command::Pair { root, .. }
        | Command::Discover { root }
        | Command::RecoveryStatus { root }
        | Command::Recover { root }
        | Command::Restore { root, .. } => Some(root.canonicalize()?),
        _ => configured_root(&cli.data_dir)?.map(|root| root.canonicalize().unwrap_or(root)),
    };
    if let Some(root) = selected_root {
        let destination = if cli.data_dir.exists() {
            cli.data_dir.canonicalize()?
        } else {
            let absolute = std::path::absolute(&cli.data_dir)?;
            let mut ancestor = absolute.as_path();
            while !ancestor.exists() {
                ancestor = ancestor.parent().context("invalid data directory")?;
            }
            ancestor
                .canonicalize()?
                .join(absolute.strip_prefix(ancestor)?)
        };
        if destination.starts_with(root) {
            bail!("collector data must be outside the WoW installation");
        }
    }
    fs::create_dir_all(&cli.data_dir)?;
    let run_lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(cli.data_dir.join("collector.lock"))?;
    run_lock
        .try_lock()
        .context("another collector command owns this data directory")?;
    let mut db = database(&cli.data_dir)?;
    market_telemetry::init(
        "collector",
        Some(cli.data_dir.join("collector-diagnostics.jsonl")),
    );
    match cli.command {
        Command::Discover { root } => {
            for path in discover(&root)? {
                println!("{}", path.display());
            }
        }
        Command::Pair {
            root,
            server,
            code,
            display_name,
            allow_localhost_http,
        } => {
            let root = root
                .canonicalize()
                .context("select an existing WoW installation root")?;
            if cli.data_dir.canonicalize()?.starts_with(&root) {
                bail!("collector data must be outside the WoW installation");
            }
            validate_server(&server, allow_localhost_http)?;
            let http = client()?;
            http.get(format!(
                "{}/v1/collector/capabilities",
                server.trim_end_matches('/')
            ))
            .send()?
            .error_for_status()?;
            let enrollment: Enrollment = http
                .post(format!(
                    "{}/v1/devices/enroll",
                    server.trim_end_matches('/')
                ))
                .json(&serde_json::json!({"code":code,"display_name":display_name}))
                .send()?
                .error_for_status()?
                .json()?;
            let config = Config {
                root,
                server,
                device_id: enrollment.device_id,
                allow_localhost_http,
            };
            let credential = protect(enrollment.token.as_bytes(), false)?;
            save_pairing(&mut db, &config, &credential)?;
            // Compatibility copies for existing desktop setup detection. The transaction is authoritative.
            let _ = fs::write(cli.data_dir.join("credential.dpapi"), &credential);
            let _ = fs::write(
                cli.data_dir.join("config.json"),
                serde_json::to_vec_pretty(&config)?,
            );
            println!("Device paired. Run the collector to start reconciliation.");
        }
        Command::Sync { options } => sync::run(&cli.data_dir, &mut db, options, None)?,
        Command::SyncAndStart { options, game_exe } => {
            sync::run(&cli.data_dir, &mut db, options, Some(game_exe))?
        }
        Command::RecoveryStatus { root } => {
            sync::local(&cli.data_dir, &mut db, &root, "recovery-status", None, None)?
        }
        Command::Recover { root } => {
            sync::local(&cli.data_dir, &mut db, &root, "recover", None, None)?
        }
        Command::Restore {
            root,
            operation,
            expected_current_sha256,
        } => sync::local(
            &cli.data_dir,
            &mut db,
            &root,
            "restore",
            Some(operation),
            Some(&expected_current_sha256),
        )?,
        Command::Rejections { limit } => {
            for rejection in maintenance::list_rejections(&db, limit)? {
                println!("{}", serde_json::to_string(&rejection)?);
            }
        }
        Command::DiscardRejected {
            upload_id,
            sha256,
            confirm,
        } => {
            maintenance::discard(&mut db, &upload_id.to_string(), &sha256, confirm, now())?;
            println!("Reviewed rejected payload discarded. Pending uploads and source deduplication were kept.");
        }
        Command::Status => {
            println!(
                "collector_version={} timestamps=unix_utc_seconds",
                env!("CARGO_PKG_VERSION")
            );
            let source_count: i64 =
                db.query_row("SELECT COUNT(*) FROM sources", [], |r| r.get(0))?;
            let pending_bytes: i64 = db.query_row(
                "SELECT COALESCE(SUM(length(payload)),0) FROM queue",
                [],
                |r| r.get(0),
            )?;
            println!("sources={source_count} spool_bytes={pending_bytes} quota_bytes={QUOTA}");
            let mut health=db.prepare("SELECT source,modified_at,staged_at,accepted_at,acquisition_error FROM source_health ORDER BY source")?;
            for row in health.query_map([],|r|Ok(format!("source={} modified_at={:?} staged_at={:?} accepted_at={:?} acquisition_error={:?}",r.get::<_,String>(0)?,r.get::<_,Option<i64>>(1)?,r.get::<_,Option<i64>>(2)?,r.get::<_,Option<i64>>(3)?,r.get::<_,Option<String>>(4)?)))? {println!("{}",row?);}
            let mut stmt = db.prepare("SELECT id,state,length(payload),attempts,COALESCE(error,''),next_at FROM queue ORDER BY rowid DESC LIMIT 50")?;
            for row in stmt.query_map([], |r| {
                Ok(format!(
                    "{} {} bytes={} attempts={} error={} next_retry_at={}",
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, Option<i64>>(2)?.unwrap_or(0),
                    r.get::<_, i64>(3)?,
                    r.get::<_, String>(4)?,
                    r.get::<_, i64>(5)?
                ))
            })? {
                println!("{}", row?);
            }
            if db
                .query_row("SELECT value FROM settings WHERE key='paused'", [], |r| {
                    r.get::<_, String>(0)
                })
                .optional()?
                .is_some()
            {
                println!("Authentication paused. Pair again.");
            }
        }
        Command::Retry => {
            db.execute(
                "UPDATE queue SET next_at=0 WHERE state IN ('queued','received')",
                [],
            )?;
            println!("Pending uploads scheduled. Rejections require server review.");
        }
        Command::Heartbeat => {
            let (config, credential) = load_pairing(&db, &cli.data_dir)?;
            validate_server(&config.server, config.allow_localhost_http)?;
            let token = String::from_utf8(protect(&credential, true)?)?;
            heartbeat(&db, &config, &token)?;
        }
        Command::Run {
            once,
            game_aware,
            game_exe,
        } => {
            let (config, credential) = load_pairing(&db, &cli.data_dir)?;
            validate_server(&config.server, config.allow_localhost_http)?;
            if cli.data_dir.canonicalize()?.starts_with(
                config
                    .root
                    .canonicalize()
                    .unwrap_or_else(|_| config.root.clone()),
            ) {
                bail!("collector data must be outside the WoW installation");
            }
            let token = String::from_utf8(protect(&credential, true)?)?;
            if let Some(exe) = &game_exe {
                if !exe.canonicalize()?.starts_with(
                    config
                        .root
                        .canonicalize()
                        .unwrap_or_else(|_| config.root.clone()),
                ) {
                    bail!("game executable must be inside the selected installation");
                }
            }
            let stop_transport = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
            let transport_thread = if !once {
                let data = cli.data_dir.clone();
                let config = config.clone();
                let token = token.clone();
                let stop = stop_transport.clone();
                Some(thread::spawn(move || -> Result<()> {
                    let db = database(&data)?;
                    let mut health_at = std::time::Instant::now() - Duration::from_secs(300);
                    while !stop.load(std::sync::atomic::Ordering::Relaxed) {
                        if let Err(error) = transport(&db, &config, &token) {
                            eprintln!("{error}");
                        }
                        if health_at.elapsed() >= Duration::from_secs(300) {
                            let _ = heartbeat(&db, &config, &token);
                            health_at = std::time::Instant::now();
                        }
                        thread::sleep(Duration::from_secs(1));
                    }
                    Ok(())
                }))
            } else {
                None
            };
            let monitoring_started = std::time::Instant::now();
            if maintenance::prune(&db, now()).is_err() {
                market_telemetry::error("receipt_cleanup_failed", serde_json::json!({}));
                eprintln!("Receipt cleanup failed. Pending uploads were kept.");
            }
            let mut last_maintenance = std::time::Instant::now();
            let mut gate = market_game_process::CaptureGate::new(0);
            loop {
                if last_maintenance.elapsed() >= Duration::from_secs(3600) {
                    if maintenance::prune(&db, now()).is_err() {
                        market_telemetry::error("receipt_cleanup_failed", serde_json::json!({}));
                        eprintln!("Receipt cleanup failed. Pending uploads were kept.");
                    }
                    last_maintenance = std::time::Instant::now();
                }
                let mode = if game_aware {
                    let game = market_game_process::detect(&config.root, game_exe.as_deref());
                    let mode = gate.update(game, monitoring_started.elapsed().as_secs());
                    fs::write(
                        cli.data_dir.join("runtime.json"),
                        serde_json::to_vec(
                            &serde_json::json!({"game_state":game,"capture_mode":mode,"observed_at":now()}),
                        )?,
                    )?;
                    mode
                } else {
                    market_game_process::CaptureMode::Playing
                };
                // Reconcile saved files in standby too: a full session can occur between samples.
                let source_unavailable = acquire(&mut db, &config.root)?;
                if once {
                    if let Err(error) = transport(&db, &config, &token) {
                        market_telemetry::error(
                            "collector_transport_failed",
                            serde_json::json!({}),
                        );
                        eprintln!("{error}");
                    }
                }
                let mut health = health_snapshot(&db)?;
                health["source_unavailable"] = serde_json::json!(source_unavailable);
                fs::write(
                    cli.data_dir.join("runtime.json"),
                    serde_json::to_vec(
                        &serde_json::json!({"capture_mode":mode,"observed_at":now(),"health":health}),
                    )?,
                )?;
                if once {
                    break;
                }
                thread::sleep(Duration::from_secs(10));
            }
            stop_transport.store(true, std::sync::atomic::Ordering::Relaxed);
            if let Some(worker) = transport_thread {
                let _ = worker.join();
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn received_repair_commits_identity_and_queue_then_accepts() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let mut db = database(dir.path())?;
        enqueue(&mut db, "source", b"immutable")?;
        let (server, receipt) = mock_server(202, "received");
        let mut config = Config {
            root: dir.path().join("missing-game"),
            server,
            device_id: "old".into(),
            allow_localhost_http: true,
        };
        transport(&db, &config, "old-token")?;
        receipt.join().unwrap();
        let old: String = db.query_row("SELECT id FROM queue", [], |r| r.get(0))?;
        let (server, accepted) = mock_server(200, "accepted");
        config.server = server;
        config.device_id = "new".into();
        save_pairing(&mut db, &config, b"new-protected-token")?;
        drop(db);
        let db = database(dir.path())?;
        let (saved, credential) = load_pairing(&db, dir.path())?;
        assert_eq!(saved.device_id, "new");
        assert_eq!(credential, b"new-protected-token");
        let (id, state): (String, String) =
            db.query_row("SELECT id,state FROM queue", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })?;
        assert_ne!(id, old);
        assert_eq!(state, "queued");
        transport(&db, &saved, "new-token")?;
        let request = accepted.join().unwrap();
        assert!(request.starts_with("POST /v1/uploads "));
        assert!(request.ends_with("immutable"));
        assert!(request
            .to_ascii_lowercase()
            .contains("authorization: bearer new-token"));
        assert_eq!(
            db.query_row("SELECT state FROM queue", [], |r| r.get::<_, String>(0))?,
            "accepted"
        );
        Ok(())
    }
    #[test]
    fn spool_commands_survive_missing_game_root_and_lock_is_exclusive() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let config = Config {
            root: dir.path().join("disconnected"),
            server: "https://example.test".into(),
            device_id: "d".into(),
            allow_localhost_http: false,
        };
        fs::write(dir.path().join("config.json"), serde_json::to_vec(&config)?)?;
        run(Cli {
            data_dir: dir.path().to_owned(),
            command: Command::Status,
        })?;
        run(Cli {
            data_dir: dir.path().to_owned(),
            command: Command::Retry,
        })?;
        run(Cli {
            data_dir: dir.path().to_owned(),
            command: Command::Rejections { limit: 5 },
        })?;
        let lock = fs::OpenOptions::new()
            .write(true)
            .open(dir.path().join("collector.lock"))?;
        lock.try_lock()?;
        assert!(run(Cli {
            data_dir: dir.path().to_owned(),
            command: Command::Retry
        })
        .is_err());
        drop(lock);
        run(Cli {
            data_dir: dir.path().to_owned(),
            command: Command::Retry,
        })?;
        Ok(())
    }
    #[test]
    fn acquisition_reconciles_a_missed_session_while_transport_is_stalled() -> Result<()> {
        use std::net::TcpListener;
        let dir = tempfile::tempdir()?;
        let data = dir.path().join("data");
        let root = dir.path().join("game");
        let mut db = database(&data)?;
        enqueue(&mut db, "old", b"pending")?;
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let config = Config {
            root: root.clone(),
            server: format!("http://{}", listener.local_addr()?),
            device_id: "d".into(),
            allow_localhost_http: true,
        };
        let (started, wait) = std::sync::mpsc::channel();
        let stall = thread::spawn(move || {
            let (_socket, _) = listener.accept().unwrap();
            started.send(()).unwrap();
            thread::sleep(Duration::from_secs(2));
        });
        let transport = thread::spawn(move || {
            let db = database(&data).unwrap();
            transport(&db, &config, "token").unwrap();
        });
        wait.recv_timeout(Duration::from_secs(3))?;
        // Synthetic session begins, saves, and ends while the upload is waiting.
        let sv = root.join("WTF/Account/Synthetic/SavedVariables");
        fs::create_dir_all(&sv)?;
        fs::write(sv.join("AuctionDB.lua"), b"saved = {}")?;
        let at = std::time::Instant::now();
        assert!(!acquire(&mut db, &root)?);
        assert!(at.elapsed() < Duration::from_secs(1));
        assert_eq!(
            db.query_row("SELECT count(*) FROM queue", [], |r| r.get::<_, i64>(0))?,
            2
        );
        stall.join().unwrap();
        transport.join().unwrap();
        Ok(())
    }
    #[test]
    fn file_age_is_not_reported_as_observed_scan_age() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let db = database(dir.path())?;
        db.execute(
            "INSERT INTO source_health(source,modified_at) VALUES('s',?)",
            [now() - 60],
        )?;
        let health = health_snapshot(&db)?;
        assert!(health["scan_age_seconds"].is_null());
        assert!(health["source_file_age_seconds"].as_i64().unwrap() >= 60);
        Ok(())
    }
    #[test]
    fn restart_preserves_immutable_bytes_and_deduplication() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let mut db = database(dir.path())?;
        assert!(enqueue(&mut db, "account", b"first")?);
        drop(db);
        let mut db = database(dir.path())?;
        assert!(!enqueue(&mut db, "account", b"first")?);
        assert!(enqueue(&mut db, "account", b"second")?);
        let bytes: Vec<u8> = db.query_row(
            "SELECT payload FROM queue ORDER BY rowid LIMIT 1",
            [],
            |r| r.get(0),
        )?;
        assert_eq!(bytes, b"first");
        Ok(())
    }
    #[test]
    fn discovery_and_reads_do_not_write_source() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let sv = dir.path().join("WTF/Account/A/SavedVariables");
        fs::create_dir_all(&sv)?;
        fs::write(sv.join("AuctionDB.lua"), b"saved = {}")?;
        fs::write(sv.join("Other.lua"), b"unrelated")?;
        let files = discover(dir.path())?;
        assert_eq!(files.len(), 1);
        let modified = fs::metadata(&files[0])?.modified()?;
        assert_eq!(stable_read(&files[0])?, b"saved = {}");
        assert_eq!(modified, fs::metadata(&files[0])?.modified()?);
        Ok(())
    }
    #[test]
    fn quota_failure_does_not_advance_source_hash() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let mut db = database(dir.path())?;
        db.execute(
            "INSERT INTO queue(id,source,checksum,payload) VALUES('large','x','x',zeroblob(?))",
            [QUOTA],
        )?;
        assert!(enqueue(&mut db, "a", b"x").is_err());
        assert_eq!(
            db.query_row("SELECT COUNT(*) FROM sources", [], |r| r.get::<_, i64>(0))?,
            0
        );
        Ok(())
    }
    #[test]
    fn transport_requires_https_or_explicit_localhost() {
        assert!(validate_server("http://example.com", true).is_err());
        assert!(validate_server("http://localhost:8080", false).is_err());
        assert!(validate_server("http://localhost:8080", true).is_ok());
    }
    #[test]
    fn offline_attempt_retains_uuid_and_bytes_across_restart() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let mut db = database(dir.path())?;
        enqueue(&mut db, "source", b"offline payload")?;
        let original: String = db.query_row("SELECT id FROM queue", [], |r| r.get(0))?;
        let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
        let server = format!("http://{}", listener.local_addr()?);
        drop(listener);
        let config = Config {
            root: dir.path().to_path_buf(),
            server,
            device_id: "d".into(),
            allow_localhost_http: true,
        };
        transport(&db, &config, "secret")?;
        drop(db);
        let db = database(dir.path())?;
        let (id, bytes, error): (String, Vec<u8>, String) =
            db.query_row("SELECT id,payload,error FROM queue", [], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })?;
        assert_eq!(original, id);
        assert_eq!(bytes, b"offline payload");
        assert_eq!(error, "network unavailable");
        Ok(())
    }
    #[test]
    fn overlapping_write_is_deferred() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("AuctionDB.lua");
        fs::write(&path, b"before")?;
        let writer_path = path.clone();
        let writer = thread::spawn(move || {
            thread::sleep(Duration::from_millis(80));
            fs::write(writer_path, b"after rewrite").unwrap();
        });
        assert!(stable_read(&path).is_err());
        writer.join().unwrap();
        assert_eq!(stable_read(&path)?, b"after rewrite");
        Ok(())
    }
    #[cfg(windows)]
    #[test]
    fn dpapi_round_trip() -> Result<()> {
        let encrypted = protect(b"device-test-token", false)?;
        assert_ne!(encrypted, b"device-test-token");
        assert_eq!(protect(&encrypted, true)?, b"device-test-token");
        Ok(())
    }
    fn mock_server(
        status: u16,
        receipt_state: &'static str,
    ) -> (String, std::thread::JoinHandle<String>) {
        use std::io::{Read, Write};
        use std::net::TcpListener;
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let mut raw = Vec::new();
            let mut chunk = [0; 4096];
            loop {
                let n = stream.read(&mut chunk).unwrap();
                if n == 0 {
                    break;
                }
                raw.extend_from_slice(&chunk[..n]);
                let text = String::from_utf8_lossy(&raw);
                if let Some(end) = text.find("\r\n\r\n") {
                    let len = text[..end]
                        .lines()
                        .find_map(|l| {
                            l.to_lowercase()
                                .strip_prefix("content-length:")
                                .and_then(|x| x.trim().parse::<usize>().ok())
                        })
                        .unwrap_or(0);
                    if raw.len() >= end + 4 + len {
                        break;
                    }
                }
            }
            let request = String::from_utf8_lossy(&raw).into_owned();
            let id = request
                .lines()
                .find_map(|l| {
                    l.to_lowercase()
                        .strip_prefix("x-upload-id:")
                        .map(|x| x.trim().to_string())
                })
                .unwrap_or_else(|| {
                    request
                        .lines()
                        .next()
                        .unwrap()
                        .split_whitespace()
                        .nth(1)
                        .unwrap()
                        .split('/')
                        .next_back()
                        .unwrap()
                        .to_string()
                });
            let body=serde_json::json!({"upload_id":id,"status":receipt_state,"error_code":"unsupported_format"}).to_string();
            write!(stream,"HTTP/1.1 {status} OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).unwrap();
            request
        });
        (url, handle)
    }
    #[test]
    fn received_receipt_keeps_payload_until_poll_accepts() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let mut db = database(dir.path())?;
        enqueue(&mut db, "source", b"immutable")?;
        let (server, handle) = mock_server(202, "received");
        let mut config = Config {
            root: dir.path().to_path_buf(),
            server,
            device_id: "d".into(),
            allow_localhost_http: true,
        };
        transport(&db, &config, "secret")?;
        let request = handle.join().unwrap();
        assert!(request.starts_with("POST /v1/uploads "));
        assert!(request.ends_with("immutable"));
        assert_eq!(
            db.query_row("SELECT length(payload) FROM queue", [], |r| r
                .get::<_, i64>(0))?,
            9
        );
        db.execute("UPDATE queue SET next_at=0", [])?;
        let (server, handle) = mock_server(200, "accepted");
        config.server = server;
        transport(&db, &config, "secret")?;
        assert!(handle.join().unwrap().starts_with("GET /v1/uploads/"));
        assert!(db.query_row("SELECT payload IS NULL FROM queue", [], |r| r
            .get::<_, bool>(0))?);
        Ok(())
    }
    #[test]
    fn rejection_retains_bytes_and_unauthorized_pauses() -> Result<()> {
        for (status, state) in [
            (200, "rejected"),
            (401, "received"),
            (403, "received"),
            (422, "rejected"),
        ] {
            let dir = tempfile::tempdir()?;
            let mut db = database(dir.path())?;
            enqueue(&mut db, "source", b"immutable")?;
            let (server, handle) = mock_server(status, state);
            let config = Config {
                root: dir.path().to_path_buf(),
                server,
                device_id: "d".into(),
                allow_localhost_http: true,
            };
            let result = transport(&db, &config, "secret");
            handle.join().unwrap();
            assert_eq!(
                db.query_row("SELECT length(payload) FROM queue", [], |r| r
                    .get::<_, i64>(0))?,
                9
            );
            if matches!(status, 401 | 403) {
                assert!(result.is_err());
                assert!(transport(&db, &config, "secret").is_err());
            } else {
                result?;
                assert_eq!(
                    db.query_row("SELECT state FROM queue", [], |r| r.get::<_, String>(0))?,
                    "rejected"
                );
                if status == 422 {
                    assert_eq!(
                        db.query_row("SELECT error FROM queue", [], |r| r.get::<_, String>(0))?,
                        "HTTP 422"
                    );
                }
            }
        }
        Ok(())
    }
    #[test]
    fn heartbeat_revocation_pauses_future_requests() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let db = database(dir.path())?;
        let (server, handle) = mock_server(403, "rejected");
        let config = Config {
            root: dir.path().to_path_buf(),
            server,
            device_id: "d".into(),
            allow_localhost_http: true,
        };
        assert!(heartbeat(&db, &config, "secret").is_err());
        assert!(handle
            .join()
            .unwrap()
            .starts_with("POST /v1/collector/health"));
        assert!(heartbeat(&db, &config, "secret").is_err());
        assert!(transport(&db, &config, "secret").is_err());
        Ok(())
    }
}
