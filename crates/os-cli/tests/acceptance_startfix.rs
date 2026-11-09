//! Unabhängige Abnahmetests B4-STARTFIX (Kommandozeile und Planer): Verhalten, wenn ein Crawl an seiner Start-URL scheitert oder die
//! robots.txt die Start-URL sperrt.
//!
//! Erwartetes Verhalten stammt ausschließlich aus der Spezifikation (OUT-182, OUT-204, OUT-152, OUT-169, OUT-281, OUT-282,
//! CRAWL-128 samt der OS-Festlegungen vom 09.11.2026); der Produktcode wurde nur gelesen, um zu wissen, wie die Schnittstellen
//! aufgerufen werden. Geprüft wird von außen: das Binary `openspider` (Exit-Codes, stderr, Crawl-Datei, Historie) und – für die
//! Mail der Planer-Aufgabe, deren Konto-Kennwort im Schlüsselbund eines Kindprozesses fehlen würde – der Planerlauf im Prozess
//! (`run_in`) mit dem SMTP-Mock der Testsite.
//!
//! Kriterienliste: `docs/plan/M3-reports/b4-startfix-tests.md`.

mod common;
#[path = "../../os-app/tests/acceptance_startfix/fixtures.rs"]
mod fixtures;
#[path = "../../os-app/tests/acceptance_startfix/mime.rs"]
mod mime;

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use common::{Env, Run};
use fixtures::{robots_fixture, robots_status_fixture, RULE, RULE_LINE};
use os_analyze::analysis::CancelToken;
use os_cli::exit::Exit;
use os_cli::reporter::Reporter;
use os_cli::run_task::{comparison_mail::ComparisonMailHooks, run_in, DefaultHooks, RunCtx};
use os_cli::run_task::{TaskHooks, TaskRun};
use os_cli::signals::Signals;
use os_fetch::MemorySecretStore;
use os_model::SystemClock;
use os_sched::history::{HistoryStore, RunRecord, RunResult};
use os_sched::mail::{Account, AccountStore, Auth, Tls};
use os_sched::tasks::{CrawlSpec, Execution, Notify, Output, Schedule, Source, Task, TaskStore};
use os_store::{CrawlFile, OpenMode};
use os_testsite::mocks::smtp::{Config, SmtpMock};
use os_testsite::{SiteOptions, TestSite};

const SKIPPED: &str = "Auto-Vergleich übersprungen (Crawl fehlgeschlagen)";
const REFUSED: [&str; 4] = [
    "connection_refused",
    "connection refused",
    "verbindung abgelehnt",
    "abgelehnt",
];

// ---------------------------------------------------------------------------------------------------
// Hilfen

fn closed_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap().port()
}

fn closed_url() -> String {
    format!("http://127.0.0.1:{}/", closed_port())
}

async fn dir_site(dir: &Path) -> TestSite {
    TestSite::start_dir_with(
        dir,
        SiteOptions {
            port: 0,
            log_requests: false,
        },
    )
    .await
    .unwrap()
}

fn has_any(text: &str, needles: &[&str]) -> bool {
    let lower = text.to_lowercase();
    needles.iter().any(|n| lower.contains(&n.to_lowercase()))
}

fn crawl_args(url: &str, extra: &[&str]) -> Vec<String> {
    ["--crawl", url, "--set", "robots.retries=0"]
        .iter()
        .chain(extra.iter())
        .map(|s| s.to_string())
        .collect()
}

/// Zustand der einzigen Crawl-Datei des Env laut Manifest (OUT-233).
fn crawl_state(env: &Env) -> String {
    let files = env.crawl_files();
    assert_eq!(files.len(), 1, "{files:?}");
    let file = CrawlFile::open(&files[0], OpenMode::ReadOnly).unwrap();
    file.conn()
        .query_row(
            "SELECT value FROM q_manifest WHERE key = 'state'",
            [],
            |r| r.get::<_, String>(0),
        )
        .unwrap()
}

/// Die Zeilen von stderr, die die robots.txt-URL nennen.
fn lines_with<'a>(run: &'a Run, needle: &str) -> Vec<&'a str> {
    run.stderr.lines().filter(|l| l.contains(needle)).collect()
}

/// SMTP-Mock ohne Anmeldung: Das Konto braucht dann kein Kennwort im Schlüsselbund (auch nicht im Kindprozess der CLI).
async fn smtp() -> SmtpMock {
    SmtpMock::start(Config {
        username: String::new(),
        ..Config::default()
    })
    .await
    .unwrap()
}

fn save_account(settings: &Path, mock: &SmtpMock) {
    AccountStore::new(settings)
        .save(
            &Account {
                id: "fixture".into(),
                label: "Mock".into(),
                host: "localhost".into(),
                port: mock.addr.port(),
                tls: Tls::None,
                username: String::new(),
                from: "crawler@site.test".into(),
                auth: Auth::Password,
                allow_insecure: true,
                client_id: None,
            },
            None,
            &MemorySecretStore::new(),
        )
        .unwrap();
}

