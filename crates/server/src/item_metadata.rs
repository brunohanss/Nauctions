//! On-demand Classic descriptions. The browser renders this markup as text only.
use crate::{
    api::{ApiError, Result},
    auth, App,
};
use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    sync::OnceLock,
    time::{Duration, Instant},
};
use tokio::sync::Mutex;

#[derive(Clone, Deserialize, Serialize)]
pub struct Metadata {
    name: String,
    quality: u8,
    tooltip: String,
}

// The same bundled Classic tooltips are used by the browser and merchant fallback.
pub(crate) fn merchant_price(key: &str, metadata: Option<&serde_json::Value>) -> Option<u64> {
    if let Some(price) = metadata.and_then(|m| {
        m["sell_price"]
            .as_u64()
            .or_else(|| m["tooltip_sell_price"].as_u64())
    }) {
        return Some(price);
    }
    let id = key.strip_prefix('i')?.parse::<u32>().ok()?;
    if key != format!("i{id}") {
        return None;
    }
    static PRICES: OnceLock<BTreeMap<u32, u64>> = OnceLock::new();
    PRICES
        .get_or_init(|| {
            let bundled: &[(u32, &str)] = include!(concat!(env!("OUT_DIR"), "/item_metadata.rs"));
            bundled
                .iter()
                .filter_map(|(id, json)| {
                    let metadata: Metadata =
                        serde_json::from_str(json).expect("valid bundled item metadata");
                    tooltip_sell_price(&metadata.tooltip).map(|price| (*id, price))
                })
                .collect()
        })
        .get(&id)
        .copied()
}

// Read only the sell-price block. Other currency values in a tooltip are not prices.
fn tooltip_sell_price(html: &str) -> Option<u64> {
    let start = html.find("whtt-sellprice")?;
    let block = html[start..].split_once('>')?.1.split_once("</div>")?.0;
    let mut price = 0u64;
    let mut seen = 0u8;
    for span in block.split("<span").skip(1) {
        let (attributes, text) = span.split_once('>')?;
        let class = attributes.split_once("class=")?.1;
        let quote = class.chars().next()?;
        if quote != '\'' && quote != '"' {
            return None;
        }
        let class = class[1..].split(quote).next()?;
        let (factor, bit) = match class {
            "moneygold" => (10000u64, 1u8),
            "moneysilver" => (100, 2),
            "moneycopper" => (1, 4),
            _ => continue,
        };
        if seen & bit != 0 {
            return None;
        }
        seen |= bit;
        let amount = text
            .split_once('<')?
            .0
            .trim()
            .replace(',', "")
            .parse::<u64>()
            .ok()?;
        price = price.checked_add(amount.checked_mul(factor)?)?;
    }
    (seen != 0).then_some(price)
}

async fn remember_price(app: &App, id: u32, metadata: &Metadata) -> Result<()> {
    if let Some(price) = tooltip_sell_price(&metadata.tooltip) {
        sqlx::query("UPDATE items SET metadata=jsonb_set(metadata,'{tooltip_sell_price}',$2) WHERE item_key=$1")
            .bind(format!("i{id}")).bind(serde_json::json!(price)).execute(&app.db).await?;
    }
    Ok(())
}

pub async fn get(
    State(app): State<App>,
    headers: HeaderMap,
    Path(id): Path<u32>,
) -> Result<Json<Metadata>> {
    let member = auth::member(&app, &headers, false).await?;
    if id == 0 {
        return Err(ApiError(StatusCode::BAD_REQUEST, "invalid_item_id"));
    }
    type Cache = BTreeMap<u32, (Instant, Option<Metadata>)>;
    static CACHE: OnceLock<Mutex<Cache>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(BTreeMap::new()));
    let cached = {
        let cache = cache.lock().await;
        cache
            .get(&id)
            .filter(|(time, value)| {
                time.elapsed() < Duration::from_secs(if value.is_some() { 86400 } else { 300 })
            })
            .map(|(_, value)| value.clone())
    };
    if let Some(value) = cached {
        let metadata = value.ok_or(ApiError(
            StatusCode::BAD_GATEWAY,
            "item_description_unavailable",
        ))?;
        remember_price(&app, id, &metadata).await?;
        return Ok(Json(metadata));
    }
    auth::rate(&app, &format!("item-metadata:{}", member.id), 60).await?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(8))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| ApiError(StatusCode::BAD_GATEWAY, "item_description_unavailable"))?;
    let value = async {
        let mut response = client
            .get(format!(
                "https://nether.wowhead.com/classic/tooltip/item/{id}"
            ))
            .send()
            .await
            .ok()?
            .error_for_status()
            .ok()?;
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.ok()? {
            if bytes.len() + chunk.len() > 128 * 1024 {
                return None;
            }
            bytes.extend_from_slice(&chunk);
        }
        let data: Metadata = serde_json::from_slice(&bytes).ok()?;
        if data.name.is_empty() || data.tooltip.is_empty() || data.quality > 7 {
            return None;
        }
        Some(data)
    }
    .await;
    let mut cache = cache.lock().await;
    if cache.len() >= 512 {
        cache.clear();
    }
    cache.insert(id, (Instant::now(), value.clone()));
    drop(cache);
    if let Some(metadata) = &value {
        remember_price(&app, id, metadata).await?;
    }
    value.map(Json).ok_or(ApiError(
        StatusCode::BAD_GATEWAY,
        "item_description_unavailable",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn sell_price_matches_bundled_tooltip_and_preserves_scan_overrides() {
        assert_eq!(merchant_price("i4238", None), Some(200));
        assert_eq!(
            merchant_price("i4238", Some(&json!({"sell_price": 900}))),
            Some(900)
        );
        assert_eq!(
            merchant_price("i4238", Some(&json!({"sell_price": 0}))),
            Some(0)
        );
        assert_eq!(
            merchant_price("i900001", Some(&json!({"tooltip_sell_price": 250}))),
            Some(250)
        );
        assert_eq!(merchant_price("i4238:5", None), None);
        assert_eq!(merchant_price("i99999999", None), None);
    }

    #[test]
    fn parses_only_sell_price_denominations_and_rejects_bad_values() {
        assert_eq!(tooltip_sell_price("<div class='whtt-sellprice'>Sell Price: <span class='moneygold'>2</span> <span class='moneysilver'>3</span> <span class='moneycopper'>4</span></div>"), Some(20304));
        assert_eq!(
            tooltip_sell_price("<div><span class='moneygold'>99</span></div>"),
            None
        );
        assert_eq!(
            tooltip_sell_price(
                "<div class='whtt-sellprice'><span class='moneycopper'>0</span></div>"
            ),
            Some(0)
        );
        assert_eq!(
            tooltip_sell_price(
                "<div class='whtt-sellprice'><span class='moneycopper'>-3</span></div>"
            ),
            None
        );
        assert_eq!(tooltip_sell_price("<div class='whtt-sellprice'><span class='moneygold'>18446744073709551615</span></div>"), None);
    }
}
