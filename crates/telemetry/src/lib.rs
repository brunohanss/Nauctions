//! Bounded, best-effort observability. Callers pass fixed codes, never raw errors or payloads.
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    fs::{self, OpenOptions},
    io::Write,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{sync_channel, SyncSender},
        Mutex, OnceLock,
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

pub const RELEASE: &str = env!("NAUCTIONS_RELEASE");
pub const REVISION: &str = env!("NAUCTIONS_REVISION");
const HOST: &str = "https://eu.i.posthog.com";
static STATE: OnceLock<State> = OnceLock::new();
static SEND: AtomicBool = AtomicBool::new(true);
static GENERATION: AtomicU64 = AtomicU64::new(0);
static LOCAL_LOCK: Mutex<()> = Mutex::new(());
pub fn set_enabled(enabled: bool) {
    SEND.store(enabled, Ordering::SeqCst);
    if !enabled {
        GENERATION.fetch_add(1, Ordering::SeqCst);
    }
}
enum Work {
    Send(u64, &'static str, Value),
    Flush(SyncSender<()>),
}
struct State {
    component: &'static str,
    logs: SyncSender<Work>,
    local: Option<PathBuf>,
    recent: Mutex<HashMap<&'static str, Instant>>,
}

pub fn enabled() -> bool {
    std::env::var("NAUCTIONS_TELEMETRY").map_or(!cfg!(debug_assertions), |v| v == "1")
}

/// Initialize outside the application runtime. No network operation runs on the caller.
pub fn init(component: &'static str, local: Option<PathBuf>) {
    STATE.get_or_init(|| {
        let key = std::env::var("POSTHOG_API_KEY")
            .unwrap_or_else(|_| env!("NAUCTIONS_POSTHOG_KEY").into());
        let remote = enabled() && key.starts_with("phc_");
        let (logs, receiver) = sync_channel::<Work>(128);
        std::thread::spawn(move || {
            let http = reqwest::blocking::Client::builder()
                .timeout(Duration::from_secs(3))
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .ok();
            while let Ok(work) = receiver.recv() {
                let Work::Send(generation, endpoint, record) = work else {
                    if let Work::Flush(done) = work {
                        let _ = done.send(());
                    }
                    continue;
                };
                if remote && permitted(generation) {
                    if let Some(http) = &http {
                        // No retries: an outage must not produce unbounded traffic or storage.
                        let _ = http
                            .post(format!("{HOST}/{endpoint}"))
                            .bearer_auth(&key)
                            .json(&record)
                            .send();
                    }
                }
            }
        });
        State {
            component,
            logs,
            local,
            recent: Mutex::new(HashMap::new()),
        }
    });
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        // Panic messages can contain credentials or file contents. Keep only a fixed code.
        error("rust_panic", json!({}));
        flush(Duration::from_secs(2));
        previous(info);
    }));
    record("process_started", json!({}));
}

/// Give short-lived commands a bounded chance to send their final exception.
pub fn flush(timeout: Duration) {
    if !SEND.load(Ordering::SeqCst) {
        return;
    }
    let Some(state) = STATE.get() else {
        return;
    };
    let (done, wait) = sync_channel(1);
    if state.logs.try_send(Work::Flush(done)).is_ok() {
        let _ = wait.recv_timeout(timeout);
    }
}

fn permitted(generation: u64) -> bool {
    SEND.load(Ordering::SeqCst) && generation == GENERATION.load(Ordering::SeqCst)
}

fn attributes(state: &State, input: Value) -> Value {
    let mut result = json!({"component": state.component, "release": RELEASE,
        "revision": REVISION, "os": std::env::consts::OS});
    // The caller must provide fixed codes, numeric measurements, or generated incident IDs.
    for name in [
        "incident_id",
        "request_id",
        "status",
        "duration_ms",
        "queued",
        "rejected",
        "operation",
    ] {
        if let Some(value) = input.get(name) {
            if value.is_number()
                || value.is_boolean()
                || value.as_str().is_some_and(|s| {
                    s.len() <= 100
                        && s.chars()
                            .all(|c| c.is_ascii_alphanumeric() || "_-./".contains(c))
                })
            {
                result[name] = value.clone();
            }
        }
    }
    result
}

pub fn record(code: &'static str, input: Value) {
    emit(code, input, false);
}

pub fn error(code: &'static str, input: Value) {
    emit(code, input, true);
}

