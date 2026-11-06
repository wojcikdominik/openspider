//! Command-Katalog (M2-F01, `docs/plan/M2.md` 3.6/3.6a, `M2-ui-concept.md` 4.4): Registry, `commands.json` und
//! DTO-Typen bilden einen Vertrag. Die erwarteten Namen stehen hier wörtlich (aus den Tabellen der Pläne), damit ein
//! fehlender oder zusätzlicher Command auffällt.

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

use os_app::{default_registry, manifest_json, App, AppConfig, CallCtx, ErrorCode, WindowId};

fn repo(rel: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(rel)
}

/// Alle Commands aus M2-ui-concept 4.4 (mit den Ersetzungen aus M2.md 3.6a für `profile.*`, `segment.*`, `api.*`).
const CATALOG_4_4: &[&str] = &[
    "app.info",
    "events.subscribe",
    "events.setVisibility",
    "events.setInterval",
    "settings.resources", // UI-123 / OUT-267: hardware limits in settings
    "settings.importCertificates", // OUT-270: explicitly accepted probe certificates
    "settings.get",
    "settings.set",
    "uistate.load",
    "uistate.save",
    "diag.debugInfo",
    "diag.bundle",
    "diag.logFrontend",
    "diag.openLogs",
    "diag.usageStats",
    "crawl.validateStartUrl",
    "crawl.parseList",
    "crawl.start",
    "crawl.pause",
    "crawl.resume",
    "crawl.stop",
    "crawl.clear",
    "crawl.open",
    "crawl.saveAs",
    "crawl.recrawl",
    "crawl.remove",
    "crawl.addUrls",
    "crawl.path",
    "analysis.start",
    "analysis.status",
    "catalog.tabs",
    "catalog.columns",
    "catalog.filterCounts",
    "view.open",
    "view.update",
    "view.rows",
    "view.locate",
    "view.range",
    "view.copy",
    "view.close",
    "search.parse",
    "search.validateRegex",
    "op.cancel",
    "detail.urlDetails",
    "detail.serp",
    "detail.serpMeasure",
    "detail.serpSaveEdit",
    "detail.serpReset",
    "detail.headers",
    "detail.sourceOpen",
    "detail.sourceLines",
    "detail.sourceDiff",
    "detail.screenshot",
    "detail.imagePreview",
    "detail.structuredData",
    "detail.duplicates",
    "detail.lighthouse",
    "sidebar.overview",
    "sidebar.issues",
    "sidebar.issueExplain",
    "sidebar.countStatus",
    "sidebar.countCancel",
    "sidebar.siteStructure",
    "sidebar.responseTimes",
    "sidebar.segments",
    "sidebar.apiStatus",
    "config.get",
    "config.validate",
    "config.set",
    "config.schema",
    "config.diff",
    "config.testRules",
    "crawls.list",
    "crawls.rename",
    "crawls.move",
    "crawls.lock",
    "crawls.unlock",
    "crawls.duplicate",
    "crawls.delete",
    "crawls.import",
    "crawls.export",
    "crawls.reveal",
    "crawls.copyIds",
    "projects.create",
    "projects.rename",
    "projects.lock",
    "export.catalog",
    "export.suggestName",
    "export.view",
    "export.detail",
    "export.bulk",
    "export.report",
    "export.sitemap",
    "export.issues",
    "export.sidebar",
    "compare.open",
    "compare.swap",
    "compare.testMapping",
    "compare.contentAvailable", // TECH-300: read-only prerequisite for Content selection
    "compare.close",
    "viz.open",
    "viz.graph",
    "viz.words",
    "viz.exportCsv",
    "viz.focusInMain",
    "mcp.status",
    "mcp.start",
    "mcp.stop",
    "mcp.tools",
    "mcp.setToolEnabled",
    "mcp.setClasses",
    "mcp.token",
    "mcp.regenerateToken",
    "mcp.setAutostart",
    "mcp.saveToolsMarkdown",
    "mcp.clientConfigPreview",
    "mcp.clientConnect",    // AGENT-043 (B3-MCP): Verbinden in einem Schritt
    "mcp.clientDisconnect", // AGENT-043 (B3-MCP): nur den eigenen Eintrag entfernen
    "mcp.callLog",
];

