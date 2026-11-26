//! Unabhängige Abnahme M4-D01 – KI-Warteschlange und Lebenszyklus (AI-020, Nachtrag 26.11.2026, Klärung K4-13; gilt
//! für beide Wege nach AI-034 Nachtrag 26.11.2026): Pause, Fortsetzen ohne neue Kostenbestätigung, „Gestoppt
//! (endgültig)“, geordnetes Beenden, Absturz, Atomarität der Antwortverarbeitung, fertige Einträge beim Fortsetzen.
//! Abläufe sind über das Mock-Szenario `hold` an einen bestimmten Request gebunden (keine Wanduhr; Zeitgrenzen dienen
//! nur als Abbruchschutz). Erwartungen nur aus `docs/spec/08-ai.md`.
// acceptance: independent
mod acceptance_d01_support;

use std::cell::Cell;
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::time::Duration;

use acceptance_d01_support as s;
use os_ai::decisions::exec::{
    ActiveQuestion, Confirmation, RunControl, RunEnd, RunItem, RunOptions, Selection,
};
use os_model::budget::MicroUsd;
use os_store::StoreError;

const GUARD: Duration = Duration::from_secs(90);

fn site() -> Vec<RunItem> {
    s::site(|i| format!("Content of page {i}."))
}

fn questions() -> Vec<ActiveQuestion> {
    vec![s::active(s::page_type(&["title", "page_text"]))]
}

fn pages(n: i64) -> Vec<RunItem> {
    (1..=n)
        .map(|i| {
            s::page(
                i,
                &format!("Lifecycle page {i}"),
                &format!("Content of page {i}."),
            )
        })
        .collect()
}

/// Gibt die ersten `answered` Requests einzeln frei und wartet, bis Request `answered + 1` gehalten wird.
async fn answer_then_hold(bench: &s::Bench, answered: usize) {
    for n in 1..=answered {
        bench.mock.wait_held(s::DECISIONS, n).await;
        assert_eq!(bench.mock.release("openai", Some(1)).await, 1);
    }
    bench.mock.wait_held(s::DECISIONS, answered + 1).await;
}

/// Jede URL erscheint in genau einem gesendeten Request (keine doppelte Anfrage).
fn assert_each_url_once(bench: &s::Bench, ids: impl IntoIterator<Item = i64>) {
    let bodies = bench.mock.bodies(s::DECISIONS);
    for id in ids {
        let marker = format!("Content of page {id}.");
        let hits = bodies
            .iter()
            .filter(|b| s::decision_input_text(b).contains(&marker))
            .count();
        assert_eq!(hits, 1, "URL {id} {hits}-mal angefragt");
    }
}

/// Höchstens eine Protokollzeile mit Kosten je Eintrag.
fn assert_one_row_per_url(bench: &s::Bench) {
    let rows = bench.sent_rows();
    let urls: Vec<i64> = rows.iter().filter_map(|r| r.record.url_id).collect();
    let unique: BTreeSet<i64> = urls.iter().copied().collect();
    assert_eq!(urls.len(), unique.len(), "doppelte Verbuchung: {urls:?}");
}

// spec: AI-020 [M4]
// acceptance: independent
#[tokio::test]
async fn ai020_pause_during_request_6_then_resume_after_restart() {
    let mut bench = s::Bench::new(&["openai"], 1).await;
    let items = site();
    let qs = questions();
    bench.mock.hold("openai", 6).await;
    let control = RunControl::new();
    let ctl = control.clone();
    let driver = async {
        answer_then_hold(&bench, 5).await;
        ctl.pause();
        // Der laufende Request 6 wird zu Ende verarbeitet.
        assert_eq!(bench.mock.release("openai", Some(1)).await, 1);
    };
    let (report, ()) = tokio::time::timeout(GUARD, async {
        tokio::join!(
            bench.run_ctl(&qs, &items, s::options(), control, None),
            driver
        )
    })
    .await
    .expect("Lauf endet nach der Pause");
    assert_eq!(report.end, RunEnd::Paused);
    assert_eq!(
        bench.mock.posts(s::DECISIONS).len(),
        6,
        "nach der Pause kein neuer Request"
    );
    assert_eq!(bench.count("page-type", "Success"), 6);
    assert_eq!(bench.count("page-type", "Pending"), 16);
    assert_eq!(bench.count("page-type", "Skipped (scope)"), 3);
    assert_eq!(bench.sent_rows().len(), 6);
    assert_eq!(
        bench.spent(),
        bench.logged_cost(),
        "Budgetverbrauch = Summe der 6 Zeilen"
    );
    assert_eq!(
        bench.reserved(),
        0,
        "offene Einträge belegen keine Reservierung"
    );

    // Fortsetzen nach App-Neustart: genau 16 weitere Requests, keine erneute Kostenbestätigung.
    bench.reopen_file();
    assert_eq!(
        bench.spent(),
        bench.logged_cost(),
        "bisheriger Verbrauch gilt weiter"
    );
    let resume = RunOptions {
        selection: Selection::Open,
        confirmation: Confirmation::Granted,
        ..RunOptions::default()
    };
    let report = tokio::time::timeout(GUARD, bench.run(&qs, &items, resume))
        .await
        .expect("Fortsetzen endet");
    assert_eq!(report.end, RunEnd::Completed);
    assert!(
        report.estimate.is_none(),
        "beim Fortsetzen kein Kostenvoranschlag"
    );
    assert_eq!(bench.mock.posts(s::DECISIONS).len(), 22);
    assert_eq!(bench.count("page-type", "Success"), 22);
    assert_eq!(bench.count("page-type", "Pending (confirmation)"), 0);
    assert_eq!(bench.sent_rows().len(), 22);
    assert_each_url_once(&bench, 1..=22);
    assert_one_row_per_url(&bench);
}

