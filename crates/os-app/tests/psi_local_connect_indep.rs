// Unabhängige Tests (nur aus der Spezifikation) zu „Verbinden“ der PageSpeed-Quelle Lokal über `api.connect`
// (INTEG-130, INTEG-132). Als Lighthouse dient die Fixture `tests/fixtures/integ-psi-local/fake-lighthouse.js`
// (Node-Skript ohne Netzwerk; gibt bei `--version` 12.4.1 aus und protokolliert Aufrufe in `calls.log`).
#![allow(clippy::disallowed_methods)]
#[path = "cmd_api_support.rs"]
mod support;
use std::path::PathBuf;
use support::*;

const FAKE: &str = include_str!("../../../tests/fixtures/integ-psi-local/fake-lighthouse.js");
const FAKE_VERSION: &str = "12.4.1";

/// Ersatz-Lighthouse in einem Ordner mit Leerzeichen.
struct Install {
    _root: tempfile::TempDir,
    dir: PathBuf,
}

impl Install {
    fn new() -> Install {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("Program Files").join("Light House");
        std::fs::create_dir_all(&dir).unwrap();
        Install { _root: root, dir }
    }
    fn script(&self) -> PathBuf {
        let path = self.dir.join("index.js");
        std::fs::write(&path, FAKE).unwrap();
        path
    }
    fn calls(&self) -> usize {
        std::fs::read_to_string(self.dir.join("calls.log"))
            .unwrap_or_default()
            .lines()
            .count()
    }
}

async fn psi_row(h: &Harness) -> Value {
    let rows = h.call("api.list", json!(null)).await.unwrap();
    rows.as_array()
        .unwrap()
        .iter()
        .find(|r| r["kind"] == "psi")
        .cloned()
        .unwrap()
}

// spec: INTEG-130
// spec: INTEG-132
#[tokio::test]
async fn local_connect_needs_no_key_and_reports_the_lighthouse_version() {
    let h = Harness::new().await;
    let install = Install::new();
    let path = install.script();
    let account = h
        .op(
            "api.connect",
            json!({"kind":"psi","psiLocal":true,"lighthousePath":path.to_string_lossy(),"remember":false}),
        )
        .await
        .unwrap();
    assert!(
        account["account"]
            .as_str()
            .unwrap_or("")
            .contains(FAKE_VERSION),
        "Version im Kontodialog: {account}"
    );
    assert_eq!(account["keySuffix"], Value::Null);
    let row = psi_row(&h).await;
    assert_eq!(row["state"], "connected", "{row}");
    assert!(install.calls() >= 1, "Lighthouse wurde nicht befragt");
    assert!(
        h.mock.log.calls().is_empty(),
        "Lokal darf die PSI-API nicht ansprechen: {:?}",
        h.mock.log.calls()
    );
    h.server.stop().await;
}

// spec: INTEG-132
#[tokio::test]
async fn local_connect_with_padded_path_uses_the_trimmed_path() {
    let h = Harness::new().await;
    let install = Install::new();
    let path = install.script();
    let padded = format!("  {}  ", path.to_string_lossy());
    let account = h
        .op(
            "api.connect",
            json!({"kind":"psi","psiLocal":true,"lighthousePath":padded,"remember":false}),
        )
        .await
        .unwrap();
    assert!(account["account"]
        .as_str()
        .unwrap_or("")
        .contains(FAKE_VERSION));
    h.server.stop().await;
}

// spec: INTEG-132
#[tokio::test]
async fn local_connect_without_runtime_fails_with_runtime_missing() {
    let h = Harness::new().await;
    let install = Install::new();
    let not_there = install.dir.join("fehlt").join("lighthouse");
    let cases: Vec<(&str, Value)> = vec![
        ("kein Pfad", Value::Null),
        ("leerer Pfad", json!("")),
        ("nur Leerzeichen", json!("   ")),
        ("Pfad existiert nicht", json!(not_there.to_string_lossy())),
        ("Verzeichnis", json!(install.dir.to_string_lossy())),
        (
            "relativer Pfad",
            json!("node_modules/lighthouse/cli/index.js"),
        ),
    ];
    for (case, path) in cases {
        let failed = h
            .op(
                "api.connect",
                json!({"kind":"psi","psiLocal":true,"lighthousePath":path,"remember":false}),
            )
            .await
            .unwrap_err();
        assert_eq!(failed.message_key, "err-api-lighthouse-missing", "{case}");
        let row = psi_row(&h).await;
        assert_eq!(row["state"], "error", "{case}: {row}");
        assert_eq!(
            row["error"]["messageKey"], "err-api-lighthouse-missing",
            "{case}: {row}"
        );
    }
    assert!(h.mock.log.calls().is_empty());
    h.server.stop().await;
}

// spec: INTEG-130
#[tokio::test]
async fn remote_without_key_is_not_switched_to_local_by_a_valid_path() {
    let h = Harness::new().await;
    let install = Install::new();
    let path = install.script();
    let failed = h
        .op(
            "api.connect",
            json!({"kind":"psi","psiLocal":false,"lighthousePath":path.to_string_lossy()}),
        )
        .await
        .unwrap_err();
    assert_ne!(failed.message_key, "err-api-lighthouse-missing");
    assert_eq!(install.calls(), 0, "Remote darf Lighthouse nicht starten");
    assert_ne!(psi_row(&h).await["state"], "connected");
    h.server.stop().await;
}

// spec: INTEG-130
#[tokio::test]
async fn remote_key_is_untouched_by_local_connect_attempts() {
    let h = Harness::new().await;
    // Remote wie bisher: Schlüssel, eine Prüfanfrage an die API.
    h.connect().await;
    assert_eq!(h.mock.log.calls().len(), 1);
    let remote = psi_row(&h).await;
    assert_eq!(remote["state"], "connected");
    let remote_id = remote["accounts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| !a["keySuffix"].is_null())
        .map(|a| a["accountId"].as_str().unwrap().to_string())
        .expect("gespeicherter Remote-Schlüssel");

    // Lokal scheitert (keine Laufzeit), dann gelingt Lokal: beides ohne Remote-Anfrage.
    h.op(
        "api.connect",
        json!({"kind":"psi","psiLocal":true,"lighthousePath":"","remember":false}),
    )
    .await
    .unwrap_err();
    let install = Install::new();
    h.op(
        "api.connect",
        json!({"kind":"psi","psiLocal":true,"lighthousePath":install.script().to_string_lossy(),"remember":false}),
    )
    .await
    .unwrap();
    assert_eq!(h.mock.log.calls().len(), 1);

    // Der Remote-Schlüssel ist weiterhin gespeichert und verbindet wie zuvor mit einer Prüfanfrage.
    let row = psi_row(&h).await;
    assert!(
        row["accounts"]
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a["accountId"] == remote_id.as_str() && !a["keySuffix"].is_null()),
        "{row}"
    );
    let account = h
        .op(
            "api.connect",
            json!({"kind":"psi","psiLocal":false,"accountId":remote_id}),
        )
        .await
        .unwrap();
    assert!(!account["keySuffix"].is_null(), "{account}");
    assert_eq!(h.mock.log.calls().len(), 2);
    h.server.stop().await;
}
