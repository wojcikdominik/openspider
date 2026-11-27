//! Eingabeaufbereitung (AI-032): Aus den in `inputs` gewählten Feldern wird je URL bzw. je Bild eine Eingabe gebaut.
//!
//! * Textfelder werden als beschrifteter Textblock (`URL: …`, `Title: …`, …) übergeben ([`FieldValues::text_block`]),
//!   Bilder als Base64 ([`FieldValue::Image`]); nur Felder der Frage gelangen in die Eingabe
//!   ([`FieldValues::select`], AI-018).
//! * `page_text` wird gekürzt: Grundfunktion ist die Kürzung an einer Unicode-Zeichengrenze nach INTEG-172
//!   ([`truncate_chars`], unverändert); ergänzt um die Höchstlänge je Frage (`max_chars`, sonst die INTEG-172-Einstellung
//!   des Modells, Standard 5.000) und die Satzende-Regel im letzten Zwanzigstel ([`truncate_page_text`],
//!   OS-Erweiterung).
//! * Sind alle gewählten Felder leer, wird nichts gesendet: `Skipped (empty input)` (INTEG-173).
//! * Bilder: über 20 MB `Skipped (image too large)`, nicht unterstütztes Format `Skipped (unsupported format)`; längste
//!   Kante über 2.048 px → Verkleinerung auf 2.048 px ([`resized_dimensions`], [`ImageResizer`]).
//! * Fragen mit identischer Eingabe und identischem Ausführungsweg werden gebündelt ([`bundle_key`]).
use os_model::ai::AiStatus;

use super::format::{QuestionDef, Subject};
use crate::adapters::anthropic::{base64_encode, sniff_image_media_type};
use crate::adapters::openai_responses::image::MAX_IMAGE_BYTES;
use crate::policy::{FieldValue, FieldValues, InputField};

/// Standard der INTEG-172-Einstellung „Seiteninhalt begrenzen“ (Zeichen).
pub const DEFAULT_MAX_INPUT_CHARS: u64 = 5_000;
/// Höchstlänge von `image_context` (AI-032).
pub const IMAGE_CONTEXT_MAX_CHARS: usize = 300;
/// Längste Bildkante nach der Verkleinerung (AI-032).
pub const MAX_IMAGE_EDGE: u32 = 2_048;

/// INTEG-172: kürzt auf höchstens `max` Zeichen (Unicode-Skalarwerte) an einer Zeichengrenze. Diese Funktion ist die
/// gemeinsame Grundkürzung; sie selbst kennt keine Satzende-Regel (Paritäts-Prompts aus 04 erhalten genau `max`
/// Zeichen).
pub fn truncate_chars(text: &str, max: usize) -> &str {
    match text.char_indices().nth(max) {
        Some((byte, _)) => &text[..byte],
        None => text,
    }
}

fn is_sentence_end(c: char) -> bool {
    matches!(c, '.' | '!' | '?' | '…')
}

/// AI-032: Kürzung von `page_text` auf `max` Zeichen; liegt im letzten Zwanzigstel vor der Höchstlänge
/// (Schnittlänge zwischen `max − max/20` und `max`) ein Satzende – Satzzeichen `.`, `!`, `?` oder `…`, gefolgt von
/// Leerraum oder Textende –, wird unmittelbar nach dem letzten solchen Satzende geschnitten. Texte bis `max` Zeichen
/// bleiben unverändert.
pub fn truncate_page_text(text: &str, max: usize) -> &str {
    let cut = truncate_chars(text, max);
    if cut.len() == text.len() {
        return text;
    }
    let window_start = max - max / 20;
    let chars: Vec<(usize, char)> = text.char_indices().take(max + 1).collect();
    // Schnittlänge `len` (Zeichen): Zeichen `len − 1` ist Satzzeichen, Zeichen `len` Leerraum.
    for len in (window_start.max(1)..=max).rev() {
        let (_, last) = chars[len - 1];
        let next = chars.get(len).map(|&(_, c)| c);
        if is_sentence_end(last) && next.is_none_or(char::is_whitespace) {
            let end = chars.get(len).map_or(text.len(), |&(b, _)| b);
            return &text[..end];
        }
    }
    cut
}

