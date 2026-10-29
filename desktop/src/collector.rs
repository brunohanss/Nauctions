use crate::{Config, Desktop};
use serde::Serialize;
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
                self.child = None;
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
        let mut child = command
            .spawn()
            .map_err(|_| "Cannot start the bundled collector.")?;
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
fn data_dir(state: &Desktop, config: &Config) -> Result<PathBuf, String> {
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
    app: tauri::AppHandle,
) -> Result<String, String> {
    if matches!(action.as_str(), "sync" | "sync_and_start") {
        return Err("Shared-history writes are disabled pending recovery and game compatibility validation. Use Preview sync.".into());
    }
    tauri::async_runtime::spawn_blocking(move || {
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
            "pair" | "status" | "retry" | "sync_preview" | "sync" | "sync_and_start"
        ) {
            return Err("Unknown collector action.".into());
        }
        if action != "pair" && !paired(&state, &config) {
            return Err("Pair the collector first.".into());
        }
        let data = data_dir(&state, &config)?;
        let mut cmd = command(&state, &data)?;
        match action.as_str() {
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
