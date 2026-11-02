use super::*;
use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use tower::ServiceExt;
use uuid::Uuid;

async fn get(a: &App, path: &str, authenticated: bool) -> (StatusCode, Value) {
    let mut req = Request::builder().uri(path);
    if authenticated {
        req = req.header("cookie", "market_session=crafting-test");
    }
    let response = router(a.clone())
        .oneshot(req.body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    (
        status,
        serde_json::from_slice(&bytes)
            .unwrap_or_else(|_| json!({"text":String::from_utf8_lossy(&bytes)})),
    )
}
async fn upload(a: &App, bytes: &[u8], id: Uuid) -> Value {
    let response = router(a.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/uploads")
                .header("authorization", "Bearer crafting-device")
                .header("x-upload-id", id.to_string())
                .header(
                    "x-content-sha256",
                    auth::hash(&String::from_utf8_lossy(bytes)),
                )
                .header("x-source-file-id", "a".repeat(64))
                .body(Body::from(bytes.to_vec()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert!(response.status().is_success());
    while worker::process_upload(a).await.unwrap() {}
    sqlx::query_scalar("SELECT jsonb_build_object('status',status,'error',error_code) FROM uploads WHERE upload_id=$1")
        .bind(id).fetch_one(&a.db).await.unwrap()
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL; uses an isolated temporary schema"]
async fn shared_recipe_uploads_and_snapshot_costs() {
    let owner = PgPoolOptions::new()
        .max_connections(2)
        .connect(&std::env::var("TEST_DATABASE_URL").unwrap())
        .await
        .unwrap();
    let schema = format!("crafting_test_{}", Uuid::new_v4().simple());
    sqlx::query(&format!("CREATE SCHEMA {schema}"))
        .execute(&owner)
        .await
        .unwrap();
    let search = schema.clone();
    let db = PgPoolOptions::new()
        .max_connections(5)
        .after_connect(move |c, _| {
            let search = search.clone();
            Box::pin(async move {
                sqlx::query(&format!("SET search_path TO {search}"))
                    .execute(c)
                    .await?;
                Ok(())
            })
        })
        .connect(&std::env::var("TEST_DATABASE_URL").unwrap())
        .await
        .unwrap();
    migrations::run(&db).await.unwrap();
    let a = App {
        db,
        config: Arc::new(Config {
            demo_as_of: None,
            origin: "https://test.example".into(),
            insecure: false,
            realm: "Crafting Test".into(),
            house: "Alliance".into(),
            trust_scans: false,
            staging_key: [13; 32],
            webhook: None,
        }),
    };
    let app = a.clone();
    let outcome=tokio::spawn(async move {
        let member=Uuid::new_v4(); let device=Uuid::new_v4(); let market=Uuid::new_v4();
        sqlx::query("INSERT INTO members(id,login,display_name,password_hash,role) VALUES($1,'crafting','Tester','unused','member')").bind(member).execute(&app.db).await.unwrap();
        sqlx::query("INSERT INTO sessions(token_hash,member_id,csrf_token,expires_at) VALUES($1,$2,'unused',now()+interval '1 hour')").bind(auth::hash("crafting-test")).bind(member).execute(&app.db).await.unwrap();
        sqlx::query("INSERT INTO devices(id,member_id,display_name,credential_hash) VALUES($1,$2,'Test',$3)").bind(device).bind(member).bind(auth::hash("crafting-device")).execute(&app.db).await.unwrap();
        sqlx::query("INSERT INTO markets(id,realm,house) VALUES($1,'Crafting Test','Alliance')").bind(market).execute(&app.db).await.unwrap();
        for path in ["/v1/recipe-items","/v1/professions","/v1/recipes","/v1/recipes/900001","/v1/recipes/900001/quote"] {assert_eq!(get(&app,path,false).await.0,StatusCode::UNAUTHORIZED);}
        let now=chrono::Utc::now().timestamp();
        let recipe=include_str!("../../../fixtures/synthetic-professions.lua").replace("AuctionDBForeverSaved = { houses = {} }","").replace("1793270000",&(now-60).to_string());
        assert_eq!(upload(&app,recipe.as_bytes(),Uuid::new_v4()).await["status"],"accepted");
        assert_eq!(get(&app,"/v1/recipes/900001/quote",true).await.1["status"],"no_market_snapshot");
        let auction=format!("AuctionDBForeverSaved={{ah={{{{realm=\"Crafting Test\",faction=\"Alliance\",ts={},count=3,itemsCount=3,dataFormatVersion=6,data=\"i900201!/2,3,,300, i900202!/2,1,,50, i900101!/2,1,,500,\"}}}}}}",now-30);
        let combined=format!("{auction}\n{recipe}");
        let id=Uuid::new_v4();
        assert_eq!(upload(&app,combined.as_bytes(),id).await["status"],"accepted");
        assert_eq!(upload(&app,combined.as_bytes(),id).await["status"],"accepted");
        let count:i64=sqlx::query_scalar("SELECT count(*) FROM recipe_versions").fetch_one(&app.db).await.unwrap(); assert_eq!(count,2);
        let list=get(&app,"/v1/recipes?q=potion&profession_id=171",true).await;
        assert_eq!(list.1["total_count"],1); assert_eq!(list.1["recipes"][0]["conflicted"],false);
        assert_eq!(get(&app,"/v1/professions",true).await.1.as_array().unwrap().len(),2);
        let url=format!("/v1/recipes/900001/quote?market_id={market}");
        let q=get(&app,&url,true).await;
        assert_eq!(q.0,StatusCode::OK,"{}",q.1);
        assert_eq!(q.1["quote"]["total_cost"],json!({"numerator":"350","denominator":"1"}));
        assert_eq!(q.1["quote"]["estimated_profit"],json!({"numerator":"600","denominator":"1"}));
        // Synthetic names sort after the fixture; profits must still rank before pagination.
        sqlx::query("INSERT INTO recipe_versions(id,product,build,build_number,spell_id,profession_id,name,locale,kind,facts,first_observed_at,last_observed_at) SELECT 'profit-sort-'||n,v.product,v.build,v.build_number,910000+n,v.profession_id,'ZZ rank '||n,v.locale,v.kind,jsonb_set(jsonb_set(jsonb_set(v.facts,'{spell_id}',to_jsonb(910000+n)),'{name}',to_jsonb('ZZ rank '||n)),'{reagents,900201}',to_jsonb(n)),v.first_observed_at,v.last_observed_at FROM recipe_versions v CROSS JOIN generate_series(1,35) n WHERE v.spell_id=900001")
            .execute(&app.db).await.unwrap();
        let ranked=get(&app,&format!("/v1/recipes?market_id={market}&limit=1"),true).await;
        assert_eq!(ranked.0,StatusCode::OK,"{}",ranked.1);
        assert_eq!(ranked.1["total_count"],37);
        assert_eq!(ranked.1["recipes"][0]["facts"]["spell_id"],910001);
        assert_eq!(ranked.1["recipes"][0]["quote"]["estimated_profit"],json!({"numerator":"800","denominator":"1"}));
        let second_page=get(&app,&format!("/v1/recipes?market_id={market}&limit=30&offset=30"),true).await.1;
        assert_eq!(second_page["total_count"],37);
        assert_eq!(second_page["recipes"].as_array().unwrap().len(),7);
        assert_eq!(second_page["recipes"][6]["quote"]["estimated_profit"],Value::Null);
        let lowest=get(&app,&format!("/v1/recipes?market_id={market}&sort=profit&direction=asc&limit=1"),true).await.1;
        assert_eq!(lowest["recipes"][0]["facts"]["spell_id"],910035);
        let beyond=get(&app,&format!("/v1/recipes?market_id={market}&offset=100"),true).await.1;
        assert_eq!(beyond["total_count"],37);
        assert!(beyond["recipes"].as_array().unwrap().is_empty());
        assert_eq!(get(&app,"/v1/recipes?sort=inventory",true).await.0,StatusCode::BAD_REQUEST);
        sqlx::query("DELETE FROM recipe_versions WHERE id LIKE 'profit-sort-%'").execute(&app.db).await.unwrap();
        assert_eq!(get(&app,&format!("{url}&mode=purchase&crafts=2"),true).await.1["status"],"missing_materials");
        assert_eq!(get(&app,"/v1/recipes/900002/quote",true).await.1["quote"]["estimated_profit"],Value::Null);
        assert_eq!(get(&app,&format!("{url}&market_id={}",Uuid::new_v4()),true).await.0,StatusCode::BAD_REQUEST);
        assert_eq!(get(&app,&format!("/v1/recipes/900001/quote?market_id={}",Uuid::new_v4()),true).await.0,StatusCode::NOT_FOUND);
        let unknown_binding=recipe.replace("tradable = true,", "");
        assert_eq!(upload(&app,unknown_binding.as_bytes(),Uuid::new_v4()).await["status"],"accepted");
        let refined=get(&app,"/v1/recipes/900001",true).await.1;
        assert_eq!(refined["conflicted"],false);
        assert_eq!(refined["facts"]["tradable"],true);
        let unknown_yield=recipe.replace("min_output = 2, max_output = 2,", "");
        assert_eq!(upload(&app,unknown_yield.as_bytes(),Uuid::new_v4()).await["status"],"accepted");
        assert_eq!(get(&app,&url,true).await.1["status"],"estimated");
        let partial=auction.replace(&(now-30).to_string(),&(now-10).to_string()).replace("count=3,itemsCount=3","count=1,itemsCount=1").replace("i900201!/2,3,,300, i900202!/2,1,,50, ","");
        assert_eq!(upload(&app,partial.as_bytes(),Uuid::new_v4()).await["status"],"accepted");
        assert_eq!(get(&app,&url,true).await.1["status"],"missing_materials");
        let conflict=recipe.replace("[900201] = 3","[900201] = 4");
        assert_eq!(upload(&app,conflict.as_bytes(),Uuid::new_v4()).await["status"],"accepted");
        assert_eq!(get(&app,&url,true).await.1["status"],"recipe_conflict");
        let newer=recipe.replace("1.60.1.70630","1.60.1.70658");
        assert_eq!(upload(&app,newer.as_bytes(),Uuid::new_v4()).await["status"],"accepted");
        assert_eq!(get(&app,"/v1/recipes/900001",true).await.1["build"],"1.60.1.70658");
        let invalid=newer.replace("tools = {}","tools = {}, learned = true");
        assert_eq!(upload(&app,invalid.as_bytes(),Uuid::new_v4()).await["status"],"rejected");
        let binding_conflict=newer.replace("tradable = true", "tradable = false");
        assert_eq!(upload(&app,binding_conflict.as_bytes(),Uuid::new_v4()).await["status"],"accepted");
        assert_eq!(get(&app,&url,true).await.1["status"],"recipe_conflict");
        // Continue with a separate synthetic auction discovery pass after the upload rate window.
        sqlx::query("DELETE FROM rate_limits WHERE key=$1").bind(format!("upload:{device}")).execute(&app.db).await.unwrap();
        let discoveries=include_str!("../../../fixtures/synthetic-auction-recipes.lua").replace("1793270000",&(now-5).to_string());
        let discovered_upload=Uuid::new_v4();
        assert_eq!(upload(&app,discoveries.as_bytes(),discovered_upload).await["status"],"accepted");
        assert_eq!(upload(&app,discoveries.as_bytes(),discovered_upload).await["status"],"accepted");
        let items=get(&app,"/v1/recipe-items",true).await;
        assert_eq!(items.0,StatusCode::OK,"{}",items.1);
        assert_eq!(items.1["total_count"],2);
        assert_eq!(items.1["unresolved_count"],1);
        assert_eq!(items.1["recipe_items"][0]["recipe_spell_id"],900003);
        let observation_count:i64=sqlx::query_scalar("SELECT count(*) FROM recipe_item_observations").fetch_one(&app.db).await.unwrap();
        assert_eq!(observation_count,2);
        assert_eq!(get(&app,"/v1/recipes/900003",true).await.1["facts"]["min_output"],2);
        assert_eq!(get(&app,"/v1/recipes/900004",true).await.0,StatusCode::NOT_FOUND);
        let pending=format!("NauctionsProfessionsSaved={{schema_version=1,product=\"wow_classic_beta\",build=\"1.60.1.70630\",addon_version=\"0.5.0-nauctions.2\",locale=\"enUS\",recipes={{}},recipe_items={{[910005]={{captured_at={},status=\"mapping_unavailable\"}}}}}}",now-1);
        assert_eq!(upload(&app,pending.as_bytes(),Uuid::new_v4()).await["status"],"accepted");
        let future=pending.replace(&(now-1).to_string(),&(now+600).to_string());
        assert_eq!(upload(&app,future.as_bytes(),Uuid::new_v4()).await["status"],"rejected");
        assert_eq!(get(&app,"/v1/recipe-items?limit=1",true).await.1["unresolved_count"],2);
        assert_eq!(get(&app,"/v1/recipe-items?limit=0",true).await.0,StatusCode::BAD_REQUEST);
        assert_eq!(get(&app,"/v1/recipes?limit=0",true).await.0,StatusCode::BAD_REQUEST);
        assert_eq!(get(&app,"/v1/recipes/900001/quote?crafts=0",true).await.0,StatusCode::BAD_REQUEST);
    }).await;
    a.db.close().await;
    sqlx::query(&format!("DROP SCHEMA {schema} CASCADE"))
        .execute(&owner)
        .await
        .unwrap();
    owner.close().await;
    outcome.expect("crafting integration flow");
}
