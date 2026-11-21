use crate::schema::{migrate, MIGRATIONS};
use rusqlite::Connection;

// spec: AI-001
// spec: AI-013
#[test]
fn v5_fixture_migrates_preserving_data_and_roundtrips_all_ai_tables() {
    let mut db = Connection::open_in_memory().unwrap();
    db.pragma_update(None, "foreign_keys", true).unwrap();
    migrate(&mut db, 0, &MIGRATIONS[..5]).unwrap();
    db.execute(
        "INSERT INTO urls(id,url,address,origin,is_internal,state,discovered_via,folder_depth,url_length) VALUES(1,'https://ai.test/','https://ai.test/','https://ai.test',1,'Done','Seed',0,16)",
        [],
    )
    .unwrap();
    migrate(&mut db, 5, MIGRATIONS).unwrap();
    db.execute_batch(r#"
INSERT INTO ai_runs(id,started_at,budget_at_start_microusd,config_json) VALUES(1,'2026-11-09',10000,'{}');
INSERT INTO api_budget(area,limit_microusd) VALUES('',10000);
INSERT INTO ai_batches(id,run_id,provider,endpoint,provider_batch_id,status,created_at,request_manifest_json) VALUES(1,1,'mock','responses','b1','Pending (batch)','2026-11-09','[]');
INSERT INTO ai_requests(id,run_id,timestamp,task,provider,model,endpoint,token_cost_microusd,cost_microusd,price_source,status,duration_ms) VALUES(1,1,'2026-11-09','decision','mock','mock','decisions',4000,4000,'estimate','Success',10);
INSERT INTO ai_cache VALUES('hash','mock','mock','decisions','page-type','1.0.0','input','params','{"ok":true}','Success','2026-11-09');
INSERT INTO ai_questions VALUES('page-type','1.0.0','{"type":"predicate"}');
INSERT INTO ai_answers(url_id,question_id,question_version,answer_json,status) VALUES(1,'page-type','1.0.0','{"ok":true}','Success');
INSERT INTO ai_suggestions(url_id,element,candidate,value,status) VALUES(1,'title',1,'A title','Success');
INSERT INTO ai_redirect_map(source_address,target_address,status) VALUES('https://old.test/','https://ai.test/','Success');
INSERT INTO ai_link_suggestions(source_url_id,target_url_id,anchor,status) VALUES(1,1,'example','Success');
INSERT INTO ai_queries(name,question,sql,created_at,status) VALUES('Titles','Find titles','SELECT title_1 FROM q_urls','2026-11-09','Success');
INSERT INTO ai_prompt_results VALUES(1,'prompt','1.0.0','{"ok":true}','Success','mock',1);
INSERT INTO ai_embeddings VALUES(1,'embedding','1.0.0','mock','mock',2,'hash',X'0000803F00000000');
"#).unwrap();
    for table in [
        "ai_runs",
        "api_budget",
        "ai_batches",
        "ai_requests",
        "ai_cache",
        "ai_questions",
        "ai_answers",
        "ai_suggestions",
        "ai_redirect_map",
        "ai_link_suggestions",
        "ai_queries",
        "ai_prompt_results",
        "ai_embeddings",
    ] {
        let count: i64 = db
            .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1, "{table}");
        let mut query = db.prepare(&format!("SELECT * FROM {table}")).unwrap();
        let columns = query.column_count();
        let row: Vec<rusqlite::types::Value> = query
            .query_row([], |r| (0..columns).map(|i| r.get(i)).collect())
            .unwrap();
        assert_eq!(row.len(), columns);
    }
    assert_eq!(
        db.query_row("SELECT input_tokens FROM ai_requests", [], |r| r
            .get::<_, Option<i64>>(0))
            .unwrap(),
        None
    );
    assert_eq!(
        db.query_row("SELECT cost_microusd FROM ai_requests", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        4000
    );
    assert_eq!(
        db.query_row("SELECT vector FROM ai_embeddings", [], |r| r
            .get::<_, Vec<u8>>(0))
            .unwrap(),
        [0, 0, 128, 63, 0, 0, 0, 0]
    );
    assert!(db
        .execute("UPDATE ai_questions SET definition_json='{}'", [])
        .is_err());
    assert!(db
        .execute("UPDATE api_budget SET limit_microusd=-1", [])
        .is_err());
    assert!(db
        .execute("UPDATE ai_embeddings SET dimensions=3", [])
        .is_err());
    assert_eq!(
        db.query_row("SELECT url FROM urls WHERE id=1", [], |r| r
            .get::<_, String>(0))
            .unwrap(),
        "https://ai.test/"
    );
    assert_eq!(crate::schema::user_version(&db).unwrap(), 6);
}

// spec: AI-013
#[test]
fn v5_file_fixture_gets_byte_identical_backup_and_survives_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests")
        .join("fixtures")
        .join("crawl-files")
        .join("schema-v2.ospider");
    let path = dir.path().join("v5.ospider");
    std::fs::copy(fixture, &path).unwrap();
    {
        let mut db = Connection::open(&path).unwrap();
        migrate(&mut db, 2, &MIGRATIONS[..5]).unwrap();
        db.execute(
            "UPDATE crawl_meta SET value='5' WHERE key='schema_version'",
            [],
        )
        .unwrap();
    }
    let original = std::fs::read(&path).unwrap();
    {
        let file = crate::CrawlFile::open(&path, crate::OpenMode::ReadWrite).unwrap();
        assert_eq!(crate::schema::user_version(file.conn()).unwrap(), 6);
        file.conn()
            .execute(
                "INSERT INTO api_budget(area,limit_microusd) VALUES('',10000)",
                [],
            )
            .unwrap();
        assert_eq!(
            file.conn()
                .query_row("SELECT count(*) FROM urls", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            42
        );
    }
    assert_eq!(
        std::fs::read(crate::backup_path(&path, 5)).unwrap(),
        original
    );
    let reopened = crate::CrawlFile::open(&path, crate::OpenMode::ReadOnly).unwrap();
    assert_eq!(
        reopened
            .conn()
            .query_row("SELECT limit_microusd FROM api_budget", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        10000
    );
}
