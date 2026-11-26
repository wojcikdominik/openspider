//! Unabhängige Abnahme M4-K11 (os-cli): `--load-crawl <ID> --resume` bei einem aktiven Besitzer außerhalb von
//! `Running` und `--list-crawls` für Dateien aus 1.0.
//!
//! Erwartetes Verhalten ausschließlich aus 08 AI-020, Nachtrag 26.11.2026, Unterpunkt „Aktiver Besitzer außerhalb
//! von `Running`“ (Wirkung b: „Engine und CLI mit dem Hinweis „läuft noch (Prozess <pid>)““; Kriterium 2:
//! „Fortsetzen (… CLI `--resume`) … mit „läuft noch“ abgewiesen“) und 01 CRAWL-125, Nachtrag K4-22 („die CLI …
//! endet mit Exit-Code 1“; Listen zeigen den Zustand nach derselben Regel). Die CLI liest die Systemuhr; der
//! simulierte Heartbeat liegt deshalb 60 s in der Zukunft (nach CRAWL-125 aktuell) bzw. ist älter als 10 s.

mod common;

#[path = "../../os-store/tests/acceptance_k11_support/mod.rs"]
mod k11;

use common::Env;
use k11::*;
use os_model::{Clock, CrawlState, SystemClock};
use serde_json::Value;

fn future() -> os_model::Timestamp {
    ago(SystemClock.now(), -60_000)
}

fn resume(env: &Env, id: &str) -> common::Run {
    env.run(&["--load-crawl", id, "--resume"])
}

// spec: AI-020 [M4]
// spec: CRAWL-125
// acceptance: independent
#[test]
fn resuming_a_paused_crawl_of_a_foreign_active_owner_ends_with_exit_code_1_and_names_the_process() {
    let env = Env::new();
    let (id, path) = new_crawl(&env.crawls_dir(), 10, CrawlState::Paused);
    set_owner(&path, "mcp-stdio", FOREIGN_PID, Some(future()));
    let before = meta_dump(&path);
    let run = resume(&env, &id.to_string());
    assert_eq!(run.code, 1, "{}", run.dump());
    assert!(
        run.stderr
            .contains(&format!("läuft noch (Prozess {FOREIGN_PID})")),
        "{}",
        run.dump()
    );
    assert_eq!(meta_dump(&path), before, "Datei unverändert");
}

// spec: AI-020 [M4]
// spec: CRAWL-125
// acceptance: independent
#[test]
fn resuming_a_limit_ended_crawl_of_a_foreign_active_owner_ends_with_exit_code_1_and_names_the_process(
) {
    let env = Env::new();
    let (id, path) = new_crawl(&env.crawls_dir(), 20, CrawlState::Completed);
    set(&path, "end_reason", "limit_reached");
    {
        let f = os_store::CrawlFile::open(&path, os_store::OpenMode::ReadWrite).unwrap();
        f.conn()
            .execute(
                "INSERT INTO queue(url_id, seq, depth, priority) VALUES (1, 1, 0, 0)",
                [],
            )
            .unwrap();
    }
    set_owner(&path, "cli", FOREIGN_PID, Some(future()));
    let before = meta_dump(&path);
    let run = resume(&env, &id.to_string());
    assert_eq!(run.code, 1, "{}", run.dump());
    assert!(
        run.stderr
            .contains(&format!("läuft noch (Prozess {FOREIGN_PID})")),
        "{}",
        run.dump()
    );
    assert_eq!(meta_dump(&path), before);
}

// spec: AI-020 [M4]
// spec: CRAWL-125
// acceptance: independent
#[test]
fn resuming_a_completed_crawl_of_a_foreign_active_owner_reports_that_it_is_still_running() {
    // Kriterium 2 wörtlich: `Completed`, fremder `owner_pid`, aktueller Heartbeat → CLI `--resume` mit „läuft noch“
    // abgewiesen (Nacharbeit M4-K11, `os-cli` `validate_resume`).
    let env = Env::new();
    let (id, path) = new_crawl(&env.crawls_dir(), 30, CrawlState::Completed);
    set_owner(&path, "mcp-stdio", FOREIGN_PID, Some(future()));
    let before = meta_dump(&path);
    let run = resume(&env, &id.to_string());
    assert_eq!(run.code, 1, "{}", run.dump());
    assert!(
        run.stderr
            .contains(&format!("läuft noch (Prozess {FOREIGN_PID})")),
        "{}",
        run.dump()
    );
    assert_eq!(meta_dump(&path), before);
}

fn listed(env: &Env) -> Vec<Value> {
    let run = env.run(&["--list-crawls", "--json"]);
    assert_eq!(run.code, 0, "{}", run.dump());
    serde_json::from_str::<Value>(&run.stdout)
        .unwrap_or_else(|e| panic!("{e}: {}", run.dump()))
        .as_array()
        .unwrap()
        .clone()
}

fn status_of(items: &[Value], id: &str) -> String {
    items
        .iter()
        .find(|i| i["id"] == id)
        .unwrap_or_else(|| panic!("{id} fehlt"))["status"]
        .as_str()
        .unwrap()
        .to_string()
}

// spec: AI-020 [M4]
// spec: CRAWL-125
// acceptance: independent
#[test]
fn the_list_shows_the_stored_state_for_an_active_owner_and_crawl_125_for_files_from_1_0() {
    // „Der abgeleitete Zustand (CRAWL-128) bleibt, wie CRAWL-125 ihn bestimmt“; Kriterium 6: Dateien aus 1.0 wie
    // CRAWL-125 allein (Fälle nahe der Grenze entfallen bei der Systemuhr, siehe Hilfsmodul).
    let env = Env::new();
    let dir = env.crawls_dir();
    let mut expected = Vec::new();
    for (i, state) in [CrawlState::Completed, CrawlState::Paused]
        .into_iter()
        .enumerate()
    {
        let (id, path) = new_crawl(&dir, 40 + i as u64, state);
        set_owner(&path, "mcp-stdio", FOREIGN_PID, Some(future()));
        expected.push((id.to_string(), state, "aktiver Besitzer".to_string()));
    }
    let (_, cases) = v10_cases();
    assert_fixture_consistent(&cases);
    let now = SystemClock.now();
    for (i, case) in cases.iter().enumerate() {
        let Some(case) = case.for_system_clock() else {
            continue;
        };
        let (id, _) = case.build(&dir, 100 + i as u64, now);
        expected.push((id.to_string(), case.oracle_state(), case.name.clone()));
    }
    let items = listed(&env);
    for (id, state, label) in expected {
        assert_eq!(status_of(&items, &id), state.as_str(), "{label}");
    }
}
