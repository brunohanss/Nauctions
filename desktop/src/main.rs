#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod collector;
#[cfg(windows)]
mod collector_job;
mod collector_log;
mod diagnostics;
mod identity;
#[cfg(test)]
mod live_smoke;
mod remember;
mod settings;
mod setup;
mod startup;
mod transport;
mod tray_activity;
mod updates;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{fs, path::PathBuf, sync::Mutex};
use tauri::{Manager, State, WebviewUrl, WebviewWindowBuilder};

use tauri_plugin_dialog::DialogExt;
use tauri_plugin_notification::NotificationExt;

#[tauri::command]
fn send_test_alert(app: tauri::AppHandle) -> Result<(), String> {
    app.notification()
        .builder()
        .title("Nauctions test alert")
        .body("This is a test notification from your watchlist. No purchase action is needed.")
        .show()
        .map_err(|_| {
            "Could not send the test alert. Check Windows notification settings for Nauctions."
                .into()
        })
}

#[derive(Clone, Deserialize, Serialize)]
struct Config {
    server: Option<String>,
    game_root: Option<PathBuf>,
    game_exe: Option<PathBuf>,
    #[serde(default = "enabled")]
    launch_at_login: bool,
    #[serde(default = "enabled")]
    automatic_collection: bool,
    #[serde(default)]
    startup_registered_path: Option<PathBuf>,
}
fn enabled() -> bool {
    true
}
impl Default for Config {
    fn default() -> Self {
        Self {
            server: None,
            game_root: None,
            game_exe: None,
            launch_at_login: true,
            automatic_collection: true,
            startup_registered_path: None,
        }
    }
}
struct Connection {
    config: Config,
    client: Client,
}
struct Desktop {
    connection: Mutex<Connection>,
    collector: Mutex<collector::Collector>,
    data: PathBuf,
    binary: PathBuf,
    startup_error: Mutex<Option<String>>,
}
impl Desktop {
    fn save(&self, config: &Config) -> Result<(), String> {
        settings::save(&self.data, config)
    }
}
#[tauri::command]
fn desktop_config(state: State<'_, Desktop>) -> Result<Config, String> {
    Ok(state
        .connection
        .lock()
        .map_err(|_| "Desktop settings are unavailable.")?
        .config
        .clone())
}
#[tauri::command]
fn configure_server(server: String, state: State<'_, Desktop>) -> Result<Config, String> {
    let server = transport::server_origin(&server)?;
    let mut runner = state.collector.lock().map_err(|_| "Collector is busy.")?;
    let mut conn = state
        .connection
        .lock()
        .map_err(|_| "Desktop settings are unavailable.")?;
    runner.stop()?;
    let mut config = conn.config.clone();
    config.server = Some(server);
    config.automatic_collection = false;
    state.save(&config)?;
    conn.client = remember::client(&state.data, config.server.as_deref())?;
    conn.config = config.clone();
    Ok(config)
}
#[tauri::command]
async fn api_request(
    path: String,
    method: String,
    body: Option<Value>,
    csrf: Option<String>,
    state: State<'_, Desktop>,
    app: tauri::AppHandle,
) -> Result<transport::Reply, String> {
    let _activity =
        (path == "/forecasts/addon" && method == "POST").then(|| tray_activity::begin(&app));
    let (client, origin) = {
        let conn = state
            .connection
            .lock()
            .map_err(|_| "Desktop settings are unavailable.")?;
        (
            conn.client.clone(),
            conn.config
                .server
                .clone()
                .ok_or("Configure the server first.")?,
        )
    };
    let logout = path == "/auth/logout" && method == "POST";
    let joining = path == "/auth/join" && method == "POST";
    let admin_login = path == "/auth/login" && method == "POST";
    let checking_session = path == "/me";
    let result = transport::request(client, origin.clone(), path, method, body, csrf).await?;
    if joining && result.status < 300 {
        remember::save(
            &state.data,
            &origin,
            result
                .session_token
                .as_deref()
                .ok_or("The server did not issue a membership credential.")?,
        )?;
    }
    if (checking_session && result.status == 401) || (admin_login && result.status < 300) {
        remember::forget(&state.data, &origin)?;
    }
    if logout && result.status < 300 {
        let mut runner = state.collector.lock().map_err(|_| "Collector is busy.")?;
        let mut conn = state
            .connection
            .lock()
            .map_err(|_| "Desktop settings are unavailable.")?;
        runner.stop()?;
        let mut config = conn.config.clone();
        config.automatic_collection = false;
        state.save(&config)?;
        conn.config = config;
        remember::forget(&state.data, &origin)?;
    }
    Ok(result)
}
#[tauri::command]
async fn select_game_path(executable: bool, app: tauri::AppHandle) -> Result<Config, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let chosen = if executable {
            app.dialog()
                .file()
                .set_title("Select the WoW executable")
                .add_filter("WoW executable", &["exe"])
                .blocking_pick_file()
        } else {
            app.dialog()
                .file()
                .set_title("Select the WoW folder containing WTF")
                .blocking_pick_folder()
        };
        let state = app.state::<Desktop>();
        let Some(chosen) = chosen else {
            return desktop_config(state);
        };
        let path = chosen
            .into_path()
            .map_err(|_| "Select a local filesystem path.")?
            .canonicalize()
            .map_err(|_| "Selected path is unavailable.")?;
        let mut runner = state.collector.lock().map_err(|_| "Collector is busy.")?;
        let mut conn = state
            .connection
            .lock()
            .map_err(|_| "Desktop settings are unavailable.")?;
        let mut config = conn.config.clone();
        if executable {
            let root = config
                .game_root
                .as_ref()
                .ok_or("Select the WoW folder first.")?;
            if !path.starts_with(root)
                || !path.is_file()
                || path
                    .extension()
                    .is_none_or(|e| !e.eq_ignore_ascii_case("exe"))
            {
                return Err("Choose an executable inside the selected WoW folder.".into());
            }
            runner.stop()?;
            config.game_exe = Some(path);
        } else {
            if !path.join("WTF").join("Account").is_dir() {
                return Err("Select the WoW folder that contains WTF\\Account.".into());
            }
            if state.data.starts_with(&path) {
                return Err("The WoW folder cannot contain Nauctions application data.".into());
            }
            runner.stop()?;
            config.game_root = Some(path);
            config.game_exe = None;
            config.automatic_collection = false;
        }
        state.save(&config)?;
        conn.config = config.clone();
        Ok(config)
    })
    .await
    .map_err(|_| "Folder selection failed.".to_string())?
}
#[tauri::command]
fn set_desktop_preferences(
    launch_at_login: bool,
    automatic_collection: bool,
    app: tauri::AppHandle,
) -> Result<Config, String> {
    let state = app.state::<Desktop>();
    let mut runner = state
        .collector
        .try_lock()
        .map_err(|_| "Collector is busy. Try again when the current action ends.")?;
    let mut conn = state
        .connection
        .lock()
        .map_err(|_| "Desktop settings are unavailable.")?;
    let previous = conn.config.clone();
    if launch_at_login {
        startup::set_enabled(true)
    } else {
        startup::set_enabled(false)
    }
    .map_err(|e| format!("Cannot change Windows startup: {e}"))?;
    let mut config = previous.clone();
    config.launch_at_login = launch_at_login;
    config.automatic_collection = automatic_collection;
    config.startup_registered_path = std::env::current_exe().ok();
    if let Err(error) = state.save(&config) {
        let _ = if previous.launch_at_login {
            startup::set_enabled(true)
        } else {
            startup::set_enabled(false)
        };
        return Err(error);
    }
    conn.config = config;
    *state
        .startup_error
        .lock()
        .map_err(|_| "Desktop settings are unavailable.")? = None;
    if !automatic_collection {
        runner.stop()?;
    }
    Ok(conn.config.clone())
}
fn show(app: &tauri::AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}
fn background_launch(args: &[String]) -> bool {
    args.iter().any(|a| a == "--background")
}
fn main() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, args, _| {
            if !background_launch(&args) {
                show(app);
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(updates::Updates::default())
        .invoke_handler(tauri::generate_handler![
            desktop_config,
            send_test_alert,
            configure_server,
            api_request,
            select_game_path,
            setup::discover_wow,
            setup::prepare_wow,
            collector::auto_connect_collector,
            set_desktop_preferences,
            collector::collector_action,
            collector::collector_state,
            updates::check_update,
            updates::install_update,
            diagnostics::diagnostics_snapshot,
            diagnostics::export_diagnostics,
            diagnostics::set_observability,
            diagnostics::observability_preference
        ])
        .setup(|app| {
            let data = app.path().app_local_data_dir()?;
            fs::create_dir_all(&data)?;
            market_telemetry::set_enabled(!data.join("telemetry-disabled").exists());
            market_telemetry::init("desktop", Some(data.join("desktop-diagnostics.jsonl")));
            let (mut config, mut startup_error) =
                settings::load(&data).map_err(std::io::Error::other)?;
            if !cfg!(debug_assertions) {
                let path = std::env::current_exe()?;
                // Repair a removed registration after reinstall. A Task Manager
                // disable retains its Run entry and must remain disabled.
                let registration_missing =
                    config.launch_at_login && matches!(startup::registration_present(), Ok(false));
                if config.startup_registered_path.as_ref() != Some(&path) || registration_missing {
                    match if config.launch_at_login {
                        startup::set_enabled(true)
                    } else {
                        startup::set_enabled(false)
                    } {
                        Ok(()) => {
                            config.startup_registered_path = Some(path);
                            settings::save(&data, &config).map_err(std::io::Error::other)?;
                        }
                        Err(error) => {
                            startup_error =
                                Some(format!("Windows startup could not be configured: {error}"));
                        }
                    }
                }
            }
            let client =
                remember::client(&data, config.server.as_deref()).map_err(std::io::Error::other)?;
            app.manage(Desktop {
                connection: Mutex::new(Connection { config, client }),
                collector: Mutex::new(collector::Collector::default()),
                data,
                binary: app.path().resource_dir()?.join("market-collector.exe"),
                startup_error: Mutex::new(startup_error),
            });
            let window =
                WebviewWindowBuilder::new(app, "main", WebviewUrl::App("index.html".into()))
                    .title(identity::NAME)
                    .visible(!background_launch(&std::env::args().collect::<Vec<_>>()))
                    .inner_size(1280.0, 850.0)
                    .min_inner_size(760.0, 560.0)
                    .on_navigation(|url| {
                        url.scheme() == "tauri"
                            || matches!(url.host_str(), Some("tauri.localhost"))
                            || (cfg!(debug_assertions)
                                && url.host_str() == Some("127.0.0.1")
                                && url.port() == Some(1420))
                    })
                    .on_new_window(|_, _| tauri::webview::NewWindowResponse::Deny)
                    .build()?;
            if let Some(icon) = app.default_window_icon() {
                window.set_icon(icon.clone())?;
            }
            let open = tauri::menu::MenuItem::with_id(
                app,
                "open",
                format!("Open {}", identity::NAME),
                true,
                None::<&str>,
            )?;
            let quit = tauri::menu::MenuItem::with_id(
                app,
                "quit",
                format!("Quit {}", identity::NAME),
                true,
                None::<&str>,
            )?;
            let menu = tauri::menu::Menu::with_items(app, &[&open, &quit])?;
            let mut tray = tauri::tray::TrayIconBuilder::with_id(tray_activity::TRAY_ID)
                .tooltip(identity::NAME)
                .menu(&menu)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "open" => show(app),
                    "quit" => app.exit(0),
                    _ => {}
                });
            if let Some(icon) = app.default_window_icon() {
                tray = tray.icon(icon.clone());
            }
            tray.build(app)?;
            tray_activity::start(app.handle());
            let handle = app.handle().clone();
            std::thread::spawn(move || loop {
                collector::supervise(&handle);
                std::thread::sleep(std::time::Duration::from_secs(10));
            });
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .build(tauri::generate_context!())
        .expect("Cannot initialize Nauctions");
    app.run(|app, event| {
        if matches!(event, tauri::RunEvent::Exit) {
            market_telemetry::flush(std::time::Duration::from_secs(2));
            if let Ok(mut collector) = app.state::<Desktop>().collector.lock() {
                let _ = collector.stop();
            }
        }
    });
}

#[cfg(test)]
mod startup_tests {
    use super::*;
    #[test]
    fn old_settings_enable_new_defaults_without_losing_selected_paths() {
        let config: Config = serde_json::from_value(
            serde_json::json!({"server":"https://example.com","game_root":null,"game_exe":null}),
        )
        .unwrap();
        assert!(config.launch_at_login && config.automatic_collection);
        assert_eq!(config.server.as_deref(), Some("https://example.com"));
        assert!(config.startup_registered_path.is_none());
    }
    #[test]
    fn startup_flag_keeps_the_window_hidden_but_manual_launch_opens_it() {
        assert!(background_launch(&[
            "Nauctions.exe".into(),
            "--background".into()
        ]));
        assert!(!background_launch(&["Nauctions.exe".into()]));
    }
}
