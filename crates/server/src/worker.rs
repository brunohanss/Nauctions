use crate::{ingest, read, App};
use chrono::{DateTime, Utc};
use market_domain::{ScanQuality, UnitPrice};
use serde_json::{json, Value};
use sqlx::{Postgres, Row, Transaction};
use std::time::Duration;
use uuid::Uuid;

pub async fn run(a: App) {
    let mut interval = tokio::time::interval(Duration::from_secs(2));
    let mut tick = 0u64;
    loop {
        interval.tick().await;
        for _ in 0..4 {
            match process_upload(&a).await {
                Ok(true) => {}
                Ok(false) => break,
                Err(_) => {
                    tracing::error!(
                        error_class = "ingest_worker",
                        "ingestion worker retry scheduled"
                    );
                    break;
                }
            }
        }
        if evaluate_job(&a).await.is_err() {
            tracing::error!(
                error_class = "alert_worker",
                "alert evaluation retry scheduled"
            );
        }
        if deliver(&a).await.is_err() {
            tracing::error!(error_class = "delivery_worker", "delivery worker failed");
        }
        tick += 1;
        if tick.is_multiple_of(1800) {
            let _ = retention(&a).await;
        }
    }
}
fn instant(ts: i64) -> anyhow::Result<DateTime<Utc>> {
    DateTime::from_timestamp(ts, 0).ok_or_else(|| anyhow::anyhow!("invalid timestamp"))
}
async fn ensure_item(tx: &mut Transaction<'_, Postgres>, key: &str) -> anyhow::Result<()> {
    let base = key
        .trim_start_matches('i')
        .split(|c: char| !c.is_ascii_digit())
        .next()
        .unwrap_or("")
        .parse::<i64>()?;
    sqlx::query(
        "INSERT INTO items(item_key,base_item_id,name) VALUES($1,$2,$1) ON CONFLICT DO NOTHING",
    )
    .bind(key)
    .bind(base)
    .execute(&mut **tx)
    .await?;
    Ok(())
}
async fn provenance(
    tx: &mut Transaction<'_, Postgres>,
    scan: Uuid,
    device: Uuid,
    upload: Uuid,
) -> anyhow::Result<()> {
    sqlx::query("INSERT INTO scan_sources(scan_id,device_id,upload_id) VALUES($1,$2,$3) ON CONFLICT DO NOTHING").bind(scan).bind(device).bind(upload).execute(&mut **tx).await?;
    Ok(())
}
pub async fn process_upload(a: &App) -> anyhow::Result<bool> {
    if a.config.is_demo() {
        return Ok(false);
    }
    let mut tx = a.db.begin().await?;
    let row=sqlx::query("SELECT id,device_id,staged_bytes FROM uploads WHERE status='received' ORDER BY received_at FOR UPDATE SKIP LOCKED LIMIT 1").fetch_optional(&mut *tx).await?;
    let Some(row) = row else { return Ok(false) };
    let upload: Uuid = row.get("id");
    let device: Uuid = row.get("device_id");
    let bytes: Vec<u8> = row.get("staged_bytes");
    // Keep the upload lock outside the savepoint. PostgreSQL data errors abort
    // normalization, but must not leave this upload at the head of the queue.
    sqlx::query("SAVEPOINT normalize_upload")
        .execute(&mut *tx)
        .await?;
    if let Err(error) = normalize_upload(a, &mut tx, upload, device, bytes).await {
        let invalid_data = error
            .downcast_ref::<sqlx::Error>()
            .and_then(|error| error.as_database_error())
            .and_then(|error| error.code())
            .is_some_and(|code| code.starts_with("22"));
        if !invalid_data {
            // Connection failures and other operational errors remain retryable.
            return Err(error);
        }
        sqlx::query("ROLLBACK TO SAVEPOINT normalize_upload")
            .execute(&mut *tx)
            .await?;
        sqlx::query(
            "UPDATE uploads SET status='rejected',error_code='invalid_saved_variables' WHERE id=$1",
        )
        .bind(upload)
        .execute(&mut *tx)
        .await?;
    }
    sqlx::query("RELEASE SAVEPOINT normalize_upload")
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(true)
}

