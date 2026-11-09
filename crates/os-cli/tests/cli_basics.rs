//! Aufruf, Hilfe, Validierung vor dem Netz und Exit-Codes (OUT-180, OUT-181, OUT-188, OUT-195, OUT-203,
//! OUT-204, CRAWL-135).

mod common;

use std::path::Path;
use std::time::{Duration, Instant};

use common::{column, file_names, read_csv, resolve, site, Env};
fn assert_golden(path: impl AsRef<Path>, actual: &str) {
    let path = path.as_ref();
    let expected = std::fs::read_to_string(path).unwrap();
    assert_eq!(actual, expected, "{}", path.display());
}

/// Kein Warten auf Wiederholungen der robots.txt für den externen Link `localhost:8900` der Fixture.
const FAST: &str = "robots.retries=0";

fn golden(name: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/golden/out-cli")
        .join(name)
}

// spec: OUT-180
#[test]
fn the_binary_runs_without_a_gui_session_and_returns_an_exit_code() {
    let env = Env::new();
    let run = env.run(&["--list-crawls"]);
    assert_eq!(run.code, 0, "{}", run.dump());
    // Kein Fenster, keine Java-/Node-Laufzeit: ein einzelnes natives Programm.
    assert!(run.last_line().contains("Exit-Code 0"), "{}", run.dump());
}

// spec: OUT-181
#[test]
fn version_has_the_documented_format() {
    let env = Env::new();
    let run = env.run(&["--version"]);
    assert_eq!(run.code, 0, "{}", run.dump());
    let line = run.stdout.trim();
    let prefix = format!("openspider {} (", env!("CARGO_PKG_VERSION"));
    assert!(line.starts_with(&prefix), "{line}");
    let suffix = format!(", {}-{})", std::env::consts::OS, std::env::consts::ARCH);
    assert!(line.ends_with(&suffix), "{line}");
    // Optional build number first (UI-127, Nutzerentscheidung 09.11.2026): `(Build 2578, <hash>, …)`.
    let inner = &line[prefix.len()..line.len() - suffix.len()];
    let hash = match os_model::build_number::BUILD_NUMBER {
        Some(number) => inner
            .strip_prefix(&format!("Build {number}, "))
            .unwrap_or_else(|| panic!("build number missing: {line}")),
        None => inner,
    };
    assert!(!hash.is_empty() && !hash.contains(' '), "{line}");
}

// spec: OUT-181
#[test]
fn help_topics_list_every_name_one_per_line_and_match_the_golden() {
    let env = Env::new();
    let tabs = env.run(&["--help", "export-tabs"]);
    assert_eq!(tabs.code, 0, "{}", tabs.dump());
    assert_golden(golden("help_export_tabs.txt"), &tabs.stdout);

    // Vollständigkeit: genau ein Eintrag je Filter der Filterregistry (Anzahl der UI-Filter).
    let filters: usize = os_analyze::tabs::configured(&os_model::CrawlConfig::default())
        .iter()
        .map(|t| t.filters.len())
        .sum();
    assert_eq!(tabs.stdout.lines().count(), filters);
    assert!(tabs.stdout.lines().any(|l| l == "Internal:All"));

    // maschinenlesbar
    let json = env.run(&["--help", "export-tabs", "--json"]);
    let names: Vec<String> = serde_json::from_str(&json.stdout).unwrap();
    assert_eq!(names.join("\n") + "\n", tabs.stdout);

    for topic in ["bulk-export", "save-report"] {
        let run = env.run(&["--help", topic]);
        assert_eq!(run.code, 0, "{}", run.dump());
        assert!(!run.stdout.trim().is_empty(), "{topic}");
        assert!(run.stderr.is_empty(), "{topic}: {}", run.stderr);
    }
    let bad = env.run(&["--help", "gibtsnicht"]);
    assert_eq!(bad.code, 2);
}

