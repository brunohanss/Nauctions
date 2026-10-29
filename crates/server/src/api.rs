use crate::{auth, App};
use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::Row;
use uuid::Uuid;
pub type Result<T> = std::result::Result<T, ApiError>;
#[derive(Debug)]
pub struct ApiError(pub StatusCode, pub &'static str);
impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, Json(json!({"error":self.1,"code":self.1}))).into_response()
    }
}
impl From<sqlx::Error> for ApiError {
    fn from(e: sqlx::Error) -> Self {
        if let sqlx::Error::Database(ref db) = e {
            if db.is_unique_violation() {
                return Self(StatusCode::CONFLICT, "conflict");
            }
        }
        tracing::error!(error_class = "database", "database operation failed");
        Self(StatusCode::SERVICE_UNAVAILABLE, "database_unavailable")
    }
}
pub async fn health(State(a): State<App>) -> Result<Json<Value>> {
    sqlx::query("SELECT 1").execute(&a.db).await?;
    Ok(Json(
        json!({"status":"ok","version":env!("CARGO_PKG_VERSION")}),
    ))
}
pub async fn watchlists(State(a): State<App>, h: HeaderMap) -> Result<Json<Value>> {
    let m = auth::member(&a, &h, false).await?;
    let keys: Vec<String> =
        sqlx::query_scalar("SELECT item_key FROM watchlists WHERE member_id=$1 ORDER BY item_key")
            .bind(m.id)
            .fetch_all(&a.db)
            .await?;
    Ok(Json(json!(keys
        .into_iter()
        .map(|k| json!({"item_key":k}))
        .collect::<Vec<_>>())))
}
#[derive(Deserialize)]
pub struct Watch {
    item_key: String,
}
fn valid_key(k: &str) -> bool {
    !k.is_empty() && k.len() <= 200 && k.chars().all(|c| c.is_ascii_graphic())
}
pub async fn watch_add(
    State(a): State<App>,
    h: HeaderMap,
    Json(p): Json<Watch>,
) -> Result<Json<Value>> {
    let m = auth::member(&a, &h, true).await?;
    if !valid_key(&p.item_key) {
        return Err(ApiError(StatusCode::BAD_REQUEST, "invalid_item_key"));
    }
    auth::rate(&a, &format!("watch:{}", m.id), 30).await?;
    sqlx::query("INSERT INTO watchlists(member_id,item_key) VALUES($1,$2) ON CONFLICT DO NOTHING")
        .bind(m.id)
        .bind(p.item_key)
        .execute(&a.db)
        .await?;
    Ok(Json(json!({"ok":true})))
}
pub async fn watch_delete(
    State(a): State<App>,
    h: HeaderMap,
    Path(key): Path<String>,
) -> Result<Json<Value>> {
    let m = auth::member(&a, &h, true).await?;
    sqlx::query("DELETE FROM watchlists WHERE member_id=$1 AND item_key=$2")
        .bind(m.id)
        .bind(key)
        .execute(&a.db)
        .await?;
    Ok(Json(json!({"ok":true})))
}
pub fn rule_json(r: &sqlx::postgres::PgRow) -> Value {
    json!({"id":r.get::<Uuid,_>("id"),"item_key":r.get::<String,_>("item_key"),"kind":r.get::<String,_>("kind"),"revision":r.get::<i32,_>("revision"),"threshold_copper":r.get::<Option<i64>,_>("threshold_copper"),"discount_percent":r.get::<i32,_>("discount_percent"),"enabled":r.get::<bool,_>("enabled"),"notify":r.get::<bool,_>("notify")})
}
pub async fn rules(State(a): State<App>, h: HeaderMap) -> Result<Json<Value>> {
    let m = auth::member(&a, &h, false).await?;
    let r = sqlx::query("SELECT * FROM alert_rules WHERE member_id=$1 AND deleted_at IS NULL ORDER BY created_at DESC")
        .bind(m.id)
        .fetch_all(&a.db)
        .await?;
    Ok(Json(json!(r.iter().map(rule_json).collect::<Vec<_>>())))
}
#[derive(Deserialize, Default)]
pub struct Rule {
    item_key: Option<String>,
    kind: Option<String>,
    threshold_copper: Option<i64>,
    discount_percent: Option<i32>,
    enabled: Option<bool>,
    notify: Option<bool>,
}
fn validate_rule(key: &str, kind: &str, threshold: Option<i64>, discount: i32) -> Result<()> {
    if !valid_key(key)
        || !["underpriced", "threshold", "movement"].contains(&kind)
        || !(1..=99).contains(&discount)
        || (kind == "threshold" && threshold.is_none_or(|n| n <= 0))
    {
        return Err(ApiError(StatusCode::BAD_REQUEST, "invalid_rule"));
    }
    Ok(())
}
pub async fn rule_add(
    State(a): State<App>,
    h: HeaderMap,
    Json(p): Json<Rule>,
) -> Result<Json<Value>> {
    let m = auth::member(&a, &h, true).await?;
    auth::rate(&a, &format!("rules:{}", m.id), 20).await?;
    let key = p.item_key.unwrap_or_default();
    let kind = p.kind.unwrap_or_default();
    let discount = p.discount_percent.unwrap_or(30);
    validate_rule(&key, &kind, p.threshold_copper, discount)?;
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM alert_rules WHERE member_id=$1 AND deleted_at IS NULL",
    )
    .bind(m.id)
    .fetch_one(&a.db)
    .await?;
    if count >= 100 {
        return Err(ApiError(StatusCode::BAD_REQUEST, "rule_limit"));
    }
    let id = Uuid::new_v4();
    let r=sqlx::query("INSERT INTO alert_rules(id,member_id,item_key,kind,threshold_copper,discount_percent,enabled,notify) VALUES($1,$2,$3,$4,$5,$6,$7,$8) RETURNING *").bind(id).bind(m.id).bind(key).bind(kind).bind(p.threshold_copper).bind(discount).bind(p.enabled.unwrap_or(true)).bind(p.notify.unwrap_or(false)).fetch_one(&a.db).await?;
    Ok(Json(rule_json(&r)))
}
pub async fn rule_update(
    State(a): State<App>,
    h: HeaderMap,
    Path(id): Path<Uuid>,
    Json(p): Json<Rule>,
) -> Result<Json<Value>> {
    let m = auth::member(&a, &h, true).await?;
    let mut tx = a.db.begin().await?;
    let r = sqlx::query(
        "SELECT * FROM alert_rules WHERE id=$1 AND member_id=$2 AND deleted_at IS NULL FOR UPDATE",
    )
    .bind(id)
    .bind(m.id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(ApiError(StatusCode::NOT_FOUND, "rule_not_found"))?;
    let key = p.item_key.unwrap_or(r.get("item_key"));
    let kind = p.kind.unwrap_or(r.get("kind"));
    let threshold = p.threshold_copper.or(r.get("threshold_copper"));
    let discount = p.discount_percent.unwrap_or(r.get("discount_percent"));
    validate_rule(&key, &kind, threshold, discount)?;
    let row=sqlx::query("UPDATE alert_rules SET item_key=$1,kind=$2,threshold_copper=$3,discount_percent=$4,enabled=$5,notify=$6,revision=revision+1 WHERE id=$7 AND member_id=$8 AND deleted_at IS NULL RETURNING *").bind(key).bind(kind).bind(threshold).bind(discount).bind(p.enabled.unwrap_or(r.get("enabled"))).bind(p.notify.unwrap_or(r.get("notify"))).bind(id).bind(m.id).fetch_one(&mut *tx).await?;
    tx.commit().await?;
    Ok(Json(rule_json(&row)))
}
pub async fn rule_delete(
    State(a): State<App>,
    h: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>> {
    let m = auth::member(&a, &h, true).await?;
    let n = sqlx::query(
        "UPDATE alert_rules SET enabled=false,deleted_at=now(),revision=revision+1 WHERE id=$1 AND member_id=$2 AND deleted_at IS NULL",
    )
    .bind(id)
    .bind(m.id)
    .execute(&a.db)
    .await?
    .rows_affected();
    if n == 0 {
        return Err(ApiError(StatusCode::NOT_FOUND, "rule_not_found"));
    }
    Ok(Json(json!({"ok":true})))
}
pub async fn alerts(State(a): State<App>, h: HeaderMap) -> Result<Json<Value>> {
    let m = auth::member(&a, &h, false).await?;
    let rows=sqlx::query("SELECT e.id,e.item_key,e.observed_at,e.evidence,coalesce(d.status,'dashboard_only') AS status FROM alert_events e JOIN alert_rules r ON r.id=e.rule_id LEFT JOIN deliveries d ON d.event_id=e.id WHERE r.member_id=$1 ORDER BY e.created_at DESC LIMIT 200").bind(m.id).fetch_all(&a.db).await?;
    Ok(Json(json!(rows.iter().map(|r|json!({"id":r.get::<Uuid,_>("id"),"item_key":r.get::<String,_>("item_key"),"observed_at":r.get::<chrono::DateTime<chrono::Utc>,_>("observed_at"),"evidence":r.get::<Value,_>("evidence"),"status":r.get::<String,_>("status")})).collect::<Vec<_>>())))
}
pub async fn sources(State(a): State<App>, h: HeaderMap) -> Result<Json<Value>> {
    auth::admin(&a, &h, false).await?;
    let rows=sqlx::query("SELECT d.id,d.display_name,d.last_seen_at,d.health,(SELECT count(*) FROM uploads u WHERE u.device_id=d.id AND status='received') AS pending,(SELECT error_code FROM uploads u WHERE u.device_id=d.id AND error_code IS NOT NULL ORDER BY received_at DESC LIMIT 1) AS error_code FROM devices d ORDER BY d.created_at DESC LIMIT 100").fetch_all(&a.db).await?;
    Ok(Json(json!(rows.iter().map(|r|json!({"id":r.get::<Uuid,_>("id"),"display_name":r.get::<String,_>("display_name"),"last_seen_at":r.get::<Option<chrono::DateTime<chrono::Utc>>,_>("last_seen_at"),"health":r.get::<Option<Value>,_>("health"),"pending":r.get::<i64,_>("pending"),"error_code":r.get::<Option<String>,_>("error_code")})).collect::<Vec<_>>())))
}