async fn normalize_upload(
    a: &App,
    tx: &mut Transaction<'_, Postgres>,
    upload: Uuid,
    device: Uuid,
    bytes: Vec<u8>,
) -> anyhow::Result<()> {
    let data = ingest::decrypt(&a.config.staging_key, &bytes)?;
    let parsed = tokio::task::spawn_blocking(move || ahdb_format::parse(&data)).await?;
    let parsed = match parsed {
        Ok(p) => p,
        Err(e) => {
            let code = if e.to_string().contains("unsupported") {
                "unsupported_format"
            } else {
                "invalid_saved_variables"
            };
            sqlx::query("UPDATE uploads SET status='rejected',error_code=$1 WHERE id=$2")
                .bind(code)
                .bind(upload)
                .execute(&mut **tx)
                .await?;
            return Ok(());
        }
    };
    let market: Uuid = sqlx::query_scalar("SELECT id FROM markets WHERE realm=$1 AND house=$2")
        .bind(&a.config.realm)
        .bind(&a.config.house)
        .fetch_one(&mut **tx)
        .await?;
    // A synced SavedVariables file contains shared observations. Only its original
    // full scans are new contributions; later addon compaction cannot create a loop.
    let synced = |m: &market_domain::Market| {
        parsed
            .imported_history_markets
            .iter()
            .any(|s| s.realm == a.config.realm && s.house == m.house)
    };
    let recent_keys: Vec<String> = parsed
        .history
        .iter()
        .filter_map(|p| {
            p.item_key
                .trim_start_matches('i')
                .parse::<u64>()
                .ok()
                .map(|id| {
                    crate::export::history_fingerprint(
                        id,
                        p.observed_at,
                        p.min_unit_copper,
                        p.median_unit_copper,
                        p.quantity,
                    )
                })
        })
        .collect();
    let daily_keys: Vec<String> = parsed
        .daily
        .iter()
        .filter_map(|p| {
            p.item_key
                .trim_start_matches('i')
                .parse::<u64>()
                .ok()
                .map(|id| {
                    crate::export::daily_fingerprint(
                        id,
                        p.observed_at,
                        p.min_unit_copper,
                        p.median_unit_copper,
                        p.average_quantity,
                        p.scan_count,
                    )
                })
        })
        .collect();
    let exported_recent:std::collections::HashSet<String>=sqlx::query_scalar::<_,String>("SELECT fingerprint FROM export_history_fingerprints WHERE market_id=$1 AND kind='recent' AND fingerprint=ANY($2)").bind(market).bind(recent_keys).fetch_all(&mut **tx).await?.into_iter().collect();
    let exported_daily:std::collections::HashSet<String>=sqlx::query_scalar::<_,String>("SELECT fingerprint FROM export_history_fingerprints WHERE market_id=$1 AND kind='daily' AND fingerprint=ANY($2)").bind(market).bind(daily_keys).fetch_all(&mut **tx).await?.into_iter().collect();
    // Fail the entire payload before inserting any market rows when scope or time is invalid.
    let now = Utc::now().timestamp();
    let scope = |m: &market_domain::Market| {
        (m.realm.is_empty() || m.realm == a.config.realm) && m.house == a.config.house
    };
    let invalid = parsed
        .scans
        .iter()
        .any(|s| !scope(&s.market) || s.observed_at <= 0 || s.observed_at > now + 300)
        || parsed
            .history
            .iter()
            .any(|p| !scope(&p.market) || p.observed_at <= 0 || p.observed_at > now + 300)
        || parsed
            .daily
            .iter()
            .any(|p| !scope(&p.market) || p.observed_at <= 0 || p.observed_at > now + 300);
    if invalid {
        sqlx::query("UPDATE uploads SET status='rejected',error_code='market_or_time_not_allowed' WHERE id=$1").bind(upload).execute(&mut **tx).await?;
        return Ok(());
    }
    if parsed
        .scans
        .iter()
        .any(|s| market_analysis::snapshot(&s.listings).is_err())
    {
        sqlx::query(
            "UPDATE uploads SET status='rejected',error_code='invalid_snapshot_totals' WHERE id=$1",
        )
        .bind(upload)
        .execute(&mut **tx)
        .await?;
        return Ok(());
    }
    if parsed.scans.is_empty() && parsed.history.is_empty() && parsed.daily.is_empty() {
        sqlx::query("UPDATE uploads SET status='rejected',error_code='no_scan_data' WHERE id=$1")
            .bind(upload)
            .execute(&mut **tx)
            .await?;
        return Ok(());
    }
    for m in &parsed.items {
        let name = m
            .link
            .split("|h[")
            .nth(1)
            .and_then(|s| s.split("]|h").next())
            .unwrap_or(&m.item_key);
        sqlx::query("INSERT INTO items(item_key,base_item_id,name,metadata) VALUES($1,$2,$3,$4) ON CONFLICT(item_key) DO UPDATE SET name=excluded.name,metadata=excluded.metadata").bind(&m.item_key).bind(i64::try_from(m.base_item_id)?).bind(name).bind(serde_json::to_value(m)?).execute(&mut **tx).await?;
    }
    for s in parsed.scans {
        let quality = match s.quality {
            ScanQuality::Partial => "partial",
            ScanQuality::Complete => "complete",
            ScanQuality::Unknown if a.config.trust_scans => "complete",
            ScanQuality::Unknown => "unknown",
        };
        let new = Uuid::new_v4();
        let inserted=sqlx::query("INSERT INTO scans(id,market_id,fingerprint,source_time,format_version,quality,source_kind,realm_explicit) VALUES($1,$2,$3,$4,$5,$6,'full_scan',true) ON CONFLICT(market_id,fingerprint) DO NOTHING").bind(new).bind(market).bind(&s.fingerprint).bind(instant(s.observed_at)?).bind(s.format_version as i32).bind(quality).execute(&mut **tx).await?.rows_affected()>0;
        let id: Uuid =
            sqlx::query_scalar("SELECT id FROM scans WHERE market_id=$1 AND fingerprint=$2")
                .bind(market)
                .bind(&s.fingerprint)
                .fetch_one(&mut **tx)
                .await?;
        provenance(tx, id, device, upload).await?;
        if !inserted {
            continue;
        }
        let metrics = market_analysis::snapshot(&s.listings)?;
        for (key, m) in metrics {
            ensure_item(tx, &key).await?;
            sqlx::query("INSERT INTO item_observations(scan_id,item_key,min_buyout,median_buyout,total_units,listing_count) VALUES($1,$2,$3,$4,$5,$6)").bind(id).bind(key).bind(m.min_buyout.map(|p|json!(p))).bind(m.median_buyout.map(|p|json!(p))).bind(i64::try_from(m.total_units)?).bind(i64::try_from(m.listing_count)?).execute(&mut **tx).await?;
        }
        for (ordinal, l) in s.listings.into_iter().enumerate() {
            sqlx::query("INSERT INTO listings(scan_id,item_key,ordinal,stack_count,buyout_total_copper,min_bid_copper,current_bid_copper,time_left_band) VALUES($1,$2,$3,$4,$5,$6,$7,$8)").bind(id).bind(l.item_key).bind(ordinal as i32).bind(i64::try_from(l.stack_count)?).bind(l.buyout_total_copper.map(i64::try_from).transpose()?).bind(i64::try_from(l.min_bid_copper)?).bind(i64::try_from(l.current_bid_copper)?).bind(l.time_left_band as i32).execute(&mut **tx).await?;
        }
        sqlx::query("INSERT INTO jobs(id,job_type,payload) VALUES($1,'evaluate',$2)")
            .bind(Uuid::new_v4())
            .bind(json!({"scan_id":id,"market_id":market}))
            .execute(&mut **tx)
            .await?;
    }
    for p in parsed.history {
        if synced(&p.market)
            || p.item_key
                .trim_start_matches('i')
                .parse::<u64>()
                .ok()
                .is_some_and(|id| {
                    exported_recent.contains(&crate::export::history_fingerprint(
                        id,
                        p.observed_at,
                        p.min_unit_copper,
                        p.median_unit_copper,
                        p.quantity,
                    ))
                })
        {
            continue;
        }
        ensure_item(tx, &p.item_key).await?;
        let fingerprint = crate::auth::hash(&format!(
            "history-v1:{market}:{}",
            serde_json::to_string(&p)?
        ));
        let id = Uuid::new_v4();
        sqlx::query("INSERT INTO scans(id,market_id,fingerprint,source_time,format_version,quality,source_kind,realm_explicit) VALUES($1,$2,$3,$4,0,'history','ahdb_history',$5) ON CONFLICT DO NOTHING").bind(id).bind(market).bind(&fingerprint).bind(instant(p.observed_at)?).bind(!p.market.realm.is_empty()).execute(&mut **tx).await?;
        let id: Uuid =
            sqlx::query_scalar("SELECT id FROM scans WHERE market_id=$1 AND fingerprint=$2")
                .bind(market)
                .bind(fingerprint)
                .fetch_one(&mut **tx)
                .await?;
        provenance(tx, id, device, upload).await?;
        sqlx::query("INSERT INTO item_observations(scan_id,item_key,min_buyout,median_buyout,total_units) VALUES($1,$2,$3,$4,$5) ON CONFLICT DO NOTHING").bind(id).bind(p.item_key).bind(p.min_unit_copper.and_then(|n|UnitPrice::new(n,1)).map(|p|json!(p))).bind(p.median_unit_copper.and_then(|n|UnitPrice::new(n,1)).map(|p|json!(p))).bind(i64::try_from(p.quantity)?).execute(&mut **tx).await?;
    }
    for p in parsed.daily {
        if synced(&p.market)
            || p.item_key
                .trim_start_matches('i')
                .parse::<u64>()
                .ok()
                .is_some_and(|id| {
                    exported_daily.contains(&crate::export::daily_fingerprint(
                        id,
                        p.observed_at,
                        p.min_unit_copper,
                        p.median_unit_copper,
                        p.average_quantity,
                        p.scan_count,
                    ))
                })
        {
            continue;
        }
        ensure_item(tx, &p.item_key).await?;
        let value = serde_json::to_value(&p)?;
        let fingerprint = crate::auth::hash(&serde_json::to_string(&value)?);
        sqlx::query("INSERT INTO historic_daily_imports(market_id,item_key,utc_day,source_identity,point,realm_explicit) VALUES($1,$2,$3,$4,$5,$6) ON CONFLICT DO NOTHING").bind(market).bind(p.item_key).bind(p.utc_day).bind(fingerprint).bind(value).bind(!p.market.realm.is_empty()).execute(&mut **tx).await?;
    }
    sqlx::query("UPDATE uploads SET status='accepted' WHERE id=$1")
        .bind(upload)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

pub async fn evaluate_job(a: &App) -> anyhow::Result<bool> {
    let mut tx = a.db.begin().await?;
    // One evaluator at a time prevents suppression races between different scans.
    let locked: bool = sqlx::query_scalar("SELECT pg_try_advisory_xact_lock(7302401)")
        .fetch_one(&mut *tx)
        .await?;
    if !locked {
        return Ok(false);
    }
    let job=sqlx::query("SELECT id,payload FROM jobs WHERE status='pending' AND run_after<=now() ORDER BY run_after FOR UPDATE SKIP LOCKED LIMIT 1").fetch_optional(&mut *tx).await?;
    let Some(job) = job else { return Ok(false) };
    let jid: Uuid = job.get("id");
    let payload: Value = job.get("payload");
    let scan: Uuid = serde_json::from_value(payload["scan_id"].clone())?;
    let market: Uuid = serde_json::from_value(payload["market_id"].clone())?;
    let sr = sqlx::query("SELECT s.source_time,s.quality,s.source_kind,m.realm FROM scans s JOIN markets m ON m.id=s.market_id WHERE s.id=$1 AND s.market_id=$2")
        .bind(scan)
        .bind(market)
        .fetch_one(&mut *tx)
        .await?;
    let observed: DateTime<Utc> = sr.get("source_time");
    let now = Utc::now().timestamp();
    let superseded:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM scans WHERE market_id=$1 AND source_kind='full_scan' AND source_time>$2)").bind(market).bind(observed).fetch_one(&mut *tx).await?;
    if sr.get::<String, _>("source_kind") == "full_scan"
        && sr.get::<String, _>("realm") != crate::DEMO_REALM
        && !superseded
        && sr.get::<String, _>("quality") == "complete"
        && (0..=2700).contains(&(now - observed.timestamp()))
    {
        let rules=sqlx::query("SELECT r.*,o.min_buyout FROM alert_rules r JOIN members m ON m.id=r.member_id JOIN item_observations o ON o.item_key=r.item_key WHERE r.enabled AND m.status='active' AND o.scan_id=$1 AND o.min_buyout IS NOT NULL").bind(scan).fetch_all(&mut *tx).await?;
        for r in rules {
            let id: Uuid = r.get("id");
            let revision: i32 = r.get("revision");
            let key: String = r.get("item_key");
            let current: UnitPrice = serde_json::from_value(r.get::<Value, _>("min_buyout"))?;
            let baseline = read::baseline_for(a, market, &key, now)
                .await
                .map_err(|_| anyhow::anyhow!("baseline unavailable"))?;
            let kind: String = r.get("kind");
            let discount: i32 = r.get("discount_percent");
            let rule = match kind.as_str() {
                "threshold" => market_analysis::SignalRule::Threshold {
                    max_unit_price: UnitPrice::new(
                        r.get::<Option<i64>, _>("threshold_copper").unwrap_or(0) as u64,
                        1,
                    )
                    .unwrap(),
                },
                "movement" => market_analysis::SignalRule::Movement {
                    change_percent: discount as u8,
                },
                _ => market_analysis::SignalRule::Underpriced {
                    discount_percent: discount as u8,
                },
            };
            // Movement requires two complete seven-day windows with the same coverage policy.
            let previous = if kind == "movement" {
                let p = read::baseline_for(a, market, &key, now - 7 * 86400)
                    .await
                    .map_err(|_| anyhow::anyhow!("baseline unavailable"))?;
                if p.day_count >= 3 && p.day_count == baseline.day_count {
                    p.price
                } else {
                    None
                }
            } else {
                None
            };
            let signal = market_analysis::evaluate(
                market_analysis::SignalContext {
                    now,
                    observed_at: observed.timestamp(),
                    eligible: true,
                    current: if kind == "movement" {
                        baseline.price.unwrap_or(current)
                    } else {
                        current
                    },
                    baseline: &baseline,
                    previous,
                },
                &rule,
            );
            if signal.is_none() {
                continue;
            }
            let dedupe = format!("{id}:{revision}:{scan}:{key}");
            let already: bool =
                sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM alert_events WHERE dedupe_key=$1)")
                    .bind(&dedupe)
                    .fetch_one(&mut *tx)
                    .await?;
            if already {
                continue;
            }
            let last=sqlx::query("SELECT evidence FROM alert_events WHERE rule_id=$1 AND item_key=$2 AND created_at>now()-interval '1 hour' ORDER BY created_at DESC LIMIT 1").bind(id).bind(&key).fetch_optional(&mut *tx).await?;
            if let Some(last) = last {
                let v: Value = last.get("evidence");
                if let Ok(old) = serde_json::from_value::<UnitPrice>(v["current_price"].clone()) {
                    if !material_improvement(current, old) {
                        continue;
                    }
                } else {
                    continue;
                }
            }
            let eid = Uuid::new_v4();
            let evidence = json!({"market_id":market,"scan_id":scan,"item_key":key,"current_price":current,"baseline":baseline,"observed_at":observed,"freshness_seconds":now-observed.timestamp(),"method_version":"daily-median-v1","rule_revision":revision,"signal":signal,"unit":"copper_per_unit","dashboard_url":format!("{}/",a.config.origin)});
            sqlx::query("INSERT INTO alert_events(id,rule_id,scan_id,item_key,observed_at,evidence,dedupe_key) VALUES($1,$2,$3,$4,$5,$6,$7) ON CONFLICT DO NOTHING").bind(eid).bind(id).bind(scan).bind(key).bind(observed).bind(evidence).bind(dedupe).execute(&mut *tx).await?;
            if r.get::<bool, _>("notify") {
                sqlx::query(
                    "INSERT INTO deliveries(id,event_id) VALUES($1,$2) ON CONFLICT DO NOTHING",
                )
                .bind(Uuid::new_v4())
                .bind(eid)
                .execute(&mut *tx)
                .await?;
            }
        }
    }
    sqlx::query("UPDATE jobs SET status='complete' WHERE id=$1")
        .bind(jid)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(true)
}
fn material_improvement(current: UnitPrice, old: UnitPrice) -> bool {
    let left = current.numerator() as u128 * old.denominator() as u128;
    let right = old.numerator() as u128 * current.denominator() as u128;
    left <= (right / 10) * 9 + (right % 10) * 9 / 10
}
pub async fn deliver(a: &App) -> anyhow::Result<bool> {
    let Some(url) = a.config.webhook.as_ref().filter(|s| !s.is_empty()) else {
        return Ok(false);
    };
    let parsed = reqwest::Url::parse(url)?;
    anyhow::ensure!(
        parsed.scheme() == "https"
            && matches!(parsed.host_str(), Some("discord.com" | "discordapp.com"))
            && parsed.path().starts_with("/api/webhooks/"),
        "invalid Discord webhook endpoint"
    );
    deliver_to(a, url).await
}

