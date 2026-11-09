//! B4-LAGFIX2 (UI-018, PERF-004): Live-Zähler und Tabellenfuß folgen dem Crawl im selben Tick.
//! Tabellenzeilen werden nur in einem Poll veröffentlicht, dessen Tick fällig ist; der Tick wartet begrenzt auf die
//! dadurch gestartete View-Pflege. Nach dem Anhängen eines Crawls kommt der erste Tick sofort (Mindestabstand bleibt).
#![allow(clippy::disallowed_types, clippy::disallowed_methods)]
#[path = "session_common.rs"]
mod common;
use common::*;
use os_app::{
    dto::UiEvent,
    events::{coalescer::Coalescer, CollectingSink},
    runtime::hooks::TickPolicy,
    session::feed,
};
use os_model::time::Clock;
use serde_json::json;
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

fn ticks(sink: &CollectingSink) -> Vec<os_app::dto::Tick> {
    sink.take()
        .into_iter()
        .filter_map(|e| match e {
            UiEvent::Tick { tick } => Some(*tick),
            _ => None,
        })
        .collect()
}

// spec: UI-018
// spec: PERF-004
// spec: UI-143
#[tokio::test]
async fn the_tick_preview_does_not_consume_and_an_expedited_tick_keeps_the_minimum_spacing() {
    let dir = tempfile::tempdir().unwrap();
    let (app, clock) = manual_app(dir.path());
    let policy = Coalescer::default();
    let sink = Arc::new(CollectingSink::new());
    app.subscribe(window(), sink.clone());
    // Ein neues Fenster startet seine Uhr: ohne Beschleunigung kein sofortiger Tick (bisheriges Verhalten).
    assert!(!policy.any_due(&app));
    assert!(policy.due_windows(&app).is_empty());
    clock.advance(Duration::from_millis(499));
    assert!(!policy.any_due(&app));
    clock.advance(Duration::from_millis(1));
    // Die Vorschau verbraucht den fälligen Tick nicht.
    assert!(policy.any_due(&app));
    assert!(policy.any_due(&app));
    assert_eq!(policy.due_windows(&app), vec![window()]);
    assert!(!policy.any_due(&app));
    // Beschleunigt: frühestens 100 ms nach dem letzten Tick (PERF-004), danach wieder der Live-Takt.
    policy.expedite();
    clock.advance(Duration::from_millis(99));
    assert!(!policy.any_due(&app));
    clock.advance(Duration::from_millis(1));
    assert!(policy.any_due(&app));
    assert_eq!(policy.due_windows(&app), vec![window()]);
    clock.advance(Duration::from_millis(100));
    assert!(!policy.any_due(&app), "one expedited tick only");
    clock.advance(Duration::from_millis(400));
    assert!(policy.any_due(&app));
    // Ein neu abonniertes, sichtbares Fenster bekommt den beschleunigten Tick sofort, ein verdecktes nicht (UI-143).
    let fresh = Coalescer::default();
    let hidden = os_app::WindowId("hidden".into());
    app.set_visibility(hidden.clone(), false);
    app.subscribe(hidden.clone(), Arc::new(CollectingSink::new()));
    fresh.expedite();
    assert_eq!(fresh.due_windows(&app), vec![window()]);
    clock.advance(Duration::from_millis(999));
    assert!(!fresh.due_windows(&app).contains(&hidden));
    clock.advance(Duration::from_millis(1));
    assert!(fresh.due_windows(&app).contains(&hidden));
    // Ohne abonniertes Fenster gibt es nichts abzustimmen: die Zeilen folgen weiter dem Live-Intervall.
    let other = tempfile::tempdir().unwrap();
    let (alone, _) = manual_app(other.path());
    assert!(Coalescer::default().any_due(&alone));
}