/// Höchstlänge von `page_text` für eine Frage: `max_chars` der Frage, sonst die INTEG-172-Einstellung des Modells
/// (`max_input_chars` der Aufgabe), sonst 5.000.
pub fn page_text_limit(question: &QuestionDef, model_setting: Option<u64>) -> usize {
    let limit = question
        .page_text_max_chars()
        .map(u64::from)
        .or(model_setting)
        .unwrap_or(DEFAULT_MAX_INPUT_CHARS);
    usize::try_from(limit).unwrap_or(usize::MAX)
}

/// Bilddaten eines Eintrags.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageData {
    /// Medientyp laut Crawl; die Bytes haben Vorrang.
    pub media_type: Option<String>,
    pub bytes: Vec<u8>,
}

/// Felder einer URL bzw. eines Bildes, wie die Crawl-Daten sie liefern (Bindung an die Crawl-Datei: Aufrufer).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SubjectData {
    pub url: String,
    pub title: Option<String>,
    pub meta_description: Option<String>,
    /// Alle H1 in Reihenfolge.
    pub h1: Vec<String>,
    /// Alle H2 in Reihenfolge (durch Zeilenumbruch getrennt gesendet).
    pub h2: Vec<String>,
    /// Text des Inhaltsbereichs (ONPAGE-140; im Rendering-Modus aus dem gerenderten HTML).
    pub page_text: Option<String>,
    pub word_count: Option<u64>,
    pub html_lang: Option<String>,
    pub hreflang_self: Vec<String>,
    pub structured_data_types: Vec<String>,
    pub image: Option<ImageData>,
    pub alt: Option<String>,
    /// Bildunterschrift bzw. umgebender Text (wird auf 300 Zeichen gekürzt).
    pub image_context: Option<String>,
}

/// Verkleinert Bilder (Bytes, Medientyp) auf eine Zielgröße; Rückgabe: neue Bytes und Medientyp. Eine Implementierung
/// mit Bilddekoder fehlt in `os-ai` (keine Abhängigkeit vorhanden, Bericht d01); ohne Resizer wird das Bild unverändert
/// gesendet.
pub trait ImageResizer: Send + Sync {
    fn resize(
        &self,
        bytes: &[u8],
        media_type: &str,
        width: u32,
        height: u32,
    ) -> Option<(Vec<u8>, String)>;
}

/// Zielmaße bei Verkleinerung auf `max` px längste Kante (Seitenverhältnis erhalten, abgerundet): 3.000 × 2.000 →
/// 2.048 × 1.365. Bilder, deren längste Kante `max` nicht übersteigt, bleiben unverändert.
pub fn resized_dimensions(width: u32, height: u32, max: u32) -> (u32, u32) {
    let longest = width.max(height);
    if longest <= max || longest == 0 {
        return (width, height);
    }
    let scale = |v: u32| -> u32 {
        u32::try_from(u64::from(v) * u64::from(max) / u64::from(longest)).unwrap_or(max)
    };
    if width >= height {
        (max, scale(height).max(1))
    } else {
        (scale(width).max(1), max)
    }
}