/// Env mit globaler Einstellung „E-Mail bei Crawl-Ende“.
fn mail_env(mock: &SmtpMock, on_success: bool, on_failure: bool) -> Env {
    let env = Env::new();
    let mut settings = os_model::settings::AppSettings::load(env.home.path()).unwrap();
    settings.network.proxy.mode = os_model::ProxyMode::None;
    settings.accounts.notify_account = Some("fixture".into());
    settings.accounts.notify_to = vec!["recipient@site.test".into()];
    settings.accounts.notify_on_success = on_success;
    settings.accounts.notify_on_failure = on_failure;
    settings.save(env.home.path()).unwrap();
    save_account(env.home.path(), mock);
    env
}

/// Nachrichten des Mocks; wartet kurz, damit eine zweite Mail desselben Laufs nicht unbemerkt bleibt.
async fn messages(mock: &SmtpMock) -> Vec<mime::Message> {
    tokio::time::sleep(Duration::from_millis(1000)).await;
    mock.messages()
        .into_iter()
        .map(|m| mime::parse(&m.data))
        .collect()
}

/// Fehler-Mail (OUT-281, OUT-282): Betreff-Präfix, HTML und Klartext, Adresse der Start-URL in beiden Teilen.
fn assert_failure_mail(m: &mime::Message, address: &str) {
    assert!(m.subject.starts_with("[FAILED]"), "Betreff: {}", m.subject);
    assert!(
        !m.plain.is_empty() && !m.html.is_empty(),
        "HTML und Klartext erwartet: {}",
        m.raw
    );
    for (kind, parts) in [("Klartext", &m.plain), ("HTML", &m.html)] {
        let text = parts.join("\n");
        assert!(text.contains(address), "{kind} ohne {address}: {text}");
    }
}

/// Die Mail nennt die Fehlerklasse (hier: Verbindung abgelehnt) in Klartext und HTML.
fn assert_names_class(m: &mime::Message) {
    for (kind, parts) in [("Klartext", &m.plain), ("HTML", &m.html)] {
        let text = parts.join("\n");
        assert!(has_any(&text, &REFUSED), "{kind} ohne Fehlerklasse: {text}");
    }
}

// ---------------------------------------------------------------------------------------------------
// OUT-182 / OUT-204 / CRAWL-128: Exit-Code, Warnung, Schlusszeile

// Start-URL nicht erreichbar: Exit 0 mit Warnung, der Zustand ist „Fehlgeschlagen“ (CRAWL-128), und die Schlusszeile nennt den
// Fehler des Crawls statt „Fertig“ (OS-Festlegung 09.11.2026). Mit `--fail-on-start-error` Exit 3.
// spec: OUT-182
// spec: OUT-204
// spec: CRAWL-128
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_unreachable_start_url_exits_0_with_a_warning_and_the_final_line_names_the_error() {
    let url = closed_url();
    let env = Env::new();
    let run = env.run_async(crawl_args(&url, &[])).await;
    assert_eq!(run.code, 0, "{}", run.dump());
    let address = url.trim_start_matches("http://").trim_end_matches('/');
    assert!(
        run.stderr
            .lines()
            .any(|l| l.contains(address) && l.contains("nicht erreichbar")),
        "Warnung mit Adresse fehlt: {}",
        run.dump()
    );
    assert!(
        run.last_line().contains("nicht erreichbar"),
        "Schlusszeile: {}",
        run.dump()
    );
    assert!(!run.last_line().contains("Fertig"), "{}", run.dump());
    assert_eq!(crawl_state(&env), "Failed");

    let env = Env::new();
    let run = env
        .run_async(crawl_args(&url, &["--fail-on-start-error"]))
        .await;
    assert_eq!(run.code, 3, "{}", run.dump());
    assert!(run.stderr.contains("nicht erreichbar"), "{}", run.dump());
}

// Start-URL per robots.txt gesperrt: kein Fehlschlag. Exit 0 auch mit `--fail-on-start-error`, der Zustand ist „Abgeschlossen“, die
// Warnung auf stderr nennt die robots.txt-URL und die sperrende Zeile (Zeilennummer und Regel).
// spec: OUT-182
// spec: OUT-204
// spec: CRAWL-128
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_start_url_blocked_by_robots_txt_exits_0_even_with_fail_on_start_error_and_warns_with_the_line(
) {
    let dir = robots_fixture();
    let site = dir_site(dir.path()).await;
    let base = site.base_url();
    let robots_url = format!("{base}/robots.txt");
    let start = format!("{base}/secret/index.html");
    for extra in [&[][..], &["--fail-on-start-error"][..]] {
        let env = Env::new();
        let run = env.run_async(crawl_args(&start, extra)).await;
        assert_eq!(run.code, 0, "{extra:?}: {}", run.dump());
        assert_eq!(crawl_state(&env), "Completed", "{extra:?}");
        let lines = lines_with(&run, &robots_url);
        assert!(
            !lines.is_empty(),
            "{extra:?}: Warnung fehlt: {}",
            run.dump()
        );
        // Zeilennummer und Regel stehen in derselben Warnung wie die robots.txt-URL (Hostteil entfernt, damit die Portnummer
        // nicht als Zeilennummer durchgeht).
        let warning = lines
            .iter()
            .find(|l| l.contains(RULE))
            .unwrap_or_else(|| panic!("{extra:?}: Regel {RULE:?} fehlt in {lines:?}"));
        let stripped = warning.replace(&base, "");
        assert!(
            stripped.contains(&RULE_LINE.to_string()),
            "{extra:?}: Zeilennummer {RULE_LINE} fehlt in {warning:?}"
        );
        assert!(
            !run.stderr.contains("nicht erreichbar"),
            "{extra:?}: {}",
            run.dump()
        );
    }
}