// spec: OUT-181
#[test]
fn the_general_help_groups_the_flags_and_marks_the_os_extensions() {
    let env = Env::new();
    let run = env.run(&["--help"]);
    assert_eq!(run.code, 0, "{}", run.dump());
    for heading in [
        "Quelle:",
        "Konfiguration:",
        "Vergleich:",
        "APIs:",
        "Exporte:",
        "OS-Erweiterungen:",
    ] {
        assert!(
            run.stdout.contains(heading),
            "{heading} fehlt:\n{}",
            run.stdout
        );
    }
    let os_part = run.stdout.split("OS-Erweiterungen:").nth(1).unwrap();
    for flag in [
        "--resolve",
        "--set",
        "--max-urls",
        "--resume",
        "--crawl-sitemap",
        "--no-store",
        "--json-progress",
        "--fail-on-start-error",
        "--strict",
    ] {
        assert!(
            os_part.contains(flag),
            "{flag} nicht unter OS-Erweiterungen"
        );
    }
}

// spec: OUT-203 [P1] [M2]
#[test]
fn list_exports_and_list_reports_equal_the_help_topics_and_need_no_network() {
    let env = Env::new();
    let exports = env.run(&["--list-exports"]);
    let tabs = env.run(&["--help", "export-tabs"]);
    let bulk = env.run(&["--help", "bulk-export"]);
    assert_eq!(exports.code, 0, "{}", exports.dump());
    let combined = format!("{}{}", tabs.stdout, bulk.stdout);
    let mut expected: Vec<_> = combined.lines().collect();
    expected.sort();
    expected.dedup();
    assert_eq!(exports.stdout, expected.join("\n") + "\n");
    assert_golden(golden("list_exports.txt"), &exports.stdout);

    let reports = env.run(&["--list-reports"]);
    let help = env.run(&["--help", "save-report"]);
    assert_eq!(reports.code, 0, "{}", reports.dump());
    assert_eq!(reports.stdout, help.stdout);
    assert_golden(golden("list_reports.txt"), &reports.stdout);
    assert!(reports.stdout.lines().any(|l| l == "Crawl Overview"));

    let json = env.run(&["--list-exports", "--json"]);
    let v: serde_json::Value = serde_json::from_str(&json.stdout).unwrap();
    assert!(v["export-tabs"].as_array().unwrap().len() > 10);
    assert!(v["bulk-export"]
        .as_array()
        .unwrap()
        .iter()
        .any(|n| n == "Links:All Inlinks"));
}

// spec: OUT-203, CRAWL-135
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn invalid_names_are_rejected_before_any_request_in_under_a_second() {
    let site = site("out-basic-site").await;
    let env = Env::new();
    let resolve = resolve(&site);
    let base = |extra: &[&str]| -> Vec<String> {
        let mut v = vec![
            "--crawl".to_string(),
            "http://site.test/".to_string(),
            "--resolve".to_string(),
            resolve.clone(),
            "--output-folder".to_string(),
            env.out().to_string_lossy().into_owned(),
        ];
        v.extend(extra.iter().map(|s| s.to_string()));
        v
    };

    // Aufwärmen: der erste Start eines frisch gebauten Programms kann auf manchen Systemen länger dauern.
    assert_eq!(env.run(&["--version"]).code, 0);
    let started = Instant::now();
    let run = env.run_async(base(&["--export-tabs", "Internal:Al"])).await;
    assert!(
        started.elapsed() < Duration::from_secs(1),
        "{:?}",
        started.elapsed()
    );
    assert_eq!(run.code, 2, "{}", run.dump());
    assert!(
        run.stderr
            .contains("unknown export 'Internal:Al' – did you mean 'Internal:All'?"),
        "{}",
        run.dump()
    );

    // Unbekannte Option mit Vorschlag
    let run = env.run_async(vec!["--crawl-lst", "x"]).await;
    assert_eq!(run.code, 2);
    assert!(
        run.stderr.contains("meinten Sie `--crawl-list`?"),
        "{}",
        run.dump()
    );

    // Unbekannter Report ohne ähnlichen Namen: Meldung plus vollständige Liste (wie --list-reports)
    let run = env
        .run_async(base(&["--save-report", "Gibtsnicht Gibtsnicht Gibtsnicht"]))
        .await;
    assert_eq!(run.code, 2, "{}", run.dump());
    let list = env.run(&["--list-reports"]).stdout;
    for name in list.lines() {
        assert!(
            run.stderr.lines().any(|l| l == name),
            "{name} fehlt in der Fehlerliste:\n{}",
            run.stderr
        );
    }
    // Bulk-Exporte: nur der betroffene Katalog
    let run = env
        .run_async(base(&["--bulk-export", "Gibtsnicht Gibtsnicht Gibtsnicht"]))
        .await;
    assert_eq!(run.code, 2, "{}", run.dump());
    assert!(run.stderr.lines().any(|l| l == "Links:All Inlinks"));

    // Formate
    for (fmt, needle) in [
        ("gsheet", "--google-drive-account"),
        ("odt", "csv, xls, xlsx, gsheet"),
    ] {
        let run = env.run_async(base(&["--export-format", fmt])).await;
        assert_eq!(run.code, 2, "{fmt}: {}", run.dump());
        assert!(run.stderr.contains(needle), "{fmt}: {}", run.dump());
    }

    // OUT-201 / INTEG-017: eine optionale API ohne Schlüssel warnt; der Crawl läuft.
    let run = env.run_async(base(&["--use-pagespeed"])).await;
    assert_eq!(run.code, 0, "{}", run.dump());

    assert!(
        site.total_requests() > 0,
        "der gültige Aufruf mit optionaler API muss crawlen"
    );
    assert!(!env.out().exists(), "kein Ausgabeordner bei Aufruffehlern");
}

