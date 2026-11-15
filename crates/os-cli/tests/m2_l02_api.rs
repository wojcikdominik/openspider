mod common;
use clap::Parser;
use common::{resolve, site, Env};
use os_cli::{exec, plan, reporter::Reporter, signals::Signals, Cli};
use os_model::{ApiKind, CancelToken, CrawlConfig};
use std::sync::Arc;

// spec: OUT-201
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn missing_credentials_warn_or_fail_before_crawling_and_are_logged() {
    let server = site("out-basic-site").await;
    for (flags, code) in [
        (vec![], 0),
        (vec!["--strict"], 4),
        (vec!["--require-apis"], 2),
    ] {
        let env = Env::new();
        let mut args = vec![
            "--crawl".into(),
            "http://site.test/".into(),
            "--resolve".into(),
            resolve(&server),
            "--use-google-search-console".into(),
            "missing@example.test".into(),
            "sc-domain:integ.test".into(),
        ];
        args.extend(flags.iter().map(|s| s.to_string()));
        let before = server.total_requests();
        let run = env.run_async(args).await;
        assert_eq!(run.code, code, "{}", run.dump());
        assert!(run.stderr.contains("missing@example.test"));
        if code == 2 {
            assert_eq!(before, server.total_requests());
        } else {
            let logs = std::fs::read_dir(env.home.path().join("logs"))
                .unwrap()
                .map(|p| std::fs::read_to_string(p.unwrap().path()).unwrap())
                .collect::<String>();
            assert!(logs.contains("missing@example.test"));
        }
    }
    for flags in [
        vec!["--use-google-analytics"],
        vec!["--use-google-search-console", "", "property"],
        vec!["--use-google-analytics-4", "a", "b"],
    ] {
        let env = Env::new();
        let mut args = vec!["--crawl", "http://site.test/"];
        args.extend(flags);
        let before = server.total_requests();
        let run = env.run_async(args).await;
        assert_eq!(run.code, 2, "{}", run.dump());
        assert_eq!(before, server.total_requests());
        if run.stderr.contains("Universal Analytics") {
            assert!(run.stderr.contains("nicht mehr unterstützt"));
        }
    }
}

