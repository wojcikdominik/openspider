//! Command-Domänen der Service-Schicht: je Domäne eine Datei `commands/<domäne>.rs` (Besitzer laut `docs/plan/M2.md`
//! 3.6). `register_all` ruft `register` jeder Domäne; Namen und Typen stehen im Katalog (`apps/desktop/src/ipc/commands.json`).
//!
//! Die Dateien sind von M2-F01 als Stubs angelegt (`STUB(M2-xx)`): der Besitzer ersetzt die Datei und behält Namen,
//! Argument-/Rückgabetypen und `long_running` bei.

/// KI-Command-Katalog (M4-F02, Plan M4 3.3).
pub mod ai_catalog;
/// KI-Einwilligung und Datenflussübersicht (M4-U01).
pub mod ai_consent;
/// KI-Kosten: Statusleistenfeld und Kostenvoranschlag (M4-U01).
pub mod ai_cost;
/// Schalter „KI (Vorschau)“ in der Service-Schicht (M4-F02).
pub mod ai_preview;
/// KI-Einstellungen: Modellwahl je Aufgabe, Richtlinie, Budget (M4-U01).
pub mod ai_settings;
/// Optimierungsvorschläge (M4-V01, AI-070 ff.).
pub mod ai_suggest;
pub mod analysis;
pub mod api;
pub mod app;
pub mod auth;
pub mod auth_login;
pub mod catalog;
pub mod compare;
pub mod config;
pub mod crawl;
pub mod crawl_add;
pub mod crawls;
pub mod detail;
pub mod detail_a11y;
pub mod detail_ext;
pub mod detail_spelling;
pub mod diag;
pub mod events;
pub mod export;
pub mod mail;
pub mod mcp;
pub mod op;
pub mod profile;
pub mod projects;
pub mod search;
pub mod segment;
pub mod settings;
pub mod sidebar;
pub mod sidebar_ext;
pub mod tasks;
pub mod uistate;
pub mod view;
pub mod viz;

use crate::registry::Registry;

/// Trägt alle Domänen in die Registry ein (Reihenfolge = Reihenfolge von `commands.json`).
pub fn register_all(r: &mut Registry) {
    app::register(r);
    app_quit::register(r);

    settings::register(r);
    settings_network::register(r);
    uistate::register(r);
    op::register(r);
    events::register(r);
    diag::register(r);
    crawl::register(r);
    analysis::register(r);
    crawl_add::register(r);
    catalog::register(r);
    view::register(r);
    search::register(r);
    detail::register(r);
    detail_ext::register(r);
    detail_spelling::register(r);
    detail_a11y::register(r);
    sidebar::register(r);
    sidebar_ext::register(r);
    config::register(r);
    config_robots::register(r);
    profile::register(r);
    segment::register(r);
    crawls::register(r);
    projects::register(r);
    export::register(r);
    export_multi::register(r);
    export_summary::register(r);
    compare::register(r);
    viz::register(r);
    mcp::register(r);
    api::register(r);
    tasks::register(r);
    auth::register(r);
    auth_login::register(r);
    mail::register(r);
    update::register(r);
    render_provision::register(r);
    // M3-C01 (K2-61 additiv): Absturzberichte.
    crate::crash::commands::register(r);
    // M4-F02 (Plan M4 3.3): KI-Command-Katalog (Stubs, Besitzer je Command).
    ai_catalog::register(r);
    // M4-V01 (additiv): Prüfen, Übernehmen, Live-Prüfung und Fokus-Begriffe der Vorschläge.
    ai_suggest::register(r);
}

pub mod app_quit;
pub mod config_robots;
pub mod export_multi;
pub mod export_summary;
pub mod settings_network;

/// Crawl-file desktop extension points (M2-U15, K2-61).
pub mod crawl_files;

pub mod update;

/// Chromium-Bereitstellung nach K3-01 (M3-K01, Registrierung additiv nach K2-61).
pub mod render_provision;
