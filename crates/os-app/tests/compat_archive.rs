#![allow(clippy::disallowed_types, clippy::disallowed_methods)]
#[path = "session_common.rs"]
mod common;
// Intentionally include the real desktop open.rs via #[path] to test the forwarding
// adapter without GTK/WebView; Tauri reexports url::Url. If open.rs starts using
// Tauri runtime APIs, move this coverage into a src-tauri unit test.
extern crate self as tauri;
pub use reqwest::Url;
#[path = "../../../apps/desktop/src-tauri/src/open.rs"]
mod desktop_open;

use common::*;
use os_app::{dto::UiEvent, events::CollectingSink};
use std::{fs, path::PathBuf, sync::Arc};

fn archive() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/migrations")
}

// spec: OUT-255
// spec: OUT-226 [M3]
#[tokio::test]
async fn app_opens_every_archived_schema_without_touching_originals() {
    let mut stands: Vec<_> = (1..=5)
        .map(|version| archive().join(format!("schema-v{version}")))
        .collect();
    let releases = archive().join("releases");
    if releases.is_dir() {
        stands.extend(fs::read_dir(releases).unwrap().map(|e| e.unwrap().path()));
    }
    for stand in stands {
        let dir = tempfile::tempdir().unwrap();
        let app = app(dir.path());
        let source = stand.join("crawl.ospider");
        let bytes = fs::read(&source).unwrap();
        let path = dir.path().join("archived.ospider");
        fs::write(&path, &bytes).unwrap();
        let summary = open(&app, &path).await;
        assert_eq!(summary.schema_version, 5);
        assert_eq!(summary.phase, os_app::dto::CrawlPhase::Completed);
        // CLI ensures pending analysis before exports; reach the same state through the app command.
        let analysis = operation(&app, "analysis.start", serde_json::json!({"steps":null})).await;
        assert_eq!(
            analysis.state,
            os_app::dto::OpState::Done,
            "{:?}",
            analysis.error
        );
        let exported = operation(&app, "export.multi", serde_json::json!({
            "items":[{"kind":"tab","name":"Internal:All"}],
            "format":"csv", "dir":dir.path().join("out"), "singleWorkbook":false, "skipEmpty":false
        })).await;
        assert_eq!(
            exported.state,
            os_app::dto::OpState::Done,
            "{:?}",
            exported.error
        );
        let result = exported.result.unwrap();
        let csv = fs::read_to_string(result["files"][0]["path"].as_str().unwrap()).unwrap();
        os_testsite::golden::assert_golden(
            stand.join("internal_all.csv"),
            &os_testsite::golden::normalize(&csv, &[]),
        );
        assert_eq!(fs::read(&source).unwrap(), bytes);
        if stand.join("manifest.json").is_file() {
            os_model::CrawlConfig::load_file(&stand.join("profile.osconfig")).unwrap();
            os_export::types::ExportPreset::load(&stand.join("basic.ospreset")).unwrap();
            os_sched::tasks::bundle::import_text(
                &fs::read_to_string(stand.join("nightly.ostask")).unwrap(),
            )
            .unwrap();
            os_model::settings::AppSettings::load(&stand).unwrap();
            assert_eq!(
                os_app::uistate::UiStateStore::new(&stand).load().state["activeTab"],
                "Internal"
            );
            os_store::auth_profiles::open(
                &fs::read(stand.join("profile.osauth")).unwrap(),
                None,
                |_| Some("11".repeat(32)),
            )
            .unwrap();
        }
    }
}