// spec: OUT-201
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cli_selection_reaches_the_mock_and_exports_after_apis_finish() {
    use os_integrations::credentials::{Credential, Credentials};
    use os_testsite::mocks::{
        ga4::{Ga4Mock, MockRow, ReportGate},
        gsc::GscMock,
        MockServer,
    };
    let ga = Ga4Mock::default();
    let gate = Arc::new(ReportGate::default());
    ga.configure(|cfg| {
        cfg.report_gate = Some(gate.clone());
        cfg.rows = vec![MockRow::sessions("http://integ.test/", 137.0)];
    });
    let gs = GscMock::default();
    gs.set_rows(&[("http://integ.test/", 17.0, 68.0, 0.25, 3.75)]);
    let mocks = MockServer::start(vec![Arc::new(ga.clone()), Arc::new(gs.clone())])
        .await
        .unwrap();
    let server = site("out-basic-site").await;
    let home = tempfile::tempdir().unwrap();
    let paths = os_store::paths::app_paths_with_home(Some(home.path().as_os_str()));
    let output = home.path().join("exports");
    let config = home.path().join("api.osconfig");
    let mut cfg = CrawlConfig::default();
    cfg.api.ga4.base_url = Some(mocks.base_url("ga4"));
    cfg.api.gsc.base_url = Some(mocks.base_url("gsc"));
    cfg.save_file(&config).unwrap();
    let cli = Cli::try_parse_from([
        "openspider",
        "--crawl",
        "http://integ.test/",
        "--resolve",
        &resolve(&server),
        "--config",
        config.to_str().unwrap(),
        "--output-folder",
        output.to_str().unwrap(),
        "--use-google-analytics-4",
        "fixture",
        "1001",
        "2001",
        "3001",
        "--use-google-search-console",
        "fixture",
        "sc-domain:integ.test",
        "--require-apis",
        "--export-tabs",
        "Analytics:All,Search Console:All",
    ])
    .unwrap();
    let rep = Reporter::new(false);
    let mut plan = plan::build(&cli.run, paths, &rep).unwrap();
    assert_eq!(
        plan.config.api.ga4.analytics_account.as_deref(),
        Some("1001")
    );
    assert_eq!(
        plan.config.api.ga4.property.as_deref(),
        Some("properties/2001/dataStreams/3001")
    );
    let store = Arc::new(os_fetch::MemorySecretStore::new());
    let credentials = Credentials::new(store.clone());
    for kind in [ApiKind::Ga4, ApiKind::Gsc] {
        credentials
            .put(
                kind,
                Credential {
                    account_id: "fixture".into(),
                    label: "Fixture".into(),
                    access_token: Some("secret-fixture-token".into()),
                    api_key: None,
                    refresh_token: None,
                    expires_at: None,
                    client_secret: None,
                    scopes: vec![],
                },
                true,
            )
            .unwrap();
    }
    plan.api_secrets = Some(os_integrations::crawl_glue::SecretSnapshot(store));
    let execution = exec::execute(plan, rep.clone(), Signals::manual(CancelToken::new()));
    tokio::pin!(execution);
    tokio::select! {
        outcome = &mut execution => panic!("execution finished before the API response was released: {outcome:?}"),
        () = gate.wait_for_request() => {
            assert!(rep.exported_files().is_empty());
            assert!(!output.join("analytics_all.csv").exists());
            assert!(!output.join("search_console_all.csv").exists());
        }
    }
    gate.release();
    let outcome = execution.await;
    assert_eq!(
        outcome.exit,
        os_cli::Exit::Ok,
        "{} {:?}",
        outcome.summary,
        rep.warning_messages()
    );
    assert!(ga
        .requests()
        .iter()
        .any(|r| r.path.contains("properties/2001:runReport")));
    assert!(ga
        .requests()
        .iter()
        .any(|r| r.body.to_string().contains("integ.test")));
    assert!(gs
        .requests()
        .iter()
        .any(|r| r.property.as_deref() == Some("sc-domain:integ.test")));
    assert_eq!(rep.exported_files().len(), 2);
    assert!(rep.exported_files().iter().all(|p| p.is_file()));
    for (file, expected) in [
        ("analytics_all.csv", vec![("GA4 Sessions", 137.0)]),
        (
            "search_console_all.csv",
            vec![
                ("Clicks", 17.0),
                ("Impressions", 68.0),
                ("CTR", 0.25),
                ("Position", 3.75),
            ],
        ),
    ] {
        let rows = common::read_csv(&output.join(file));
        let address = rows[0].iter().position(|h| h == "Address").unwrap();
        let row = rows
            .iter()
            .skip(1)
            .find(|r| r[address] == "http://integ.test/")
            .expect("crawled fixture root in API export");
        for (column, expected) in expected {
            let index = rows[0]
                .iter()
                .position(|h| h == column)
                .unwrap_or_else(|| panic!("missing {column} in {}", rows[0].join(",")));
            assert_eq!(
                row[index].parse::<f64>().unwrap(),
                expected,
                "{file}: {column}"
            );
        }
    }
    assert!(rep
        .warning_messages()
        .iter()
        .all(|w| !w.contains("secret-fixture-token")));
}

// RC1-PSILOCAL (Issue 21): Quelle Lokal verlangt kein Konto; fehlt die Lighthouse-Laufzeit, nennt die CLI genau das.
// spec: INTEG-130
// spec: INTEG-132
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn local_pagespeed_without_runtime_warns_or_fails_with_the_runtime_message() {
    let server = site("out-basic-site").await;
    for (flags, code) in [(vec![], 0), (vec!["--require-apis"], 2)] {
        let env = Env::new();
        let config = env.home.path().join("psi-local.osconfig");
        let mut cfg = CrawlConfig::default();
        cfg.api.psi.local = true;
        cfg.save_file(&config).unwrap();
        let mut args = vec![
            "--crawl".to_string(),
            "http://site.test/".into(),
            "--resolve".into(),
            resolve(&server),
            "--config".into(),
            config.to_str().unwrap().into(),
            "--use-pagespeed".into(),
        ];
        args.extend(flags.iter().map(|s| s.to_string()));
        let before = server.total_requests();
        let run = env.run_async(args).await;
        assert_eq!(run.code, code, "{}", run.dump());
        assert!(
            run.stderr.contains("Lighthouse-Laufzeit nicht gefunden"),
            "{}",
            run.dump()
        );
        assert!(!run.stderr.contains("Konto Standard"), "{}", run.dump());
        if code == 2 {
            assert_eq!(before, server.total_requests());
        } else {
            assert!(run.stderr.contains("PageSpeed läuft nicht lokal"));
        }
    }
}
