//! OUT-254: `openspider --check-update` liest `updates.channel` aus derselben `settings.toml`, wendet dieselbe
//! Standardregel an wie die Desktop-App, fragt dieselbe URL an und schreibt nie in `settings.toml`.
#![allow(clippy::disallowed_types, clippy::disallowed_methods)]
use os_cli::update::{self, policy};
use os_model::{settings::AppSettings, ProxyMode};
use os_testsite::mocks::update::ChannelFixture;
use serde_json::Value;

const MATRIX: &str = include_str!("../../../tests/fixtures/update/channel-matrix.json");

fn text(row: &Value, key: &str) -> Option<String> {
    row[key].as_str().map(str::to_owned)
}

// spec: OUT-254
// spec: OUT-250
// spec: OUT-252
#[tokio::test]
async fn cli_matrix_rows_1_to_13_never_write_settings() {
    let rows = serde_json::from_str::<Value>(MATRIX).unwrap()["rows"]
        .as_array()
        .unwrap()
        .clone();
    assert_eq!(rows.len(), 13);
    for row in rows {
        let nr = row["nr"].as_u64().unwrap();
        let installed = text(&row, "installed").unwrap();
        let fixture = ChannelFixture::start(
            text(&row, "stable").as_deref(),
            text(&row, "beta").as_deref(),
        )
        .await
        .unwrap();
        let dir = tempfile::tempdir().unwrap();
        let body = match text(&row, "channel") {
            Some(c) => format!("[updates]\ncheck = true\nchannel = \"{c}\"\n"),
            None => "[updates]\ncheck = true\n".to_string(),
        };
        let path = AppSettings::path_in(dir.path());
        std::fs::write(&path, &body).unwrap();
        let before = std::fs::read(&path).unwrap();

        let mut ctx = update::context_from_settings(dir.path()).unwrap();
        assert_eq!(ctx.settings.channel, text(&row, "channel"), "row {nr}");
        // Only the base of both manifest URLs (mock server) and the running version are injected.
        ctx.endpoint = fixture.stable_url();
        ctx.beta_endpoint = fixture.beta_url();
        ctx.current = installed.parse().unwrap();
        ctx.os = "darwin".into();
        ctx.arch = "aarch64".into();
        ctx.pubkey = include_str!("../../../tests/fixtures/update/test.key.pub")
            .trim()
            .into();
        ctx.disabled_by_environment = false;
        ctx.network.proxy.mode = ProxyMode::None;
        let ca = dir.path().join("ca.pem");
        std::fs::write(&ca, fixture.site.ca_cert_pem()).unwrap();
        ctx.network
            .trusted_certificates
            .push(ca.to_string_lossy().into_owned());
        assert_eq!(ctx.installer, policy::Installer::AnyReportOnly);

        let result = update::check_with_context(&ctx).await;
        match row["result"].as_str().unwrap() {
            "current" => assert_eq!(result.as_ref().ok(), Some(&0), "row {nr}"),
            "update" => assert_eq!(result.as_ref().ok(), Some(&10), "row {nr}"),
            _ => {
                // Rejected manifest (OUT-252): error exit as for an invalid manifest (OUT-203/204), never 0 or 10.
                let error = result.as_ref().expect_err("row 12 rejected");
                assert_eq!(error.exit.code(), 1, "row {nr}");
            }
        }
        let (stable, beta) = (fixture.stable_requests(), fixture.beta_requests());
        match row["requested"].as_str().unwrap() {
            "stable" => assert_eq!((stable, beta), (1, 0), "row {nr}"),
            _ => assert_eq!((stable, beta), (0, 1), "row {nr}"),
        }
        let request = &fixture.site.requests()[0];
        assert!(
            request
                .path_and_query
                .ends_with(&format!("?version={installed}&os=darwin&arch=aarch64")),
            "row {nr}: {}",
            request.path_and_query
        );
        // The CLI never writes settings.toml: byte-identical before and after (rows 5/6 store nothing either).
        assert_eq!(std::fs::read(&path).unwrap(), before, "row {nr}");
    }
}

// spec: OUT-254
#[test]
fn cli_uses_the_same_built_in_urls_and_no_build_switch() {
    let ctx = policy::UpdateContext::default();
    assert_eq!(
        ctx.endpoint,
        "https://github.com/wojcikdominik/openspider/releases/latest/download/latest.json"
    );
    assert_eq!(
        ctx.beta_endpoint,
        "https://github.com/wojcikdominik/openspider/releases/download/update-beta/latest-beta.json"
    );
    assert_eq!(policy::Channel::Stable.manifest_url(), ctx.endpoint);
    assert_eq!(policy::Channel::Beta.manifest_url(), ctx.beta_endpoint);
    // Without a settings file the CLI applies the standard rule for its own version and creates no file.
    let dir = tempfile::tempdir().unwrap();
    let ctx = update::context_from_settings(dir.path()).unwrap();
    assert_eq!(ctx.settings.channel, None);
    assert_eq!(
        ctx.channel(),
        policy::default_channel(&policy::running_version())
    );
    assert!(!AppSettings::path_in(dir.path()).exists());
}