// Ist die Start-URL erlaubt, auch wenn Teile der Site gesperrt sind, gibt es keine Sperr-Warnung.
// spec: OUT-182
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_allowed_start_url_gets_no_robots_warning() {
    let dir = robots_fixture();
    let site = dir_site(dir.path()).await;
    let env = Env::new();
    let run = env
        .run_async(crawl_args(
            &format!("{}/", site.base_url()),
            &["--fail-on-start-error"],
        ))
        .await;
    assert_eq!(run.code, 0, "{}", run.dump());
    assert!(!run.stderr.contains(RULE), "{}", run.dump());
    assert_eq!(crawl_state(&env), "Completed");
}

// Regel „robots.txt nicht erreichbar“ (robots.txt antwortet mit 5xx oder 429): kein Fehlschlag. Exit 0 auch mit
// `--fail-on-start-error`, Zustand „Abgeschlossen“, eine Warnung zur Sperre ohne die Meldung „Start-URL nicht erreichbar“.
// spec: OUT-182
// spec: OUT-204
// spec: CRAWL-128
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_robots_txt_answering_5xx_or_429_is_no_start_error() {
    for status in [500u16, 503, 429] {
        let dir = robots_status_fixture(status);
        let site = dir_site(dir.path()).await;
        let env = Env::new();
        let run = env
            .run_async(crawl_args(
                &format!("{}/", site.base_url()),
                &["--fail-on-start-error"],
            ))
            .await;
        assert_eq!(run.code, 0, "{status}: {}", run.dump());
        assert_eq!(crawl_state(&env), "Completed", "{status}");
        assert!(
            run.stderr
                .lines()
                .any(|l| l.starts_with("Warnung") && l.contains("robots.txt")),
            "{status}: Warnung zur Sperre fehlt: {}",
            run.dump()
        );
        assert!(!run.stderr.contains("nicht erreichbar"), "{status}: {}", run.dump());
    }
}

// Die Warnung zur Regel „robots.txt nicht erreichbar“ nennt die robots.txt-URL und den Grund (hier der Statuscode der robots.txt),
// wie die Warnung zur Sperre per `Disallow` die robots.txt-URL und die Zeile nennt (OS-Festlegung 09.11.2026).
// spec: OUT-182
// spec: CRAWL-128
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_warning_for_an_unreachable_robots_txt_names_its_url_and_the_reason() {
    for status in [500u16, 503, 429] {
        let dir = robots_status_fixture(status);
        let site = dir_site(dir.path()).await;
        let robots_url = format!("{}/robots.txt", site.base_url());
        let env = Env::new();
        let run = env
            .run_async(crawl_args(
                &format!("{}/", site.base_url()),
                &["--fail-on-start-error"],
            ))
            .await;
        assert_eq!(run.code, 0, "{status}: {}", run.dump());
        let warnings: Vec<&str> = run
            .stderr
            .lines()
            .filter(|l| l.starts_with("Warnung"))
            .collect();
        assert!(
            warnings.iter().any(|l| l.contains(&robots_url)),
            "{status}: robots.txt-URL fehlt in {warnings:?}"
        );
        let code = status.to_string();
        assert!(
            warnings.iter().any(|l| l.contains(&code)),
            "{status}: Grund (Statuscode) fehlt in {warnings:?}"
        );
    }
}

// ---------------------------------------------------------------------------------------------------
// OUT-281: Fehler-Mail der Kommandozeile (globale Einstellung)

// Fehlgeschlagener Crawl (Exit 0): genau eine Fehler-Mail mit Ursache, Fehlerklasse und Adresse, nie zusätzlich eine Erfolgsmail –
// unabhängig davon, ob „bei Erfolg“ an ist.
// spec: OUT-281
// spec: OUT-282
// spec: CRAWL-128
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cli_a_failed_crawl_sends_exactly_one_failure_mail() {
    for on_success in [true, false] {
        let mock = smtp().await;
        let env = mail_env(&mock, on_success, true);
        let url = closed_url();
        let run = env.run_async(crawl_args(&url, &[])).await;
        assert_eq!(run.code, 0, "{}", run.dump());
        let mails = messages(&mock).await;
        assert_eq!(mails.len(), 1, "bei Erfolg {on_success}: {mails:?}");
        assert_failure_mail(
            &mails[0],
            url.trim_start_matches("http://").trim_end_matches('/'),
        );
    }
}

