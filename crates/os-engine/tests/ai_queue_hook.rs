//! M4-D05: Engine ↔ KI-Warteschlange (08 AI-020 [M4], Nachtrag K4-13; AI-032; AI-039, E-6 a; Klärung K4-25 Variante A).
//! Die Engine kennt nur `os_model::ai::hook`; hier steht eine aufzeichnende Warteschlange an der Stelle von `os_ai::run`.
//! Geprüft: Seitenfelder nach AI-032 aus der geparsten Seite (Fixture `ai/run/site`, Seite `/fields` = `fields.html`), nur
//! für Seiten der Grundmenge G, das Ende je Endgrund (Ende ohne Stoppen: erst leer, dann `on_finished`), Fortsetzen mit dem
//! Leser der gecrawlten Seiten und ohne Warteschlange das Verhalten von 1.0.
#[path = "lifecycle_support.rs"]
mod support;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use os_engine::testing::{crawl_fixture_with, test_deps};
use os_engine::EngineDeps;
use os_model::ai::hook::{
    AiQueue, AiQueueDone, AiQueueEnd, AiQueueSession, AiQueueStart, InputSource, PageAiFields,
};
use os_model::{CrawlConfig, CrawlState};

#[derive(Default)]
struct Rec {
    begins: Mutex<Vec<(bool, bool)>>,
    pages: Mutex<Vec<PageAiFields>>,
    ends: Mutex<Vec<AiQueueEnd>>,
    drained: AtomicBool,
    hook_saw_drained: AtomicBool,
}

struct Queue(Arc<Rec>);
struct Session(Arc<Rec>);

impl AiQueue for Queue {
    fn begin(&self, start: AiQueueStart) -> Option<Arc<dyn AiQueueSession>> {
        self.0
            .begins
            .lock()
            .unwrap()
            .push((start.resumed, start.existing.is_some()));
        Some(Arc::new(Session(self.0.clone())))
    }
}

impl AiQueueSession for Session {
    fn page(&self, page: PageAiFields) {
        self.0.pages.lock().unwrap().push(page);
    }
    fn end(&self, end: AiQueueEnd) -> AiQueueDone {
        self.0.ends.lock().unwrap().push(end);
        let rec = self.0.clone();
        Box::pin(async move {
            if end == AiQueueEnd::Drain {
                // Die Warteschlange braucht noch etwas, bis sie leer ist.
                tokio::time::sleep(Duration::from_millis(200)).await;
                rec.drained.store(true, Ordering::SeqCst);
            }
        })
    }
}

fn deps_with(rec: &Arc<Rec>) -> EngineDeps {
    let mut deps = test_deps();
    deps.ai_queue = Some(Arc::new(Queue(rec.clone())));
    let probe = rec.clone();
    deps.on_finished = Some(Arc::new(move |_| {
        probe
            .hook_saw_drained
            .store(probe.drained.load(Ordering::SeqCst), Ordering::SeqCst);
    }));
    deps
}

fn page<'a>(rec: &'a [PageAiFields], base: &str, path: &str) -> &'a PageAiFields {
    rec.iter()
        .find(|p| p.url == format!("{base}{path}"))
        .unwrap_or_else(|| {
            panic!(
                "keine Seitenfelder für {path}: {:?}",
                rec.iter().map(|p| &p.url).collect::<Vec<_>>()
            )
        })
}