fn emit(code: &'static str, mut input: Value, failure: bool) {
    let Some(state) = STATE.get() else { return };
    // Suppress loops such as a failed upload every few seconds. Local and remote use one budget.
    if let Ok(mut recent) = state.recent.try_lock() {
        if recent
            .get(code)
            .is_some_and(|t| t.elapsed() < Duration::from_secs(30))
        {
            return;
        }
        recent.insert(code, Instant::now());
    } else {
        return;
    }
    if failure && input.get("incident_id").is_none() {
        input["incident_id"] = json!(uuid::Uuid::new_v4().to_string());
    }
    let properties = attributes(state, input);
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let line = json!({"time_unix_ms": now.as_millis() as u64, "level": if failure {"error"} else {"info"}, "code": code, "attributes": properties});
    if let Some(path) = &state.local {
        append_local(path, &line);
    }
    let attrs: Vec<Value> = properties.as_object().unwrap().iter().map(|(k,v)| json!({"key": k, "value": if let Some(n) = v.as_u64() { json!({"intValue": n.to_string()}) } else { json!({"stringValue": v.as_str().unwrap_or("")}) }})).collect();
    if !SEND.load(Ordering::Relaxed) {
        return;
    }
    let generation = GENERATION.load(Ordering::SeqCst);
    let _ = state.logs.try_send(Work::Send(generation, "i/v1/logs", json!({"resourceLogs":[{"resource":{"attributes":[{"key":"service.name","value":{"stringValue":format!("nauctions-{}", state.component)}},{"key":"service.version","value":{"stringValue":RELEASE}}]},"scopeLogs":[{"scope":{"name":"nauctions"},"logRecords":[{"timeUnixNano":now.as_nanos().to_string(),"severityNumber":if failure {17} else {9},"severityText":if failure {"ERROR"} else {"INFO"},"body":{"stringValue":code},"attributes":attrs}]}]}]})));
    // Build the complete native payload here. Do not collect stacks or loaded modules:
    // SDK-generated source/module paths can contain a Windows account name.
    let _ = state.logs.try_send(Work::Send(
        generation,
        "capture/",
        native_event(state, code, properties, failure),
    ));
}
fn native_event(state: &State, code: &str, mut properties: Value, failure: bool) -> Value {
    properties["$process_person_profile"] = json!(false);
    properties["$geoip_disable"] = json!(true);
    if failure {
        properties["$exception_list"] = json!([{"type":"NativeFailure","value":code}]);
        properties["$exception_fingerprint"] = json!(format!("{}:{code}", state.component));
    }
    json!({"api_key": std::env::var("POSTHOG_API_KEY").unwrap_or_else(|_| env!("NAUCTIONS_POSTHOG_KEY").into()),
        "event": if failure { "$exception" } else { code },
        "distinct_id": format!("nauctions-{}", state.component), "properties": properties})
}

fn append_local(path: &PathBuf, value: &Value) {
    let Ok(_lock) = LOCAL_LOCK.try_lock() else {
        return;
    };
    // A separate file per process. Never include raw stdout/stderr in the safe diagnostics log.
    if fs::metadata(path).is_ok_and(|m| m.len() >= 1024 * 1024) {
        let backup = path.with_extension("jsonl.1");
        let _ = fs::remove_file(&backup);
        if fs::rename(path, backup).is_err() {
            return;
        }
    }
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "{value}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn opt_out_invalidates_prepared_work_even_after_reenable() {
        let queued = GENERATION.load(Ordering::SeqCst);
        set_enabled(false);
        assert!(!permitted(queued));
        set_enabled(true);
        assert!(!permitted(queued));
        assert!(permitted(GENERATION.load(Ordering::SeqCst)));
    }
    #[test]
    fn native_error_and_panic_have_no_sdk_paths() {
        let (logs, _) = sync_channel(1);
        let state = State {
            component: "test",
            logs,
            local: None,
            recent: Mutex::new(HashMap::new()),
        };
        for code in ["rust_panic", "collector_command_failed"] {
            let safe = attributes(
                &state,
                json!({"filename":"C:\\Users\\Alice\\main.rs", "$debug_images":[{"code_file":"C:\\Users\\Alice\\app.exe"}], "stack":"private"}),
            );
            let payload = native_event(&state, code, safe, true);
            let encoded = payload.to_string();
            assert!(!encoded.contains("Alice"));
            assert!(!encoded.contains("debug_images"));
            assert!(!encoded.contains("stack"));
            assert_eq!(payload["properties"]["$exception_list"][0]["value"], code);
        }
    }
    #[test]
    fn excludes_secrets_paths_and_unbounded_values() {
        let (logs, _) = sync_channel(1);
        let state = State {
            component: "test",
            logs,
            local: None,
            recent: Mutex::new(HashMap::new()),
        };
        let result = attributes(
            &state,
            json!({"token":"secret", "body":"private", "operation":"C:\\Users\\Alice", "request_id":"valid-id", "status":503}),
        );
        assert!(result.get("token").is_none());
        assert!(result.get("body").is_none());
        assert!(result.get("operation").is_none());
        assert_eq!(result["request_id"], "valid-id");
        assert_eq!(result["status"], 503);
    }
}
