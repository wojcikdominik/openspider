//! Unabhängige Abnahme M4-D05 (os-app), Kriterium (d): geordnetes Beenden der App und Schließen der Crawl-Datei bei
//! gehaltenem KI-Request.
//!
//! 08 AI-020, Nachtrag K4-13, „Geordnetes Beenden der App bzw. Schließen der Crawl-Datei bei laufenden KI-Requests: Die
//! laufenden Requests werden abgebrochen, nicht abgewartet, damit das Beenden im Zeitbudget aus CRAWL-126 bleibt; ihre
//! Einträge werden wie beim Absturz behandelt (`Pending`, keine Protokollzeile, Reservierung verfällt, Neusenden genau
//! einmal).“ Kriterium: „App beenden, während der Mock Request n hält (ohne Freigabe) → die App beendet sich ohne auf die
//! Antwort zu warten; nach dem Öffnen gilt dasselbe wie beim Absturz …; dasselbe beim Schließen der Crawl-Datei.“
//! Zeitbudget des Beendens: 3 s (CRAWL-126). Eigenes Binary, weil das Beenden alle KI-Läufe des Prozesses abbricht.
#![allow(clippy::disallowed_types, clippy::disallowed_methods)]

mod acceptance_d05_support;

use std::path::Path;
use std::time::{Duration, Instant};

use acceptance_d05_support::*;

/// Adresse des gehaltenen Requests.
fn held_address(mock: &Mock) -> String {
    let held: Vec<_> = mock.decisions().into_iter().filter(|r| r.held).collect();
    assert_eq!(held.len(), 1, "genau ein gehaltener Request");
    request_url(&body(&held[0])).expect("URL im gehaltenen Request")
}

/// Zustand nach dem Abbruch: Eintrag `Pending`, keine Protokollzeile, keine offene Reservierung, Verbrauch = Summe der
/// Protokollzeilen.
fn assert_like_a_crash(path: &Path, address: &str, question: &str) {
    assert_eq!(
        status_of(path, address, question).as_deref(),
        Some("Pending"),
        "Eintrag des abgebrochenen Requests ist `Pending`"
    );
    assert_eq!(
        logged_rows_for(path, address),
        0,
        "keine Protokollzeile für {address}"
    );
    let (consumption, reserved, logged) = budget_on_open(path);
    assert_eq!(reserved, 0, "Reservierung verfallen");
    assert_eq!(consumption, logged, "Verbrauch = Summe der Protokollzeilen");
}

/// Öffnen sendet nichts; ein neuer KI-Lauf sendet den Eintrag genau einmal, danach genau eine Protokollzeile.
async fn resend_once(h: &Home, mock: &Mock, path: &Path, address: &str, question: &str) {
    let before = mock.decisions().len();
    open(&h.app, path).await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert_eq!(mock.decisions().len(), before, "beim Öffnen 0 Requests");
    run_ai(&h.app, true).await.expect("neuer KI-Lauf");
    let sent: Vec<String> = mock.decision_urls();
    let n = sent.iter().filter(|u| u.as_str() == address).count();
    assert_eq!(
        n, 2,
        "{address}: einmal gehalten und abgebrochen, einmal neu gesendet ({sent:?})"
    );
    assert_eq!(
        logged_rows_for(path, address),
        1,
        "genau eine Protokollzeile"
    );
    assert_eq!(
        status_of(path, address, question).as_deref(),
        Some("Success")
    );
}

async fn prepared(mock: &Mock, question_id: &str) -> (Home, std::path::PathBuf) {
    let site = ai_site().await;
    let h = home(mock, Opts::default());
    let mut cfg = profile(&site, &[question(question_id, &["url", "title"])]);
    cfg.ai.auto_run.0 = false;
    h.profile(&cfg);
    crawl_to_end(&h.app, START).await;
    let path = crawl_path(&h.app);
    (h, path)
}

// spec: AI-020 [M4]
// spec: CRAWL-126 [M4]
// acceptance: independent
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn d_closing_the_crawl_file_aborts_the_held_request() {
    let _s = serial().await;
    let mock = Mock::start().await;
    let (h, path) = prepared(&mock, "d05-close").await;
    mock.hold(1).await;
    let _op = start_ai(&h.app, true).await;
    mock.wait_held(1).await;
    let address = held_address(&mock);
    let t0 = Instant::now();
    close_during_run(&h.app, &mock)
        .await
        .expect("Schließen der Crawl-Datei während des KI-Laufs (bricht den Request ab)");
    let took = t0.elapsed();
    assert!(
        took <= Duration::from_secs(3),
        "Schließen ohne Warten auf die Antwort ({took:?})"
    );
    wait_until("Mock vermerkt den Abbruch", Duration::from_secs(5), || {
        mock.decisions().iter().any(|r| r.held && r.aborted)
    })
    .await;
    wait_until("Besitz freigegeben", Duration::from_secs(5), || {
        meta(&path, "owner_pid").unwrap_or_default().is_empty()
    })
    .await;
    assert_like_a_crash(&path, &address, "d05-close");
    resend_once(&h, &mock, &path, &address, "d05-close").await;
}

// spec: AI-020 [M4]
// spec: CRAWL-126 [M4]
// acceptance: independent
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn d_quitting_the_app_aborts_the_held_request_within_the_budget() {
    let _s = serial().await;
    let mock = Mock::start().await;
    let (h, path) = prepared(&mock, "d05-quit").await;
    mock.hold(1).await;
    let _op = start_ai(&h.app, true).await;
    mock.wait_held(1).await;
    let address = held_address(&mock);
    let t0 = Instant::now();
    tokio::time::timeout(Duration::from_secs(10), os_app::quit::confirm(&h.app, true))
        .await
        .expect("Beenden wartet nicht auf die Antwort")
        .expect("Beenden gelingt");
    let took = t0.elapsed();
    assert!(
        took <= Duration::from_secs(3),
        "Beenden im Zeitbudget von 3 s ({took:?})"
    );
    wait_until("Mock vermerkt den Abbruch", Duration::from_secs(5), || {
        mock.decisions().iter().any(|r| r.held && r.aborted)
    })
    .await;
    let h = h.restart();
    assert_like_a_crash(&path, &address, "d05-quit");
    resend_once(&h, &mock, &path, &address, "d05-quit").await;
}