// spec: AI-032 [M4]
// spec: AI-039 [M4]
// Eingabefelder aus `fields.html` während des Crawls: Title, Meta Description, H1, alle H2, ONPAGE-140-Text, Wortzahl,
// `html_lang`, `hreflang_self`, Bildkontext; strukturierte Datentypen bei ausgeschalteter Extraktion (Standard) „nicht
// vorhanden“. Übergeben werden genau die Seiten der Grundmenge G (HTML, 404, 301), keine Ressourcen.
#[tokio::test]
async fn page_fields_follow_ai_032_and_cover_exactly_the_base_set() {
    let rec = Arc::new(Rec::default());
    let fx = crawl_fixture_with("ai/run/site", "/", CrawlConfig::default(), deps_with(&rec)).await;
    assert_eq!(fx.summary.state, CrawlState::Completed);
    let pages = rec.pages.lock().unwrap().clone();
    let base = fx.site.base_url();
    let mut paths: Vec<String> = pages
        .iter()
        .map(|p| p.url.trim_start_matches(&fx.site.base_url()).to_string())
        .collect();
    paths.sort();
    assert_eq!(
        paths,
        ["/", "/fields", "/gone", "/moved", "/p1", "/p2"],
        "Grundmenge G"
    );
    let f = page(&pages, &base, "/fields");
    assert_eq!(f.source, InputSource::Parsed);
    assert!(f.is_html_200());
    assert_eq!(f.title.as_deref(), Some("Wanderschuhe MARK-TITLE"));
    assert_eq!(
        f.meta_description.as_deref(),
        Some("Leichte Schuhe MARK-META")
    );
    assert_eq!(f.h1, ["Wanderschuhe MARK-H1"]);
    assert_eq!(f.h2, ["Pflege MARK-H2", "Größen MARK-H2"]);
    let text = f.page_text.as_deref().unwrap();
    assert!(
        text.contains("MARK-TEXT") && text.contains("MOCK-ID:fields"),
        "{text}"
    );
    assert!(
        !text.contains("MARK-H1"),
        "Kopfbereich gehört nicht zum Inhaltsbereich (ONPAGE-140): {text}"
    );
    assert!(f.word_count.unwrap() > 0);
    assert_eq!(f.html_lang.as_deref(), Some("MARK-LANG"));
    assert_eq!(f.hreflang_self, ["MARK-HREFLANG"]);
    assert_eq!(
        f.structured_data_types, None,
        "Extraktion aus (Standard): nicht vorhanden"
    );
    assert_eq!(f.images.len(), 1);
    assert_eq!(f.images[0].alt.as_deref(), Some("Schuh MARK-ALT"));
    assert_eq!(
        f.images[0].context.as_deref(),
        Some("Bildunterschrift MARK-CONTEXT")
    );
    assert!(f.indexable);
    let gone = page(&pages, &base, "/gone");
    assert_eq!(gone.status_code, Some(404));
    let moved = page(&pages, &base, "/moved");
    assert_eq!(moved.status_code, Some(301));
    assert!(!moved.is_html_200());
}

// spec: AI-032 [M4]
// K4-26: mit eingeschalteter JSON-LD-Extraktion die Liste der Typen, für eine Seite ohne strukturierte Daten eine leere Liste.
#[tokio::test]
async fn structured_data_types_follow_the_extraction_switch() {
    let rec = Arc::new(Rec::default());
    let mut cfg = CrawlConfig::default();
    cfg.extraction.structured_data.json_ld = true;
    let fx = crawl_fixture_with("ai/run/site", "/", cfg, deps_with(&rec)).await;
    let pages = rec.pages.lock().unwrap().clone();
    let base = fx.site.base_url();
    assert_eq!(
        page(&pages, &base, "/fields")
            .structured_data_types
            .as_deref(),
        Some(&["Product".to_string(), "MARK-SD".to_string()][..])
    );
    assert_eq!(
        page(&pages, &base, "/p1").structured_data_types.as_deref(),
        Some(&[][..])
    );
}

// spec: AI-020 [M4]
// Crawl-Ende ohne Stoppen: Die Engine meldet `Drain` und ruft `on_finished` erst, wenn die Warteschlange leer ist.
#[tokio::test]
async fn crawl_end_waits_for_the_queue_before_on_finished() {
    let rec = Arc::new(Rec::default());
    let fx = crawl_fixture_with("ai/run/site", "/", CrawlConfig::default(), deps_with(&rec)).await;
    assert_eq!(fx.summary.state, CrawlState::Completed);
    assert_eq!(*rec.ends.lock().unwrap(), [AiQueueEnd::Drain]);
    assert!(
        rec.hook_saw_drained.load(Ordering::SeqCst),
        "on_finished erst nach leerer Warteschlange"
    );
    assert_eq!(*rec.begins.lock().unwrap(), [(false, false)]);
}

