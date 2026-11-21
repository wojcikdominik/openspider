//! STUB(M4-E01): analysis implementation.
//! K4-09: hnsw for ANN; linfa-clustering, bhtsne and hdbscan for M4-E02.
//! STUB(M4-E02): own UMAP implementation; umap-rs conflicts with linfa on sprs.
pub fn run(
    _: &os_store::CrawlFile,
    _: &os_model::CrawlConfig,
    _: &os_model::CancelToken,
) -> Result<os_model::StepState, crate::AnalyzeError> {
    Ok(os_model::StepState::NotApplicable)
}