// spec: OUT-232 [M3]
// spec: OUT-255
#[tokio::test]
async fn archived_file_associations_forward_to_existing_app_as_open_notices() {
    let dir = tempfile::tempdir().unwrap();
    let app = app(dir.path());
    let sink = Arc::new(CollectingSink::new());
    app.subscribe(window(), sink.clone());
    let files = [
        "schema-v1/crawl.ospider",
        "config-v1/profile.osconfig",
        "app-v1/basic.ospreset",
        "app-v1/nightly.ostask",
        "app-v1/profile.osauth",
    ];
    let args: Vec<_> = std::iter::once("openspider".to_string())
        .chain(
            files
                .iter()
                .map(|p| archive().join(p).to_str().unwrap().to_string()),
        )
        .collect();
    let requests = desktop_open::requests_from_args(&args, None);
    let urls: Vec<_> = requests
        .iter()
        .map(|r| Url::from_file_path(&r.path).unwrap())
        .collect();
    assert_eq!(desktop_open::requests_from_urls(&urls), requests);
    assert_eq!(desktop_open::forward(&app, &requests), files.len());
    let events = sink.take();
    let notices: Vec<_> = events
        .iter()
        .flat_map(|event| match event {
            UiEvent::Tick { tick } => tick.notices.iter().collect::<Vec<_>>(),
            _ => vec![],
        })
        .filter(|notice| notice.code == "open-file")
        .collect();
    assert_eq!(notices.len(), files.len());
    for (notice, request) in notices.iter().zip(requests.iter()) {
        assert_eq!(notice.args["kind"], request.kind.as_str());
        assert_eq!(notice.args["path"], request.path.to_str().unwrap());
    }
}

// spec: OUT-255
#[tokio::test]
async fn fixed_data_directory_keeps_settings_ui_state_and_crawl_management_after_restart() {
    let dir = tempfile::tempdir().unwrap();
    let paths = os_store::paths::app_paths_with_home(Some(dir.path().as_os_str()));
    paths.create_all().unwrap();
    for name in ["settings.toml", "ui-state.json"] {
        fs::copy(
            archive().join("app-v1").join(name),
            paths.settings.join(name),
        )
        .unwrap();
    }
    let id = "0199aaaa-0000-7000-8000-000000000001";
    fs::copy(
        archive().join("schema-v1/crawl.ospider"),
        paths.crawls.join(format!("{id}.ospider")),
    )
    .unwrap();
    for _ in 0..2 {
        let app = app(dir.path());
        assert_eq!(
            app.settings_snapshot().general.language,
            os_model::UiLanguage::En
        );
        let ui = os_app::uistate::UiStateStore::new(&paths.settings);
        assert_eq!(ui.load().state["activeTab"], "Internal");
        let listing = os_store::registry::Manager::open(&paths.crawls)
            .unwrap()
            .list()
            .unwrap();
        assert!(listing.skipped.is_empty());
        assert_eq!(listing.crawls.len(), 1);
        assert_eq!(listing.crawls[0].meta.name, "Archived crawl");
        let summary = call(
            &app,
            "crawl.open",
            serde_json::json!({"kind":"id","crawlId":id}),
        )
        .await;
        assert_eq!(summary["schemaVersion"], 5);
        let loaded = ui.load();
        ui.save(&loaded).unwrap();
    }
    let manifest: serde_json::Value = serde_json::from_str(include_str!(
        "../../../apps/desktop/src-tauri/tauri.conf.json"
    ))
    .unwrap();
    assert_eq!(manifest["identifier"], "org.openspider.desktop");
}

// spec: OUT-255
// spec: OUT-226 [M3]
#[tokio::test]
async fn reopening_legacy_crawl_preserves_previous_migrations_and_occupied_names() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("legacy.ospider");
    fs::copy(archive().join("schema-v1/crawl.ospider"), &source).unwrap();
    let original = fs::read(&source).unwrap();
    let first = {
        let app = app(dir.path());
        let summary = open(&app, &source).await;
        // Close the crawl before taking the reference bytes: background cache writes of the opened copy end there,
        // and the last write connection folds its WAL into the file (no handle stays open, UI-003).
        call(&app, "crawl.clear", serde_json::json!(null)).await;
        PathBuf::from(summary.file)
    };
    let migrated = fs::read(&first).unwrap();
    let occupied = dir.path().join("legacy.migrated-2.ospider");
    fs::write(&occupied, b"existing file must survive").unwrap();
    let app = app(dir.path());
    let reopened = open(&app, &source).await;
    assert_eq!(reopened.schema_version, 5);
    // Opening canonicalizes (macOS /private/var, Windows \\?\ and 8.3 names): compare the same file.
    assert_eq!(
        fs::canonicalize(PathBuf::from(reopened.file)).unwrap(),
        fs::canonicalize(dir.path().join("legacy.migrated-3.ospider")).unwrap()
    );
    assert_eq!(fs::read(first).unwrap(), migrated);
    assert_eq!(fs::read(occupied).unwrap(), b"existing file must survive");
    assert_eq!(fs::read(source).unwrap(), original);
}