// spec: AI-020 [M4]
// Pause → `Paused` (kein neuer Request), Fortsetzen → neue Sitzung mit `resumed` und dem Leser der gecrawlten Seiten,
// Stoppen → `Stopped`.
#[tokio::test]
async fn pause_resume_and_stop_reach_the_queue() {
    let dir = support::slow_site_dir(40);
    let site = support::start_site(dir.path()).await;
    let work = tempfile::tempdir().unwrap();
    let path = work.path().join("crawl.ospider");
    let rec = Arc::new(Rec::default());
    let cfg = support::config_for(&site, 1);
    let handle = support::start(cfg, &path, deps_with(&rec)).await;
    support::until_crawled(&handle, 3).await;
    handle.pause();
    let paused = support::run_to_end(handle).await;
    assert_eq!(paused.state, CrawlState::Paused);
    assert_eq!(*rec.ends.lock().unwrap(), [AiQueueEnd::Paused]);
    let crawled_before = rec.pages.lock().unwrap().len();
    assert!(crawled_before >= 3);

    let cfg = support::resume_config(&path, &site, |_| {});
    let handle = support::resume(cfg, &path, deps_with(&rec)).await;
    support::until_crawled(&handle, paused.crawled + 2).await;
    handle.stop();
    let stopped = support::run_to_end(handle).await;
    assert_eq!(stopped.end_reason, Some(os_model::EndReason::Stopped));
    assert_eq!(*rec.begins.lock().unwrap(), [(false, false), (true, true)]);
    assert_eq!(
        *rec.ends.lock().unwrap(),
        [AiQueueEnd::Paused, AiQueueEnd::Stopped]
    );
    assert!(rec.pages.lock().unwrap().len() > crawled_before);
}

// spec: AI-001 [M4]
// Schalter „KI (Vorschau)“ aus = keine Warteschlange in den `EngineDeps`: Der Crawl verhält sich wie 1.0 (Ergebnis und
// Zustand), es entstehen keine gespeicherten KI-Eingaben.
#[tokio::test]
async fn without_a_queue_the_crawl_behaves_like_one_point_zero() {
    let fx = crawl_fixture_with("ai/run/site", "/", CrawlConfig::default(), test_deps()).await;
    assert_eq!(fx.summary.state, CrawlState::Completed);
    let inputs = os_store::ai::inputs::size(fx.file.conn()).unwrap();
    assert_eq!(inputs.entries, 0);
}

// spec: AI-032 [M4]
// Befund A4 der Abnahme M4-D05 (K4-26, `hreflang_self`): Ein **absoluter** Selbstverweis mit Port
// (`http://127.0.0.1:<port>/french.html`, Platzhalter `{{PORT}}` in einer statischen Datei der Testsite ersetzt) ist
// selbstreferenzierend – während des Crawls (geparste Seite) und nach dem Crawl (Tabelle `hreflang`, `load_pages`).
// Der Verweis auf eine andere Seite (`de` → `/`) zählt nicht.
#[tokio::test]
async fn absolute_self_reference_with_port_is_hreflang_self() {
    let rec = Arc::new(Rec::default());
    let fx = crawl_fixture_with(
        "ai/run/hreflang",
        "/",
        CrawlConfig::default(),
        deps_with(&rec),
    )
    .await;
    assert_eq!(fx.summary.state, CrawlState::Completed);
    let base = fx.site.base_url();
    let pages = rec.pages.lock().unwrap().clone();
    let during = page(&pages, &base, "/french.html");
    assert_eq!(during.hreflang_self, ["fr"], "während des Crawls");
    let stored: Vec<(String, Option<String>)> = {
        let mut stmt = fx
            .file
            .conn()
            .prepare("SELECT h.code_raw, h.url FROM hreflang h JOIN urls u ON u.id = h.url_id WHERE u.address LIKE '%/french.html' ORDER BY h.position")
            .unwrap();
        stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
    };
    assert_eq!(
        stored,
        [
            ("fr".to_string(), Some(format!("{base}/french.html"))),
            ("de".to_string(), Some(format!("{base}/"))),
        ],
        "aufgelöste Ziele in der Tabelle hreflang"
    );
    let after = os_engine::ai_input::load_pages(fx.file.path(), &CrawlConfig::default()).unwrap();
    let french = after
        .iter()
        .find(|p| p.url.ends_with("/french.html"))
        .expect("Seite nach dem Crawl");
    assert_eq!(french.hreflang_self, ["fr"], "nach dem Crawl");
}

/// Warteschlange, deren Ende ohne Stoppen erst nach `release` fertig wird (gehaltener Request).
#[derive(Default)]
struct Gate {
    ends: Mutex<Vec<AiQueueEnd>>,
    release: tokio::sync::Notify,
    hook_ran: AtomicBool,
}

struct GateQueue(Arc<Gate>);
struct GateSession(Arc<Gate>);

impl AiQueue for GateQueue {
    fn begin(&self, _start: AiQueueStart) -> Option<Arc<dyn AiQueueSession>> {
        Some(Arc::new(GateSession(self.0.clone())))
    }
}

impl AiQueueSession for GateSession {
    fn page(&self, _page: PageAiFields) {}
    fn end(&self, end: AiQueueEnd) -> AiQueueDone {
        self.0.ends.lock().unwrap().push(end);
        let gate = self.0.clone();
        Box::pin(async move {
            if end == AiQueueEnd::Drain {
                gate.release.notified().await;
            }
        })
    }
}

