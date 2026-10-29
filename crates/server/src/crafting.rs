//! Shared discovered recipes and market-scoped production estimates.
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
use market_domain::{
    crafting::{ProfessionExport, Recipe},
    Listing,
};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::{Postgres, Row, Transaction};
use uuid::Uuid;

pub async fn store(
    tx: &mut Transaction<'_, Postgres>,
    upload: Uuid,
    export: &ProfessionExport,
) -> anyhow::Result<()> {
    for item in &export.recipe_items {
        let at = DateTime::from_timestamp(item.captured_at, 0)
            .ok_or_else(|| anyhow::anyhow!("invalid recipe item timestamp"))?;
        sqlx::query("INSERT INTO recipe_item_observations(product,build,build_number,item_id,upload_id,observed_at,recipe_spell_id,status) VALUES($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT DO NOTHING")
            .bind(&export.product).bind(&export.build).bind(export.build_number as i64)
            .bind(item.item_id as i64).bind(upload).bind(at).bind(item.recipe_spell_id.map(|id| id as i64))
            .bind(&item.status).execute(&mut **tx).await?;
    }
    for recipe in &export.recipes {
        let id = recipe.fingerprint(&export.product, &export.build);
        let at = DateTime::from_timestamp(recipe.captured_at, 0)
            .ok_or_else(|| anyhow::anyhow!("invalid recipe timestamp"))?;
        let facts = serde_json::to_value(recipe)?;
        sqlx::query("INSERT INTO professions(product,skill_line_id,name) VALUES($1,$2,$3) ON CONFLICT(product,skill_line_id) DO UPDATE SET name=excluded.name")
            .bind(&export.product).bind(recipe.profession_id as i64).bind(&recipe.profession_name).execute(&mut **tx).await?;
        sqlx::query("INSERT INTO recipe_versions(id,product,build,build_number,spell_id,profession_id,name,locale,kind,facts,first_observed_at,last_observed_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$11) ON CONFLICT(id) DO UPDATE SET last_observed_at=greatest(recipe_versions.last_observed_at,excluded.last_observed_at), first_observed_at=least(recipe_versions.first_observed_at,excluded.first_observed_at), name=CASE WHEN excluded.last_observed_at>=recipe_versions.last_observed_at THEN excluded.name ELSE recipe_versions.name END, locale=CASE WHEN excluded.last_observed_at>=recipe_versions.last_observed_at THEN excluded.locale ELSE recipe_versions.locale END, facts=CASE WHEN excluded.last_observed_at>=recipe_versions.last_observed_at THEN jsonb_set(excluded.facts,'{tradable}',coalesce(nullif(excluded.facts->'tradable','null'::jsonb),recipe_versions.facts->'tradable','null'::jsonb)) ELSE recipe_versions.facts END")
            .bind(&id).bind(&export.product).bind(&export.build).bind(export.build_number as i64)
            .bind(recipe.spell_id as i64).bind(recipe.profession_id as i64).bind(&recipe.name)
            .bind(&export.locale).bind(&recipe.kind).bind(&facts).bind(at).execute(&mut **tx).await?;
        for (&item, &quantity) in &recipe.reagents {
            sqlx::query("INSERT INTO recipe_reagents(recipe_id,item_id,quantity) VALUES($1,$2,$3) ON CONFLICT DO NOTHING")
                .bind(&id).bind(item as i64).bind(quantity as i64).execute(&mut **tx).await?;
        }
        if let Some(item) = recipe.output_item_id {
            sqlx::query("INSERT INTO recipe_outputs(recipe_id,item_id,min_quantity,max_quantity) VALUES($1,$2,$3,$4) ON CONFLICT DO NOTHING")
                .bind(&id).bind(item as i64).bind(recipe.min_output.map(|n|n as i64)).bind(recipe.max_output.map(|n|n as i64)).execute(&mut **tx).await?;
        }
        sqlx::query("INSERT INTO recipe_observations(recipe_id,upload_id,observed_at,addon_version,locale,facts) VALUES($1,$2,$3,$4,$5,$6) ON CONFLICT DO NOTHING")
            .bind(id).bind(upload).bind(at).bind(&export.addon_version).bind(&export.locale).bind(facts).execute(&mut **tx).await?;
    }
    Ok(())
}

