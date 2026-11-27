//! Optimierungsvorschläge (08 Abschnitt D; M4-V01, Release 1.1: AI-070, AI-071, AI-072, AI-075, AI-076).
//!
//! * [`element`]: Elemente (Title, Meta Description; H1 und Alt-Text folgen mit M4-V02).
//! * [`limits`]: Grenzwerte aus den Preferences, Optionen (Kandidatenzahl, Reparaturrunden, Regeln).
//! * [`validate`]: lokale Prüfung (AI-075, Regeln 1–6), Pixelbreite über die Funktion aus UI-038 ([`PixelMeter`]).
//! * [`dedupe`]: Dublettenprüfung in zwei Stufen (AI-071).
//! * [`prompt`]: Aufgabenvorlage, Antwortschema `{"candidates": [...]}`, Rückmeldung der Reparaturrunde.
//! * [`run`]: Ablauf eines Laufs über `policy::guard::execute` mit Cache, Budget, Protokoll und Nachlauf.
//! * [`review`]: Prüfen, Übernehmen, Bearbeiten, Ablehnen, Massenaktion, Neuprüfung ohne Request (AI-076, AI-075).
//! * [`estimate`]: Zahlen des Kostenvoranschlags (AI-012) für den Dialog von M4-U01.
//! * [`style`]: Anschluss der Stil- und Markenregeln (AI-077, Release 1.2).
//! * [`lang`]: Sprachprüfung mit dem lokalen Detektor.
//!
//! Alles steht hinter dem Schalter „KI (Vorschau)“: Ohne ihn sendet [`run::Engine::run`] nichts
//! ([`run::RunError::PreviewOff`]); den Schalter prüft außerdem `guard::execute` vor jedem Request.
pub mod dedupe;
pub mod element;
pub mod estimate;
pub mod lang;
pub mod limits;
pub mod prompt;
pub mod review;
pub mod run;
pub mod style;
pub mod validate;

pub use dedupe::{normalized, post_pass, SiteIndex};
pub use element::Element;
pub use limits::{ElementLimits, RuleSet, SuggestOptions};
pub use review::{
    accept_first, apply, check_text, revalidate, BulkResult, ReviewAction, ReviewError,
};
pub use run::{
    finish, finish_without_request, CachedAnswer, ElementJob, Engine, FileStore, PageInput,
    RunControl, RunError, RunSummary, SuggestStore,
};
pub use style::{NoStyleRules, StyleRules};
pub use validate::{Checked, Checker, PixelMeter};