// Mit `--fail-on-start-error` (Exit 3) gilt dasselbe: eine einzige Fehler-Mail.
// spec: OUT-281
// spec: OUT-182
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cli_a_failed_crawl_with_exit_3_also_sends_exactly_one_failure_mail() {
    let mock = smtp().await;
    let env = mail_env(&mock, true, true);
    let url = closed_url();
    let run = env
        .run_async(crawl_args(&url, &["--fail-on-start-error"]))
        .await;
    assert_eq!(run.code, 3, "{}", run.dump());
    let mails = messages(&mock).await;
    assert_eq!(mails.len(), 1, "{mails:?}");
    assert_failure_mail(
        &mails[0],
        url.trim_start_matches("http://").trim_end_matches('/'),
    );
}

// Die Fehler-Mail nennt die Fehlerklasse der Ursache (Verbindung abgelehnt), mit dem Standard „robots.txt beachten“ (die robots.txt
// der Start-URL ist nicht erreichbar) ebenso wie mit „robots.txt ignorieren“ (die Start-URL selbst).
// spec: OUT-281
// spec: CRAWL-128
async fn cli_mail_class_case(extra: &[&str]) {
    let mock = smtp().await;
    let env = mail_env(&mock, false, true);
    let url = closed_url();
    let run = env.run_async(crawl_args(&url, extra)).await;
    assert_eq!(run.code, 0, "{}", run.dump());
    let mails = messages(&mock).await;
    assert_eq!(mails.len(), 1, "{mails:?}");
    assert_names_class(&mails[0]);
}

// spec: OUT-281
// spec: CRAWL-128
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cli_failure_mail_names_the_error_class_with_robots_respected() {
    cli_mail_class_case(&[]).await;
}

// spec: OUT-281
// spec: CRAWL-128
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cli_failure_mail_names_the_error_class_with_robots_ignored() {
    cli_mail_class_case(&["--set", "robots.mode=ignore"]).await;
}

// Mit „bei Fehlschlag“ aus geht für den fehlgeschlagenen Crawl keine Mail hinaus, auch keine Erfolgsmail. Die Kontrolle davor
// zeigt, dass die Mail-Technik funktioniert: Ein erfolgreicher Crawl sendet seine Erfolgsmail (ohne Präfix).
// spec: OUT-281
// spec: CRAWL-128
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cli_a_failed_crawl_sends_no_mail_when_failure_mails_are_off() {
    let dir = robots_fixture();
    let site = dir_site(dir.path()).await;
    let mock = smtp().await;
    let env = mail_env(&mock, true, false);
    let run = env
        .run_async(crawl_args(&format!("{}/", site.base_url()), &[]))
        .await;
    assert_eq!(run.code, 0, "{}", run.dump());
    let mails = messages(&mock).await;
    assert_eq!(mails.len(), 1, "Kontrolle: {mails:?}");
    assert!(!mails[0].subject.starts_with("[FAILED]"));

    let env = mail_env(&mock, true, false);
    let run = env.run_async(crawl_args(&closed_url(), &[])).await;
    assert_eq!(run.code, 0, "{}", run.dump());
    let mails = messages(&mock).await;
    assert_eq!(mails.len(), 1, "{mails:?}");
}

// Ein per robots.txt gesperrter Start ist kein Fehlschlag (CRAWL-128): Erfolgsmail (ohne Präfix), und mit „bei Erfolg“ aus keine
// Mail.
// spec: OUT-281
// spec: CRAWL-128
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cli_a_robots_block_is_reported_as_success() {
    let dir = robots_fixture();
    let site = dir_site(dir.path()).await;
    let start = format!("{}/secret/index.html", site.base_url());

    let mock = smtp().await;
    let env = mail_env(&mock, true, true);
    let run = env.run_async(crawl_args(&start, &[])).await;
    assert_eq!(run.code, 0, "{}", run.dump());
    let mails = messages(&mock).await;
    assert_eq!(mails.len(), 1, "{mails:?}");
    assert!(
        !mails[0].subject.starts_with("[FAILED]"),
        "{}",
        mails[0].subject
    );

    let mock = smtp().await;
    let env = mail_env(&mock, false, true);
    let run = env.run_async(crawl_args(&start, &[])).await;
    assert_eq!(run.code, 0, "{}", run.dump());
    let mails = messages(&mock).await;
    assert!(mails.is_empty(), "{mails:?}");
}

// ---------------------------------------------------------------------------------------------------
// OUT-152: Auto-Vergleich der Kommandozeile

fn compare_args(url: &str, project: &str) -> Vec<String> {
    crawl_args(url, &["--project-name", project, "--compare-with-previous"])
}

fn saved_comparisons(env: &Env) -> usize {
    std::fs::read_dir(env.crawls_dir().join("comparisons"))
        .map(|d| d.flatten().count())
        .unwrap_or(0)
}

