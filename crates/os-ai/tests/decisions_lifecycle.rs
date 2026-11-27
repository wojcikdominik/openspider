//! M4-D01: KI-Warteschlange und Crawl-Lebenszyklus (AI-020, Nachtrag 26.11.2026, K4-13; Verweise in AI-034): Pause,
//! Fortsetzen, Stoppen, Absturz, geordnetes Beenden, Atomarität und Besitz eines KI-Laufs. Die Tests binden sich über
//! das ai-mock-Szenario `hold` an bestimmte Requests, nicht an die Wanduhr.
#[path = "decisions_support.rs"]
mod support;

use std::cell::Cell;
use std::sync::{Arc, Mutex};

use os_ai::decisions::exec::{
    ActiveQuestion, Confirmation, OwnerLease, RunControl, RunEnd, RunOptions, Selection,
};
use os_model::budget::{BudgetLedger, MicroUsd};
use os_store::StoreError;
use support::*;

fn pt() -> ActiveQuestion {
    active(question("page-type"))
}

fn snapshot_ok(conn: &os_store::ai::cache::Connection) -> (MicroUsd, MicroUsd) {
    let l = ledger(conn, 5_000_000);
    let s = l.snapshot().unwrap();
    (s.spent, s.reserved)
}

fn log_sum(conn: &os_store::ai::cache::Connection) -> MicroUsd {
    os_store::ai::requests::total_cost(conn).unwrap()
}

/// Lässt die ersten `answered` Requests durch und ruft beim Request `answered + 1` (gehalten) `on_held` auf; gibt ihn
/// dann frei, wenn `release_last`.
async fn drive(env: &Env, answered: usize, on_held: impl FnOnce(), release_last: bool) {
    for i in 1..=answered {
        env.mock
            .until(&format!("Request {i} gehalten"), |_| {
                env.mock.posts(DECISIONS).len() == i
            })
            .await;
        assert_eq!(env.mock.release("openai", 1).await, 1);
    }
    env.mock
        .until("letzter Request gehalten", |_| {
            env.mock.posts(DECISIONS).len() == answered + 1
                && env.mock.held(DECISIONS) == answered + 1
        })
        .await;
    on_held();
    if release_last {
        assert_eq!(env.mock.release("openai", 1).await, 1);
    }
}

