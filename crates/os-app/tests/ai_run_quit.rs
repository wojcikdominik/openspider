//! M4-D05: Geordnetes Beenden der App bei gehaltenem KI-Request (08 AI-020 [M4], Nachtrag K4-13; CRAWL-126). Eigene
//! Testdatei, weil das Beenden alle KI-Läufe des Prozesses abbricht.
#![allow(clippy::disallowed_types, clippy::disallowed_methods)]
#[path = "ai_run_support.rs"]
mod support;

use std::time::{Duration, Instant};

use os_app::dto::OpState;
use support::*;

// spec: AI-020 [M4]
// spec: CRAWL-126 [M4]
// Geordnetes Beenden der App bei gehaltenem Request: Beenden im Zeitbudget, danach wie beim Absturz.
#[tokio::test]
async fn quitting_the_app_aborts_a_held_request() {
    let (_dir, mock, app, path) = setup(NO_CONFIRMATION).await;
    let op = held_run(&app, &mock).await;
    let started = Instant::now();
    os_app::quit::confirm(&app, true).await.unwrap();
    assert!(
        started.elapsed() < Duration::from_secs(3),
        "{:?}",
        started.elapsed()
    );
    let status = wait_op(&app, &op).await;
    assert_ne!(status.state, OpState::Running);
    assert!(os_ai::run::controls().running(&path) == 0);
    assert_like_a_crash(&path);
}
