//! M4-D05, Kriterium (h): Jeder KI-Lauf über den Laufrahmen trägt den Besitz (`owner_kind`, `owner_pid`, Heartbeat) und
//! gibt ihn in jedem Ende-Pfad frei (fertig, Pause, Stopp, Bestätigung ausstehend, Budget, Abbruch, Fehler); der Zustand
//! des Crawls bleibt `Completed` (08 AI-020 [M4], Nachtrag K4-13, „Wer schreibt“). Gegen den ai-mock mit `hold`.
#[path = "run_support.rs"]
mod rs;

use os_ai::decisions::exec::RunEnd;
use os_ai::run::{controls, run_decisions, JobConfirmation};
use os_model::budget::MicroUsd;
use rs::*;

fn pid() -> String {
    std::process::id().to_string()
}

/// Wartet, bis der Mock `n` gehaltene Decisions-Requests zeigt, prüft den Besitz und führt `then` aus.
async fn while_held(mock: &Mock, path: &std::path::Path, n: usize, then: impl FnOnce()) {
    mock.until("Request gehalten", |_| mock.held(DECISIONS) >= n)
        .await;
    let (kind, owner_pid, state) = owner(path);
    assert_eq!((kind.as_str(), owner_pid.as_str()), ("app", pid().as_str()), "Besitz während des gehaltenen Requests");
    assert_eq!(state, "Completed", "Zustand unverändert");
    then();
}

async fn setup(budget: u64) -> (Mock, TempDir, std::path::PathBuf, os_model::CrawlConfig, Vec<os_model::ai::hook::PageAiFields>) {
    let mock = Mock::start().await;
    let dir = tmp("d05-owner");
    let pages = site_pages("site");
    let path = crawl_file(&dir.0, &pages);
    let cfg = config(&[question_json("page-type")], budget);
    (mock, dir, path, cfg, pages)
}

fn released(path: &std::path::Path) {
    let (_, owner_pid, state) = owner(path);
    assert_eq!(owner_pid, "", "owner_pid nach dem Ende leer");
    assert_eq!(state, "Completed");
}

// spec: AI-020 [M4]
#[tokio::test]
async fn completed_run_holds_and_releases_the_owner() {
    let (mock, _dir, path, cfg, pages) = setup(5_000_000).await;
    let env = env(&mock, &["openai"], NO_CONFIRMATION);
    mock.hold("openai", 1).await;
    let run = run_decisions(&env, job(&path, &cfg, &pages, JobConfirmation::Confirmed));
    let driver = async {
        while_held(&mock, &path, 1, || {}).await;
        mock.release("openai", 1).await;
    };
    let (outcome, ()) = tokio::join!(run, driver);
    assert_eq!(outcome.unwrap().end, RunEnd::Completed);
    released(&path);
}

// spec: AI-020 [M4]
#[tokio::test]
async fn paused_run_releases_the_owner() {
    let (mock, _dir, path, cfg, pages) = setup(5_000_000).await;
    let env = env(&mock, &["openai"], NO_CONFIRMATION);
    mock.hold("openai", 1).await;
    let run = run_decisions(&env, job(&path, &cfg, &pages, JobConfirmation::Confirmed));
    let driver = async {
        while_held(&mock, &path, 1, || controls().pause(&path)).await;
        mock.release("openai", 1).await;
    };
    let (outcome, ()) = tokio::join!(run, driver);
    let outcome = outcome.unwrap();
    assert_eq!(outcome.end, RunEnd::Paused);
    assert_eq!(decisions_posts(&mock), 1, "nach der Pause kein weiterer Request");
    released(&path);
}

// spec: AI-020 [M4]
#[tokio::test]
async fn stopped_run_releases_the_owner() {
    let (mock, _dir, path, cfg, pages) = setup(5_000_000).await;
    let env = env(&mock, &["openai"], NO_CONFIRMATION);
    mock.hold("openai", 1).await;
    let run = run_decisions(&env, job(&path, &cfg, &pages, JobConfirmation::Confirmed));
    let driver = async {
        while_held(&mock, &path, 1, || controls().stop(&path)).await;
        mock.release("openai", 1).await;
    };
    let (outcome, ()) = tokio::join!(run, driver);
    assert_eq!(outcome.unwrap().end, RunEnd::Stopped);
    assert_eq!(decisions_posts(&mock), 1);
    released(&path);
}

