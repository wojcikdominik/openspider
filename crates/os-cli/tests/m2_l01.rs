//! End-to-end checks for M2-L01 against local fixtures only.
mod common;
use calamine::{Reader, Xls};
use common::{resolve, site, Env};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/out-m2-cli")
        .join(name)
}
fn crawl(env: &Env, site: &os_testsite::TestSite, extra: &[&str]) -> Vec<String> {
    let mut args = vec![
        "--crawl".into(),
        "http://site.test/".into(),
        "--resolve".into(),
        resolve(site),
        "--set".into(),
        "threads=1".into(),
        "--set".into(),
        "robots.retries=0".into(),
        "--output-folder".into(),
        env.out().to_string_lossy().into_owned(),
    ];
    args.extend(extra.iter().map(|s| s.to_string()));
    args
}

// spec: OUT-198
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn disconnected_google_account_fails_before_crawling() {
    let server = site("out-basic-site").await;
    let env = Env::new();
    let run = env
        .run_async(crawl(
            &env,
            &server,
            &[
                "--export-tabs",
                "Internal:All",
                "--export-format",
                "gsheet",
                "--google-drive-account",
                "missing@example.test",
            ],
        ))
        .await;
    assert_eq!(run.code, 2, "{}", run.dump());
    assert!(run.stderr.contains("missing@example.test") && run.stderr.contains("verbinden"));
    assert_eq!(server.total_requests(), 0);
}

// spec: OUT-133
// spec: OUT-197
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cli_sitemaps_use_the_profile_and_timestamped_directory() {
    let server = site("out-basic-site").await;
    let env = Env::new();
    let run = env
        .run_async(crawl(
            &env,
            &server,
            &[
                "--create-sitemap",
                "--create-images-sitemap",
                "--timestamped-output",
                "--save-crawl",
            ],
        ))
        .await;
    assert_eq!(run.code, 0, "{}", run.dump());
    let output = std::fs::read_dir(env.out())
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let file =
        os_store::CrawlFile::open(&output.join("crawl.ospider"), os_store::OpenMode::ReadOnly)
            .unwrap();
    let expected = tempfile::tempdir().unwrap();
    os_export::sitemap_gen::generate_sitemap_configured_with(
        &file,
        &Default::default(),
        &file.config().unwrap(),
        None,
        &os_model::SystemClock,
        expected.path(),
    )
    .unwrap();
    assert_eq!(
        std::fs::read(output.join("sitemap.xml")).unwrap(),
        std::fs::read(expected.path().join("sitemap.xml")).unwrap()
    );
    assert!(common::file_names(&output)
        .iter()
        .any(|s| s.contains("image") && s.ends_with(".xml")));
}

// spec: OUT-196
// spec: OUT-075 [M2]
// spec: OUT-012
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn preset_and_explicit_exports_deduplicate_and_skip_empty_in_one_workbook() {
    let server = site("out-basic-site").await;
    let env = Env::new();
    let run = env
        .run_async(crawl(
            &env,
            &server,
            &[
                "--export-preset",
                fixture("exports.ospreset").to_str().unwrap(),
                "--export-tabs",
                "Internal:All",
                "--export-format",
                "xlsx",
                "--single-workbook",
                "--skip-empty-exports",
                "--column-headers",
                "canonical",
            ],
        ))
        .await;
    assert_eq!(run.code, 0, "{}", run.dump());
    let files = common::file_names(&env.out());
    assert_eq!(files.len(), 1, "{files:?}");
    let mut book: calamine::Xlsx<_> = calamine::open_workbook(env.out().join(&files[0])).unwrap();
    assert_eq!(book.sheet_names().len(), 1);
    let sheet = book.worksheet_range_at(0).unwrap().unwrap();
    assert!(sheet.rows().next().unwrap().iter().any(|s| s == "Address"));
    assert!(sheet.rows().count() > 2);
}

// spec: OUT-196
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ui_headers_and_segment_selection_are_accepted_and_invalid_segments_never_crawl() {
    let server = site("out-basic-site").await;
    let env = Env::new();
    let run = env
        .run_async(crawl(
            &env,
            &server,
            &[
                "--export-tabs",
                "Internal:All",
                "--segment",
                "unknown",
                "--column-headers",
                "ui",
            ],
        ))
        .await;
    assert_eq!(run.code, 2, "{}", run.dump());
    assert_eq!(server.total_requests(), 0);
    let run = env
        .run_async(crawl(
            &env,
            &server,
            &[
                "--export-tabs",
                "Internal:All",
                "--segment",
                "all",
                "--column-headers",
                "ui",
            ],
        ))
        .await;
    assert_eq!(run.code, 0, "{}", run.dump());
    assert!(common::read_csv(&env.out().join("internal_all.csv"))[0]
        .iter()
        .any(|h| h == "Address"));
}

