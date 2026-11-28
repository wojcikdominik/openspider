//! M4-D05, Nacharbeit nach der Abnahme (28.11.2026): KI-Warteschlange während des Crawls (`os_ai::run::DecisionQueue`)
//! gegen den ai-mock – Pause bzw. SIGINT warten laufende Requests ab (Befund A2), „Ein offener Eintrag ist eine Zeile“
//! (08 AI-020, Nachtrag 28.11.2026), Kostenbestätigung während des Crawls nur, wenn etwas wartet (R1), Kontingentsperre
//! über alle Seitenblöcke eines KI-Laufs (AI-020, Nachtrag 26.11.2026). Die Seiten liefert der Test so, wie die Engine sie
//! übergibt.
#[path = "run_support.rs"]
mod rs;

use std::sync::Arc;
use std::time::Duration;

use os_ai::run::{DecisionQueue, EnvFactory, RunEnv};
use os_model::ai::hook::{AiQueue, AiQueueEnd, AiQueueStart, InputSource, PageAiFields};
use os_model::CrawlConfig;
use rs::*;

fn queue(env: RunEnv) -> DecisionQueue {
    let factory: EnvFactory = Arc::new(move |_, _| Ok(env.clone()));
    DecisionQueue::new(factory)
}

fn start(path: &std::path::Path, cfg: &CrawlConfig) -> AiQueueStart {
    AiQueueStart {
        crawl_file: path.to_path_buf(),
        config: cfg.clone(),
        resumed: false,
        owner_kind: "cli",
        existing: None,
    }
}

fn parsed(mut pages: Vec<PageAiFields>) -> Vec<PageAiFields> {
    for p in &mut pages {
        p.source = InputSource::Parsed;
    }
    pages
}

