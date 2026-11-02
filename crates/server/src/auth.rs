use crate::{
    api::{ApiError, Result},
    App,
};
use argon2::{
    password_hash::{PasswordHash, SaltString},
    Argon2, PasswordHasher, PasswordVerifier,
};
use axum::{
    extract::{Path, State},
    http::{header, HeaderMap, StatusCode},
    Json,
};
use rand::RngCore;
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::Row;
use uuid::Uuid;

pub fn secret() -> String {
    let mut b = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut b);
    hex::encode(b)
}
pub fn hash(s: &str) -> String {
    hex::encode(Sha256::digest(s.as_bytes()))
}
pub fn origin(a: &App, h: &HeaderMap) -> Result<()> {
    if h.get(header::ORIGIN).and_then(|v| v.to_str().ok()) != Some(a.config.origin.as_str()) {
        return Err(ApiError(StatusCode::FORBIDDEN, "origin_denied"));
    }
    Ok(())
}
pub struct Member {
    pub id: Uuid,
    pub name: String,
    pub role: String,
    pub csrf: String,
}
pub async fn member(a: &App, h: &HeaderMap, mutation: bool) -> Result<Member> {
    let token = h
        .get(header::COOKIE)
        .and_then(|s| s.to_str().ok())
        .and_then(|s| {
            s.split(';')
                .find_map(|p| p.trim().strip_prefix("market_session="))
        })
        .ok_or(ApiError(StatusCode::UNAUTHORIZED, "login_required"))?;
    let r=sqlx::query("SELECT m.id,m.display_name,m.role,s.csrf_token FROM sessions s JOIN members m ON m.id=s.member_id WHERE s.token_hash=$1 AND s.expires_at>now() AND m.status='active'").bind(hash(token)).fetch_optional(&a.db).await?.ok_or(ApiError(StatusCode::UNAUTHORIZED,"session_expired"))?;
    let m = Member {
        id: r.get("id"),
        name: r.get("display_name"),
        role: r.get("role"),
        csrf: r.get("csrf_token"),
    };
    // Remembered memberships renew while in use. Existing password sessions remain short-lived.
    sqlx::query("UPDATE sessions s SET expires_at=now()+interval '365 days' FROM members m WHERE s.token_hash=$1 AND s.persistent AND s.expires_at<now()+interval '180 days' AND m.id=s.member_id AND m.status='active'")
        .bind(hash(token)).execute(&a.db).await?;
    if mutation {
        origin(a, h)?;
        if h.get("x-csrf-token").and_then(|v| v.to_str().ok()) != Some(m.csrf.as_str()) {
            return Err(ApiError(StatusCode::FORBIDDEN, "csrf_denied"));
        }
    }
    Ok(m)
}
pub async fn admin(a: &App, h: &HeaderMap, mutation: bool) -> Result<Member> {
    let m = member(a, h, mutation).await?;
    if m.role != "admin" {
        return Err(ApiError(StatusCode::FORBIDDEN, "admin_required"));
    }
    Ok(m)
}
pub async fn device(a: &App, h: &HeaderMap) -> Result<Uuid> {
    let token = h
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or(ApiError(StatusCode::UNAUTHORIZED, "device_token_required"))?;
    let id=sqlx::query_scalar("SELECT d.id FROM devices d JOIN members m ON m.id=d.member_id WHERE d.credential_hash=$1 AND d.revoked_at IS NULL AND m.status='active'").bind(hash(token)).fetch_optional(&a.db).await?.ok_or(ApiError(StatusCode::FORBIDDEN,"device_revoked_or_invalid"))?;
    Ok(id)
}
async fn password_hash(p: String) -> Result<String> {
    if p.len() < 12 || p.len() > 256 {
        return Err(ApiError(
            StatusCode::BAD_REQUEST,
            "password_requires_12_to_256_characters",
        ));
    }
    tokio::task::spawn_blocking(move || {
        Argon2::default()
            .hash_password(p.as_bytes(), &SaltString::generate(&mut rand::rngs::OsRng))
            .map(|h| h.to_string())
            .map_err(|_| ApiError(StatusCode::INTERNAL_SERVER_ERROR, "password_error"))
    })
    .await
    .map_err(|_| ApiError(StatusCode::INTERNAL_SERVER_ERROR, "password_error"))?
}
pub async fn bootstrap(a: &App) -> anyhow::Result<()> {
    if let Ok(p) = std::env::var("BOOTSTRAP_ADMIN_PASSWORD") {
        let login = std::env::var("BOOTSTRAP_ADMIN_LOGIN").unwrap_or("admin".into());
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM members")
            .fetch_one(&a.db)
            .await?;
        if count == 0 {
            let ph = password_hash(p).await.map_err(|_| {
                anyhow::anyhow!("bootstrap password must contain 12 to 256 characters")
            })?;
            sqlx::query("INSERT INTO members(id,login,display_name,password_hash,role) VALUES($1,$2,'Administrator',$3,'admin')").bind(Uuid::new_v4()).bind(login).bind(ph).execute(&a.db).await?;
        }
    }
    Ok(())
}
pub async fn audit_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    actor: Uuid,
    action: &str,
    target: Uuid,
) -> Result<()> {
    sqlx::query("INSERT INTO audit_events(actor_id,action,target_id) VALUES($1,$2,$3)")
        .bind(actor)
        .bind(action)
        .bind(target)
        .execute(&mut **tx)
        .await?;
    Ok(())
}
fn valid_code(code: &str) -> bool {
    code.len() == 64 && code.bytes().all(|b| b.is_ascii_hexdigit())
}
// Database rate limits also apply across backend replicas.
pub async fn rate(a: &App, key: &str, max: i64) -> Result<()> {
    let n:i64=sqlx::query_scalar("INSERT INTO rate_limits(key,window_start,count) VALUES($1,to_timestamp(floor(extract(epoch from now())/60)*60),1) ON CONFLICT(key,window_start) DO UPDATE SET count=rate_limits.count+1 RETURNING count").bind(key).fetch_one(&a.db).await?;
    if n > max {
        return Err(ApiError(StatusCode::TOO_MANY_REQUESTS, "rate_limited"));
    }
    Ok(())
}
#[derive(Deserialize)]
pub struct Login {
    login: String,
    password: String,
}
// Four default Argon2 jobs use about 76 MiB for password work. Fail fast
// instead of storing an unbounded queue of passwords or blocking tasks.
static PASSWORD_WORK: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(4);
#[cfg(test)]
mod password_capacity_tests {
    use super::*;
    #[test]
    fn password_work_has_a_fixed_concurrent_bound() {
        let permits: Vec<_> = (0..4)
            .map(|_| PASSWORD_WORK.try_acquire().unwrap())
            .collect();
        assert!(PASSWORD_WORK.try_acquire().is_err());
        drop(permits);
        assert!(PASSWORD_WORK.try_acquire().is_ok());
    }
}
pub async fn login(
    State(a): State<App>,
    h: HeaderMap,
    Json(p): Json<Login>,
) -> Result<(HeaderMap, Json<Value>)> {
    origin(&a, &h)?;
    if p.login.is_empty() || p.login.len() > 128 || p.password.len() > 256 {
        return Err(ApiError(StatusCode::BAD_REQUEST, "invalid_credentials"));
    }
    rate(&a, "login_global", 300).await?;
    rate(&a, &format!("login:{}", hash(&p.login)), 15).await?;
    let r = sqlx::query("SELECT id,password_hash FROM members WHERE login=$1 AND status='active'")
        .bind(p.login)
        .fetch_optional(&a.db)
        .await?;
    let dummy="$argon2id$v=19$m=19456,t=2,p=1$c29tZXNhbHRzb21lc2FsdA$2STN7zVjFQXVFCgNPwvSZKrxvbeCUOgT0r77J8P3M+Q";
    let ph = r
        .as_ref()
        .map(|r| r.get::<String, _>("password_hash"))
        .unwrap_or(dummy.into());
    let permit = PASSWORD_WORK
        .try_acquire()
        .map_err(|_| ApiError(StatusCode::TOO_MANY_REQUESTS, "password_work_busy"))?;
    let valid = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        PasswordHash::new(&ph).is_ok_and(|v| {
            Argon2::default()
                .verify_password(p.password.as_bytes(), &v)
                .is_ok()
        })
    })
    .await
    .unwrap_or(false);
    if !valid || r.is_none() {
        return Err(ApiError(StatusCode::UNAUTHORIZED, "invalid_credentials"));
    }
    let id: Uuid = r.unwrap().get("id");
    let token = secret();
    let csrf = secret();
    let mut tx = a.db.begin().await?;
    // Serialize session creation with member disable, which also removes prior sessions.
    let active: Option<Uuid> =
        sqlx::query_scalar("SELECT id FROM members WHERE id=$1 AND status='active' FOR SHARE")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?;
    if active.is_none() {
        return Err(ApiError(StatusCode::UNAUTHORIZED, "invalid_credentials"));
    }
    sqlx::query("INSERT INTO sessions(token_hash,member_id,csrf_token,expires_at) VALUES($1,$2,$3,now()+interval '12 hours')").bind(hash(&token)).bind(id).bind(&csrf).execute(&mut *tx).await?;
    tx.commit().await?;
    let mut headers = HeaderMap::new();
    headers.insert(
        header::SET_COOKIE,
        format!(
            "market_session={token}; HttpOnly; SameSite=Strict; Path=/; Max-Age=43200{}",
            if a.config.insecure { "" } else { "; Secure" }
        )
        .parse()
        .unwrap(),
    );
    Ok((headers, Json(json!({"csrf_token":csrf}))))
}
pub async fn me(State(a): State<App>, h: HeaderMap) -> Result<Json<Value>> {
    let m = member(&a, &h, false).await?;
    Ok(Json(
        json!({"id":m.id,"display_name":m.name,"role":m.role,"csrf_token":m.csrf}),
    ))
}
const FRIEND_INVITATION_CODE: &str = "COPAINS";
#[derive(Deserialize)]
pub struct Join {
    nickname: String,
    code: String,
}
fn valid_nickname(nickname: &str) -> bool {
    (2..=32).contains(&nickname.chars().count())
        && nickname
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | '.'))
}
pub async fn join(
    State(a): State<App>,
    h: HeaderMap,
    Json(p): Json<Join>,
) -> Result<(HeaderMap, Json<Value>)> {
    origin(&a, &h)?;
    rate(&a, "friend_join", 20).await?;
    if !p.code.trim().eq_ignore_ascii_case(FRIEND_INVITATION_CODE) {
        return Err(ApiError(StatusCode::FORBIDDEN, "invalid_invitation_code"));
    }
    let nickname = p.nickname.trim();
    if !valid_nickname(nickname) {
        return Err(ApiError(StatusCode::BAD_REQUEST, "invalid_nickname"));
    }
    let login = format!("friend:{}", nickname.to_lowercase());
    let token = secret();
    let csrf = secret();
    // No user password is needed. The random unused hash keeps legacy schema compatibility.
    let ph = password_hash(secret()).await?;
    let mut tx = a.db.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
        .bind(&login)
        .execute(&mut *tx)
        .await?;
    let existing = sqlx::query(
        "SELECT id,login,role,status FROM members WHERE login=$1 OR lower(display_name)=lower($2) FOR UPDATE",
    )
    .bind(&login)
    .bind(nickname)
    .fetch_all(&mut *tx)
    .await?;
    let (id, event) = if existing.is_empty() {
        let id = Uuid::new_v4();
        sqlx::query("INSERT INTO members(id,login,display_name,password_hash,role) VALUES($1,$2,$3,$4,'member')").bind(id).bind(&login).bind(nickname).bind(ph).execute(&mut *tx).await?;
        (id, "friend_joined")
    } else {
        // The shared invitation code permits access to existing friend accounts.
        // Password accounts, administrators and disabled members cannot use it.
        if existing.len() != 1
            || existing[0].get::<String, _>("login") != login
            || existing[0].get::<String, _>("role") != "member"
        {
            return Err(ApiError(StatusCode::CONFLICT, "nickname_taken"));
        }
        if existing[0].get::<String, _>("status") != "active" {
            return Err(ApiError(StatusCode::FORBIDDEN, "member_disabled"));
        }
        (existing[0].get::<Uuid, _>("id"), "friend_reconnected")
    };
    sqlx::query("INSERT INTO sessions(token_hash,member_id,csrf_token,expires_at,persistent) VALUES($1,$2,$3,now()+interval '365 days',true)").bind(hash(&token)).bind(id).bind(&csrf).execute(&mut *tx).await?;
    audit_tx(&mut tx, id, event, id).await?;
    tx.commit().await?;
    let mut headers = HeaderMap::new();
    headers.insert(
        header::SET_COOKIE,
        format!(
            "market_session={token}; HttpOnly; SameSite=Strict; Path=/; Max-Age=31536000{}",
            if a.config.insecure { "" } else { "; Secure" }
        )
        .parse()
        .unwrap(),
    );
    Ok((
        headers,
        Json(json!({"csrf_token":csrf,"display_name":nickname})),
    ))
}
pub async fn logout(State(a): State<App>, h: HeaderMap) -> Result<(HeaderMap, Json<Value>)> {
    let m = member(&a, &h, true).await?;
    sqlx::query("DELETE FROM sessions WHERE member_id=$1 AND csrf_token=$2")
        .bind(m.id)
        .bind(m.csrf)
        .execute(&a.db)
        .await?;
    let mut hs = HeaderMap::new();
    hs.insert(
        header::SET_COOKIE,
        "market_session=; HttpOnly; SameSite=Strict; Path=/; Max-Age=0"
            .parse()
            .unwrap(),
    );
    Ok((hs, Json(json!({"ok":true}))))
}
#[derive(Deserialize)]
pub struct Invite {
    role: String,
}
pub async fn invite(
    State(a): State<App>,
    h: HeaderMap,
    Json(p): Json<Invite>,
) -> Result<Json<Value>> {
    let m = admin(&a, &h, true).await?;
    if !["member", "admin"].contains(&p.role.as_str()) {
        return Err(ApiError(StatusCode::BAD_REQUEST, "invalid_role"));
    }
    let code = secret();
    let exp = chrono::Utc::now() + chrono::Duration::hours(24);
    let mut tx = a.db.begin().await?;
    sqlx::query("INSERT INTO invitations(code_hash,role,expires_at) VALUES($1,$2,$3)")
        .bind(hash(&code))
        .bind(p.role)
        .bind(exp)
        .execute(&mut *tx)
        .await?;
    audit_tx(&mut tx, m.id, "invite_created", m.id).await?;
    tx.commit().await?;
    Ok(Json(json!({"code":code,"expires_at":exp})))
}
#[derive(Deserialize)]
pub struct Accept {
    code: String,
    login: String,
    display_name: String,
    password: String,
}
pub async fn accept_invite(
    State(a): State<App>,
    h: HeaderMap,
    Json(p): Json<Accept>,
) -> Result<Json<Value>> {
    origin(&a, &h)?;
    if !valid_code(&p.code) {
        return Err(ApiError(StatusCode::BAD_REQUEST, "invalid_invitation_code"));
    }
    rate(&a, "invite_accept", 20).await?;
    if p.login.is_empty()
        || p.login.len() > 128
        || p.display_name.is_empty()
        || p.display_name.len() > 80
    {
        return Err(ApiError(StatusCode::BAD_REQUEST, "invalid_member"));
    }
    let ph = password_hash(p.password).await?;
    let mut tx = a.db.begin().await?;
    let role:Option<String>=sqlx::query_scalar("UPDATE invitations SET used_at=now() WHERE code_hash=$1 AND used_at IS NULL AND expires_at>now() RETURNING role").bind(hash(&p.code)).fetch_optional(&mut *tx).await?;
    let role = role.ok_or(ApiError(StatusCode::BAD_REQUEST, "invite_expired_or_used"))?;
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO members(id,login,display_name,password_hash,role) VALUES($1,$2,$3,$4,$5)",
    )
    .bind(id)
    .bind(p.login)
    .bind(p.display_name)
    .bind(ph)
    .bind(role)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(Json(json!({"id":id})))
}
pub async fn pair_code(State(a): State<App>, h: HeaderMap) -> Result<Json<Value>> {
    let m = member(&a, &h, true).await?;
    rate(&a, &format!("pair:{}", m.id), 5).await?;
    let code = secret();
    let exp = chrono::Utc::now() + chrono::Duration::minutes(10);
    sqlx::query("INSERT INTO pairing_codes(code_hash,member_id,expires_at) VALUES($1,$2,$3)")
        .bind(hash(&code))
        .bind(m.id)
        .bind(exp)
        .execute(&a.db)
        .await?;
    Ok(Json(json!({"code":code,"expires_at":exp})))
}
#[derive(Deserialize)]
pub struct Enroll {
    code: String,
    display_name: String,
}
pub async fn enroll(State(a): State<App>, Json(p): Json<Enroll>) -> Result<Json<Value>> {
    if !valid_code(&p.code) {
        return Err(ApiError(StatusCode::BAD_REQUEST, "invalid_pairing_code"));
    }
    rate(&a, "enroll", 30).await?;
    if p.display_name.is_empty() || p.display_name.len() > 80 {
        return Err(ApiError(StatusCode::BAD_REQUEST, "invalid_device_name"));
    }
    let mut tx = a.db.begin().await?;
    let member:Option<Uuid>=sqlx::query_scalar("UPDATE pairing_codes p SET used_at=now() FROM members m WHERE p.member_id=m.id AND m.status='active' AND code_hash=$1 AND used_at IS NULL AND expires_at>now() RETURNING member_id").bind(hash(&p.code)).fetch_optional(&mut *tx).await?;
    let member = member.ok_or(ApiError(StatusCode::BAD_REQUEST, "pairing_expired_or_used"))?;
    let id = Uuid::new_v4();
    let token = secret();
    sqlx::query(
        "INSERT INTO devices(id,member_id,display_name,credential_hash) VALUES($1,$2,$3,$4)",
    )
    .bind(id)
    .bind(member)
    .bind(p.display_name)
    .bind(hash(&token))
    .execute(&mut *tx)
    .await?;
    audit_tx(&mut tx, member, "device_enrolled", id).await?;
    tx.commit().await?;
    Ok(Json(json!({"device_id":id,"token":token})))
}
pub async fn devices(State(a): State<App>, h: HeaderMap) -> Result<Json<Value>> {
    let m = member(&a, &h, false).await?;
    let rs=sqlx::query("SELECT id,display_name,last_seen_at,revoked_at,health,(SELECT max(received_at) FROM uploads u WHERE u.device_id=d.id AND u.status='accepted') AS last_accepted_at FROM devices d WHERE member_id=$1 ORDER BY created_at DESC").bind(m.id).fetch_all(&a.db).await?;
    Ok(Json(Value::Array(rs.iter().map(|r|json!({"id":r.get::<Uuid,_>("id"),"display_name":r.get::<String,_>("display_name"),"last_seen_at":r.get::<Option<chrono::DateTime<chrono::Utc>>,_>("last_seen_at"),"last_accepted_at":r.get::<Option<chrono::DateTime<chrono::Utc>>,_>("last_accepted_at"),"revoked_at":r.get::<Option<chrono::DateTime<chrono::Utc>>,_>("revoked_at"),"health":r.get::<Option<Value>,_>("health")})).collect())))
}
pub async fn revoke_device(
    State(a): State<App>,
    h: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>> {
    let m = member(&a, &h, true).await?;
    let mut tx = a.db.begin().await?;
    let n = sqlx::query("UPDATE devices SET revoked_at=now() WHERE id=$1 AND (member_id=$2 OR $3)")
        .bind(id)
        .bind(m.id)
        .bind(m.role == "admin")
        .execute(&mut *tx)
        .await?
        .rows_affected();
    if n == 0 {
        return Err(ApiError(StatusCode::NOT_FOUND, "device_not_found"));
    }
    audit_tx(&mut tx, m.id, "device_revoked", id).await?;
    tx.commit().await?;
    Ok(Json(json!({"ok":true})))
}
pub async fn members(State(a): State<App>, h: HeaderMap) -> Result<Json<Value>> {
    admin(&a, &h, false).await?;
    let rows =
        sqlx::query("SELECT id,login,display_name,role,status FROM members ORDER BY created_at")
            .fetch_all(&a.db)
            .await?;
    Ok(Json(Value::Array(rows.iter().map(|r|json!({"id":r.get::<Uuid,_>("id"),"login":r.get::<String,_>("login"),"display_name":r.get::<String,_>("display_name"),"role":r.get::<String,_>("role"),"status":r.get::<String,_>("status")})).collect())))
}
#[derive(Deserialize)]
pub struct MemberUpdate {
    status: String,
}
pub async fn member_update(
    State(a): State<App>,
    h: HeaderMap,
    Path(id): Path<Uuid>,
    Json(p): Json<MemberUpdate>,
) -> Result<Json<Value>> {
    let m = admin(&a, &h, true).await?;
    if id == m.id || !["active", "disabled"].contains(&p.status.as_str()) {
        return Err(ApiError(StatusCode::BAD_REQUEST, "invalid_member_status"));
    }
    let mut tx = a.db.begin().await?;
    let changed = sqlx::query("UPDATE members SET status=$1 WHERE id=$2")
        .bind(&p.status)
        .bind(id)
        .execute(&mut *tx)
        .await?
        .rows_affected();
    if changed == 0 {
        return Err(ApiError(StatusCode::NOT_FOUND, "member_not_found"));
    }
    if p.status == "disabled" {
        sqlx::query("DELETE FROM sessions WHERE member_id=$1")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM pairing_codes WHERE member_id=$1")
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }
    audit_tx(&mut tx, m.id, "member_status_changed", id).await?;
    tx.commit().await?;
    Ok(Json(json!({"ok":true})))
}

#[cfg(test)]
mod validation_tests {
    use super::*;
    #[test]
    fn nicknames_are_short_names_not_logins_or_markup() {
        for value in ["Hans", "Chloé", "Player_2", "Jean-Luc"] {
            assert!(valid_nickname(value));
        }
        for value in ["", "x", "<script>", "two names", "../", "line\nbreak"] {
            assert!(!valid_nickname(value));
        }
        assert!(!valid_nickname(&"x".repeat(33)));
    }
    #[test]
    fn credential_codes_are_fixed_size_random_hex() {
        let a = secret();
        let b = secret();
        assert_ne!(a, b);
        assert!(valid_code(&a));
        assert_eq!(hash(&a).len(), 64);
        assert_ne!(hash(&a), a);
        for code in [
            "".to_string(),
            "a".repeat(63),
            "a".repeat(65),
            "x".repeat(64),
            "\u{e9}".repeat(32),
        ] {
            assert!(!valid_code(&code));
        }
    }
}