// spec: OUT-195
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn csv_is_the_default_and_xlsx_is_supported() {
    let site = site("out-basic-site").await;
    let env = Env::new();
    let common = |fmt: Option<&str>, out: &str| -> Vec<String> {
        let mut v: Vec<String> = [
            "--crawl",
            "http://site.test/",
            "--resolve",
            &resolve(&site),
            "--set",
            FAST,
            "--export-tabs",
            "Internal:All",
            "--output-folder",
            out,
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        if let Some(f) = fmt {
            v.push("--export-format".into());
            v.push(f.into());
        }
        v
    };
    let csv_out = env.work.path().join("csv");
    let run = env
        .run_async(common(None, &csv_out.to_string_lossy()))
        .await;
    assert_eq!(run.code, 0, "{}", run.dump());
    assert_eq!(file_names(&csv_out), ["internal_all.csv"]);

    let xlsx_out = env.work.path().join("xlsx");
    let run = env
        .run_async(common(Some("XLSX"), &xlsx_out.to_string_lossy()))
        .await;
    assert_eq!(run.code, 0, "{}", run.dump());
    assert_eq!(file_names(&xlsx_out), ["internal_all.xlsx"]);

    // Gleicher Inhalt in beiden Formaten (Spalte Address).
    use calamine::{open_workbook, Reader, Xlsx};
    let mut book: Xlsx<_> = open_workbook(xlsx_out.join("internal_all.xlsx")).unwrap();
    let sheet = book.worksheet_range_at(0).unwrap().unwrap();
    let rows = read_csv(&csv_out.join("internal_all.csv"));
    assert_eq!(sheet.rows().count(), rows.len());
    let addr_idx = rows[0].iter().position(|h| h == "Address").unwrap();
    let xlsx_addrs: Vec<String> = sheet
        .rows()
        .skip(1)
        .map(|r| r[addr_idx].to_string())
        .collect();
    // Die Reihenfolge hängt von der Crawl-Reihenfolge ab (zwei getrennte Läufe): als Menge vergleichen.
    let mut xlsx_sorted = xlsx_addrs;
    xlsx_sorted.sort();
    let mut csv_sorted = column(&rows, "Address");
    csv_sorted.sort();
    assert_eq!(xlsx_sorted, csv_sorted);
}

// spec: OUT-188
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn headless_changes_nothing() {
    let site = site("out-basic-site").await;
    let mut results = Vec::new();
    for headless in [false, true] {
        let env = Env::new();
        let mut args: Vec<String> = [
            "--crawl",
            "http://site.test/",
            "--resolve",
            &resolve(&site),
            "--set",
            FAST,
            "--export-tabs",
            "Internal:All",
            "--output-folder",
            &env.out().to_string_lossy(),
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        if headless {
            args.push("--headless".into());
        }
        let run = env.run_async(args).await;
        assert_eq!(run.code, 0, "{}", run.dump());
        let rows = read_csv(&env.out().join("internal_all.csv"));
        let mut cols: Vec<(String, String, String)> = column(&rows, "Address")
            .into_iter()
            .zip(column(&rows, "Status Code"))
            .zip(column(&rows, "Crawl Depth"))
            .map(|((a, s), d)| (a, s, d))
            .collect();
        cols.sort();
        results.push(cols);
    }
    assert_eq!(results[0], results[1]);
    assert!(results[0].len() >= 10);
}

// spec: OUT-204
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn exit_code_0_with_a_summary_as_the_last_stderr_line() {
    let site = site("out-basic-site").await;
    let env = Env::new();
    let run = env
        .run_async(vec![
            "--crawl".to_string(),
            "http://site.test/".into(),
            "--resolve".into(),
            resolve(&site),
            "--set".into(),
            FAST.into(),
        ])
        .await;
    assert_eq!(run.code, 0, "{}", run.dump());
    assert!(
        run.stdout.is_empty(),
        "stdout nur für Nutzdaten: {}",
        run.stdout
    );
    let last = run.last_line();
    assert!(last.starts_with("openspider: "), "{last}");
    assert!(last.ends_with("(Exit-Code 0)"), "{last}");
    assert!(!run.stderr.contains('\u{1b}'), "keine ANSI-Sequenzen");
}

// spec: OUT-204
#[test]
fn exit_code_2_for_usage_errors() {
    let env = Env::new();
    let run = env.run(&["--bulk-export", "Links:All Inlinks"]);
    assert_eq!(run.code, 2, "{}", run.dump());
    let last = run.last_line();
    assert!(
        last.starts_with("openspider: ") && last.ends_with("(Exit-Code 2)"),
        "{last}"
    );
    assert!(run.stdout.is_empty());
}

// spec: OUT-204
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn exit_code_3_when_the_crawl_cannot_be_written() {
    let site = site("out-basic-site").await;
    let env = Env::new();
    // `crawls` ist eine Datei, der Ordner lässt sich nicht anlegen (plattformneutral „nicht schreibbar“).
    std::fs::write(env.home.path().join("crawls"), "x").unwrap();
    let run = env
        .run_async(vec![
            "--crawl".to_string(),
            "http://site.test/".into(),
            "--resolve".into(),
            resolve(&site),
        ])
        .await;
    assert_eq!(run.code, 3, "{}", run.dump());
    assert!(run.last_line().ends_with("(Exit-Code 3)"), "{}", run.dump());
    assert_eq!(site.total_requests(), 0);
}

// spec: OUT-204, OUT-182
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_unreachable_start_url_is_a_valid_result_unless_fail_on_start_error() {
    // Port, auf dem niemand lauscht.
    let closed = {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        l.local_addr().unwrap().port()
    };
    let url = format!("http://127.0.0.1:{closed}/");
    let fast = |extra: &[&str]| -> Vec<String> {
        let mut v: Vec<String> = ["--crawl", &url, "--set", "robots.retries=0"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        v.extend(extra.iter().map(|s| s.to_string()));
        v
    };
    let env = Env::new();
    let run = env
        .run_async(fast(&[
            "--export-tabs",
            "Internal:All",
            "--output-folder",
            &env.out().to_string_lossy(),
        ]))
        .await;
    assert_eq!(run.code, 0, "{}", run.dump());
    assert!(run.stderr.contains("nicht erreichbar"), "{}", run.dump());
    // Genau eine Seite (No Response, Statuscode 0); robots.txt zählt als eigener Abruf.
    let rows = read_csv(&env.out().join("internal_all.csv"));
    let pages: Vec<(String, String)> = column(&rows, "Address")
        .into_iter()
        .zip(column(&rows, "Status Code"))
        .filter(|(a, _)| !a.ends_with("/robots.txt"))
        .collect();
    assert_eq!(pages, [(url.clone(), "0".to_string())]);

    // --strict: die Warnung wird zum Fehler (Exit-Code 4)
    let env2 = Env::new();
    let strict = env2.run_async(fast(&["--strict"])).await;
    assert_eq!(strict.code, 4, "{}", strict.dump());

    // --fail-on-start-error: Exit-Code 3
    let env3 = Env::new();
    let fail = env3.run_async(fast(&["--fail-on-start-error"])).await;
    assert_eq!(fail.code, 3, "{}", fail.dump());
    assert!(fail.last_line().ends_with("(Exit-Code 3)"));
}

// CRAWL-128 ohne `--fail-on-start-error`: weiterhin Exit 0 mit Exporten (OUT-182), aber in der Verwaltung `Failed`.
// spec: CRAWL-128
// spec: OUT-182
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_unreachable_start_url_is_listed_as_failed_and_stays_a_valid_result() {
    let closed = {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        l.local_addr().unwrap().port()
    };
    let env = Env::new();
    let run = env
        .run_async(vec![
            "--crawl".to_string(),
            format!("http://127.0.0.1:{closed}/"),
            "--set".into(),
            "robots.retries=0".into(),
            "--export-tabs".into(),
            "Internal:All".into(),
            "--output-folder".into(),
            env.out().to_string_lossy().into_owned(),
        ])
        .await;
    assert_eq!(run.code, 0, "{}", run.dump());
    assert!(env.out().join("internal_all.csv").is_file());
    let list = env.run(&["--list-crawls", "--json"]);
    let rows: serde_json::Value = serde_json::from_str(&list.stdout).unwrap();
    assert_eq!(rows[0]["status"], "Failed", "{rows}");
    let file =
        os_store::CrawlFile::open(&env.crawl_files()[0], os_store::OpenMode::ReadOnly).unwrap();
    let completeness: String = file
        .conn()
        .query_row(
            "SELECT value FROM q_manifest WHERE key = 'completeness'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(completeness, "failed");
    // „Fortsetzen“ ist für „Fehlgeschlagen“ ausgeschlossen (CRAWL-128).
    let id = rows[0]["id"].as_str().unwrap();
    let resume = env.run(&["--load-crawl", id, "--resume"]);
    assert_eq!(resume.code, 1, "{}", resume.dump());
}

// spec: OUT-204
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn exit_code_4_when_an_export_fails_but_the_crawl_succeeded() {
    let site = site("out-basic-site").await;
    let env = Env::new();
    // Am Zielnamen steht ein Ordner: Die Datei lässt sich auch mit --overwrite nicht schreiben.
    std::fs::create_dir_all(env.out().join("internal_all.csv")).unwrap();
    let run = env
        .run_async(vec![
            "--crawl".to_string(),
            "http://site.test/".into(),
            "--resolve".into(),
            resolve(&site),
            "--set".into(),
            FAST.into(),
            "--export-tabs".into(),
            "Internal:All,Response Codes:Client Error (4xx)".into(),
            "--output-folder".into(),
            env.out().to_string_lossy().into_owned(),
            "--overwrite".into(),
        ])
        .await;
    assert_eq!(run.code, 4, "{}", run.dump());
    assert!(run.last_line().ends_with("(Exit-Code 4)"), "{}", run.dump());
    // Der zweite Export lief trotzdem.
    assert!(env
        .out()
        .join("response_codes_client_error_4xx.csv")
        .is_file());
    // Der Crawl liegt vollständig in der Crawl-Verwaltung.
    assert_eq!(env.crawl_files().len(), 1);
}

// spec: OUT-180
#[test]
fn the_mcp_subcommand_is_passed_through_to_os_mcp() {
    let env = Env::new();
    let run = env.run(&["mcp", "--help"]);
    assert_eq!(run.code, 0, "{}", run.dump());
    assert!(run.stdout.contains("--read-only"), "{}", run.dump());
}