// spec: AI-020 [M4]
// spec: AI-034 [M4]
// Pause: Mock-Log zeigt 5 beantwortete Requests und Request 6 gehalten → Pause → nach Freigabe von Request 6 kommt
// kein weiterer Request (Mock-Log 6); 6 `Success`, 16 `Pending`, 3 `Skipped (scope)`; Budgetverbrauch = Summe der 6
// Protokollzeilen, keine offene Reservierung. Fortsetzen (nach Neuöffnen der Datei): genau 16 weitere Requests,
// Endstand 22 `Success`, 22 Protokollzeilen, keine URL doppelt im Mock-Log, kein `Pending (confirmation)`.
#[tokio::test]
async fn pause_and_resume() {
    let env = Env::new(&["openai"]).await;
    env.mock.hold("openai", 100).await;
    let q = pt();
    let qs = [q.clone()];
    let items = site_items("site");
    let dir = TempDir::new("pause");
    let (file, run) = crawl_file(&dir.0, &items);
    let control = RunControl::new();
    {
        let ledger = support::ledger(file.conn(), 5_000_000);
        let run_fut = env
            .run(
                file.conn(),
                run,
                &qs,
                &items,
                &ledger,
                control.clone(),
                RunOptions::default(),
            )
            .run();
        let c2 = control.clone();
        let (report, ()) = tokio::join!(run_fut, drive(&env, 5, move || c2.pause(), true));
        assert_eq!(report.unwrap().end, RunEnd::Paused);
        let s = ledger.snapshot().unwrap();
        assert_eq!(s.reserved, MicroUsd(0));
        assert_eq!(s.spent, log_sum(file.conn()));
    }
    // Weitere Plätze freigeben: es kommt nichts mehr.
    assert_eq!(env.mock.release("openai", 10).await, 0);
    assert_eq!(env.mock.posts(DECISIONS).len(), 6);
    let c = counts(file.conn(), &q.def);
    assert_eq!(count(&c, "Success"), 6, "{c:?}");
    assert_eq!(count(&c, "Pending"), 16, "{c:?}");
    assert_eq!(count(&c, "Skipped (scope)"), 3);
    assert_eq!(logged(file.conn()).len(), 6);
    drop(file);

    // Fortsetzen nach „App-Neustart“: Datei neu geöffnet, Ledger aus dem Protokoll.
    let file = reopen(&dir.0);
    let (spent, reserved) = snapshot_ok(file.conn());
    assert_eq!(reserved, MicroUsd(0));
    assert_eq!(spent, log_sum(file.conn()));
    let control = RunControl::new();
    let ledger = support::ledger(file.conn(), 5_000_000);
    // `hold` ist noch aktiv: jeden weiteren Request einzeln freigeben.
    let release_all = async {
        for k in 1..=16 {
            env.mock
                .until("nächster Request gehalten", |_| {
                    env.mock.posts(DECISIONS).len() == 6 + k
                })
                .await;
            assert_eq!(env.mock.release("openai", 1).await, 1);
        }
    };
    let options = RunOptions {
        confirmation: Confirmation::Granted,
        selection: Selection::Open,
        ..RunOptions::default()
    };
    let (report, ()) = tokio::join!(
        env.run(file.conn(), run, &qs, &items, &ledger, control, options)
            .run(),
        release_all
    );
    assert_eq!(report.unwrap().end, RunEnd::Completed);
    let posts = env.mock.posts(DECISIONS);
    assert_eq!(posts.len(), 22);
    let c = counts(file.conn(), &q.def);
    assert_eq!(count(&c, "Success"), 22);
    assert_eq!(count(&c, "Pending (confirmation)"), 0);
    assert_eq!(logged(file.conn()).len(), 22);
    let mut ids: Vec<String> = posts
        .iter()
        .map(|p| {
            let v: serde_json::Value = serde_json::from_str(&p.body).unwrap();
            v["input"]
                .as_str()
                .unwrap()
                .lines()
                .next()
                .unwrap()
                .to_string()
        })
        .collect();
    ids.sort();
    let n = ids.len();
    ids.dedup();
    assert_eq!(ids.len(), n, "keine URL doppelt");
}

// spec: AI-020 [M4]
// Stoppen: Stopp, während Request 6 gehalten wird → nach Freigabe ist Request 6 übernommen (Protokollzeile, `Success`),
// danach kein weiterer Request (auch nach Freigabe weiterer Plätze); 16 `Pending`. „KI-Analyse starten“ zeigt mit
// Schwelle 0,0001 USD den Kostenvoranschlag über 16 Einträge und sendet nach „Starten“ genau 16 Requests.
#[tokio::test]
async fn stop_then_start_a_new_run() {
    let env = Env::new(&["openai"]).await;
    env.mock.hold("openai", 6).await;
    let q = pt();
    let qs = [q.clone()];
    let items = site_items("site");
    let dir = TempDir::new("stop");
    let (file, run) = crawl_file(&dir.0, &items);
    let ledger = support::ledger(file.conn(), 5_000_000);
    let control = RunControl::new();
    let c2 = control.clone();
    let (report, ()) = tokio::join!(
        env.run(
            file.conn(),
            run,
            &qs,
            &items,
            &ledger,
            control.clone(),
            RunOptions::default()
        )
        .run(),
        drive(&env, 5, move || c2.stop(), true)
    );
    assert_eq!(report.unwrap().end, RunEnd::Stopped);
    assert_eq!(env.mock.release("openai", 10).await, 0);
    assert_eq!(env.mock.posts(DECISIONS).len(), 6);
    let log = logged(file.conn());
    assert_eq!(log.len(), 6);
    assert!(log.iter().all(|r| r.record.status == "Success"));
    assert_eq!(count(&counts(file.conn(), &q.def), "Pending"), 16);
    // Neuer KI-Lauf mit Voranschlag (Schwelle 0,0001 USD).
    let ask = RunOptions {
        confirmation: Confirmation::AskAbove(MicroUsd(100)),
        ..RunOptions::default()
    };
    let report = env
        .run(
            file.conn(),
            run,
            &qs,
            &items,
            &ledger,
            RunControl::new(),
            ask,
        )
        .run()
        .await
        .unwrap();
    assert_eq!(report.end, RunEnd::NeedsConfirmation);
    assert_eq!(report.estimate.unwrap().tasks[0].estimate.items, 16);
    assert_eq!(env.mock.posts(DECISIONS).len(), 6);
    let report = env
        .run(
            file.conn(),
            run,
            &qs,
            &items,
            &ledger,
            RunControl::new(),
            RunOptions::default(),
        )
        .run()
        .await
        .unwrap();
    assert_eq!(report.end, RunEnd::Completed);
    assert_eq!(env.mock.posts(DECISIONS).len(), 22);
    assert_eq!(count(&counts(file.conn(), &q.def), "Success"), 22);
}

