fn main() {
    if let Err(e) = run() {
        eprintln!("{e}");
        std::process::exit(1)
    }
}
fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 3 || args[1] != "--input" {
        return Err("usage: ahdb-inspect --input <AuctionDB.lua>".into());
    }
    let file = std::fs::File::open(&args[2])?;
    use std::io::Read;
    let mut bytes = Vec::new();
    file.take(ahdb_format::Limits::default().max_bytes as u64 + 1)
        .read_to_end(&mut bytes)?;
    println!(
        "{}",
        serde_json::to_string_pretty(&ahdb_format::parse(&bytes)?)?
    );
    Ok(())
}
