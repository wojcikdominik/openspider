//! AI-Search-Sichtbarkeit (08 Abschnitt F, ohne KI-Schlüssel nutzbar).
//!
//! * [`catalog`]: AI-Crawler-Katalog (AI-120, M4-S01).
//! * [`robots`]: robots.txt-Auswertung je AI-Crawler (AI-121, M4-S01).
//!
//! STUB(M4-S02): analysis implementation ([`run`]).
pub mod catalog;
pub mod robots;

pub fn run(
    _: &os_store::CrawlFile,
    _: &os_model::CrawlConfig,
    _: &os_model::CancelToken,
) -> Result<os_model::StepState, crate::AnalyzeError> {
    Ok(os_model::StepState::NotApplicable)
}