// Endet der Crawl als „Fehlgeschlagen“, wird nicht verglichen: Der Lauf vermerkt „Auto-Vergleich übersprungen (Crawl
// fehlgeschlagen)“, das zählt nicht als fehlgeschlagener Vergleich (kein Exit-Code ≠ 0, kein Fehler „Auto-Vergleich
// fehlgeschlagen“), und die Schlusszeile nennt den Fehler des Crawls, nie „Crawl erfolgreich, aber Auto-Vergleich fehlgeschlagen“.
// Es entsteht kein gespeicherter Vergleich. Gilt mit und ohne vorherigen erfolgreichen Crawl des Projekts.
// spec: OUT-152
// spec: CRAWL-128
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cli_a_failed_crawl_skips_the_auto_comparison_and_names_the_crawl_error() {
    let dir = robots_fixture();
    let site = dir_site(dir.path()).await;
    let env = Env::new();

    // Ohne vorherigen Crawl: der erste Lauf des Projekts scheitert.
    let first = env.run_async(compare_args(&closed_url(), "Shop")).await;
    assert_failed_compare_run(&first);
    assert_eq!(saved_comparisons(&env), 0);

    // Mit vorherigem erfolgreichen Crawl (ein Vergleich wäre sonst möglich).
    let ok = env
        .run_async(compare_args(&format!("{}/", site.base_url()), "Shop"))
        .await;
    assert_eq!(ok.code, 0, "{}", ok.dump());
    let before = saved_comparisons(&env);
    let run = env.run_async(compare_args(&closed_url(), "Shop")).await;
    assert_failed_compare_run(&run);
    assert_eq!(saved_comparisons(&env), before, "{}", run.dump());
}

fn assert_failed_compare_run(run: &Run) {
    assert_eq!(run.code, 0, "{}", run.dump());
    assert!(
        run.stderr.lines().any(|l| l.contains(SKIPPED)),
        "Vermerk fehlt: {}",
        run.dump()
    );
    assert!(
        !has_any(
            &run.stderr,
            &[
                "Auto-Vergleich fehlgeschlagen",
                "auto comparison",
                "Crawl erfolgreich",
            ]
        ),
        "{}",
        run.dump()
    );
    assert!(
        run.last_line().contains("nicht erreichbar"),
        "Schlusszeile: {}",
        run.dump()
    );
}

// Die Fehler-Mail enthält statt der Änderungstabelle denselben Vermerk (OUT-152).
// spec: OUT-152
// spec: OUT-281
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cli_the_failure_mail_of_a_skipped_auto_comparison_carries_the_note_instead_of_the_change_table(
) {
    let dir = robots_fixture();
    let site = dir_site(dir.path()).await;
    let mock = smtp().await;
    let env = mail_env(&mock, true, true);
    let ok = env
        .run_async(compare_args(&format!("{}/", site.base_url()), "Shop"))
        .await;
    assert_eq!(ok.code, 0, "{}", ok.dump());
    assert_eq!(messages(&mock).await.len(), 1);

    let url = closed_url();
    let run = env.run_async(compare_args(&url, "Shop")).await;
    assert_eq!(run.code, 0, "{}", run.dump());
    let mails = messages(&mock).await;
    assert_eq!(mails.len(), 2, "{mails:?}");
    let failure = &mails[1];
    assert_failure_mail(
        failure,
        url.trim_start_matches("http://").trim_end_matches('/'),
    );
    for (kind, parts) in [("Klartext", &failure.plain), ("HTML", &failure.html)] {
        let text = parts.join("\n");
        assert!(text.contains(SKIPPED), "{kind} ohne Vermerk: {text}");
        assert!(!text.contains("Change %"), "{kind} mit Tabelle: {text}");
        assert!(!has_any(&text, &["Crawl erfolgreich"]), "{kind}: {text}");
    }
}

// ---------------------------------------------------------------------------------------------------
// OUT-169: Planer-Historie (`openspider --run-task` als eigener Prozess)

