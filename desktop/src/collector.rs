use crate::{Config, Desktop};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
};
use tauri::Manager;

#[derive(Default)]
pub struct Collector {
    child: Option<Child>,
    #[cfg(windows)]
    job: Option<crate::collector_job::Job>,
    last_exit: Option<String>,
    logs: Option<crate::collector_log::Capture>,
}
impl Collector {
    pub fn stop(&mut self) -> Result<(), String> {
        if let Some(child) = self.child.as_mut() {
            if child
                .try_wait()
                .map_err(|_| "Cannot check collector process.")?
                .is_none()
            {
                child.kill().map_err(|_| "Cannot stop the collector.")?;
            }
            child
                .wait()
                .map_err(|_| "Cannot wait for the collector to stop.")?;
            self.child = None;
            #[cfg(windows)]
            {
                self.job = None;
            }
            self.finish_logs();
        }
        Ok(())
    }
    fn running(&mut self) -> Result<bool, String> {
        if let Some(child) = self.child.as_mut() {
            if let Some(exit) = child
                .try_wait()
                .map_err(|_| "Cannot inspect collector process.")?
            {
                self.last_exit = Some(format!(
                    "Collector exited ({exit}). Check status or pair again."
                ));
                market_telemetry::error(
                    "collector_exited",
                    serde_json::json!({"status":exit.code()}),
                );
                self.child = None;
                #[cfg(windows)]
                {
                    self.job = None;
                }
                self.finish_logs();
            }
        }
        Ok(self.child.is_some())
    }
    fn start(&mut self, state: &Desktop, config: &Config) -> Result<(), String> {
        if self.running()? {
            return Ok(());
        }
        if !paired(state, config) {
            return Err("Pair this collector for the selected server and WoW folder first.".into());
        }
        let data = data_dir(state, config)?;
        let log_path = data.join("collector.log");
        let log =
            crate::collector_log::Log::open(&log_path).map_err(|_| "Cannot open collector log.")?;
        let mut command = command(state, &data)?;
        command
            .arg("run")
            .arg("--game-aware")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(exe) = &config.game_exe {
            command.arg("--game-exe").arg(exe);
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000 | 0x00000004); // CREATE_NO_WINDOW | CREATE_SUSPENDED
        }
        let mut child = command
            .spawn()
            .map_err(|_| "Cannot start the bundled collector.")?;
        #[cfg(windows)]
        {
            match crate::collector_job::Job::attach(&child) {
                Ok(job) => self.job = Some(job),
                Err(_) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err("Cannot own the collector process lifetime.".into());
                }
            }
        }
        match crate::collector_log::Capture::attach(&mut child, log) {
            Ok(logs) => self.logs = Some(logs),
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(error);
            }
        }
        self.child = Some(child);
        self.last_exit = None;
        Ok(())
    }

    fn finish_logs(&mut self) {
        if let Some(error) = self.logs.take().and_then(|logs| logs.finish()) {
            self.last_exit = Some(error);
        }
    }
}
pub(crate) fn data_dir(state: &Desktop, config: &Config) -> Result<PathBuf, String> {
    let server = config.server.as_ref().ok_or("Connect to a server first.")?;
    let id = format!("{:x}", Sha256::digest(server.as_bytes()));
    let dir = state.data.join("collectors").join(id);
    fs::create_dir_all(&dir).map_err(|_| "Cannot create collector storage.")?;
    Ok(dir)
}
fn paired(state: &Desktop, config: &Config) -> bool {
    let Ok(dir) = data_dir(state, config) else {
        return false;
    };
    // Pairing and pending-upload migration commit together in SQLite. Read that
    // snapshot first so interrupted compatibility-file writes cannot undo pairing.
    if let Ok(db) = rusqlite::Connection::open_with_flags(
        dir.join("spool.sqlite"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    ) {
        use rusqlite::OptionalExtension;
        let snapshot: Result<Option<String>, _> = db
            .query_row(
                "SELECT value FROM settings WHERE key='paired_config'",
                [],
                |r| r.get(0),
            )
            .optional();
        if let Ok(Some(snapshot)) = snapshot {
            let stored = serde_json::from_str::<serde_json::Value>(&snapshot).ok();
            return stored.is_some_and(|c| {
                c["server"].as_str() == config.server.as_deref()
                    && c["root"].as_str().map(Path::new) == config.game_root.as_deref()
            }) && db
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM settings WHERE key='paired_credential')",
                    [],
                    |r| r.get::<_, bool>(0),
                )
                .unwrap_or(false);
        }
    }
    let stored = fs::read(dir.join("config.json"))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok());
    stored.is_some_and(|c| {
        c["server"].as_str() == config.server.as_deref()
            && c["root"].as_str().map(Path::new) == config.game_root.as_deref()
    }) && dir.join("credential.dpapi").is_file()
}
fn command(state: &Desktop, data: &Path) -> Result<Command, String> {
    let mut binary = state.binary.clone();
    if cfg!(debug_assertions) && !binary.is_file() {
        binary = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/market-collector.exe");
    }
    if !binary.is_file() {
        return Err("The bundled collector is missing. Reinstall Nauctions.".into());
    }
    let mut command = Command::new(binary);
    if state.data.join("telemetry-disabled").exists() {
        command.env("NAUCTIONS_TELEMETRY", "0");
    }
    command.arg("--data-dir").arg(data).stdin(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    Ok(command)
}
#[derive(Serialize)]
pub struct Status {
    running: bool,
    paired: bool,
    last_exit: Option<String>,
    game_state: market_game_process::GameState,
    capture_mode: Option<String>,
    health: Option<serde_json::Value>,
    startup_enabled: Option<bool>,
    startup_error: Option<String>,
}
#[tauri::command]
pub fn collector_state(state: tauri::State<'_, Desktop>) -> Result<Status, String> {
    let mut runner = state
        .collector
        .try_lock()
        .map_err(|_| "Collector is busy.")?;
    let conn = state
        .connection
        .lock()
        .map_err(|_| "Desktop settings are unavailable.")?;
    let running = runner.running()?;
    let mode = if running {
        data_dir(&state, &conn.config)
            .ok()
            .and_then(|d| fs::read(d.join("runtime.json")).ok())
            .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok())
            .and_then(|v| {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .ok()?
                    .as_secs();
                let observed = v["observed_at"].as_u64()?;
                if now.saturating_sub(observed) > 60 {
                    return None;
                }
                v["capture_mode"].as_str().map(str::to_owned)
            })
    } else {
        None
    };
    Ok(Status {
        running,
        paired: paired(&state, &conn.config),
        last_exit: runner
            .last_exit
            .clone()
            .or_else(|| runner.logs.as_ref().and_then(|logs| logs.error())),
        game_state: conn
            .config
            .game_root
            .as_deref()
            .map(|root| market_game_process::detect(root, conn.config.game_exe.as_deref()))
            .unwrap_or(market_game_process::GameState::Unknown),
        capture_mode: mode,
        health: if running {
            data_dir(&state, &conn.config)
                .ok()
                .and_then(|d| fs::read(d.join("runtime.json")).ok())
                .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok())
                .and_then(|v| {
                    let recent = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .ok()?
                        .as_secs()
                        .saturating_sub(v["observed_at"].as_u64()?)
                        <= 60;
                    recent.then(|| v["health"].clone())
                })
        } else {
            None
        },
        startup_enabled: crate::startup::is_enabled().ok(),
        startup_error: state
            .startup_error
            .lock()
            .map_err(|_| "Desktop settings are unavailable.")?
            .clone(),
    })
}
#[tauri::command]
pub async fn collector_action(
    action: String,
    code: Option<String>,
    realm: Option<String>,
    house: Option<String>,
    account: Option<String>,
    restore: Option<RestorePreview>,
    app: tauri::AppHandle,
) -> Result<String, String> {
    if matches!(action.as_str(), "sync" | "sync_and_start") {
        return Err("Shared-history writes are disabled pending recovery and game compatibility validation. Use Preview sync.".into());
    }
    tauri::async_runtime::spawn_blocking(move || {
        let (operation, expected_current_sha256) = restore
            .map(|p| (Some(p.operation), Some(p.expected_current_sha256)))
            .unwrap_or_default();
        let state = app.state::<Desktop>();
        let mut runner = state.collector.lock().map_err(|_| "Collector is busy.")?;
        if crate::updates::installation_active(&app) {
            return Err("An app update is being installed. Wait for Nauctions to restart.".into());
        }
        let config = state
            .connection
            .lock()
            .map_err(|_| "Desktop settings are unavailable.")?
            .config
            .clone();
        if action == "stop" {
            runner.stop()?;
            set_automatic(&state, false)?;
            return Ok("Collection stopped. Queued uploads are retained.".into());
        }
        if action == "start" {
            runner.start(&state, &config)?;
            set_automatic(&state, true)?;
            return Ok(
                "Collection started. Close the window to keep it running in the tray.".into(),
            );
        }
        if !matches!(
            action.as_str(),
            "pair"
                | "status"
                | "retry"
                | "sync_preview"
                | "sync"
                | "sync_and_start"
                | "accounts"
                | "recovery-status"
                | "recover"
                | "restore"
        ) {
            return Err("Unknown collector action.".into());
        }
        let local = matches!(
            action.as_str(),
            "accounts" | "recovery-status" | "recover" | "restore"
        );
        if action != "pair" && !local && !paired(&state, &config) {
            return Err("Pair the collector first.".into());
        }
        let data = if local && config.server.is_none() {
            let path = state.data.join("local-recovery");
            fs::create_dir_all(&path).map_err(|_| "Cannot create local recovery storage.")?;
            path
        } else {
            data_dir(&state, &config)?
        };
        let mut cmd = command(&state, &data)?;
        match action.as_str() {
            "accounts" | "recovery-status" | "recover" | "restore" => {
                cmd.arg(if action == "accounts" {
                    "discover"
                } else {
                    &action
                })
                .arg("--root")
                .arg(
                    config
                        .game_root
                        .as_ref()
                        .ok_or("Select the WoW folder first.")?,
                );
                if action == "restore" {
                    let operation = operation
                        .filter(|v| {
                            v.len() == 36 && v.bytes().all(|b| b.is_ascii_hexdigit() || b == b'-')
                        })
                        .ok_or("Select a recorded operation.")?;
                    let expected = expected_current_sha256
                        .filter(|v| {
                            v == "missing"
                                || (v.len() == 64 && v.bytes().all(|b| b.is_ascii_hexdigit()))
                        })
                        .ok_or("Refresh the recovery preview first.")?;
                    cmd.arg("--operation")
                        .arg(operation)
                        .arg("--expected-current-sha256")
                        .arg(expected);
                }
            }
            "pair" => {
                let code = code.ok_or("Create a pairing code first.")?;
                if code.len() != 64 || !code.bytes().all(|b| b.is_ascii_hexdigit()) {
                    return Err("Invalid pairing code.".into());
                }
                let server = config.server.as_ref().ok_or("Connect to a server first.")?;
                cmd.arg("pair")
                    .arg("--root")
                    .arg(
                        config
                            .game_root
                            .as_ref()
                            .ok_or("Select the WoW folder first.")?,
                    )
                    .arg("--server")
                    .arg(server)
                    .arg("--code")
                    .arg(code)
                    .arg("--display-name")
                    .arg("Nauctions desktop");
                if server.starts_with("http://") {
                    cmd.arg("--allow-localhost-http");
                }
            }
            "sync" | "sync_preview" | "sync_and_start" => {
                let realm = realm
                    .filter(|r| {
                        !r.trim().is_empty() && r.len() <= 100 && !r.chars().any(char::is_control)
                    })
                    .ok_or("Select a market first.")?;
                let house = house
                    .filter(|h| matches!(h.as_str(), "Alliance" | "Horde" | "Neutral"))
                    .ok_or("Select a supported auction house.")?;
                cmd.arg(if action == "sync_and_start" {
                    "sync-and-start"
                } else {
                    "sync"
                })
                .arg("--realm")
                .arg(realm)
                .arg("--house")
                .arg(house);
                let account = account
                    .filter(|v| {
                        !v.is_empty()
                            && v.len() <= 200
                            && !v.contains(['/', '\\', ':'])
                            && v != "."
                            && v != ".."
                    })
                    .ok_or("Select one local account first.")?;
                cmd.arg("--account").arg(account);
                if action == "sync_preview" {
                    cmd.arg("--dry-run");
                }
                if action == "sync_and_start" {
                    cmd.arg("--game-exe").arg(
                        config
                            .game_exe
                            .as_ref()
                            .ok_or("Select the WoW executable first.")?,
                    );
                }
            }
            _ => {
                cmd.arg(&action);
            }
        }
        let resume = runner.running()?;
        runner.stop()?;
        let _activity = matches!(
            action.as_str(),
            "sync" | "sync_preview" | "sync_and_start" | "recover" | "restore"
        )
        .then(|| crate::tray_activity::begin(&app));
        let output = cmd
            .output()
            .map_err(|_| "Cannot run the bundled collector.")?;
        let result = if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout)
                .chars()
                .take(16000)
                .collect::<String>())
        } else {
            Err(format!(
                "Collector action failed: {}",
                String::from_utf8_lossy(&output.stderr)
                    .chars()
                    .take(4000)
                    .collect::<String>()
            ))
        };
        if action == "pair" && result.is_ok() {
            set_automatic(&state, true)?;
        }
        if (resume || (action == "pair" && result.is_ok())) && paired(&state, &config) {
            runner.start(&state, &config)?;
        }
        result
    })
    .await
    .map_err(|_| "Collector task failed.".to_string())?
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RestorePreview {
    operation: String,
    expected_current_sha256: String,
}
fn set_automatic(state: &Desktop, enabled: bool) -> Result<(), String> {
    let mut conn = state
        .connection
        .lock()
        .map_err(|_| "Desktop settings are unavailable.")?;
    let mut config = conn.config.clone();
    config.automatic_collection = enabled;
    state.save(&config)?;
    conn.config = config;
    Ok(())
}
pub fn supervise(app: &tauri::AppHandle) {
    let state = app.state::<Desktop>();
    let Ok(mut runner) = state.collector.try_lock() else {
        return;
    };
    if crate::updates::installation_active(app) {
        return;
    }
    let Ok(conn) = state.connection.try_lock() else {
        return;
    };
    let config = conn.config.clone();
    drop(conn);
    if config.automatic_collection && paired(&state, &config) {
        if let Err(error) = runner.start(&state, &config) {
            runner.last_exit = Some(error);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn child_helper() {
        if std::env::var_os("NAUCTIONS_TEST_CHILD").is_some() {
            std::thread::sleep(std::time::Duration::from_secs(30));
        }
    }
    #[test]
    fn shutdown_reaps_only_the_owned_process() {
        let spawn = || {
            Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "collector::tests::child_helper"])
                .env("NAUCTIONS_TEST_CHILD", "1")
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .unwrap()
        };
        let mut other = spawn();
        let mut runner = Collector {
            child: Some(spawn()),
            #[cfg(windows)]
            job: None,
            last_exit: None,
            logs: None,
        };
        assert!(runner.running().unwrap());
        runner.stop().unwrap();
        assert!(!runner.running().unwrap());
        assert!(other.try_wait().unwrap().is_none());
        other.kill().unwrap();
        other.wait().unwrap();
        runner.stop().unwrap();
    }
}