// spec: AI-020 [M4]
// Fertige Einträge bleiben beim Fortsetzen unberührt: 2 Seiten `MOCK-SCENARIO:500-always` (`site-partial/`) vor der
// Pause gelaufen → nach dem Fortsetzen keine weiteren Requests für diese Seiten; „Fehlgeschlagene erneut anfragen“
// sendet danach genau diese.
#[tokio::test]
async fn finished_entries_are_not_resent() {
    let env = Env::new(&["openai"]).await;
    let q = pt();
    let qs = [q.clone()];
    let items = site_items("site-partial");
    let dir = TempDir::new("finished");
    let (file, run) = crawl_file(&dir.0, &items);
    let ledger = support::ledger(file.conn(), 5_000_000);
    let control = RunControl::new();
    let failing = |env: &Env| {
        env.mock
            .posts(DECISIONS)
            .iter()
            .filter(|r| r.body.contains("500-always"))
            .count()
    };
    let c2 = control.clone();
    let pause_after_failures = async {
        env.mock
            .until("beide 500-Seiten fertig", |_| failing(&env) == 12)
            .await;
        c2.pause();
    };
    let (report, ()) = tokio::join!(
        env.run(
            file.conn(),
            run,
            &qs,
            &items,
            &ledger,
            control,
            RunOptions::default()
        )
        .run(),
        pause_after_failures
    );
    let end = report.unwrap().end;
    assert!(matches!(end, RunEnd::Paused | RunEnd::Completed), "{end:?}");
    let before = env.mock.posts(DECISIONS).len();
    env.run(
        file.conn(),
        run,
        &qs,
        &items,
        &ledger,
        RunControl::new(),
        RunOptions::default(),
    )
    .run()
    .await
    .unwrap();
    assert_eq!(
        failing(&env),
        12,
        "keine weiteren Requests für die 500-Seiten"
    );
    let after_resume = env.mock.posts(DECISIONS).len();
    assert!(after_resume >= before);
    let retry = RunOptions {
        selection: Selection::Failed,
        ..RunOptions::default()
    };
    env.run(
        file.conn(),
        run,
        &qs,
        &items,
        &ledger,
        RunControl::new(),
        retry,
    )
    .run()
    .await
    .unwrap();
    let new: Vec<_> = env.mock.posts(DECISIONS)[after_resume..].to_vec();
    assert_eq!(new.len(), 12);
    assert!(new.iter().all(|r| r.body.contains("500-always")));
    let c = counts(file.conn(), &q.def);
    assert_eq!(count(&c, "Success"), 19);
    assert_eq!(count(&c, "Refusal"), 1);
}

