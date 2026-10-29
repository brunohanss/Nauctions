//! Bounded, authenticated forecast reads and isolated beta imports.
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
use market_analysis::forecast::{self, BetaRun, Sample};
use market_domain::UnitPrice;
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::Row;
use std::sync::OnceLock;
use uuid::Uuid;

fn reference() -> &'static market_analysis::beta::Reference {
    static DATA: OnceLock<market_analysis::beta::Reference> = OnceLock::new();
    DATA.get_or_init(|| {
        serde_json::from_str(include_str!("../../../assets/beta-normal-horde.json"))
            .expect("validated bundled beta reference")
    })
}
fn reference_item(key: &str) -> Option<&'static market_analysis::beta::ReferenceItem> {
    let digits = key.strip_prefix('i')?;
    if !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let id = digits.parse::<u64>().ok()?;
    reference()
        .items
        .binary_search_by_key(&id, |item| item.id)
        .ok()
        .map(|i| &reference().items[i])
}
fn reference_meta() -> Value {
    let r = reference();
    json!({"schema_version":r.schema_version,"source_realm":r.source_realm,"house":r.house,"reference_date":r.reference_date,"source_updated_at":r.source_updated_at,"source_sha256":r.source_sha256,"daily_rows":r.daily_rows,"item_count":r.items.len(),"attribution":r.attribution,"beta_reset_time_known":false,"scenario_range":"Assumed margin of plus or minus 0.25 in log price; not a probability interval."})
}
#[derive(Deserialize)]
pub struct BetaQuery {
    q: Option<String>,
    offset: Option<usize>,
    day: Option<f64>,
}
pub async fn beta(
    State(a): State<App>,
    h: HeaderMap,
    Query(p): Query<BetaQuery>,
) -> Result<Json<Value>> {
    auth::member(&a, &h, false).await?;
    let q = p.q.unwrap_or_default();
    let offset = p.offset.unwrap_or(0);
    let day = p.day.unwrap_or(0.0);
    if q.len() > 200
        || offset > reference().items.len()
        || !day.is_finite()
        || !(0.0..=17.0).contains(&day)
    {
        return Err(bad("invalid_beta_query"));
    }
    let query = q.to_lowercase();
    let matching: Vec<_> = reference()
        .items
        .iter()
        .filter(|item| item.name.to_lowercase().contains(&query) || item.id.to_string() == query)
        .collect();
    let items:Vec<_>=matching.iter().skip(offset).take(30).map(|item|json!({"item":item,"scenario":market_analysis::beta::scenario(Some(item),day,None,a.config.read_now().timestamp())})).collect();
    Ok(Json(
        json!({"reference":reference_meta(),"items":items,"total_count":matching.len(),"scenario_day":day,"next_offset":(offset+30<matching.len()).then_some(offset+30)}),
    ))
}

