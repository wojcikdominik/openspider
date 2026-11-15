//! Absturzberichte (M3-C01; OUT-246 lokal erfassen, OUT-247 ansehen und nur nach Klick senden).
//!
//! | Datei | Inhalt |
//! |---|---|
//! | `shared.rs` (+ `sanitize.rs`) | Berichtsformat, Bereinigung, Panic-Teil, Schreiben/Aufbewahrung; auch in der CLI (`#[path]`) |
//! | `session.rs` | Sitzungsmarke und `unclean_exit` |
//! | `service.rs` | App-Zustand, Hinweisliste, Fehlergrenzen der Oberfläche, Verwerfen/Löschen |
//! | `send.rs` | Senden an openspider.de (Proxy, 15 s, kein Wiederholversuch) |
//! | `commands.rs` | Commands `crash.*` |
//!
//! Es gibt keine Minidumps und keinen Fremddienst; ohne Klick auf „Senden“ verlässt nichts den Rechner.

/// Für `shared.rs` (gemeinsam mit der CLI, die dort `logging::diagnostics` einsetzt).
pub(crate) use crate::diag as diagnostics;

pub mod commands;
pub mod send;
pub mod service;
pub mod session;
pub mod shared;

pub use service::{
    enable_session_tracking, on_app_start, on_orderly_exit, on_update_exit,
    resume_after_update_exit,
};
pub use shared::{sanitize, Report, Sanitizer};