// spec: AI-020 [M4]
// Absturz (Prozessende ohne Aufräumen, hier: Verwerfen des laufenden Laufs), während der Mock Request n hält → nach dem
// Öffnen: Eintrag n `Pending`, keine Protokollzeile für ihn, Budgetverbrauch = Summe der vorhandenen Protokollzeilen
// (keine Reservierung); beim Öffnen 0 Requests; „Fortsetzen“ sendet Eintrag n genau einmal; am Ende genau 1
// Protokollzeile für ihn.
#[tokio::test]
async fn crash_while_a_request_is_held() {
    let env = Env::new(&["openai"]).await;
    env.mock.hold("openai", 3).await;
    let q = pt();
    let qs = [q.clone()];
    let items = site_items("site");
    let dir = TempDir::new("crash");
    let (file, run) = crawl_file(&dir.0, &items);
    {
        let ledger = support::ledger(file.conn(), 5_000_000);
        let run_fut = env
            .run(
                file.conn(),
                run,
                &qs,
                &items,
                &ledger,
                RunControl::new(),
                RunOptions::default(),
            )
            .run();
        tokio::select! {
            _ = run_fut => panic!("der Lauf darf nicht enden"),
            () = drive(&env, 2, || {}, false) => {}
        }
        // Der Lauf ist verworfen (wie ein Prozessende): kein Commit, keine Freigabe.
    }
    env.mock
        .until("Verbindung des gehaltenen Requests abgebrochen", |log| {
            log.iter().any(|r| r.aborted)
        })
        .await;
    drop(file);
    let posts_before_open = env.mock.posts(DECISIONS).len();
    let file = reopen(&dir.0);
    assert_eq!(
        env.mock.posts(DECISIONS).len(),
        posts_before_open,
        "beim Öffnen 0 Requests"
    );
    let n_url = 3; // dritter Eintrag im Geltungsbereich = URL 3
    let row = os_store::ai::answers::get(file.conn(), n_url, &q.def.id, "1.0.0", "")
        .unwrap()
        .unwrap();
    assert_eq!(row.status, "Pending");
    let rows_for = |conn: &os_store::ai::cache::Connection, id: i64| {
        logged(conn)
            .iter()
            .filter(|r| r.record.url_id == Some(id))
            .count()
    };
    assert_eq!(rows_for(file.conn(), n_url), 0);
    let (spent, reserved) = snapshot_ok(file.conn());
    assert_eq!(reserved, MicroUsd(0));
    assert_eq!(spent, log_sum(file.conn()));
    assert_eq!(logged(file.conn()).len(), 2);
    // Fortsetzen.
    let ledger = support::ledger(file.conn(), 5_000_000);
    env.run(
        file.conn(),
        run,
        &qs,
        &items,
        &ledger,
        RunControl::new(),
        RunOptions::default(),
    )
    .run()
    .await
    .unwrap();
    assert_eq!(rows_for(file.conn(), n_url), 1);
    assert_eq!(count(&counts(file.conn(), &q.def), "Success"), 22);
    let for_n = env
        .mock
        .posts(DECISIONS)
        .iter()
        .filter(|r| r.body.contains("MOCK-ID:category-2"))
        .count();
    assert_eq!(for_n, 2, "ein abgebrochener und genau ein neuer Request");
}

// spec: AI-020 [M4]
// Geordnetes Beenden bzw. Schließen der Crawl-Datei, während der Mock Request n hält (ohne Freigabe) → der Lauf endet
// ohne auf die Antwort zu warten; danach wie beim Absturz (Eintrag `Pending`, keine Protokollzeile, Reservierung
// freigegeben).
#[tokio::test]
async fn orderly_shutdown_cancels_running_requests() {
    let env = Env::new(&["openai"]).await;
    env.mock.hold("openai", 2).await;
    let q = pt();
    let qs = [q.clone()];
    let items = site_items("site");
    let dir = TempDir::new("quit");
    let (file, run) = crawl_file(&dir.0, &items);
    let ledger = support::ledger(file.conn(), 5_000_000);
    let control = RunControl::new();
    let c2 = control.clone();
    let (report, ()) = tokio::join!(
        env.run(
            file.conn(),
            run,
            &qs,
            &items,
            &ledger,
            control,
            RunOptions::default()
        )
        .run(),
        drive(&env, 1, move || c2.abort(), false)
    );
    assert_eq!(report.unwrap().end, RunEnd::Aborted);
    let s = ledger.snapshot().unwrap();
    assert_eq!(s.reserved, MicroUsd(0));
    assert_eq!(s.spent, log_sum(file.conn()));
    env.mock
        .until("abgebrochen", |log| log.iter().any(|r| r.aborted))
        .await;
    assert_eq!(env.mock.release("openai", 1).await, 0);
    let row = os_store::ai::answers::get(file.conn(), 2, &q.def.id, "1.0.0", "")
        .unwrap()
        .unwrap();
    assert_eq!(row.status, "Pending");
    assert_eq!(logged(file.conn()).len(), 1);
}