// spec: OUT-195 [M2]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn xls_is_a_readable_biff8_file_and_unconnected_sheets_fail_before_crawling() {
    let server = site("out-basic-site").await;
    let env = Env::new();
    let run = env
        .run_async(crawl(
            &env,
            &server,
            &["--export-tabs", "Internal:All", "--export-format", "gsheet"],
        ))
        .await;
    assert_eq!(run.code, 2, "{}", run.dump());
    assert!(run.stderr.contains("--google-drive-account") && run.stderr.contains("verbinden"));
    assert_eq!(server.total_requests(), 0);
    let run = env
        .run_async(crawl(
            &env,
            &server,
            &["--export-tabs", "Internal:All", "--export-format", "xls"],
        ))
        .await;
    assert_eq!(run.code, 0, "{}", run.dump());
    let mut book: Xls<_> = calamine::open_workbook(env.out().join("internal_all.xls")).unwrap();
    assert!(book.worksheet_range_at(0).unwrap().unwrap().rows().count() > 2);
    assert_eq!(common::file_names(&env.out()), ["internal_all.xls"]);
}

// spec: OUT-205
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn progress_alias_emits_complete_phase_objects_and_quiet_suppresses_success_output() {
    let server = site("out-basic-site").await;
    let env = Env::new();
    let run = env
        .run_async(crawl(
            &env,
            &server,
            &["--progress", "json", "--export-tabs", "Internal:All"],
        ))
        .await;
    assert_eq!(run.code, 0, "{}", run.dump());
    assert!(run.stdout.is_empty() && !run.stderr.contains('\u{1b}'));
    let lines: Vec<serde_json::Value> = run
        .stderr
        .lines()
        .filter(|s| s.starts_with('{'))
        .map(|s| serde_json::from_str(s).unwrap())
        .collect();
    for phase in ["validate", "crawl", "analysis", "export", "notify", "done"] {
        let line = lines
            .iter()
            .find(|v| v["phase"] == phase)
            .unwrap_or_else(|| panic!("{phase}: {}", run.dump()));
        for key in ["crawled", "queued", "errors", "elapsed_s", "eta_s"] {
            assert!(line.get(key).is_some(), "{line}");
        }
    }
    let env = Env::new();
    let run = env
        .run_async(crawl(&env, &server, &["--quiet", "--json-progress"]))
        .await;
    assert_eq!(run.code, 0, "{}", run.dump());
    assert!(
        run.stdout.is_empty() && run.stderr.is_empty(),
        "{}",
        run.dump()
    );
}

// spec: OUT-211
// spec: OUT-203 [P1]
#[test]
fn catalogs_are_sorted_complete_and_unknown_names_link_their_listing() {
    let env = Env::new();
    for (list, help) in [
        ("--list-reports", "save-report"),
        ("--list-exports", "export-tabs"),
    ] {
        let run = env.run(&[list]);
        assert_eq!(run.code, 0);
        let entries: Vec<_> = run.stdout.lines().collect();
        let mut sorted = entries.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(entries, sorted);
        let run = env.run(&[
            "--crawl",
            "http://site.test/",
            if help == "save-report" {
                "--save-report"
            } else {
                "--export-tabs"
            },
            "Gibtsnicht",
        ]);
        assert_eq!(run.code, 2);
        assert!(run.stderr.contains(list), "{}", run.dump());
    }
    let run = env.run(&["--help", "export-custom-summary", "--json"]);
    let metrics: Vec<String> = serde_json::from_str(&run.stdout).unwrap();
    let cfg = os_model::CrawlConfig::default();
    for metric in os_export::timeseries::catalog(&os_analyze::tabs::configured(&cfg)) {
        // Ohne Schalter „KI (Vorschau)“ (Standard, Plan M4 E1, M4-F02) genau ohne die Kennzahlen der KI-Tabs.
        let shown = os_cli::registry::shows_name(os_ai::PreviewSwitch::OFF, &metric.name);
        assert_eq!(metrics.contains(&metric.name), shown, "{}", metric.name);
    }
}

