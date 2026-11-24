//! Input fields of an AI task and the field set that consent and the data-flow overview refer to (AI-018).
//!
//! Field identifiers follow the list of available inputs in 08 (AI-030). Only fields contained in a task's
//! [`FieldSet`] ever leave [`FieldValues::select`]; this is the guarantee behind "the listed fields are exactly the
//! sent fields" (AI-018).
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// One input field of a page or image task (identifiers as in AI-030).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InputField {
    Url,
    Title,
    MetaDescription,
    H1,
    H2,
    PageText,
    WordCount,
    HtmlLang,
    HreflangSelf,
    StructuredDataTypes,
    PageType,
    Image,
    Alt,
    ImageContext,
}

impl InputField {
    pub const ALL: [InputField; 14] = [
        Self::Url,
        Self::Title,
        Self::MetaDescription,
        Self::H1,
        Self::H2,
        Self::PageText,
        Self::WordCount,
        Self::HtmlLang,
        Self::HreflangSelf,
        Self::StructuredDataTypes,
        Self::PageType,
        Self::Image,
        Self::Alt,
        Self::ImageContext,
    ];

    /// Identifier as used in task definitions (`inputs`) and in the consent key.
    pub fn id(self) -> &'static str {
        match self {
            Self::Url => "url",
            Self::Title => "title",
            Self::MetaDescription => "meta_description",
            Self::H1 => "h1",
            Self::H2 => "h2",
            Self::PageText => "page_text",
            Self::WordCount => "word_count",
            Self::HtmlLang => "html_lang",
            Self::HreflangSelf => "hreflang_self",
            Self::StructuredDataTypes => "structured_data_types",
            Self::PageType => "page_type",
            Self::Image => "image",
            Self::Alt => "alt",
            Self::ImageContext => "image_context",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|f| f.id() == id)
    }

    /// English label for the overview, the CSV export and the labelled text block.
    pub fn label(self) -> &'static str {
        match self {
            Self::Url => "URL",
            Self::Title => "Title",
            Self::MetaDescription => "Meta Description",
            Self::H1 => "H1",
            Self::H2 => "H2",
            Self::PageText => "Page Text",
            Self::WordCount => "Word Count",
            Self::HtmlLang => "HTML Lang",
            Self::HreflangSelf => "Hreflang Self",
            Self::StructuredDataTypes => "Structured Data Types",
            Self::PageType => "Page Type",
            Self::Image => "Image",
            Self::Alt => "Alt",
            Self::ImageContext => "Image Context",
        }
    }

    /// Text fields that AI-024 redacts before sending.
    pub fn is_redacted(self) -> bool {
        matches!(
            self,
            Self::PageText
                | Self::H2
                | Self::ImageContext
                | Self::MetaDescription
                | Self::Title
                | Self::H1
        )
    }

    /// Fields that trigger the PII notice when redaction is off (AI-018).
    pub fn may_carry_contact_data(self) -> bool {
        matches!(self, Self::PageText | Self::H2 | Self::ImageContext)
    }
}

/// Set of fields a task sends; part of the consent key (AI-018). Ordered, so equal sets compare and serialise equally.
#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct FieldSet(BTreeSet<InputField>);

impl FieldSet {
    pub fn new(fields: impl IntoIterator<Item = InputField>) -> Self {
        Self(fields.into_iter().collect())
    }

    /// Parse task-definition identifiers; unknown identifiers are returned as error.
    pub fn from_ids<'a>(ids: impl IntoIterator<Item = &'a str>) -> Result<Self, String> {
        ids.into_iter()
            .map(|id| InputField::from_id(id).ok_or_else(|| id.to_string()))
            .collect::<Result<BTreeSet<_>, _>>()
            .map(Self)
    }

    pub fn contains(&self, field: InputField) -> bool {
        self.0.contains(&field)
    }

    pub fn insert(&mut self, field: InputField) -> bool {
        self.0.insert(field)
    }

    pub fn iter(&self) -> impl Iterator<Item = InputField> + '_ {
        self.0.iter().copied()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// `true` if every field of `other` is contained in `self`.
    pub fn covers(&self, other: &FieldSet) -> bool {
        other.0.is_subset(&self.0)
    }

    /// Fields of `self` that `consented` does not contain (the reason for asking again).
    pub fn added_to(&self, consented: &FieldSet) -> FieldSet {
        Self(self.0.difference(&consented.0).copied().collect())
    }

    pub fn ids(&self) -> Vec<&'static str> {
        self.iter().map(InputField::id).collect()
    }

    /// Labels joined with `", "` (column `Fields Sent`).
    pub fn labels(&self) -> String {
        self.iter()
            .map(InputField::label)
            .collect::<Vec<_>>()
            .join(", ")
    }

    pub fn has_images(&self) -> bool {
        self.contains(InputField::Image)
    }

    pub fn may_carry_contact_data(&self) -> bool {
        self.iter().any(InputField::may_carry_contact_data)
    }
}

impl FromIterator<InputField> for FieldSet {
    fn from_iter<T: IntoIterator<Item = InputField>>(iter: T) -> Self {
        Self::new(iter)
    }
}

/// Value of one field as extracted by the caller (D01 builds them from the crawl; INTEG-172 truncation is applied
/// there before redaction).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum FieldValue {
    Text(String),
    /// Base64 image data (data URL or raw base64); never redacted, never part of the text block.
    Image {
        media_type: String,
        base64: String,
    },
}

/// Field values of one entry (URL or image).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct FieldValues(BTreeMap<InputField, FieldValue>);

impl FieldValues {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_text(mut self, field: InputField, text: impl Into<String>) -> Self {
        self.0.insert(field, FieldValue::Text(text.into()));
        self
    }

    pub fn insert(&mut self, field: InputField, value: FieldValue) {
        self.0.insert(field, value);
    }

    pub fn get(&self, field: InputField) -> Option<&FieldValue> {
        self.0.get(&field)
    }

    pub fn text(&self, field: InputField) -> Option<&str> {
        match self.0.get(&field) {
            Some(FieldValue::Text(text)) => Some(text),
            _ => None,
        }
    }

    pub fn fields(&self) -> FieldSet {
        self.0.keys().copied().collect()
    }

    /// Keep only the fields of `set`; every other value is dropped and cannot reach a request (AI-018).
    pub fn select(&self, set: &FieldSet) -> FieldValues {
        Self(
            self.0
                .iter()
                .filter(|(field, _)| set.contains(**field))
                .map(|(field, value)| (*field, value.clone()))
                .collect(),
        )
    }

    pub(crate) fn texts_mut(&mut self) -> impl Iterator<Item = (InputField, &mut String)> {
        self.0.iter_mut().filter_map(|(field, value)| match value {
            FieldValue::Text(text) => Some((*field, text)),
            FieldValue::Image { .. } => None,
        })
    }

    /// Labelled text block (`URL: …`, `Title: …`), in field order, without image data (AI-030 format; D01 may
    /// replace the formatting, the field selection stays here).
    pub fn text_block(&self) -> String {
        self.0
            .iter()
            .filter_map(|(field, value)| match value {
                FieldValue::Text(text) => Some(format!("{}: {}", field.label(), text)),
                FieldValue::Image { .. } => None,
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Image parts in field order.
    pub fn images(&self) -> Vec<(&str, &str)> {
        self.0
            .values()
            .filter_map(|value| match value {
                FieldValue::Image { media_type, base64 } => {
                    Some((media_type.as_str(), base64.as_str()))
                }
                FieldValue::Text(_) => None,
            })
            .collect()
    }
}
