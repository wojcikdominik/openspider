//! RC1-RECENT: „Zuletzt geöffnet“ zeigt jede Crawl-Datei genau einmal, gleich über welche Pfadschreibweise
//! (symbolische Verknüpfung, z. B. `/tmp` → `/private/tmp` unter macOS) sie geöffnet oder gespeichert wurde.
#![allow(clippy::disallowed_types, clippy::disallowed_methods)]
#[path = "session_common.rs"]
mod common;
use common::*;
use serde_json::{json, Value};
use std::time::Duration;

fn recent(app: &os_app::App) -> Vec<Value> {
    app.uistate().load().state["recentCrawls"]
        .as_array()
        .cloned()
        .unwrap_or_default()
}

async fn loaded_recent(app: &os_app::App) -> Vec<Value> {
    call(app, "uistate.load", json!(null)).await["state"]["recentCrawls"]
        .as_array()
        .cloned()
        .unwrap_or_default()
}

/// Wie die Crawl-Verwaltung der Oberfläche: Eintrag mit der vom Kern gemeldeten Datei vorne einfügen
/// (Textvergleich der Pfade) und über `uistate.save` speichern.
async fn frontend_prepends(app: &os_app::App, path: &str) {
    let mut saved = call(app, "uistate.load", json!(null)).await;
    let old = saved["state"]["recentCrawls"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let mut list = vec![json!({ "path": path })];
    list.extend(old.into_iter().filter(|entry| entry["path"] != path));
    saved["state"]["recentCrawls"] = Value::Array(list);
    call(app, "uistate.save", saved).await;
}

// spec: CRAWL-127
#[cfg(unix)]
#[tokio::test]
async fn a_crawl_behind_a_symbolic_link_appears_once_in_recently_opened() {
    tokio::time::timeout(Duration::from_secs(30), async {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("home");
        let real = dir.path().join("real");
        std::fs::create_dir_all(&real).unwrap();
        let link = dir.path().join("link");
        std::os::unix::fs::symlink(&real, &link).unwrap();
        drop(create(&real.join("site.ospider"), cfg()));
        let app = app(&home);
        let via_link = link.join("site.ospider");
        let resolved = std::fs::canonicalize(&via_link).unwrap();
        assert_ne!(via_link, resolved, "fixture must use two spellings");

        // Opened through the link: the core remembers the spelling the user chose, not the resolved one.
        let summary = open(&app, &via_link).await;
        assert_eq!(summary.file, resolved.to_string_lossy());
        let entries = recent(&app);
        assert_eq!(entries.len(), 1, "{entries:?}");
        assert_eq!(entries[0]["path"], via_link.to_string_lossy().into_owned());

        // The UI then prepends the file reported by the core (resolved spelling): still one entry, known spelling.
        frontend_prepends(&app, &summary.file).await;
        let entries = loaded_recent(&app).await;
        assert_eq!(entries.len(), 1, "{entries:?}");
        assert_eq!(entries[0]["path"], via_link.to_string_lossy().into_owned());

        // Opened again with the resolved spelling: still one entry.
        open(&app, &resolved).await;
        let entries = loaded_recent(&app).await;
        assert_eq!(entries.len(), 1, "{entries:?}");
        assert_eq!(entries[0]["path"], via_link.to_string_lossy().into_owned());

        // A second crawl keeps its own entry, newest first.
        drop(create(&real.join("other.ospider"), cfg()));
        open(&app, &link.join("other.ospider")).await;
        let entries = loaded_recent(&app).await;
        assert_eq!(entries.len(), 2, "{entries:?}");
        assert_eq!(
            entries[0]["path"],
            link.join("other.ospider").to_string_lossy().into_owned()
        );
        assert_eq!(entries[1]["path"], via_link.to_string_lossy().into_owned());
    })
    .await
    .expect("RC1-RECENT fixture exceeded its deadline");
}

// spec: CRAWL-127
#[cfg(unix)]
#[tokio::test]
async fn duplicates_saved_by_beta_4_are_merged_on_load_newest_wins() {
    tokio::time::timeout(Duration::from_secs(30), async {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("home");
        let real = dir.path().join("real");
        std::fs::create_dir_all(&real).unwrap();
        let link = dir.path().join("link");
        std::os::unix::fs::symlink(&real, &link).unwrap();
        drop(create(&real.join("site.ospider"), cfg()));
        let resolved = std::fs::canonicalize(real.join("site.ospider")).unwrap();
        let via_link = link.join("site.ospider");
        // A file that no longer exists, saved in both spellings: merged as well, entry kept (removed on click).
        let gone_link = link.join("gone.ospider");
        let gone_real = std::fs::canonicalize(&real).unwrap().join("gone.ospider");
        let app = app(&home);
        // State as written by beta.4: newest first, the same file under two spellings.
        app.uistate()
            .update(|saved| {
                saved.state = json!({
                    "layout": {"split": 0.3},
                    "recentCrawls": [
                        {"path": resolved, "label": "neu"},
                        {"path": gone_link},
                        {"path": "/nowhere/missing.ospider"},
                        {"path": via_link, "label": "alt"},
                        {"path": gone_real, "label": "weg"},
                    ]
                })
            })
            .unwrap();

        let loaded = call(&app, "uistate.load", json!(null)).await;
        assert_eq!(loaded["state"]["layout"]["split"], 0.3);
        assert_eq!(
            loaded["state"]["recentCrawls"],
            json!([
                // Position and label of the newer entry, the spelling the user knows.
                {"path": via_link, "label": "neu"},
                {"path": gone_link, "label": "weg"},
                {"path": "/nowhere/missing.ospider"},
            ])
        );
        // Saving the loaded state persists the merged list.
        call(&app, "uistate.save", loaded.clone()).await;
        assert_eq!(
            app.uistate().load().state["recentCrawls"],
            loaded["state"]["recentCrawls"]
        );
    })
    .await
    .expect("RC1-RECENT fixture exceeded its deadline");
}

// spec: CRAWL-127
// Windows: resolving yields `\\?\C:\…`; "Recently opened" must show the path as the user knows it.
#[cfg(windows)]
#[tokio::test]
async fn recently_opened_never_shows_verbatim_windows_paths() {
    tokio::time::timeout(Duration::from_secs(30), async {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("home");
        let file = dir.path().join("site.ospider");
        drop(create(&file, cfg()));
        let app = app(&home);
        let summary = open(&app, &file).await;
        frontend_prepends(&app, &summary.file).await;
        let entries = loaded_recent(&app).await;
        assert_eq!(entries.len(), 1, "{entries:?}");
        let shown = entries[0]["path"].as_str().unwrap();
        assert!(!shown.starts_with(r"\\?\"), "{shown}");
        assert_eq!(shown, file.to_string_lossy());
    })
    .await
    .expect("RC1-RECENT fixture exceeded its deadline");
}