// The production entry point validates the Discord endpoint before dispatch.
// Tests call this internal boundary with a local HTTP mock only.
pub(crate) async fn deliver_to(a: &App, url: &str) -> anyhow::Result<bool> {
    let mut tx = a.db.begin().await?;
    let row=sqlx::query("SELECT d.id,d.attempt_count,e.observed_at,e.evidence,r.enabled,m.status AS member_status,s.source_kind,sm.realm FROM deliveries d JOIN alert_events e ON e.id=d.event_id JOIN alert_rules r ON r.id=e.rule_id JOIN members m ON m.id=r.member_id JOIN scans s ON s.id=e.scan_id JOIN markets sm ON sm.id=s.market_id WHERE d.status='pending' AND d.run_after<=now() ORDER BY d.run_after FOR UPDATE OF d SKIP LOCKED LIMIT 1").fetch_optional(&mut *tx).await?;
    let Some(row) = row else { return Ok(false) };
    let id: Uuid = row.get("id");
    let observed: DateTime<Utc> = row.get("observed_at");
    let attempt: i32 = row.get("attempt_count");
    if row.get::<String, _>("source_kind") != "full_scan"
        || row.get::<String, _>("realm") == crate::DEMO_REALM
        || Utc::now().signed_duration_since(observed).num_seconds() > 2700
        || !row.get::<bool, _>("enabled")
        || row.get::<String, _>("member_status") != "active"
    {
        sqlx::query("UPDATE deliveries SET status='expired' WHERE id=$1")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        return Ok(true);
    }
    sqlx::query("UPDATE deliveries SET status='sending',attempt_count=attempt_count+1,run_after=now() WHERE id=$1").bind(id).execute(&mut *tx).await?;
    tx.commit().await?;
    let evidence: Value = row.get("evidence");
    let body = json!({"content":format!("Auction observation: {}\nObserved: {} (UTC); asking prices, not confirmed sales.\nCurrent copper/unit: {} / {}. Baseline: {}. Method: daily-median-v1.\n{}",evidence["item_key"].as_str().unwrap_or("item"),observed,evidence["current_price"]["numerator"],evidence["current_price"]["denominator"],evidence["baseline"]["price"],a.config.origin),"allowed_mentions":{"parse":[]}});
    let result = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .redirect(reqwest::redirect::Policy::none())
        .build()?
        .post(url)
        .json(&body)
        .send()
        .await;
    let (status, error, delay) = match result {
        Ok(r) if r.status().is_success() => ("sent", None, 0),
        Ok(r) if r.status().as_u16() == 429 && attempt < 5 => {
            let retry = r
                .headers()
                .get("retry-after")
                .and_then(|s| s.to_str().ok())
                .and_then(|s| s.parse::<i64>().ok())
                .unwrap_or(60)
                .clamp(1, 3600);
            ("pending", Some("discord_rate_limited"), retry)
        }
        Ok(r) if r.status().is_client_error() => ("failed", Some("discord_request_rejected"), 0),
        _ => ("uncertain", Some("delivery_outcome_unknown"), 0),
    };
    sqlx::query("UPDATE deliveries SET status=$1,last_error=$2,run_after=now()+make_interval(secs=>$3),sent_at=CASE WHEN $1='sent' THEN now() ELSE NULL END WHERE id=$4").bind(status).bind(error).bind(delay as f64).bind(id).execute(&a.db).await?;
    Ok(true)
}
pub async fn retention(a: &App) -> anyhow::Result<()> {
    let mut tx = a.db.begin().await?;
    sqlx::query("UPDATE uploads SET staged_bytes=NULL WHERE status<>'received' AND received_at<now()-interval '7 days'").execute(&mut *tx).await?;
    sqlx::query("DELETE FROM listings WHERE scan_id IN (SELECT id FROM scans WHERE source_kind<>'demo' AND source_time<now()-interval '30 days')").execute(&mut *tx).await?;
    sqlx::query("DELETE FROM sessions WHERE expires_at<now()")
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM rate_limits WHERE window_start<now()-interval '1 day'")
        .execute(&mut *tx)
        .await?;
    sqlx::query("UPDATE deliveries SET status='uncertain',last_error='worker_interrupted_during_send' WHERE status='sending' AND run_after<now()-interval '5 minutes'").execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(())
}
