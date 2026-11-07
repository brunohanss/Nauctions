//! Authenticated, market-scoped reads. Snapshot inventories are never combined.
use crate::{
    api::{ApiError, Result},
    auth, App,
};
use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use chrono::{DateTime, Utc};
use market_analysis::{Baseline, HistorySample};
use market_domain::UnitPrice;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{postgres::PgRow, Row};
use std::collections::BTreeMap;
use uuid::Uuid;

const ITEM_BATCH_SIZE: i64 = 2000;
const BASELINE_BATCH_SIZE: usize = 100;
const MAX_SAMPLES: i64 = 50000;
fn bad(code: &'static str) -> ApiError {
    ApiError(StatusCode::BAD_REQUEST, code)
}
fn absent() -> ApiError {
    ApiError(StatusCode::NOT_FOUND, "item_not_found")
}
fn key_valid(key: &str) -> bool {
    !key.is_empty() && key.len() <= 200 && key.chars().all(|c| c.is_ascii_graphic())
}

#[derive(Deserialize, Default)]
pub struct Filters {
    market_id: Option<Uuid>,
    q: Option<String>,
    sort: Option<String>,
    direction: Option<String>,
    category: Option<String>,
    limit: Option<usize>,
    cursor: Option<String>,
    freshness: Option<String>,
    range: Option<String>,
}
impl Filters {
    fn paging(&self) -> Result<(usize, usize)> {
        let limit = self.limit.unwrap_or(30);
        if !(1..=100).contains(&limit) {
            return Err(bad("limit_requires_1_to_100"));
        }
        let offset = match self.cursor.as_deref() {
            None | Some("") => 0,
            Some(v) => v.parse::<usize>().map_err(|_| bad("invalid_cursor"))?,
        };
        if offset
            .checked_add(limit)
            .and_then(|n| n.checked_add(1))
            .is_none()
        {
            return Err(bad("cursor_out_of_range"));
        }
        Ok((limit, offset))
    }
    fn validate(&self) -> Result<()> {
        if self.q.as_ref().is_some_and(|q| q.len() > 200) {
            return Err(bad("search_too_long"));
        }
        if ![
            "name",
            "price",
            "current",
            "discount",
            "freshness",
            "age",
            "quantity",
            "score",
            "reference_profit",
        ]
        .contains(&self.sort.as_deref().unwrap_or("name"))
        {
            return Err(bad("invalid_sort"));
        }
        if !["asc", "desc"].contains(&self.direction.as_deref().unwrap_or("asc")) {
            return Err(bad("invalid_direction"));
        }
        if ![
            "all",
            "materials",
            "consumables",
            "recipes",
            "equipment",
            "other",
            "watchlist",
        ]
        .contains(&self.category.as_deref().unwrap_or("all"))
        {
            return Err(bad("invalid_category"));
        }
        if !["all", "fresh", "stale"].contains(&self.freshness.as_deref().unwrap_or("all")) {
            return Err(bad("invalid_freshness"));
        }
        Ok(())
    }
}
pub(crate) async fn scope(a: &App, id: Option<Uuid>) -> Result<Uuid> {
    let configured: Option<Uuid> =
        sqlx::query_scalar("SELECT id FROM markets WHERE realm=$1 AND house=$2 AND region='EU'")
            .bind(&a.config.realm)
            .bind(&a.config.house)
            .fetch_optional(&a.db)
            .await?;
    let configured = configured.ok_or(ApiError(StatusCode::NOT_FOUND, "market_not_configured"))?;
    if id.is_some_and(|id| id != configured) {
        return Err(ApiError(StatusCode::NOT_FOUND, "market_not_found"));
    }
    Ok(configured)
}
pub async fn markets(State(a): State<App>, h: HeaderMap) -> Result<Json<Value>> {
    auth::member(&a, &h, false).await?;
    let r=sqlx::query("SELECT m.id,m.realm,m.house,(SELECT max(source_time) FROM scans WHERE market_id=m.id AND source_time<=to_timestamp($3) AND ((source_kind='demo') = (m.realm='Nauctions Demo' AND m.house='Alliance'))) AS last_observed_at,(SELECT count(*) FROM scans WHERE market_id=m.id AND source_time<=to_timestamp($3) AND ((source_kind='demo') = (m.realm='Nauctions Demo' AND m.house='Alliance'))) AS scan_count,(SELECT count(DISTINCT ss.device_id) FROM scan_sources ss JOIN scans s ON s.id=ss.scan_id WHERE s.market_id=m.id) AS source_count FROM markets m WHERE m.realm=$1 AND m.house=$2 AND m.region='EU'")
      .bind(&a.config.realm).bind(&a.config.house).bind(a.config.read_now().timestamp() as f64).fetch_all(&a.db).await?;
    Ok(Json(json!(r.iter().map(|r|json!({"id":r.get::<Uuid,_>("id"),"realm":r.get::<String,_>("realm"),"house":r.get::<String,_>("house"),"last_observed_at":r.get::<Option<DateTime<Utc>>,_>("last_observed_at"),"scan_count":r.get::<i64,_>("scan_count"),"source_count":r.get::<i64,_>("source_count"),"demo":a.config.is_demo(),"as_of":a.config.demo_as_of,"read_only":a.config.is_demo()})).collect::<Vec<_>>())))
}

