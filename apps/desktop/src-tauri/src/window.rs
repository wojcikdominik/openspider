//! Fensterereignisse: Sichtbarkeit (UI-143) und Schließen/Beenden (F-11).
//!
//! * Sichtbarkeit: `Focused`, `Resized` und `Moved` lösen eine Neuberechnung aus; sichtbar ist ein Fenster, wenn es
//!   angezeigt und nicht minimiert ist. Das Ergebnis geht an `App::set_visibility` (Quelle 2 von 2); die zweite Quelle
//!   ist `visibilitychange` im Frontend. Der Verdeckt-Zustand von macOS (`NSWindowOcclusionState`) ist über Tauri nicht
//!   erreichbar; die Quelle des Frontends deckt ihn ab, soweit die WebView ihn meldet.
//! * Schließen: `CloseRequested` des Hauptfensters und `RunEvent::ExitRequested` (Strg/Cmd+Q) werden angehalten und
//!   als `App::request_quit` weitergegeben; erst bei `QuitDecision::Quit` beendet die Hülle den Prozess. Die Rückfrage
//!   bei laufendem Crawl stellt das Frontend.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

use os_app::quit::QuitDecision;
use os_app::{App, WindowId};
use tauri::{AppHandle, Emitter, Listener, Manager, Runtime, Window, WindowEvent};

/// Label des Hauptfensters.
pub const MAIN_WINDOW: &str = "main";

/// Merker, dass der Prozess nach `request_quit` kontrolliert beendet wird.
#[derive(Clone, Default)]
pub struct Quitting {
    exiting: Arc<AtomicBool>,
    pending: Arc<AtomicBool>,
}

impl Quitting {
    /// Läuft das kontrollierte Beenden?
    pub fn is_set(&self) -> bool {
        self.exiting.load(Ordering::SeqCst)
    }

    pub(crate) fn set(&self) {
        self.exiting.store(true, Ordering::SeqCst);
    }
}

/// Reagiert auf ein Fensterereignis.
pub fn on_window_event<R: Runtime>(
    app: &App,
    quitting: &Quitting,
    window: &Window<R>,
    event: &WindowEvent,
) {
    match event {
        WindowEvent::Focused(_) | WindowEvent::Resized(_) | WindowEvent::Moved(_) => {
            report_visibility(app, window);
        }
        WindowEvent::CloseRequested { api, .. } if window.label() == MAIN_WINDOW => {
            if !quitting.is_set() {
                api.prevent_close();
                request_quit(app.clone(), window.app_handle().clone(), quitting.clone());
            }
        }
        WindowEvent::Destroyed => {
            app.window_closed(&WindowId(window.label().to_string()));
        }
        _ => {}
    }
}

fn report_visibility<R: Runtime>(app: &App, window: &Window<R>) {
    let visible = window.is_visible().unwrap_or(true) && !window.is_minimized().unwrap_or(false);
    app.set_visibility(WindowId(window.label().to_string()), visible);
}

/// Fragt den Kern, ob beendet werden darf, und beendet bei `Quit`.
pub fn request_quit<R: Runtime>(app: App, handle: AppHandle<R>, quitting: Quitting) {
    if quitting.pending.swap(true, Ordering::SeqCst) {
        return;
    }
    tauri::async_runtime::spawn(async move {
        if !flush_frontend(&handle).await {
            quitting.pending.store(false, Ordering::SeqCst);
            focus_main(&handle);
            return;
        }
        match app.request_quit().await {
            QuitDecision::Quit => {
                if !os_app::update::install::on_quit(&app).await {
                    quitting.pending.store(false, Ordering::SeqCst);
                    return;
                }
                quitting.set();
                handle.exit(0);
            }
            // The App menu is also available while Settings is in front. Its
            // confirmation lives in main and must be made visible there.
            QuitDecision::Confirm { .. } => focus_main(&handle),
        }
        quitting.pending.store(false, Ordering::SeqCst);
    });
}

/// Holt das Hauptfenster nach vorn (zweiter Start, geöffnete Datei).
pub fn focus_main<R: Runtime>(handle: &AppHandle<R>) {
    if let Some(w) = handle.get_webview_window(MAIN_WINDOW) {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

/// UI-008/UI-030: pending frontend edits must reach the shared UI-state file before native close/menu/OS exit.
async fn flush_frontend<R: Runtime>(handle: &AppHandle<R>) -> bool {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    if handle.get_webview_window(MAIN_WINDOW).is_none() {
        return true;
    }
    let reply = format!(
        "os:ui-state-flushed-{}",
        NEXT.fetch_add(1, Ordering::Relaxed)
    );
    let (send, receive) = tokio::sync::oneshot::channel();
    let listener = handle.once(reply.clone(), move |event| {
        let _ = send.send(event.payload() == "true");
    });
    let sent = handle
        .emit_to(MAIN_WINDOW, "os:flush-ui-state", reply)
        .is_ok();
    let saved = if sent {
        tokio::time::timeout(std::time::Duration::from_secs(3), receive)
            .await
            .ok()
            .and_then(Result::ok)
            .unwrap_or(false)
    } else {
        false
    };
    handle.unlisten(listener);
    saved
}

#[cfg(test)]
mod tests {
    use super::*;

    // spec: UI-008
    // spec: UI-030
    #[tokio::test]
    async fn native_close_waits_for_the_frontend_save_acknowledgement() {
        let app = tauri::test::mock_app();
        let window =
            tauri::WebviewWindowBuilder::new(&app, MAIN_WINDOW, tauri::WebviewUrl::default())
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
}
