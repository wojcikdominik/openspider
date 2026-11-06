//! Gespeicherte Crawls laden und fortsetzen (OUT-184), Abbruch per Signal (OUT-204 Exit-Code 130, CRAWL-126).

mod common;

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant};

use common::{column, read_csv, resolve, site, Env};
use os_store::{CrawlFile, OpenMode};

fn crawl_args(site: &os_testsite::TestSite, extra: &[&str]) -> Vec<String> {
    let mut v: Vec<String> = [
        "--crawl",
        "http://site.test/",
        "--resolve",
        &resolve(site),
        "--set",
        "threads=1",
        "--set",
        "robots.retries=0",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    v.extend(extra.iter().map(|s| s.to_string()));
    v
}

fn only_crawl(env: &Env) -> (PathBuf, String) {
    let files = env.crawl_files();
    assert_eq!(files.len(), 1, "{files:?}");
    let id = files[0].file_stem().unwrap().to_string_lossy().into_owned();
    (files[0].clone(), id)
}

fn crawled_paths(env: &Env, out: &Path) -> Vec<String> {
    let _ = env;
    let mut v: Vec<String> = column(&read_csv(&out.join("internal_all.csv")), "Address")
        .iter()
        .map(|a| a.replace("http://site.test", ""))
        .collect();
    v.sort();
    v
}

// spec: OUT-184
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_stored_crawl_loads_by_id_without_any_request() {
    let site = site("out-basic-site").await;
    let env = Env::new();
    let run = env.run_async(crawl_args(&site, &[])).await;
    assert_eq!(run.code, 0, "{}", run.dump());
    let (_, id) = only_crawl(&env);
    let requests = site.total_requests();

    let run = env
        .run_async(vec![
            "--load-crawl".to_string(),
            id,
            "--export-tabs".into(),
            "Internal:All".into(),
            "--output-folder".into(),
            "out".into(),
        ])
        .await;
    assert_eq!(run.code, 0, "{}", run.dump());
    assert!(!run.stderr.contains("crawl incomplete"), "{}", run.dump());
    assert_eq!(site.total_requests(), requests, "kein Netzwerkzugriff");
    assert!(env.out().join("internal_all.csv").is_file());
}

// spec: OUT-184
#[test]
fn an_unknown_crawl_id_points_to_list_crawls() {
    let env = Env::new();
    let run = env.run(&["--load-crawl", "0199aaaa-0000-7000-8000-000000000001"]);
    assert_eq!(run.code, 2, "{}", run.dump());
    assert!(run.stderr.contains("--list-crawls"), "{}", run.dump());
    let run = env.run(&["--load-crawl", "weder-datei-noch-id"]);
    assert_eq!(run.code, 2, "{}", run.dump());
    assert!(run.stderr.contains("--list-crawls"), "{}", run.dump());
}

// spec: OUT-184
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_incomplete_crawl_loads_with_a_warning_and_exports_the_stored_state() {
    let site = site("out-queue").await;
    let env = Env::new();
    let run = env.run_async(crawl_args(&site, &["--max-urls", "4"])).await;
    assert_eq!(run.code, 0, "{}", run.dump());
    let (path, id) = only_crawl(&env);
    // Zustand „pausiert“, wie nach einem Abbruch.
    CrawlFile::open(&path, OpenMode::ReadWrite)
        .unwrap()
        .set_meta("state", "Paused")
        .unwrap();

    let args = |extra: &[&str]| -> Vec<String> {
        let mut v = vec![
            "--load-crawl".to_string(),
            id.clone(),
            "--export-tabs".into(),
            "Internal:All".into(),
            "--output-folder".into(),
            env.out().to_string_lossy().into_owned(),
            "--overwrite".into(),
        ];
        v.extend(extra.iter().map(|s| s.to_string()));
        v
    };
    let before = site.total_requests();
    let run = env.run_async(args(&[])).await;
    assert_eq!(run.code, 0, "{}", run.dump());
    assert!(run.stderr.contains("crawl incomplete"), "{}", run.dump());
    assert_eq!(
        site.total_requests(),
        before,
        "ohne --resume wird nicht weitergecrawlt"
    );
    assert!(env.out().join("internal_all.csv").is_file());
    let strict = env.run_async(args(&["--strict"])).await;
    assert_eq!(strict.code, 4, "{}", strict.dump());
}

// spec: OUT-184
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn resume_continues_a_crawl_that_stopped_at_the_url_limit() {
    let site = site("out-queue").await;
    let env = Env::new();
    let run = env.run_async(crawl_args(&site, &["--max-urls", "5"])).await;
    assert_eq!(run.code, 0, "{}", run.dump());
    let (_, id) = only_crawl(&env);
    let requests_after_first = site.total_requests();
    assert!(requests_after_first < 15, "{requests_after_first}");

    let run = env
        .run_async(vec![
            "--load-crawl".to_string(),
            id,
            "--resume".into(),
            "--max-urls".into(),
            "1000".into(),
            "--resolve".into(),
            resolve(&site),
            "--set".into(),
            "robots.retries=0".into(),
            "--export-tabs".into(),
            "Internal:All".into(),
            "--output-folder".into(),
            env.out().to_string_lossy().into_owned(),
        ])
        .await;
    assert_eq!(run.code, 0, "{}", run.dump());
    // Alle 20 Unterseiten wurden genau einmal abgerufen (keine Wiederholung bereits gecrawlter URLs).
    for i in 1..=20 {
        assert_eq!(site.count("site.test", &format!("/q{i}.html")), 1, "q{i}");
    }
    let urls = crawled_paths(&env, &env.out());
    assert!(urls.contains(&"/q20.html".to_string()), "{urls:?}");
    // Die gespeicherte Datei ist vollständig: eine weitere Fortsetzung hat nichts mehr zu tun.
    let list = env.run(&["--list-crawls", "--json"]);
    assert!(list.stdout.contains("Completed"), "{}", list.stdout);
}

// spec: CRAWL-128
// spec: AGENT-020
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn resume_refuses_a_stopped_crawl_with_exit_1_whatever_its_queue_holds() {
    let site = site("out-queue").await;
    let env = Env::new();
    let run = env.run_async(crawl_args(&site, &["--max-urls", "5"])).await;
    assert_eq!(run.code, 0, "{}", run.dump());
    let (path, id) = only_crawl(&env);
    // Derselbe Rest in der Warteschlange wie nach dem Limit, aber Endgrund `stopped` (UI-003 „Stoppen“).
    let file = CrawlFile::open(&path, OpenMode::ReadWrite).unwrap();
    let queued: i64 = file
        .conn()
        .query_row("SELECT COUNT(*) FROM queue", [], |r| r.get(0))
        .unwrap();
    assert!(queued > 0);
    file.set_meta("end_reason", "stopped").unwrap();
    drop(file);
    let requests = site.total_requests();

    let run = env
        .run_async(vec![
            "--load-crawl".to_string(),
            id,
            "--resume".into(),
            "--max-urls".into(),
            "1000".into(),
            "--resolve".into(),
            resolve(&site),
        ])
        .await;
    assert_eq!(run.code, 1, "{}", run.dump());
    assert!(
        run.stderr.contains("gestoppt – endgültig beendet"),
        "{}",
        run.dump()
    );
    assert_eq!(site.total_requests(), requests, "kein Abruf");
    let meta = CrawlFile::open(&path, OpenMode::ReadOnly)
        .unwrap()
        .meta()
        .unwrap();
    assert_eq!(meta.state, os_model::CrawlState::Completed);
    assert_eq!(meta.end_reason, Some(os_model::EndReason::Stopped));
}

// spec: OUT-184
#[test]
fn resume_needs_a_loaded_crawl() {
    let env = Env::new();
    let run = env.run(&["--crawl", "http://site.test/", "--resume"]);
    assert_eq!(run.code, 2, "{}", run.dump());
    assert!(run.stderr.contains("--load-crawl"), "{}", run.dump());
}

// spec: OUT-204, OUT-184
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn exit_code_1_when_a_running_crawl_is_resumed_within_the_heartbeat_window() {
    let site = site("out-queue").await;
    let env = Env::new();
    let mut child = {
        let mut cmd = env.command();
        cmd.args(crawl_args(&site, &[]))
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        cmd.spawn().unwrap()
    };
    // Warten, bis der Crawl läuft: Datei da und mindestens zwei Abrufe.
    let started = Instant::now();
    while (env.crawl_files().is_empty() || site.total_requests() < 2)
        && started.elapsed() < Duration::from_secs(20)
    {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let (_, id) = only_crawl(&env);
    let run = env
        .run_async(vec![
            "--load-crawl".to_string(),
            id,
            "--resume".into(),
            "--resolve".into(),
            resolve(&site),
        ])
        .await;
    let _ = child.kill();
    let _ = child.wait();
    assert_eq!(run.code, 1, "{}", run.dump());
    assert!(run.stderr.contains("läuft noch"), "{}", run.dump());
    assert!(run.last_line().ends_with("(Exit-Code 1)"), "{}", run.dump());
}

/// Abbruch per Signal: SIGINT (macOS) bzw. Strg+Untbr an die Prozessgruppe des Kindes (Windows; der
/// Ctrl-C-Handler der CLI fängt beide ab).
#[cfg(any(unix, windows))]
mod signals {
    use super::*;

    /// Sendet das Signal; `false`, wenn die Umgebung es nicht zustellen kann (Windows ohne Konsole).
    fn send_sigint(child: &std::process::Child) -> bool {
        #[cfg(unix)]
        {
            let pid = i32::try_from(child.id()).unwrap();
            // SAFETY: `kill` mit der PID eines eigenen Kindprozesses.
            let rc = unsafe { libc::kill(pid, libc::SIGINT) };
            assert_eq!(rc, 0, "kill(SIGINT) fehlgeschlagen");
            true
        }
        #[cfg(windows)]
        {
            use windows_sys::Win32::System::Console::{GenerateConsoleCtrlEvent, CTRL_BREAK_EVENT};
            // SAFETY: Aufruf mit der Prozessgruppe eines eigenen Kindprozesses (CREATE_NEW_PROCESS_GROUP).
            unsafe { GenerateConsoleCtrlEvent(CTRL_BREAK_EVENT, child.id()) != 0 }
        }
    }

    async fn wait_until_running(site: &os_testsite::TestSite, env: &Env, requests: u64) {
        let started = Instant::now();
        while (env.crawl_files().is_empty() || site.total_requests() < requests)
            && started.elapsed() < Duration::from_secs(20)
        {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        assert!(
            site.total_requests() >= requests,
            "der Crawl kam nicht in Gang"
        );
    }

    fn spawn(env: &Env, args: Vec<String>) -> std::process::Child {
        let mut cmd = env.command();
        cmd.args(args).stdout(Stdio::piped()).stderr(Stdio::piped());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            use windows_sys::Win32::System::Threading::CREATE_NEW_PROCESS_GROUP;
            cmd.creation_flags(CREATE_NEW_PROCESS_GROUP);
        }
        cmd.spawn().unwrap()
    }

    // spec: OUT-206
    // spec: OUT-228
    // spec: OUT-204, CRAWL-135
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn the_first_sigint_pauses_and_saves_with_exit_code_130_and_resume_completes_the_crawl() {
        let site = site("out-queue").await;
        let env = Env::new();
        let mut extra = crawl_args(&site, &["--export-tabs", "Internal:All", "--output-folder"]);
        extra.push(env.out().to_string_lossy().into_owned());
        let mut child = spawn(&env, extra);
        wait_until_running(&site, &env, 3).await;
        let interrupted_at = Instant::now();
        if !send_sigint(&child) {
            eprintln!("Signal nicht zustellbar (keine Konsole) – Test übersprungen");
            let _ = child.kill();
            let _ = child.wait();
            return;
        }
        let out = tokio::task::spawn_blocking(move || child.wait_with_output().unwrap())
            .await
            .unwrap();
        let run = common::Run::from_output(out);
        assert!(interrupted_at.elapsed() < Duration::from_secs(10));
        assert_eq!(run.code, 130, "{}", run.dump());
        assert!(
            run.last_line().ends_with("(Exit-Code 130)"),
            "{}",
            run.dump()
        );
        assert!(run.stdout.is_empty());
        assert!(
            common::file_names(&env.out()).is_empty(),
            "nach einem Abbruch folgen keine Exporte"
        );

        // Gespeichert und pausiert; die Warteschlange ist nicht leer.
        let (path, id) = only_crawl(&env);
        let file = CrawlFile::open(&path, OpenMode::ReadOnly).unwrap();
        assert_eq!(file.meta().unwrap().state.as_str(), "Paused");
        let queued: i64 = file
            .conn()
            .query_row(
                "SELECT COUNT(*) FROM urls WHERE state = 'queued'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(queued > 0, "Restwarteschlange erwartet");
        drop(file);

        // Fortsetzen: alle Seiten fertig, jede nur einmal abgerufen.
        let run = env
            .run_async(vec![
                "--load-crawl".to_string(),
                id,
                "--resume".into(),
                "--resolve".into(),
                resolve(&site),
                "--export-tabs".into(),
                "Internal:All".into(),
                "--output-folder".into(),
                env.out().to_string_lossy().into_owned(),
            ])
            .await;
        assert_eq!(run.code, 0, "{}", run.dump());
        for i in 1..=20 {
            assert_eq!(site.count("site.test", &format!("/q{i}.html")), 1, "q{i}");
        }
        let urls = crawled_paths(&env, &env.out());
        assert!(urls.contains(&"/q20.html".to_string()), "{urls:?}");
    }

    // spec: OUT-206
    // spec: OUT-228
    // spec: OUT-204, CRAWL-135
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_second_sigint_within_five_seconds_ends_the_process_immediately() {
        let site = site("out-queue").await;
        let env = Env::new();
        // Startseite mit 6 s Antwortzeit: Beim ersten Signal ist der Abruf noch unterwegs.
        let args = vec![
            "--crawl".to_string(),
            "http://site.test/slow.html".into(),
            "--resolve".into(),
            resolve(&site),
            "--set".into(),
            "robots.retries=0".into(),
        ];
        let mut child = spawn(&env, args);
        wait_until_running(&site, &env, 2).await;
        if !send_sigint(&child) {
            eprintln!("Signal nicht zustellbar (keine Konsole) – Test übersprungen");
            let _ = child.kill();
            let _ = child.wait();
            return;
        }
        tokio::time::sleep(Duration::from_millis(300)).await;
        let second = Instant::now();
        assert!(send_sigint(&child));
        let out = tokio::task::spawn_blocking(move || child.wait_with_output().unwrap())
            .await
            .unwrap();
        let run = common::Run::from_output(out);
        assert_eq!(run.code, 130, "{}", run.dump());
        // Ohne das zweite Signal wartet die Pause noch rund vier Sekunden auf den laufenden Abruf.
        assert!(
            second.elapsed() < Duration::from_secs(2),
            "{:?}",
            second.elapsed()
        );
        // Die Crawl-Datei bleibt öffnenbar und konsistent.
        let (path, _) = only_crawl(&env);
        let file = CrawlFile::open(&path, OpenMode::ReadOnly).unwrap();
        let check: String = file
            .conn()
            .query_row("PRAGMA integrity_check", [], |r| r.get(0))
            .unwrap();
        assert_eq!(check, "ok");
    }
}

// spec: OUT-184
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_crawl_file_of_a_newer_schema_or_a_foreign_file_is_rejected_without_changes() {
    let site = site("out-basic-site").await;
    let env = Env::new();
    let run = env.run_async(crawl_args(&site, &[])).await;
    assert_eq!(run.code, 0, "{}", run.dump());
    let (path, _) = only_crawl(&env);

    // Kopie mit höherer Schemaversion
    let newer = env.work.path().join("neuer.ospider");
    {
        let file = CrawlFile::open(&path, OpenMode::ReadWrite).unwrap();
        file.conn()
            .execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
            .ok();
    }
    std::fs::copy(&path, &newer).unwrap();
    {
        let file = CrawlFile::open(&newer, OpenMode::ReadWrite).unwrap();
        file.conn()
            .execute_batch("PRAGMA user_version = 99;")
            .unwrap();
    }
    let before = std::fs::read(&newer).unwrap();
    let run = env
        .run_async(vec![
            "--load-crawl".to_string(),
            newer.to_string_lossy().into_owned(),
        ])
        .await;
    assert_eq!(run.code, 2, "{}", run.dump());
    assert!(run.stderr.contains("nicht ladbar"), "{}", run.dump());
    assert_eq!(
        std::fs::read(&newer).unwrap(),
        before,
        "die Datei bleibt unverändert"
    );

    // Fremde Datei
    let foreign = env.work.path().join("liste.txt");
    std::fs::write(&foreign, "keine Crawl-Datei").unwrap();
    let run = env
        .run_async(vec![
            "--load-crawl".to_string(),
            foreign.to_string_lossy().into_owned(),
        ])
        .await;
    assert_eq!(run.code, 2, "{}", run.dump());
    assert_eq!(
        std::fs::read_to_string(&foreign).unwrap(),
        "keine Crawl-Datei"
    );
}