#[derive(Clone, Serialize)]
struct Item {
    unit: &'static str,
    price_kind: &'static str,
    item_key: String,
    name: String,
    item_id: i64,
    category: Option<&'static str>,
    rarity: Option<u64>,
    icon: Option<String>,
    catalog_attribution: Option<crate::catalog::Attribution>,
    opportunity_score: f64,
    reference_profit: Option<market_analysis::crafting::Amount>,
    discount_percentage: Option<f64>,
    freshness_status: &'static str,
    trend: Vec<TrendPoint>,
    change_24h_percentage: Option<f64>,
    market_id: Uuid,
    scan_id: Uuid,
    observed_at: DateTime<Utc>,
    received_at: DateTime<Utc>,
    source_kind: String,
    quality: String,
    sample_count: i64,
    min_buyout: Option<UnitPrice>,
    median_buyout: Option<UnitPrice>,
    total_units: Option<i64>,
    listing_count: Option<i64>,
    baseline: Baseline,
    freshness_seconds: i64,
    presence: &'static str,
    source_count: i64,
}
#[derive(Clone, Serialize)]
struct TrendPoint {
    observed_at: DateTime<Utc>,
    median_buyout: UnitPrice,
    quality: String,
}
fn category(class_id: Option<u64>) -> Option<&'static str> {
    class_id.map(|id| match id {
        0 => "consumables",
        2 | 4 => "equipment",
        3 | 5 | 7 => "materials",
        9 => "recipes",
        _ => "other",
    })
}
fn parse_price(value: Option<Value>) -> Result<Option<UnitPrice>> {
    value
        .map(|v| {
            serde_json::from_value(v)
                .map_err(|_| ApiError(StatusCode::INTERNAL_SERVER_ERROR, "invalid_stored_price"))
        })
        .transpose()
}
fn item_row(r: &PgRow, market_id: Uuid, now: i64) -> Result<Item> {
    let observed_at: DateTime<Utc> = r.get("source_time");
    let is_absent: bool = r.get("absent");
    let metadata: Value = r.get("metadata");
    let catalog_entry = crate::catalog::lookup(r.get("base_item_id"));
    Ok(Item {
        unit: "copper_per_unit",
        price_kind: "asking_price",
        item_key: r.get("item_key"),
        name: r.get("name"),
        item_id: r.get("base_item_id"),
        category: category(metadata.get("class_id").and_then(Value::as_u64)),
        rarity: metadata.get("rarity").and_then(Value::as_u64),
        icon: catalog_entry.map(|entry| entry.icon.clone()),
        catalog_attribution: catalog_entry.map(crate::catalog::Entry::attribution),
        opportunity_score: 0.0,
        reference_profit: None,
        discount_percentage: None,
        freshness_status: if now.saturating_sub(observed_at.timestamp()) > 2700 {
            "stale"
        } else {
            "fresh"
        },
        trend: vec![],
        change_24h_percentage: None,
        market_id,
        scan_id: r.get("id"),
        observed_at,
        received_at: r.get("received_at"),
        source_kind: r.get("source_kind"),
        quality: r.get("quality"),
        sample_count: 1,
        min_buyout: if is_absent {
            None
        } else {
            parse_price(r.get("min_buyout"))?
        },
        median_buyout: if is_absent {
            None
        } else {
            parse_price(r.get("median_buyout"))?
        },
        total_units: if is_absent {
            Some(0)
        } else {
            r.get("total_units")
        },
        listing_count: if is_absent {
            Some(0)
        } else {
            r.get("listing_count")
        },
        baseline: Baseline {
            price: None,
            day_count: 0,
            source_count: 0,
        },
        freshness_seconds: now.saturating_sub(observed_at.timestamp()).max(0),
        presence: if is_absent {
            "absent_in_complete_scan"
        } else {
            "observed"
        },
        source_count: r.get("source_count"),
    })
}

// Choose one latest observation per exact variant key. A newer complete scan that
// omits the key establishes absence. Partial/unknown omissions establish nothing.
async fn latest(a: &App, market: Uuid, q: &str, exact: Option<&str>) -> Result<Vec<Item>> {
    let now = a.config.read_now().timestamp();
    let mut rows = Vec::new();
    let mut after = None;
    loop {
        let batch = latest_batch(a, market, q, exact, after.as_deref(), now).await?;
        let done = batch.len() < ITEM_BATCH_SIZE as usize;
        after = batch.last().map(|row| row.item_key.clone());
        rows.extend(batch);
        if done {
            return Ok(rows);
        }
    }
}