/// Bildmaße aus dem Dateikopf (PNG, JPEG, GIF, WebP); `None`, wenn nicht lesbar.
pub fn image_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    let be16 = |b: &[u8], i: usize| -> Option<u32> {
        Some(u32::from(u16::from_be_bytes([*b.get(i)?, *b.get(i + 1)?])))
    };
    let le16 = |b: &[u8], i: usize| -> Option<u32> {
        Some(u32::from(u16::from_le_bytes([*b.get(i)?, *b.get(i + 1)?])))
    };
    let be32 = |b: &[u8], i: usize| -> Option<u32> {
        Some(u32::from_be_bytes([
            *b.get(i)?,
            *b.get(i + 1)?,
            *b.get(i + 2)?,
            *b.get(i + 3)?,
        ]))
    };
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Some((be32(bytes, 16)?, be32(bytes, 20)?));
    }
    if bytes.starts_with(b"GIF8") {
        return Some((le16(bytes, 6)?, le16(bytes, 8)?));
    }
    if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
        return match bytes.get(12..16)? {
            b"VP8X" => {
                let w = 1
                    + (u32::from(*bytes.get(24)?)
                        | u32::from(*bytes.get(25)?) << 8
                        | u32::from(*bytes.get(26)?) << 16);
                let h = 1
                    + (u32::from(*bytes.get(27)?)
                        | u32::from(*bytes.get(28)?) << 8
                        | u32::from(*bytes.get(29)?) << 16);
                Some((w, h))
            }
            b"VP8 " => Some((le16(bytes, 26)? & 0x3fff, le16(bytes, 28)? & 0x3fff)),
            b"VP8L" => {
                let b = |i| u32::from(*bytes.get(i).unwrap_or(&0));
                let w = 1 + (((b(22) & 0x3f) << 8) | b(21));
                let h = 1 + (((b(24) & 0xf) << 10) | (b(23) << 2) | ((b(22) & 0xc0) >> 6));
                Some((w, h))
            }
            _ => None,
        };
    }
    if bytes.starts_with(&[0xff, 0xd8]) {
        let mut i = 2;
        while i + 9 < bytes.len() {
            if bytes[i] != 0xff {
                i += 1;
                continue;
            }
            let marker = bytes[i + 1];
            if matches!(marker, 0xc0..=0xc3 | 0xc5..=0xc7 | 0xc9..=0xcb | 0xcd..=0xcf) {
                return Some((be16(bytes, i + 7)?, be16(bytes, i + 5)?));
            }
            if marker == 0xd8 || marker == 0x01 || (0xd0..=0xd7).contains(&marker) {
                i += 2;
                continue;
            }
            i += 2 + usize::try_from(be16(bytes, i + 2)?).ok()?;
        }
    }
    None
}

/// Aufbereitetes Bild: gesendete Bytes (nach einer eventuellen Verkleinerung) und Maße.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedImage {
    pub media_type: String,
    pub bytes: Vec<u8>,
    /// Maße der gesendeten Bytes (laut Dateikopf), falls lesbar.
    pub dimensions: Option<(u32, u32)>,
    /// Zielmaße nach AI-032; weichen sie von `dimensions` ab, fehlte ein [`ImageResizer`].
    pub target: Option<(u32, u32)>,
}

/// Prüft und verkleinert ein Bild (AI-032). Fehler: Status des Eintrags.
pub fn prepare_image(
    image: &ImageData,
    resizer: Option<&dyn ImageResizer>,
) -> Result<PreparedImage, AiStatus> {
    if u64::try_from(image.bytes.len()).unwrap_or(u64::MAX) > MAX_IMAGE_BYTES {
        return Err(AiStatus::SkippedImageTooLarge);
    }
    let media_type = sniff_image_media_type(&image.bytes)
        .map(str::to_string)
        .ok_or(AiStatus::SkippedUnsupportedFormat)?;
    let dimensions = image_dimensions(&image.bytes);
    let target = dimensions.map(|(w, h)| resized_dimensions(w, h, MAX_IMAGE_EDGE));
    if let (Some(dim), Some(tgt), Some(resizer)) = (dimensions, target, resizer) {
        if dim != tgt {
            if let Some((bytes, media)) = resizer.resize(&image.bytes, &media_type, tgt.0, tgt.1) {
                return Ok(PreparedImage {
                    dimensions: image_dimensions(&bytes).or(Some(tgt)),
                    media_type: media,
                    bytes,
                    target,
                });
            }
        }
    }
    Ok(PreparedImage {
        media_type,
        bytes: image.bytes.clone(),
        dimensions,
        target,
    })
}

fn non_empty(text: Option<&str>) -> Option<String> {
    text.map(str::trim)
        .filter(|t| !t.is_empty())
        .map(str::to_string)
}

fn joined(items: &[String]) -> Option<String> {
    let parts: Vec<&str> = items
        .iter()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();
    (!parts.is_empty()).then(|| parts.join("\n"))
}

