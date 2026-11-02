fn main() {
    // Windows embeds these assets in the executable. Rebuild when either changes.
    println!("cargo:rerun-if-changed=icons/icon.ico");
    println!("cargo:rerun-if-changed=icons/icon.png");
    let addon = std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap())
        .join("../addon/AuctionDB")
        .canonicalize()
        .unwrap();
    println!("cargo:rerun-if-changed={}", addon.display());
    let toc = std::fs::read_to_string(addon.join("AuctionDB.toc")).unwrap();
    let mut names = vec!["AuctionDB.toc", "LICENSE", "README.md", "UPSTREAM.json"];
    names.extend(
        toc.lines()
            .map(str::trim)
            .filter(|line| !line.is_empty() && !line.starts_with('#')),
    );
    let mut source = String::from("static ADDON: &[(&str, &[u8])] = &[\n");
    for name in names {
        assert!(name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b)));
        let path = addon.join(name);
        assert!(path.is_file(), "Missing addon file: {name}");
        source.push_str(&format!(
            "({name:?}, include_bytes!({:?})),\n",
            path.to_str().unwrap()
        ));
    }
    source.push_str("];\n");
    std::fs::write(
        std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("bundled_addon.rs"),
        source,
    )
    .unwrap();
    tauri_build::build();
}