// spec: AI-020 [M4]
// Abbruch (geordnetes Beenden, Schließen der Datei): laufender Request verworfen, Eintrag `Pending`, keine Protokollzeile.
#[tokio::test]
async fn aborted_run_releases_the_owner_and_leaves_the_entry_pending() {
    let (mock, _dir, path, cfg, pages) = setup(5_000_000).await;
    let env = env(&mock, &["openai"], NO_CONFIRMATION);
    mock.hold("openai", 1).await;
    let run = run_decisions(&env, job(&path, &cfg, &pages, JobConfirmation::Confirmed));
    let driver = async {
        while_held(&mock, &path, 1, || controls().abort(&path)).await;
    };
    let (outcome, ()) = tokio::join!(run, driver);
    assert_eq!(outcome.unwrap().end, RunEnd::Aborted);
    assert_eq!(logged(&path), 0, "keine Protokollzeile");
    assert_eq!(counts(&path, "page-type").get("Pending"), Some(&22));
    released(&path);
}

// spec: AI-020 [M4]
// Bestätigung ausstehend (Schwelle 0,0001 USD): kein Request, Besitz freigegeben.
#[tokio::test]
async fn pending_confirmation_releases_the_owner() {
    let (mock, _dir, path, cfg, pages) = setup(5_000_000).await;
    let env = env(&mock, &["openai"], MicroUsd(100));
    let outcome = run_decisions(&env, job(&path, &cfg, &pages, JobConfirmation::Unconfirmed))
        .await
        .unwrap();
    assert_eq!(outcome.end, RunEnd::NeedsConfirmation);
    assert_eq!(decisions_posts(&mock), 0);
    released(&path);
}

// spec: AI-020 [M4]
// Budget erreicht: der Lauf endet regulär mit `Budget exceeded`, Besitz freigegeben.
#[tokio::test]
async fn budget_end_releases_the_owner() {
    let (mock, _dir, path, cfg, pages) = setup(1).await;
    let env = env(&mock, &["openai"], NO_CONFIRMATION);
    mock.hold("openai", 1).await;
    let outcome = run_decisions(&env, job(&path, &cfg, &pages, JobConfirmation::Confirmed))
        .await
        .unwrap();
    assert_eq!(outcome.end, RunEnd::Completed);
    assert_eq!(counts(&path, "page-type").get("Budget exceeded"), Some(&22));
    assert_eq!(decisions_posts(&mock), 0);
    released(&path);
}

// spec: AI-020 [M4]
// Fehler des Laufs nach dem Beginn des Besitzes (andere Definition derselben Fragenversion in der Datei): Fehler, Besitz
// freigegeben.
#[tokio::test]
async fn failed_run_releases_the_owner() {
    let (mock, _dir, path, cfg, pages) = setup(5_000_000).await;
    {
        let f = os_store::CrawlFile::open(&path, os_store::OpenMode::ReadWrite).unwrap();
        f.conn()
            .execute(
                "INSERT INTO ai_questions VALUES('page-type','1.0.0','{\"id\":\"page-type\",\"other\":true}')",
                [],
            )
            .unwrap();
    }
    let env = env(&mock, &["openai"], NO_CONFIRMATION);
    let outcome = run_decisions(&env, job(&path, &cfg, &pages, JobConfirmation::Confirmed)).await;
    assert!(outcome.is_err(), "{outcome:?}");
    assert_eq!(decisions_posts(&mock), 0);
    released(&path);
}

