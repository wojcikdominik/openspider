//! Embedding of the build number (UI-127, Nutzerentscheidung 09.11.2026): what `build.rs` put into this build must
//! match the rules, checked with the same environment and Git state at test time. Run in debug (no number unless the
//! variable is set), in release (Git count) and in release without Git (`GIT_DIR` pointing nowhere, separate target
//! directory): see docs/plan/M3-reports/b4-upddlg.md.
#![allow(clippy::disallowed_methods)]
use os_model::build_number::{self, BUILD_NUMBER};
use std::process::Command;

fn git(args: &[&str]) -> Option<String> {
    let out = Command::new("git")
        .args(args)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_owned())
        .filter(|s| !s.is_empty())
}

// spec: UI-127
#[test]
fn embedded_build_number_follows_environment_git_and_profile() {
    let compiled_env = option_env!("OPENSPIDER_BUILD_NUMBER").filter(|v| !v.trim().is_empty());
    let expected = match compiled_env {
        Some(value) => build_number::parse(value),
        None if cfg!(debug_assertions) => None,
        None => {
            let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("..")
                .join("..");
            let own = git(&["rev-parse", "--show-toplevel"])
                .and_then(|top| std::fs::canonicalize(top).ok())
                .is_some_and(|top| std::fs::canonicalize(&root).ok() == Some(top));
            build_number::from_git(&build_number::GitFacts {
                count: git(&["rev-list", "--count", "HEAD"]),
                shallow: git(&["rev-parse", "--is-shallow-repository"]),
                own_repository: own,
            })
        }
    };
    assert_eq!(
        BUILD_NUMBER.map(|n| n.parse::<u64>().unwrap()),
        expected,
        "env {compiled_env:?}, debug {}",
        cfg!(debug_assertions)
    );
    eprintln!(
        "build number: {BUILD_NUMBER:?} (debug {})",
        cfg!(debug_assertions)
    );
}
