//! Explicit, opt-in live check. Uses the native transport and the shipped collector.
//! It reads only a new synthetic game folder and uploads one empty scan, with no prices.
use crate::transport;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{fs, path::PathBuf, process::Command, time::Duration};

#[test]
#[ignore = "requires an explicitly configured live Nauctions endpoint and CI collector"]
fn live_companion_sign_in_pair_and_upload() {
    let origin = std::env::var("NAUCTIONS_LIVE_SMOKE_ORIGIN").expect("Set the live smoke origin");
    assert_eq!(origin, "https://nauctions.when-to-plant.net");
    assert_eq!(transport::server_origin(&origin).unwrap(), origin);
    let collector = PathBuf::from(
        std::env::var("NAUCTIONS_LIVE_SMOKE_COLLECTOR").expect("Set the CI collector path"),
    );
    assert!(collector.is_file());
    let root =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../artifacts/deployment/live-companion");
    fs::create_dir_all(&root).unwrap();
    let trial = tempfile::Builder::new()
        .prefix("trial-")
        .tempdir_in(root)
        .unwrap()
        .keep();
    let game = trial.join("synthetic-wow");
    let account = game.join("WTF/Account/NauctionsConnectivity/SavedVariables");
    fs::create_dir_all(&account).unwrap();
    let source = account.join("AuctionDB.lua");
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let payload = format!("-- SYNTHETIC connectivity check. One empty scan; no prices or character data.\nAuctionDBForeverSaved = {{ ah = {{ {{ realm=\"Normal\", faction=\"Horde\", ts={stamp}, dataFormatVersion=6, count=0, itemsCount=0, data=\"\" }} }}, houses = {{}} }}\n");
    fs::write(&source, &payload).unwrap();
    let source_hash = Sha256::digest(payload.as_bytes()).to_vec();
    let data = trial.join("collector");
    let nickname = format!("NativeCheck{stamp}");
    let mut report = json!({"origin": origin, "nickname": nickname, "trial": trial,
        "synthetic": true, "uploaded_scans": 1, "uploaded_prices": 0});
    tauri::async_runtime::block_on(async {
        let anonymous = transport::client().unwrap();
        let unauthenticated = transport::request(
            anonymous,
            origin.clone(),
            "/me".into(),
            "GET".into(),
            None,
            None,
        )
        .await
        .unwrap();
        assert_eq!(unauthenticated.status, 401);
        let client = transport::client().unwrap();
        assert_eq!(
            client
                .get(format!("{origin}/healthz"))
                .send()
                .await
                .unwrap()
                .status(),
            200
        );
        assert_eq!(client.get(&origin).send().await.unwrap().status(), 200);
        let joined = transport::request(
            client.clone(),
            origin.clone(),
            "/auth/join".into(),
            "POST".into(),
            Some(json!({"nickname": nickname, "code": "COPAINS"})),
            None,
        )
        .await
        .unwrap();
        assert_eq!(joined.status, 200);
        assert!(joined.session_token.is_some());
        let csrf = joined.body["csrf_token"].as_str().unwrap().to_owned();
        let me = transport::request(
            client.clone(),
            origin.clone(),
            "/me".into(),
            "GET".into(),
            None,
            None,
        )
        .await
        .unwrap();
        assert_eq!(me.status, 200);
        report["member_id"] = me.body["id"].clone();
        let markets = transport::request(
            client.clone(),
            origin.clone(),
            "/markets".into(),
            "GET".into(),
            None,
            None,
        )
        .await
        .unwrap();
        assert_eq!(markets.status, 200);
        report["markets"] = markets.body;
        let code = transport::request(
            client,
            origin.clone(),
            "/devices/pair-code".into(),
            "POST".into(),
            None,
            Some(csrf),
        )
        .await
        .unwrap();
        assert_eq!(code.status, 200);
        let paired = Command::new(&collector)
            .arg("--data-dir")
            .arg(&data)
            .args(["pair", "--root"])
            .arg(&game)
            .args([
                "--server",
                &origin,
                "--code",
                code.body["code"].as_str().unwrap(),
                "--display-name",
                "Synthetic native connection check",
            ])
            .output()
            .unwrap();
        assert!(
            paired.status.success(),
            "Collector pairing failed: {}",
            String::from_utf8_lossy(&paired.stderr)
        );
    });
    for _ in 0..8 {
        let retry = Command::new(&collector)
            .arg("--data-dir")
            .arg(&data)
            .arg("retry")
            .output()
            .unwrap();
        assert!(retry.status.success());
        let run = Command::new(&collector)
            .arg("--data-dir")
            .arg(&data)
            .args(["run", "--once"])
            .output()
            .unwrap();
        assert!(
            run.status.success(),
            "Collector reconciliation failed: {}",
            String::from_utf8_lossy(&run.stderr)
        );
        let db = rusqlite::Connection::open(data.join("spool.sqlite")).unwrap();
        let rows: Vec<(String, String, bool)> = db
            .prepare("SELECT id,state,payload IS NULL FROM queue")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        if rows.len() == 1 && rows[0].1 == "accepted" {
            assert!(rows[0].2, "Accepted payload bytes were not cleared");
            report["upload_id"] = json!(rows[0].0);
            report["receipt_status"] = json!("accepted");
            break;
        }
        assert!(
            rows.iter().all(|row| row.1 != "rejected"),
            "The synthetic upload was rejected"
        );
        std::thread::sleep(Duration::from_secs(2));
    }
    assert_eq!(report["receipt_status"], "accepted");
    assert_eq!(
        Sha256::digest(fs::read(source).unwrap()).to_vec(),
        source_hash
    );
    let config: serde_json::Value =
        serde_json::from_slice(&fs::read(data.join("config.json")).unwrap()).unwrap();
    report["device_id"] = config["device_id"].clone();
    report["source_hash_unchanged"] = json!(true);
    report["tls_validation"] = json!("standard Rustls trust; no overrides");
    fs::write(
        trial.join("result.json"),
        serde_json::to_vec_pretty(&report).unwrap(),
    )
    .unwrap();
    println!("{}", serde_json::to_string_pretty(&report).unwrap());
}
