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
AuctionDBForeverSaved={settings={recordScans=true,yes=true,no=false},trust={x=true},houses={Alliance={items={[1]="17280000,1,2,4"},daily={},scans={}},Horde={items={[2]="17280000,3,4,5"},daily={},scans={}}}}"#;
    let output = merge_history(input, &bundle(), "Realm", "Alliance").unwrap();
    assert!(output.starts_with(br#"Unknown={flag=false,[1]=true,["1"]=false,s="a\085\n"}"#));
    let text = std::str::from_utf8(&output).unwrap();
    assert!(text.contains("settings={recordScans=true,yes=true,no=false},trust={x=true}"));
    assert!(text.contains("Horde={items={[2]=\"17280000,3,4,5\"},daily={},scans={}}"));
    assert!(text.contains("17280000,1,2,4"));
    assert_eq!(
        merge_history(&output, &bundle(), "Realm", "Alliance").unwrap(),
        output
    );
}
#[test]
fn retention_rollup_and_binding() {
    let input=br#"AuctionDBForeverSaved={settings={recordScans=true},houses={Alliance={items={[1]="86400,1,2,4 16416000,5,8,10 16416001,3,6,20"},daily={},scans={}}}}"#;
    let output = merge_history(input, &bundle(), "Realm", "Alliance").unwrap();
    let decoded = parse(&output).unwrap();
    assert_eq!(decoded.history.len(), 1);
    assert_eq!(decoded.daily.len(), 2); // Sync must retain old local data.
    let recent_rollup = decoded
        .daily
        .iter()
        .find(|p| p.average_quantity == 15)
        .unwrap();
    assert_eq!(recent_rollup.min_unit_copper, Some(3));
    assert_eq!(recent_rollup.median_unit_copper, Some(6));
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
        .map(|i| format!("{{{},1,1}}", 199 * 86400 + i))
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
            "AuctionDBForeverSaved={{settings={},houses={{Alliance={{items={{}},daily={{}},scans={{}}}}}}}}",
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
        let input=format!("AuctionDBForeverSaved={{settings={{recordScans=true}},nauctionsSync={},houses={{Alliance={{items={{}},daily={{}},scans={{}}}},Horde={{items={{}},daily={{}},scans={{}}}}}}}}",marker);
        assert!(merge_history(input.as_bytes(), &bundle(), "Realm", "Alliance").is_err());
    }
}

#[test]
fn refuses_missing_house_and_history_fields_without_repairs() {
    for houses in [
        "{}",
        "{Horde={items={},daily={},scans={}}}",
        "{Alliance={custom=1}}",
        "{Alliance={items={},daily={}}}",
    ] {
        let input =
            format!("AuctionDBForeverSaved={{settings={{recordScans=true}},houses={houses}}}");
        parse(input.as_bytes()).unwrap(); // Readable legacy data is not necessarily writable.
        assert!(merge_history(input.as_bytes(), &bundle(), "Realm", "Alliance").is_err());
    }
}
#[test]
fn refuses_string_only_and_ambiguous_item_keys() {
    let input=br#"AuctionDBForeverSaved={settings={recordScans=true},houses={Alliance={items={["1"]="17280000,1,2,4"},daily={},scans={}}}}"#;
    assert!(merge_history(input, &bundle(), "Realm", "Alliance").is_err());
    let ambiguous=br#"AuctionDBForeverSaved={settings={recordScans=true},houses={Alliance={items={[1]="17280000,1,2,4",["1"]="17280000,9,9,9"},daily={},scans={}}}}"#;
    assert!(merge_history(ambiguous, &bundle(), "Realm", "Alliance")
        .unwrap_err()
        .to_string()
        .contains("numeric item keys"));
}

#[test]
fn writable_schema_rejects_sparse_scans_legacy_roots_and_multiple_roots() {
    for scans in [
        "{[2]={17279999,1,1}}",
        "{[\"1\"]={17279999,1,1}}",
        "{{17279999,1}}",
        "{{17279999,1,2}}",
        "{{17279999,1,1},{17279998,1,1}}",
        "{{9007199254740992,1,1}}",
    ] {
        let input = format!("AuctionDBForeverSaved={{settings={{recordScans=true}},houses={{Alliance={{items={{}},daily={{}},scans={scans}}}}}}}");
        assert!(
            merge_history(input.as_bytes(), &bundle(), "Realm", "Alliance").is_err(),
            "{scans}"
        );
    }
    let input = b"AuctionDBForeverSaved={settings={recordScans=true},houses={Alliance={items={},daily={},scans={}}}}";
    let legacy = String::from_utf8(input.to_vec())
        .unwrap()
        .replace("AuctionDBForeverSaved", "AuctionDBSaved");
    parse(legacy.as_bytes()).unwrap();
    assert!(merge_history(legacy.as_bytes(), &bundle(), "Realm", "Alliance").is_err());
    let multiple = format!("{}\n{legacy}", String::from_utf8_lossy(input));
    assert!(merge_history(multiple.as_bytes(), &bundle(), "Realm", "Alliance").is_err());
}

#[test]
fn export_duplicates_are_order_independent_or_explicitly_rejected() {
    let input = b"AuctionDBForeverSaved={settings={recordScans=true},houses={Alliance={items={},daily={},scans={}}}}";
    let mut export = bundle();
    let expected = merge_history(input, &export, "Realm", "Alliance").unwrap();
    export.points.push(export.points[0].clone());
    assert_eq!(
        merge_history(input, &export, "Realm", "Alliance").unwrap(),
        expected
    );
    export.points[1].quantity += 1;
    for _ in 0..2 {
        assert!(merge_history(input, &export, "Realm", "Alliance")
            .unwrap_err()
            .to_string()
            .contains("conflicting export"));
        export.points.reverse();
    }
}

#[test]
fn daily_overlap_never_silently_drops_an_alternative_or_adds_counts() {
    let input = b"AuctionDBForeverSaved={settings={recordScans=true},houses={Alliance={items={},daily={[1]=\"16416001,3,6,20,2\"},scans={}}}}";
    let mut export = bundle();
    export.daily.push(ExportDaily {
        item_id: 1,
        observed_at: 16416001,
        min_unit_copper: 3,
        median_unit_copper: 6,
        quantity: 20,
        scan_count: 2,
    });
    let output = merge_history(input, &export, "Realm", "Alliance").unwrap();
    assert_eq!(parse(&output).unwrap().daily[0].scan_count, 2);
    export.daily[0].quantity += 1;
    assert!(merge_history(input, &export, "Realm", "Alliance")
        .unwrap_err()
        .to_string()
        .contains("daily coverage conflict"));
    export.daily.push(export.daily[0].clone());
    export.daily[1].scan_count += 1;
    for _ in 0..2 {
        assert!(merge_history(input, &export, "Realm", "Alliance")
            .unwrap_err()
            .to_string()
            .contains("daily coverage conflict"));
        export.daily.reverse();
    }
}

#[test]
fn local_conflicts_and_raw_daily_overlap_block_merge() {
    for history in [
        "items={[1]=\"17280000,1,2,3 17280000,2,3,4\"},daily={}",
        "items={[1]=\"17280000,1,2,3\"},daily={[1]=\"17280000,1,2,3,1\"}",
    ] {
        let input = format!("AuctionDBForeverSaved={{settings={{recordScans=true}},houses={{Alliance={{{history},scans={{}}}}}}}}");
        assert!(merge_history(input.as_bytes(), &bundle(), "Realm", "Alliance").is_err());
    }
}