/// Zahl der Zeilen je URL für die Frage (jede URL höchstens eine).
fn rows_per_url(path: &std::path::Path, id: &str) -> Vec<(i64, i64)> {
    let f = os_store::CrawlFile::open(path, os_store::OpenMode::ReadOnly).unwrap();
    let mut stmt = f
        .conn()
        .prepare(
            "SELECT url_id, COUNT(*) FROM ai_answers WHERE question_id = ?1 GROUP BY url_id ORDER BY url_id",
        )
        .unwrap();
    stmt.query_map([id], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}

async fn until(what: &str, mut f: impl FnMut() -> bool) {
    for _ in 0..2_000 {
        if f() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("Zeitüberschreitung: {what}");
}

// spec: AI-020 [M4]
// spec: CRAWL-126 [M4]
// spec: OUT-206
// Befund A2 und Nachtrag 28.11.2026 („Ein offener Eintrag ist eine Zeile“): Pause (bzw. SIGINT der CLI), während Request 3
// gehalten wird → die Zukunft von `end(Paused)` wird erst fertig, wenn Request 3 übernommen ist (die CLI beendet sich
// danach; vorher ging der Request verloren und wurde nach `--resume` doppelt gesendet); danach kein weiterer Request.
// Jede Seite der Grundmenge G hat genau eine Zeile: 3 × `Success`, 19 × `Pending`, 3 × `Skipped (scope)`.
#[tokio::test]
async fn pause_waits_for_the_running_request_and_every_page_has_one_row() {
    let mock = Mock::start().await;
    let dir = tmp("d05-n-pause");
    let pages = parsed(site_pages("site"));
    let path = crawl_file(&dir.0, &pages);
    let cfg = config(&[question_json("page-type")], 5_000_000);
    let session = queue(env(&mock, &["openai"], NO_CONFIRMATION))
        .begin(start(&path, &cfg))
        .unwrap();
    mock.hold("openai", 3).await;
    for p in &pages {
        session.page(p.clone());
    }
    for i in 1..=2 {
        mock.until("Request gehalten", |_| mock.posts(DECISIONS).len() >= i)
            .await;
        mock.release("openai", 1).await;
    }
    mock.until("Request 3 gehalten", |_| mock.posts(DECISIONS).len() >= 3)
        .await;
    let mut paused = session.end(AiQueueEnd::Paused);
    assert!(
        tokio::time::timeout(Duration::from_millis(400), &mut paused)
            .await
            .is_err(),
        "Pause wartet den laufenden Request ab"
    );
    mock.release("openai", 1).await;
    tokio::time::timeout(Duration::from_secs(10), paused)
        .await
        .expect("nach der Übernahme fertig");
    // Kein Nachlauf: der Sitzungsthread ist beendet, das Ergebnis steht schon in der Datei.
    assert_eq!(
        decisions_posts(&mock),
        3,
        "nach der Pause kein weiterer Request"
    );
    assert_eq!(logged(&path), 3, "Request 3 übernommen");
    let c = counts(&path, "page-type");
    assert_eq!(c.get("Success"), Some(&3), "{c:?}");
    assert_eq!(c.get("Pending"), Some(&19), "{c:?}");
    assert_eq!(c.get("Skipped (scope)"), Some(&3), "{c:?}");
    let rows = rows_per_url(&path, "page-type");
    assert_eq!(rows.len(), 25, "jede URL der Grundmenge G hat eine Zeile");
    assert!(rows.iter().all(|(_, n)| *n == 1), "{rows:?}");
}

// spec: AI-020 [M4]
// Nachtrag 28.11.2026 („Ein offener Eintrag ist eine Zeile“) auch beim Stoppen und beim Abbruch (Schließen der
// Crawl-Datei, Beenden): Seiten, die nicht mehr gesendet wurden, erhalten `Pending`; der abgebrochene Request bleibt
// `Pending` ohne Protokollzeile.
#[tokio::test]
async fn stop_and_abort_leave_one_row_per_page() {
    for end in [AiQueueEnd::Stopped, AiQueueEnd::Abort] {
        let mock = Mock::start().await;
        let dir = tmp("d05-n-stop");
        let pages = parsed(site_pages("site"));
        let path = crawl_file(&dir.0, &pages);
        let cfg = config(&[question_json("page-type")], 5_000_000);
        let session = queue(env(&mock, &["openai"], NO_CONFIRMATION))
            .begin(start(&path, &cfg))
            .unwrap();
        mock.hold("openai", 1).await;
        for p in &pages {
            session.page(p.clone());
        }
        mock.until("Request 1 gehalten", |_| mock.held(DECISIONS) >= 1)
            .await;
        let done = session.end(end);
        if end == AiQueueEnd::Stopped {
            mock.release("openai", 1).await;
        }
        tokio::time::timeout(Duration::from_secs(10), done)
            .await
            .expect("Ende umgesetzt");
        let c = counts(&path, "page-type");
        let expected_success = u64::from(end == AiQueueEnd::Stopped);
        assert_eq!(
            c.get("Success").copied().unwrap_or(0),
            expected_success,
            "{end:?}: {c:?}"
        );
        assert_eq!(
            c.get("Pending").copied(),
            Some(22 - expected_success),
            "{end:?}: {c:?}"
        );
        assert_eq!(c.get("Skipped (scope)"), Some(&3), "{end:?}");
        assert_eq!(logged(&path) as u64, expected_success, "{end:?}");
        assert_eq!(decisions_posts(&mock), 1, "{end:?}: kein weiterer Request");
        let rows = rows_per_url(&path, "page-type");
        assert_eq!(rows.len(), 25, "{end:?}");
        assert!(rows.iter().all(|(_, n)| *n == 1), "{end:?}: {rows:?}");
    }
}

// spec: AI-020 [M4]
// spec: AI-012 [M4]
// R1, Nachtrag 28.11.2026 („Start während des eigenen laufenden Crawls“, einzige Ausnahme): `confirm_running` ist nur die
// Antwort auf einen Kostenvoranschlag, auf den die Warteschlange gerade wartet. Ohne Sitzung, mit ausgeschalteter
// automatischer Ausführung und mit einer Sitzung, die nichts zurückhält, liefert es `false` und ändert nichts.
#[tokio::test]
async fn confirmation_during_the_crawl_only_while_entries_wait() {
    let mock = Mock::start().await;
    let dir = tmp("d05-n-confirm");
    let pages = parsed(site_pages("site"));
    let path = crawl_file(&dir.0, &pages);
    assert!(!os_ai::run::confirm_running(&path), "keine Sitzung");

    // Automatische Ausführung aus: die Sitzung speichert nur Eingaben, nichts wartet.
    let mut off = config(&[question_json("page-type")], 5_000_000);
    off.ai.auto_run = os_model::config::DefaultOn(false);
    let session = queue(env(&mock, &["openai"], os_model::budget::MicroUsd(100)))
        .begin(start(&path, &off))
        .unwrap();
    for p in &pages {
        session.page(p.clone());
    }
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(
        !os_ai::run::confirm_running(&path),
        "automatische Ausführung aus"
    );
    session.end(AiQueueEnd::Drain).await;

    // Unter der Schwelle: es wartet nichts auf eine Bestätigung.
    let cfg = config(&[question_json("page-type")], 5_000_000);
    let session = queue(env(&mock, &["openai"], NO_CONFIRMATION))
        .begin(start(&path, &cfg))
        .unwrap();
    mock.hold("openai", 1).await;
    for p in &pages {
        session.page(p.clone());
    }
    mock.until("Request gehalten", |_| mock.held(DECISIONS) >= 1)
        .await;
    assert!(!os_ai::run::confirm_running(&path), "nichts wartet");
    mock.release("openai", 100).await;
    session.end(AiQueueEnd::Drain).await;
    assert_eq!(counts(&path, "page-type").get("Success"), Some(&22));

    // Über der Schwelle: die Einträge warten, die Bestätigung wird angenommen.
    let dir = tmp("d05-n-confirm-b");
    let path = crawl_file(&dir.0, &pages);
    let before = decisions_posts(&mock);
    let session = queue(env(&mock, &["openai"], os_model::budget::MicroUsd(100)))
        .begin(start(&path, &cfg))
        .unwrap();
    for p in &pages {
        session.page(p.clone());
    }
    until("Einträge warten auf die Bestätigung", || {
        counts(&path, "page-type").get("Pending (confirmation)") == Some(&22)
    })
    .await;
    assert_eq!(decisions_posts(&mock), before);
    assert!(os_ai::run::confirm_running(&path), "wartende Einträge");
    session.end(AiQueueEnd::Drain).await;
    assert_eq!(counts(&path, "page-type").get("Success"), Some(&22));
    let f = os_store::CrawlFile::open(&path, os_store::OpenMode::ReadOnly).unwrap();
    let runs: i64 = f
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM ai_runs WHERE confirmed_at IS NOT NULL",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(runs, 1, "confirmed_at in der Laufzeile dieses Crawls");
}

// spec: AI-020 [M4]
// Nachtrag 26.11.2026 (Auslegung D01 15): Die Kontingentsperre gilt je KI-Lauf – während des Crawls über alle Seitenblöcke
// derselben Sitzung. Seite 1 allein → Kontingent-Antwort; die danach gecrawlten Seiten erhalten `Quota exceeded` ohne
// Request (vorher versuchte jeder neue Block den Anbieter erneut).
#[tokio::test]
async fn quota_lock_holds_across_page_blocks_of_the_crawl() {
    let mock = Mock::start().await;
    let dir = tmp("d05-n-quota");
    let pages = parsed(site_pages("site"));
    let path = crawl_file(&dir.0, &pages);
    let cfg = config(&[question_json("page-type")], 5_000_000);
    mock.scenario("openai", "insufficient-quota", 1).await;
    let session = queue(env(&mock, &["openai"], NO_CONFIRMATION))
        .begin(start(&path, &cfg))
        .unwrap();
    let first = pages
        .iter()
        .position(|p| p.is_html_200() && p.indexable)
        .expect("Seite im Geltungsbereich");
    session.page(pages[first].clone());
    until("Kontingent-Antwort übernommen", || {
        counts(&path, "page-type").get("Quota exceeded") == Some(&1)
    })
    .await;
    for (i, p) in pages.iter().enumerate() {
        if i != first {
            session.page(p.clone());
        }
    }
    session.end(AiQueueEnd::Drain).await;
    let c = counts(&path, "page-type");
    assert_eq!(c.get("Quota exceeded"), Some(&22), "{c:?}");
    assert_eq!(decisions_posts(&mock), 1, "ein Request je KI-Lauf");
    assert_eq!(logged(&path), 1, "keine Protokollzeile ohne Request");
}

// spec: AI-017 [M4]
// spec: AI-020 [M4]
// R3 der Abnahme M4-D05 (Szenario `401` für alle Requests, Lauf endet nicht in 60 s): spec-gemäß. 401 wird nach INTEG-005
// nicht wiederholt (je Eintrag genau 1 Request, `Error: 401 …`), zählt aber als endgültiger Fehler für den Schutzschalter
// (AI-017: nach 10 aufeinanderfolgenden endgültigen Fehlern sendet der Anbieter 5 Minuten lang nichts; 401 gehört nicht zu
// den Ausnahmen). Bei 22 Einträgen und einer gleichzeitigen Anfrage: 10 Requests, dann 5 Minuten Pause, 10 Requests, wieder
// 5 Minuten, 2 Requests – der Lauf endet nach gut 10 Minuten. Geprüft wird der erste Abschnitt; danach Abbruch.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn status_401_trips_the_breaker_after_ten_requests_without_retries() {
    let mock = Mock::start().await;
    let dir = tmp("d05-n-401");
    let pages = site_pages("site");
    let path = crawl_file(&dir.0, &pages);
    let cfg = config(&[question_json("page-type")], 5_000_000);
    mock.scenario("openai", "401", 1_000).await;
    let env = env(&mock, &["openai"], NO_CONFIRMATION);
    let job = job(&path, &cfg, &pages, os_ai::run::JobConfirmation::Confirmed);
    let run = tokio::task::spawn_blocking(move || os_ai::run::run_decisions_blocking(env, job));
    mock.until("10 Requests", |_| decisions_posts(&mock) >= 10)
        .await;
    tokio::time::sleep(Duration::from_secs(3)).await;
    assert_eq!(
        decisions_posts(&mock),
        10,
        "Schutzschalter: 5 Minuten nichts"
    );
    let c = counts(&path, "page-type");
    let errors: u64 = c
        .iter()
        .filter(|(s, _)| s.starts_with("Error: 401"))
        .map(|(_, n)| *n)
        .sum();
    assert_eq!(
        errors, 10,
        "je Eintrag 1 Request, keine Wiederholung: {c:?}"
    );
    assert_eq!(c.get("Pending"), Some(&12), "{c:?}");
    assert_eq!(logged(&path), 10);
    os_ai::run::controls().abort(&path);
    let outcome = tokio::time::timeout(Duration::from_secs(5), run)
        .await
        .expect("Abbruch beendet das Warten auf den Schutzschalter")
        .unwrap()
        .unwrap();
    assert_eq!(outcome.end, os_ai::decisions::exec::RunEnd::Aborted);
}