#[derive(Deserialize)]
pub struct Scope {
    market_id: Option<Uuid>,
    day: Option<f64>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Import {
    market_id: Option<Uuid>,
    live_started_at: Option<DateTime<Utc>>,
    run: Option<BetaRun>,
}
fn bad(code: &'static str) -> ApiError {
    ApiError(StatusCode::BAD_REQUEST, code)
}
fn valid_key(key: &str) -> bool {
    !key.is_empty() && key.len() <= 200 && key.chars().all(|c| c.is_ascii_graphic())
}
fn validate(run: &BetaRun, now: i64) -> Result<()> {
    if run.run_id.is_empty()
        || run.run_id.len() > 80
        || !run
            .run_id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"_-".contains(&c))
        || run.realm.trim().is_empty()
        || run.realm.len() > 200
        || !["Alliance", "Horde", "Neutral"].contains(&run.house.as_str())
        || run.build.trim().is_empty()
        || run.build.len() > 80
        || run.notes.trim().is_empty()
        || run.notes.len() > 2000
        || run.started_at <= 0
        || run.started_at > now
        || run.samples.is_empty()
        || run.samples.len() > 5000
    {
        return Err(bad("invalid_beta_run"));
    }
    let mut seen = std::collections::BTreeSet::new();
    for sample in &run.samples {
        if !valid_key(&sample.item_key)
            || sample.elapsed_hours > 24 * 180
            || sample.price.numerator() == 0
            || run.started_at + i64::from(sample.elapsed_hours) * 3600 > now
            || !seen.insert((&sample.item_key, sample.elapsed_hours))
        {
            return Err(bad("invalid_or_duplicate_beta_sample"));
        }
    }
    Ok(())
}
pub async fn import(
    State(a): State<App>,
    h: HeaderMap,
    Json(p): Json<Import>,
) -> Result<Json<Value>> {
    let member = auth::admin(&a, &h, true).await?;
    auth::rate(&a, &format!("forecast-import:{}", member.id), 10).await?;
    if a.config.is_demo() {
        return Err(bad("demo_is_read_only"));
    }
    let market = read::scope(&a, p.market_id).await?;
    let now = a.config.read_now().timestamp();
    if p.run.is_none() && p.live_started_at.is_none() {
        return Err(bad("forecast_import_is_empty"));
    }
    if let Some(run) = &p.run {
        validate(run, now)?;
        if run.house != a.config.house {
            return Err(bad("beta_run_house_mismatch"));
        }
    }
    if p.live_started_at.is_some_and(|t| t.timestamp() <= 0) {
        return Err(bad("invalid_launch_time"));
    }
    let mut tx = a.db.begin().await?;
    // Serialize imports for this market so the run limit cannot race.
    sqlx::query("SELECT id FROM markets WHERE id=$1 FOR UPDATE")
        .bind(market)
        .execute(&mut *tx)
        .await?;
    if let Some(run) = p.run {
        let count: i64 =
            sqlx::query_scalar("SELECT count(*) FROM forecast_beta_runs WHERE market_id=$1")
                .bind(market)
                .fetch_one(&mut *tx)
                .await?;
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM forecast_beta_runs WHERE market_id=$1 AND run_id=$2)",
        )
        .bind(market)
        .bind(&run.run_id)
        .fetch_one(&mut *tx)
        .await?;
        if count >= 20 && !exists {
            return Err(bad("beta_run_limit_20"));
        }
        sqlx::query("INSERT INTO forecast_beta_runs(market_id,run_id,payload) VALUES($1,$2,$3) ON CONFLICT(market_id,run_id) DO UPDATE SET payload=excluded.payload,imported_at=now()")
            .bind(market).bind(&run.run_id).bind(json!(run)).execute(&mut *tx).await?;
    }
    if let Some(start) = p.live_started_at {
        sqlx::query("INSERT INTO forecast_launch_settings(market_id,started_at) VALUES($1,$2) ON CONFLICT(market_id) DO UPDATE SET started_at=excluded.started_at").bind(market).bind(start).execute(&mut *tx).await?;
    }
    tx.commit().await?;
    Ok(Json(json!({"ok":true})))
}
pub async fn settings(
    State(a): State<App>,
    h: HeaderMap,
    Query(p): Query<Scope>,
) -> Result<Json<Value>> {
    auth::admin(&a, &h, false).await?;
    let market = read::scope(&a, p.market_id).await?;
    let rows=sqlx::query("SELECT run_id,payload->>'realm' AS realm,payload->>'house' AS house,payload->>'build' AS build,payload->>'notes' AS notes,jsonb_array_length(payload->'samples') AS samples,imported_at FROM forecast_beta_runs WHERE market_id=$1 ORDER BY run_id").bind(market).fetch_all(&a.db).await?;
    let start: Option<DateTime<Utc>> =
        sqlx::query_scalar("SELECT started_at FROM forecast_launch_settings WHERE market_id=$1")
            .bind(market)
            .fetch_optional(&a.db)
            .await?;
    Ok(Json(
        json!({"live_started_at":start,"runs":rows.iter().map(|r|json!({"run_id":r.get::<String,_>("run_id"),"realm":r.get::<Option<String>,_>("realm"),"house":r.get::<Option<String>,_>("house"),"build":r.get::<Option<String>,_>("build"),"notes":r.get::<Option<String>,_>("notes"),"samples":r.get::<i32,_>("samples"),"imported_at":r.get::<DateTime<Utc>,_>("imported_at")})).collect::<Vec<_>>()}),
    ))
}
pub async fn remove(
    State(a): State<App>,
    h: HeaderMap,
    Path(id): Path<String>,
    Query(p): Query<Scope>,
) -> Result<Json<Value>> {
    auth::admin(&a, &h, true).await?;
    if a.config.is_demo() {
        return Err(bad("demo_is_read_only"));
    }
    let market = read::scope(&a, p.market_id).await?;
    sqlx::query("DELETE FROM forecast_beta_runs WHERE market_id=$1 AND run_id=$2")
        .bind(market)
        .bind(id)
        .execute(&a.db)
        .await?;
    Ok(Json(json!({"ok":true})))
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
    let day = p.day.unwrap_or(0.0);
    if !day.is_finite() || !(0.0..=17.0).contains(&day) {
        return Err(bad("invalid_scenario_day"));
    }
    Ok(Json(build(&a, market, &key, day).await?))
}
async fn build(a: &App, market: Uuid, key: &str, day: f64) -> Result<Value> {
    let now = a.config.read_now().timestamp();
    let rows=sqlx::query("SELECT o.median_buyout,s.source_time FROM item_observations o JOIN scans s ON s.id=o.scan_id WHERE s.market_id=$1 AND o.item_key=$2 AND s.quality='complete' AND (s.source_kind='full_scan' OR (s.source_kind='demo' AND $4)) AND ((s.source_kind='demo')=$4) AND s.source_time>=to_timestamp($3)-interval '60 days' AND s.source_time<=to_timestamp($3) AND o.median_buyout IS NOT NULL ORDER BY s.source_time,s.id LIMIT 50001")
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
    let expires_at = if current.is_some() {
        samples.last().map(|s| s.at + 2700).unwrap_or(now)
    } else {
        now + 2700
    };
    let rolling = forecast::rolling(&samples, now, current.is_some());
    let start: Option<DateTime<Utc>> =
        sqlx::query_scalar("SELECT started_at FROM forecast_launch_settings WHERE market_id=$1")
            .bind(market)
            .fetch_optional(&a.db)
            .await?;
    let payloads: Vec<Value> = sqlx::query_scalar(
        "SELECT payload FROM forecast_beta_runs WHERE market_id=$1 ORDER BY run_id",
    )
    .bind(market)
    .fetch_all(&a.db)
    .await?;
    let runs: Vec<BetaRun> = payloads
        .into_iter()
        .map(serde_json::from_value)
        .collect::<std::result::Result<_, _>>()
        .map_err(|_| ApiError(StatusCode::INTERNAL_SERVER_ERROR, "invalid_beta_data"))?;
    let launch = forecast::launch(&runs, key, start.map(|t| t.timestamp()), current, now);
    let item = if a.config.house == "Horde" {
        reference_item(key)
    } else {
        None
    };
    let scenario = market_analysis::beta::scenario(item, day, current, now);
    Ok(
        json!({"schema_version":1,"market_id":market,"realm":a.config.realm,"house":a.config.house,"item_key":key,"generated_at":now,"expires_at":expires_at.min(now+2700),"demo":a.config.is_demo(),"price_kind":"asking_price","unit":"copper_per_unit","rolling":rolling,"launch":launch,"beta_runs":runs.len(),"reference":reference_meta(),"beta_item":item,"scenario":scenario,"scenario_day":day,"current_median":current,"reference_scope_matches":a.config.house=="Horde"}),
    )
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AddonExport {
    market_id: Option<Uuid>,
    item_keys: Vec<String>,
    day: Option<f64>,
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
    let day = p.day.unwrap_or(0.0);
    if p.item_keys.is_empty()
        || p.item_keys.len() > 30
        || p.item_keys.iter().any(|key| !valid_key(key))
        || !day.is_finite()
        || !(0.0..=17.0).contains(&day)
    {
        return Err(bad("forecast_export_requires_1_to_30_items_and_valid_day"));
    }
    let keys: std::collections::BTreeSet<_> = p.item_keys.into_iter().collect();
    let mut records = vec![];
    for key in keys {
        records.push(build(&a, market, &key, day).await?);
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
    fn bundled_reference_is_complete_sorted_and_never_matches_variant_keys() {
        let r = reference();
        assert_eq!(r.items.len(), 3458);
        assert_eq!(r.daily_rows, 11298);
        assert_eq!(r.items.iter().map(|i| i.points.len()).sum::<usize>(), 11298);
        assert!(r.items.windows(2).all(|w| w[0].id < w[1].id));
        assert!(r
            .items
            .iter()
            .all(|i| i.points.windows(2).all(|w| w[0].0 < w[1].0)));
        assert!(reference_item("i1180").is_some());
        assert!(reference_item("i1180:1").is_none());
        assert_eq!(reference_meta()["beta_reset_time_known"], false);
        let f = market_analysis::beta::scenario(reference_item("i1180"), 0.0, None, 100);
        let expected = ((199_f64 / 99.0).ln() * 6.0 / 7.0).exp() * 100.0 - 100.0;
        assert!((f[2].change_percent.unwrap() - expected).abs() < 1e-8);
    }
    #[test]
    fn lua_export_escapes_untrusted_strings_as_utf8_bytes() {
        assert_eq!(lua_value(&json!("\"\\\n")).unwrap(), "\"\\034\\092\\010\"");
        assert_eq!(lua_value(&json!(null)).unwrap(), "false");
        assert!(lua_value(&json!(u64::MAX)).is_err());
    }
    #[test]
    fn imports_reject_future_duplicates_and_missing_context() {
        let mut run = BetaRun {
            run_id: "reset1".into(),
            realm: "Beta".into(),
            house: "Alliance".into(),
            build: "70630".into(),
            notes: "Fresh reset; no free gold".into(),
            started_at: 100,
            samples: vec![forecast::BetaSample {
                item_key: "i1".into(),
                elapsed_hours: 0,
                price: UnitPrice::new(1, 1).unwrap(),
            }],
        };
        assert!(validate(&run, 1000).is_ok());
        run.samples.push(run.samples[0].clone());
        assert!(validate(&run, 1000).is_err());
        run.samples.pop();
        run.samples[0].elapsed_hours = 1;
        assert!(validate(&run, 1000).is_err());
        run.samples[0].elapsed_hours = 0;
        run.notes.clear();
        assert!(validate(&run, 1000).is_err());
    }
}
