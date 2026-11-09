//! `os-model` – gemeinsame Typen für alle OpenSpider-Crates.
//!
//! Enthält Kennungen, Zeit-/ID-Quellen, Enums, das Parse- und Schreibmodell, die Crawl-Konfiguration
//! und die Indexierbarkeitsbewertung (M1-Plan 3.3/3.4). Ab M2: Konfiguration v2, App-Einstellungen
//! ([`settings`]), Erweiterungsschlüssel ([`ext`]) und das erweiterte Schreib-/Parse-Modell (M2-Plan 3.3). Das Crate kennt weder HTTP noch SQLite noch HTML.

/// Build-Nummer zur Anzeige neben der Version (UI-127, OUT-181).
pub mod build_number;
pub mod cancel;
pub mod config;
pub use cancel::{CancelToken, Cancelled};
pub mod enums;
pub mod enums_m2;
pub mod error;
pub mod ext;
pub mod ids;
pub mod indexability;
pub mod issue;
/// Eingedämmte Panics in Parsern für Fremddaten (OUT-246, M3-C01; additiv nach K2-61).
pub mod panic_guard;
pub mod parse;
pub mod parse_m2;
pub mod progress;
pub mod settings;
pub mod time;
pub mod value;
pub mod write;
pub mod write_m2;

pub use config::*;
pub use enums::*;
pub use enums_m2::*;
pub use error::ModelError;
pub use ext::{ExtKey, ExtWrite};
pub use ids::*;
pub use indexability::{evaluate, IndexabilityInput};
pub use issue::*;
pub use parse::*;
pub use parse_m2::*;
pub use progress::Progress;
pub use time::*;
pub use value::Value;
pub use write::*;
pub use write_m2::*;