// spec: AI-020 [M4]
// Atomarität: Fehlerinjektion zwischen Empfang einer Antwort und Commit (Test-Hook) → nach dem Öffnen weder Ergebnis
// noch Cache-Eintrag noch Protokollzeile noch verbuchte Kosten für diesen Eintrag; ohne Injektion alle vier.
#[tokio::test]
async fn failed_commit_leaves_nothing_behind() {
    let env = Env::new(&["openai"]).await;
    let q = pt();
    let qs = [q.clone()];
    let items = vec![text_item(1, "Seite eins"), text_item(2, "Seite zwei")];
    let dir = TempDir::new("atomic");
    let (file, run) = crawl_file(&dir.0, &items);
    let ledger = support::ledger(file.conn(), 5_000_000);
    let calls = Cell::new(0);
    let hook = || -> Result<(), StoreError> {
        calls.set(calls.get() + 1);
        if calls.get() == 2 {
            Err(StoreError::InvalidInput("injected".into()))
        } else {
            Ok(())
        }
    };
    let mut r = env.run(
        file.conn(),
        run,
        &qs,
        &items,
        &ledger,
        RunControl::new(),
        RunOptions::default(),
    );
    r.before_commit = Some(&hook);
    let report = r.run().await.unwrap();
    assert_eq!(report.commit_errors.len(), 1);
    assert_eq!(ledger.snapshot().unwrap().reserved, MicroUsd(0));
    drop(file);
    let file = reopen(&dir.0);
    let conn = file.conn();
    let get = |id| {
        os_store::ai::answers::get(conn, id, &q.def.id, "1.0.0", "")
            .unwrap()
            .unwrap()
    };
    assert_eq!(get(1).status, "Success");
    assert_eq!(get(2).status, "Pending");
    assert!(get(2).answer_json.is_none());
    let rows = logged(conn);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].record.url_id, Some(1));
    assert_eq!(os_store::ai::cache::stats(conn).unwrap().entries, 1);
    let (spent, _) = snapshot_ok(conn);
    assert_eq!(spent, rows[0].record.cost);
}

