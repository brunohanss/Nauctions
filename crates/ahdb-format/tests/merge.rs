use ahdb_format::{merge_history, parse};
use market_domain::*;
fn bundle() -> HistoryExport {
    HistoryExport {
        schema_version: 1,
        market: Market {
            realm: "Realm".into(),
            house: "Alliance".into(),
        },
        generated_at: 200 * 86400,
        points: vec![ExportPoint {
            item_id: 1,
            observed_at: 200 * 86400,
            min_unit_copper: 99,
            median_unit_copper: 99,
            quantity: 3,
        }],
        daily: vec![],
    }
}
#[test]
fn surgical_preservation_overlap_and_repeat() {
    let input=br#"Unknown={flag=false,[1]=true,["1"]=false,s="a\085\n"}
AuctionDBForeverSaved={settings={recordScans=true,yes=true,no=false},trust={x=true},houses={Alliance={items={[1]="17280000,1,2,4"},daily={}},Horde={items={[2]="17280000,3,4,5"},daily={}}}}"#;
    let output = merge_history(input, &bundle(), "Realm", "Alliance").unwrap();
    assert!(output.starts_with(br#"Unknown={flag=false,[1]=true,["1"]=false,s="a\085\n"}"#));
    let text = std::str::from_utf8(&output).unwrap();
    assert!(text.contains("settings={recordScans=true,yes=true,no=false},trust={x=true}"));
    assert!(text.contains("Horde={items={[2]=\"17280000,3,4,5\"},daily={}}"));
    assert!(text.contains("17280000,1,2,4"));
    assert_eq!(
        merge_history(&output, &bundle(), "Realm", "Alliance").unwrap(),
        output
    );
}
#[test]
fn retention_rollup_and_binding() {
    let input=br#"AuctionDBForeverSaved={settings={recordScans=true},houses={Alliance={items={[1]="86400,1,2,4 16416000,5,8,10 16416001,3,6,20"},daily={}}}}"#;
    let output = merge_history(input, &bundle(), "Realm", "Alliance").unwrap();
    let decoded = parse(&output).unwrap();
    assert_eq!(decoded.history.len(), 1);
    assert_eq!(decoded.daily.len(), 1);
    assert_eq!(decoded.daily[0].min_unit_copper, Some(3));
    assert_eq!(decoded.daily[0].median_unit_copper, Some(6));
    assert_eq!(decoded.daily[0].average_quantity, 15);
    assert_eq!(
        merge_history(&output, &bundle(), "Realm", "Alliance").unwrap(),
        output
    );
    assert!(merge_history(input, &bundle(), "Other", "Alliance").is_err());
    let mut bad = bundle();
    bad.points[0].min_unit_copper = 9_007_199_254_740_992;
    assert!(merge_history(input, &bad, "Realm", "Alliance").is_err());
}

#[test]
fn house_scan_cutoff_uses_whole_days_and_preserves_scans() {
    let scans = (0..30)
        .map(|i| format!("{{{},1}}", 199 * 86400 + i))
        .collect::<Vec<_>>()
        .join(",");
    let text=format!("AuctionDBForeverSaved={{settings={{recordScans=true}},houses={{Alliance={{scans={{{}}},items={{[1]=\"17193600,1,2,3 17279999,2,3,4\"}},daily={{}}}}}}}}",scans);
    let output = merge_history(text.as_bytes(), &bundle(), "Realm", "Alliance").unwrap();
    assert!(std::str::from_utf8(&output)
        .unwrap()
        .contains(&format!("scans={{{}}}", scans)));
    // Day 199 stays intact even though the first scan is earlier than the 30-scan cutoff time.
    assert_eq!(parse(&output).unwrap().history.len(), 3);
}

#[test]
fn durable_marker_exposes_only_selected_market_and_keeps_full_scan_bytes() {
    let input = include_bytes!("../../../fixtures/synthetic-v6.lua");
    let before = parse(input).unwrap();
    let mut b = bundle();
    b.market.realm = "Synthetic Realm".into();
    b.generated_at = 1793101000;
    let output = merge_history(input, &b, "Synthetic Realm", "Alliance").unwrap();
    let after = parse(&output).unwrap();
    assert_eq!(after.imported_history_markets, vec![b.market.clone()]);
    assert_eq!(after.scans[0].fingerprint, before.scans[0].fingerprint);
    let original = std::str::from_utf8(input).unwrap();
    let modified = std::str::from_utf8(&output).unwrap();
    let scan = original
        .split(" ah = ")
        .nth(1)
        .unwrap()
        .split(" itemDB_2")
        .next()
        .unwrap();
    assert!(modified.contains(scan));
    assert!(modified.contains("settings = { recordScans = true }"));
    assert!(!after.history.is_empty()); // Consumers can identify and exclude marked history.
    assert_eq!(
        merge_history(&output, &b, "Synthetic Realm", "Alliance").unwrap(),
        output
    );
    // Simulate addon serialization/compaction. The durable marker still identifies derived daily data.
    let rewritten=br#"AuctionDBForeverSaved={settings={recordScans=true},nauctionsSync={schema_version=1,realm="Synthetic Realm",house="Alliance"},houses={Alliance={items={},daily={[123]="1793100060,1000,1000,11,2"}}}}"#;
    let p = parse(rewritten).unwrap();
    assert_eq!(p.imported_history_markets, vec![b.market]);
    assert_eq!(p.daily.len(), 1);
}
#[test]
fn settings_and_marker_fail_closed() {
    for settings in [
        "{}",
        "{recordScans=false}",
        "{recordScans=1}",
        "{recordScans=\"true\"}",
    ] {
        let input = format!(
            "AuctionDBForeverSaved={{settings={},houses={{Alliance={{items={{}},daily={{}}}}}}}}",
            settings
        );
        let error = merge_history(input.as_bytes(), &bundle(), "Realm", "Alliance").unwrap_err();
        assert!(error.to_string().contains("Enable Record full scans"));
    }
    for marker in [
        "{schema_version=2,realm=\"Realm\",house=\"Alliance\"}",
        "{schema_version=1,realm=\"Other\",house=\"Alliance\"}",
        "{schema_version=1,realm=\"Realm\",house=\"Horde\"}",
    ] {
        let input=format!("AuctionDBForeverSaved={{settings={{recordScans=true}},nauctionsSync={},houses={{Alliance={{items={{}},daily={{}}}},Horde={{items={{}},daily={{}}}}}}}}",marker);
        assert!(merge_history(input.as_bytes(), &bundle(), "Realm", "Alliance").is_err());
    }
}

#[test]
fn initializes_missing_house_and_history_fields_without_touching_other_fields() {
    for houses in [
        "{}",
        "{Horde={items={},daily={},other=1.25}}",
        "{Alliance={custom=-3.5e-2}}",
    ] {
        let input = format!(
            "AuctionDBForeverSaved={{settings={{recordScans=true,scale=1.25}},houses={}}}",
            houses
        );
        let output = merge_history(input.as_bytes(), &bundle(), "Realm", "Alliance").unwrap();
        let p = parse(&output).unwrap();
        assert_eq!(p.history.len(), 1);
        let text = std::str::from_utf8(&output).unwrap();
        assert!(text.contains("settings={recordScans=true,scale=1.25}"));
        if houses.contains("Horde") {
            assert!(text.contains("Horde={items={},daily={},other=1.25}"));
        }
        if houses.contains("custom") {
            assert!(text.contains("custom=-3.5e-2"));
        }
        assert_eq!(
            merge_history(&output, &bundle(), "Realm", "Alliance").unwrap(),
            output
        );
    }
}
#[test]
fn preserves_local_history_key_types_and_rejects_ambiguous_item_keys() {
    let input=br#"AuctionDBForeverSaved={settings={recordScans=true},houses={Alliance={items={["1"]="17280000,1,2,4"},daily={}}}}"#;
    let output = merge_history(input, &bundle(), "Realm", "Alliance").unwrap();
    assert!(std::str::from_utf8(&output)
        .unwrap()
        .contains("[\"1\"]=\"17280000,1,2,4\""));
    let ambiguous=br#"AuctionDBForeverSaved={settings={recordScans=true},houses={Alliance={items={[1]="17280000,1,2,4",["1"]="17280000,9,9,9"},daily={}}}}"#;
    assert!(merge_history(ambiguous, &bundle(), "Realm", "Alliance")
        .unwrap_err()
        .to_string()
        .contains("ambiguous"));
}