async fn latest_batch(
    a: &App,
    market: Uuid,
    q: &str,
    exact: Option<&str>,
    after: Option<&str>,
    now: i64,
) -> Result<Vec<Item>> {
    let rs=sqlx::query(r#"
      WITH complete AS (
        SELECT * FROM scans WHERE market_id=$1 AND source_time<=to_timestamp($5) AND quality='complete' AND ((source_kind='demo') = $7)
        ORDER BY source_time DESC,received_at DESC,id DESC LIMIT 1
      ), latest AS (
        SELECT DISTINCT ON(o.item_key) o.*,s.source_time,s.received_at,s.id,s.source_kind,s.quality
        FROM item_observations o JOIN scans s ON s.id=o.scan_id JOIN items i ON i.item_key=o.item_key
        WHERE s.market_id=$1 AND ((s.source_kind='demo') = $7) AND s.source_time<=to_timestamp($5) AND ($3::text IS NULL OR i.item_key=$3) AND ($6::text IS NULL OR o.item_key>$6)
          AND (strpos(lower(i.name),lower($2))>0 OR strpos(lower(i.item_key),lower($2))>0)
        ORDER BY o.item_key,s.source_time DESC,s.received_at DESC,s.id DESC
      ), chosen AS (
        SELECT l.item_key,i.name,i.base_item_id,i.metadata,l.min_buyout,l.median_buyout,l.total_units,l.listing_count,
          CASE WHEN (c.source_time,c.received_at,c.id)>(l.source_time,l.received_at,l.id) THEN c.id ELSE l.id END AS chosen_id,
          coalesce((c.source_time,c.received_at,c.id)>(l.source_time,l.received_at,l.id),false) AS absent
        FROM latest l JOIN items i ON i.item_key=l.item_key LEFT JOIN complete c ON true
        ORDER BY l.item_key LIMIT $4
      ) SELECT c.*,s.id,s.source_time,s.received_at,s.source_kind,s.quality,
        (SELECT count(DISTINCT device_id) FROM scan_sources WHERE scan_id=s.id) AS source_count
      FROM chosen c JOIN scans s ON s.id=c.chosen_id ORDER BY c.item_key
    "#).bind(market).bind(q).bind(exact).bind(ITEM_BATCH_SIZE).bind(now as f64).bind(after).bind(a.config.is_demo()).fetch_all(&a.db).await?;
    rs.iter().map(|r| item_row(r, market, now)).collect()
}

async fn baselines(
    a: &App,
    market: Uuid,
    keys: &[String],
    now: i64,
) -> Result<BTreeMap<String, Baseline>> {
    if keys.is_empty() {
        return Ok(BTreeMap::new());
    }
    let mut result = BTreeMap::new();
    for batch in keys.chunks(BASELINE_BATCH_SIZE) {
        result.extend(baselines_batch(a, market, batch, now).await?);
    }
    Ok(result)
}

async fn baselines_batch(
    a: &App,
    market: Uuid,
    keys: &[String],
    now: i64,
) -> Result<BTreeMap<String, Baseline>> {
    // The first presenter is attribution, not a second vote for the same scan.
    let rs=sqlx::query("SELECT o.item_key,o.median_buyout,s.source_time,s.quality,coalesce((SELECT device_id::text FROM scan_sources WHERE scan_id=s.id ORDER BY upload_id LIMIT 1),s.id::text) AS source_id FROM item_observations o JOIN scans s ON s.id=o.scan_id WHERE s.market_id=$1 AND ((s.source_kind='demo') = EXISTS(SELECT 1 FROM markets dm WHERE dm.id=s.market_id AND dm.realm='Nauctions Demo' AND dm.house='Alliance' AND dm.region='EU')) AND o.item_key=ANY($2) AND s.source_time>=to_timestamp($3)-interval '9 days' AND s.source_time<to_timestamp($3) AND s.quality='complete' AND o.median_buyout IS NOT NULL ORDER BY s.source_time LIMIT $4")
      .bind(market).bind(keys).bind(now as f64).bind(MAX_SAMPLES+1).fetch_all(&a.db).await?;
    if rs.len() > MAX_SAMPLES as usize {
        return Err(bad("history_too_dense_narrow_search"));
    }
    let mut samples: BTreeMap<String, Vec<HistorySample>> = BTreeMap::new();
    for r in rs {
        if let Some(price) = parse_price(r.get("median_buyout"))? {
            samples
                .entry(r.get("item_key"))
                .or_default()
                .push(HistorySample {
                    observed_at: r.get::<DateTime<Utc>, _>("source_time").timestamp(),
                    price,
                    source_id: r.get("source_id"),
                    eligible: true,
                })
        }
    }
    Ok(keys
        .iter()
        .map(|key| {
            (
                key.clone(),
                market_analysis::baseline(samples.get(key).map(Vec::as_slice).unwrap_or(&[]), now),
            )
        })
        .collect())
}
pub async fn baseline_for(a: &App, market_id: Uuid, key: &str, now: i64) -> Result<Baseline> {
    Ok(baselines(a, market_id, &[key.to_owned()], now)
        .await?
        .remove(key)
        .unwrap_or(Baseline {
            price: None,
            day_count: 0,
            source_count: 0,
        }))
}
fn relative_price(item: &Item) -> Option<(u128, u128)> {
    let a = item.min_buyout?;
    let b = item.baseline.price?;
    if b.numerator() == 0 {
        return None;
    }
    Some((
        a.numerator() as u128 * b.denominator() as u128,
        a.denominator() as u128 * b.numerator() as u128,
    ))
}
// Continued-fraction comparison avoids overflow when comparing two u128 ratios.
fn ratio_cmp(mut a: u128, mut b: u128, mut c: u128, mut d: u128) -> std::cmp::Ordering {
    let mut reverse = false;
    loop {
        let order = (a / b).cmp(&(c / d));
        if !order.is_eq() {
            return if reverse { order.reverse() } else { order };
        }
        let (r, s) = (a % b, c % d);
        if r == 0 || s == 0 {
            let order = r.cmp(&s);
            return if reverse { order.reverse() } else { order };
        }
        (a, b, c, d) = (b, r, d, s);
        reverse = !reverse;
    }
}
fn discount_cmp(a: &Item, b: &Item) -> std::cmp::Ordering {
    match (relative_price(a), relative_price(b)) {
        (Some((a, b)), Some((c, d))) => ratio_cmp(a, b, c, d),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        _ => std::cmp::Ordering::Equal,
    }
}
fn qualifies(item: &Item, now: i64) -> bool {
    item.presence == "observed"
        && item.min_buyout.is_some_and(|current| {
            market_analysis::evaluate(
                market_analysis::SignalContext {
                    now,
                    observed_at: item.observed_at.timestamp(),
                    eligible: item.quality == "complete",
                    current,
                    baseline: &item.baseline,
                    previous: None,
                },
                &market_analysis::SignalRule::Underpriced {
                    discount_percent: 30,
                },
            )
            .is_some()
        })
}
fn percentage(current: UnitPrice, reference: UnitPrice) -> Option<f64> {
    (reference.numerator() > 0).then(|| {
        ((current.numerator() as f64 / current.denominator() as f64)
            / (reference.numerator() as f64 / reference.denominator() as f64)
            - 1.0)
            * 100.0
    })
}
fn score(item: &mut Item) {
    item.discount_percentage = item
        .min_buyout
        .zip(item.baseline.price)
        .and_then(|(c, r)| percentage(c, r));
    // A bounded advisory score, not expected profit. Every factor is observable.
    // Stale, incomplete or absent snapshots cannot be actionable opportunities.
    item.opportunity_score = if item.quality != "complete"
        || item.presence != "observed"
        || item.freshness_seconds > 2700
    {
        0.0
    } else {
        opportunity_score(
            item.discount_percentage.unwrap_or(0.0),
            item.baseline.day_count,
            item.freshness_seconds,
            item.total_units.unwrap_or(0),
        )
    };
}
// Conditional unit margin at the historical median, not a sale forecast.
fn reference_profit(
    item: &Item,
    proceeds_percent: i128,
) -> Option<market_analysis::crafting::Amount> {
    if item.quality != "complete" || item.presence != "observed" || item.freshness_seconds > 2700 {
        return None;
    }
    let current = item.min_buyout?;
    let reference = item.baseline.price?;
    reference_margin(current, reference, proceeds_percent)
}
fn reference_margin(
    current: UnitPrice,
    reference: UnitPrice,
    proceeds_percent: i128,
) -> Option<market_analysis::crafting::Amount> {
    let proceeds = i128::from(reference.numerator())
        .checked_mul(proceeds_percent)?
        .checked_mul(i128::from(current.denominator()))?;
    let cost = i128::from(current.numerator())
        .checked_mul(100)?
        .checked_mul(i128::from(reference.denominator()))?;
    Some(market_analysis::crafting::Amount {
        numerator: proceeds.checked_sub(cost)?,
        denominator: i128::from(reference.denominator())
            .checked_mul(i128::from(current.denominator()))?
            .checked_mul(100)?,
    })
}
fn opportunity_score(discount_percentage: f64, days: usize, age: i64, quantity: i64) -> f64 {
    let deviation = (-discount_percentage / 100.0).clamp(0.0, 1.0);
    let coverage = (days as f64 / 7.0).clamp(0.0, 1.0);
    let freshness = (1.0 - age.max(0) as f64 / 2700.0).clamp(0.0, 1.0);
    let depth = (quantity.max(0) as f64).ln_1p() / 101_f64.ln();
    100.0 * deviation * coverage * freshness * depth.clamp(0.0, 1.0)
}
// One bounded query for the displayed page. Samples are actual complete scans;
// no buckets, interpolation or upload timestamps are used for the sparkline.
async fn trends(a: &App, market: Uuid, rows: &mut [Item], now: i64) -> Result<()> {
    let keys: Vec<_> = rows.iter().map(|r| r.item_key.clone()).collect();
    if keys.is_empty() {
        return Ok(());
    }
    let rs = sqlx::query("SELECT o.item_key,o.median_buyout,s.source_time,s.quality FROM item_observations o JOIN scans s ON s.id=o.scan_id WHERE s.market_id=$1 AND ((s.source_kind='demo') = EXISTS(SELECT 1 FROM markets dm WHERE dm.id=s.market_id AND dm.realm='Nauctions Demo' AND dm.house='Alliance' AND dm.region='EU')) AND o.item_key=ANY($2) AND s.quality='complete' AND s.source_time>=to_timestamp($3)-interval '25 hours' AND s.source_time<=to_timestamp($3) AND o.median_buyout IS NOT NULL ORDER BY s.source_time,s.received_at,s.id LIMIT $4")
        .bind(market).bind(&keys).bind(now as f64).bind(MAX_SAMPLES+1).fetch_all(&a.db).await?;
    if rs.len() > MAX_SAMPLES as usize {
        return Err(bad("history_too_dense_narrow_search"));
    }
    let mut grouped: BTreeMap<String, Vec<TrendPoint>> = BTreeMap::new();
    for r in rs {
        if let Some(price) = parse_price(r.get("median_buyout"))? {
            grouped
                .entry(r.get("item_key"))
                .or_default()
                .push(TrendPoint {
                    observed_at: r.get("source_time"),
                    median_buyout: price,
                    quality: r.get("quality"),
                });
        }
    }
    for row in rows {
        let points = grouped.remove(&row.item_key).unwrap_or_default();
        // A 24h change needs a real observation within one hour before the cutoff.
        let previous = points
            .iter()
            .rev()
            .find(|p| p.observed_at.timestamp() <= now - 86400);
        row.change_24h_percentage = row
            .median_buyout
            .zip(previous.map(|p| p.median_buyout))
            .filter(|_| row.presence == "observed" && row.freshness_seconds <= 2700)
            .and_then(|(current, prior)| percentage(current, prior));
        row.trend = points
            .into_iter()
            .filter(|p| p.observed_at.timestamp() >= now - 86400)
            .collect();
    }
    Ok(())
}
async fn listing(a: App, h: HeaderMap, f: Filters, opportunity: bool) -> Result<Json<Value>> {
    let member = auth::member(&a, &h, false).await?;
    f.validate()?;
    let (limit, offset) = f.paging()?;
    let market = scope(&a, f.market_id).await?;
    let now = a.config.read_now().timestamp();
    let watched: Option<Vec<String>> = if f.category.as_deref() == Some("watchlist") {
        Some(
            sqlx::query_scalar("SELECT item_key FROM watchlists WHERE member_id=$1")
                .bind(member.id)
                .fetch_all(&a.db)
                .await?,
        )
    } else {
        None
    };
    let needs_history_for_ranking = opportunity
        || matches!(
            f.sort.as_deref(),
            Some("score" | "discount" | "reference_profit")
        );
    let mut after = None;
    let mut selected = Vec::new();
    let mut total_count = 0;
    loop {
        let mut rows = latest_batch(
            &a,
            market,
            f.q.as_deref().unwrap_or(""),
            None,
            after.as_deref(),
            now,
        )
        .await?;
        let done = rows.len() < ITEM_BATCH_SIZE as usize;
        after = rows.last().map(|row| row.item_key.clone());
        if let Some(watched) = &watched {
            rows.retain(|row| watched.contains(&row.item_key));
        }
        if needs_history_for_ranking {
            let keys = rows.iter().map(|r| r.item_key.clone()).collect::<Vec<_>>();
            let mut bs = baselines(&a, market, &keys, now).await?;
            for row in &mut rows {
                if let Some(b) = bs.remove(&row.item_key) {
                    row.baseline = b
                }
                score(row);
                row.reference_profit =
                    reference_profit(row, if a.config.house == "Neutral" { 85 } else { 95 });
            }
        }
        rows.retain(|r| {
            let fresh = r.freshness_seconds <= 2700;
            let filter = match f.freshness.as_deref().unwrap_or("all") {
                "fresh" => fresh,
                "stale" => !fresh,
                _ => true,
            };
            let category_matches =
                matches!(f.category.as_deref().unwrap_or("all"), "all" | "watchlist")
                    || r.category == f.category.as_deref();
            filter && category_matches && (!opportunity || qualifies(r, now))
        });
        total_count += rows.len();
        selected.extend(rows);
        let mut rows = selected;
        rows.sort_by(|a, b| {
            if f.sort.as_deref() == Some("reference_profit") {
                let descending = f.direction.as_deref().unwrap_or("desc") == "desc";
                return match (a.reference_profit, b.reference_profit) {
                    (Some(x), Some(y)) => {
                        let order = crate::crafting::compare_amounts(x, y);
                        (if descending { order.reverse() } else { order })
                            .then(a.item_key.cmp(&b.item_key))
                    }
                    (Some(_), None) => std::cmp::Ordering::Less,
                    (None, Some(_)) => std::cmp::Ordering::Greater,
                    _ => a.item_key.cmp(&b.item_key),
                };
            }
            let order =
                match f
                    .sort
                    .as_deref()
                    .unwrap_or(if opportunity { "score" } else { "name" })
                {
                    "price" | "current" => match (a.min_buyout, b.min_buyout) {
                        (Some(a), Some(b)) => a.cmp(&b),
                        (Some(_), None) => std::cmp::Ordering::Less,
                        (None, Some(_)) => std::cmp::Ordering::Greater,
                        _ => std::cmp::Ordering::Equal,
                    },
                    "freshness" | "age" => b.observed_at.cmp(&a.observed_at),
                    "quantity" => a.total_units.cmp(&b.total_units),
                    "score" => a.opportunity_score.total_cmp(&b.opportunity_score),
                    "discount" => discount_cmp(a, b),
                    _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
                };
            let descending = f.direction.as_deref().unwrap_or(
                if opportunity && f.sort.as_deref().unwrap_or("score") == "score" {
                    "desc"
                } else {
                    "asc"
                },
            ) == "desc";
            (if descending { order.reverse() } else { order }).then(a.item_key.cmp(&b.item_key))
        });
        // Keep only candidates that can reach the requested globally sorted page.
        rows.truncate(offset + limit + 1);
        selected = rows;
        if done {
            break;
        }
    }
    let rows = selected;
    let next = (offset + limit < total_count).then(|| (offset + limit).to_string());
    let mut rows = rows
        .into_iter()
        .skip(offset)
        .take(limit)
        .collect::<Vec<_>>();
    if !needs_history_for_ranking {
        let keys = rows.iter().map(|r| r.item_key.clone()).collect::<Vec<_>>();
        let mut bs = baselines(&a, market, &keys, now).await?;
        for row in &mut rows {
            if let Some(b) = bs.remove(&row.item_key) {
                row.baseline = b
            }
            score(row);
            row.reference_profit =
                reference_profit(row, if a.config.house == "Neutral" { 85 } else { 95 });
        }
    }
    trends(&a, market, &mut rows, now).await?;
    Ok(Json(
        json!({"items":rows,"next_cursor":next,"total_count":total_count}),
    ))
}
pub async fn items(
    State(a): State<App>,
    h: HeaderMap,
    Query(f): Query<Filters>,
) -> Result<Json<Value>> {
    listing(a, h, f, false).await
}
pub async fn opportunities(
    State(a): State<App>,
    h: HeaderMap,
    Query(f): Query<Filters>,
) -> Result<Json<Value>> {
    listing(a, h, f, true).await
}
/// Research candidates use complete, canonical scans only. No sale or liquidity inference.
pub async fn strategies(
    State(a): State<App>,
    h: HeaderMap,
    Query(f): Query<Filters>,
) -> Result<Json<Value>> {
    auth::member(&a, &h, false).await?;
    f.validate()?;
    let market = scope(&a, f.market_id).await?;
    let now = a.config.read_now().timestamp();
    let rows = latest(&a, market, f.q.as_deref().unwrap_or(""), None).await?;
    let keys: Vec<_> = rows.iter().map(|r| r.item_key.clone()).collect();
    let mut samples: BTreeMap<String, Vec<market_analysis::strategy::Sample>> = BTreeMap::new();
    for key_batch in keys.chunks(BASELINE_BATCH_SIZE) {
        let rs = sqlx::query("SELECT o.item_key,o.median_buyout,o.total_units,s.source_time FROM item_observations o JOIN scans s ON s.id=o.scan_id WHERE s.market_id=$1 AND o.item_key=ANY($2) AND s.quality='complete' AND (s.source_kind='full_scan' OR (s.source_kind='demo' AND $5)) AND ((s.source_kind='demo') = $5) AND s.source_time>=to_timestamp($3)-interval '90 days' AND s.source_time<=to_timestamp($3) AND o.median_buyout IS NOT NULL AND o.total_units IS NOT NULL ORDER BY s.source_time,s.id LIMIT $4")
        .bind(market).bind(key_batch).bind(now as f64).bind(MAX_SAMPLES + 1).bind(a.config.is_demo()).fetch_all(&a.db).await?;
        if rs.len() > MAX_SAMPLES as usize {
            return Err(bad("strategy_history_too_dense_narrow_search"));
        }
        for r in rs {
            if let Some(price) = parse_price(r.get("median_buyout"))? {
                samples.entry(r.get("item_key")).or_default().push(
                    market_analysis::strategy::Sample {
                        at: r.get::<DateTime<Utc>, _>("source_time").timestamp(),
                        price,
                        quantity: r.get::<i64, _>("total_units").max(0) as u64,
                    },
                );
            }
        }
    }
    let results: Vec<_> = rows.into_iter().map(|row| {
        let ss = samples.remove(&row.item_key).unwrap_or_default();
        let fresh = row.quality == "complete" && row.presence == "observed" && row.freshness_seconds <= 2700
            && row.min_buyout.is_some_and(|p| p.numerator() > 0)
            && (row.source_kind == "full_scan" || (a.config.is_demo() && row.source_kind == "demo"));
        let patterns = market_analysis::strategy::analyze(&ss, now, row.min_buyout, fresh);
        json!({"item":row,"patterns":patterns,"heatmap":market_analysis::strategy::heatmap(&ss, now)})
    }).collect();
    Ok(Json(
        json!({"items":results,"as_of":a.config.read_now(),"timezone":"Europe/Paris","liquidity":"unknown","method":"exploratory_v2_irregular_pairs","history_days":90}),
    ))
}
pub(crate) async fn forecast_anchor(
    a: &App,
    market: Uuid,
    key: &str,
    now: i64,
) -> Result<Option<UnitPrice>> {
    let Some(row) = latest(a, market, "", Some(key)).await?.pop() else {
        return Ok(None);
    };
    Ok((row.quality == "complete"
        && row.presence == "observed"
        && now.saturating_sub(row.observed_at.timestamp()) <= 2700
        && (row.source_kind == "full_scan" || (a.config.is_demo() && row.source_kind == "demo")))
        .then_some(row.median_buyout)
        .flatten()
        .filter(|p| p.numerator() > 0))
}
pub async fn item(
    State(a): State<App>,
    h: HeaderMap,
    Path(key): Path<String>,
    Query(f): Query<Filters>,
) -> Result<Json<Value>> {
    auth::member(&a, &h, false).await?;
    if !key_valid(&key) {
        return Err(bad("invalid_item_key"));
    }
    let market = scope(&a, f.market_id).await?;
    let mut row = latest(&a, market, "", Some(&key))
        .await?
        .pop()
        .ok_or_else(absent)?;
    row.baseline = baseline_for(&a, market, &key, a.config.read_now().timestamp()).await?;
    score(&mut row);
    let mut value = json!(row);
    let meta = sqlx::query_scalar::<_, Value>("SELECT metadata FROM items WHERE item_key=$1")
        .bind(&key)
        .fetch_optional(&a.db)
        .await?;
    let merchant = crate::item_metadata::merchant_price(&key, meta.as_ref());
    value["merchant_unit_price"] = json!(merchant
        .filter(|n| *n > 0)
        .map(market_analysis::crafting::Amount::integer));
    value["merchant_status"] = json!(match merchant {
        Some(0) => "not_sellable",
        Some(_) => "available",
        None => "unavailable",
    });
    Ok(Json(value))
}
pub async fn history(
    State(a): State<App>,
    h: HeaderMap,
    Path(key): Path<String>,
    Query(f): Query<Filters>,
) -> Result<Json<Value>> {
    auth::member(&a, &h, false).await?;
    if !key_valid(&key) {
        return Err(bad("invalid_item_key"));
    }
    let market = scope(&a, f.market_id).await?;
    let days = match f.range.as_deref().unwrap_or("7d") {
        "24h" => 1,
        "7d" => 7,
        "30d" => 30,
        "all" => a.config.read_now().timestamp().div_euclid(86400) + 1,
        _ => return Err(bad("invalid_history_range")),
    };
    let now = a.config.read_now().timestamp();
    let since = now - days * 86400;
    let rs=sqlx::query("SELECT i.name,i.base_item_id,i.metadata,o.item_key,o.min_buyout,o.median_buyout,o.total_units,o.listing_count,s.id,s.source_time,s.received_at,s.source_kind,s.quality,false AS absent,(SELECT count(DISTINCT device_id) FROM scan_sources WHERE scan_id=s.id) AS source_count FROM item_observations o JOIN scans s ON s.id=o.scan_id JOIN items i ON i.item_key=o.item_key WHERE s.market_id=$1 AND ((s.source_kind='demo') = EXISTS(SELECT 1 FROM markets dm WHERE dm.id=s.market_id AND dm.realm='Nauctions Demo' AND dm.house='Alliance' AND dm.region='EU')) AND o.item_key=$2 AND s.source_time>=to_timestamp($3) AND s.source_time<=to_timestamp($4) ORDER BY s.source_time,s.id LIMIT $5")
      .bind(market).bind(&key).bind(since as f64).bind(now as f64).bind(MAX_SAMPLES+1).fetch_all(&a.db).await?;
    if rs.len() > MAX_SAMPLES as usize {
        return Err(bad("history_too_dense_select_shorter_range"));
    }
    let points: Vec<Item> = rs
        .iter()
        .map(|r| item_row(r, market, now))
        .collect::<Result<_>>()?;
    let (today, _) =
        market_analysis::paris_day_bounds(now).map_err(|_| bad("invalid_timestamp"))?;
    let (yesterday, _) =
        market_analysis::paris_day_bounds(today - 1).map_err(|_| bad("invalid_timestamp"))?;
    let yr=sqlx::query("SELECT o.median_buyout FROM item_observations o JOIN scans s ON s.id=o.scan_id WHERE s.market_id=$1 AND ((s.source_kind='demo') = EXISTS(SELECT 1 FROM markets dm WHERE dm.id=s.market_id AND dm.realm='Nauctions Demo' AND dm.house='Alliance' AND dm.region='EU')) AND o.item_key=$2 AND s.source_time>=to_timestamp($3) AND s.source_time<to_timestamp($4) AND o.median_buyout IS NOT NULL LIMIT $5")
      .bind(market).bind(&key).bind(yesterday as f64).bind(today as f64).bind(MAX_SAMPLES+1).fetch_all(&a.db).await?;
    if yr.len() > MAX_SAMPLES as usize {
        return Err(bad("history_too_dense_select_shorter_range"));
    }
    let prices = yr
        .iter()
        .map(|r| parse_price(r.get("median_buyout")))
        .collect::<Result<Vec<_>>>()?
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    let daily=sqlx::query("SELECT utc_day,source_identity,point FROM historic_daily_imports WHERE market_id=$1 AND item_key=$2 AND utc_day>=$3 AND utc_day<=$4 ORDER BY utc_day,source_identity LIMIT 1001")
      .bind(market).bind(&key).bind(since.div_euclid(86400)).bind(now.div_euclid(86400)).fetch_all(&a.db).await?;
    if daily.len() > 1000 {
        return Err(bad("daily_history_too_dense"));
    }
    let summary_rows = sqlx::query("SELECT o.min_buyout FROM item_observations o JOIN scans s ON s.id=o.scan_id WHERE s.market_id=$1 AND ((s.source_kind='demo') = EXISTS(SELECT 1 FROM markets dm WHERE dm.id=s.market_id AND dm.realm='Nauctions Demo' AND dm.house='Alliance' AND dm.region='EU')) AND o.item_key=$2 AND s.source_time>=to_timestamp($3)-interval '7 days' AND s.source_time<=to_timestamp($3) ORDER BY s.source_time LIMIT $4")
        .bind(market).bind(&key).bind(now as f64).bind(MAX_SAMPLES+1).fetch_all(&a.db).await?;
    if summary_rows.len() > MAX_SAMPLES as usize {
        return Err(bad("history_too_dense_select_shorter_range"));
    }
    let minimum = summary_rows
        .iter()
        .map(|r| parse_price(r.get("min_buyout")))
        .collect::<Result<Vec<_>>>()?
        .into_iter()
        .flatten()
        .min();
    let maximum_row = sqlx::query("SELECT l.buyout_total_copper,l.stack_count FROM listings l JOIN scans s ON s.id=l.scan_id WHERE s.market_id=$1 AND ((s.source_kind='demo') = EXISTS(SELECT 1 FROM markets dm WHERE dm.id=s.market_id AND dm.realm='Nauctions Demo' AND dm.house='Alliance' AND dm.region='EU')) AND l.item_key=$2 AND s.source_time>=to_timestamp($3)-interval '7 days' AND s.source_time<=to_timestamp($3) AND (s.source_kind='full_scan' OR (s.source_kind='demo' AND $4)) AND l.buyout_total_copper>0 ORDER BY l.buyout_total_copper::numeric/l.stack_count DESC LIMIT 1")
        .bind(market).bind(&key).bind(now as f64).bind(a.config.is_demo()).fetch_optional(&a.db).await?;
    let maximum = maximum_row.and_then(|r| {
        UnitPrice::new(
            r.get::<i64, _>("buyout_total_copper") as u64,
            r.get::<i64, _>("stack_count") as u64,
        )
    });
    Ok(Json(
        json!({"seven_day":{"minimum":minimum,"maximum":maximum,"observation_count":summary_rows.len(),"maximum_source":if a.config.is_demo() { "demo_listings" } else { "full_scan_listings" }},"points":points,"yesterday":{"low":prices.iter().min(),"high":prices.iter().max(),"start":DateTime::from_timestamp(yesterday,0),"end":DateTime::from_timestamp(today,0),"timezone":"Europe/Paris","method":"range_of_observed_snapshot_median_reference_prices"},"daily_imports":daily.iter().map(|r|json!({"utc_day":r.get::<i64,_>("utc_day"),"source_kind":"ahdb_daily","source_identity":r.get::<String,_>("source_identity"),"point":r.get::<Value,_>("point")})).collect::<Vec<_>>()}),
    ))
}
pub async fn depth(
    State(a): State<App>,
    h: HeaderMap,
    Path(key): Path<String>,
    Query(f): Query<Filters>,
) -> Result<Json<Value>> {
    auth::member(&a, &h, false).await?;
    if !key_valid(&key) {
        return Err(bad("invalid_item_key"));
    }
    let market = scope(&a, f.market_id).await?;
    let row = latest(&a, market, "", Some(&key))
        .await?
        .pop()
        .ok_or_else(absent)?;
    if row.quality != "complete"
        || (row.source_kind != "full_scan" && !(a.config.is_demo() && row.source_kind == "demo"))
        || row.freshness_seconds > 2700
    {
        return Ok(Json(
            json!({"available":false,"listings":[],"reason":"latest_observation_has_no_fresh_qualifying_full_depth","market_id":market,"item_key":key,"observed_at":row.observed_at,"received_at":row.received_at,"source_kind":row.source_kind,"quality":row.quality,"sample_count":1,"unit":"copper_per_unit"}),
        ));
    }
    let rs=sqlx::query("SELECT stack_count,buyout_total_copper FROM listings WHERE scan_id=$1 AND item_key=$2 AND buyout_total_copper IS NOT NULL ORDER BY buyout_total_copper::numeric/stack_count,ordinal LIMIT 10001").bind(row.scan_id).bind(&key).fetch_all(&a.db).await?;
    if rs.len() > 10000 {
        return Err(bad("listing_depth_exceeds_display_limit"));
    }
    Ok(Json(
        json!({"available":true,"market_id":market,"item_key":key,"sample_count":1,"unit":"copper_per_unit","observed_at":row.observed_at,"received_at":row.received_at,"quality":row.quality,"source_kind":row.source_kind,"presence":row.presence,"listings":rs.iter().map(|r|{let n=r.get::<i64,_>("buyout_total_copper");let d=r.get::<i64,_>("stack_count");json!({"stack_count":d,"buyout_total_copper":n.to_string(),"unit_price":UnitPrice::new(n as u64,d as u64)})}).collect::<Vec<_>>()}),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reference_margin_deducts_fee_and_preserves_fractional_and_negative_values() {
        let margin = reference_margin(
            UnitPrice::new(200, 3).unwrap(),
            UnitPrice::new(100, 1).unwrap(),
            95,
        )
        .unwrap();
        assert_eq!(
            crate::crafting::compare_amounts(
                margin,
                market_analysis::crafting::Amount {
                    numerator: 85,
                    denominator: 3
                }
            ),
            std::cmp::Ordering::Equal
        );
        let loss = reference_margin(
            UnitPrice::new(100, 1).unwrap(),
            UnitPrice::new(100, 1).unwrap(),
            95,
        )
        .unwrap();
        assert_eq!(loss.numerator / loss.denominator, -5);
        assert!(reference_margin(
            UnitPrice::new(u64::MAX, u64::MAX).unwrap(),
            UnitPrice::new(u64::MAX, u64::MAX).unwrap(),
            95
        )
        .is_none());
    }
    #[test]
    fn ranking_accounts_for_evidence_and_not_only_discount() {
        let strong = opportunity_score(-40.0, 7, 60, 100);
        assert!(strong > opportunity_score(-60.0, 3, 2400, 1));
        assert!(strong > opportunity_score(-40.0, 3, 60, 100));
        assert!(strong > opportunity_score(-40.0, 7, 600, 100));
        assert!(strong > opportunity_score(-40.0, 7, 60, 1));
        assert_eq!(opportunity_score(-90.0, 7, 2700, 100), 0.0);
        assert_eq!(opportunity_score(20.0, 7, 0, 100), 0.0);
        assert_eq!(opportunity_score(-100.0, 7, 0, i64::MAX), 100.0);
    }
    #[test]
    fn category_requires_metadata_and_uses_item_class() {
        assert_eq!(category(None), None);
        assert_eq!(category(Some(7)), Some("materials"));
        assert_eq!(category(Some(9)), Some("recipes"));
        assert_eq!(category(Some(6)), Some("other"));
    }
    #[test]
    fn rejects_unbounded_or_unknown_queries() {
        let mut f = Filters {
            limit: Some(101),
            ..Default::default()
        };
        assert!(f.paging().is_err());
        f.limit = Some(100);
        f.cursor = Some("3000".into());
        assert_eq!(f.paging().unwrap(), (100, 3000));
        f.cursor = Some(usize::MAX.to_string());
        assert!(f.paging().is_err());
        f.cursor = Some("-1".into());
        assert!(f.paging().is_err());
        f.sort = Some("sql_injection".into());
        assert!(f.validate().is_err())
    }
    #[test]
    fn variant_keys_remain_exact() {
        assert!(key_valid("i123:456:-7"));
        assert_ne!("123", "i123:456:-7");
        assert!(!key_valid("a\nb"))
    }
    #[test]
    fn exact_discount_order_does_not_overflow() {
        assert_eq!(
            ratio_cmp(u128::MAX - 1, u128::MAX, u128::MAX - 2, u128::MAX - 1),
            std::cmp::Ordering::Greater
        );
        assert_eq!(ratio_cmp(1, 2, 2, 4), std::cmp::Ordering::Equal);
        assert_eq!(ratio_cmp(1, 3, 1, 2), std::cmp::Ordering::Less)
    }
}
