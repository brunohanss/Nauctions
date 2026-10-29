//! Authenticated, bounded export of asking-price history. No listings or seller data.
use crate::{
    api::{ApiError, Result},
    auth, App,
};
use axum::{
    extract::{Query, State},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
#[cfg(test)]
use market_domain::UnitPrice;
use market_domain::{DailyPoint, ExportDaily, ExportPoint, HistoryExport, Market};
use serde::Deserialize;
use sqlx::Row;
use std::collections::BTreeMap;
use uuid::Uuid;
const ROW_LIMIT: usize = 100_000;
const BYTE_LIMIT: usize = 32 * 1024 * 1024;
const LUA_MAX: u64 = 9_007_199_254_740_991;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Scope {
    realm: String,
    house: String,
    protocol: Option<u32>,
}
fn too_large() -> ApiError {
    ApiError(StatusCode::PAYLOAD_TOO_LARGE, "history_export_too_large")
}
fn corrupt() -> ApiError {
    ApiError(
        StatusCode::SERVICE_UNAVAILABLE,
        "history_export_invalid_data",
    )
}
pub fn history_fingerprint(
    item_id: u64,
    ts: i64,
    min: Option<u64>,
    median: Option<u64>,
    qty: u64,
) -> String {
    auth::hash(&format!(
        "history-export-v1:{item_id}:{ts}:{}:{}:{qty}",
        min.unwrap_or(0),
        median.unwrap_or(0)
    ))
}
pub fn daily_fingerprint(
    item_id: u64,
    ts: i64,
    min: Option<u64>,
    median: Option<u64>,
    qty: u64,
    scans: u64,
) -> String {
    auth::hash(&format!(
        "daily-export-v1:{}:{scans}",
        history_fingerprint(item_id, ts, min, median, qty)
    ))
}
fn base_key(key: &str, history: bool) -> Option<u64> {
    let digits = key.strip_prefix('i').or_else(|| history.then_some(key))?;
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    digits.parse::<u64>().ok().filter(|n| *n > 0)
}
#[cfg(test)]
fn price(value: Option<serde_json::Value>) -> Result<u64> {
    value
        .map(|v| {
            serde_json::from_value::<UnitPrice>(v)
                .map(|p| p.rounded_copper())
                .map_err(|_| corrupt())
        })
        .transpose()
        .map(|n| n.unwrap_or(0))
}
fn recent_day(now: i64, timestamps: impl Iterator<Item = i64>) -> i64 {
    let mut times: Vec<_> = timestamps.collect();
    times.sort_unstable();
    times.dedup();
    let mut day = now.div_euclid(86400) - 6;
    if times.len() >= 30 {
        day = day.max(times[times.len() - 30].div_euclid(86400));
    }
    day
}
#[cfg(test)]
fn rollup(id: u64, rows: &[ExportPoint]) -> ExportDaily {
    let mut meds: Vec<_> = rows
        .iter()
        .map(|p| p.median_unit_copper)
        .filter(|n| *n > 0)
        .collect();
    meds.sort_unstable();
    let n = rows.len() as u64;
    ExportDaily {
        item_id: id,
        observed_at: rows.last().unwrap().observed_at,
        min_unit_copper: rows
            .iter()
            .map(|p| p.min_unit_copper)
            .filter(|n| *n > 0)
            .min()
            .unwrap_or(0),
        median_unit_copper: if meds.is_empty() {
            0
        } else {
            meds[(meds.len() - 1) / 2]
        },
        quantity: ((rows.iter().map(|p| p.quantity as u128).sum::<u128>() + n as u128 / 2)
            / n as u128) as u64,
        scan_count: n,
    }
}
pub async fn history(
    State(a): State<App>,
    h: HeaderMap,
    Query(scope): Query<Scope>,
) -> Result<Response> {
    if a.config.is_demo() || scope.realm == crate::DEMO_REALM {
        return Err(ApiError(
            StatusCode::FORBIDDEN,
            "demo_history_not_exportable",
        ));
    }
    let device = auth::device(&a, &h).await?;
    auth::rate(&a, &format!("history-export:{device}"), 2).await?;
    if scope.realm.is_empty() || scope.realm != a.config.realm || scope.house != a.config.house {
        return Err(ApiError(StatusCode::BAD_REQUEST, "market_not_allowed"));
    }
    let now = chrono::Utc::now().timestamp();
    let oldest = (now.div_euclid(86400) - 179) * 86400;
    let mut tx = a.db.begin().await?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ")
        .execute(&mut *tx)
        .await?;
    let active:Option<Uuid>=sqlx::query_scalar("SELECT d.id FROM devices d JOIN members m ON m.id=d.member_id WHERE d.id=$1 AND d.revoked_at IS NULL AND m.status='active' FOR SHARE OF d,m").bind(device).fetch_optional(&mut *tx).await?;
    if active.is_none() {
        return Err(ApiError(StatusCode::FORBIDDEN, "device_revoked_or_invalid"));
    }
    if scope.protocol != Some(2) {
        return Err(ApiError(
            StatusCode::UPGRADE_REQUIRED,
            "history_sync_requires_protocol_2_upgrade_collector",
        ));
    }
    let market: Uuid =
        sqlx::query_scalar("SELECT id FROM markets WHERE realm=$1 AND house=$2 AND region='EU'")
            .bind(&scope.realm)
            .bind(&scope.house)
            .fetch_one(&mut *tx)
            .await?;
    // Derive the retained recent-day boundary before aggregating old observations.
    let times: Vec<chrono::DateTime<chrono::Utc>> = sqlx::query_scalar("SELECT DISTINCT source_time FROM scans WHERE market_id=$1 AND realm_explicit AND quality<>'partial' AND source_kind IN ('full_scan','ahdb_history') AND source_time>=to_timestamp($2) AND source_time<=to_timestamp($3) ORDER BY source_time DESC LIMIT 30")
        .bind(market).bind(oldest as f64).bind(now as f64).fetch_all(&mut *tx).await?;
    let day = recent_day(now, times.into_iter().map(|t| t.timestamp()));
    // Compact in PostgreSQL before the final response budget. 100,001 raw scans can
    // become a few daily rows; they must not fail solely because of raw row count.
    let rows=sqlx::query(r#"
        WITH canonical AS (
            SELECT DISTINCT ON (ltrim(regexp_replace(o.item_key,'^i',''),'0'),s.source_time)
                ltrim(regexp_replace(o.item_key,'^i',''),'0') AS item_key,s.source_time,
                CASE WHEN o.min_buyout IS NULL THEN 0 ELSE floor((o.min_buyout->>'numerator')::numeric/(o.min_buyout->>'denominator')::numeric+0.5) END AS minimum,
                CASE WHEN o.median_buyout IS NULL THEN 0 ELSE floor((o.median_buyout->>'numerator')::numeric/(o.median_buyout->>'denominator')::numeric+0.5) END AS median,
                o.total_units AS quantity
            FROM scans s JOIN item_observations o ON o.scan_id=s.id
            WHERE s.market_id=$1 AND s.realm_explicit AND s.quality<>'partial'
                AND s.source_kind IN ('full_scan','ahdb_history')
                AND ((s.source_kind='full_scan' AND o.item_key ~ '^i[0-9]+$') OR (s.source_kind='ahdb_history' AND o.item_key ~ '^i?[0-9]+$'))
                AND s.source_time>=to_timestamp($2) AND s.source_time<=to_timestamp($3)
            ORDER BY ltrim(regexp_replace(o.item_key,'^i',''),'0'),s.source_time,(s.source_kind='full_scan') DESC,s.received_at DESC,s.fingerprint DESC
        ), compact AS (
            SELECT item_key,source_time,minimum,median,quantity,0::bigint AS scan_count
            FROM canonical WHERE source_time>=to_timestamp($4)
            UNION ALL
            SELECT item_key,max(source_time),coalesce(min(minimum) FILTER(WHERE minimum>0),0),
                coalesce(percentile_disc(0.5) WITHIN GROUP(ORDER BY median) FILTER(WHERE median>0),0),
                floor(avg(quantity)+0.5)::bigint,count(*)
            FROM canonical WHERE source_time<to_timestamp($4)
            GROUP BY item_key,floor(extract(epoch FROM source_time)/86400)
        ) SELECT item_key,source_time,minimum::text,median::text,quantity,scan_count
          FROM compact ORDER BY item_key,source_time LIMIT 100001
    "#).bind(market).bind(oldest as f64).bind(now as f64).bind((day*86400) as f64).fetch_all(&mut *tx).await?;
    if rows.len() > ROW_LIMIT {
        return Err(too_large());
    }
    let mut points = Vec::new();
    let mut compact_daily = BTreeMap::new();
    for r in rows {
        let id = base_key(&r.get::<String, _>("item_key"), true).ok_or_else(corrupt)?;
        let ts = r
            .get::<chrono::DateTime<chrono::Utc>, _>("source_time")
            .timestamp();
        let minimum = r
            .get::<String, _>("minimum")
            .parse::<u64>()
            .map_err(|_| corrupt())?;
        let median = r
            .get::<String, _>("median")
            .parse::<u64>()
            .map_err(|_| corrupt())?;
        let quantity = u64::try_from(r.get::<i64, _>("quantity")).map_err(|_| corrupt())?;
        let count = u64::try_from(r.get::<i64, _>("scan_count")).map_err(|_| corrupt())?;
        if [id, minimum, median, quantity, count]
            .iter()
            .any(|v| *v > LUA_MAX)
        {
            return Err(too_large());
        }
        if count == 0 {
            points.push(ExportPoint {
                item_id: id,
                observed_at: ts,
                min_unit_copper: minimum,
                median_unit_copper: median,
                quantity,
            });
        } else {
            compact_daily.insert(
                (id, ts.div_euclid(86400)),
                ExportDaily {
                    item_id: id,
                    observed_at: ts,
                    min_unit_copper: minimum,
                    median_unit_copper: median,
                    quantity,
                    scan_count: count,
                },
            );
        }
    }
    let mut daily = compact_daily;
    let conflicting: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM historic_daily_imports WHERE market_id=$1 AND realm_explicit AND utc_day >= $2 AND utc_day < $3 AND item_key ~ '^i?[0-9]+$' GROUP BY ltrim(regexp_replace(item_key,'^i',''),'0'),utc_day HAVING count(DISTINCT point)>1)")
        .bind(market).bind(oldest.div_euclid(86400)).bind(day).fetch_one(&mut *tx).await?;
    if conflicting {
        return Err(ApiError(
            StatusCode::CONFLICT,
            "daily_history_coverage_conflict",
        ));
    }
    let rows=sqlx::query("SELECT DISTINCT ON (ltrim(regexp_replace(item_key,'^i',''),'0'),utc_day) item_key,utc_day,point FROM historic_daily_imports WHERE market_id=$1 AND realm_explicit AND utc_day >= $2 AND utc_day < $3 ORDER BY ltrim(regexp_replace(item_key,'^i',''),'0'),utc_day,(point->>'scan_count')::numeric DESC,received_at DESC,source_identity DESC LIMIT 100001").bind(market).bind(oldest.div_euclid(86400)).bind(day).fetch_all(&mut *tx).await?;
    if rows.len() > ROW_LIMIT {
        return Err(too_large());
    }
    for r in rows {
        let key: String = r.get("item_key");
        let Some(id) = base_key(&key, true) else {
            continue;
        };
        if id > LUA_MAX {
            return Err(too_large());
        }
        let d: i64 = r.get("utc_day");
        let p: DailyPoint = serde_json::from_value(r.get("point")).map_err(|_| corrupt())?;
        if p.observed_at.div_euclid(86400) != d
            || p.scan_count == 0
            || p.market.realm != scope.realm
            || p.market.house != scope.house
            || base_key(&p.item_key, true) != Some(id)
        {
            return Err(corrupt());
        }
        let p = ExportDaily {
            item_id: id,
            observed_at: p.observed_at,
            min_unit_copper: p.min_unit_copper.unwrap_or(0),
            median_unit_copper: p.median_unit_copper.unwrap_or(0),
            quantity: p.average_quantity,
            scan_count: p.scan_count,
        };
        if [
            p.quantity,
            p.min_unit_copper,
            p.median_unit_copper,
            p.scan_count,
        ]
        .iter()
        .any(|n| *n > LUA_MAX)
        {
            return Err(too_large());
        }
        if let Some(raw) = daily.get(&(id, d)) {
            if serde_json::to_value(raw).map_err(|_| corrupt())?
                != serde_json::to_value(&p).map_err(|_| corrupt())?
            {
                return Err(ApiError(
                    StatusCode::CONFLICT,
                    "daily_history_coverage_conflict",
                ));
            }
        } else {
            daily.insert((id, d), p);
        }
    }
    if points.len() + daily.len() > ROW_LIMIT {
        return Err(too_large());
    }
    let bundle = HistoryExport {
        schema_version: 1,
        market: Market {
            realm: scope.realm,
            house: scope.house,
        },
        generated_at: now,
        points,
        daily: daily.into_values().collect(),
    };
    let bytes = serde_json::to_vec(&market_domain::HistorySnapshot::new(bundle.clone()))
        .map_err(|_| corrupt())?;
    if bytes.len() > BYTE_LIMIT {
        return Err(too_large());
    }
    let mut kinds = Vec::new();
    let mut fingerprints = Vec::new();
    for (kind, _id, _ts, fp) in bundle
        .points
        .iter()
        .map(|p| {
            (
                "recent",
                p.item_id,
                p.observed_at,
                history_fingerprint(
                    p.item_id,
                    p.observed_at,
                    Some(p.min_unit_copper),
                    Some(p.median_unit_copper),
                    p.quantity,
                ),
            )
        })
        .chain(bundle.daily.iter().map(|p| {
            (
                "daily",
                p.item_id,
                p.observed_at,
                daily_fingerprint(
                    p.item_id,
                    p.observed_at,
                    Some(p.min_unit_copper),
                    Some(p.median_unit_copper),
                    p.quantity,
                    p.scan_count,
                ),
            )
        }))
    {
        kinds.push(kind.to_owned());
        fingerprints.push(fp);
    }
    sqlx::query("INSERT INTO export_history_fingerprints(market_id,kind,fingerprint) SELECT $1,k,f FROM unnest($2::text[],$3::text[]) AS t(k,f) ON CONFLICT DO NOTHING").bind(market).bind(kinds).bind(fingerprints).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok((
        [
            (header::CONTENT_TYPE, "application/json"),
            (header::CACHE_CONTROL, "no-store"),
        ],
        bytes,
    )
        .into_response())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn variants_are_excluded() {
        assert_eq!(base_key("i123", false), Some(123));
        assert_eq!(base_key("123", false), None);
        assert_eq!(base_key("i123:0", true), None);
        assert_eq!(base_key("123", true), Some(123));
    }
    #[test]
    fn half_up_prices() {
        assert_eq!(
            price(Some(serde_json::json!({"numerator":"5","denominator":"2"}))).unwrap(),
            3
        );
        assert_eq!(
            price(Some(
                serde_json::json!({"numerator":u64::MAX.to_string(),"denominator":"1"})
            ))
            .unwrap(),
            u64::MAX
        );
    }
    #[test]
    fn whole_day_retention() {
        assert_eq!(recent_day(100 * 86400, (0..31).map(|n| 98 * 86400 + n)), 98);
        assert_eq!(recent_day(100 * 86400, std::iter::empty()), 94);
    }
    #[test]
    fn fingerprints_canonicalize_missing() {
        assert_eq!(
            history_fingerprint(1, 2, None, None, 3),
            history_fingerprint(1, 2, Some(0), Some(0), 3)
        );
    }
    #[test]
    fn rollup_preserves_coverage_and_half_up_quantity() {
        let rows = vec![
            ExportPoint {
                item_id: 1,
                observed_at: 86401,
                min_unit_copper: 2,
                median_unit_copper: 9,
                quantity: 2,
            },
            ExportPoint {
                item_id: 1,
                observed_at: 86402,
                min_unit_copper: 4,
                median_unit_copper: 7,
                quantity: 3,
            },
        ];
        let d = rollup(1, &rows);
        assert_eq!(d.scan_count, 2);
        assert_eq!(d.observed_at, 86402);
        assert_eq!(d.min_unit_copper, 2);
        assert_eq!(d.median_unit_copper, 7);
        assert_eq!(d.quantity, 3);
    }
}
