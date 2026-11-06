//! App neben Crawls anderer Prozesse und andere Prozesse neben Crawls der App (AGENT-020 Fremdprozess-Crawls, CRAWL-125,
//! OUT-228). Die Prozesse sind echt (Neustart dieser Testdatei in einer Rolle), weil es um Dateisperren je Prozess geht.
#![allow(clippy::disallowed_types, clippy::disallowed_methods)]
#[path = "session_common.rs"]
mod common;
use common::*;
use os_store::{registry, CrawlFile, OpenMode};
use serde_json::json;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const OWNER: &str = "OS_APP_WAL_TEST_OWNER";
const READER: &str = "OS_APP_WAL_TEST_READER";

fn companion(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(suffix);
    name.into()
}

/// Rolle „Prozess eines fremden Crawls“: hält eine Schreibverbindung und öffnet die Datei im Betrieb erneut, wie
/// Analyse-Worker, `crawl_status` und `--json-progress`. Ohne Umgebungsvariable keine Wirkung.
#[test]
fn owner_process_role() {
    let Some(path) = std::env::var_os(OWNER).map(PathBuf::from) else {
        return;
    };
    let file = CrawlFile::open(&path, OpenMode::ReadWrite).unwrap();
    file.conn()
        .execute_batch("PRAGMA wal_autocheckpoint=0")
        .unwrap();
    file.set_meta("probe", "before").unwrap();
    drop(CrawlFile::open(&path, OpenMode::ReadOnly).unwrap());
    println!("owner-ready");
    std::io::stdout().flush().unwrap();
    let mut line = String::new();
    std::io::stdin().read_line(&mut line).unwrap();
    assert!(companion(&path, "-wal").exists(), "-wal wurde entfernt");
    assert!(companion(&path, "-shm").exists(), "-shm wurde entfernt");
    file.set_meta("probe", "after").unwrap();
    let reader = CrawlFile::open(&path, OpenMode::ReadOnly).unwrap();
    assert_eq!(reader.get_meta("probe").unwrap().as_deref(), Some("after"));
    drop(reader);
    drop(file);
    assert!(CrawlFile::open(&path, OpenMode::ReadOnly)
        .unwrap()
        .integrity_check()
        .unwrap());
}

/// Rolle „anderer Prozess liest die Verwaltung“ (`--list-crawls`, zweiter MCP-Server, geplante Aufgabe).
#[test]
fn reader_process_role() {
    if let Some(path) = std::env::var_os(READER) {
        registry::read_listing(Path::new(&path)).unwrap();
    }
}

fn foreign_crawl(app: &os_app::App) -> PathBuf {
    let dir = app.config().paths.crawls.clone();
    std::fs::create_dir_all(&dir).unwrap();
    let id = "0199f000-0000-7000-8000-00000000f001";
    let path = dir.join(format!("{id}.ospider"));
    drop(create(&path, cfg()));
    let file = CrawlFile::open(&path, OpenMode::ReadWrite).unwrap();
    file.set_meta("crawl_id", id).unwrap();
    drop(file);
    path
}

/// Startet den fremden Besitzer, führt `during` aus und lässt ihn dann weiter schreiben und prüfen.
async fn with_foreign_owner<F: std::future::Future<Output = ()>>(path: &Path, during: F) {
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "owner_process_role", "--nocapture"])
        .env(OWNER, path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());
    let mut line = String::new();
    loop {
        line.clear();
        assert!(stdout.read_line(&mut line).unwrap() > 0, "Besitzer endete");
        if line.contains("owner-ready") {
            break;
        }
    }
    during.await;
    child.stdin.as_mut().unwrap().write_all(b"go\n").unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

// Crawl-Verwaltung der App (Dialog, Aufbewahrung, Fortsetzen-Prüfung) neben dem Crawl eines anderen Prozesses.
// spec: AGENT-020
// spec: OUT-228
// spec: CRAWL-125
#[tokio::test]
async fn the_crawl_management_keeps_the_wal_of_a_crawl_in_another_process() {
    let dir = tempfile::tempdir().unwrap();
    let app = app(dir.path());
    let path = foreign_crawl(&app);
    with_foreign_owner(&path, async {
        for _ in 0..3 {
            let listed = call(&app, "crawls.list", json!({"show":"all"})).await;
            assert_eq!(listed.as_array().unwrap().len(), 1, "{listed}");
        }
    })
    .await;
}

// Anzeige eines fremden Crawls: Öffnen und Polling (nur lesend) der Live-Ansicht, danach Schließen der App.
// spec: AGENT-020
// spec: CRAWL-125
#[tokio::test]
async fn opening_and_polling_a_crawl_of_another_process_keeps_its_wal() {
    let dir = tempfile::tempdir().unwrap();
    let app = app(dir.path());
    let path = foreign_crawl(&app);
    with_foreign_owner(&path, async {
        open(&app, &path).await;
        for _ in 0..5 {
            os_app::session::poll(&app).await.unwrap();
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        app.read_pool().unregister(&key(&app));
        app.write_queue().unregister(&key(&app));
    })
    .await;
}

// Ein Crawl der App: Die Verwaltung im App-Prozess darf seine Sperren nicht aufheben, ein anderer Prozess liest daneben.
// spec: AGENT-020
// spec: OUT-228
// spec: CRAWL-125
#[tokio::test]
async fn another_process_keeps_the_wal_of_an_app_crawl_after_the_app_listed_its_crawls() {
    let dir = tempfile::tempdir().unwrap();
    let app = app(dir.path());
    let mut site = os_engine::testing::SimSite::new().delay(std::time::Duration::from_millis(50));
    for n in 0..400 {
        let url = if n == 0 {
            "https://fixture.test/".to_owned()
        } else {
            format!("https://fixture.test/p/{n}")
        };
        site = site.page(&url, &format!("<a href='/p/{}'>weiter</a>", n + 1));
    }
    let started = os_app::session::control::start_with_fetcher(
        &app,
        ctx(),
        request("https://fixture.test/".into()),
        cfg(),
        os_engine::testing::SimFetcher::new(site),
    )
    .await
    .unwrap();
    let path = PathBuf::from(&started.file);
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    call(&app, "crawls.list", json!({"show":"all"})).await;
    let output = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "reader_process_role", "--nocapture"])
        .env(READER, &path)
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    assert!(companion(&path, "-wal").exists(), "-wal wurde entfernt");
    assert!(companion(&path, "-shm").exists(), "-shm wurde entfernt");
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    call(&app, "crawl.stop", json!(null)).await;
    app.read_pool().unregister(&key(&app));
    app.write_queue().unregister(&key(&app));
    let file = CrawlFile::open(&path, OpenMode::ReadOnly).unwrap();
    assert!(file.integrity_check().unwrap());
    let meta = file.meta().unwrap();
    assert_eq!(meta.state, os_model::CrawlState::Completed);
    assert_eq!(meta.end_reason, Some(os_model::EndReason::Stopped));
}