// spec: AI-020 [M4]
// acceptance: independent
#[tokio::test]
async fn ai020_stop_final_no_further_request_then_new_run_with_estimate() {
    let bench = s::Bench::new(&["openai"], 1).await;
    let items = site();
    let qs = questions();
    bench.mock.hold("openai", 6).await;
    let control = RunControl::new();
    let ctl = control.clone();
    let driver = async {
        answer_then_hold(&bench, 5).await;
        ctl.stop();
        assert_eq!(bench.mock.release("openai", Some(1)).await, 1);
    };
    let (report, ()) = tokio::time::timeout(GUARD, async {
        tokio::join!(
            bench.run_ctl(&qs, &items, s::options(), control, None),
            driver
        )
    })
    .await
    .expect("Lauf endet nach dem Stopp");
    assert_eq!(report.end, RunEnd::Stopped);
    // Request 6 ist übernommen, danach kein weiterer Request.
    assert_eq!(bench.mock.posts(s::DECISIONS).len(), 6);
    assert_eq!(bench.count("page-type", "Success"), 6);
    assert_eq!(
        bench.count("page-type", "Pending"),
        16,
        "offene Einträge bleiben Pending"
    );
    assert_eq!(bench.sent_rows().len(), 6);
    assert_eq!(bench.reserved(), 0);

    // „KI-Analyse starten“: neuer Lauf mit Voranschlag (Schwelle 0,0001 USD) über die 16 offenen Einträge.
    let ask = RunOptions {
        confirmation: Confirmation::AskAbove(MicroUsd(100)),
        ..RunOptions::default()
    };
    let report = bench.run(&qs, &items, ask).await;
    assert_eq!(report.end, RunEnd::NeedsConfirmation);
    let estimate = report.estimate.expect("Voranschlag");
    let items_open: u64 = estimate
        .tasks
        .iter()
        .map(|t| t.estimate.items - t.estimate.cached)
        .sum();
    assert_eq!(items_open, 16, "Voranschlag über 16 Einträge");
    assert_eq!(
        bench.mock.posts(s::DECISIONS).len(),
        6,
        "vor „Starten“ kein Request"
    );
    assert_eq!(bench.count("page-type", "Pending (confirmation)"), 16);
    // „Starten“ → genau 16 Requests.
    let report = bench.run(&qs, &items, s::options()).await;
    assert_eq!(report.end, RunEnd::Completed);
    assert_eq!(bench.mock.posts(s::DECISIONS).len(), 22);
    assert_eq!(bench.count("page-type", "Success"), 22);
    assert_each_url_once(&bench, 1..=22);
}