// spec: OUT-207
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn missing_chromium_is_an_actionable_usage_error_before_requests() {
    let server = site("out-basic-site").await;
    let env = Env::new();
    let run = env
        .run_async(crawl(
            &env,
            &server,
            &[
                "--set",
                "rendering.mode=javascript",
                "--set",
                "rendering.chromium_path=/openspider-fixture/missing-chromium",
            ],
        ))
        .await;
    assert_eq!(run.code, 2, "{}", run.dump());
    assert!(run.stderr.contains("openspider --install-browser"));
    assert_eq!(server.total_requests(), 0);
    assert!(env.crawl_files().is_empty());
}

// spec: OUT-187
// spec: OUT-223
// spec: CRAWL-121
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn encrypted_basic_and_form_profiles_authenticate_without_persisting_secrets() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}/", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        loop {
            let (mut stream, _) = listener.accept().await.unwrap();
            tokio::spawn(async move {
                let mut bytes = vec![0; 8192];
                let n = stream.read(&mut bytes).await.unwrap();
                let request = String::from_utf8_lossy(&bytes[..n]).to_lowercase();
                let authenticated = request
                    .contains("authorization: basic zml4dhvyztpmaxh0dxjllxnly3jlda==")
                    || request.contains("cookie: session=fixture-cookie");
                let status = if request.contains(" /robots.txt ") {
                    "404 Not Found"
                } else if authenticated {
                    "200 OK"
                } else {
                    "401 Unauthorized"
                };
                let body="<html><head><title>Protected fixture</title></head><body>Members</body></html>";
                stream.write_all(format!("HTTP/1.1 {status}\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap();
            });
        }
    });
    let env = Env::new();
    let mut profile: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(fixture("profile.json")).unwrap()).unwrap();
    profile["profile"]["host"] = serde_json::json!(base);
    let file = env.work.path().join("fixture.osauth");
    let pass = "fixture-passphrase";
    let seal = |value: &serde_json::Value| {
        os_store::auth_profiles::seal(value, os_store::auth_profiles::Protection::Passphrase(pass))
            .unwrap()
    };
    std::fs::write(&file, seal(&profile)).unwrap();
    let execute = |profile: bool, password: Option<&str>, name: &str| {
        let mut cmd = env.command();
        cmd.args([
            "--crawl",
            &base,
            "--set",
            "robots.retries=0",
            "--export-tabs",
            "Internal:All",
            "--output-folder",
        ])
        .arg(env.work.path().join(name));
        if profile {
            cmd.arg("--auth-config").arg(&file);
        }
        if let Some(password) = password {
            cmd.env("OPENSPIDER_AUTH_PASSPHRASE", password);
        }
        cmd
    };
    let run = common::Run::from_output(
        tokio::task::spawn_blocking({
            let mut cmd = execute(false, None, "unauth");
            move || cmd.output().unwrap()
        })
        .await
        .unwrap(),
    );
    assert_eq!(run.code, 0, "{}", run.dump());
    assert_eq!(
        common::column(
            &common::read_csv(&env.work.path().join("unauth/internal_all.csv")),
            "Status Code"
        ),
        ["401"]
    );
    let run = common::Run::from_output(
        tokio::task::spawn_blocking({
            let mut cmd = execute(true, Some("wrong"), "wrong");
            move || cmd.output().unwrap()
        })
        .await
        .unwrap(),
    );
    assert_eq!(run.code, 2, "{}", run.dump());
    let run = common::Run::from_output(
        tokio::task::spawn_blocking({
            let mut cmd = execute(true, Some(pass), "basic");
            move || cmd.output().unwrap()
        })
        .await
        .unwrap(),
    );
    assert_eq!(run.code, 0, "{}", run.dump());
    assert_eq!(
        common::column(
            &common::read_csv(&env.work.path().join("basic/internal_all.csv")),
            "Status Code"
        ),
        ["200"]
    );
    profile["profile"]["kind"] = serde_json::json!("form");
    profile["profile"]["secret"]=serde_json::json!(serde_json::json!({"cookies":[{"name":"session","value":"fixture-cookie","path":"/","expires":-1},{"name":"expired","value":"old","expires":1}]}).to_string());
    std::fs::write(&file, seal(&profile)).unwrap();
    let run = common::Run::from_output(
        tokio::task::spawn_blocking({
            let mut cmd = execute(true, Some(pass), "form");
            move || cmd.output().unwrap()
        })
        .await
        .unwrap(),
    );
    assert_eq!(run.code, 0, "{}", run.dump());
    assert!(
        run.stderr.contains("authentication cookies expired"),
        "{}",
        run.dump()
    );
    assert!(!run.stderr.contains("fixture-cookie") && !run.stderr.contains("fixture-secret"));
    assert_eq!(
        common::column(
            &common::read_csv(&env.work.path().join("form/internal_all.csv")),
            "Status Code"
        ),
        ["200"]
    );
    for path in env.crawl_files() {
        let file = os_store::CrawlFile::open(&path, os_store::OpenMode::ReadOnly).unwrap();
        let config = serde_json::to_string(&file.config().unwrap()).unwrap();
        assert!(!config.contains("fixture-secret") && !config.contains("fixture-cookie"));
    }
    server.abort();
}

