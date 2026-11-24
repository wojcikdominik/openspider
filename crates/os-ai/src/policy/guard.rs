//! Mandatory privacy guard before every request (AI-018, AI-024): select only the consented fields, redact them
//! locally, check consent for (provider, task type, field set) and only then hand over to
//! [`crate::provider::execute`]. Without consent no request is sent and the entry is `Pending (consent)`.
use super::{
    consent::{ConsentGate, ConsentKey},
    fields::{FieldSet, FieldValues},
    redact::{RedactionCounts, Redactor},
};
use crate::{
    preview::PreviewSwitch,
    provider::{self, AiProvider, Input, Outcome, Request},
};
use os_model::ai::AiStatus;

/// Input of one request after field selection and redaction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedInput {
    /// Field set of the task (part of the consent key, column `Fields Sent`).
    pub fields: FieldSet,
    /// Values actually sent: only fields of `fields`, text fields redacted.
    pub values: FieldValues,
    /// Replacements for the log column `Redactions`.
    pub redactions: RedactionCounts,
}

impl PreparedInput {
    /// Keep only `task_fields` of `values` and redact them (AI-024 runs before the cache key and before sending).
    pub fn new(values: &FieldValues, task_fields: &FieldSet, redactor: &Redactor) -> Self {
        let mut selected = values.select(task_fields);
        let redactions = redactor.redact_fields(&mut selected);
        Self {
            fields: task_fields.clone(),
            values: selected,
            redactions,
        }
    }

    /// Request parts: one labelled text block, then the images. The cache key (AI-015, `cache::input_hash`) is
    /// computed over these parts, i.e. over the redacted text.
    pub fn inputs(&self) -> Vec<Input> {
        let mut parts = Vec::new();
        let text = self.values.text_block();
        if !text.is_empty() {
            parts.push(Input::Text { text });
        }
        for (media_type, base64) in self.values.images() {
            parts.push(Input::Image {
                media_type: media_type.to_string(),
                base64: base64.to_string(),
            });
        }
        parts
    }

    pub fn consent_key(&self, provider: &str, task: &str) -> ConsentKey {
        ConsentKey::new(provider, task, self.fields.clone())
    }
}

/// Result of a guarded execution with the redaction count of the request (0 if nothing was sent).
#[derive(Debug, Clone, PartialEq)]
pub struct GuardedOutcome {
    pub outcome: Outcome,
    pub redactions: u32,
    /// `true` if the adapter was invoked (a request may have left the machine).
    pub sent: bool,
}

/// Guard order: preview switch (no row), consent (`Pending (consent)`, no request), then routing and adapter.
/// `request.input` is replaced by the prepared input, so nothing outside the consented field set can be sent.
pub async fn execute(
    switch: PreviewSwitch,
    consent: &ConsentGate,
    task: &str,
    input: &PreparedInput,
    provider: &dyn AiProvider,
    mut request: Request,
) -> GuardedOutcome {
    if !switch.is_on() {
        return GuardedOutcome {
            outcome: Outcome::Disabled,
            redactions: 0,
            sent: false,
        };
    }
    let key = input.consent_key(&request.model.provider, task);
    if !consent.check(&key).is_granted() {
        return GuardedOutcome {
            outcome: Outcome::Status(AiStatus::PendingConsent),
            redactions: 0,
            sent: false,
        };
    }
    request.input = input.inputs();
    let routable = request.model.provider == provider.provider_id()
        && request.model.capabilities.supports(request.requirements());
    let outcome = provider::execute(switch, provider, request).await;
    GuardedOutcome {
        outcome,
        redactions: if routable {
            input.redactions.total()
        } else {
            0
        },
        sent: routable,
    }
}
