use ahdb_format::parse;
const SAMPLE: &[u8] = include_bytes!("../../../fixtures/synthetic-professions.lua");
#[test]
fn newer_forever_builds_are_recorded_without_an_exact_build_allowlist() {
    let source = String::from_utf8_lossy(SAMPLE);
    for build in ["1.60.1.70685", "1.60.1.71000", "1.60.2.71001"] {
        let parsed = parse(source.replace("1.60.1.70630", build).as_bytes()).unwrap();
        let export = parsed.professions.unwrap();
        assert_eq!(export.build, build);
        assert_eq!(export.recipes.len(), 2);
    }
}
#[test]
fn auction_discoveries_keep_unresolved_items_without_invented_recipe_facts() {
    let source = include_str!("../../../fixtures/synthetic-auction-recipes.lua");
    let export = parse(source.as_bytes()).unwrap().professions.unwrap();
    assert_eq!(export.recipes.len(), 1);
    assert_eq!(export.recipe_items.len(), 2);
    assert_eq!(export.recipe_items[0].recipe_spell_id, Some(900003));
    assert_eq!(export.recipe_items[1].status, "details_unavailable");
    for (from, to) in [
        ("recipe_spell_id=900003", "recipe_spell_id=999999"),
        ("status=\"resolved\"", "status=\"learned\""),
        (
            "status=\"resolved\"",
            "status=\"resolved\", character=\"Alice\"",
        ),
        ("[910001]", "[\"910001\"]"),
        ("recipe_spell_id=900004,", ""),
    ] {
        assert!(
            parse(source.replace(from, to).as_bytes()).is_err(),
            "accepted {to}"
        );
    }
}
#[test]
fn combined_file_and_profession_only_file_preserve_shared_facts() {
    assert!(parse(
        &[
            include_bytes!("../../../fixtures/synthetic-v6.lua").as_slice(),
            b"\nNauctionsProfessionsSaved=nil"
        ]
        .concat()
    )
    .is_ok());
    let parsed = parse(SAMPLE).unwrap();
    assert!(parsed.scans.is_empty());
    let export = parsed.professions.unwrap();
    assert_eq!(export.build_number, 70630);
    assert_eq!(export.recipes.len(), 2);
    assert_eq!(export.recipes[0].min_output, Some(2));
    assert_eq!(export.recipes[0].reagents[&900201], 3);
    assert_eq!(export.recipes[1].kind, "service");
    let only =
        String::from_utf8_lossy(SAMPLE).replace("AuctionDBForeverSaved = { houses = {} }", "");
    assert!(parse(only.as_bytes()).is_ok());
    let combined = [
        include_bytes!("../../../fixtures/synthetic-v6.lua").as_slice(),
        only.as_bytes(),
    ]
    .concat();
    let result = parse(&combined).unwrap();
    assert_eq!(result.scans.len(), 1);
    assert_eq!(result.professions.unwrap().recipes.len(), 2);
}
#[test]
fn reject_invalid_fields_builds_quantities_and_executable_data() {
    let input = String::from_utf8_lossy(SAMPLE);
    for (from, to) in [
        ("schema_version = 1", "schema_version = 2"),
        ("wow_classic_beta", "wow"),
        ("1.60.1.70630", "1.15.9.70630"),
        ("min_output = 2", "min_output = 3"),
        ("[900201] = 3", "[900201] = 0"),
        ("kind = \"service\"", "kind = \"other\""),
        ("tools = {}", "tools = {}, learned = true"),
        (
            "locale = \"enUS\"",
            "locale = \"enUS\", character = \"Alice\"",
        ),
        ("[900201] = 3", "[900201] = os.time()"),
    ] {
        assert!(
            parse(input.replace(from, to).as_bytes()).is_err(),
            "accepted {to}"
        );
    }
}
#[test]
fn recipe_identity_excludes_names_and_time_but_includes_quantities_and_build() {
    let export = parse(SAMPLE).unwrap().professions.unwrap();
    let mut recipe = export.recipes[0].clone();
    let id = recipe.fingerprint(&export.product, &export.build);
    recipe.name = "Localized name".into();
    recipe.captured_at += 100;
    assert_eq!(id, recipe.fingerprint(&export.product, &export.build));
    recipe.reagents.insert(900201, 4);
    assert_ne!(id, recipe.fingerprint(&export.product, &export.build));
    assert_ne!(
        id,
        export.recipes[0].fingerprint(&export.product, "1.60.1.70658")
    );
}
