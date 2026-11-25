use os_ai::{provider::ProviderError, AiStatus};
use proptest::prelude::*;
fn fixture() -> Vec<AiStatus> {
    serde_json::from_str(r#"["Success", "Success (cached)", "Refusal", "Pending", "Pending (batch)", "Pending (confirmation)", "Pending (consent)", "Skipped (scope)", "Skipped (segment)", "Skipped (empty input)", "Skipped (policy)", "Skipped (no capable model)", "Skipped (image too large)", "Skipped (unsupported format)", "Skipped (no declared language)", "Budget exceeded", "Quota exceeded", "Error: Missing API key", "Error: Invalid output", "Error: Missing answer", "Error: Batch expired", "Error: Connection failed", "Error: Output limit reached"]"#).unwrap()
}
// spec: AI-020
#[test]
fn all_canonical_statuses_roundtrip_and_error_retry_membership_is_exact() {
    for status in fixture().into_iter().chain([
        AiStatus::Timeout { seconds: 30 },
        AiStatus::HttpError {
            code: 500,
            message: "fixture".into(),
        },
    ]) {
        let label = status.to_string();
        if let Some(canonical) = serde_json::to_value(&status).unwrap().as_str() {
            assert_eq!(label, canonical);
        }
        assert_eq!(
            status.is_error(),
            label.starts_with("Error: ") || label == "Quota exceeded" || label == "Budget exceeded"
        );
        assert_eq!(
            serde_json::from_str::<AiStatus>(&serde_json::to_string(&status).unwrap()).unwrap(),
            status
        );
    }
    assert_eq!(
        AiStatus::Timeout { seconds: 30 }.to_string(),
        "Error: Timeout nach 30 s"
    );
    assert_eq!(
        AiStatus::HttpError {
            code: 500,
            message: "fixture".into()
        }
        .to_string(),
        "Error: 500 fixture"
    );
}
// spec: AI-020
#[test]
fn documented_errors_have_canonical_statuses_without_invented_transport_codes() {
    let fixture = [
        ProviderError::MissingApiKey,
        ProviderError::Timeout { seconds: 30 },
        ProviderError::InvalidOutput,
        ProviderError::MissingAnswer,
        ProviderError::BatchExpired,
        ProviderError::QuotaExceeded,
        ProviderError::RateLimited {
            retry_after_seconds: Some(3),
        },
        ProviderError::Overloaded,
        ProviderError::Refusal,
        ProviderError::Policy,
        ProviderError::Http {
            status: 500,
            message: "fixture".into(),
        },
        ProviderError::OutputLimitReached {
            reason: "max_tokens".into(),
        },
    ];
    for error in fixture {
        assert!(!error.status().unwrap().to_string().is_empty());
    }
    assert_ne!(
        ProviderError::RateLimited {
            retry_after_seconds: Some(3)
        }
        .status(),
        Some(AiStatus::QuotaExceeded)
    );
    assert!(!ProviderError::Refusal.status().unwrap().is_error());
    // spec: AI-020 (K4-15): no HTTP code is invented; transport errors have their own final status.
    assert_eq!(
        ProviderError::Transport {
            message: "fixture".into()
        }
        .status(),
        Some(AiStatus::ConnectionFailed)
    );
}
proptest! {
    // spec: AI-020
    #[test]
    fn arbitrary_status_columns_use_only_canonical_values(index in 0usize..25, seconds in 0u64..3601, code in 400u16..600) {
        let mut statuses=fixture(); statuses.push(AiStatus::Timeout { seconds });statuses.push(AiStatus::HttpError { code,message:"fixture".into() });
        let status=&statuses[index];
        let text=status.to_string();
        prop_assert_eq!(status.is_error(),text.starts_with("Error: ") || text=="Quota exceeded" || text=="Budget exceeded");
    }
}
