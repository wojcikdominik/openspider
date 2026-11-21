//! AI persistence extension points; table names are stable across package handoffs.
pub mod answers;
pub mod batches;
pub mod budget;
pub mod cache;
pub mod embeddings;
pub mod link_suggestions;
pub mod prompt_results;
pub mod queries;
pub mod questions;
pub mod redirect_map;
pub mod requests;
pub mod runs;
pub mod suggestions;
#[cfg(test)]
mod tests;
