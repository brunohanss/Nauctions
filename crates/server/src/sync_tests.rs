use super::*;
use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use tower::ServiceExt;
use uuid::Uuid;
async fn get(a: &App, path: &str, token: Option<&str>) -> (StatusCode, Vec<u8>) {
    let mut req = Request::builder().uri(path);
    if let Some(t) = token {
        req = req.header("authorization", format!("Bearer {t}"));
    }
    let res = router(a.clone())
        .oneshot(req.body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = res.status();
    (
        status,
        to_bytes(res.into_body(), 33 * 1024 * 1024)
            .await
            .unwrap()
            .to_vec(),
    )
}
async fn ingest(a: &App, device: Uuid, bytes: &[u8]) -> String {
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO uploads(id,device_id,upload_id,checksum,byte_count,source_file_id,staged_bytes) VALUES($1,$2,$1,$3,$4,$3,$5)").bind(id).bind(device).bind(auth::hash(&String::from_utf8_lossy(bytes))).bind(bytes.len() as i64).bind(ingest::encrypt(&a.config.staging_key,bytes).unwrap()).execute(&a.db).await.unwrap();
    assert!(worker::process_upload(a).await.unwrap());
    sqlx::query_scalar("SELECT status FROM uploads WHERE id=$1")
        .bind(id)
        .fetch_one(&a.db)
        .await
        .unwrap()
}
async fn counts(a: &App) -> (i64, i64, i64) {
    (
        sqlx::query_scalar("SELECT count(*) FROM scans")
            .fetch_one(&a.db)
            .await
            .unwrap(),
        sqlx::query_scalar("SELECT count(*) FROM item_observations")
            .fetch_one(&a.db)
            .await
            .unwrap(),
        sqlx::query_scalar("SELECT count(*) FROM historic_daily_imports")
            .fetch_one(&a.db)
            .await
            .unwrap(),
    )
}
fn scan(ts: i64, partial: bool) -> String {
    format!("{{realm=\"Synthetic Realm\",faction=\"Alliance\",ts={ts},count={},itemsCount=3,dataFormatVersion=6,data=\"i123!/2,20,100,20000,&3,1,50,, i123;42!/4,3,,10000000000, i456!/2,1,50,,\"}}",if partial{99}else{4})
}
#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL; uses isolated schema"]
async fn collector_history_roundtrip_no_feedback() {
    let url = std::env::var("TEST_DATABASE_URL").expect("TEST_DATABASE_URL required");
    let owner = PgPoolOptions::new()
        .max_connections(2)
        .connect(&url)
        .await
        .unwrap();
    let schema = format!("sync_test_{}", Uuid::new_v4().simple());
    sqlx::query(&format!("CREATE SCHEMA {schema}"))
        .execute(&owner)
        .await
        .unwrap();
    let search = schema.clone();
    let db = PgPoolOptions::new()
        .max_connections(5)
        .after_connect(move |c, _| {
            let s = search.clone();
            Box::pin(async move {
                sqlx::query(&format!("SET search_path TO {s}"))
                    .execute(c)
                    .await?;
                Ok(())
            })
        })
        .connect(&url)
        .await
        .unwrap();
    migrations::run(&db).await.unwrap();
    let a = App {
        db,
        config: Arc::new(Config {
            demo_as_of: None,
            origin: "https://example.test".into(),
            insecure: false,
            realm: "Synthetic Realm".into(),
            house: "Alliance".into(),
            trust_scans: false,
            staging_key: [3; 32],
            webhook: None,
        }),
    };
    let cloned = a.clone();
    let result=tokio::spawn(async move{
 let a=cloned;let member=Uuid::new_v4();let device=Uuid::new_v4();sqlx::query("INSERT INTO members(id,login,display_name,password_hash,role) VALUES($1,'sync','Sync','unused','member')").bind(member).execute(&a.db).await.unwrap();sqlx::query("INSERT INTO devices(id,member_id,display_name,credential_hash) VALUES($1,$2,'sync',$3)").bind(device).bind(member).bind(auth::hash("sync-token")).execute(&a.db).await.unwrap();sqlx::query("INSERT INTO markets(id,realm,house) VALUES($1,'Synthetic Realm','Alliance')").bind(Uuid::new_v4()).execute(&a.db).await.unwrap();
 let path="/v1/collector/history?realm=Synthetic%20Realm&house=Alliance&protocol=2";
 assert_eq!(get(&a,path,None).await.0,StatusCode::UNAUTHORIZED);assert_eq!(get(&a,"/v1/collector/history?realm=Wrong&house=Alliance&protocol=2",Some("sync-token")).await.0,StatusCode::BAD_REQUEST);
 sqlx::query("DELETE FROM rate_limits").execute(&a.db).await.unwrap();
 let now=chrono::Utc::now().timestamp()-60;let old=now-10*86400;
 let original=format!("AuctionDBForeverSaved={{settings={{recordScans=true}},ah={{{},{},{}}},houses={{Alliance={{items={{[999]=\"{now},8,9,2\"}},daily={{}},scans={{}}}}}}}}",scan(now,false),scan(old,false),scan(now-10,true));
 assert_eq!(ingest(&a,device,original.as_bytes()).await,"accepted");
 // A second member presents the same evidence; canonical quantities must not double.
 let second_member = Uuid::new_v4();
 let second_device = Uuid::new_v4();
 sqlx::query("INSERT INTO members(id,login,display_name,password_hash,role) VALUES($1,'sync-second','Second contributor','unused','member')")
     .bind(second_member).execute(&a.db).await.unwrap();
 sqlx::query("INSERT INTO devices(id,member_id,display_name,credential_hash) VALUES($1,$2,'second computer',$3)")
     .bind(second_device).bind(second_member).bind(auth::hash("second-sync-token")).execute(&a.db).await.unwrap();
 let before_second = counts(&a).await;
 assert_eq!(ingest(&a,second_device,original.as_bytes()).await,"accepted");
 assert_eq!(counts(&a).await,before_second);
 let presenters: i64 = sqlx::query_scalar("SELECT count(DISTINCT device_id) FROM scan_sources")
     .fetch_one(&a.db).await.unwrap();
 assert_eq!(presenters,2);
 let (status,bytes)=get(&a,path,Some("sync-token")).await;assert_eq!(status,StatusCode::OK,"{}",String::from_utf8_lossy(&bytes));let snapshot:market_domain::HistorySnapshot=serde_json::from_slice(&bytes).unwrap();assert!(snapshot.verify(chrono::Utc::now().timestamp()));let bundle=snapshot.history;
 assert_eq!(bundle.points.len(),2);assert!(bundle.points.iter().all(|p|p.item_id==123||p.item_id==456));let p=bundle.points.iter().find(|p|p.item_id==123).unwrap();assert_eq!(p.quantity,21);assert_eq!(p.min_unit_copper,1000);assert_eq!(p.median_unit_copper,1000);let bid=bundle.points.iter().find(|p|p.item_id==456).unwrap();assert_eq!(bid.min_unit_copper,0);assert_eq!(bid.quantity,1);assert_eq!(bundle.daily.len(),2);assert!(bundle.daily.iter().all(|p|p.scan_count==1));
 let before=counts(&a).await;let merged=ahdb_format::merge_history(original.as_bytes(),&bundle,"Synthetic Realm","Alliance").unwrap();assert_eq!(ingest(&a,device,&merged).await,"accepted");assert_eq!(counts(&a).await,before);
 // A later addon save can compact imported recent rows into a changed daily tuple.
 let merged=String::from_utf8(merged).unwrap();let marker=merged.find("nauctionsSync").unwrap();assert!(marker>0);
 let compacted=format!(r#"AuctionDBForeverSaved={{nauctionsSync={{schema_version=1,realm="Synthetic Realm",house="Alliance"}},settings={{recordScans=true}},houses={{Alliance={{items={{}},daily={{[123]="{old},12000,13000,8,99"}},scans={{}}}}}}}}"#);assert_eq!(ingest(&a,device,compacted.as_bytes()).await,"accepted");assert_eq!(counts(&a).await,before);
 // Preserve marker, append a genuinely new original full scan.
 let with_new=merged.replacen("ah={",&format!("ah={{{},",scan(now+1,false)),1);assert_eq!(ingest(&a,device,with_new.as_bytes()).await,"accepted");assert_eq!(counts(&a).await.0,before.0+1);assert_eq!(ingest(&a,device,with_new.as_bytes()).await,"accepted");assert_eq!(counts(&a).await.0,before.0+1);
 // Exact exported tuples are rejected even if another tool strips the marker.
 let naked=format!("AuctionDBForeverSaved={{settings={{recordScans=true}},houses={{Alliance={{items={{[123]=\"{now},1000,1000,21\"}},daily={{}},scans={{}}}}}}}}");let before=counts(&a).await;assert_eq!(ingest(&a,device,naked.as_bytes()).await,"accepted");assert_eq!(counts(&a).await,before);
 let history_only=b"AuctionDBForeverSaved={settings={recordScans=true},houses={Alliance={items={},daily={},scans={}}}}";let synced=ahdb_format::merge_history(history_only,&bundle,"Synthetic Realm","Alliance").unwrap();assert_eq!(ingest(&a,device,&synced).await,"accepted");assert_eq!(counts(&a).await,before);
 // The second player imports shared history, then contributes another native scan.
 assert_eq!(ingest(&a,second_device,&synced).await,"accepted");
 assert_eq!(counts(&a).await,before);
 let second_save=merged.replacen("ah={",&format!("ah={{{},",scan(now+2,false)),1);
 assert_eq!(ingest(&a,second_device,second_save.as_bytes()).await,"accepted");
 assert_eq!(counts(&a).await.0,before.0+1);
 assert_eq!(ingest(&a,second_device,second_save.as_bytes()).await,"accepted");
 assert_eq!(counts(&a).await.0,before.0+1);
 for token in ["sync-token", "second-sync-token"] {
     let (status,bytes)=get(&a,path,Some(token)).await;
     assert_eq!(status,StatusCode::OK);
     let shared=serde_json::from_slice::<market_domain::HistorySnapshot>(&bytes).unwrap().history;
     assert!(shared.points.iter().any(|point|point.observed_at==now+1));
     assert!(shared.points.iter().any(|point|point.observed_at==now+2));
 }
 large_export_checks(&a,path,now).await;
 sqlx::query("UPDATE devices SET revoked_at=now() WHERE id=$1").bind(device).execute(&a.db).await.unwrap();assert_eq!(get(&a,path,Some("sync-token")).await.0,StatusCode::FORBIDDEN);
 }).await;
    a.db.close().await;
    sqlx::query(&format!("DROP SCHEMA {schema} CASCADE"))
        .execute(&owner)
        .await
        .unwrap();
    owner.close().await;
    if let Err(e) = result {
        std::panic::resume_unwind(e.into_panic());
    }
}

async fn large_export_checks(a: &App, path: &str, now: i64) {
    let market: Uuid = sqlx::query_scalar(
        "SELECT id FROM markets WHERE realm='Synthetic Realm' AND house='Alliance'",
    )
    .fetch_one(&a.db)
    .await
    .unwrap();
    sqlx::query("DELETE FROM rate_limits")
        .execute(&a.db)
        .await
        .unwrap();
    assert_eq!(
        get(
            a,
            "/v1/collector/history?realm=Synthetic%20Realm&house=Alliance",
            Some("sync-token")
        )
        .await
        .0,
        StatusCode::UPGRADE_REQUIRED
    );
    sqlx::query(
        "INSERT INTO items(item_key,base_item_id,name) VALUES('i777',777,'Large export fixture')",
    )
    .execute(&a.db)
    .await
    .unwrap();
    sqlx::query("INSERT INTO scans(id,market_id,fingerprint,source_time,format_version,quality,source_kind,realm_explicit) SELECT md5('bulk-'||n)::uuid,$1,'bulk-'||n,to_timestamp($2::double precision+n),6,'complete','full_scan',true FROM generate_series(1,100001) n")
        .bind(market).bind((now-20*86400) as f64).execute(&a.db).await.unwrap();
    sqlx::query("INSERT INTO item_observations(scan_id,item_key,min_buyout,median_buyout,total_units,listing_count) SELECT id,'i777','{\"numerator\":\"100\",\"denominator\":\"1\"}'::jsonb,'{\"numerator\":\"200\",\"denominator\":\"1\"}'::jsonb,4,1 FROM scans WHERE fingerprint LIKE 'bulk-%'").execute(&a.db).await.unwrap();
    // More than the old raw-row budget must compact into a few daily rows.
    sqlx::query("DELETE FROM rate_limits")
        .execute(&a.db)
        .await
        .unwrap();
    let (status, bytes) = get(a, path, Some("sync-token")).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "{}",
        String::from_utf8_lossy(&bytes)
    );
    let snapshot: market_domain::HistorySnapshot = serde_json::from_slice(&bytes).unwrap();
    assert!(snapshot.verify(chrono::Utc::now().timestamp()));
    assert_eq!(
        snapshot
            .history
            .daily
            .iter()
            .filter(|p| p.item_id == 777)
            .map(|p| p.scan_count)
            .sum::<u64>(),
        100001
    );
    // Duplicate presenters must be deduplicated before the output budget too.
    let ts = now - 60 * 86400;
    let point = serde_json::json!({"market":{"realm":"Synthetic Realm","house":"Alliance"},"item_key":"i777","observed_at":ts,"utc_day":ts.div_euclid(86400),"min_unit_copper":100,"median_unit_copper":200,"average_quantity":4,"scan_count":1});
    sqlx::query("INSERT INTO historic_daily_imports(market_id,item_key,utc_day,source_identity,point,realm_explicit) SELECT $1,'i777',$2,'copy-'||n,$3,true FROM generate_series(1,100001) n")
        .bind(market).bind(ts.div_euclid(86400)).bind(point).execute(&a.db).await.unwrap();
    sqlx::query("DELETE FROM rate_limits")
        .execute(&a.db)
        .await
        .unwrap();
    let (status, bytes) = get(a, path, Some("sync-token")).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "{}",
        String::from_utf8_lossy(&bytes)
    );
    let snapshot: market_domain::HistorySnapshot = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(
        snapshot
            .history
            .daily
            .iter()
            .filter(|p| p.item_id == 777 && p.observed_at == ts)
            .count(),
        1
    );
    // Unknown overlap must produce a visible conflict instead of choosing by scan count.
    sqlx::query("UPDATE historic_daily_imports SET point=jsonb_set(point,'{scan_count}','2') WHERE source_identity='copy-1'").execute(&a.db).await.unwrap();
    sqlx::query("DELETE FROM rate_limits")
        .execute(&a.db)
        .await
        .unwrap();
    assert_eq!(
        get(a, path, Some("sync-token")).await.0,
        StatusCode::CONFLICT
    );
}