/// Attrappe der Besitzerkapsel (die echte Umsetzung liefert M4-K11).
struct Lease(Arc<Mutex<Vec<&'static str>>>);
impl OwnerLease for Lease {
    fn finish(self: Box<Self>) -> Result<(), StoreError> {
        self.0.lock().unwrap().push("finish");
        Ok(())
    }
}

// spec: AI-020 [M4]
// Besitz für die Dauer des KI-Laufs (Kapsel, an `os_ai::run_owner` aus M4-K11 anzubinden): Beginn vor dem ersten
// Request, Ende bei jedem Ende des Laufs (hier auch bei Pause); solange Request 1 gehalten wird, ist der Besitz
// eingetragen und nicht beendet. Ein abgelehnter Beginn (fremder aktiver Besitzer) sendet und schreibt nichts.
#[tokio::test]
async fn owner_lease_spans_the_run() {
    let env = Env::new(&["openai"]).await;
    env.mock.hold("openai", 1).await;
    let q = pt();
    let qs = [q.clone()];
    let items = vec![text_item(1, "Seite eins")];
    let dir = TempDir::new("owner");
    let (file, run) = crawl_file(&dir.0, &items);
    let ledger = support::ledger(file.conn(), 5_000_000);
    let events = Arc::new(Mutex::new(Vec::new()));
    let ev = events.clone();
    let mut r = env.run(
        file.conn(),
        run,
        &qs,
        &items,
        &ledger,
        RunControl::new(),
        RunOptions::default(),
    );
    r.owner = Some(Box::new(move || {
        ev.lock().unwrap().push("start");
        Ok(Box::new(Lease(ev.clone())) as Box<dyn OwnerLease>)
    }));
    let check = async {
        env.mock
            .until("Request 1 gehalten", |_| env.mock.held(DECISIONS) == 1)
            .await;
        assert_eq!(*events.lock().unwrap(), vec!["start"]);
        env.mock.release("openai", 1).await;
    };
    let (report, ()) = tokio::join!(r.run(), check);
    assert_eq!(report.unwrap().end, RunEnd::Completed);
    assert_eq!(*events.lock().unwrap(), vec!["start", "finish"]);

    let mut refused = env.run(
        file.conn(),
        run,
        &qs,
        &items,
        &ledger,
        RunControl::new(),
        RunOptions {
            selection: Selection::All,
            ..RunOptions::default()
        },
    );
    refused.owner = Some(Box::new(|| {
        Err(StoreError::InvalidInput(
            "gestartet von cli (PID 4242) – Steuerung nur dort".into(),
        ))
    }));
    let error = refused.run().await.unwrap_err();
    assert_eq!(
        error.to_string(),
        StoreError::InvalidInput("gestartet von cli (PID 4242) – Steuerung nur dort".into())
            .to_string(),
        "Fehler unverändert durchgereicht"
    );
    assert_eq!(env.mock.posts(DECISIONS).len(), 1);
}

// spec: AI-020 [M4]
// Heartbeat des KI-Laufs (Unterpunkt „Aktiver Besitzer außerhalb von `Running`“, Kriterium 3): mindestens vier Werte
// `heartbeat_at` in 7 s mit `hold`, `state` bleibt `Completed`; danach `owner_pid` leer.
#[tokio::test]
#[ignore = "wartet auf M4-K11: os_ai::run_owner::RunOwner (Heartbeat alle 2 s, Ende mit leerem owner_pid)"]
async fn heartbeat_of_an_ai_run() {
    unimplemented!("Anbindung von RunOwner::start in DecisionRun::owner (eine Zeile), dann simulierte Uhr und hold");
}

// spec: AI-020 [M4]
// KI während eines laufenden Crawls desselben Prozesses (Kriterium 4): endet der KI-Teil vor dem Crawl, bleibt
// `owner_pid` gesetzt und der Crawl `Running`.
#[tokio::test]
#[ignore = "wartet auf M4-K11: passiver RunOwner, wenn der eigene Prozess den Crawl führt"]
async fn ai_inside_the_own_running_crawl_keeps_the_owner() {
    unimplemented!("RunOwner::start auf einem Running-Crawl des eigenen Prozesses");
}

// spec: AI-020 [M4]
// Besitzer eines KI-Laufs (erster Satz): solange Request 1 gehalten wird, tragen die Crawl-Metadaten `owner_kind` und
// `owner_pid` des laufenden Prozesses, der Zustand bleibt `Completed`; nach dem Ende ist `owner_pid` leer.
#[tokio::test]
#[ignore = "wartet auf M4-K11: Schreibseite der Besitzerangaben (RunOwner)"]
async fn owner_metadata_during_an_ai_run() {
    unimplemented!(
        "wie owner_lease_spans_the_run mit RunOwner statt der Attrappe, dann crawl_meta lesen"
    );
}

// spec: AI-020 [M4]
// Löschschutz: Solange der Mock Request 1 eines KI-Laufs auf einem abgeschlossenen Crawl hält, lehnt die
// Crawl-Verwaltung das Löschen ab wie bei einem laufenden Crawl (OUT-228); nach dem Ende ist Löschen möglich.
#[tokio::test]
#[ignore = "wartet auf M4-K11: Leseseite „aktiver Besitzer“ in der Crawl-Verwaltung"]
async fn delete_protection_during_an_ai_run() {
    unimplemented!("Verwaltung prüft aktiven Besitzer");
}
