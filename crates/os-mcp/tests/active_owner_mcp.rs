//! M4-K11, Kriterium (f) und Teil MCP von (b): `stop_crawl`, `pause_crawl` und `resume_crawl` des STDIO-Servers auf
//! einem abgeschlossenen bzw. pausierten Crawl, den ein anderer Prozess als aktiver Besitzer führt (KI-Lauf, simulierter
//! Heartbeat) – Fehler wie bei einem dort laufenden Crawl, mit `owner_kind` und PID, **ohne** Code `busy` (08 AI-020
//! [M4], Wirkung b; 01 CRAWL-125, Nachtrag K4-22). Der Server prüft mit der Systemuhr; der Heartbeat liegt deshalb in
//! der Zukunft und gilt nach CRAWL-125 als aktuell (keine Wanduhr im Test).
use std::path::{Path, PathBuf};

use os_mcp::tools::crawl::CrawlTool;
use os_mcp::{Tool, ToolContext, ToolError};
use os_model::time::{IdSource, SeededIds};
use os_model::{CrawlConfig, CrawlMode, Timestamp};
use os_store::{CrawlFile, CrawlInit, OpenMode, StorageProfile};
use serde_json::json;

const FUTURE: &str = "2099-01-01T00:00:00.000Z";

fn foreign_pid() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/crawl-files/active-owner/cases.json");
    let cases: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let pid = cases["foreign_pid"].as_str().unwrap().to_string();
    assert_ne!(pid, std::process::id().to_string());
    pid
}

/// Crawl-Datei `<id>.ospider` im Crawl-Ordner mit Zustand `state` und Besitzer `pid` (Art `cli`).
fn crawl(dir: &Path, seed: u64, state: &str, pid: &str) -> (String, PathBuf) {
    let created = Timestamp::from_db("2026-11-26T12:00:00.000Z").unwrap();
    let id = SeededIds::new(seed).new_crawl_id(created);
    let path = dir.join(format!("{id}.ospider"));
    let init = CrawlInit {
        crawl_id: id,
        mode: CrawlMode::Spider,
        start_url: Some("https://site.test/".into()),
        list_source: None,
        project: String::new(),
        name: "active-owner".into(),
        config: CrawlConfig::default(),
        created_at: created,
    };
    let file = CrawlFile::create(&path, &init, StorageProfile::Balanced).unwrap();
    file.set_meta("state", state).unwrap();
    file.set_meta("started_at", &created.to_db()).unwrap();
    if state == "Completed" {
        file.set_meta("end_reason", "limit_reached").unwrap();
    }
    file.set_meta("owner_kind", "cli").unwrap();
    file.set_meta("owner_pid", pid).unwrap();
    file.set_meta("heartbeat_at", FUTURE).unwrap();
    (id.to_string(), path)
}

fn call(ctx: &ToolContext, tool: &'static str, id: &str) -> Result<(), ToolError> {
    CrawlTool(tool)
        .call(ctx, json!({"crawl_id": id}))
        .map(|_| ())
}

fn meta(path: &Path, key: &str) -> String {
    CrawlFile::open(path, OpenMode::ReadOnly)
        .unwrap()
        .get_meta(key)
        .unwrap()
        .unwrap_or_default()
}

fn assert_foreign(err: &ToolError, pid: &str, what: &str) {
    assert_ne!(err.code(), "busy", "{what}: ohne busy");
    let text = err.to_string();
    assert!(
        text.contains("another process") && text.contains("cli") && text.contains(pid),
        "{what}: {text}"
    );
}

// spec: AI-020 [M4]
// spec: AGENT-020
// spec: CRAWL-125
#[test]
fn stop_and_pause_report_the_foreign_owner_without_busy() {
    let dir = tempfile::tempdir().unwrap();
    let pid = foreign_pid();
    let ctx = ToolContext::new(dir.path().into());
    for (seed, state) in [(1, "Completed"), (2, "Paused")] {
        let (id, path) = crawl(dir.path(), seed, state, &pid);
        for tool in ["stop_crawl", "pause_crawl"] {
            let err = call(&ctx, tool, &id).unwrap_err();
            assert_foreign(&err, &pid, &format!("{tool} {state}"));
        }
        assert_eq!(meta(&path, "state"), state, "nichts geschrieben");
        assert_eq!(meta(&path, "owner_pid"), pid);
    }
}

// spec: AI-020 [M4]
// spec: AGENT-020
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn resume_is_rejected_with_running_elsewhere_while_another_process_owns_the_crawl() {
    let dir = tempfile::tempdir().unwrap();
    let pid = foreign_pid();
    let (id, path) = crawl(dir.path(), 3, "Paused", &pid);
    let root = dir.path().to_path_buf();
    let err = tokio::task::spawn_blocking(move || {
        let ctx = ToolContext::new(root);
        call(&ctx, "resume_crawl", &id)
    })
    .await
    .unwrap()
    .unwrap_err();
    assert_ne!(err.code(), "busy");
    let text = err.to_string();
    assert!(
        text.contains("läuft noch") && text.contains(&format!("Prozess {pid}")),
        "{text}"
    );
    assert_eq!(meta(&path, "state"), "Paused");
    assert_eq!(meta(&path, "owner_pid"), pid);
}

/// Ohne aktiven Besitzer (owner_pid leer, wie 1.0 nach jedem geordneten Ende) bleibt es bei den Meldungen aus 1.0.
// spec: AI-020 [M4]
// spec: AGENT-020
#[test]
fn without_an_owner_the_messages_of_one_point_zero_remain() {
    let dir = tempfile::tempdir().unwrap();
    let ctx = ToolContext::new(dir.path().into());
    let (id, _) = crawl(dir.path(), 4, "Completed", "");
    let err = call(&ctx, "stop_crawl", &id).unwrap_err().to_string();
    assert!(err.contains("already finished"), "{err}");
    let (id, path) = crawl(dir.path(), 5, "Paused", "");
    let err = call(&ctx, "pause_crawl", &id).unwrap_err().to_string();
    assert!(err.contains("already paused"), "{err}");
    assert_eq!(meta(&path, "state"), "Paused");
}
