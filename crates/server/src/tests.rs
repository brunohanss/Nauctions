//! Real PostgreSQL router tests. Each run uses a private random schema.
use super::*;
use axum::{
    body::{to_bytes, Body},
    http::{HeaderMap, Request, StatusCode},
};
use serde_json::{json, Value};

use tower::ServiceExt;
use uuid::Uuid;
struct Session {
    cookie: String,
    csrf: String,
}
async fn request(
    app: &App,
    method: &str,
    path: &str,
    body: Option<Value>,
    session: Option<&Session>,
    bearer: Option<&str>,
) -> (StatusCode, HeaderMap, Value) {
    let mut builder = Request::builder()
        .method(method)
        .uri(path)
        .header("origin", &app.config.origin);
    if let Some(s) = session {
        builder = builder
            .header("cookie", &s.cookie)
            .header("x-csrf-token", &s.csrf);
    }
    if let Some(token) = bearer {
        builder = builder.header("authorization", format!("Bearer {token}"));
    }
    let body = if let Some(body) = body {
        builder = builder.header("content-type", "application/json");
        Body::from(serde_json::to_vec(&body).unwrap())
    } else {
        Body::empty()
    };
    let response = router(app.clone())
        .oneshot(builder.body(body).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    let json = serde_json::from_slice(&bytes)
        .unwrap_or_else(|_| json!({"unparsed":String::from_utf8_lossy(&bytes)}));
    (status, headers, json)
}
async fn login(app: &App, login: &str, password: &str) -> Session {
    let (status, h, p) = request(
        app,
        "POST",
        "/v1/auth/login",
        Some(json!({"login":login,"password":password})),
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{p}");
    let cookie = h["set-cookie"].to_str().unwrap();
    assert!(
        cookie.contains("HttpOnly")
            && cookie.contains("SameSite=Strict")
            && cookie.contains("Secure")
    );
    Session {
        cookie: cookie.split(';').next().unwrap().into(),
        csrf: p["csrf_token"].as_str().unwrap().into(),
    }
}
async fn enroll(app: &App, session: &Session) -> (String, String) {
    let (status, _, p) = request(
        app,
        "POST",
        "/v1/devices/pair-code",
        None,
        Some(session),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{p}");
    let code = p["code"].as_str().unwrap();
    let (status, _, p) = request(
        app,
        "POST",
        "/v1/devices/enroll",
        Some(json!({"code":code,"display_name":"Test device"})),
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{p}");
    let token = p["token"].as_str().unwrap().to_owned();
    let device = p["device_id"].as_str().unwrap().to_owned();
    let (status, _, _) = request(
        app,
        "POST",
        "/v1/devices/enroll",
        Some(json!({"code":code,"display_name":"Replay"})),
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    (device, token)
}
async fn upload(app: &App, token: &str, id: Uuid, data: &[u8]) -> StatusCode {
    use sha2::{Digest, Sha256};
    let response = router(app.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/uploads")
                .header("authorization", format!("Bearer {token}"))
                .header("x-upload-id", id.to_string())
                .header("x-content-sha256", format!("{:x}", Sha256::digest(data)))
                .header("x-source-file-id", "f".repeat(64))
                .body(Body::from(data.to_vec()))
                .unwrap(),
        )
        .await
        .unwrap();
    response.status()
}
async fn count(app: &App, table: &str) -> i64 {
    assert!([
        "scans",
        "listings",
        "historic_daily_imports",
        "devices",
        "alert_events"
    ]
    .contains(&table));
    sqlx::query_scalar(&format!("SELECT count(*) FROM {table}"))
        .fetch_one(&app.db)
        .await
        .unwrap()
}
async fn exercise_invalid_metadata_queue(app: &App, token: &str) {
    let valid = include_bytes!("../../../fixtures/synthetic-v6.lua");
    let source = String::from_utf8(valid.to_vec()).unwrap();
    let catalog = || async {
        sqlx::query_scalar::<_, Value>(
            "SELECT coalesce(jsonb_agg(to_jsonb(i) ORDER BY item_key),'[]'::jsonb) FROM items i",
        )
        .fetch_one(&app.db)
        .await
        .unwrap()
    };
    for (original, malformed) in [
        ("Synthetic item", r"Bad\000name"),
        ("Synthetic variant", r"Bad\000variant"),
        ("enUS", r"en\000US"),
    ] {
        // Each case has a fresh upload budget in this isolated test schema.
        sqlx::query("DELETE FROM rate_limits WHERE key=(SELECT 'upload:' || id::text FROM devices WHERE credential_hash=$1)")
            .bind(auth::hash(token)).execute(&app.db).await.unwrap();
        let bad = source
            .replace(original, malformed)
            .replace("Synthetic item", "Uncommitted catalog change");
        assert!(ahdb_format::parse(bad.as_bytes()).is_ok());
        let before = catalog().await;
        let bad_id = Uuid::new_v4();
        let good_id = Uuid::new_v4();
        assert_eq!(
            upload(app, token, bad_id, bad.as_bytes()).await,
            StatusCode::ACCEPTED
        );
        assert_eq!(
            upload(app, token, good_id, valid).await,
            StatusCode::ACCEPTED
        );
        sqlx::query("UPDATE uploads SET received_at=now()-interval '1 minute' WHERE upload_id=$1")
            .bind(bad_id)
            .execute(&app.db)
            .await
            .unwrap();
        assert!(worker::process_upload(app).await.unwrap());
        let (_, _, receipt) = request(
            app,
            "GET",
            &format!("/v1/uploads/{bad_id}"),
            None,
            None,
            Some(token),
        )
        .await;
        assert_eq!(receipt["status"], "rejected");
        assert_eq!(receipt["error_code"], "invalid_saved_variables");
        assert_eq!(
            catalog().await,
            before,
            "failed payload must roll back every catalog write"
        );
        let retained: bool =
            sqlx::query_scalar("SELECT staged_bytes IS NOT NULL FROM uploads WHERE upload_id=$1")
                .bind(bad_id)
                .fetch_one(&app.db)
                .await
                .unwrap();
        assert!(retained, "retain rejected payload for diagnosis");
        assert!(worker::process_upload(app).await.unwrap());
        let (_, _, receipt) = request(
            app,
            "GET",
            &format!("/v1/uploads/{good_id}"),
            None,
            None,
            Some(token),
        )
        .await;
        assert_eq!(receipt["status"], "accepted");
        assert_eq!(
            count(app, "scans").await,
            4,
            "valid replay must not duplicate scans"
        );
        assert!(!worker::process_upload(app).await.unwrap());
    }

    // An operational database error must remain retryable, not reject good data.
    sqlx::query("CREATE FUNCTION audit_connection_error() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'synthetic connection error' USING ERRCODE='08006'; END $$")
        .execute(&app.db).await.unwrap();
    sqlx::query("CREATE TRIGGER audit_connection_error BEFORE INSERT OR UPDATE ON items FOR EACH ROW EXECUTE FUNCTION audit_connection_error()")
        .execute(&app.db).await.unwrap();
    let id = Uuid::new_v4();
    assert_eq!(upload(app, token, id, valid).await, StatusCode::ACCEPTED);
    assert!(worker::process_upload(app).await.is_err());
    let (_, _, receipt) = request(
        app,
        "GET",
        &format!("/v1/uploads/{id}"),
        None,
        None,
        Some(token),
    )
    .await;
    assert_eq!(receipt["status"], "received");
    let retained: bool =
        sqlx::query_scalar("SELECT staged_bytes IS NOT NULL FROM uploads WHERE upload_id=$1")
            .bind(id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert!(retained);
    sqlx::query("DROP TRIGGER audit_connection_error ON items")
        .execute(&app.db)
        .await
        .unwrap();
    sqlx::query("DROP FUNCTION audit_connection_error()")
        .execute(&app.db)
        .await
        .unwrap();
    assert!(worker::process_upload(app).await.unwrap());
    let (_, _, receipt) = request(
        app,
        "GET",
        &format!("/v1/uploads/{id}"),
        None,
        None,
        Some(token),
    )
    .await;
    assert_eq!(receipt["status"], "accepted");
}

async fn drain(app: &App) {
    for _ in 0..30 {
        if !worker::process_upload(app).await.unwrap() {
            return;
        }
    }
    panic!("worker did not drain bounded queue")
}
#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL; uses an isolated temporary schema"]
async fn authenticated_ingestion_and_revocation_flow() {
    let url = std::env::var("TEST_DATABASE_URL").expect("TEST_DATABASE_URL is required");
    let owner = PgPoolOptions::new()
        .max_connections(2)
        .connect(&url)
        .await
        .expect("connect test PostgreSQL");
    let schema = format!("nauctions_test_{}", Uuid::new_v4().simple());
    sqlx::query(&format!("CREATE SCHEMA {schema}"))
        .execute(&owner)
        .await
        .unwrap();
    let search_schema = schema.clone();
    let db = PgPoolOptions::new()
        .max_connections(5)
        .after_connect(move |connection, _| {
            let s = search_schema.clone();
            Box::pin(async move {
                sqlx::query(&format!("SET search_path TO {s}"))
                    .execute(connection)
                    .await?;
                Ok(())
            })
        })
        .connect(&url)
        .await
        .unwrap();
    crate::migrations::run(&db).await.unwrap();
    sqlx::query("UPDATE _sqlx_migrations SET checksum=$1 WHERE version=1")
        .bind(vec![0_u8; 48])
        .execute(&db)
        .await
        .unwrap();
    assert!(crate::migrations::run(&db).await.is_err());
    // Simulate a server first installed from a Windows checkout.
    for migration in crate::migrations::embedded().iter() {
        use sha2::{Digest, Sha384};
        let crlf = migration.sql.replace("\r\n", "\n").replace('\n', "\r\n");
        sqlx::query("UPDATE _sqlx_migrations SET checksum=$1 WHERE version=$2")
            .bind(Sha384::digest(crlf.as_bytes()).to_vec())
            .bind(migration.version)
            .execute(&db)
            .await
            .unwrap();
    }
    crate::migrations::run(&db).await.unwrap();

    let app = App {
        db,
        config: Arc::new(Config {
            demo_as_of: None,
            origin: "https://market.example".into(),
            insecure: false,
            realm: "Synthetic Realm".into(),
            house: "Alliance".into(),
            trust_scans: false,
            staging_key: [19; 32],
            webhook: None,
        }),
    };
    let test_app = app.clone();
    let outcome=tokio::spawn(async move {
    let app=test_app;
    let admin_id = Uuid::new_v4();
    let password = "Test-only-password-123";
    use argon2::{password_hash::SaltString, Argon2, PasswordHasher};
    let hash = Argon2::default()
        .hash_password(
            password.as_bytes(),
            &SaltString::generate(&mut rand::rngs::OsRng),
        )
        .unwrap()
        .to_string();
    sqlx::query("INSERT INTO members(id,login,display_name,password_hash,role) VALUES($1,'admin','Admin',$2,'admin')").bind(admin_id).bind(hash).execute(&app.db).await.unwrap();
    sqlx::query("INSERT INTO markets(id,realm,house) VALUES($1,$2,$3)")
        .bind(Uuid::new_v4())
        .bind(&app.config.realm)
        .bind(&app.config.house)
        .execute(&app.db)
        .await
        .unwrap();
    assert_eq!(
        request(&app, "GET", "/v1/items", None, None, None).await.0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        request(
            &app,
            "POST",
            "/v1/auth/login",
            Some(json!({"login":"admin","password":"wrong"})),
            None,
            None
        )
        .await
        .0,
        StatusCode::UNAUTHORIZED
    );
    let admin = login(&app, "admin", password).await;
    let bad_csrf = Session {
        cookie: admin.cookie.clone(),
        csrf: "wrong".into(),
    };
    assert_eq!(
        request(
            &app,
            "POST",
            "/v1/devices/pair-code",
            None,
            Some(&bad_csrf),
            None
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    let (status, _, p) = request(
        &app,
        "POST",
        "/v1/admin/invites",
        Some(json!({"role":"member"})),
        Some(&admin),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let code = p["code"].as_str().unwrap();
    let signup = json!({"code":code,"login":"friend","display_name":"Friend","password":password});
    let (status, _, p) = request(
        &app,
        "POST",
        "/v1/auth/accept-invite",
        Some(signup.clone()),
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{p}");
    let friend_id = p["id"].as_str().unwrap().to_owned();
    assert_eq!(
        request(
            &app,
            "POST",
            "/v1/auth/accept-invite",
            Some(signup),
            None,
            None
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    let friend = login(&app, "friend", password).await;
    assert_eq!(
        request(&app, "GET", "/v1/admin/members", None, Some(&friend), None)
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    let (device, token) = enroll(&app, &admin).await;
    let (_, foreign_token) = enroll(&app, &friend).await;
    assert_eq!(
        request(
            &app,
            "DELETE",
            &format!("/v1/devices/{device}"),
            None,
            Some(&friend),
            None
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    let data = include_bytes!("../../../fixtures/synthetic-v6.lua");
    let first = Uuid::new_v4();
    assert_eq!(
        upload(&app, &token, first, data).await,
        StatusCode::ACCEPTED
    );
    assert_eq!(
        upload(&app, &token, first, data).await,
        StatusCode::ACCEPTED
    );
    assert_eq!(
        upload(&app, &token, first, b"changed").await,
        StatusCode::CONFLICT
    );
    assert_eq!(
        request(
            &app,
            "GET",
            &format!("/v1/uploads/{first}"),
            None,
            None,
            Some(&foreign_token)
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    drain(&app).await;
    let (status, _, receipt) = request(
        &app,
        "GET",
        &format!("/v1/uploads/{first}"),
        None,
        None,
        Some(&token),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(receipt["status"], "accepted");
    assert_eq!(count(&app, "scans").await, 3);
    assert_eq!(count(&app, "listings").await, 3);
    assert_eq!(count(&app, "historic_daily_imports").await, 1);
    let metrics: Value =
        sqlx::query_scalar("SELECT min_buyout FROM item_observations WHERE item_key='i123'")
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(metrics, json!({"numerator":"20000","denominator":"20"}));
    assert_eq!(upload(&app, &token, first, data).await, StatusCode::OK);
    assert_eq!(
        upload(&app, &token, Uuid::new_v4(), data).await,
        StatusCode::ACCEPTED
    );
    drain(&app).await;
    assert_eq!(count(&app, "scans").await, 3);
    let changed = String::from_utf8(data.to_vec())
        .unwrap()
        .replace("20000,", "20001,");
    assert_eq!(
        upload(&app, &token, Uuid::new_v4(), changed.as_bytes()).await,
        StatusCode::ACCEPTED
    );
    drain(&app).await;
    assert_eq!(count(&app, "scans").await, 4);
    for (replace, with, error) in [
        (
            "dataFormatVersion=6",
            "dataFormatVersion=7",
            "unsupported_format",
        ),
        (
            "Synthetic Realm",
            "Wrong Realm",
            "market_or_time_not_allowed",
        ),
    ] {
        let data = String::from_utf8(data.to_vec())
            .unwrap()
            .replace(replace, with);
        let id = Uuid::new_v4();
        assert_eq!(
            upload(&app, &token, id, data.as_bytes()).await,
            StatusCode::ACCEPTED
        );
        drain(&app).await;
        let (_, _, receipt) = request(
            &app,
            "GET",
            &format!("/v1/uploads/{id}"),
            None,
            None,
            Some(&token),
        )
        .await;
        assert_eq!(receipt["status"], "rejected");
        assert_eq!(receipt["error_code"], error);
        assert_eq!(count(&app, "scans").await, 4);
    }
    exercise_invalid_metadata_queue(&app, &token).await;
    while worker::evaluate_job(&app).await.unwrap() {}
    assert_eq!(count(&app, "alert_events").await, 0);
    exercise_reads_signals(&app,&admin,&friend,&foreign_token).await;
    assert_eq!(
        request(
            &app,
            "DELETE",
            &format!("/v1/devices/{device}"),
            None,
            Some(&admin),
            None
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(
        request(
            &app,
            "GET",
            &format!("/v1/uploads/{first}"),
            None,
            None,
            Some(&token)
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    // If audit insertion fails, one-time enrollment must roll back with its code.
    let (status, _, p) = request(
        &app,
        "POST",
        "/v1/devices/pair-code",
        None,
        Some(&admin),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let code = p["code"].as_str().unwrap();
    let before = count(&app, "devices").await;
    sqlx::query("ALTER TABLE audit_events ADD CONSTRAINT test_audit_reject CHECK(action <> 'device_enrolled') NOT VALID").execute(&app.db).await.unwrap();
    let enroll_json = json!({"code":code,"display_name":"Audit rollback"});
    assert_eq!(
        request(
            &app,
            "POST",
            "/v1/devices/enroll",
            Some(enroll_json.clone()),
            None,
            None
        )
        .await
        .0,
        StatusCode::SERVICE_UNAVAILABLE
    );
    assert_eq!(count(&app, "devices").await, before);
    sqlx::query("ALTER TABLE audit_events DROP CONSTRAINT test_audit_reject")
        .execute(&app.db)
        .await
        .unwrap();
    assert_eq!(
        request(
            &app,
            "POST",
            "/v1/devices/enroll",
            Some(enroll_json),
            None,
            None
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(
        request(
            &app,
            "PATCH",
            &format!("/v1/admin/members/{friend_id}"),
            Some(json!({"status":"disabled"})),
            Some(&admin),
            None
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(
        request(&app, "GET", "/v1/me", None, Some(&friend), None)
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        request(
            &app,
            "PATCH",
            &format!("/v1/admin/members/{friend_id}"),
            Some(json!({"status":"active"})),
            Some(&admin),
            None
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(
        request(&app, "GET", "/v1/me", None, Some(&friend), None)
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
    exercise_friend_join(&app, &admin).await;
    exercise_forecasts(&app, &admin).await;
    let audit_count: i64 = sqlx::query_scalar("SELECT count(*) FROM audit_events")
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert!(audit_count >= 6);
    }).await;
    app.db.close().await;
    assert!(schema.starts_with("nauctions_test_") && schema.len() == 47);
    sqlx::query(&format!("DROP SCHEMA {schema} CASCADE"))
        .execute(&owner)
        .await
        .unwrap();
    owner.close().await;
    outcome.expect("integration flow must pass");
}

async fn read_json(app: &App, path: &str, session: &Session) -> Value {
    let (status, _, body) = request(app, "GET", path, None, Some(session), None).await;
    assert_eq!(status, StatusCode::OK, "{path}: {body}");
    body
}
async fn exercise_friend_join(app: &App, admin: &Session) {
    assert_eq!(
        request(
            app,
            "POST",
            "/v1/auth/join",
            Some(json!({"nickname":"NewFriend","code":"WRONG"})),
            None,
            None
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        request(
            app,
            "POST",
            "/v1/auth/join",
            Some(json!({"nickname":"<invalid>","code":"COPAINS"})),
            None,
            None
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    let response = router(app.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/auth/join")
                .header("origin", "https://other.example")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"nickname":"NewFriend","code":"COPAINS"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let (status, headers, body) = request(
        app,
        "POST",
        "/v1/auth/join",
        Some(json!({"nickname":"NewFriend","code":"COPAINS","role":"admin"})),
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let cookie = headers["set-cookie"].to_str().unwrap();
    assert!(
        cookie.contains("HttpOnly")
            && cookie.contains("Secure")
            && cookie.contains("Max-Age=31536000")
    );
    let session = Session {
        cookie: cookie.split(';').next().unwrap().into(),
        csrf: body["csrf_token"].as_str().unwrap().into(),
    };
    let me = read_json(app, "/v1/me", &session).await;
    assert_eq!(me["display_name"], "NewFriend");
    assert_eq!(me["role"], "member");
    let (status, headers, body) = request(
        app,
        "POST",
        "/v1/auth/join",
        Some(json!({"nickname":" newFRIEND ","code":"COPAINS"})),
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let reconnected = Session {
        cookie: headers["set-cookie"]
            .to_str()
            .unwrap()
            .split(';')
            .next()
            .unwrap()
            .into(),
        csrf: body["csrf_token"].as_str().unwrap().into(),
    };
    assert_ne!(reconnected.cookie, session.cookie);
    let restored = read_json(app, "/v1/me", &reconnected).await;
    for field in ["id", "display_name", "role"] {
        assert_eq!(restored[field], me[field]);
    }
    read_json(app, "/v1/me", &session).await;
    assert_eq!(
        request(
            app,
            "POST",
            "/v1/auth/join",
            Some(json!({"nickname":"NewFriend","code":"WRONG"})),
            None,
            None
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    for nickname in ["Admin"] {
        assert_eq!(
            request(
                app,
                "POST",
                "/v1/auth/join",
                Some(json!({"nickname":nickname,"code":"COPAINS"})),
                None,
                None
            )
            .await
            .0,
            StatusCode::CONFLICT
        );
    }
    sqlx::query("UPDATE sessions SET expires_at=now()+interval '1 day' WHERE member_id=$1")
        .bind(Uuid::parse_str(me["id"].as_str().unwrap()).unwrap())
        .execute(&app.db)
        .await
        .unwrap();
    read_json(app, "/v1/me", &session).await;
    let renewed: bool = sqlx::query_scalar("SELECT expires_at>now()+interval '300 days' AND persistent FROM sessions WHERE csrf_token=$1").bind(&session.csrf).fetch_one(&app.db).await.unwrap();
    assert!(renewed);
    let invalid = Session {
        cookie: session.cookie.clone(),
        csrf: "wrong".into(),
    };
    assert_eq!(
        request(
            app,
            "POST",
            "/v1/devices/pair-code",
            Some(json!({})),
            Some(&invalid),
            None
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        request(
            app,
            "PATCH",
            &format!("/v1/admin/members/{}", me["id"].as_str().unwrap()),
            Some(json!({"status":"disabled"})),
            Some(admin),
            None
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(
        request(app, "GET", "/v1/me", None, Some(&session), None)
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        request(app, "GET", "/v1/me", None, Some(&reconnected), None)
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        request(
            app,
            "POST",
            "/v1/auth/join",
            Some(json!({"nickname":"NewFriend","code":"COPAINS"})),
            None,
            None
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    let a = request(
        app,
        "POST",
        "/v1/auth/join",
        Some(json!({"nickname":"Concurrent","code":"COPAINS"})),
        None,
        None,
    );
    let b = request(
        app,
        "POST",
        "/v1/auth/join",
        Some(json!({"nickname":"concurrent","code":"COPAINS"})),
        None,
        None,
    );
    let (a, b) = tokio::join!(a, b);
    assert_eq!(a.0, StatusCode::OK, "{}", a.2);
    assert_eq!(b.0, StatusCode::OK, "{}", b.2);
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM members WHERE login='friend:concurrent'")
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(count, 1);
}
fn synthetic_scan(ts: i64, price: Option<u64>) -> String {
    let (count, items, data) = match price {
        Some(price) => (1, 1, format!("i555!/1,1,,{price},")),
        None => (0, 0, String::new()),
    };
    format!("AuctionDBForeverSaved={{ah={{{{realm=\"Synthetic Realm\",faction=\"Alliance\",ts={ts},count={count},itemsCount={items},dataFormatVersion=6,data=\"{data}\"}}}}}}")
}
async fn exercise_reads_signals(app: &App, admin: &Session, friend: &Session, upload_token: &str) {
    let unknown = read_json(app, "/v1/items?q=i123&sort=price&limit=1", admin).await;
    assert_eq!(unknown["items"].as_array().unwrap().len(), 1);
    let detail = read_json(app, "/v1/items/i123", admin).await;
    assert_eq!(detail["quality"], "unknown");
    assert_eq!(detail["presence"], "observed");
    assert_eq!(
        request(app, "GET", "/v1/strategies", None, None, None)
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
    let research = read_json(app, "/v1/strategies?q=i123", admin).await;
    assert_eq!(research["liquidity"], "unknown");
    assert!(research["items"][0]["heatmap"]
        .as_array()
        .unwrap()
        .is_empty());
    assert!(research["items"][0]["patterns"]
        .as_array()
        .unwrap()
        .iter()
        .all(|p| p["status"] == "insufficient"));
    assert_eq!(
        request(
            app,
            "GET",
            &format!("/v1/strategies?market_id={}", Uuid::new_v4()),
            None,
            Some(admin),
            None
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    let history = read_json(app, "/v1/items/i123/history?range=30d", admin).await;
    assert_eq!(history["points"].as_array().unwrap().len(), 2);
    assert_eq!(history["yesterday"]["timezone"], "Europe/Paris");
    assert_eq!(
        read_json(app, "/v1/items/i123/depth", admin).await["available"],
        false
    );
    assert!(read_json(app, "/v1/opportunities", admin).await["items"]
        .as_array()
        .unwrap()
        .is_empty());
    let bad_market = Uuid::new_v4();
    assert_eq!(
        request(
            app,
            "GET",
            &format!("/v1/items?market_id={bad_market}"),
            None,
            Some(admin),
            None
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    // Watchlist writes use the caller's member ID. Another member cannot change its rows.
    assert_eq!(
        request(
            app,
            "POST",
            "/v1/watchlists",
            Some(json!({"item_key":"i555"})),
            Some(admin),
            None
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(
        request(
            app,
            "DELETE",
            "/v1/watchlists/i555",
            None,
            Some(friend),
            None
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(
        read_json(app, "/v1/watchlists", admin)
            .await
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert!(read_json(app, "/v1/watchlists", friend)
        .await
        .as_array()
        .unwrap()
        .is_empty());
    // Filter the full result before paging, using only this member's saved items.
    assert_eq!(
        request(
            app,
            "POST",
            "/v1/watchlists",
            Some(json!({"item_key":"i123"})),
            Some(admin),
            None
        )
        .await
        .0,
        StatusCode::OK
    );
    let watched = read_json(app, "/v1/items?category=watchlist&limit=1", admin).await;
    assert_eq!(watched["total_count"], 1);
    assert_eq!(watched["items"][0]["item_key"], "i123");
    assert!(watched["next_cursor"].is_null());
    assert_eq!(
        read_json(app, "/v1/items?category=watchlist", friend).await["total_count"],
        0
    );
    assert_eq!(
        read_json(app, "/v1/items?category=watchlist&q=no-match", admin).await["total_count"],
        0
    );
    assert_eq!(
        request(
            app,
            "DELETE",
            "/v1/watchlists/i123",
            None,
            Some(admin),
            None
        )
        .await
        .0,
        StatusCode::OK
    );
    let trusted = App {
        db: app.db.clone(),
        config: Arc::new(Config {
            demo_as_of: None,
            origin: app.config.origin.clone(),
            insecure: false,
            realm: app.config.realm.clone(),
            house: app.config.house.clone(),
            trust_scans: true,
            staging_key: app.config.staging_key,
            webhook: None,
        }),
    };
    let now = chrono::Utc::now().timestamp();
    for days in 1..=3 {
        let data = synthetic_scan(now - days * 86400, Some(1000));
        assert_eq!(
            upload(&trusted, upload_token, Uuid::new_v4(), data.as_bytes()).await,
            StatusCode::ACCEPTED
        );
        drain(&trusted).await;
    }
    let(status,_,rule)=request(&trusted,"POST","/v1/alert-rules",Some(json!({"item_key":"i555","kind":"underpriced","discount_percent":30,"enabled":true,"notify":true})),Some(admin),None).await;
    assert_eq!(status, StatusCode::OK, "{rule}");
    let rule_id = rule["id"].as_str().unwrap();
    for method in ["PATCH", "DELETE"] {
        assert_eq!(
            request(
                &trusted,
                method,
                &format!("/v1/alert-rules/{rule_id}"),
                if method == "PATCH" {
                    Some(json!({"enabled":false}))
                } else {
                    None
                },
                Some(friend),
                None
            )
            .await
            .0,
            StatusCode::NOT_FOUND
        );
    }
    assert!(read_json(&trusted, "/v1/alert-rules", friend)
        .await
        .as_array()
        .unwrap()
        .is_empty());
    let data = synthetic_scan(now - 2, Some(500));
    assert_eq!(
        upload(&trusted, upload_token, Uuid::new_v4(), data.as_bytes()).await,
        StatusCode::ACCEPTED
    );
    drain(&trusted).await;
    let detail = read_json(&trusted, "/v1/items/i555", admin).await;
    assert_eq!(detail["quality"], "complete");
    assert_eq!(detail["baseline"]["day_count"], 3);
    assert_eq!(detail["discount_percentage"], -50.0);
    assert!(detail["opportunity_score"].as_f64().unwrap() > 0.0);
    assert_eq!(detail["freshness_status"], "fresh");
    assert!(detail["icon"].is_null());
    assert_eq!(
        detail["baseline"]["price"],
        json!({"numerator":"1000","denominator":"1"})
    );
    let history = read_json(&trusted, "/v1/items/i555/history?range=7d", admin).await;
    assert_eq!(history["points"].as_array().unwrap().len(), 4);
    assert_eq!(history["seven_day"]["observation_count"], 4);
    assert_eq!(
        history["seven_day"]["minimum"],
        json!({"numerator":"500","denominator":"1"})
    );
    assert_eq!(
        history["seven_day"]["maximum"],
        json!({"numerator":"1000","denominator":"1"})
    );
    assert_eq!(
        read_json(&trusted, "/v1/items/i555/history?range=all", admin).await["points"]
            .as_array()
            .unwrap()
            .len(),
        4
    );
    assert_eq!(
        history["yesterday"]["low"],
        json!({"numerator":"1000","denominator":"1"})
    );
    let depth = read_json(&trusted, "/v1/items/i555/depth", admin).await;
    assert_eq!(depth["available"], true);
    assert_eq!(depth["listings"].as_array().unwrap().len(), 1);
    assert_eq!(
        depth["listings"][0]["unit_price"],
        json!({"numerator":"500","denominator":"1"})
    );
    let opportunities = read_json(&trusted, "/v1/opportunities?q=i555", admin).await;
    assert_eq!(opportunities["items"].as_array().unwrap().len(), 1);
    assert_eq!(opportunities["total_count"], 1);
    let research = read_json(&trusted, "/v1/strategies?q=i555", admin).await;
    assert_eq!(research["items"].as_array().unwrap().len(), 1);
    assert!(!research["items"][0]["heatmap"]
        .as_array()
        .unwrap()
        .is_empty());
    assert!(research["items"][0]["patterns"]
        .as_array()
        .unwrap()
        .iter()
        .all(|p| p["action"] != "Review entry"));
    assert!(!opportunities["items"][0]["trend"]
        .as_array()
        .unwrap()
        .is_empty());
    let sorted = read_json(
        &trusted,
        "/v1/opportunities?q=i555&sort=quantity&direction=desc&category=all",
        admin,
    )
    .await;
    assert_eq!(sorted["items"][0]["item_key"], "i555");
    let unknown_category =
        read_json(&trusted, "/v1/opportunities?q=i555&category=recipes", admin).await;
    assert_eq!(unknown_category["total_count"], 0);
    while worker::evaluate_job(&trusted).await.unwrap() {}
    assert_eq!(count(&trusted, "alert_events").await, 1);
    let deliveries: i64 = sqlx::query_scalar("SELECT count(*) FROM deliveries")
        .fetch_one(&trusted.db)
        .await
        .unwrap();
    assert_eq!(deliveries, 1);
    for _ in 0..10 {
        sqlx::query("UPDATE jobs SET status='pending' WHERE payload->>'scan_id'=$1")
            .bind(detail["scan_id"].as_str().unwrap())
            .execute(&trusted.db)
            .await
            .unwrap();
        assert!(worker::evaluate_job(&trusted).await.unwrap());
    }
    assert_eq!(count(&trusted, "alert_events").await, 1);
    assert!(!worker::deliver(&trusted).await.unwrap()); // No configured webhook: no network request.
    let evidence = read_json(&trusted, "/v1/alerts", admin).await;
    assert_eq!(evidence.as_array().unwrap().len(), 1);
    assert_eq!(evidence[0]["evidence"]["baseline"]["day_count"], 3);
    assert!(read_json(&trusted, "/v1/alerts", friend)
        .await
        .as_array()
        .unwrap()
        .is_empty());
    // An old, cheaper scan is retained as evidence, but cannot create another alert.
    let stale = synthetic_scan(now - 3601, Some(400));
    assert_eq!(
        upload(&trusted, upload_token, Uuid::new_v4(), stale.as_bytes()).await,
        StatusCode::ACCEPTED
    );
    drain(&trusted).await;
    while worker::evaluate_job(&trusted).await.unwrap() {}
    assert_eq!(count(&trusted, "alert_events").await, 1);
    // Only an explicitly trusted newer full snapshot can establish absence.
    let absent = synthetic_scan(now - 1, None);
    assert_eq!(
        upload(&trusted, upload_token, Uuid::new_v4(), absent.as_bytes()).await,
        StatusCode::ACCEPTED
    );
    drain(&trusted).await;
    let absent = read_json(&trusted, "/v1/items/i555", admin).await;
    assert_eq!(absent["presence"], "absent_in_complete_scan");
    assert!(absent["min_buyout"].is_null());
    assert_eq!(absent["total_units"], 0);
    assert!(
        read_json(&trusted, "/v1/opportunities?q=i555", admin).await["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let depth = read_json(&trusted, "/v1/items/i555/depth", admin).await;
    assert_eq!(depth["presence"], "absent_in_complete_scan");
    assert!(depth["listings"].as_array().unwrap().is_empty());
    exercise_delivery_recovery(&trusted).await;
    // DELETE commits while PATCH waits on its row. PATCH must not restore stale enabled=true.
    let mut retiring = trusted.db.begin().await.unwrap();
    sqlx::query("UPDATE alert_rules SET enabled=false,deleted_at=now() WHERE id=$1")
        .bind(Uuid::parse_str(rule_id).unwrap())
        .execute(&mut *retiring)
        .await
        .unwrap();
    let patch_path = format!("/v1/alert-rules/{rule_id}");
    let patch = request(
        &trusted,
        "PATCH",
        &patch_path,
        Some(json!({"discount_percent":35})),
        Some(admin),
        None,
    );
    tokio::pin!(patch);
    assert!(tokio::time::timeout(Duration::from_millis(150), &mut patch)
        .await
        .is_err());
    retiring.commit().await.unwrap();
    assert_eq!(patch.await.0, StatusCode::NOT_FOUND);
    let retired: bool = sqlx::query_scalar(
        "SELECT NOT enabled AND deleted_at IS NOT NULL FROM alert_rules WHERE id=$1",
    )
    .bind(Uuid::parse_str(rule_id).unwrap())
    .fetch_one(&trusted.db)
    .await
    .unwrap();
    assert!(retired);
    let normalized = count(&trusted, "scans").await;
    sqlx::query("UPDATE uploads SET received_at=now()-interval '8 days' WHERE status<>'received'")
        .execute(&trusted.db)
        .await
        .unwrap();
    worker::retention(&trusted).await.unwrap();
    let raw: i64 =
        sqlx::query_scalar("SELECT count(*) FROM uploads WHERE staged_bytes IS NOT NULL")
            .fetch_one(&trusted.db)
            .await
            .unwrap();
    assert_eq!(raw, 0);
    assert_eq!(count(&trusted, "scans").await, normalized);
}

async fn mock_http_request(socket: &mut tokio::net::TcpStream) -> Value {
    use tokio::io::AsyncReadExt;
    let mut bytes = Vec::new();
    let mut buffer = [0u8; 2048];
    let header_end = loop {
        let n = socket.read(&mut buffer).await.unwrap();
        assert!(n > 0, "mock requires a request");
        bytes.extend_from_slice(&buffer[..n]);
        assert!(bytes.len() < 65536, "mock request bound");
        if let Some(i) = bytes.windows(4).position(|s| s == b"\r\n\r\n") {
            break i + 4;
        }
    };
    let headers = std::str::from_utf8(&bytes[..header_end]).unwrap();
    assert!(headers.starts_with("POST /mock-webhook HTTP/1.1"));
    let size: usize = headers
        .lines()
        .find_map(|line| {
            line.to_ascii_lowercase()
                .strip_prefix("content-length:")
                .map(|v| v.trim().parse().unwrap())
        })
        .expect("JSON content length");
    assert!(size < 32768);
    while bytes.len() < header_end + size {
        let n = socket.read(&mut buffer).await.unwrap();
        assert!(n > 0);
        bytes.extend_from_slice(&buffer[..n]);
    }
    let body: Value = serde_json::from_slice(&bytes[header_end..header_end + size]).unwrap();
    assert_eq!(body["allowed_mentions"]["parse"], json!([]));
    assert!(body["content"]
        .as_str()
        .unwrap()
        .contains("asking prices, not confirmed sales"));
    body
}
async fn delivery_row(app: &App, id: Uuid) -> (String, i32, Option<String>, bool) {
    use sqlx::Row;
    let r=sqlx::query("SELECT status,attempt_count,last_error,sent_at IS NOT NULL AS sent FROM deliveries WHERE id=$1").bind(id).fetch_one(&app.db).await.unwrap();
    (
        r.get("status"),
        r.get("attempt_count"),
        r.get("last_error"),
        r.get("sent"),
    )
}
async fn seeded_delivery(app: &App, base: Uuid, age_seconds: i64) -> Uuid {
    let event = Uuid::new_v4();
    let delivery = Uuid::new_v4();
    sqlx::query("INSERT INTO alert_events(id,rule_id,scan_id,item_key,observed_at,evidence,dedupe_key) SELECT $1,rule_id,scan_id,item_key,now()-make_interval(secs=>$2),evidence,$3 FROM alert_events WHERE id=$4").bind(event).bind(age_seconds as f64).bind(format!("test-delivery-{event}")).bind(base).execute(&app.db).await.unwrap();
    sqlx::query("INSERT INTO deliveries(id,event_id) VALUES($1,$2)")
        .bind(delivery)
        .bind(event)
        .execute(&app.db)
        .await
        .unwrap();
    delivery
}
async fn exercise_delivery_recovery(app: &App) {
    use sqlx::Row;
    use tokio::io::AsyncWriteExt;
    let row=sqlx::query("SELECT e.id AS event_id,d.id AS delivery_id FROM alert_events e JOIN deliveries d ON d.event_id=e.id ORDER BY e.created_at LIMIT 1").fetch_one(&app.db).await.unwrap();
    let event: Uuid = row.get("event_id");
    let delivery: Uuid = row.get("delivery_id");
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/mock-webhook", listener.local_addr().unwrap());
    let mock = tokio::spawn(async move {
        for code in [429, 204] {
            let (mut socket, _) = tokio::time::timeout(Duration::from_secs(5), listener.accept())
                .await
                .unwrap()
                .unwrap();
            let _ = mock_http_request(&mut socket).await;
            let response = if code == 429 {
                "HTTP/1.1 429 Too Many Requests\r\nContent-Length: 0\r\nRetry-After: 120\r\nConnection: close\r\n\r\n"
            } else {
                "HTTP/1.1 204 No Content\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            };
            socket.write_all(response.as_bytes()).await.unwrap();
            socket.shutdown().await.unwrap();
        }
    });
    assert!(worker::deliver_to(app, &url).await.unwrap());
    assert_eq!(
        delivery_row(app, delivery).await,
        (
            "pending".into(),
            1,
            Some("discord_rate_limited".into()),
            false
        )
    );
    let wait:f64=sqlx::query_scalar("SELECT extract(epoch FROM (run_after-now()))::double precision FROM deliveries WHERE id=$1").bind(delivery).fetch_one(&app.db).await.unwrap();
    assert!((110.0..=120.0).contains(&wait));
    assert!(!worker::deliver_to(app, &url).await.unwrap()); // Backoff is persisted, not a process-local timer.
    sqlx::query("UPDATE deliveries SET run_after=now()-interval '1 second' WHERE id=$1")
        .bind(delivery)
        .execute(&app.db)
        .await
        .unwrap();
    let restarted = app.clone();
    let (first, second) = tokio::join!(
        worker::deliver_to(&restarted, &url),
        worker::deliver_to(&restarted, &url)
    );
    assert_eq!(first.unwrap() as u8 + second.unwrap() as u8, 1);
    mock.await.unwrap();
    assert_eq!(
        delivery_row(app, delivery).await,
        ("sent".into(), 2, None, true)
    );
    for _ in 0..10 {
        assert!(!worker::deliver_to(app, &url).await.unwrap());
    }
    // A connection that closes after receiving a request has an unknown outcome.
    let uncertain = seeded_delivery(app, event, 0).await;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let loss_url = format!("http://{}/mock-webhook", listener.local_addr().unwrap());
    let loss = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let _ = mock_http_request(&mut socket).await;
        drop(socket);
        drop(listener);
    });
    assert!(worker::deliver_to(app, &loss_url).await.unwrap());
    loss.await.unwrap();
    assert_eq!(
        delivery_row(app, uncertain).await,
        (
            "uncertain".into(),
            1,
            Some("delivery_outcome_unknown".into()),
            false
        )
    );
    for _ in 0..10 {
        assert!(!worker::deliver_to(app, &loss_url).await.unwrap());
    }
    // Expired evidence is never sent, even when a delivery remains queued.
    let expired = seeded_delivery(app, event, 2701).await;
    assert!(worker::deliver_to(app, &loss_url).await.unwrap());
    assert_eq!(
        delivery_row(app, expired).await,
        ("expired".into(), 0, None, false)
    );
    // A queued retry after 429 must honor the current notification preference.
    let cancelled = seeded_delivery(app, event, 0).await;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let cancel_url = format!("http://{}/mock-webhook", listener.local_addr().unwrap());
    let limited = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let _ = mock_http_request(&mut socket).await;
        socket.write_all(b"HTTP/1.1 429 Too Many Requests\r\nContent-Length: 0\r\nRetry-After: 1\r\nConnection: close\r\n\r\n").await.unwrap();
    });
    assert!(worker::deliver_to(app, &cancel_url).await.unwrap());
    limited.await.unwrap();
    sqlx::query("UPDATE alert_rules SET notify=false WHERE id=(SELECT rule_id FROM alert_events WHERE id=$1)").bind(event).execute(&app.db).await.unwrap();
    sqlx::query("UPDATE deliveries SET run_after=now()-interval '1 second' WHERE id=$1")
        .bind(cancelled)
        .execute(&app.db)
        .await
        .unwrap();
    assert!(worker::deliver_to(app, &cancel_url).await.unwrap());
    assert_eq!(delivery_row(app, cancelled).await.0, "expired");
    assert_eq!(delivery_row(app, cancelled).await.1, 1);
    sqlx::query("UPDATE alert_rules SET notify=true WHERE id=(SELECT rule_id FROM alert_events WHERE id=$1)").bind(event).execute(&app.db).await.unwrap();
    // A crash after durable 'sending' is uncertain after recovery. It is not retried.
    let interrupted = seeded_delivery(app, event, 0).await;
    sqlx::query("UPDATE deliveries SET status='sending',attempt_count=1,run_after=now()-interval '6 minutes' WHERE id=$1").bind(interrupted).execute(&app.db).await.unwrap();
    worker::retention(app).await.unwrap();
    assert_eq!(
        delivery_row(app, interrupted).await,
        (
            "uncertain".into(),
            1,
            Some("worker_interrupted_during_send".into()),
            false
        )
    );
    assert!(!worker::deliver_to(app, &loss_url).await.unwrap());
}

async fn exercise_forecasts(base: &App, admin: &Session) {
    let app = App {
        db: base.db.clone(),
        config: Arc::new(Config {
            demo_as_of: None,
            origin: base.config.origin.clone(),
            insecure: false,
            realm: "Synthetic Forecast Realm".into(),
            house: "Horde".into(),
            trust_scans: false,
            staging_key: base.config.staging_key,
            webhook: None,
        }),
    };
    let market = Uuid::new_v4();
    sqlx::query("INSERT INTO markets(id,realm,house) VALUES($1,$2,'Horde')")
        .bind(market)
        .bind(&app.config.realm)
        .execute(&app.db)
        .await
        .unwrap();
    assert_eq!(
        request(&app, "GET", "/v1/forecasts", None, None, None)
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
    let path = format!("/v1/items/i1180/forecast?market_id={market}");
    let (status, _, empty) = request(&app, "GET", &path, None, Some(admin), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(empty["rolling"]["status"], "insufficient");
    assert!(empty.get("scenario").is_none());
    sqlx::query("INSERT INTO items(item_key,base_item_id,name) VALUES('i1180',1180,'Scroll of Stamina') ON CONFLICT DO NOTHING").execute(&app.db).await.unwrap();
    let now = chrono::Utc::now().timestamp() - 30;
    sqlx::query("INSERT INTO scans(id,market_id,fingerprint,source_time,format_version,quality,source_kind,realm_explicit) SELECT md5('forecast-'||n)::uuid,$1,'forecast-'||n,to_timestamp($2::double precision+n*3600),6,'complete','full_scan',true FROM generate_series(0,480) n")
      .bind(market).bind((now-20*86400) as f64).execute(&app.db).await.unwrap();
    sqlx::query("INSERT INTO item_observations(scan_id,item_key,min_buyout,median_buyout,total_units,listing_count) SELECT id,'i1180','{\"numerator\":\"900\",\"denominator\":\"1\"}'::jsonb,'{\"numerator\":\"1000\",\"denominator\":\"1\"}'::jsonb,100,10 FROM scans WHERE market_id=$1").bind(market).execute(&app.db).await.unwrap();
    let (status, _, live) = request(&app, "GET", &path, None, Some(admin), None).await;
    assert_eq!(status, StatusCode::OK, "{live}");
    assert_eq!(live["rolling"]["status"], "baseline");
    assert_eq!(live["rolling"]["price"]["numerator"], "1000");
    assert!(live["rolling"]["validation_pairs"].as_u64().unwrap() >= 8);
    let batch_body = json!({"market_id":market,"item_keys":["i1180","i1180"]});
    assert_eq!(
        request(
            &app,
            "POST",
            "/v1/forecasts/batch",
            Some(batch_body.clone()),
            None,
            None
        )
        .await
        .0,
        StatusCode::UNAUTHORIZED
    );
    for keys in [json!([]), json!(["bad key"]), json!(vec!["i1180"; 31])] {
        let (status, _, _) = request(
            &app,
            "POST",
            "/v1/forecasts/batch",
            Some(json!({"market_id":market,"item_keys":keys})),
            Some(admin),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }
    let (status, _, batch) = request(
        &app,
        "POST",
        "/v1/forecasts/batch",
        Some(batch_body.clone()),
        Some(admin),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{batch}");
    assert_eq!(batch["items"].as_array().unwrap().len(), 1);
    for field in [
        "status",
        "method",
        "price",
        "observations",
        "validation_pairs",
        "change_percent",
    ] {
        assert_eq!(batch["items"][0]["rolling"][field], live["rolling"][field]);
    }
    assert_eq!(
        batch["items"][0]["rise_frequencies"],
        live["rise_frequencies"]
    );
    assert_eq!(batch["items"][0]["current_median"], live["current_median"]);
    for frequency in live["rise_frequencies"].as_array().unwrap() {
        assert_eq!(frequency["percent"], 0.0);
        assert!(frequency["comparisons"].as_u64().unwrap() >= 10);
    }
    // Equal source times must choose the same canonical scan in history and anchor.
    let tied = Uuid::new_v4();
    sqlx::query("INSERT INTO scans(id,market_id,fingerprint,source_time,received_at,format_version,quality,source_kind,realm_explicit) VALUES($1,$2,'forecast-tied',to_timestamp($3),now()+interval '1 second',6,'complete','full_scan',true)")
        .bind(tied).bind(market).bind(now as f64).execute(&app.db).await.unwrap();
    sqlx::query("INSERT INTO item_observations(scan_id,item_key,median_buyout,total_units,listing_count) VALUES($1,'i1180','{\"numerator\":\"1200\",\"denominator\":\"1\"}'::jsonb,1,1)")
        .bind(tied).execute(&app.db).await.unwrap();
    let (_, _, tied_single) = request(&app, "GET", &path, None, Some(admin), None).await;
    let (_, _, tied_batch) = request(
        &app,
        "POST",
        "/v1/forecasts/batch",
        Some(batch_body.clone()),
        Some(admin),
        None,
    )
    .await;
    assert_eq!(tied_single["current_median"]["numerator"], "1200");
    assert_eq!(tied_single["rolling"]["price"]["numerator"], "1200");
    assert_eq!(
        tied_batch["items"][0]["rolling"]["price"],
        tied_single["rolling"]["price"]
    );
    sqlx::query("DELETE FROM item_observations WHERE scan_id=$1")
        .bind(tied)
        .execute(&app.db)
        .await
        .unwrap();
    sqlx::query("DELETE FROM scans WHERE id=$1")
        .bind(tied)
        .execute(&app.db)
        .await
        .unwrap();
    // A newer partial observation must suppress both the single and batch result.
    let partial = Uuid::new_v4();
    sqlx::query("INSERT INTO scans(id,market_id,fingerprint,source_time,format_version,quality,source_kind,realm_explicit) VALUES($1,$2,'forecast-partial',to_timestamp($3),6,'partial','full_scan',true)")
        .bind(partial).bind(market).bind((now+1) as f64).execute(&app.db).await.unwrap();
    sqlx::query("INSERT INTO item_observations(scan_id,item_key,median_buyout,total_units,listing_count) VALUES($1,'i1180','{\"numerator\":\"1000\",\"denominator\":\"1\"}'::jsonb,1,1)")
        .bind(partial).execute(&app.db).await.unwrap();
    let (_, _, partial_single) = request(&app, "GET", &path, None, Some(admin), None).await;
    let (_, _, partial_batch) = request(
        &app,
        "POST",
        "/v1/forecasts/batch",
        Some(batch_body.clone()),
        Some(admin),
        None,
    )
    .await;
    assert!(partial_single["current_median"].is_null());
    assert_eq!(
        partial_batch["items"][0]["rise_frequencies"],
        partial_single["rise_frequencies"]
    );
    assert!(partial_batch["items"][0]["rise_frequencies"]
        .as_array()
        .unwrap()
        .iter()
        .all(|f| f["percent"].is_null()));
    sqlx::query("DELETE FROM item_observations WHERE scan_id=$1")
        .bind(partial)
        .execute(&app.db)
        .await
        .unwrap();
    sqlx::query("DELETE FROM scans WHERE id=$1")
        .bind(partial)
        .execute(&app.db)
        .await
        .unwrap();
    let list_path = format!("/v1/forecasts?market_id={market}&q=1180");
    let (status, _, page) = request(&app, "GET", &list_path, None, Some(admin), None).await;
    assert_eq!(status, StatusCode::OK, "{page}");
    assert_eq!(page["total_count"], 1);
    assert_eq!(page["items"][0]["forecast"]["item_key"], "i1180");
    assert_eq!(
        page["items"][0]["forecast"]["rolling"]["status"],
        "baseline"
    );
    assert!(page["next_offset"].is_null());
    assert!(live.get("launch").is_none());
    assert!(live["expires_at"].as_i64().unwrap() <= now + 2700);
    // Search and market scope must not expose items from another market.
    let (_, _, no_match) = request(
        &app,
        "GET",
        &format!("/v1/forecasts?market_id={market}&q=no-such-item"),
        None,
        Some(admin),
        None,
    )
    .await;
    assert_eq!(no_match["total_count"], 0);
    assert_eq!(no_match["items"], json!([]));
    let (status, _, _) = request(
        &app,
        "GET",
        &format!("/v1/forecasts?market_id={}", Uuid::new_v4()),
        None,
        Some(admin),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _, _) = request(
        &app,
        "GET",
        &format!("/v1/forecasts?market_id={market}&offset=100001"),
        None,
        Some(admin),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let retired_tables: i64 = sqlx::query_scalar("SELECT count(*) FROM information_schema.tables WHERE table_schema=current_schema() AND table_name IN ('forecast_beta_runs','forecast_launch_settings')").fetch_one(&app.db).await.unwrap();
    assert_eq!(retired_tables, 0);
    // Exact variants must remain separate. Sparse history remains visible with its reason.
    sqlx::query("INSERT INTO items(item_key,base_item_id,name) VALUES('i1180:1',1180,'Scroll variant') ON CONFLICT DO NOTHING").execute(&app.db).await.unwrap();
    sqlx::query("INSERT INTO item_observations(scan_id,item_key,min_buyout,median_buyout,total_units,listing_count) SELECT id,'i1180:1',NULL,'{\"numerator\":\"1500\",\"denominator\":\"1\"}'::jsonb,1,1 FROM scans WHERE market_id=$1 ORDER BY source_time DESC LIMIT 1").bind(market).execute(&app.db).await.unwrap();
    let (_, _, variants) = request(&app, "GET", &list_path, None, Some(admin), None).await;
    assert_eq!(variants["total_count"], 2);
    let sparse = variants["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["forecast"]["item_key"] == "i1180:1")
        .unwrap();
    assert_eq!(sparse["forecast"]["rolling"]["status"], "insufficient");
    // Two scans per day on three observed dates must produce a provisional trend.
    sqlx::query("INSERT INTO item_observations(scan_id,item_key,min_buyout,median_buyout,total_units,listing_count) SELECT id,'i1180:1',NULL,'{\"numerator\":\"1500\",\"denominator\":\"1\"}'::jsonb,1,1 FROM scans WHERE market_id=$1 AND source_time>=date_trunc('day',now(),'Europe/Paris')-interval '3 days' AND source_time<date_trunc('day',now(),'Europe/Paris') AND extract(hour FROM source_time AT TIME ZONE 'Europe/Paris') IN (8,20) ON CONFLICT DO NOTHING").bind(market).execute(&app.db).await.unwrap();
    let (_, _, basic) = request(
        &app,
        "GET",
        &format!("/v1/items/i1180:1/forecast?market_id={market}"),
        None,
        Some(admin),
        None,
    )
    .await;
    assert_eq!(basic["rolling"]["status"], "experimental", "{basic}");
    assert_eq!(basic["rolling"]["method"], "damped_daily_change");
    assert!(basic["rolling"]["range_kind"]
        .as_str()
        .unwrap()
        .starts_with("limited_history"));
    assert_eq!(basic["rolling"]["price"]["numerator"], "1500");
    let body = json!({"market_id":market,"item_keys":["i1180"]});
    assert_eq!(
        request(
            &app,
            "POST",
            "/v1/forecasts/addon",
            Some(body.clone()),
            None,
            None
        )
        .await
        .0,
        StatusCode::UNAUTHORIZED
    );
    let (status, _, export) = request(
        &app,
        "POST",
        "/v1/forecasts/addon",
        Some(body),
        Some(admin),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{export}");
    assert_eq!(export["item_count"], 1);
    let artifact =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../artifacts/forecast-cache");
    std::fs::create_dir_all(&artifact).unwrap();
    std::fs::write(
        artifact.join("ForecastData.lua"),
        export["files"][1]["content"].as_str().unwrap(),
    )
    .unwrap();
    // A newer complete scan that has no listing must invalidate the live anchor.
    sqlx::query("INSERT INTO scans(id,market_id,fingerprint,source_time,format_version,quality,source_kind,realm_explicit) VALUES($1,$2,'forecast-absence',to_timestamp($3),6,'complete','full_scan',true)")
      .bind(Uuid::new_v4()).bind(market).bind((now+1) as f64).execute(&app.db).await.unwrap();
    let (_, _, absent) = request(&app, "GET", &path, None, Some(admin), None).await;
    assert_eq!(absent["rolling"]["status"], "insufficient");
    assert!(absent["current_median"].is_null());
    let (_, _, absent_batch) = request(
        &app,
        "POST",
        "/v1/forecasts/batch",
        Some(batch_body),
        Some(admin),
        None,
    )
    .await;
    assert!(absent_batch["items"][0]["current_median"].is_null());
    assert_eq!(
        absent_batch["items"][0]["rise_frequencies"],
        absent["rise_frequencies"]
    );
    assert!(absent_batch["items"][0]["rise_frequencies"]
        .as_array()
        .unwrap()
        .iter()
        .all(|f| f["percent"].is_null()));

    let (status, _, wrong) = request(
        &app,
        "GET",
        &format!("/v1/items/i1180/forecast?market_id={}", Uuid::new_v4()),
        None,
        Some(admin),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{wrong}");
}
