//! Read a synthetic JSON fixture without importing it into a real market.
use market_analysis::strategy::{analyze, Sample};
use serde::Deserialize;

#[derive(Deserialize)]
struct Dataset {
    schema_version: u32,
    synthetic: bool,
    as_of: i64,
    items: Vec<Item>,
}
#[derive(Deserialize)]
struct Item {
    item_key: String,
    scenario: String,
    samples: Vec<Sample>,
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args()
        .nth(1)
        .ok_or("Usage: strategy-inspect dataset.json")?;
    let bytes = std::fs::read(path)?;
    let data: Dataset = serde_json::from_slice(&bytes)?;
    if data.schema_version != 1 || !data.synthetic {
        return Err("Expected synthetic schema_version=1".into());
    }
    let mut results = Vec::new();
    for item in data.items {
        if item
            .samples
            .iter()
            .any(|s| s.at > data.as_of || s.price.numerator() == 0)
        {
            return Err(format!(
                "Invalid future observation or zero price: {}",
                item.item_key
            )
            .into());
        }
        let current = item.samples.iter().max_by_key(|s| s.at);
        let fresh = current.is_some_and(|s| data.as_of - s.at <= 45 * 60);
        results.push(serde_json::json!({
            "item_key": item.item_key, "scenario": item.scenario,
            "patterns": analyze(&item.samples, data.as_of, current.map(|s| s.price), fresh)
        }));
    }
    println!("{}", serde_json::to_string_pretty(&results)?);
    Ok(())
}