fn overrides() -> std::collections::BTreeMap<String, String> {
    [("threads", "1"), ("robots.retries", "0")]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

fn task(id: &str, project: &str, url: &str, auto_compare: bool, notify: Notify) -> Task {
    Task {
        id: id.into(),
        name: id.to_uppercase(),
        project: project.into(),
        description: String::new(),
        enabled: true,
        schedule: Schedule::Daily {
            time: "03:00".into(),
            from: None,
        },
        crawl: CrawlSpec {
            source: Source::Spider { url: url.into() },
            config: None,
            auth_profile: None,
            overrides: overrides(),
        },
        execution: Execution {
            auto_compare,
            ..Execution::default()
        },
        apis: Vec::new(),
        output: Output::default(),
        exports: Vec::new(),
        export_options: None,
        preset: None,
        custom_summary: None,
        notify,
    }
}

fn save(env: &Env, task: &Task) {
    TaskStore::in_settings(env.home.path())
        .save(task.clone())
        .expect("Aufgabe speicherbar");
}

fn history(env: &Env, id: &str) -> Vec<RunRecord> {
    HistoryStore::in_settings(env.home.path()).list(Some(id), 0, 10)
}

/// Prüft den Historieneintrag eines Laufs, dessen Crawl als „Fehlgeschlagen“ endet (OS-Festlegung 09.11.2026): `Failed` – den
/// Fehlschlag stuft der Crawl ein, nicht der Vergleich –, `Message` ist die Fehlermeldung des Crawls, Exit-Code 0 (OUT-182).
fn assert_failed_record(record: &RunRecord, auto_compare: bool) {
    assert_eq!(record.result, RunResult::Failed, "{record:?}");
    assert_eq!(record.exit_code, Some(0), "{record:?}");
    let message = record.message.as_deref().unwrap_or_default();
    assert!(message.contains("nicht erreichbar"), "{record:?}");
    assert!(
        !has_any(
            message,
            &["erfolgreich", "Auto-Vergleich", "auto comparison"]
        ),
        "{record:?}"
    );
    let skipped = record.warnings.iter().any(|w| w == SKIPPED);
    assert_eq!(skipped, auto_compare, "Vermerk in der Historie: {record:?}");
    assert!(
        !record
            .warnings
            .iter()
            .any(|w| has_any(w, &["Auto-Vergleich fehlgeschlagen", "auto comparison"])),
        "{record:?}"
    );
}

// Ohne Auto-Vergleich: `Failed` in der Historie, Meldung = Fehlermeldung des Crawls, Prozess-Exit 0.
// spec: OUT-169
// spec: OUT-182
// spec: CRAWL-128
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn planner_history_shows_failed_for_a_crawl_that_ends_as_failed() {
    let env = Env::new();
    save(
        &env,
        &task("nightly", "Shop", &closed_url(), false, Notify::default()),
    );
    let run = env
        .run_async(vec!["--run-task".to_string(), "nightly".into()])
        .await;
    assert_eq!(run.code, 0, "{}", run.dump());
    let runs = history(&env, "nightly");
    assert_eq!(runs.len(), 1, "{runs:?}");
    assert_failed_record(&runs[0], false);
    let log = std::fs::read_to_string(runs[0].log_path.clone().expect("Lauf-Log")).unwrap();
    assert!(log.contains("result=Failed"), "{log}");
}

// Mit Auto-Vergleich, sowohl ohne als auch mit vorherigem erfolgreichen Crawl des Projekts: `Failed`, der Vergleich ist in Log und
// Historie als „übersprungen (Crawl fehlgeschlagen)“ vermerkt, nie „Crawl erfolgreich, aber Auto-Vergleich fehlgeschlagen“; kein
// gespeicherter Vergleich.
// spec: OUT-169
// spec: OUT-152
// spec: CRAWL-128
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn planner_history_shows_failed_and_a_skipped_auto_comparison() {
    let dir = robots_fixture();
    let site = dir_site(dir.path()).await;
    let env = Env::new();

    // Erster Lauf des Projekts, noch ohne vorherigen Crawl.
    save(
        &env,
        &task("first", "Shop", &closed_url(), true, Notify::default()),
    );
    let run = env
        .run_async(vec!["--run-task".to_string(), "first".into()])
        .await;
    assert_eq!(run.code, 0, "{}", run.dump());
    let runs = history(&env, "first");
    assert_eq!(runs.len(), 1, "{runs:?}");
    assert_failed_record(&runs[0], true);

    // Vorheriger erfolgreicher Crawl des Projekts.
    save(
        &env,
        &task(
            "good",
            "Shop",
            &format!("{}/", site.base_url()),
            true,
            Notify::default(),
        ),
    );
    let run = env
        .run_async(vec!["--run-task".to_string(), "good".into()])
        .await;
    assert_eq!(run.code, 0, "{}", run.dump());
    let good = history(&env, "good");
    assert_eq!(good.len(), 1, "{good:?}");
    assert_ne!(good[0].result, RunResult::Failed, "{:?}", good[0]);
    let comparisons = saved_comparisons(&env);

    save(
        &env,
        &task("second", "Shop", &closed_url(), true, Notify::default()),
    );
    let run = env
        .run_async(vec!["--run-task".to_string(), "second".into()])
        .await;
    assert_eq!(run.code, 0, "{}", run.dump());
    let runs = history(&env, "second");
    assert_eq!(runs.len(), 1, "{runs:?}");
    assert_failed_record(&runs[0], true);
    assert_eq!(saved_comparisons(&env), comparisons);
    let log = std::fs::read_to_string(runs[0].log_path.clone().expect("Lauf-Log")).unwrap();
    assert!(log.contains(SKIPPED), "Vermerk im Lauf-Log fehlt: {log}");
    assert!(!log.contains("Crawl erfolgreich"), "{log}");
}

// Ein per robots.txt gesperrter Start ist kein Fehlschlag (CRAWL-128): Die Historie führt den Lauf nicht als `Failed`, Exit 0.
// spec: OUT-169
// spec: CRAWL-128
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn planner_history_does_not_show_failed_for_a_robots_block() {
    let dir = robots_fixture();
    let site = dir_site(dir.path()).await;
    let env = Env::new();
    for (id, auto_compare) in [("plain", false), ("compared", true)] {
        save(
            &env,
            &task(
                id,
                "Shop",
                &format!("{}/secret/index.html", site.base_url()),
                auto_compare,
                Notify::default(),
            ),
        );
        let run = env
            .run_async(vec!["--run-task".to_string(), id.into()])
            .await;
        assert_eq!(run.code, 0, "{id}: {}", run.dump());
        let runs = history(&env, id);
        assert_eq!(runs.len(), 1, "{id}: {runs:?}");
        assert!(
            matches!(
                runs[0].result,
                RunResult::Completed | RunResult::CompletedWithWarnings
            ),
            "{id}: {:?}",
            runs[0]
        );
        assert_eq!(runs[0].exit_code, Some(0), "{id}");
    }
}

// ---------------------------------------------------------------------------------------------------
// OUT-281 / OUT-152: Fehler-Mail der Planer-Aufgabe (Lauf im Prozess)

struct Planner {
    home: tempfile::TempDir,
    secrets: Arc<MemorySecretStore>,
}

impl Planner {
    fn new(mock: &SmtpMock) -> Planner {
        let planner = Planner {
            home: tempfile::tempdir().unwrap(),
            secrets: Arc::new(MemorySecretStore::new()),
        };
        // Die Aufgabenprüfung (OUT-168) verlangt den Schlüsselbund-Eintrag des Kontos; der Mock fragt keine Anmeldung ab.
        planner.secrets.set("mail/fixture", "unused");
        let mut settings = os_model::settings::AppSettings::default();
        settings.network.proxy.mode = os_model::ProxyMode::None;
        settings.save(&planner.ctx().paths.settings).unwrap();
        save_account(&planner.ctx().paths.settings, mock);
        planner
    }

    fn ctx(&self) -> RunCtx {
        RunCtx {
            paths: os_store::paths::app_paths_with_home(Some(self.home.path().as_os_str())),
            clock: Arc::new(SystemClock),
            secrets: self.secrets.clone(),
            run_id: None,
            manual: true,
            log_path: None,
            export_dir: self.home.path().join("exports"),
            json_progress: false,
            heartbeat: Duration::from_millis(200),
            poll: Duration::from_millis(50),
            timeout_scale: 1.0,
        }
    }

    async fn run(&self, task: Task, hooks: &dyn TaskHooks) -> TaskRun {
        TaskStore::in_settings(&self.ctx().paths.settings)
            .save(task.clone())
            .unwrap();
        run_in(
            &self.ctx(),
            &task.id,
            hooks,
            Signals::manual(CancelToken::new()),
            Reporter::for_task(false),
        )
        .await
    }

    fn comparison_hooks(&self, mock: &SmtpMock) -> ComparisonMailHooks {
        ComparisonMailHooks {
            crawls: self.ctx().paths.crawls,
            sender: Arc::new(
                os_sched::mail::completion::SmtpCompletionSender::loopback(
                    mock.addr.port(),
                    "sender@site.test",
                )
                .unwrap(),
            ),
        }
    }
}

fn notify(on_success: bool, on_failure: bool) -> Notify {
    Notify {
        on_success,
        on_failure,
        account: Some("fixture".into()),
        to: vec!["recipient@site.test".into()],
    }
}

fn failed_record(run: &TaskRun) -> &RunRecord {
    assert_eq!(run.exit, Exit::Ok, "{}", run.summary);
    run.record.as_ref().expect("Historieneintrag")
}

// Planer-Aufgabe ohne Auto-Vergleich: Der fehlgeschlagene Crawl erzeugt genau eine Fehler-Mail (`[FAILED]`, Ursache, Fehlerklasse,
// Adresse), nie eine Erfolgsmail – auch wenn „bei Erfolg“ an ist. Mit „bei Fehlschlag“ aus geht gar nichts hinaus.
// spec: OUT-281
// spec: OUT-169
// spec: CRAWL-128
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn planner_sends_exactly_one_failure_mail_for_a_failed_crawl() {
    for (on_success, on_failure, expected) in [
        (false, true, 1),
        (true, true, 1),
        (true, false, 0),
        (false, false, 0),
    ] {
        let mock = smtp().await;
        let planner = Planner::new(&mock);
        let url = closed_url();
        let run = planner
            .run(
                task(
                    "nightly",
                    "Shop",
                    &url,
                    false,
                    notify(on_success, on_failure),
                ),
                &DefaultHooks,
            )
            .await;
        assert_eq!(
            failed_record(&run).result,
            RunResult::Failed,
            "{}",
            run.summary
        );
        let mails = messages(&mock).await;
        assert_eq!(
            mails.len(),
            expected,
            "(Erfolg {on_success}, Fehlschlag {on_failure}): {mails:?}"
        );
        if let Some(m) = mails.first() {
            assert_failure_mail(m, url.trim_start_matches("http://").trim_end_matches('/'));
        }
    }
}

async fn planner_class_case(ignore_robots: bool) {
    let mock = smtp().await;
    let planner = Planner::new(&mock);
    let mut t = task("nightly", "Shop", &closed_url(), false, notify(false, true));
    if ignore_robots {
        t.crawl
            .overrides
            .insert("robots.mode".into(), "ignore".into());
    }
    planner.run(t, &DefaultHooks).await;
    let mails = messages(&mock).await;
    assert_eq!(mails.len(), 1, "{mails:?}");
    assert_names_class(&mails[0]);
}

// Die Fehler-Mail der Planer-Aufgabe nennt die Fehlerklasse der Ursache (Verbindung abgelehnt) – mit „robots.txt beachten“ und mit
// „robots.txt ignorieren“.
// spec: OUT-281
// spec: CRAWL-128
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn planner_failure_mail_names_the_error_class_with_robots_respected() {
    planner_class_case(false).await;
}

// spec: OUT-281
// spec: CRAWL-128
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn planner_failure_mail_names_the_error_class_with_robots_ignored() {
    planner_class_case(true).await;
}

// Planer-Aufgabe mit Auto-Vergleich (Abschlussmail mit Änderungstabelle): Der fehlgeschlagene Crawl wird nicht verglichen, die Mail
// folgt „bei Fehlschlag“ und enthält statt der Änderungstabelle den Vermerk „Auto-Vergleich übersprungen (Crawl fehlgeschlagen)“;
// Historie `Failed` mit Vermerk, nie „Crawl erfolgreich …“. Die Aussage gilt auch, wenn ein vorheriger Crawl existiert.
// spec: OUT-281
// spec: OUT-152
// spec: OUT-169
// spec: CRAWL-128
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn planner_with_auto_comparison_sends_the_failure_mail_with_the_note_instead_of_the_change_table(
) {
    let dir = robots_fixture();
    let site = dir_site(dir.path()).await;
    let mock = smtp().await;
    let planner = Planner::new(&mock);
    let hooks = planner.comparison_hooks(&mock);

    // Vorheriger erfolgreicher Crawl des Projekts, ohne Mail.
    let prev = planner
        .run(
            task(
                "prev",
                "Shop",
                &format!("{}/", site.base_url()),
                true,
                Notify::default(),
            ),
            &hooks,
        )
        .await;
    assert_ne!(
        failed_record(&prev).result,
        RunResult::Failed,
        "{}",
        prev.summary
    );
    let before = mock.messages().len();

    // „bei Erfolg“ an, „bei Fehlschlag“ aus: nichts geht hinaus.
    let url = closed_url();
    let run = planner
        .run(
            task("silent", "Shop", &url, true, notify(true, false)),
            &hooks,
        )
        .await;
    let record = failed_record(&run);
    assert_eq!(record.result, RunResult::Failed, "{record:?}");
    assert_eq!(messages(&mock).await.len(), before);

    // „bei Fehlschlag“ an: eine Fehler-Mail.
    let run = planner
        .run(task("loud", "Shop", &url, true, notify(true, true)), &hooks)
        .await;
    let record = failed_record(&run);
    assert_eq!(record.result, RunResult::Failed, "{record:?}");
    assert!(
        record.warnings.iter().any(|w| w == SKIPPED),
        "Vermerk in der Historie fehlt: {record:?}"
    );
    let message = record.message.as_deref().unwrap_or_default();
    assert!(message.contains("nicht erreichbar"), "{record:?}");
    assert!(
        !has_any(message, &["erfolgreich", "Auto-Vergleich"]),
        "{record:?}"
    );
    let mails = messages(&mock).await;
    assert_eq!(mails.len(), before + 1, "{mails:?}");
    let failure = mails.last().unwrap();
    assert_failure_mail(
        failure,
        url.trim_start_matches("http://").trim_end_matches('/'),
    );
    for (kind, parts) in [("Klartext", &failure.plain), ("HTML", &failure.html)] {
        let text = parts.join("\n");
        assert!(text.contains(SKIPPED), "{kind} ohne Vermerk: {text}");
        assert!(!text.contains("Change %"), "{kind} mit Tabelle: {text}");
        assert!(!text.contains("Crawl erfolgreich"), "{kind}: {text}");
    }
}

// Ein per robots.txt gesperrter Start ist kein Fehlschlag (CRAWL-128): Die Aufgabe meldet Erfolg (keine Fehler-Mail, kein `Failed`).
// spec: OUT-281
// spec: OUT-169
// spec: CRAWL-128
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn planner_reports_a_robots_block_as_success() {
    let dir = robots_fixture();
    let site = dir_site(dir.path()).await;
    let start = format!("{}/secret/index.html", site.base_url());
    let mock = smtp().await;
    let planner = Planner::new(&mock);
    let run = planner
        .run(
            task("blocked", "Shop", &start, false, notify(true, true)),
            &DefaultHooks,
        )
        .await;
    let record = failed_record(&run);
    assert_ne!(record.result, RunResult::Failed, "{record:?}");
    let mails = messages(&mock).await;
    assert_eq!(mails.len(), 1, "{mails:?}");
    assert!(
        !mails[0].subject.starts_with("[FAILED]"),
        "{}",
        mails[0].subject
    );
}
