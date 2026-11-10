//! Build-Skript der Tauri-Hülle.
//!
//! * `tauri::generate_context!` verlangt, dass `frontendDist` existiert. Damit `cargo check/clippy/test --workspace`
//!   auch ohne vorherigen Frontend-Build funktioniert, wird das (gitignorierte) Verzeichnis bei Bedarf angelegt.
//! * Je Command-Domäne aus `../src/ipc/commands.json` entsteht ein internes Tauri-Plugin (`plugin:<domäne>|<command>`,
//!   M2.md 3.7). Tauri prüft jeden Plugin-Aufruf gegen die Capabilities; deshalb erzeugt dieses Skript je Domäne eine
//!   Berechtigungsdatei (`<plugin>:default` erlaubt alle Commands der Domäne; `plugin_name` setzt bei Namen von
//!   Tauri-Kern-Plugins das Präfix `os-`). `capabilities/default.json` nennt die Berechtigungen; der Test
//!   `tests/capabilities.rs` prüft, dass keine Domäne fehlt.
//!
//! Die Command-Namen kommen aus der generierten Datei `commands.json` (Format von `os-app-manifest`: eine Zeile
//! `"name": "<domäne>.<command>",` je Command); das Skript liest sie zeilenweise, ohne JSON-Abhängigkeit.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use tauri_build::{Attributes, InlinedPlugin};

mod build_update;

const COMMANDS_JSON: &str = "../src/ipc/commands.json";

// gemeinsame Regel für Plugin-Namen (`plugin_name`, `TAURI_CORE_PLUGINS`)
include!("src/plugin_names.rs");

/// Domäne → Commands in der Reihenfolge der Datei.
fn read_domains(path: &Path) -> BTreeMap<String, Vec<String>> {
    let text = std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("{} nicht lesbar: {e}", path.display()));
    let mut domains: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for line in text.lines() {
        let Some(rest) = line.trim().strip_prefix("\"name\": \"") else {
            continue;
        };
        let Some(name) = rest.strip_suffix("\",") else {
            continue;
        };
        let (domain, command) = name
            .split_once('.')
            .unwrap_or_else(|| panic!("Command ohne Domäne: {name}"));
        domains
            .entry(domain.to_string())
            .or_default()
            .push(command.to_string());
    }
    assert!(
        !domains.is_empty(),
        "{} enthält keine Commands (Format von os-app-manifest erwartet)",
        path.display()
    );
    domains
}

fn permission_file(domain: &str, commands: &[String]) -> String {
    let mut list = String::new();
    for (i, c) in commands.iter().enumerate() {
        if i > 0 {
            list.push_str(", ");
        }
        let _ = write!(list, "\"{c}\"");
    }
    format!(
        "# Automatisch aus commands.json erzeugt (build.rs) – nicht von Hand ändern.\n\
         [default]\n\
         description = \"Alle Commands der Domäne {domain}\"\n\
         permissions = [\"allow-all\"]\n\
         \n\
         [[permission]]\n\
         identifier = \"allow-all\"\n\
         description = \"Erlaubt alle Commands der Domäne {domain}\"\n\
         commands.allow = [{list}]\n"
    )
}

fn leak(s: String) -> &'static str {
    Box::leak(s.into_boxed_str())
}

