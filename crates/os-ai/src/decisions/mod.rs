//! Decision-Engine (08 Abschnitt B). M4-D01: Fragenformat ([`format`], AI-030), Bibliothek und Versionierung
//! ([`library`], AI-031), Eingabeaufbereitung ([`input`], AI-032), Ausführung über die Decisions API und den Ersatzweg
//! mit Lebenszyklus der KI-Warteschlange ([`exec`], [`fallback`], AI-033, AI-034, AI-020 Nachtrag 26.11.2026) und
//! Routing ([`routing`], AI-035). Ergebnisse, Spalten und Tab: [`results`] (M4-D02); Segmente, Issues, Tester,
//! Kalibrierung: M4-D04.
pub mod calibration;
pub mod exec;
pub mod fallback;
pub mod format;
pub mod input;
pub mod issues;
pub mod library;
pub mod results;
pub mod routing;
pub mod segments;
pub mod tester;
