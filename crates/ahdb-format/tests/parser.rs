use ahdb_format::*;
const SAMPLE: &[u8] = include_bytes!("../../../fixtures/synthetic-v6.lua");
#[test]
fn synthetic_v6() {
    let p = parse(SAMPLE).unwrap();
    assert_eq!(p.scans.len(), 1);
    let s = &p.scans[0];
    assert_eq!(s.listings.len(), 3);
    assert_eq!(s.quality, market_domain::ScanQuality::Unknown);
    assert_eq!(s.listings[1].buyout_total_copper, None);
    assert_eq!(s.listings[2].buyout_total_copper, Some(10_000_000_000));
    assert_eq!(s.listings[0].unit_buyout().unwrap().rounded_copper(), 1000);
    assert_eq!(p.history.len(), 2);
    assert!(p.history[1].median_unit_copper.is_none());
    assert_eq!(p.daily[0].scan_count, 3);
    assert_ne!(s.listings[0].item_key, s.listings[2].item_key);
    assert_eq!(p.items[1].base_item_id, 123);
    assert!(p.history[0].market.realm.is_empty());
}
#[test]
fn canonical_order_and_content() {
    let p = parse(SAMPLE).unwrap();
    let mut s = p.scans[0].clone();
    let fp = s.fingerprint.clone();
    s.listings.reverse();
    assert_eq!(fp, s.canonical_fingerprint());
    s.listings[0].stack_count += 1;
    assert_ne!(fp, s.canonical_fingerprint());
}
#[test]
fn reject_executable_grammar() {
    for s in [
        "x=os.execute('evil')",
        "x=(function() end)()",
        "x={a=1+2}",
        "x={a=setmetatable({},{})}",
        "x={}; os.execute('evil')",
        "x={['a']=1,a=2}",
        "x={a='bad\\q'}",
    ] {
        assert!(parse(s.as_bytes()).is_err(), "{s}")
    }
}
#[test]
fn enforce_bounds() {
    let l = Limits {
        max_bytes: 2,
        ..Limits::default()
    };
    assert!(parse_with_limits(SAMPLE, l).is_err());
    let l = Limits {
        max_depth: 2,
        ..Limits::default()
    };
    assert!(parse_with_limits(b"x={{{{}}}}", l).is_err());
    let l = Limits {
        max_nodes: 2,
        ..Limits::default()
    };
    assert!(parse_with_limits(b"x={1,2,3}", l).is_err());
    let l = Limits {
        max_string_bytes: 2,
        ..Limits::default()
    };
    assert!(parse_with_limits(b"x={'long'}", l).is_err());
    let l = Limits {
        max_listings: 1,
        ..Limits::default()
    };
    assert!(parse_with_limits(SAMPLE, l).is_err());
}
#[test]
fn versions_and_invalid_values() {
    let text = String::from_utf8(SAMPLE.to_vec()).unwrap();
    for s in [
        text.replace("dataFormatVersion=6", "dataFormatVersion=7"),
        text.replace("2,20,100", "2,0,100"),
        text.replace("20000,", "18446744073709551616,"),
        text.replace("20000,", "-1,"),
        text.replace("20000,", "1e6,"),
    ] {
        assert!(parse(s.as_bytes()).is_err())
    }
    let partial = parse(
        text.replace("count=3, itemsCount", "count=4, itemsCount")
            .as_bytes(),
    )
    .unwrap();
    assert_eq!(
        partial.scans[0].quality,
        market_domain::ScanQuality::Partial
    );
}
#[test]
fn truncated_inputs_never_panic() {
    for n in 0..SAMPLE.len() {
        let _ = parse(&SAMPLE[..n]);
    }
}
#[test]
fn compact_variants_and_unknown_roots() {
    assert_eq!(expand_item_key("i123;42").unwrap(), "item:123::42");
    assert_ne!(
        expand_item_key("i123;42").unwrap(),
        expand_item_key("i123;43").unwrap()
    );
    assert!(expand_item_key("i123<script>").is_err());
    assert!(parse(b"arbitrary = { ah = {} }").is_err());
    assert!(parse(b"AuctionDBForeverSaved = {}").is_err());
}
#[test]
fn random_bytes_do_not_panic() {
    let mut state = 0x92f154u64;
    for n in 0..512 {
        let bytes: Vec<_> = (0..n)
            .map(|_| {
                state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
                (state >> 32) as u8
            })
            .collect();
        let _ = parse(&bytes);
    }
}
#[test]
fn limits_apply_across_scans_and_history() {
    let limits = Limits {
        max_listings: 4,
        ..Limits::default()
    };
    assert!(parse_with_limits(SAMPLE, limits).is_err());
    let text = String::from_utf8(SAMPLE.to_vec()).unwrap();
    assert!(parse(text.replace("20000,", "9223372036854775808,").as_bytes()).is_err());
}

