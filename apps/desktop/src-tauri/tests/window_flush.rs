//! Kontrolliertes Schließen wartet auf die Speicherbestätigung der Oberfläche (UI-008, UI-030).
//!
//! Der Test nutzt `tauri::test` (Mock-Laufzeit). Er liegt als Integrationstest hier und nicht als Unit-Test in
//! `src/window.rs`: Unter Windows/MSVC importiert der dabei mitgelinkte Tauri-Code `comctl32.dll`-Funktionen, die es nur
//! in Common-Controls v6 gibt. Das Manifest dafür bettet `tauri-build` nur in die App-Binaries ein; `build.rs` bettet es
//! per `cargo:rustc-link-arg-tests` zusätzlich in die Integrationstests ein. Für Unit-Tests der Bibliothek gibt es
//! keine entsprechende Cargo-Anweisung.

use openspider_desktop_lib::window::{flush_frontend, MAIN_WINDOW};
use tauri::{Emitter, Listener};

// spec: UI-008
// spec: UI-030
#[tokio::test]
async fn native_close_waits_for_the_frontend_save_acknowledgement() {
    let app = tauri::test::mock_app();
    let window = tauri::WebviewWindowBuilder::new(&app, MAIN_WINDOW, tauri::WebviewUrl::default())
        .build()
        .unwrap();
    for success in [true, false] {
        let reply_handle = app.handle().clone();
        let listener = window.once("os:flush-ui-state", move |event| {
            let reply: String = serde_json::from_str(event.payload()).unwrap();
            reply_handle.emit(&reply, success).unwrap();
        });
        assert_eq!(flush_frontend(app.handle()).await, success);
        window.unlisten(listener);
    }
    // A missing frontend must not make the native host discard unsaved edits.
    assert!(!flush_frontend(app.handle()).await);
}
