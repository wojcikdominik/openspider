//! Build-Skript: bettet die Build-Nummer ein (`OS_BUILD_NUMBER`, siehe `src/build_number.rs`, UI-127).
//!
//! Quelle: `OPENSPIDER_BUILD_NUMBER` (Release-Workflow), sonst bei Release-Builds `git rev-list --count HEAD`, wenn
//! der Checkout vollständig ist und zu diesem Workspace gehört. Ohne Git oder im flachen Checkout: keine Nummer.

use std::path::{Path, PathBuf};
use std::process::Command;

#[path = "src/build_number.rs"]
#[allow(dead_code)]
mod build_number;

fn git(dir: &Path, args: &[&str]) -> Option<String> {
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8(out.stdout).ok()?;
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_string())
}

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=src/build_number.rs");
    println!("cargo:rerun-if-env-changed={}", build_number::ENV);
    println!("cargo:rerun-if-env-changed=GIT_DIR");
    let manifest =
        PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let root = manifest.join("..").join("..");
    let release = std::env::var("PROFILE").as_deref() == Ok("release");
    let env = std::env::var(build_number::ENV).ok();
    let number = build_number::resolve(env.as_deref(), release, || {
        // A new commit or branch switch changes the number (worktrees included).
        for args in [
            ["rev-parse", "--git-path", "HEAD"],
            ["rev-parse", "--git-path", "packed-refs"],
        ] {
            if let Some(path) = git(&manifest, &args) {
                println!("cargo:rerun-if-changed={}", manifest.join(path).display());
            }
        }
        if let Some(reference) = git(&manifest, &["symbolic-ref", "-q", "HEAD"]) {
            if let Some(path) = git(&manifest, &["rev-parse", "--git-path", &reference]) {
                println!("cargo:rerun-if-changed={}", manifest.join(path).display());
            }
        }
        let canonical = |p: &Path| std::fs::canonicalize(p).ok();
        let own_repository = git(&manifest, &["rev-parse", "--show-toplevel"])
            .and_then(|top| canonical(Path::new(&top)))
            .is_some_and(|top| Some(top) == canonical(&root));
        build_number::GitFacts {
            count: git(&manifest, &["rev-list", "--count", "HEAD"]),
            shallow: git(&manifest, &["rev-parse", "--is-shallow-repository"]),
            own_repository,
        }
    })
    .unwrap_or_else(|message| panic!("{message}"));
    if let Some(number) = number {
        println!("cargo:rustc-env=OS_BUILD_NUMBER={number}");
    }
}
