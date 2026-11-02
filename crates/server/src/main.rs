mod api;
mod auth;
mod catalog;
mod crafting;
#[cfg(test)]
mod crafting_tests;
mod export;
mod forecast;
mod ingest;
mod migrations;
mod read;
#[cfg(test)]
mod sync_tests;
mod worker;

use axum::{
    extract::DefaultBodyLimit,
    routing::{delete, get, patch, post},
    Router,
};
use sqlx::postgres::PgPoolOptions;
use std::{sync::Arc, time::Duration};
use tower_http::services::{ServeDir, ServeFile};

#[derive(Clone)]
pub struct App {
    pub db: sqlx::PgPool,
    pub config: Arc<Config>,
}
pub struct Config {
    pub demo_as_of: Option<chrono::DateTime<chrono::Utc>>,
    pub origin: String,
    pub insecure: bool,
    pub realm: String,
    pub house: String,
    pub trust_scans: bool,
    pub staging_key: [u8; 32],
    pub webhook: Option<String>,
}
pub const DEMO_REALM: &str = "Nauctions Demo";
impl Config {
    pub fn is_demo(&self) -> bool {
        self.realm == DEMO_REALM && self.house == "Alliance"
    }
    pub fn read_now(&self) -> chrono::DateTime<chrono::Utc> {
        self.demo_as_of
            .filter(|_| self.is_demo())
            .unwrap_or_else(chrono::Utc::now)
    }
}
fn demo_time(
    value: Option<&str>,
    realm: &str,
    house: &str,
) -> anyhow::Result<Option<chrono::DateTime<chrono::Utc>>> {
    value
        .map(|value| {
            anyhow::ensure!(
                realm == DEMO_REALM && house == "Alliance",
                "DEMO_AS_OF requires MARKET_REALM=Nauctions Demo and MARKET_HOUSE=Alliance"
            );
            let time = chrono::DateTime::parse_from_rfc3339(value)?;
            anyhow::ensure!(
                time.offset().local_minus_utc() == 0 && time.timestamp() > 0,
                "DEMO_AS_OF must be a positive RFC3339 UTC timestamp"
            );
            Ok(time.with_timezone(&chrono::Utc))
        })
        .transpose()
}
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    market_telemetry::init("server", None);
    let result = serve().await;
    if result.is_err() {
        market_telemetry::error("server_stopped_with_error", serde_json::json!({}));
    }
    market_telemetry::flush(Duration::from_secs(2));
    result
}
async fn serve() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .json()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    let origin = std::env::var("PUBLIC_ORIGIN")?;
    let allow_insecure = std::env::var("ALLOW_INSECURE_LOCALHOST").as_deref() == Ok("true");
    let url = reqwest::Url::parse(&origin)?;
    let insecure = insecure_cookie_allowed(&url, allow_insecure);
    anyhow::ensure!(
        url.scheme() == "https" || insecure,
        "PUBLIC_ORIGIN requires HTTPS"
    );
    let key = hex::decode(std::env::var("STAGING_KEY_HEX")?)?;
    let staging_key: [u8; 32] = key
        .try_into()
        .map_err(|_| anyhow::anyhow!("STAGING_KEY_HEX must contain 32 random bytes"))?;
    let house = std::env::var("MARKET_HOUSE")?;
    anyhow::ensure!(
        ["Alliance", "Horde", "Neutral"].contains(&house.as_str()),
        "invalid MARKET_HOUSE"
    );
    let realm = std::env::var("MARKET_REALM")?;
    let demo_as_of = demo_time(std::env::var("DEMO_AS_OF").ok().as_deref(), &realm, &house)?;
    let config = Arc::new(Config {
        demo_as_of,
        origin: origin.trim_end_matches('/').into(),
        insecure,
        realm,
        house,
        trust_scans: std::env::var("TRUST_RECORDED_SCAN_COMPLETENESS").as_deref() == Ok("true"),
        staging_key,
        webhook: std::env::var("DISCORD_WEBHOOK_URL").ok(),
    });
    let db = PgPoolOptions::new()
        .max_connections(10)
        .acquire_timeout(Duration::from_secs(10))
        .connect(&std::env::var("DATABASE_URL")?)
        .await?;
    migrations::run(&db).await?;
    let app = App { db, config };
    auth::bootstrap(&app).await?;
    sqlx::query("INSERT INTO markets(id,realm,house) VALUES($1,$2,$3) ON CONFLICT DO NOTHING")
        .bind(uuid::Uuid::new_v4())
        .bind(&app.config.realm)
        .bind(&app.config.house)
        .execute(&app.db)
        .await?;
    let bg = app.clone();
    tokio::spawn(async move { worker::run(bg).await });
    let router = router(app);
    let listener = tokio::net::TcpListener::bind(
        std::env::var("BIND_ADDR").unwrap_or("127.0.0.1:8080".into()),
    )
    .await?;
    axum::serve(listener, router)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    Ok(())
}
pub fn router(app: App) -> Router {
    Router::new()
        .route("/healthz", get(api::health))
        .route(
            "/v1/auth/join",
            post(auth::join).layer(DefaultBodyLimit::max(16 * 1024)),
        )
        .route(
            "/v1/auth/login",
            post(auth::login).layer(DefaultBodyLimit::max(16 * 1024)),
        )
        .route(
            "/v1/auth/logout",
            post(auth::logout).layer(DefaultBodyLimit::max(16 * 1024)),
        )
        .route(
            "/v1/auth/accept-invite",
            post(auth::accept_invite).layer(DefaultBodyLimit::max(16 * 1024)),
        )
        .route("/v1/me", get(auth::me))
        .route(
            "/v1/devices/enroll",
            post(auth::enroll).layer(DefaultBodyLimit::max(16 * 1024)),
        )
        .route(
            "/v1/devices/pair-code",
            post(auth::pair_code).layer(DefaultBodyLimit::max(16 * 1024)),
        )
        .route("/v1/devices", get(auth::devices))
        .route("/v1/devices/{id}", delete(auth::revoke_device))
        .route("/v1/collector/capabilities", get(ingest::capabilities))
        .route("/v1/collector/history", get(export::history))
        .route("/v1/collector/health", post(ingest::heartbeat))
        .route("/v1/uploads", post(ingest::upload))
        .route("/v1/uploads/{id}", get(ingest::receipt))
        .route("/v1/markets", get(read::markets))
        .route("/v1/professions", get(crafting::professions))
        .route("/v1/recipe-items", get(crafting::recipe_items))
        .route("/v1/recipes", get(crafting::recipes))
        .route("/v1/recipes/{spell}", get(crafting::recipe))
        .route("/v1/recipes/{spell}/quote", get(crafting::quote))
        .route("/v1/items", get(read::items))
        .route("/v1/items/{key}", get(read::item))
        .route("/v1/items/{key}/history", get(read::history))
        .route("/v1/items/{key}/depth", get(read::depth))
        .route("/v1/items/{key}/forecast", get(forecast::item))
        .route("/v1/forecasts", get(forecast::list))
        .route("/v1/forecasts/batch", post(forecast::batch))
        .route("/v1/forecasts/addon", post(forecast::addon))
        .route("/v1/opportunities", get(read::opportunities))
        .route("/v1/strategies", get(read::strategies))
        .route("/v1/watchlists", get(api::watchlists).post(api::watch_add))
        .route("/v1/watchlists/{key}", delete(api::watch_delete))
        .route("/v1/alert-rules", get(api::rules).post(api::rule_add))
        .route(
            "/v1/alert-rules/{id}",
            patch(api::rule_update).delete(api::rule_delete),
        )
        .route("/v1/alerts", get(api::alerts))
        .route(
            "/v1/admin/invites",
            post(auth::invite).layer(DefaultBodyLimit::max(16 * 1024)),
        )
        .route("/v1/admin/members", get(auth::members))
        .route(
            "/v1/admin/members/{id}",
            patch(auth::member_update).layer(DefaultBodyLimit::max(16 * 1024)),
        )
        .route("/v1/admin/sources", get(api::sources))
        .fallback_service(ServeDir::new("web/dist").fallback(ServeFile::new("web/dist/index.html")))
        .layer(DefaultBodyLimit::max(32 * 1024 * 1024))
        .layer(axum::middleware::from_fn(admit_request))
        .layer(axum::Extension(Arc::new(tokio::sync::Semaphore::new(8))))
        .layer(axum::middleware::map_response(security_headers))
        .layer(axum::middleware::from_fn(observe_request))
        .with_state(app)
}