// spec: UI-018
// spec: ONPAGE-193
// Driven only by the running session loop: the test moves the manual clock in poll steps and waits for the loop.
#[tokio::test]
async fn table_rows_are_published_only_in_polls_whose_tick_is_due() {
    let dir = tempfile::tempdir().unwrap();
    let (app, clock) = manual_app(dir.path());
    let f = create(&dir.path().join("aligned.ospider"), cfg());
    f.set_meta("state", "Paused").unwrap();
    insert(&f, 1, 1);
    let published = Arc::new(Mutex::new(Vec::new()));
    let copy = published.clone();
    let probe = app.config().clock.clone();
    feed::subscribe(
        &app,
        Arc::new(move |_, _| copy.lock().unwrap().push(probe.monotonic())),
    );
    open(&app, f.path()).await;
    let loop_step = Duration::from_millis(150);
    tokio::time::sleep(loop_step).await;
    // Das Fenster kommt 200 ms später: Tabellen- und Tick-Uhr liefen ohne Abgleich versetzt (500/1000/… gegen
    // 700/1200/…); die Zeilen müssen dem Tick folgen.
    clock.advance(Duration::from_millis(200));
    let sink = Arc::new(CollectingSink::new());
    app.subscribe(window(), sink.clone());
    tokio::time::sleep(loop_step).await;
    let mut tick_times = Vec::new();
    for n in 2..=41 {
        clock.advance(Duration::from_millis(100));
        insert(&f, n, n as u64);
        tokio::time::sleep(loop_step).await;
        if !ticks(&sink).is_empty() {
            tick_times.push(clock.monotonic());
        }
    }
    let published = published.lock().unwrap().clone();
    assert!(published.len() >= 6, "rows published: {published:?}");
    // Die erste Veröffentlichung (Neuaufbau beim Öffnen) liegt vor dem Fenster.
    assert_eq!(published[0], Duration::ZERO);
    for at in &published[1..] {
        assert!(
            tick_times.contains(at),
            "rows at {at:?} without a tick in the same poll; ticks {tick_times:?}"
        );
    }
    // Höchstens eine Zeilenveröffentlichung je Live-Intervall (UI-018: ≤ 2 je Sekunde und Ansicht).
    for pair in published[1..].windows(2) {
        assert!(pair[1] - pair[0] >= Duration::from_millis(500));
    }
    assert!(tick_times.len() <= 9, "ticks {tick_times:?}");
}

// spec: UI-018
// The session loop sends the status of a poll together with the table total of the same poll: the footer does
// not follow the counter one tick later. Driven only by the running session loop (no test-side poll/flush).
#[tokio::test]
async fn the_session_loop_sends_the_table_total_with_the_status_of_the_same_poll() {
    let dir = tempfile::tempdir().unwrap();
    let (app, clock) = manual_app(dir.path());
    let f = create(&dir.path().join("settle.ospider"), cfg());
    f.set_meta("state", "Paused").unwrap();
    insert(&f, 1, 1);
    open(&app, f.path()).await;
    let sink = Arc::new(CollectingSink::new());
    app.subscribe(window(), sink.clone());
    let handle = call(
        &app,
        "view.open",
        json!({"spec":{"kind":"tab","tab":"Internal","filter":"All","search":null,"sort":[]},"columns":["Address"]}),
    )
    .await;
    let view_id = handle["viewId"].as_str().unwrap().to_string();
    // The window's tick clock starts now (first sight); every later advance makes one tick due.
    app.flush_tick();
    let mut seen = Vec::new();
    for n in 2..=6u64 {
        insert(&f, n as i64, n);
        clock.advance(Duration::from_millis(500));
        let tick = tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if let Some(t) = ticks(&sink)
                    .into_iter()
                    .rev()
                    .find(|t| t.crawl.as_ref().is_some_and(|c| c.crawled == n))
                {
                    return t;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap_or_else(|_| panic!("no tick with crawled={n} after one tick period"));
        let total = tick
            .views
            .iter()
            .find(|v| v.view_id == view_id)
            .and_then(|v| v.total);
        seen.push((n, total));
    }
    assert_eq!(
        seen,
        (2..=6u64).map(|n| (n, Some(n))).collect::<Vec<_>>(),
        "status and table total of the same poll travel in one tick"
    );
}
