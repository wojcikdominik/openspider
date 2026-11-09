//! Lint 2.3 Regel 1 (M2-F03, `crates/os-app/clippy.toml`): Datenbankzugriffe außerhalb von `pool.rs`/`write_queue.rs`
//! werden von Clippy abgewiesen (Negativtest), die Ausnahme-Attribute stehen nur dort.
//!
//! Der Negativtest übersetzt eine kleine Fixture (Verstöße gegen die Regel) mit der echten `clippy.toml` und erwartet,
//! dass Clippy genau die Verstöße meldet; dieselbe Fixture mit dem Ausnahme-Attribut muss sauber durchlaufen.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn crate_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for e in fs::read_dir(dir).unwrap().flatten() {
        let p = e.path();
        if p.is_dir() {
            rs_files(&p, out);
        } else if p.extension().is_some_and(|x| x == "rs") {
            out.push(p);
        }
    }
}

// spec: UI-140
#[test]
fn clippy_toml_forbids_direct_database_access() {
    let text = fs::read_to_string(crate_dir().join("clippy.toml")).unwrap();
    for item in [
        "rusqlite::Connection",
        "rusqlite::Statement",
        "os_store::CrawlFile",
        "rusqlite::Connection::open",
        "rusqlite::Connection::open_with_flags",
        "rusqlite::Connection::execute",
        "rusqlite::Connection::execute_batch",
        "rusqlite::Connection::prepare",
        "rusqlite::Connection::query_row",
        "os_store::CrawlFile::open",
        "os_store::CrawlFile::create",
    ] {
        assert!(
            text.contains(&format!("path = \"{item}\"")),
            "{item} fehlt in clippy.toml"
        );
    }
    // jeder Eintrag hat eine Begründung
    assert_eq!(
        text.matches("{ path = ").count(),
        text.matches(", reason = ").count()
    );
}

// spec: UI-140
#[test]
fn only_pool_and_write_queue_carry_the_exception() {
    let mut files = Vec::new();
    rs_files(&crate_dir().join("src"), &mut files);
    assert!(files.len() > 30);
    let mut with_exception = Vec::new();
    for f in files {
        let text = fs::read_to_string(&f).unwrap();
        if text.contains("clippy::disallowed_types") || text.contains("clippy::disallowed_methods")
        {
            // die Dateien dieses Moduls dürfen die Namen nennen (Kommentare), das Attribut `allow` nur pool/write_queue
            if text.lines().any(|l| {
                let l = l.trim_start();
                !l.starts_with("//") && l.contains("allow(") && l.contains("clippy::disallowed_")
            }) {
                with_exception.push(f.file_name().unwrap().to_string_lossy().into_owned());
            }
        }
    }
    with_exception.sort();
    assert_eq!(with_exception, vec!["pool.rs", "write_queue.rs"]);
}

const VIOLATIONS: &str = r#"
pub fn bad(path: &std::path::Path) {
    let conn = rusqlite::Connection::open(path).unwrap();
    conn.execute("DELETE FROM urls", []).unwrap();
    let _file = os_store::CrawlFile::open(path, os_store::OpenMode::ReadOnly);
}
"#;

fn clippy(dir: &Path) -> (bool, String) {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let out = Command::new(cargo)
        .args(["clippy", "--offline", "--quiet", "--manifest-path"])
        .arg(dir.join("Cargo.toml"))
        .args(["--", "-D", "warnings"])
        .env_remove("RUSTFLAGS")
        .env_remove("CARGO_ENCODED_RUSTFLAGS")
        .current_dir(dir)
        .output()
        .expect("cargo clippy startet");
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

fn write_fixture(dir: &Path, lib: &str) {
    let os_store = crate_dir().join("../os-store");
    let manifest = format!(
        "[package]\nname = \"lint-fixture\"\nversion = \"0.0.0\"\nedition = \"2021\"\npublish = false\n\n\
         [lib]\npath = \"lib.rs\"\n\n[workspace]\n\n[dependencies]\n\
         rusqlite = {{ version = \"0.41\", features = [\"bundled\", \"collation\"] }}\n\
         os-store = {{ path = '{}' }}\n",
        os_store.display().to_string().replace('\\', "/")
    );
    fs::write(dir.join("Cargo.toml"), manifest).unwrap();
    fs::write(dir.join("lib.rs"), lib).unwrap();
    fs::copy(crate_dir().join("clippy.toml"), dir.join("clippy.toml")).unwrap();
    // gleiche Versionen wie der Workspace
    let _ = fs::copy(crate_dir().join("../../Cargo.lock"), dir.join("Cargo.lock"));
}

fn environment_problem(stderr: &str) -> bool {
    !stderr.contains("disallowed")
        && (stderr.contains("failed to get")
            || stderr.contains("unable to update")
            || stderr.contains("no matching package")
            || stderr.contains("failed to load source")
            || stderr.contains("attempting to make an HTTP request"))
}

// spec: UI-140
#[test]
fn the_lint_rejects_direct_database_access_and_the_exception_attribute_lets_it_pass() {
    let dir = tempfile::tempdir().unwrap();

    write_fixture(dir.path(), VIOLATIONS);
    let (ok, stderr) = clippy(dir.path());
    if environment_problem(&stderr) {
        eprintln!("Lint-Negativtest übersprungen (Registry nicht erreichbar):\n{stderr}");
        return;
    }
    assert!(!ok, "Clippy hätte die Verstöße melden müssen:\n{stderr}");
    for needle in [
        "disallowed type `rusqlite::Connection`",
        "disallowed method `rusqlite::Connection::open`",
        "disallowed method `rusqlite::Connection::execute`",
        "disallowed method `os_store::CrawlFile::open`",
    ] {
        assert!(stderr.contains(needle), "{needle} fehlt in:\n{stderr}");
    }

    // Mit dem Ausnahme-Attribut (wie in pool.rs/write_queue.rs) ist dieselbe Datei sauber
    let allowed =
        format!("#![allow(clippy::disallowed_types, clippy::disallowed_methods)]\n{VIOLATIONS}");
    write_fixture(dir.path(), &allowed);
    let (ok, stderr) = clippy(dir.path());
    assert!(ok, "mit Ausnahme muss Clippy schweigen:\n{stderr}");
}
