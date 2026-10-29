//! Test fixture producer/verifier. Never reads an installed game's SavedVariables.
use ahdb_format::{merge_history, parse};
use market_domain::{ExportPoint, HistoryExport, Market};
use std::{error::Error, fs, path::PathBuf};

fn main() -> Result<(), Box<dyn Error>> {
    let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../artifacts/offline-sync");
    let verify = std::env::args().nth(1).as_deref() == Some("--verify");
    fs::create_dir_all(&directory)?;
    let professions = include_str!("../../../fixtures/synthetic-professions.lua")
        .split_once("NauctionsProfessionsSaved")
        .ok_or("missing synthetic professions")?
        .1;
    for player in ["a", "b"] {
        let original = format!(
            "{}\nNauctionsProfessionsSaved{professions}",
            include_str!("../../../fixtures/synthetic-v6.lua")
                .replace(
                    "1793100000",
                    if player == "a" {
                        "1793100000"
                    } else {
                        "1793100020"
                    }
                )
                .replace(
                    "_formatVersion_=5,",
                    "_formatVersion_=5,_count_=2,_infoCount_=1,_created_=1793100000,"
                )
        );
        let original_parsed = parse(original.as_bytes())?;
        if verify {
            let saved = fs::read(directory.join(format!("player-{player}-reloaded.lua")))?;
            let parsed = parse(&saved)?;
            assert_eq!(parsed.scans.len(), original_parsed.scans.len() + 1);
            assert_eq!(
                parsed.scans[0].fingerprint,
                original_parsed.scans[0].fingerprint
            );
            assert_eq!(
                parsed.imported_history_markets,
                vec![Market {
                    realm: "Synthetic Realm".into(),
                    house: "Alliance".into()
                }]
            );
            assert_eq!(
                serde_json::to_value(&parsed.professions)?,
                serde_json::to_value(&original_parsed.professions)?
            );
            assert!(parsed.daily.iter().any(|point| point.item_key == "789"));
            assert_eq!(parsed.scans.last().unwrap().listings[0].item_key, "i790");
            println!("player-{player}: addon reload, native scans, marker and recipes verified");
            continue;
        }
        let export = HistoryExport {
            schema_version: 1,
            market: Market {
                realm: "Synthetic Realm".into(),
                house: "Alliance".into(),
            },
            generated_at: 1793101000,
            points: vec![ExportPoint {
                item_id: 789,
                observed_at: 1793101000,
                min_unit_copper: 4500,
                median_unit_copper: 4600,
                quantity: 7,
            }],
            daily: vec![],
        };
        let merged = merge_history(original.as_bytes(), &export, "Synthetic Realm", "Alliance")?;
        assert_eq!(
            merge_history(&merged, &export, "Synthetic Realm", "Alliance")?,
            merged
        );
        assert_eq!(
            parse(&merged)?.scans[0].fingerprint,
            original_parsed.scans[0].fingerprint
        );
        fs::write(
            directory.join(format!("player-{player}-original.lua")),
            original,
        )?;
        fs::write(
            directory.join(format!("player-{player}-merged.lua")),
            merged,
        )?;
    }
    Ok(())
}
