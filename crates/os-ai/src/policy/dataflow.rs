//! Data-flow overview „KI-Datenfluss" and the content of the dialog „Datenfluss bestätigen" (AI-018).
//!
//! The UI (M4-U01) renders [`DataFlowOverview::rows`] and [`ConsentRequest`]; this module decides what is shown,
//! when the PII notice appears and how the CSV export looks.
use super::{
    consent::{rfc3339_utc, ConsentCheck, ConsentGate, ConsentKey},
    fields::FieldSet,
    providers::{ProviderPolicies, ProviderPolicy},
};
use serde::{Deserialize, Serialize};

/// CSV/table columns in the order of AI-018.
pub const COLUMNS: [&str; 11] = [
    "Task",
    "Provider",
    "Model",
    "Host",
    "Region",
    "Fields Sent",
    "Max Chars",
    "Images",
    "Redaction",
    "Training Use (per provider policy)",
    "Consent Given At",
];

/// What a configured task sends where (input of the overview; built by the task configuration, D01/U01).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskDataFlow {
    /// Task type, e.g. `suggest-title` or `decision`.
    pub task: String,
    pub provider: String,
    pub model: String,
    /// Endpoint base URL of the provider (host is taken from it).
    pub endpoint: String,
    /// Region if fixed (EU processing is AI-019, release 1.2; until then usually `None`).
    pub region: Option<String>,
    pub fields: FieldSet,
    /// Maximum characters of `page_text` (`None` if not limited or no page text).
    pub max_chars: Option<u32>,
}

impl TaskDataFlow {
    pub fn consent_key(&self) -> ConsentKey {
        ConsentKey::new(&self.provider, &self.task, self.fields.clone())
    }
}

/// One row of the data-flow table (AI-018 output columns).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DataFlowRow {
    pub task: String,
    pub provider: String,
    pub model: String,
    pub host: String,
    pub region: Option<String>,
    pub fields_sent: FieldSet,
    pub max_chars: Option<u32>,
    pub images: bool,
    /// `Redaction` = `on`/`off` (AI-024).
    pub redaction: bool,
    pub training_use: ProviderPolicy,
    pub consent_given_at_ms: Option<i64>,
}

impl DataFlowRow {
    pub fn new(flow: &TaskDataFlow, policies: &ProviderPolicies, redaction: bool) -> Self {
        Self {
            task: flow.task.clone(),
            provider: flow.provider.clone(),
            model: flow.model.clone(),
            host: host_of(&flow.endpoint),
            region: flow.region.clone(),
            fields_sent: flow.fields.clone(),
            max_chars: flow.max_chars,
            images: flow.fields.has_images(),
            redaction,
            training_use: policies.training_use(&flow.provider),
            consent_given_at_ms: None,
        }
    }

    pub fn redaction_cell(&self) -> &'static str {
        if self.redaction {
            "on"
        } else {
            "off"
        }
    }

    /// Notice that pages like contact and imprint may carry personal data sent unredacted: only with redaction
    /// **off** and a field set containing `page_text`, `h2` or `image_context` (AI-018).
    pub fn pii_notice(&self) -> bool {
        !self.redaction && self.fields_sent.may_carry_contact_data()
    }

    pub fn consent_key(&self) -> ConsentKey {
        ConsentKey::new(&self.provider, &self.task, self.fields_sent.clone())
    }

    /// Cells in [`COLUMNS`] order.
    pub fn cells(&self) -> [String; 11] {
        [
            self.task.clone(),
            self.provider.clone(),
            self.model.clone(),
            self.host.clone(),
            self.region.clone().unwrap_or_default(),
            self.fields_sent.labels(),
            self.max_chars.map(|n| n.to_string()).unwrap_or_default(),
            if self.images { "yes" } else { "no" }.into(),
            self.redaction_cell().into(),
            self.training_use.cell(),
            self.consent_given_at_ms
                .map(rfc3339_utc)
                .unwrap_or_default(),
        ]
    }
}

/// Content of the dialog „Datenfluss bestätigen" for one task that needs (renewed) consent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConsentRequest {
    pub row: DataFlowRow,
    /// Fields of the earlier consent of this provider and task, if any.
    pub previous_fields: Option<FieldSet>,
    /// Fields that are new compared with the earlier consent (all fields on first use).
    pub added_fields: FieldSet,
    pub pii_notice: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DataFlowOverview {
    pub rows: Vec<DataFlowRow>,
}

impl DataFlowOverview {
    /// Overview of all configured tasks with the current consent state.
    pub fn build(
        flows: &[TaskDataFlow],
        policies: &ProviderPolicies,
        consent: &ConsentGate,
        redaction: bool,
    ) -> Self {
        let rows = flows
            .iter()
            .map(|flow| {
                let mut row = DataFlowRow::new(flow, policies, redaction);
                if let ConsentCheck::Granted { given_at_ms } = consent.check(&flow.consent_key()) {
                    row.consent_given_at_ms = given_at_ms;
                }
                row
            })
            .collect();
        Self { rows }
    }

    /// Tasks whose next request needs the dialog (no consent, or a field was added).
    pub fn consent_requests(
        flows: &[TaskDataFlow],
        policies: &ProviderPolicies,
        consent: &ConsentGate,
        redaction: bool,
    ) -> Vec<ConsentRequest> {
        flows
            .iter()
            .filter_map(|flow| match consent.check(&flow.consent_key()) {
                ConsentCheck::Granted { .. } => None,
                ConsentCheck::Required { previous, added } => {
                    let row = DataFlowRow::new(flow, policies, redaction);
                    Some(ConsentRequest {
                        pii_notice: row.pii_notice(),
                        row,
                        previous_fields: previous,
                        added_fields: added,
                    })
                }
            })
            .collect()
    }

    /// CSV export for privacy documentation (RFC 4180, `,` separator, CRLF line ends, header row).
    pub fn to_csv(&self) -> String {
        let mut out = String::new();
        push_line(&mut out, COLUMNS.iter().map(|c| c.to_string()));
        for row in &self.rows {
            push_line(&mut out, row.cells().into_iter());
        }
        out
    }
}

fn push_line(out: &mut String, cells: impl Iterator<Item = String>) {
    let line = cells.map(|c| csv_cell(&c)).collect::<Vec<_>>().join(",");
    out.push_str(&line);
    out.push_str("\r\n");
}

fn csv_cell(value: &str) -> String {
    if value.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

/// Host (with non-default port) of an endpoint URL; the input itself if it is no URL.
pub fn host_of(endpoint: &str) -> String {
    match reqwest::Url::parse(endpoint) {
        Ok(url) => match (url.host_str(), url.port()) {
            (Some(host), Some(port)) => format!("{host}:{port}"),
            (Some(host), None) => host.to_string(),
            _ => endpoint.to_string(),
        },
        Err(_) => endpoint.to_string(),
    }
}