/// Register this installation with the authenticated member. Keep an existing
/// registration and the user's pause preference when the owner has not changed.
#[tauri::command]
pub async fn auto_connect_collector(csrf: String, app: tauri::AppHandle) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<Desktop>();
        if crate::updates::installation_active(&app) {
            return Err("Wait for the app update to finish.".into());
        }
        auto_connect(&state, csrf)
    })
    .await
    .map_err(|_| "Collector setup failed.".to_string())?
}

fn auto_connect(state: &Desktop, csrf: String) -> Result<(), String> {
    // The same lock also serializes server changes, folder changes, and pairing.
    let mut runner = state.collector.lock().map_err(|_| "Collector is busy.")?;
    let (config, client) = {
        let conn = state
            .connection
            .lock()
            .map_err(|_| "Desktop settings are unavailable.")?;
        (conn.config.clone(), conn.client.clone())
    };
    let server = config.server.clone().ok_or("Connect to a server first.")?;
    let root = config.game_root.as_ref().ok_or("Set up WoW first.")?;
    let data = data_dir(state, &config)?;
    let member = tauri::async_runtime::block_on(crate::transport::request(
        client.clone(),
        server.clone(),
        "/me".into(),
        "GET".into(),
        None,
        None,
    ))?;
    if member.status != 200 {
        return Err("Sign in again to finish collector setup.".into());
    }
    let identity = member.body;
    let owner = identity["id"]
        .as_str()
        .ok_or("Cannot identify your membership.")?;
    let owner_file = data.join("automatic-owner");
    if paired(state, &config) && fs::read_to_string(&owner_file).is_ok_and(|saved| saved == owner) {
        if config.automatic_collection {
            runner.start(state, &config)?;
        }
        return Ok(());
    }
    let reply = tauri::async_runtime::block_on(crate::transport::request(
        client,
        server.clone(),
        "/devices/pair-code".into(),
        "POST".into(),
        Some(serde_json::json!({})),
        Some(csrf),
    ))?;
    if reply.status != 200 {
        return Err("Cannot register this computer. Check the connection and retry setup.".into());
    }
    let reply = reply.body;
    let code = reply["code"]
        .as_str()
        .filter(|v| v.len() == 64 && v.bytes().all(|b| b.is_ascii_hexdigit()))
        .ok_or("Cannot read collector registration.")?;
    runner.stop()?;
    let mut cmd = command(state, &data)?;
    cmd.arg("pair")
        .arg("--root")
        .arg(root)
        .arg("--server")
        .arg(&server)
        .arg("--code")
        .arg(code)
        .arg("--display-name")
        .arg("Nauctions desktop");
    if server.starts_with("http://") {
        cmd.arg("--allow-localhost-http");
    }
    let output = cmd
        .output()
        .map_err(|_| "Cannot run the bundled collector.")?;
    if !output.status.success() {
        return Err("Collector registration failed. Check the connection and retry setup.".into());
    }
    set_automatic(state, true)?;
    fs::write(owner_file, owner).map_err(|_| "Cannot save collector ownership.")?;
    runner.start(state, &config)?;
    Ok(())
}