// spec: AI-020 [M4]
// acceptance: independent
#[tokio::test]
async fn ai020_orderly_shutdown_does_not_wait_entry_pending_no_row() {
    let mut bench = s::Bench::new(&["openai"], 1).await;
    let items = pages(5);
    let qs = questions();
    bench.mock.hold("openai", 3).await;
    let control = RunControl::new();
    let ctl = control.clone();
    let driver = async {
        answer_then_hold(&bench, 2).await;
        // App beenden bzw. Crawl-Datei schließen, während Request 3 gehalten wird – ohne Freigabe.
        ctl.abort();
    };
    let (report, ()) = tokio::time::timeout(GUARD, async {
        tokio::join!(
            bench.run_ctl(&qs, &items, s::options(), control, None),
            driver
        )
    })
    .await
    .expect("Beenden wartet nicht auf die gehaltene Antwort");
    assert_eq!(report.end, RunEnd::Aborted);
    s::wait_for("gehaltener Request 3 als abgebrochen protokolliert", || {
        bench.mock.posts(s::DECISIONS).iter().any(|r| r.aborted)
    })
    .await;
    assert_eq!(
        bench.mock.release("openai", None).await,
        0,
        "nichts mehr gehalten"
    );
    bench.reopen_file();
    assert_eq!(bench.status(3, "page-type"), "Pending");
    assert!(
        bench.sent_rows().iter().all(|r| r.record.url_id != Some(3)),
        "Protokollzeile für den abgebrochenen Request"
    );
    assert_eq!(bench.sent_rows().len(), 2);
    assert_eq!(bench.reserved(), 0, "Reservierung verfallen");
    assert_eq!(bench.spent(), bench.logged_cost());
    // Neuer Lauf: Eintrag 3 genau einmal neu, je Eintrag höchstens eine Protokollzeile.
    let report = bench.run(&qs, &items, s::options()).await;
    assert_eq!(report.end, RunEnd::Completed);
    assert_eq!(bench.count("page-type", "Success"), 5);
    assert_eq!(bench.sent_rows().len(), 5);
    assert_one_row_per_url(&bench);
    let resent = bench
        .mock
        .bodies(s::DECISIONS)
        .iter()
        .filter(|b| s::decision_input_text(b).contains("Content of page 3."))
        .count();
    assert_eq!(
        resent, 2,
        "abgebrochener und genau ein neuer Request für Eintrag 3"
    );
}

// spec: AI-020 [M4]
// acceptance: independent
#[tokio::test]
async fn ai020_crash_while_request_held_then_resume_once() {
    let mut bench = s::Bench::new(&["openai"], 1).await;
    let items = pages(5);
    let qs = questions();
    bench.mock.hold("openai", 3).await;
    {
        let mut run = Box::pin(bench.run_ctl(&qs, &items, s::options(), RunControl::new(), None));
        let driver = answer_then_hold(&bench, 2);
        tokio::select! {
            _ = &mut run => panic!("Lauf endete vor dem simulierten Absturz"),
            _ = driver => {}
        }
        // Absturz: der Lauf wird ohne jede weitere Verarbeitung verworfen.
        drop(run);
    }
    s::wait_for("gehaltener Request als abgebrochen protokolliert", || {
        bench.mock.posts(s::DECISIONS).iter().any(|r| r.aborted)
    })
    .await;
    let posts_before_open = bench.mock.posts(s::DECISIONS).len();
    bench.reopen_file();
    assert_eq!(
        bench.mock.posts(s::DECISIONS).len(),
        posts_before_open,
        "beim Öffnen 0 Requests"
    );
    assert_eq!(bench.status(3, "page-type"), "Pending");
    assert!(bench.sent_rows().iter().all(|r| r.record.url_id != Some(3)));
    assert_eq!(
        bench.spent(),
        bench.logged_cost(),
        "keine Reservierung mehr im Verbrauch"
    );
    assert_eq!(bench.reserved(), 0);
    let report = bench.run(&qs, &items, s::options()).await;
    assert_eq!(report.end, RunEnd::Completed);
    assert_eq!(bench.count("page-type", "Success"), 5);
    let rows_for_3 = bench
        .sent_rows()
        .iter()
        .filter(|r| r.record.url_id == Some(3))
        .count();
    assert_eq!(
        rows_for_3, 1,
        "am Ende genau 1 Protokollzeile für Eintrag 3"
    );
    assert_one_row_per_url(&bench);
}

