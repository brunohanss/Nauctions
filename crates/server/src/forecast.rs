//! Bounded, authenticated forecasts from market scan history.
use crate::{
    api::{ApiError, Result},
    auth, read, App,
};
use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use chrono::{DateTime, Utc};
use market_analysis::forecast::{self, Sample};
use market_domain::UnitPrice;
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::Row;
use uuid::Uuid;

#[derive(Deserialize)]
pub struct Scope {
    market_id: Option<Uuid>,
}
#[derive(Deserialize)]
pub struct ListQuery {
    market_id: Option<Uuid>,
    q: Option<String>,
    offset: Option<usize>,
}
fn bad(code: &'static str) -> ApiError {
    ApiError(StatusCode::BAD_REQUEST, code)
}
fn valid_key(key: &str) -> bool {
    !key.is_empty() && key.len() <= 200 && key.chars().all(|c| c.is_ascii_graphic())
}
/// One bounded page of items observed in this market's full scans.
pub async fn list(
    State(a): State<App>,
    h: HeaderMap,
    Query(p): Query<ListQuery>,
) -> Result<Json<Value>> {
    auth::member(&a, &h, false).await?;
    let market = read::scope(&a, p.market_id).await?;
    let q = p.q.unwrap_or_default();
    let offset = p.offset.unwrap_or(0);
    if q.len() > 200 || offset > 100000 {
        return Err(bad("invalid_forecast_query"));
    }
    let now = a.config.read_now().timestamp();
    let rows = sqlx::query("SELECT i.item_key,i.name,count(*) OVER() AS total_count FROM items i WHERE (strpos(lower(i.name),lower($2))>0 OR i.base_item_id::text=$2 OR i.item_key=$2) AND EXISTS(SELECT 1 FROM item_observations o JOIN scans s ON s.id=o.scan_id WHERE o.item_key=i.item_key AND s.market_id=$1 AND s.source_time<=to_timestamp($3) AND s.source_time>=to_timestamp($3)-interval '60 days' AND (s.source_kind='full_scan' OR (s.source_kind='demo' AND $4)) AND ((s.source_kind='demo')=$4)) ORDER BY lower(i.name),i.item_key LIMIT 30 OFFSET $5")
        .bind(market).bind(q).bind(now as f64).bind(a.config.is_demo()).bind(offset as i64)
        .fetch_all(&a.db).await?;
    let total = rows
        .first()
        .map(|r| r.get::<i64, _>("total_count"))
        .unwrap_or(0) as usize;
    let mut items = Vec::new();
    for row in rows {
        let key: String = row.get("item_key");
        items.push(
            json!({"name":row.get::<String,_>("name"),"forecast":build(&a,market,&key).await?}),
        );
    }
    let next = (offset + items.len() < total).then_some(offset + items.len());
    Ok(Json(
        json!({"items":items,"total_count":total,"next_offset":next}),
    ))
}
pub async fn item(
    State(a): State<App>,
    h: HeaderMap,
    Path(key): Path<String>,
    Query(p): Query<Scope>,
) -> Result<Json<Value>> {
    auth::member(&a, &h, false).await?;
    if !valid_key(&key) {
        return Err(bad("invalid_item_key"));
    }
    let market = read::scope(&a, p.market_id).await?;
    Ok(Json(build(&a, market, &key).await?))
}
/// Read forecasts for one visible page. Two bounded queries, independent of item count.
pub async fn batch(
    State(a): State<App>,
    h: HeaderMap,
    Json(p): Json<AddonExport>,
) -> Result<Json<Value>> {
    auth::member(&a, &h, false).await?;
    let market = read::scope(&a, p.market_id).await?;
    if p.item_keys.is_empty() || p.item_keys.len() > 30 || p.item_keys.iter().any(|k| !valid_key(k))
    {
        return Err(bad("forecast_batch_requires_1_to_30_items"));
    }
    let keys: Vec<String> = p
        .item_keys
        .into_iter()
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    let now = a.config.read_now().timestamp();
    let rows = sqlx::query("SELECT o.item_key,o.median_buyout,s.source_time FROM item_observations o JOIN scans s ON s.id=o.scan_id WHERE s.market_id=$1 AND o.item_key=ANY($2) AND s.quality='complete' AND (s.source_kind='full_scan' OR (s.source_kind='demo' AND $4)) AND ((s.source_kind='demo')=$4) AND s.source_time>=to_timestamp($3)-interval '60 days' AND s.source_time<=to_timestamp($3) AND o.median_buyout IS NOT NULL ORDER BY s.source_time,s.received_at,s.id LIMIT 50001")
        .bind(market).bind(&keys).bind(now as f64).bind(a.config.is_demo()).fetch_all(&a.db).await?;
    if rows.len() > 50000 {
        return Err(bad("forecast_history_too_dense"));
    }
    let mut history = std::collections::BTreeMap::<String, Vec<Sample>>::new();
    for row in rows {
        let price: UnitPrice = serde_json::from_value(row.get("median_buyout"))
            .map_err(|_| ApiError(StatusCode::INTERNAL_SERVER_ERROR, "invalid_stored_price"))?;
        history
            .entry(row.get("item_key"))
            .or_default()
            .push(Sample {
                at: row.get::<DateTime<Utc>, _>("source_time").timestamp(),
                price,
            });
    }
    // All scan kinds participate in recency: a newer partial observation suppresses eligibility.
    // A newer complete scan that omits an item establishes absence.
    let anchors = sqlx::query(r#"
        WITH complete AS (
            SELECT source_time,received_at,id FROM scans WHERE market_id=$1
            AND source_time<=to_timestamp($3) AND quality='complete' AND ((source_kind='demo')=$4)
            ORDER BY source_time DESC,received_at DESC,id DESC LIMIT 1
        ), latest AS (
            SELECT DISTINCT ON(o.item_key) o.item_key,o.median_buyout,s.source_time,s.received_at,s.id,s.quality,s.source_kind
            FROM item_observations o JOIN scans s ON s.id=o.scan_id
            WHERE s.market_id=$1 AND o.item_key=ANY($2) AND s.source_time<=to_timestamp($3) AND ((s.source_kind='demo')=$4)
            ORDER BY o.item_key,s.source_time DESC,s.received_at DESC,s.id DESC
        ) SELECT l.item_key,l.median_buyout FROM latest l LEFT JOIN complete c ON true
        WHERE l.quality='complete' AND (l.source_kind='full_scan' OR (l.source_kind='demo' AND $4))
        AND l.source_time>=to_timestamp($3)-interval '45 minutes'
        AND NOT coalesce((c.source_time,c.received_at,c.id)>(l.source_time,l.received_at,l.id),false)
    "#).bind(market).bind(&keys).bind(now as f64).bind(a.config.is_demo()).fetch_all(&a.db).await?;
    let mut current = std::collections::BTreeMap::<String, UnitPrice>::new();
    for row in anchors {
        let value: Option<Value> = row.get("median_buyout");
        if let Some(value) = value {
            let price: UnitPrice = serde_json::from_value(value)
                .map_err(|_| ApiError(StatusCode::INTERNAL_SERVER_ERROR, "invalid_stored_price"))?;
            if price.numerator() > 0 {
                current.insert(row.get("item_key"), price);
            }
        }
    }
    let items: Vec<_> = keys
        .into_iter()
        .map(|key| {
            let samples = history.remove(&key).unwrap_or_default();
            record(&a, market, &key, now, &samples, current.remove(&key))
        })
        .collect();
    Ok(Json(json!({"items":items})))
}
fn record(
    a: &App,
    market: Uuid,
    key: &str,
    now: i64,
    samples: &[Sample],
    current: Option<UnitPrice>,
) -> Value {
    let expires_at = if current.is_some() {
        samples.last().map(|s| s.at + 2700).unwrap_or(now)
    } else {
        now + 2700
    };
    let rolling = forecast::rolling(samples, now, current.is_some());
    let rise_frequencies = forecast::rise_frequencies(samples, now, current.is_some());
    json!({"schema_version":1,"market_id":market,"realm":a.config.realm,"house":a.config.house,"item_key":key,"generated_at":now,"expires_at":expires_at.min(now+2700),"demo":a.config.is_demo(),"price_kind":"asking_price","unit":"copper_per_unit","rolling":rolling,"rise_frequencies":rise_frequencies,"current_median":current})
}
async fn build(a: &App, market: Uuid, key: &str) -> Result<Value> {
    let now = a.config.read_now().timestamp();
    let rows=sqlx::query("SELECT o.median_buyout,s.source_time FROM item_observations o JOIN scans s ON s.id=o.scan_id WHERE s.market_id=$1 AND o.item_key=$2 AND s.quality='complete' AND (s.source_kind='full_scan' OR (s.source_kind='demo' AND $4)) AND ((s.source_kind='demo')=$4) AND s.source_time>=to_timestamp($3)-interval '60 days' AND s.source_time<=to_timestamp($3) AND o.median_buyout IS NOT NULL ORDER BY s.source_time,s.received_at,s.id LIMIT 50001")
        .bind(market).bind(key).bind(now as f64).bind(a.config.is_demo()).fetch_all(&a.db).await?;
    if rows.len() > 50000 {
        return Err(bad("forecast_history_too_dense"));
    }
    let mut samples = vec![];
    for row in rows {
        let price: UnitPrice = serde_json::from_value(row.get("median_buyout"))
            .map_err(|_| ApiError(StatusCode::INTERNAL_SERVER_ERROR, "invalid_stored_price"))?;
        samples.push(Sample {
            at: row.get::<DateTime<Utc>, _>("source_time").timestamp(),
            price,
        });
    }
    // Reuse the latest-item presence check: a newer complete scan can establish absence.
    let current = read::forecast_anchor(a, market, key, now).await?;
    Ok(record(a, market, key, now, &samples, current))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AddonExport {
    market_id: Option<Uuid>,
    item_keys: Vec<String>,
}
/// Data-only cache for a separate addon. Does not write SavedVariables or game files.
pub async fn addon(
    State(a): State<App>,
    h: HeaderMap,
    Json(p): Json<AddonExport>,
) -> Result<Json<Value>> {
    let member = auth::member(&a, &h, true).await?;
    auth::rate(&a, &format!("forecast-export:{}", member.id), 4).await?;
    if a.config.is_demo() {
        return Err(bad("demo_forecasts_not_exportable"));
    }
    let market = read::scope(&a, p.market_id).await?;
    if p.item_keys.is_empty()
        || p.item_keys.len() > 30
        || p.item_keys.iter().any(|key| !valid_key(key))
    {
        return Err(bad("forecast_export_requires_1_to_30_items"));
    }
    let keys: std::collections::BTreeSet<_> = p.item_keys.into_iter().collect();
    let mut records = vec![];
    for key in keys {
        records.push(build(&a, market, &key).await?);
    }
    let generated = a.config.read_now().timestamp();
    let expires = records
        .iter()
        .filter_map(|r| r["expires_at"].as_i64())
        .min()
        .unwrap_or(generated);
    if expires <= generated {
        return Err(bad("forecast_expired_during_export_retry"));
    }
    for row in &mut records {
        row["expires_at"] = json!(expires);
    }
    let cache = json!({"schema_version":1,"realm":a.config.realm,"house":a.config.house,"generated_at":generated,"expires_at":expires,"records":records});
    let lua=format!("-- Generated by Nauctions. Asking-price forecasts; keep timestamps and market scope.\nNauctionsForecastCache = {}\n",lua_value(&cache)?);
    Ok(Json(
        json!({"files":[{"name":"NauctionsForecastCache/NauctionsForecastCache.toc","content":"## Interface: 16001, 120105\n## Title: Nauctions Forecast Cache\n## Notes: Offline forecast data exported by Nauctions.\n## Version: 1\n## LoadOnDemand: 0\nForecastData.lua\n"},{"name":"NauctionsForecastCache/ForecastData.lua","content":lua}],"item_count":cache["records"].as_array().map(Vec::len)}),
    ))
}
fn lua_value(value: &Value) -> Result<String> {
    Ok(match value {
        Value::Null => "false".into(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => {
            let f = n.as_f64().ok_or_else(|| bad("invalid_export_number"))?;
            if !f.is_finite() || f.abs() > 9_007_199_254_740_991.0 {
                return Err(bad("unsupported_export_number"));
            }
            n.to_string()
        }
        Value::String(s) => {
            let mut out = String::from("\"");
            for b in s.as_bytes() {
                out.push_str(&format!("\\{b:03}"));
            }
            out.push('"');
            out
        }
        Value::Array(a) => format!(
            "{{{}}}",
            a.iter()
                .map(lua_value)
                .collect::<Result<Vec<_>>>()?
                .join(",")
        ),
        Value::Object(o) => format!(
            "{{{}}}",
            o.iter()
                .map(|(k, v)| Ok(format!("[{}]={}", lua_value(&json!(k))?, lua_value(v)?)))
                .collect::<Result<Vec<_>>>()?
                .join(",")
        ),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn lua_export_escapes_untrusted_strings_as_utf8_bytes() {
        assert_eq!(lua_value(&json!("\"\\\n")).unwrap(), "\"\\034\\092\\010\"");
        assert_eq!(lua_value(&json!(null)).unwrap(), "false");
        assert!(lua_value(&json!(u64::MAX)).is_err());
    }
}
