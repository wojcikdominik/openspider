//! STUB(M4-C01): AI CLI orchestration. Flags are rejected by RunArgs::unsupported until implemented.
#[cfg(test)]
mod tests {
    use clap::Parser;
    // spec: AI-022
    #[test]
    fn fixture_flags_are_recognized_and_reported_as_pending() {
        let cli = crate::args::Cli::try_parse_from([
            "openspider",
            "--use-ai",
            "--use-embeddings",
            "--ai-tasks",
            "decisions",
            "--ai-consent",
            "--ai-sample",
            "25",
            "--api-budget",
            "0.01",
            "--ai-budget",
            "0.01",
            "--export-query",
            "Titles",
        ])
        .unwrap();
        assert_eq!(cli.run.unsupported().len(), 8);
        assert!(cli.run.unsupported().contains(&"--ai-tasks"));
    }
}