#[test]
fn savedvariables_escapes() {
    let input=br#"AuctionDBForeverSaved={itemDB_2={_formatVersion_=5,_locale_="en\085\083\n\r\t\a\b\f\v\"\'\\",["i1"]="|Hitem:1|h[Item]|h"}}"#;
    let p = parse(input).unwrap();
    assert_eq!(
        p.items[0].locale.as_deref(),
        Some("enUS\n\r\t\x07\x08\x0c\x0b\"'\\")
    );
    for suffix in ["\n", "\r\n"] {
        let input=format!("AuctionDBForeverSaved={{itemDB_2={{_formatVersion_=5,_locale_=\"en\\{suffix}US\",[\"i1\"]=\"|Hitem:1|h[Item]|h\"}}}}");
        assert_eq!(
            parse(input.as_bytes()).unwrap().items[0].locale.as_deref(),
            Some("en\nUS")
        );
    }
    assert!(parse(br#"AuctionDBForeverSaved={ah={},note="\256"}"#).is_err());
    assert!(parse(br#"AuctionDBForeverSaved={ah={},note="\255"}"#).is_err());
}
#[test]
fn deep_limits_and_key_limits_are_specific() {
    let input = format!(
        "AuctionDBForeverSaved={{ah={{}},ignored={}0{}}}",
        "{".repeat(200),
        "}".repeat(200)
    );
    assert!(parse(input.as_bytes())
        .unwrap_err()
        .0
        .contains("structure limit"));
    let generous = Limits {
        max_depth: 10_000,
        ..Limits::default()
    };
    assert!(parse_with_limits(input.as_bytes(), generous)
        .unwrap_err()
        .0
        .contains("structure limit"));
    let key = "i".to_owned() + &"1".repeat(1024);
    assert!(expand_item_key(&key).is_err());
    let input = format!("AuctionDBForeverSaved={{ah={{}},{}=true}}", "a".repeat(100));
    let limits = Limits {
        max_string_bytes: 40,
        ..Limits::default()
    };
    assert!(parse_with_limits(input.as_bytes(), limits)
        .unwrap_err()
        .0
        .contains("identifier limit"));
    for number in ["NaN", "1e999", "0/0", "1..2", "0x1", "1e", "--1"] {
        let input = format!("AuctionDBForeverSaved={{ah={{}},number={number}}}");
        assert!(parse(input.as_bytes()).is_err(), "{number}");
    }
}
#[test]
fn fingerprints_keep_multiplicity_market_time_and_bid_only_content() {
    let scan = parse(SAMPLE).unwrap().scans.remove(0);
    let identity = scan.canonical_fingerprint();
    let mut changed = scan.clone();
    changed.listings.push(scan.listings[0].clone());
    assert_ne!(identity, changed.canonical_fingerprint());
    let mut changed = scan.clone();
    changed.market.house = "Neutral".into();
    assert_ne!(identity, changed.canonical_fingerprint());
    let mut changed = scan.clone();
    changed.observed_at += 1;
    assert_ne!(identity, changed.canonical_fingerprint());
    let mut changed = scan.clone();
    changed.listings[1].min_bid_copper += 1;
    assert_ne!(identity, changed.canonical_fingerprint());
}