fn main() {
    let manifest_dir =
        PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR"));

    // M3-F01: Tauri does not expand environment variables in JSON configuration.
    // Preserve CLI/platform overrides, then inject the public release configuration.
    for name in [
        "OPENSPIDER_UPDATER_PUBKEY",
        "OPENSPIDER_UPDATE_BETA",
        "OPENSPIDER_UPDATE_REPO",
        "TAURI_CONFIG",
    ] {
        println!("cargo:rerun-if-env-changed={name}");
    }
    println!("cargo:rerun-if-changed=build_update.rs");
    let mut config: serde_json::Value = std::env::var("TAURI_CONFIG")
        .ok()
        .map(|s| serde_json::from_str(&s).expect("TAURI_CONFIG invalid"))
        .unwrap_or_else(|| serde_json::json!({}));
    let base: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(manifest_dir.join("tauri.conf.json"))
            .expect("tauri.conf.json not readable"),
    )
    .expect("tauri.conf.json invalid");
    let key = std::env::var("OPENSPIDER_UPDATER_PUBKEY").ok();
    let beta = std::env::var("OPENSPIDER_UPDATE_BETA").ok();
    let repo = std::env::var("OPENSPIDER_UPDATE_REPO").ok();
    let missing_key = build_update::configure(
        &mut config,
        &base,
        key.as_deref(),
        beta.as_deref(),
        repo.as_deref(),
    )
    .unwrap_or_else(|message| panic!("{message}"));
    if build_update::warn_missing_key(std::env::var("PROFILE").as_deref().ok(), missing_key) {
        println!("cargo:warning=Release build has no updater public key; automatic updates remain disabled");
    }
    // cargo check/test do not stage the release sidecar; release builds require it.
    let triple = std::env::var("TARGET").expect("TARGET");
    let suffix = if triple.contains("windows") {
        ".exe"
    } else {
        ""
    };
    let sidecar = manifest_dir.join(format!("binaries/openspider-{triple}{suffix}"));
    println!("cargo:rerun-if-changed={}", sidecar.display());
    if std::env::var("PROFILE").as_deref() == Ok("debug") && !sidecar.exists() {
        config["bundle"]["externalBin"] = serde_json::json!([]);
    }
    let config = serde_json::to_string(&config).expect("config serialization");
    std::env::set_var("TAURI_CONFIG", &config);
    println!("cargo:rustc-env=TAURI_CONFIG={config}");

    let _ = std::fs::create_dir_all(manifest_dir.join("../dist"));

    println!("cargo:rerun-if-changed={COMMANDS_JSON}");
    let domains = read_domains(&manifest_dir.join(COMMANDS_JSON));

    let mut attributes = Attributes::new();
    for (domain, commands) in &domains {
        let plugin = plugin_name(domain);
        let dir = out_dir.join("os-permissions").join(&plugin);
        std::fs::create_dir_all(&dir).expect("Berechtigungsordner nicht anlegbar");
        std::fs::write(dir.join("default.toml"), permission_file(domain, commands))
            .expect("Berechtigungsdatei nicht schreibbar");
        let pattern = format!(
            "{}/*.toml",
            glob_escape(&dir.to_string_lossy().replace('\\', "/"))
        );
        attributes = attributes.plugin(
            leak(plugin),
            InlinedPlugin::new().permissions_path_pattern(leak(pattern)),
        );
    }
    embed_test_manifest(&out_dir);
    tauri_build::try_build(attributes).expect("tauri-build fehlgeschlagen");
}

/// Common-Controls-v6-Abhängigkeit, inhaltsgleich mit dem Standardmanifest von `tauri-build`.
const TEST_MANIFEST: &str = r#"<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <dependency>
    <dependentAssembly>
      <assemblyIdentity
        type="win32"
        name="Microsoft.Windows.Common-Controls"
        version="6.0.0.0"
        processorArchitecture="*"
        publicKeyToken="6595b64144ccf1df"
        language="*"
      />
    </dependentAssembly>
  </dependency>
</assembly>
"#;

/// Windows/MSVC: Manifest für die Integrationstest-Binaries (`tests/*.rs`).
///
/// `tauri-build` bettet sein Manifest per `cargo:rustc-link-arg-bins` nur in die App-Binaries ein. Ohne Manifest lädt
/// Windows `comctl32.dll` in Version 5; Testbinaries, die Tauri-App-Code linken (`tauri::test::mock_app`), brechen dann
/// beim Start mit `STATUS_ENTRYPOINT_NOT_FOUND` ab. `rustc-link-arg-tests` gilt nur für Integrationstests – weder für
/// die App noch für die Unit-Tests von `src/lib.rs` –, die ausgelieferte App bleibt also unverändert.
fn embed_test_manifest(out_dir: &Path) {
    let windows = std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows");
    let msvc = std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc");
    if !(windows && msvc) {
        return;
    }
    let manifest = out_dir.join("test-app-manifest.xml");
    std::fs::write(&manifest, TEST_MANIFEST).expect("Testmanifest nicht schreibbar");
    println!("cargo:rustc-link-arg-tests=/MANIFEST:EMBED");
    println!(
        "cargo:rustc-link-arg-tests=/MANIFESTINPUT:{}",
        manifest.display()
    );
}

/// Maskiert Glob-Sonderzeichen im Pfad.
fn glob_escape(path: &str) -> String {
    let mut out = String::with_capacity(path.len());
    for c in path.chars() {
        if matches!(c, '[' | ']' | '*' | '?') {
            out.push('[');
            out.push(c);
            out.push(']');
        } else {
            out.push(c);
        }
    }
    out
}