/// Tabelle 3.6a (alle Zeilen) einschließlich `detail.spellingContext` und `detail.a11yNode`.
const CATALOG_3_6A: &[&str] = &[
    // M3.md 3.2: release update contracts.
    "update.check",
    "update.status",
    "update.download",
    "update.install",
    "update.set_auto",
    // M3-K01 (K3-01): Chromium-Bereitstellung.
    "chromium.status",
    "chromium.download",
    "chromium.cancel",
    "chromium.remove",
    // M3-C01 (OUT-246/OUT-247): Absturzberichte.
    "crash.overview",
    "crash.read",
    "crash.send",
    "crash.dismiss",
    "crash.delete",
    "crash.reportFrontend",
    "app.requestQuit",
    "app.confirmQuit",
    "config.testRobots",
    "config.loadRobots",
    "settings.probeCertificates",
    "settings.setProxyPassword",
    "api.removeAccount",
    "auth.importProfile",
    "auth.exportProfile",
    "diag.previewBundle",
    "projects.list",
    "crawls.checkLocation",
    "export.multi",
    "export.presetList",
    "export.presetSave",
    "export.presetDelete",
    "export.customSummaryMetrics",
    "export.customSummary",
    "export.sheets",
    "api.list",
    "api.connect",
    "api.disconnect",
    "api.testConnection",
    "api.getSettings",
    "api.setSettings",
    "api.status",
    "api.fetch",
    "api.errors",
    "tasks.list",
    "tasks.get",
    "tasks.validate",
    "tasks.save",
    "tasks.delete",
    "tasks.run",
    "tasks.history",
    "tasks.openLog",
    "tasks.register",
    "tasks.unregister",
    "tasks.export",
    "tasks.import",
    "segment.list",
    "segment.save",
    "segment.delete",
    "segment.validate",
    "segment.preview",
    "segment.reassign",
    "auth.listProfiles",
    "auth.saveProfile",
    "auth.deleteProfile",
    "auth.openLogin",
    "auth.finishLogin",
    "auth.loginStatus", // CRAWL-120, additive secret-free status query (K2-61).
    "profile.list",
    "profile.load",
    "profile.save",
    "profile.delete",
    "profile.setDefault",
    "profile.import",
    "profile.export",
    "mail.listAccounts",
    "mail.saveAccount",
    "mail.deleteAccount",
    "mail.testAccount",
    "detail.spellingContext",
    "detail.spellingIgnore",
    "detail.spellingDictionary",
    "detail.spellingRecheck",
    "detail.screenshotSave",
    "detail.sourceSave",
    "detail.a11yNode",
];

/// `long_running` laut den Tabellen (liefert sofort einen `OpHandle`).
const LONG_RUNNING: &[&str] = &[
    "detail.spellingIgnore",
    "detail.spellingDictionary",
    "detail.spellingRecheck",
    "export.multi",
    "export.customSummary",
    "export.sheets",
    // 3.6a
    "api.connect",
    "api.fetch",
    "tasks.run",
    "tasks.export",
    "tasks.import",
    "segment.preview",
    "segment.reassign",
    "auth.openLogin",
    "mail.testAccount",
    // 4.4
    "diag.bundle",
    "crawl.saveAs",
    "crawl.recrawl",
    "crawl.remove",
    "crawl.addUrls",
    "analysis.start",
    "detail.sourceDiff",
    "crawls.move",
    "crawls.duplicate",
    "crawls.delete",
    "crawls.import",
    "crawls.export",
    "export.view",
    "export.detail",
    "export.bulk",
    "export.report",
    "export.sitemap",
    "export.issues",
    "export.sidebar",
    "viz.exportCsv",
];

fn names() -> Vec<&'static str> {
    default_registry()
        .manifest()
        .iter()
        .map(|m| m.name)
        .collect()
}

// spec: UI-140
#[test]
fn registry_contains_exactly_the_documented_commands() {
    let have: BTreeSet<&str> = names().into_iter().collect();
    let want: BTreeSet<&str> = CATALOG_4_4.iter().chain(CATALOG_3_6A).copied().collect();
    let missing: Vec<_> = want.difference(&have).collect();
    let extra: Vec<_> = have.difference(&want).collect();
    assert!(missing.is_empty(), "fehlen im Register: {missing:?}");
    assert!(extra.is_empty(), "nicht dokumentiert: {extra:?}");
    assert_eq!(names().len(), have.len(), "doppelter Name");
    // sidebar.apiStatus und api.status liefern dasselbe DTO (3.6a)
    let m = default_registry().manifest();
    let ret = |n: &str| m.iter().find(|c| c.name == n).unwrap().ret;
    assert_eq!(ret("api.status"), ret("sidebar.apiStatus"));
}

// spec: UI-140
#[test]
fn domains_are_the_documented_ones() {
    let allowed: BTreeSet<&str> = [
        "app", "events", "settings", "uistate", "diag", "crawl", "analysis", "catalog", "view",
        "search", "op", "detail", "sidebar", "config", "crawls", "projects", "export", "compare",
        "viz", "mcp", "api", "tasks", "segment", "auth", "profile", "mail", "update",
        // M3-K01 (K3-01)
        "chromium", // M3-C01 (OUT-246/OUT-247)
        "crash",
    ]
    .into_iter()
    .collect();
    for m in default_registry().manifest() {
        assert!(
            allowed.contains(m.domain()),
            "{}: unbekannte Domäne",
            m.name
        );
        assert!(
            m.name.matches('.').count() == 1,
            "{}: Form <domäne>.<command>",
            m.name
        );
    }
}