async fn admit_request(
    axum::Extension(capacity): axum::Extension<Arc<tokio::sync::Semaphore>>,
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    let Ok(_permit) = capacity.try_acquire() else {
        use axum::response::IntoResponse;
        return (
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            "Request capacity reached. Try again.",
        )
            .into_response();
    };
    match tokio::time::timeout(Duration::from_secs(30), next.run(request)).await {
        Ok(response) => response,
        Err(_) => {
            use axum::response::IntoResponse;
            (
                axum::http::StatusCode::REQUEST_TIMEOUT,
                "Request timed out. Try again.",
            )
                .into_response()
        }
    }
}

async fn security_headers(mut response: axum::response::Response) -> axum::response::Response {
    let headers = response.headers_mut();
    headers.insert("cache-control", "no-store".parse().unwrap());
    headers.insert("x-content-type-options", "nosniff".parse().unwrap());
    headers.insert("x-frame-options", "DENY".parse().unwrap());
    headers.insert("referrer-policy", "same-origin".parse().unwrap());
    headers.insert("content-security-policy","default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; connect-src 'self' https://eu.i.posthog.com; frame-ancestors 'none'; base-uri 'none'; form-action 'self'".parse().unwrap());
    response
}

async fn observe_request(
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    let id = uuid::Uuid::new_v4().to_string();
    let start = std::time::Instant::now();
    // MatchedPath is a route template. Do not log URLs, query strings, headers or bodies.
    let operation = request
        .extensions()
        .get::<axum::extract::MatchedPath>()
        .map(|p| p.as_str().to_owned())
        .unwrap_or_else(|| "unmatched".into());
    let mut response = next.run(request).await;
    response
        .headers_mut()
        .insert("x-request-id", id.parse().unwrap());
    let status = response.status().as_u16();
    let duration = start.elapsed().as_millis() as u64;
    tracing::info!(request_id = %id, operation, status, duration_ms = duration, "request completed");
    if status >= 500 {
        market_telemetry::error(
            "api_request_failed",
            serde_json::json!({"request_id":id,"status":status,"duration_ms":duration}),
        );
    } else if duration > 3000 {
        market_telemetry::record(
            "api_request_slow",
            serde_json::json!({"request_id":id,"status":status,"duration_ms":duration}),
        );
    } else if operation != "/healthz" {
        market_telemetry::record(
            "api_request_completed",
            serde_json::json!({"request_id":id,"status":status,"duration_ms":duration}),
        );
    }
    response
}