#[derive(Deserialize, Default)]
pub struct Filters {
    q: Option<String>,
    profession_id: Option<i64>,
    build: Option<String>,
    limit: Option<i64>,
    offset: Option<i64>,
}
impl Filters {
    fn validate(&self) -> Result<()> {
        if self.q.as_ref().is_some_and(|q| q.len() > 200)
            || self.build.as_ref().is_some_and(|b| b.len() > 40)
            || self.profession_id.is_some_and(|n| n <= 0)
            || !(1..=100).contains(&self.limit.unwrap_or(30))
            || !(0..=20000).contains(&self.offset.unwrap_or(0))
        {
            return Err(ApiError(StatusCode::BAD_REQUEST, "invalid_recipe_filters"));
        }
        Ok(())
    }
}
const CANDIDATES: &str = r#"
WITH eligible AS (
  SELECT v.* FROM recipe_versions v
  WHERE product='wow_classic_beta' AND ($1::text IS NULL OR build=$1)
  AND NOT (v.facts->>'min_output' IS NULL AND EXISTS (
    SELECT 1 FROM recipe_versions known
    WHERE known.product=v.product AND known.build=v.build AND known.spell_id=v.spell_id
      AND known.profession_id=v.profession_id AND known.kind=v.kind
      AND known.facts->'reagents'=v.facts->'reagents'
      AND known.facts->'output_item_id' IS NOT DISTINCT FROM v.facts->'output_item_id'
      AND known.facts->>'min_output' IS NOT NULL
  ))
), ranked AS (
  SELECT v.*, count(*) OVER(PARTITION BY spell_id,build) AS variant_count,
    (SELECT count(DISTINCT o.facts->'tradable') > 1 FROM recipe_observations o
     WHERE o.recipe_id=v.id AND o.facts->>'tradable' IS NOT NULL) AS binding_conflict,
    row_number() OVER(PARTITION BY spell_id ORDER BY build_number DESC,last_observed_at DESC,id) AS position
  FROM eligible v
), selected AS (SELECT * FROM ranked WHERE position=1)
"#;
fn row_json(r: &sqlx::postgres::PgRow) -> Value {
    json!({"recipe_id":r.get::<String,_>("id"),"build":r.get::<String,_>("build"),
        "facts":r.get::<Value,_>("facts"),"conflicted":r.get::<i64,_>("variant_count")>1 || r.get::<bool,_>("binding_conflict"),
        "first_observed_at":r.get::<DateTime<Utc>,_>("first_observed_at"),
        "last_observed_at":r.get::<DateTime<Utc>,_>("last_observed_at")})
}
pub async fn professions(State(a): State<App>, h: HeaderMap) -> Result<Json<Value>> {
    auth::member(&a, &h, false).await?;
    let rows=sqlx::query("SELECT p.skill_line_id,p.name,count(DISTINCT v.spell_id) AS recipes FROM professions p JOIN recipe_versions v ON v.product=p.product AND v.profession_id=p.skill_line_id WHERE p.product='wow_classic_beta' GROUP BY p.skill_line_id,p.name ORDER BY p.name")
        .fetch_all(&a.db).await?;
    Ok(Json(json!(rows.iter().map(|r|json!({"id":r.get::<i64,_>("skill_line_id"),"name":r.get::<String,_>("name"),"recipe_count":r.get::<i64,_>("recipes")})).collect::<Vec<_>>())))
}

