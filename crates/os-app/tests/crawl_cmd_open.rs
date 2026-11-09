#![allow(clippy::disallowed_types, clippy::disallowed_methods)]
#[path = "session_common.rs"]
mod common;
use common::*;
use serde_json::json;

// spec: CRAWL-128
// spec: CRAWL-143
#[tokio::test]
async fn opening_legacy_file_uses_a_migrated_copy_and_preserves_original_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let app = app(dir.path());
    let latest = create(&dir.path().join("latest.ospider"), cfg());
    latest.set_meta("state", "Paused").unwrap();
    let path = dir.path().join("legacy.ospider");
    let mut old = rusqlite::Connection::open(&path).unwrap();
    os_store::schema::migrate(&mut old, 0, &os_store::MIGRATIONS[..1]).unwrap();
    old.pragma_update(None, "application_id", os_store::APPLICATION_ID)
        .unwrap();
    let mut stmt = latest
        .conn()
        .prepare("SELECT key,value FROM crawl_meta")
        .unwrap();
    for row in stmt
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
        .unwrap()
    {
        let (key, value) = row.unwrap();
        old.execute(
            "INSERT INTO crawl_meta(key,value) VALUES (?1,?2)",
            [key, value],
        )
        .unwrap();
    }
    old.execute(
        "INSERT INTO crawl_config(id,osconfig,updated_at) VALUES (1,?1,'2026-10-17T00:00:00.000Z')",
        [cfg().to_toml_string().unwrap()],
    )
    .unwrap();
    drop(old);
    let before = std::fs::read(&path).unwrap();
    let summary = open(&app, &path).await;
    assert_eq!(summary.schema_version, os_store::SCHEMA_VERSION);
    assert_ne!(std::path::Path::new(&summary.file), path);
    assert_eq!(std::fs::read(&path).unwrap(), before);
    assert_eq!(summary.phase, os_app::dto::CrawlPhase::Paused);
}

// spec: CRAWL-127
#[tokio::test]
async fn newer_file_is_rejected_and_save_as_never_overwrites() {
    let dir = tempfile::tempdir().unwrap();
    let app = app(dir.path());
    let f = create(&dir.path().join("current.ospider"), cfg());
    f.set_meta("state", "Paused").unwrap();
    insert(&f, 1, 1);
    open(&app, f.path()).await;
    let destination = dir.path().join("existing.ospider");
    std::fs::write(&destination, b"keep me").unwrap();
    let result = operation(&app, "crawl.saveAs", json!({"dest":destination})).await;
    assert_eq!(result.state, os_app::dto::OpState::Failed);
    assert_eq!(std::fs::read(destination).unwrap(), b"keep me");
    let newer = create(&dir.path().join("newer.ospider"), cfg());
    newer
        .conn()
        .pragma_update(None, "user_version", 999)
        .unwrap();
    let path = newer.path().to_path_buf();
    drop(newer);
    let before = std::fs::read(&path).unwrap();
    let error = app
        .dispatch(ctx(), "crawl.open", json!({"kind":"path","path":path}))
        .await
        .unwrap_err();
    assert_eq!(error.message_key, "err-crawl-newer-version");
    assert_eq!(std::fs::read(path).unwrap(), before);
    assert_eq!(
        app.sessions().current(&window()).unwrap().path,
        std::fs::canonicalize(f.path()).unwrap()
    );
}

// spec: CRAWL-027
#[tokio::test]
async fn list_file_preview_uses_engine_deduplication_rules() {
    let dir = tempfile::tempdir().unwrap();
    let app = app(dir.path());
    let result = call(
        &app,
        "crawl.parseList",
        json!({"kind":"path","path":fixture().join("urls.txt")}),
    )
    .await;
    assert_eq!(result["valid"], 2);
    assert_eq!(result["invalid"], 1);
    assert_eq!(result["duplicates"], 1);
    assert_eq!(result["sample"][0], "https://fixture.test/");
}

// spec: CRAWL-127
// A "Recent" entry whose file is gone (e.g. a deleted temporary folder) is reported as a missing file with its path,
// so the menus remove it with a hint instead of showing a generic I/O failure (B4-CMPUI, beta.3 report).
#[tokio::test]
async fn opening_a_missing_path_reports_not_found_with_the_path() {
    let dir = tempfile::tempdir().unwrap();
    let app = app(dir.path());
    let path = dir.path().join("gone").join("missing.ospider");
    let error = app
        .dispatch(ctx(), "crawl.open", json!({"kind":"path","path":path}))
        .await
        .unwrap_err();
    assert_eq!(error.code, os_app::ErrorCode::NotFound);
    assert_eq!(error.message_key, "err-file-not-found");
    assert_eq!(error.args["path"], json!(path.display().to_string()));
    assert!(app.sessions().current(&window()).is_none());
}