// spec: INTEG-019
// spec: OUT-196
// spec: OUT-195 [M2]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn connected_sheets_streams_to_the_offline_mock_with_a_shared_workbook() {
    use os_integrations::{
        credentials::{Credential, Credentials},
        drive::{DriveClient, SCOPE},
        ApiCtx,
    };
    use os_testsite::mocks::{drive::DriveMock, MockServer};
    let mock = DriveMock::default();
    let server = MockServer::start(vec![Arc::new(mock.clone())])
        .await
        .unwrap();
    let secrets = Arc::new(Credentials::new(Arc::new(
        os_fetch::MemorySecretStore::new(),
    )));
    secrets
        .put(
            os_model::ApiKind::Drive,
            Credential {
                account_id: "fixture".into(),
                label: "Fixture".into(),
                api_key: None,
                access_token: Some("fixture-token".into()),
                refresh_token: None,
                expires_at: None,
                client_secret: None,
                scopes: vec![SCOPE.into()],
            },
            true,
        )
        .unwrap();
    let mut cfg = os_model::CrawlConfig::default();
    cfg.api.drive.base_url = Some(server.base_url("drive"));
    let ctx = ApiCtx {
        cfg,
        secrets,
        network: Default::default(),
        allow_api_override: true,
        quota_ledger: None,
    };
    let client = DriveClient::new(&ctx, "fixture", os_model::Timestamp::UNIX_EPOCH).unwrap();
    let rep = os_cli::reporter::Reporter::new(false);
    assert_eq!(
        os_cli::google::upload(
            &client,
            "Project",
            "Crawl",
            &[fixture("legacy.csv")],
            true,
            &os_model::CancelToken::new(),
            &rep
        )
        .await
        .unwrap(),
        1
    );
    let files = mock.files();
    let sheet = files.iter().find(|f| f.name == "Crawl_export").unwrap();
    assert_eq!(sheet.sheets.len(), 1);
    assert!(sheet.sheets[0].rows[1][2].is_number());
    assert!(mock
        .requests()
        .iter()
        .filter(|r| r.method == "PUT")
        .all(|r| r.query.get("valueInputOption").is_some_and(|v| v == "RAW")));
    assert!(mock
        .requests()
        .iter()
        .any(|r| r.body.to_string().contains("日本語")));

    let localized = tempfile::tempdir().unwrap();
    let path = localized.path().join("localized.csv");
    std::fs::write(
        &path,
        std::fs::read_to_string(fixture("legacy.csv"))
            .unwrap()
            .replacen(
                "Address,Title 1,Status Code",
                "Adresse,Titel 1,Statuscode",
                1,
            ),
    )
    .unwrap();
    os_cli::google::upload(
        &client,
        "Project",
        "German",
        &[path],
        true,
        &os_model::CancelToken::new(),
        &rep,
    )
    .await
    .unwrap();
    let files = mock.files();
    let sheet = files.iter().find(|f| f.name == "German_export").unwrap();
    assert_eq!(sheet.sheets[0].rows[0][0], "Adresse");
    assert!(
        sheet.sheets[0].rows[1][2].is_number(),
        "localized headers preserve numeric cells"
    );
}

// spec: OUT-196
// spec: OUT-012
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn selected_segments_and_skip_empty_share_the_saved_assignment() {
    let server = site("out-basic-site").await;
    let env = Env::new();
    let run = env
        .run_async(crawl(
            &env,
            &server,
            &[
                "--config",
                fixture("segments.osconfig").to_str().unwrap(),
                "--segment",
                "About",
                "--skip-empty-exports",
                "--export-tabs",
                "Internal:All,Page Titles:Missing",
            ],
        ))
        .await;
    assert_eq!(run.code, 0, "{}", run.dump());
    assert_eq!(
        common::file_names(&env.out()),
        ["internal_all_segment_about.csv"]
    );
    let rows = common::read_csv(&env.out().join("internal_all_segment_about.csv"));
    assert_eq!(rows.len(), 2);
    assert!(common::column(&rows, "Address")[0].ends_with("/about.html"));
}