#[derive(Deserialize)]
pub struct DiscoveryFilters {
    build: Option<String>,
    limit: Option<i64>,
    offset: Option<i64>,
}
pub async fn recipe_items(
    State(a): State<App>,
    h: HeaderMap,
    Query(f): Query<DiscoveryFilters>,
) -> Result<Json<Value>> {
    auth::member(&a, &h, false).await?;
    if f.build.as_ref().is_some_and(|b| b.len() > 40)
        || !(1..=100).contains(&f.limit.unwrap_or(30))
        || !(0..=20000).contains(&f.offset.unwrap_or(0))
    {
        return Err(ApiError(
            StatusCode::BAD_REQUEST,
            "invalid_recipe_item_filters",
        ));
    }
    let rows = sqlx::query("WITH ranked AS (SELECT *,row_number() OVER(PARTITION BY item_id ORDER BY build_number DESC,(status='resolved') DESC,observed_at DESC,upload_id DESC) AS position FROM recipe_item_observations WHERE product='wow_classic_beta' AND ($1::text IS NULL OR build=$1)) SELECT *,count(*) OVER() AS total,count(*) FILTER(WHERE status<>'resolved') OVER() AS unresolved FROM ranked WHERE position=1 ORDER BY item_id LIMIT $2 OFFSET $3")
        .bind(f.build).bind(f.limit.unwrap_or(30)).bind(f.offset.unwrap_or(0)).fetch_all(&a.db).await?;
    Ok(Json(
        json!({"recipe_items":rows.iter().map(|r|json!({"item_id":r.get::<i64,_>("item_id"),"build":r.get::<String,_>("build"),"recipe_spell_id":r.get::<Option<i64>,_>("recipe_spell_id"),"status":r.get::<String,_>("status"),"observed_at":r.get::<DateTime<Utc>,_>("observed_at")})).collect::<Vec<_>>(),"total_count":rows.first().map(|r|r.get::<i64,_>("total")).unwrap_or(0),"unresolved_count":rows.first().map(|r|r.get::<i64,_>("unresolved")).unwrap_or(0)}),
    ))
}
pub async fn recipes(
    State(a): State<App>,
    h: HeaderMap,
    Query(f): Query<Filters>,
) -> Result<Json<Value>> {
    auth::member(&a, &h, false).await?;
    f.validate()?;
    let query=format!("{CANDIDATES} SELECT *,count(*) OVER() AS total FROM selected WHERE ($2::bigint IS NULL OR profession_id=$2) AND strpos(lower(name),lower($3))>0 ORDER BY lower(name),spell_id LIMIT $4 OFFSET $5");
    let rows = sqlx::query(&query)
        .bind(f.build)
        .bind(f.profession_id)
        .bind(f.q.unwrap_or_default())
        .bind(f.limit.unwrap_or(30))
        .bind(f.offset.unwrap_or(0))
        .fetch_all(&a.db)
        .await?;
    Ok(Json(
        json!({"recipes":rows.iter().map(row_json).collect::<Vec<_>>(),"total_count":rows.first().map(|r|r.get::<i64,_>("total")).unwrap_or(0),"coverage":"discovered_recipes"}),
    ))
}
pub async fn recipe(
    State(a): State<App>,
    h: HeaderMap,
    Path(spell): Path<i64>,
    Query(f): Query<Filters>,
) -> Result<Json<Value>> {
    auth::member(&a, &h, false).await?;
    f.validate()?;
    let query = format!("{CANDIDATES} SELECT * FROM selected WHERE spell_id=$2");
    let row = sqlx::query(&query)
        .bind(f.build)
        .bind(spell)
        .fetch_optional(&a.db)
        .await?
        .ok_or(ApiError(StatusCode::NOT_FOUND, "recipe_not_found"))?;
    Ok(Json(row_json(&row)))
}

