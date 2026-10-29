use crate::{
    api::{ApiError, Result},
    auth, App,
};
use aes_gcm::{
    aead::{rand_core::RngCore, Aead, OsRng},
    Aes256Gcm, KeyInit,
};
use axum::{
    body::Bytes,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use serde_json::{json, Value};
use sqlx::Row;
use uuid::Uuid;
pub fn encrypt(key: &[u8; 32], data: &[u8]) -> anyhow::Result<Vec<u8>> {
    let cipher = Aes256Gcm::new_from_slice(key).unwrap();
    let mut nonce = [0u8; 12];
    OsRng.fill_bytes(&mut nonce);
    let mut out = nonce.to_vec();
    out.extend(
        cipher
            .encrypt((&nonce).into(), data)
            .map_err(|_| anyhow::anyhow!("staging encryption failed"))?,
    );
    Ok(out)
}
pub fn decrypt(key: &[u8; 32], data: &[u8]) -> anyhow::Result<Vec<u8>> {
    anyhow::ensure!(data.len() >= 28, "invalid staging data");
    Aes256Gcm::new_from_slice(key)
        .unwrap()
        .decrypt((&data[..12]).into(), &data[12..])
        .map_err(|_| anyhow::anyhow!("staging authentication failed"))
}
pub async fn capabilities() -> Result<Json<Value>> {
    Ok(Json(
        json!({"api_version":1,"max_upload_bytes":33554432,"max_file_size":33554432,"supported_formats":[6],"server_time":chrono::Utc::now(),"compression":["identity"]}),
    ))
}
pub async fn heartbeat(
    State(a): State<App>,
    h: HeaderMap,
    Json(p): Json<Value>,
) -> Result<Json<Value>> {
    let id = auth::device(&a, &h).await?;
    let safe = json!({"collector_version":p.get("collector_version").and_then(Value::as_str).filter(|s|s.len()<=40),"queue_count":p.get("queue_count").and_then(Value::as_u64),"scan_age_seconds":p.get("scan_age_seconds").and_then(Value::as_u64)});
    sqlx::query("UPDATE devices SET last_seen_at=now(),health=$1 WHERE id=$2")
        .bind(safe)
        .bind(id)
        .execute(&a.db)
        .await?;
    Ok(Json(json!({"ok":true})))
}
fn header<'a>(h: &'a HeaderMap, k: &str) -> Result<&'a str> {
    h.get(k)
        .and_then(|v| v.to_str().ok())
        .ok_or(ApiError(StatusCode::BAD_REQUEST, "missing_upload_header"))
}
pub async fn upload(
    State(a): State<App>,
    h: HeaderMap,
    bytes: Bytes,
) -> Result<(StatusCode, Json<Value>)> {
    if a.config.is_demo() {
        return Err(ApiError(StatusCode::FORBIDDEN, "demo_market_read_only"));
    }
    let device = auth::device(&a, &h).await?;
    auth::rate(&a, &format!("upload:{device}"), 10).await?;
    if bytes.is_empty() {
        return Err(ApiError(StatusCode::BAD_REQUEST, "empty_upload"));
    }
    if h.get("content-encoding").is_some_and(|v| v != "identity") {
        return Err(ApiError(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "unsupported_compression",
        ));
    }
    let upload_id = Uuid::parse_str(header(&h, "x-upload-id")?)
        .map_err(|_| ApiError(StatusCode::BAD_REQUEST, "invalid_upload_id"))?;
    let checksum = header(&h, "x-content-sha256")?;
    use sha2::{Digest, Sha256};
    let actual = hex::encode(Sha256::digest(&bytes));
    if checksum != actual {
        return Err(ApiError(StatusCode::BAD_REQUEST, "checksum_mismatch"));
    }
    let source = header(&h, "x-source-file-id")?;
    if source.len() != 64 || !source.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(ApiError(StatusCode::BAD_REQUEST, "invalid_source_id"));
    }
    let mut tx = a.db.begin().await?;
    let active:Option<Uuid>=sqlx::query_scalar("SELECT d.id FROM devices d JOIN members m ON m.id=d.member_id WHERE d.id=$1 AND d.revoked_at IS NULL AND m.status='active' FOR UPDATE OF d")
        .bind(device)
        .fetch_optional(&mut *tx)
        .await?;
    if active.is_none() {
        return Err(ApiError(StatusCode::FORBIDDEN, "device_revoked_or_invalid"));
    }
    let existing = sqlx::query(
        "SELECT status,error_code,checksum FROM uploads WHERE device_id=$1 AND upload_id=$2",
    )
    .bind(device)
    .bind(upload_id)
    .fetch_optional(&mut *tx)
    .await?;
    if let Some(r) = existing {
        if r.get::<String, _>("checksum") != checksum {
            return Err(ApiError(
                StatusCode::CONFLICT,
                "upload_id_checksum_conflict",
            ));
        }
        let status: String = r.get("status");
        return Ok((
            if status == "received" {
                StatusCode::ACCEPTED
            } else {
                StatusCode::OK
            },
            Json(
                json!({"upload_id":upload_id,"status":status,"error_code":r.get::<Option<String>,_>("error_code")}),
            ),
        ));
    }
    let pending:i64=sqlx::query_scalar("SELECT coalesce(sum(byte_count),0)::bigint FROM uploads WHERE device_id=$1 AND staged_bytes IS NOT NULL").bind(device).fetch_one(&mut *tx).await?;
    if pending + bytes.len() as i64 > 256 * 1024 * 1024 {
        return Err(ApiError(StatusCode::TOO_MANY_REQUESTS, "staging_quota"));
    }
    let encrypted = encrypt(&a.config.staging_key, &bytes)
        .map_err(|_| ApiError(StatusCode::INTERNAL_SERVER_ERROR, "staging_failed"))?;
    sqlx::query("INSERT INTO uploads(id,device_id,upload_id,checksum,byte_count,source_file_id,staged_bytes) VALUES($1,$2,$3,$4,$5,$6,$7)").bind(Uuid::new_v4()).bind(device).bind(upload_id).bind(checksum).bind(bytes.len() as i64).bind(source).bind(encrypted).execute(&mut *tx).await?;
    sqlx::query("INSERT INTO source_files(device_id,source_file_id,last_hash) VALUES($1,$2,$3) ON CONFLICT(device_id,source_file_id) DO UPDATE SET last_seen_at=now(),last_hash=excluded.last_hash").bind(device).bind(source).bind(checksum).execute(&mut *tx).await?;
    sqlx::query("UPDATE devices SET last_seen_at=now() WHERE id=$1")
        .bind(device)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok((
        StatusCode::ACCEPTED,
        Json(json!({"upload_id":upload_id,"status":"received","error_code":null})),
    ))
}
pub async fn receipt(
    State(a): State<App>,
    h: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>> {
    let device = auth::device(&a, &h).await?;
    let row =
        sqlx::query("SELECT status,error_code FROM uploads WHERE device_id=$1 AND upload_id=$2")
            .bind(device)
            .bind(id)
            .fetch_optional(&a.db)
            .await?
            .ok_or(ApiError(StatusCode::NOT_FOUND, "upload_not_found"))?;
    Ok(Json(
        json!({"upload_id":id,"status":row.get::<String,_>("status"),"error_code":row.get::<Option<String>,_>("error_code")}),
    ))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn staging_is_authenticated() {
        let key = [17; 32];
        let original = b"private source data";
        let mut c = encrypt(&key, original).unwrap();
        assert_ne!(&c[..original.len()], original);
        assert_eq!(decrypt(&key, &c).unwrap(), original);
        c[15] ^= 1;
        assert!(decrypt(&key, &c).is_err());
        assert!(decrypt(&[2; 32], &encrypt(&key, original).unwrap()).is_err());
    }
}