fn gate_deps(gate: &Arc<Gate>) -> EngineDeps {
    let mut deps = test_deps();
    deps.ai_queue = Some(Arc::new(GateQueue(gate.clone())));
    let probe = gate.clone();
    deps.on_finished = Some(Arc::new(move |_| {
        probe.hook_ran.store(true, Ordering::SeqCst);
    }));
    deps
}

// spec: AI-020 [M4]
// spec: CRAWL-141
// Nachtrag 28.11.2026 („Vollständig“ beim Crawl-Ende ohne Stoppen): Mit dem Ende der Abrufe steht `Completed` in der Datei
// und `CrawlHandle::ended` liefert die Zusammenfassung, während die Warteschlange noch läuft (die Statusabfrage der App
// wartet darauf, nicht auf `wait()`); `on_finished` (Crawl-Analyse, Exporte der CLI) und `wait()` erst danach.
#[tokio::test]
async fn ended_reports_the_end_state_while_the_queue_drains() {
    let dir = support::slow_site_dir(0);
    let site = support::start_site(dir.path()).await;
    let work = tempfile::tempdir().unwrap();
    let path = work.path().join("crawl.ospider");
    let gate = Arc::new(Gate::default());
    let mut cfg = support::config_for(&site, 2);
    cfg.limits.max_urls = 5;
    let handle = support::start(cfg, &path, gate_deps(&gate)).await;
    let summary = tokio::time::timeout(Duration::from_secs(30), handle.ended())
        .await
        .expect("Endzustand ohne Warten auf die Warteschlange")
        .expect("Phase „nur noch KI-Warteschlange“");
    assert_eq!(summary.state, CrawlState::Completed);
    assert_eq!(summary.end_reason, Some(os_model::EndReason::LimitReached));
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(!handle.is_finished(), "wait() wartet auf die Warteschlange");
    assert!(
        !gate.hook_ran.load(Ordering::SeqCst),
        "on_finished erst mit leerer Warteschlange"
    );
    let file = os_store::CrawlFile::open(&path, os_store::OpenMode::ReadOnly).unwrap();
    assert_eq!(
        file.get_meta("state").unwrap().as_deref(),
        Some("Completed")
    );
    assert_eq!(file.get_meta("owner_pid").unwrap().as_deref(), Some(""));
    drop(file);
    gate.release.notify_one();
    let done = support::run_to_end(handle).await;
    assert_eq!(done, summary);
    assert!(gate.hook_ran.load(Ordering::SeqCst));
    assert_eq!(*gate.ends.lock().unwrap(), [AiQueueEnd::Drain]);
}

// spec: AI-020 [M4]
// spec: CRAWL-126 [M4]
// Nachtrag 28.11.2026 („SIGINT, während nur noch die KI-Warteschlange läuft“): Pause in dieser Phase → die Warteschlange
// erhält `Paused` (kein weiterer Request, laufende werden übernommen), der Zustand bleibt `Completed`; `wait()` endet,
// ohne dass die Warteschlange leer sein muss.
#[tokio::test]
async fn pause_while_only_the_queue_runs_keeps_completed() {
    let dir = support::slow_site_dir(0);
    let site = support::start_site(dir.path()).await;
    let work = tempfile::tempdir().unwrap();
    let path = work.path().join("crawl.ospider");
    let gate = Arc::new(Gate::default());
    let mut cfg = support::config_for(&site, 2);
    cfg.limits.max_urls = 5;
    let handle = support::start(cfg, &path, gate_deps(&gate)).await;
    tokio::time::timeout(Duration::from_secs(30), handle.ended())
        .await
        .unwrap()
        .unwrap();
    let pause = handle.pauser();
    let wait = tokio::spawn(handle.wait());
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(!wait.is_finished());
    pause();
    let summary = tokio::time::timeout(Duration::from_secs(10), wait)
        .await
        .expect("Pause beendet das Warten auf die Warteschlange")
        .unwrap()
        .unwrap();
    assert_eq!(summary.state, CrawlState::Completed);
    assert_eq!(
        *gate.ends.lock().unwrap(),
        [AiQueueEnd::Drain, AiQueueEnd::Paused]
    );
    let file = os_store::CrawlFile::open(&path, os_store::OpenMode::ReadOnly).unwrap();
    assert_eq!(
        file.get_meta("state").unwrap().as_deref(),
        Some("Completed")
    );
}