// spec: AI-020 [M4]
// acceptance: independent
#[tokio::test]
async fn ai020_answer_processing_is_atomic() {
    let bench = s::Bench::new(&["openai"], 1).await;
    let items = pages(3);
    let qs = questions();
    let calls = Cell::new(0u32);
    let hook = || {
        calls.set(calls.get() + 1);
        if calls.get() == 2 {
            Err(StoreError::NotFound(PathBuf::from(
                "d01t-injected-before-commit",
            )))
        } else {
            Ok(())
        }
    };
    let hook_ref: &dyn Fn() -> Result<(), StoreError> = &hook;
    let report = bench
        .run_ctl(&qs, &items, s::options(), RunControl::new(), Some(hook_ref))
        .await;
    assert!(
        !report.commit_errors.is_empty(),
        "Fehlerinjektion nicht gemeldet"
    );
    let failed: Vec<i64> = (1..=3)
        .filter(|id| bench.status(*id, "page-type") != "Success")
        .collect();
    assert_eq!(failed.len(), 1, "genau ein Eintrag ohne Übernahme");
    let id = failed[0];
    let row = bench.answer(id, "page-type");
    assert!(row.answer_json.is_none(), "Ergebnis trotz Fehlerinjektion");
    assert_eq!(row.status, "Pending");
    assert!(
        bench
            .sent_rows()
            .iter()
            .all(|r| r.record.url_id != Some(id)),
        "Protokollzeile trotz Fehlerinjektion"
    );
    assert_eq!(bench.sent_rows().len(), 2);
    assert_eq!(bench.cache_rows(), 2, "Cache-Eintrag trotz Fehlerinjektion");
    assert_eq!(
        bench.spent(),
        bench.logged_cost(),
        "verbuchte Kosten trotz Fehlerinjektion"
    );
    assert_eq!(bench.reserved(), 0);
    // Ohne Injektion alle vier.
    bench.run(&qs, &items, s::options()).await;
    assert_eq!(bench.status(id, "page-type"), "Success");
    assert!(bench.answer(id, "page-type").answer_json.is_some());
    assert_eq!(bench.sent_rows().len(), 3);
    assert_eq!(bench.cache_rows(), 3);
    assert_eq!(bench.spent(), bench.logged_cost());
}

// spec: AI-020 [M4]
// acceptance: independent
#[tokio::test]
async fn ai020_finished_entries_untouched_failed_retry_sends_exactly_these() {
    let bench = s::Bench::new(&["openai"], 1).await;
    let items = pages(4);
    let qs = questions();
    // Die ersten beiden Requests scheitern endgültig (je 1 + 5 Versuche).
    bench.mock.scenario("openai", "500-always", 12).await;
    bench.run(&qs, &items, s::options()).await;
    let errors: Vec<i64> = (1..=4)
        .filter(|id| bench.status(*id, "page-type").starts_with("Error: 500"))
        .collect();
    assert_eq!(errors.len(), 2, "{:?}", bench.status_counts("page-type"));
    let posts = bench.mock.posts(s::DECISIONS).len();
    assert_eq!(posts, 14);
    // Fortsetzen bzw. erneuter Lauf: keine fertigen Einträge (auch nicht `Error: …`).
    bench.run(&qs, &items, s::options()).await;
    assert_eq!(bench.mock.posts(s::DECISIONS).len(), posts);
    // „Fehlgeschlagene erneut anfragen“ sendet genau diese 2.
    let failed = RunOptions {
        selection: Selection::Failed,
        ..RunOptions::default()
    };
    bench.run(&qs, &items, failed).await;
    assert_eq!(bench.mock.posts(s::DECISIONS).len(), posts + 2);
    for id in errors {
        assert_eq!(bench.status(id, "page-type"), "Success");
    }
}

// spec: AI-020 [M4]
// acceptance: independent
#[tokio::test]
async fn ai020_pause_applies_to_fallback_path_too() {
    let bench = s::Bench::new(&["anthropic"], 1).await;
    let items: Vec<RunItem> = (1..=4)
        .map(|i| s::page(i, &format!("Fallback {i} MOCK-ID:d01t-fb-choice"), "Text."))
        .collect();
    let q = s::active(s::choice(
        "d01t-fb-q",
        "Fallback Q",
        &["product", "category", "other", "article"],
        &["title"],
    ));
    bench.mock.hold("anthropic", 2).await;
    let control = RunControl::new();
    let ctl = control.clone();
    let driver = async {
        bench.mock.wait_held(s::MESSAGES, 1).await;
        assert_eq!(bench.mock.release("anthropic", Some(1)).await, 1);
        bench.mock.wait_held(s::MESSAGES, 2).await;
        ctl.pause();
        assert_eq!(bench.mock.release("anthropic", Some(1)).await, 1);
    };
    let qs = [q];
    let (report, ()) = tokio::time::timeout(GUARD, async {
        tokio::join!(
            bench.run_ctl(&qs, &items, s::options(), control, None),
            driver
        )
    })
    .await
    .expect("Lauf endet nach der Pause");
    assert_eq!(report.end, RunEnd::Paused);
    assert_eq!(bench.mock.posts(s::MESSAGES).len(), 2);
    assert_eq!(bench.count("d01t-fb-q", "Success"), 2);
    assert_eq!(bench.count("d01t-fb-q", "Pending"), 2);
    assert_eq!(bench.reserved(), 0);
}
