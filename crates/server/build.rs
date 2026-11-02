use std::{env, fs, path::PathBuf};

fn main() {
    let directory = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap())
        .join("../../assets/item-icon-metadata");
    println!("cargo:rerun-if-changed={}", directory.display());
    let mut files = fs::read_dir(&directory)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .collect::<Vec<_>>();
    files.sort();
    let mut source = String::from("&[\n");
    for path in files {
        let id: u32 = path.file_stem().unwrap().to_str().unwrap().parse().unwrap();
        source.push_str(&format!(
            "({id}, include_str!({:?})),\n",
            path.canonicalize().unwrap().to_str().unwrap()
        ));
    }
    source.push(']');
    fs::write(
        PathBuf::from(env::var("OUT_DIR").unwrap()).join("item_metadata.rs"),
        source,
    )
    .unwrap();
}
