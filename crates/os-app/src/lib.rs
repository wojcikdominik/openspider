//! `os-app` – Tauri-freie Service-Schicht der Desktop-App (`docs/plan/M2.md` 3.6, Entscheidung (Fable-Empfehlung) vom
//! 13.10.2026).
//!
//! Sitzungszustand, View-Register und View-Caches, Ereignis-Coalescer, DTOs (ts-rs), Operationen mit Abbruch und ein
//! transportunabhängiger Command-Dispatcher (Name + JSON) für Tauri-Commands, E2E-Bridge und In-App-HTTP-MCP. **Keine
//! Tauri-Abhängigkeit**; `os-mcp` und `os-cli` hängen nie an `os-app`.
//!
//! Stand M2-F01 (Gerüst): [`dto`] (alle DTOs und die TypeScript-Ausgabe), [`registry`] (Command-Register, Manifest) und
//! [`commands`] (jede Domäne als Stub-Datei, `STUB(M2-xx)`); die übrigen Module sind Stubs ihrer Besitzer.
//!
//! | Modul | Inhalt | Besitzer |
//! |---|---|---|
//! | [`dto`], [`registry`], [`commands`] (`mod.rs`) | Gerüst, DTOs, Register | M2-F01 (danach eingefroren) |
//! | [`runtime`], [`pool`], [`write_queue`], [`ops`], [`events`] (`sink`, `tick`), [`settings_store`], [`uistate`] | Laufzeit | M2-F03 |
//! | [`session`], [`events`] (`coalescer`), [`quit`] | Sitzung, Ticks | M2-K04 |
//! | [`views`] | View-Sitzungen | M2-K05 |
//! | [`clipboard`] | Zwischenablage | M2-K06 |
//! | [`source`], [`assets`] | Quelltext, Screenshots | M2-K09 |
//! | [`mcp`] | MCP im App-Prozess | M2-K17 |
//! | [`logging`], [`diag`] | Logs, Diagnose | M2-O01 |
//! | `stub` (Feature `stub-views`) | synthetische 5-Mio.-View | M2-S01 |

// Reuse integration fixtures from private unit tests.
#[cfg(test)]
extern crate self as os_app;

pub mod assets;
pub mod clipboard;
pub mod commands;
/// Absturzberichte (M3-C01, OUT-246/OUT-247; additiv nach K2-61).
pub mod crash;
pub mod diag;
pub mod dto;
pub mod events;
pub mod logging;
pub mod mcp;
pub mod ops;
pub mod pool;
pub mod quit;
pub mod registry;
pub mod runtime;
pub mod session;
pub mod settings_store;
pub mod source;
#[cfg(feature = "stub-views")]
pub mod stub;
pub mod uistate;
pub mod views;
pub mod write_queue;

pub use dto::{CmdError, ErrorCode};
pub use registry::{default_registry, manifest_json, CmdResult, CommandMeta, Handler, Registry};
pub use runtime::{App, AppConfig, CallCtx, WindowId};

pub mod update;