/// Baut die Feldwerte einer Frage für einen Eintrag (vor der Schwärzung, die `policy::PreparedInput` übernimmt).
/// `page_type` ist das Ergebnis der Seitentyp-Frage für diese URL (nur wenn die Frage das Feld wählt). Nicht gewählte
/// Felder kommen nicht vor. Alle gewählten Felder leer → `Skipped (empty input)`; Bildfehler → deren Status.
pub fn build_values(
    question: &QuestionDef,
    data: &SubjectData,
    page_type: Option<&str>,
    model_max_input_chars: Option<u64>,
    resizer: Option<&dyn ImageResizer>,
) -> Result<FieldValues, AiStatus> {
    let mut values = FieldValues::new();
    let mut any = false;
    for spec in &question.inputs {
        let text = match spec.field {
            InputField::Url => non_empty(Some(&data.url)),
            InputField::Title => non_empty(data.title.as_deref()),
            InputField::MetaDescription => non_empty(data.meta_description.as_deref()),
            InputField::H1 => joined(&data.h1),
            InputField::H2 => joined(&data.h2),
            InputField::PageText => non_empty(data.page_text.as_deref()).map(|t| {
                truncate_page_text(&t, page_text_limit(question, model_max_input_chars)).to_string()
            }),
            InputField::WordCount => data.word_count.map(|n| n.to_string()),
            InputField::HtmlLang => non_empty(data.html_lang.as_deref()),
            InputField::HreflangSelf => joined(&data.hreflang_self),
            InputField::StructuredDataTypes => (!data.structured_data_types.is_empty())
                .then(|| data.structured_data_types.join(", ")),
            InputField::PageType => non_empty(page_type),
            InputField::Alt => non_empty(data.alt.as_deref()),
            InputField::ImageContext => non_empty(data.image_context.as_deref())
                .map(|t| truncate_chars(&t, IMAGE_CONTEXT_MAX_CHARS).to_string()),
            InputField::Image => {
                if let Some(image) = &data.image {
                    let prepared = prepare_image(image, resizer)?;
                    values.insert(
                        InputField::Image,
                        FieldValue::Image {
                            media_type: prepared.media_type,
                            base64: base64_encode(&prepared.bytes),
                        },
                    );
                    any = true;
                }
                continue;
            }
        };
        if let Some(text) = text {
            any = true;
            values.insert(spec.field, FieldValue::Text(text));
        }
    }
    if question.subject == Subject::Image && values.get(InputField::Image).is_none() {
        return Err(AiStatus::SkippedEmptyInput);
    }
    if any {
        Ok(values)
    } else {
        Err(AiStatus::SkippedEmptyInput)
    }
}

/// Bündelungsschlüssel (AI-032): gleiche gesendete Eingabe (Hash der Teile nach Schwärzung) und gleicher
/// Ausführungsweg (Anbieter, Modell, Endpunkt, Weg, Reasoning-Stufe) → ein Request.
pub fn bundle_key(
    input_hash: &str,
    provider: &str,
    model: &str,
    endpoint: &str,
    path: &str,
    reasoning: &str,
) -> String {
    format!(
        "{provider}\u{1f}{model}\u{1f}{endpoint}\u{1f}{path}\u{1f}{reasoning}\u{1f}{input_hash}"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    // spec: AI-032 [M4]
    #[test]
    fn grundkuerzung_cuts_at_character_boundaries() {
        assert_eq!(truncate_chars("äöü", 2), "äö");
        assert_eq!(truncate_chars("abc", 5), "abc");
        assert_eq!(
            truncate_page_text("Eins. Zwei drei", 100),
            "Eins. Zwei drei"
        );
    }

    // spec: AI-032 [M4]
    #[test]
    fn dimensions_follow_the_longest_edge() {
        assert_eq!(resized_dimensions(3000, 2000, 2048), (2048, 1365));
        assert_eq!(resized_dimensions(2000, 3000, 2048), (1365, 2048));
        assert_eq!(resized_dimensions(800, 600, 2048), (800, 600));
    }
}