#[derive(Deserialize)]
pub struct QuoteFilters {
    market_id: Option<Uuid>,
    build: Option<String>,
    crafts: Option<u64>,
    mode: Option<String>,
    fee_bps: Option<u64>,
    additional_cost_copper: Option<u64>,
}
pub async fn quote(
    State(a): State<App>,
    h: HeaderMap,
    Path(spell): Path<i64>,
    Query(f): Query<QuoteFilters>,
) -> Result<Json<Value>> {
    auth::member(&a, &h, false).await?;
    let market = read::scope(&a, f.market_id).await?;
    let (crafts, mode, fee, extra) = (
        f.crafts.unwrap_or(1),
        f.mode.as_deref().unwrap_or("reference"),
        f.fee_bps.unwrap_or(if a.config.house == "Neutral" {
            1500
        } else {
            500
        }),
        f.additional_cost_copper.unwrap_or(0),
    );
    if crafts == 0
        || crafts > 10000
        || fee > 10000
        || !["reference", "purchase"].contains(&mode)
        || f.build.as_ref().is_some_and(|b| b.len() > 40)
    {
        return Err(ApiError(StatusCode::BAD_REQUEST, "invalid_quote_options"));
    }
    let mut tx = a.db.begin().await?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
        .execute(&mut *tx)
        .await?;
    let query = format!("{CANDIDATES} SELECT * FROM selected WHERE spell_id=$2");
    let row = sqlx::query(&query)
        .bind(&f.build)
        .bind(spell)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(ApiError(StatusCode::NOT_FOUND, "recipe_not_found"))?;
    let metadata = row_json(&row);
    if metadata["conflicted"] == true {
        return Ok(Json(
            json!({"recipe":metadata,"status":"recipe_conflict","quote":null}),
        ));
    }
    let recipe: Recipe = serde_json::from_value(row.get("facts"))
        .map_err(|_| ApiError(StatusCode::SERVICE_UNAVAILABLE, "invalid_stored_recipe"))?;
    let now = a.config.read_now();
    let snapshot=sqlx::query("SELECT id,source_time,quality FROM scans WHERE market_id=$1 AND source_time<= $2 AND source_kind IN ('full_scan','demo') AND ((source_kind='demo')=$3) ORDER BY source_time DESC,id DESC LIMIT 1")
        .bind(market).bind(now).bind(a.config.is_demo()).fetch_optional(&mut *tx).await?;
    let Some(snapshot) = snapshot else {
        return Ok(Json(
            json!({"recipe":metadata,"status":"no_market_snapshot","quote":null}),
        ));
    };
    let scan: Uuid = snapshot.get("id");
    let mut keys: Vec<String> = recipe.reagents.keys().map(|id| format!("i{id}")).collect();
    if let Some(id) = recipe.output_item_id {
        keys.push(format!("i{id}"));
    }
    let rows=sqlx::query("SELECT item_key,stack_count,buyout_total_copper,min_bid_copper,current_bid_copper,time_left_band FROM listings WHERE scan_id=$1 AND item_key=ANY($2) AND buyout_total_copper>0 ORDER BY ordinal LIMIT 500001")
        .bind(scan).bind(keys).fetch_all(&mut *tx).await?;
    if rows.len() > 500000 {
        return Err(ApiError(
            StatusCode::UNPROCESSABLE_ENTITY,
            "snapshot_too_large",
        ));
    }
    let listings = rows
        .iter()
        .map(|r| Listing {
            item_key: r.get("item_key"),
            stack_count: r.get::<i64, _>("stack_count") as u64,
            buyout_total_copper: r
                .get::<Option<i64>, _>("buyout_total_copper")
                .map(|n| n as u64),
            min_bid_copper: r.get::<i64, _>("min_bid_copper") as u64,
            current_bid_copper: r.get::<i64, _>("current_bid_copper") as u64,
            time_left_band: r.get::<i32, _>("time_left_band") as u8,
        })
        .collect::<Vec<_>>();
    let result =
        market_analysis::crafting::calculate(&recipe, &listings, crafts, mode, fee, extra).ok_or(
            ApiError(StatusCode::UNPROCESSABLE_ENTITY, "quote_arithmetic_limit"),
        )?;
    let at: DateTime<Utc> = snapshot.get("source_time");
    let quality: String = snapshot.get("quality");
    let age = (now - at).num_seconds().max(0);
    tx.commit().await?;
    Ok(Json(
        json!({"recipe":metadata,"status":result.status,"quote":result,"market_id":market,"snapshot_id":scan,"observed_at":at,"freshness_seconds":age,"stale":age>3600,"quality":quality,"binding_unknown":recipe.tradable.is_none(),"assumptions":["Asking prices; sale is not guaranteed.","One snapshot; exact unmodified item keys only.","Auction fee is configurable. Additional costs apply to the whole batch.","Deposits, failed listings, vendor purchases and intermediate crafts are not included."]}),
    ))
}
