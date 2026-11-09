//! OpenSpider Desktop: Tauri-Hülle um die Service-Schicht `os-app` (`docs/plan/M2.md` 3.7).
//!
//! Die Hülle enthält keine Fachlogik und greift nie auf Datenbank oder Kern zu (Test `tests/no_core_access.rs`):
//!
//! * [`commands`]: je Command-Domäne ein internes Plugin, alles geht an `App::dispatch`;
//! * [`events`]: ein Ereigniskanal je Fenster (Tauri-`Channel` → `EventSink`);
//! * [`window`]: Sichtbarkeit (UI-143) und Schließen/Beenden (F-11);
//! * [`open`]: Einzelinstanz und Dateizuordnung (OUT-232, OUT-272);
//! * [`assets`]: URI-Schema `ospider://`.
//!
//! Plugins: dialog, clipboard-manager, notification, os, single-instance, deep-link; mit dem Feature `e2e-webdriver`
//! zusätzlich der WebDriver-Server für E2E-B. Nach M2-F03 ist diese Datei eingefroren: neue Commands brauchen nur den
//! Registry-Eintrag in `os-app`.

pub mod assets;
pub mod commands;
pub mod crawl_files;
pub mod desktop;
pub mod events;
mod manual;
pub mod open;
pub mod plugin_names;
pub mod update;
pub mod window;

use os_app::{App, AppConfig};
use tauri::RunEvent;
use tauri_plugin_deep_link::DeepLinkExt;

/// Version der Anwendung (aus `Cargo.toml`).
fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// Tauri-Command: liefert die App-Version an die UI (Platzhalter des Gerüsts, bis das Frontend `app.info` nutzt).
#[tauri::command]
fn app_version() -> String {
    version().to_string()
}

/// Startet die Tauri-Anwendung.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // OUT-246 (M3-C01): Sitzungsmarke für `unclean_exit`, nur in der Desktop-App.
    os_app::crash::enable_session_tracking();
    // B4-RELCHECK: Installationsart vor `App::new`, das schon die erste Update-Prüfung startet.
    update::register_installer();
    let app = match App::new(AppConfig::from_env()) {
        Ok(app) => app,
        Err(e) => {
            eprintln!("OpenSpider konnte nicht starten: {e}");
            std::process::exit(1);
        }
    };
    let quitting = window::Quitting::default();

    let mut builder = tauri::Builder::default();

    // Einzelinstanz zuerst: eine zweite Instanz übergibt ihre Argumente und beendet sich (OUT-232)
    #[cfg(desktop)]
    {
        let open_app = app.clone();
        builder = builder.plugin(tauri_plugin_single_instance::init(
            move |handle, argv, cwd| {
                let cwd = std::path::PathBuf::from(cwd);
                open::forward(
                    &open_app,
                    &open::requests_from_args(&argv, Some(cwd.as_path())),
                );
                window::focus_main(handle);
            },
        ));
    }

    builder = builder
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(update::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_os::init())
        .plugin(tauri_plugin_deep_link::init());

    #[cfg(feature = "e2e-webdriver")]
    {
        builder = builder.plugin(tauri_plugin_wdio_webdriver::init());
    }

    for plugin in commands::plugins(&app) {
        builder = builder.plugin(plugin);
    }

    let menu_app = app.clone();
    let menu_quitting = quitting.clone();
    let events_app = app.clone();
    let events_quitting = quitting.clone();
    let scheme_app = app.clone();
    let setup_app = app.clone();
    let run_app = app.clone();
    let run_quitting = quitting.clone();
    let setup_quitting = quitting.clone();

    builder
        .manage(app)
        .on_menu_event(move |handle, event| {
            if matches!(event.id().as_ref(), "app.quit" | "file.quit") {
                window::request_quit(menu_app.clone(), handle.clone(), menu_quitting.clone());
            }
        })
        .on_window_event(move |window, event| {
            window::on_window_event(&events_app, &events_quitting, window, event);
        })
        .register_asynchronous_uri_scheme_protocol(
            assets::SCHEME,
            move |ctx, request, responder| {
                assets::handle(&scheme_app, ctx, request, responder);
            },
        )
        .invoke_handler(tauri::generate_handler![
            app_version,
            desktop::open_export_folder,
            desktop::browser_open,
            manual::open_manual,
            crawl_files::crawl_file_check,
            crawl_files::crawl_file_config,
            crawl_files::crawl_file_repair,
        ])
        .setup(move |tauri_app| {
            desktop::install(&setup_app, tauri_app.handle(), setup_quitting.clone());
            // Dateien, die den Start auslösen: Argumente (Windows/Linux) und geöffnete URLs (macOS)
            let args: Vec<String> = std::env::args().collect();
            let cwd = std::env::current_dir().ok();
            open::forward(&setup_app, &open::requests_from_args(&args, cwd.as_deref()));
            if let Ok(Some(urls)) = tauri_app.deep_link().get_current() {
                open::forward(&setup_app, &open::requests_from_urls(&urls));
            }
            let url_app = setup_app.clone();
            let url_handle = tauri_app.handle().clone();
            tauri_app.deep_link().on_open_url(move |event| {
                open::forward(&url_app, &open::requests_from_urls(&event.urls()));
                window::focus_main(&url_handle);
            });
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("Fehler beim Starten von OpenSpider")
        .run(move |handle, event| {
            // OUT-246 (M3-C01): geordnetes Beenden (auch durch das Betriebssystem) entfernt die Sitzungsmarke.
            if let RunEvent::Exit = event {
                os_app::crash::on_orderly_exit();
            }
            if let RunEvent::ExitRequested { api, code, .. } = event {
                // vom Nutzer ausgelöst (Cmd/Strg+Q): erst den Kern fragen
                if code.is_none() && !run_quitting.is_set() {
                    api.prevent_exit();
                    window::request_quit(run_app.clone(), handle.clone(), run_quitting.clone());
                }
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_version_matches_package_version() {
        assert_eq!(app_version(), env!("CARGO_PKG_VERSION"));
        assert!(!app_version().is_empty());
    }
}
