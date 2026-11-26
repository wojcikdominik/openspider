//! Privacy core (08 `policy`): data-flow overview and consent (AI-018), redaction of personal data before sending
//! (AI-024). EU processing and local-only mode (AI-019) follow in release 1.2 (K4-18); the dialog belongs to M4-U01.
pub mod consent;
pub mod dataflow;
pub mod fields;
pub mod guard;
pub mod providers;
pub mod redact;

pub use consent::{
    ConsentBook, ConsentCheck, ConsentGate, ConsentKey, ConsentMode, ConsentRecord, ConsentSource,
    ConsentStore, MemoryConsentStore, CLI_CONSENT_MISSING,
};
pub use dataflow::{ConsentRequest, DataFlowOverview, DataFlowRow, TaskDataFlow, COLUMNS};
pub use fields::{FieldSet, FieldValue, FieldValues, InputField};
pub use guard::{GuardedOutcome, PreparedInput};
pub use providers::{
    ProviderPolicies, ProviderPolicy, StorageNote, TrainingUse, STORED_OPENAI_RESPONSES,
    STORED_UNKNOWN,
};
pub use redact::{
    FancyRegexEngine, InvalidPattern, PatternEngine, PatternList, RedactionCounts,
    RedactionSettings, Redactor,
};