/// Crawl-Datei unter dem Registernamen (`registry::path_for`) im Ordner `dir`, damit die Crawl-Verwaltung sie kennt.
fn registered(dir: &std::path::Path, pages: &[os_model::ai::hook::PageAiFields]) -> (os_model::CrawlId, std::path::PathBuf) {
    let path = crawl_file(dir, pages);
    let id = os_store::CrawlFile::open(&path, os_store::OpenMode::ReadOnly)
        .unwrap()
        .meta()
        .unwrap()
        .crawl_id;
    let target = os_store::registry::path_for(dir, &id);
    std::fs::rename(&path, &target).unwrap();
    (id, target)
}

// spec: AI-020 [M4]
// Ersetzt den Platzhalter `decisions_lifecycle::delete_protection_during_an_ai_run` (Kernteil, siehe Bericht d05):
// Solange der Mock Request 1 eines KI-Laufs auf einem abgeschlossenen Crawl hält, lehnt die Crawl-Verwaltung das Löschen ab
// wie bei einem laufenden Crawl (OUT-228); nach dem Ende des Laufs ist Löschen möglich.
#[tokio::test]
async fn delete_protection_during_an_ai_run_via_the_run_frame() {
    let mock = Mock::start().await;
    let dir = tmp("d05-delete");
    let pages = site_pages("site");
    let (id, path) = registered(&dir.0, &pages);
    let cfg = config(&[question_json("page-type")], 5_000_000);
    let env = env(&mock, &["openai"], NO_CONFIRMATION);
    mock.hold("openai", 1).await;
    let run = run_decisions(&env, job(&path, &cfg, &pages, JobConfirmation::Confirmed));
    let driver = async {
        mock.until("gehalten", |_| mock.held(DECISIONS) >= 1).await;
        let mut m = os_store::registry::Manager::open(&dir.0).unwrap();
        let now = os_model::Clock::now(&os_model::SystemClock);
        assert!(m.delete(&id, now).is_err(), "Löschen während des KI-Laufs abgelehnt");
        mock.release("openai", 1).await;
    };
    let (outcome, ()) = tokio::join!(run, driver);
    outcome.unwrap();
    let mut m = os_store::registry::Manager::open(&dir.0).unwrap();
    m.delete(&id, os_model::Clock::now(&os_model::SystemClock)).unwrap();
    assert!(!path.exists());
}

// spec: AI-020 [M4]
// Ersetzt den Platzhalter `decisions_lifecycle::heartbeat_of_an_ai_run` (Wanduhr statt simulierter Uhr, siehe Bericht):
// Während Request 1 über mehr als zwei Takte gehalten wird, schreibt der KI-Lauf weitere `heartbeat_at`, `state` bleibt
// `Completed`; nach dem Ende ist `owner_pid` leer und `heartbeat_at` der Endzeitpunkt.
#[tokio::test]
async fn heartbeat_of_an_ai_run_via_the_run_frame() {
    let mock = Mock::start().await;
    let dir = tmp("d05-heartbeat");
    let pages = site_pages("site");
    let path = crawl_file(&dir.0, &pages);
    let cfg = config(&[question_json("page-type")], 5_000_000);
    let env = env(&mock, &["openai"], NO_CONFIRMATION);
    mock.hold("openai", 1).await;
    let beat = |p: &std::path::Path| {
        os_store::CrawlFile::open(p, os_store::OpenMode::ReadOnly)
            .unwrap()
            .get_meta("heartbeat_at")
            .unwrap()
            .unwrap_or_default()
    };
    let run = run_decisions(&env, job(&path, &cfg, &pages, JobConfirmation::Confirmed));
    let driver = async {
        mock.until("gehalten", |_| mock.held(DECISIONS) >= 1).await;
        let mut seen = vec![beat(&path)];
        for _ in 0..3 {
            tokio::time::sleep(std::time::Duration::from_millis(2_100)).await;
            seen.push(beat(&path));
            assert_eq!(owner(&path).2, "Completed");
        }
        seen.dedup();
        assert!(seen.len() >= 3, "Heartbeats während des gehaltenen Requests: {seen:?}");
        mock.release("openai", 1).await;
    };
    let (outcome, ()) = tokio::join!(run, driver);
    outcome.unwrap();
    released(&path);
}