#[cfg(test)]
mod automatic_setup_tests {
    use super::*;
    use std::{
        io::{Read, Write},
        net::TcpListener,
        sync::Mutex,
        time::{Duration, Instant},
    };

    fn mock_server(
        replies: Vec<(&'static str, u16, serde_json::Value)>,
    ) -> (String, std::thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let thread = std::thread::spawn(move || {
            for (path, status, body) in replies {
                let deadline = Instant::now() + Duration::from_secs(15);
                let mut stream = loop {
                    if let Ok((stream, _)) = listener.accept() {
                        break stream;
                    }
                    assert!(Instant::now() < deadline, "Missing request: {path}");
                    std::thread::sleep(Duration::from_millis(5));
                };
                stream.set_nonblocking(false).unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut request = vec![];
                loop {
                    let mut byte = [0];
                    stream.read_exact(&mut byte).unwrap();
                    request.push(byte[0]);
                    if request.ends_with(b"\r\n\r\n") {
                        break;
                    }
                }
                let request = String::from_utf8(request).unwrap();
                assert!(request.lines().next().unwrap().contains(path), "{request}");
                if path == "/v1/devices/pair-code" {
                    assert!(request.to_lowercase().contains("x-csrf-token: setup-csrf"));
                    assert!(request.contains("market_session=setup-session"));
                }
                let size = request
                    .lines()
                    .find_map(|line| {
                        line.to_lowercase()
                            .strip_prefix("content-length:")
                            .map(|v| v.trim().parse::<usize>().unwrap())
                    })
                    .unwrap_or(0);
                let mut posted = vec![0; size];
                stream.read_exact(&mut posted).unwrap();
                if path == "/v1/devices/enroll" {
                    let posted: serde_json::Value = serde_json::from_slice(&posted).unwrap();
                    assert_eq!(posted["code"], "a".repeat(64));
                }
                let body = body.to_string();
                write!(stream, "HTTP/1.1 {status} OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
            }
        });
        (origin, thread)
    }

    fn state(data: &Path, root: &Path, origin: String) -> Desktop {
        Desktop {
            connection: Mutex::new(crate::Connection {
                config: Config {
                    server: Some(origin.clone()),
                    game_root: Some(root.to_path_buf()),
                    automatic_collection: false,
                    ..Config::default()
                },
                client: crate::transport::client_with_session(Some(&origin), Some("setup-session"))
                    .unwrap(),
            }),
            collector: Mutex::new(Collector::default()),
            data: data.to_path_buf(),
            binary: PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("resources/market-collector.exe"),
            startup_error: Mutex::new(None),
        }
    }

    #[test]
    fn automatic_enrollment_uses_membership_and_reuses_the_device_on_next_sign_in() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("game");
        fs::create_dir_all(root.join("WTF/Account")).unwrap();
        let root = root.canonicalize().unwrap();
        let (origin, server) = mock_server(vec![
            ("/v1/me", 200, serde_json::json!({"id":"member-one"})),
            (
                "/v1/devices/pair-code",
                200,
                serde_json::json!({"code":"a".repeat(64)}),
            ),
            ("/v1/collector/capabilities", 200, serde_json::json!({})),
            (
                "/v1/devices/enroll",
                200,
                serde_json::json!({"device_id":"00000000-0000-0000-0000-000000000001", "token":"local-test-token"}),
            ),
            ("/v1/me", 200, serde_json::json!({"id":"member-one"})),
            ("/v1/me", 200, serde_json::json!({"id":"member-two"})),
            (
                "/v1/devices/pair-code",
                200,
                serde_json::json!({"code":"a".repeat(64)}),
            ),
            ("/v1/collector/capabilities", 200, serde_json::json!({})),
            (
                "/v1/devices/enroll",
                200,
                serde_json::json!({"device_id":"00000000-0000-0000-0000-000000000002", "token":"second-member-token"}),
            ),
        ]);
        let state = state(&dir.path().join("data"), &root, origin);
        auto_connect(&state, "setup-csrf".into()).unwrap();
        let config = state.connection.lock().unwrap().config.clone();
        assert!(paired(&state, &config));
        assert!(config.automatic_collection);
        assert!(state.collector.lock().unwrap().running().unwrap());
        state.collector.lock().unwrap().stop().unwrap();
        set_automatic(&state, false).unwrap();
        let data = data_dir(&state, &config).unwrap();
        let before = fs::read(data.join("credential.dpapi")).unwrap();
        auto_connect(&state, "setup-csrf".into()).unwrap();
        assert_eq!(fs::read(data.join("credential.dpapi")).unwrap(), before);
        assert!(!state.connection.lock().unwrap().config.automatic_collection);
        assert!(!state.collector.lock().unwrap().running().unwrap());
        // A different signed-in member must not inherit the first member's device.
        auto_connect(&state, "setup-csrf".into()).unwrap();
        assert_eq!(
            fs::read_to_string(data.join("automatic-owner")).unwrap(),
            "member-two"
        );
        let paired_config: serde_json::Value =
            serde_json::from_slice(&fs::read(data.join("config.json")).unwrap()).unwrap();
        assert_eq!(
            paired_config["device_id"],
            "00000000-0000-0000-0000-000000000002"
        );
        state.collector.lock().unwrap().stop().unwrap();
        server.join().unwrap();
    }

    #[test]
    fn automatic_enrollment_requires_an_active_member_session() {
        let dir = tempfile::tempdir().unwrap();
        let (origin, server) = mock_server(vec![(
            "/v1/me",
            401,
            serde_json::json!({"error":"unauthorized"}),
        )]);
        let state = state(&dir.path().join("data"), dir.path(), origin);
        assert!(auto_connect(&state, "setup-csrf".into())
            .unwrap_err()
            .contains("Sign in again"));
        assert!(!state.collector.lock().unwrap().running().unwrap());
        assert!(!state.connection.lock().unwrap().config.automatic_collection);
        server.join().unwrap();
    }
}