// spec: OUT-196
// spec: OUT-075 [M2]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn single_workbook_and_all_segments_produce_one_typed_workbook_per_scope() {
    let server = site("out-basic-site").await;
    let env = Env::new();
    let run = env
        .run_async(crawl(
            &env,
            &server,
            &[
                "--config",
                fixture("segments.osconfig").to_str().unwrap(),
                "--segment",
                "all",
                "--single-workbook",
                "--skip-empty-exports",
                "--export-format",
                "xlsx",
                "--export-tabs",
                "Internal:All,Page Titles:Missing",
            ],
        ))
        .await;
    assert_eq!(run.code, 0, "{}", run.dump());
    let files = common::file_names(&env.out());
    assert_eq!(files.len(), 2, "{files:?}");
    let path = files
        .iter()
        .find(|name| name.ends_with("_segment_about.xlsx"))
        .unwrap();
    let mut book: calamine::Xlsx<_> = calamine::open_workbook(env.out().join(path)).unwrap();
    assert_eq!(book.sheet_names().len(), 1);
    let range = book.worksheet_range_at(0).unwrap().unwrap();
    assert_eq!(range.rows().count(), 2);
    let status = range
        .rows()
        .next()
        .unwrap()
        .iter()
        .position(|v| v == "Status Code")
        .unwrap();
    assert!(matches!(
        range.get_value((1, status as u32)),
        Some(calamine::Data::Int(200) | calamine::Data::Float(200.0))
    ));
}

// spec: OUT-196
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn column_header_modes_reach_csv_xlsx_batch_segments_and_xls() {
    let server = site("out-basic-site").await;
    for (format, batch, segment) in [
        ("csv", false, false),
        ("xlsx", false, false),
        ("xlsx", true, false),
        ("xlsx", true, true),
        ("xls", false, false),
    ] {
        for (mode, expected_address, expected_status) in [
            ("canonical", "Address", "Status Code"),
            ("ui", "Adresse", "Statuscode"),
        ] {
            let env = Env::new();
            std::fs::copy(
                fixture("settings-de.toml"),
                env.home.path().join("settings.toml"),
            )
            .unwrap();
            let mut extra = vec![
                "--export-tabs",
                "Internal:All",
                "--export-format",
                format,
                "--column-headers",
                mode,
            ];
            if batch {
                extra.push("--single-workbook");
            }
            if segment {
                extra.extend(["--segment", "all"]);
            }
            let run = env.run_async(crawl(&env, &server, &extra)).await;
            assert_eq!(run.code, 0, "{}", run.dump());
            let output = std::fs::read_dir(env.out())
                .unwrap()
                .map(|p| p.unwrap().path())
                .find(|p| p.extension().is_some_and(|s| s == format))
                .unwrap();
            let header = match format {
                "csv" => common::read_csv(&output)[0].clone(),
                "xlsx" => {
                    let mut book: calamine::Xlsx<_> = calamine::open_workbook(&output).unwrap();
                    let table = book.worksheet_range_at(0).unwrap().unwrap();
                    let header: Vec<String> = table
                        .rows()
                        .next()
                        .unwrap()
                        .iter()
                        .map(ToString::to_string)
                        .collect();
                    let index = header.iter().position(|h| h == expected_status).unwrap();
                    assert!(matches!(
                        table.rows().nth(1).unwrap()[index],
                        calamine::Data::Int(_) | calamine::Data::Float(_)
                    ));
                    header
                }
                "xls" => {
                    let mut book: calamine::Xls<_> = calamine::open_workbook(&output).unwrap();
                    book.worksheet_range_at(0)
                        .unwrap()
                        .unwrap()
                        .rows()
                        .next()
                        .unwrap()
                        .iter()
                        .map(ToString::to_string)
                        .collect()
                }
                _ => unreachable!(),
            };
            assert!(
                header.iter().any(|h| h == expected_address),
                "{format}/{mode}/{batch}/{segment}: {header:?}"
            );
            assert!(header.iter().any(|h| h == expected_status));
            assert!(!header
                .iter()
                .any(|h| h == if mode == "ui" { "Address" } else { "Adresse" }));
        }
    }
}
