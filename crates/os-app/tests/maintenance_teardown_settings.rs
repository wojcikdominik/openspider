//! OUT-229: Eine Aufbewahrungsrunde löscht nach dem gespeicherten Einstellungsstand, nicht nach einer Kopie des zuletzt
//! gemeldeten (zwei gleichzeitige Änderungen können ihre Meldungen in umgekehrter Reihenfolge abliefern).
#![allow(clippy::disallowed_types, clippy::disallowed_methods)]
#[path = "runtime_common.rs"]
mod common;

use os_model::CrawlState;
use os_store::{CrawlFile, OpenMode};
use serde_json::json;

// spec: OUT-229
#[tokio::test]
async fn a_round_deletes_by_the_stored_settings_not_by_a_copy_of_the_last_reported_change() {
    // Der Speicher ändert sich, ohne dass die Änderung gemeldet wird (Stellvertreter für zwei gleichzeitige Änderungen,
    // deren Meldungen in umgekehrter Reihenfolge ankommen): Die Runde richtet sich nach dem gespeicherten Stand.
    let dir = tempfile::tempdir().unwrap();
    let crawls = dir.path().join("crawls");
    std::fs::create_dir_all(&crawls).unwrap();
    let id = common::crawl_id(41).to_string();
    let path = common::make_crawl(&crawls, &format!("{id}.ospider"));
    let file = CrawlFile::open(&path, OpenMode::ReadWrite).unwrap();
    file.set_meta("crawl_id", &id).unwrap();
    file.set_meta("state", CrawlState::Completed.as_str())
        .unwrap();
    file.set_meta("heartbeat_at", &common::created_at().to_db())
        .unwrap();
    file.set_meta("ended_at", "2000-01-01T00:00:00.000Z")
        .unwrap();
    drop(file);
    let app = common::new_app(dir.path());
    assert_eq!(
        os_app::commands::crawls::run_retention(&app).await.unwrap(),
        0,
        "Standard „Nie“: nichts gelöscht"
    );
    app.settings()
        .apply_patch(&json!({"retention":{"policy":"one_week"}}))
        .unwrap();
    assert_eq!(
        os_app::commands::crawls::run_retention(&app).await.unwrap(),
        1,
        "die Runde löschte nicht nach dem gespeicherten Stand „1 Woche“"
    );
    assert!(!path.exists());
}