// spec: UI-141
#[test]
fn long_running_flags_follow_the_tables() {
    let reg = default_registry();
    let long: BTreeSet<&str> = reg
        .manifest()
        .iter()
        .filter(|m| m.long_running)
        .map(|m| m.name)
        .collect();
    let want: BTreeSet<&str> = LONG_RUNNING.iter().copied().collect();
    assert_eq!(long, want);
}

// spec: UI-140
#[test]
fn argument_and_result_types_resolve_to_generated_files() {
    let gen = repo("apps/desktop/src/ipc/gen");
    let primitives = ["void", "string", "number", "boolean"];
    let resolve = |expr: &str, cmd: &str| {
        for part in expr.split('|').map(str::trim) {
            let base = part.trim_end_matches("[]");
            if primitives.contains(&base) || base == "null" {
                continue;
            }
            let file = if base == "JsonValue" {
                gen.join("serde_json/JsonValue.ts")
            } else {
                gen.join(format!("{base}.ts"))
            };
            assert!(
                file.is_file(),
                "{cmd}: Typ {base} ({expr}) hat keine Datei {}",
                file.display()
            );
        }
    };
    for m in default_registry().manifest() {
        resolve(m.args, m.name);
        resolve(m.ret, m.name);
    }
}

// spec: UI-140
#[test]
fn commands_json_matches_the_registry() {
    let json = manifest_json(&default_registry());
    let path = repo("apps/desktop/src/ipc/commands.json");
    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        fs::write(&path, &json).unwrap();
    }
    let on_disk = fs::read_to_string(&path).expect("commands.json fehlt (UPDATE_GOLDEN=1)");
    assert_eq!(
        on_disk, json,
        "commands.json weicht vom Register ab (UPDATE_GOLDEN=1 oder os-app-manifest)"
    );
    // Form der Einträge
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    let first = &v.as_array().unwrap()[0];
    assert_eq!(first["name"], "app.info");
    assert!(
        first.get("longRunning").is_some()
            && first.get("args").is_some()
            && first.get("ret").is_some()
    );
}

// spec: UI-140
#[tokio::test]
async fn stubs_answer_unsupported_with_command_and_owner() {
    let dir = std::env::temp_dir().join(format!("os-app-manifest-{}", std::process::id()));
    let app = App::new(AppConfig::new(dir)).unwrap();
    let mut owners = BTreeSet::new();
    for m in app.registry().manifest() {
        let ctx = CallCtx {
            window: WindowId("main".into()),
            cancel: Default::default(),
        };
        // M2-F03 hat die Laufzeit-Domänen (app, settings, uistate, op, events) umgesetzt: sie sind keine Stubs mehr
        let Err(err) = app.dispatch(ctx, m.name, serde_json::json!({})).await else {
            continue;
        };
        if err.message_key != "err-not-implemented" {
            continue;
        }
        assert_eq!(err.code, ErrorCode::Unsupported, "{}", m.name);
        assert_eq!(err.args["command"], m.name);
        owners.insert(err.args["owner"].as_str().unwrap().to_string());
    }
}

// spec: UI-140
#[tokio::test]
async fn unknown_commands_are_not_found() {
    let app = App::new(AppConfig::new(std::env::temp_dir().join("os-app-unknown"))).unwrap();
    let ctx = CallCtx {
        window: WindowId("main".into()),
        cancel: Default::default(),
    };
    let err = app
        .dispatch(ctx, "nope.nothing", serde_json::Value::Null)
        .await
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::NotFound);
    assert_eq!(err.args["command"], "nope.nothing");
}

// Modulkarte 3.6: jede Domäne liegt in einer Datei commands/<name>.rs mit Besitzer-Marke
// spec: UI-140
#[test]
fn every_command_file_is_a_marked_stub_with_a_known_owner() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/commands");
    let mut n = 0;
    for e in fs::read_dir(&dir).unwrap() {
        let p = e.unwrap().path();
        if p.file_name().is_some_and(|f| f == "mod.rs") || p.extension().is_none_or(|x| x != "rs") {
            continue;
        }
        let text = fs::read_to_string(&p).unwrap();
        if text.contains("stub_commands!(") {
            assert!(
                text.contains("STUB(M2-"),
                "{}: Stub ohne Besitzer-Marke",
                p.display()
            );
        }
        n += 1;
    }
    assert!(n >= 30, "{n} Command-Dateien");
}

// spec: UI-140
#[test]
fn service_extensions_follow_their_command_domains() {
    let manifest = default_registry().manifest();
    let pos = |name| manifest.iter().position(|m| m.name == name).unwrap();
    assert!(pos("settings.get") < pos("config.testRobots"));
    assert!(pos("export.catalog") < pos("export.multi"));
    assert_eq!(
        manifest
            .iter()
            .find(|m| m.name == "app.requestQuit")
            .unwrap()
            .ret,
        "QuitDecision"
    );
}
