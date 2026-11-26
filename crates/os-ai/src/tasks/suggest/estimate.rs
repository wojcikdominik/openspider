//! Kostenvoranschlag eines Vorschlagslaufs (AI-012) als Zahlen für den Dialog von M4-U01: je URL ein geplanter
//! Request (Erstrequest; Reparaturrunden sind nicht eingerechnet, sie entstehen nur bei ungültigen Kandidaten).
use super::element::Element;
use super::limits::{ElementLimits, SuggestOptions};
use super::prompt::{self, PromptContext};
use super::run::PageInput;
use super::style::StyleRules;
use crate::catalog::{AnswerTokens, TaskPlan};
use crate::cost::tokens::{char_count, text_tokens};
use crate::cost::{
    estimate_requests, CachedLookup, Calibration, EstimatePath, PlannedEntry, PlannedRequest,
    PriceBasis, RequestEstimateInput, RequestTaskEstimate,
};
use crate::policy::{PreparedInput, Redactor};

/// Eingaben des Voranschlags.
pub struct SuggestEstimate<'a> {
    pub element: Element,
    pub pages: &'a [PageInput],
    pub limits: ElementLimits,
    pub options: SuggestOptions,
    pub plan: &'a TaskPlan,
    pub basis: Option<PriceBasis<'a>>,
    pub api_key_present: bool,
    pub schema_overhead_tokens: u64,
    pub calibration: Calibration,
    pub output_calibration: Calibration,
    pub redactor: &'a Redactor,
    pub style: &'a dyn StyleRules,
}

/// Voranschlag nach `cost::estimate_requests` (Spalten `Items`, `Cached`, Tokens, Kosten, `Reasoning`).
pub fn estimate(input: &SuggestEstimate<'_>, cache: &dyn CachedLookup) -> RequestTaskEstimate {
    let mut requests = Vec::with_capacity(input.pages.len());
    let mut instruction_chars = 0u64;
    for page in input.pages {
        let prepared = PreparedInput::new(
            &page.field_values(),
            &input.element.fields(),
            input.redactor,
        );
        let instructions = prompt::instructions(
            input.element,
            &input.limits,
            &PromptContext {
                candidates: input.options.candidates,
                language: page.language.as_deref(),
                focus: page.focus.as_deref(),
                search_intent: page.search_intent.as_deref(),
                style_lines: input.style.prompt_lines(input.element),
            },
        );
        instruction_chars = instruction_chars.max(char_count(&instructions));
        requests.push(PlannedRequest {
            input_chars: char_count(&prepared.values.text_block()),
            images: 0,
            entries: vec![PlannedEntry {
                cache_key: None,
                question_chars: 0,
                weight: 0,
            }],
        });
    }
    estimate_requests(
        &RequestEstimateInput {
            task: input.element.task().as_str().to_string(),
            model: input.plan.model.id.clone(),
            requests: &requests,
            path: EstimatePath::Prompt {
                instruction_tokens: text_tokens(instruction_chars),
                schema_overhead_tokens: input.schema_overhead_tokens,
                answer: input.plan.answer_tokens.unwrap_or(AnswerTokens::Fixed(512)),
                reasoning: input.plan.reasoning.effective,
            },
            basis: input.basis,
            api_key_present: input.api_key_present,
            calibration: input.calibration,
            output_calibration: input.output_calibration,
            batch_capable: false,
        },
        cache,
    )
}