fn insecure_cookie_allowed(url: &reqwest::Url, explicitly_allowed: bool) -> bool {
    explicitly_allowed
        && url.scheme() == "http"
        && matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"))
}
#[cfg(test)]
mod config_security_tests {
    use super::*;
    #[tokio::test]
    async fn request_capacity_rejects_before_another_body_is_buffered() {
        use axum::{
            body::Body,
            http::{Request, StatusCode},
            routing::post,
        };
        use tower::ServiceExt;
        let entered = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let release = Arc::new(tokio::sync::Notify::new());
        let count = entered.clone();
        let ready = release.clone();
        let app = Router::new()
            .route(
                "/",
                post(move || {
                    let count = count.clone();
                    let ready = ready.clone();
                    async move {
                        count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                        ready.notified().await;
                        StatusCode::OK
                    }
                }),
            )
            .layer(axum::middleware::from_fn(admit_request))
            .layer(axum::Extension(Arc::new(tokio::sync::Semaphore::new(8))));
        let mut jobs = Vec::new();
        for _ in 0..8 {
            let router = app.clone();
            jobs.push(tokio::spawn(async move {
                router
                    .oneshot(Request::post("/").body(Body::empty()).unwrap())
                    .await
                    .unwrap()
            }));
        }
        tokio::time::timeout(Duration::from_secs(3), async {
            while entered.load(std::sync::atomic::Ordering::SeqCst) != 8 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        let response = app
            .oneshot(Request::post("/").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(entered.load(std::sync::atomic::Ordering::SeqCst), 8);
        release.notify_waiters();
        for job in jobs {
            assert_eq!(job.await.unwrap().status(), StatusCode::OK);
        }
    }
    #[test]
    fn demo_clock_is_validated_and_cannot_freeze_real_markets() {
        let fixed = "2026-10-27T12:00:00Z";
        assert!(demo_time(Some(fixed), "Real Realm", "Alliance").is_err());
        assert!(demo_time(Some(fixed), DEMO_REALM, "Horde").is_err());
        for invalid in [
            "invalid",
            "2026-10-27T12:00:00+02:00",
            "1969-01-01T00:00:00Z",
        ] {
            assert!(demo_time(Some(invalid), DEMO_REALM, "Alliance").is_err());
        }
        let time = demo_time(Some(fixed), DEMO_REALM, "Alliance")
            .unwrap()
            .unwrap();
        let mut config = Config {
            demo_as_of: Some(time),
            origin: "https://example.test".into(),
            insecure: false,
            realm: DEMO_REALM.into(),
            house: "Alliance".into(),
            trust_scans: false,
            staging_key: [1; 32],
            webhook: None,
        };
        assert_eq!(config.read_now(), time);
        assert_eq!(config.read_now(), time);
        config.realm = "Real Realm".into();
        assert!(
            config
                .read_now()
                .signed_duration_since(chrono::Utc::now())
                .num_seconds()
                .abs()
                < 2
        );
    }
    #[tokio::test]
    async fn demo_cannot_export_or_accept_collector_data() {
        use axum::{
            body::Body,
            http::{Request, StatusCode},
        };
        use tower::ServiceExt;
        let app = App {
            db: PgPoolOptions::new()
                .connect_lazy("postgres://unused:unused@127.0.0.1:1/unused")
                .unwrap(),
            config: Arc::new(Config {
                demo_as_of: demo_time(Some("2026-10-27T12:00:00Z"), DEMO_REALM, "Alliance")
                    .unwrap(),
                origin: "https://example.test".into(),
                insecure: false,
                realm: DEMO_REALM.into(),
                house: "Alliance".into(),
                trust_scans: false,
                staging_key: [1; 32],
                webhook: None,
            }),
        };
        for (method, path, expected) in [
            (
                "GET",
                "/v1/collector/history?realm=Nauctions%20Demo&house=Alliance",
                "demo_history_not_exportable",
            ),
            ("POST", "/v1/uploads", "demo_market_read_only"),
        ] {
            let response = router(app.clone())
                .oneshot(
                    Request::builder()
                        .method(method)
                        .uri(path)
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::FORBIDDEN);
            let bytes = axum::body::to_bytes(response.into_body(), 4096)
                .await
                .unwrap();
            assert!(std::str::from_utf8(&bytes).unwrap().contains(expected));
        }
        assert!(!worker::process_upload(&app).await.unwrap());
    }
    #[test]
    fn secure_cookie_policy() {
        for origin in [
            "https://market.example",
            "https://localhost",
            "http://market.example",
            "ftp://localhost",
        ] {
            assert!(!insecure_cookie_allowed(
                &reqwest::Url::parse(origin).unwrap(),
                true
            ));
        }
        assert!(insecure_cookie_allowed(
            &reqwest::Url::parse("http://localhost:8080").unwrap(),
            true
        ));
        assert!(!insecure_cookie_allowed(
            &reqwest::Url::parse("http://localhost:8080").unwrap(),
            false
        ));
    }
    #[tokio::test]
    async fn auth_json_is_bounded_before_database_access() {
        use axum::{
            body::Body,
            http::{Request, StatusCode},
        };
        use tower::ServiceExt;
        let db = PgPoolOptions::new()
            .connect_lazy("postgres://unused:unused@127.0.0.1:1/unused")
            .unwrap();
        let app = App {
            db,
            config: Arc::new(Config {
                demo_as_of: None,
                origin: "https://market.example".into(),
                insecure: false,
                realm: "Test".into(),
                house: "Alliance".into(),
                trust_scans: false,
                staging_key: [1; 32],
                webhook: None,
            }),
        };
        // Generate the reference even when validation fails. Never trust a caller-supplied ID.
        let probe = router(app.clone())
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/auth/login")
                    .header("content-type", "application/json")
                    .header("x-request-id", "untrusted-user-value")
                    .body(Body::from("{"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert!(uuid::Uuid::parse_str(probe.headers()["x-request-id"].to_str().unwrap()).is_ok());
        assert!(probe.headers()["content-security-policy"]
            .to_str()
            .unwrap()
            .contains("https://eu.i.posthog.com"));
        for route in [
            "/v1/auth/login",
            "/v1/auth/accept-invite",
            "/v1/devices/enroll",
            "/v1/admin/invites",
            "/v1/admin/members/00000000-0000-0000-0000-000000000001",
        ] {
            let method = if route.contains("/members/") {
                "PATCH"
            } else {
                "POST"
            };
            let request = Request::builder()
                .method(method)
                .uri(route)
                .header("content-type", "application/json")
                .body(Body::from(format!(
                    "{{\"padding\":\"{}\"}}",
                    "a".repeat(17 * 1024)
                )))
                .unwrap();
            let response = router(app.clone()).oneshot(request).await.unwrap();
            assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE, "{route}");
        }
    }
}

#[cfg(test)]
mod tests;
