use std::{env, fs, path::Path, process::Command};

fn main() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let local = root.join("telemetry.local.json");
    println!("cargo:rerun-if-changed={}", local.display());
    println!("cargo:rerun-if-env-changed=POSTHOG_API_KEY");
    println!("cargo:rerun-if-env-changed=NAUCTIONS_REVISION");
    println!(
        "cargo:rerun-if-changed={}",
        root.join(".git/HEAD").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        root.join(".git/refs").display()
    );
    let config: serde_json::Value = fs::read(&local)
        .ok()
        .and_then(|v| serde_json::from_slice(&v).ok())
        .unwrap_or_default();
    let key = env::var("POSTHOG_API_KEY")
        .ok()
        .or_else(|| config["project_key"].as_str().map(str::to_owned))
        .unwrap_or_default();
    assert!(!key.contains(['\n', '\r']), "Invalid project key");
    assert!(
        key.is_empty() || key.starts_with("phc_"),
        "POSTHOG_API_KEY must be a project ingestion key, never a personal API key"
    );
    println!("cargo:rustc-env=NAUCTIONS_POSTHOG_KEY={key}");
    let version: serde_json::Value = fs::read(root.join("desktop/tauri.conf.json"))
        .ok()
        .and_then(|v| serde_json::from_slice(&v).ok())
        .unwrap_or_default();
    println!(
        "cargo:rerun-if-changed={}",
        root.join("desktop/tauri.conf.json").display()
    );
    println!(
        "cargo:rustc-env=NAUCTIONS_RELEASE={}",
        version["version"].as_str().unwrap_or("unknown")
    );
    let revision = env::var("NAUCTIONS_REVISION")
        .ok()
        .or_else(|| {
            Command::new("git")
                .args(["rev-parse", "--short=12", "HEAD"])
                .current_dir(&root)
                .output()
                .ok()
                .filter(|o| o.status.success())
                .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
        })
        .unwrap_or_else(|| "unknown".into());
    println!("cargo:rustc-env=NAUCTIONS_REVISION={revision}");
}
