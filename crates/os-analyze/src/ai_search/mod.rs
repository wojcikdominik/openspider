//! STUB(M4-S02): analysis implementation.
pub fn run(
    _: &os_store::CrawlFile,
    _: &os_model::CrawlConfig,
    _: &os_model::CancelToken,
) -> Result<os_model::StepState, crate::AnalyzeError> {
    Ok(os_model::StepState::NotApplicable)
}
