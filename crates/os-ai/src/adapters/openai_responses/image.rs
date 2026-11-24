//! Bildeingaben der OpenAI-Adapter: ausschließlich als `data:<mime>;base64,…` (AI-003, AI-004), nie als HTTP-URL
//! oder `file_id`. Bilder, die nicht als Data-URL vorliegen, werden vor dem Senden geladen und kodiert; Bilder über
//! 20 MB werden nicht gesendet (`Skipped (image too large)`, AI-004).
use os_model::ai::AiStatus;

use crate::limits::AttemptFailure;

/// Größte zulässige Bildgröße in Byte (dekodiert): 20 MB = 20 × 1024 × 1024 Byte (Auslegung, siehe Bericht).
pub const MAX_IMAGE_BYTES: u64 = 20 * 1024 * 1024;

/// Unterstützte Bildformate (PNG, JPEG, WebP, GIF).
pub const SUPPORTED_MEDIA_TYPES: [&str; 4] = ["image/png", "image/jpeg", "image/webp", "image/gif"];

/// Herkunft eines Bildes vor der Aufbereitung.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImageSource {
    /// Bereits eine Data-URL.
    DataUrl(String),
    /// Base64-kodierte Bytes mit Medientyp.
    Base64 { media_type: String, base64: String },
    /// Rohe Bytes; der Medientyp wird aus den Bytes erkannt, sonst aus `media_type`.
    Bytes {
        media_type: Option<String>,
        bytes: Vec<u8>,
    },
    /// HTTP(S)-Adresse: wird vor dem Senden geladen und als Data-URL kodiert.
    Url(String),
}

/// Sendefertiges Bild.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedImage {
    media_type: String,
    data_url: String,
    bytes: u64,
}

impl PreparedImage {
    pub fn media_type(&self) -> &str {
        &self.media_type
    }
    /// `data:<mime>;base64,…`
    pub fn data_url(&self) -> &str {
        &self.data_url
    }
    /// Dekodierte Größe in Byte.
    pub fn bytes(&self) -> u64 {
        self.bytes
    }
}

/// Grund, aus dem ein Bild nicht gesendet wird.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImageError {
    /// Über [`MAX_IMAGE_BYTES`] → `Skipped (image too large)`.
    TooLarge { bytes: u64 },
    /// Kein unterstütztes Bildformat bzw. keine gültige Base64-Kodierung → `Skipped (unsupported format)`.
    UnsupportedFormat { detail: String },
    /// Laden fehlgeschlagen (HTTP-Fehler, Verbindung).
    Load(AttemptFailure),
}

impl ImageError {
    /// Status der betroffenen Einträge (AI-020).
    pub fn status(&self) -> AiStatus {
        match self {
            Self::TooLarge { .. } => AiStatus::SkippedImageTooLarge,
            Self::UnsupportedFormat { .. } => AiStatus::SkippedUnsupportedFormat,
            Self::Load(failure) => failure.error.status().unwrap_or(AiStatus::ConnectionFailed),
        }
    }
}

/// Bereitet ein Bild auf: Data-URL prüfen bzw. Bytes laden und kodieren, Größe und Format prüfen.
pub async fn prepare_image(
    client: &reqwest::Client,
    source: ImageSource,
) -> Result<PreparedImage, ImageError> {
    match source {
        ImageSource::DataUrl(url) => from_data_url(&url),
        ImageSource::Base64 { media_type, base64 } => from_base64(&media_type, &base64),
        ImageSource::Bytes { media_type, bytes } => from_bytes(media_type.as_deref(), &bytes),
        ImageSource::Url(url) => {
            let (media_type, bytes) = load(client, &url).await?;
            from_bytes(media_type.as_deref(), &bytes)
        }
    }
}

/// Prüft eine Data-URL (`data:<mime>;base64,<daten>`).
pub fn from_data_url(url: &str) -> Result<PreparedImage, ImageError> {
    let unsupported = |detail: &str| ImageError::UnsupportedFormat {
        detail: detail.to_string(),
    };
    let rest = url
        .strip_prefix("data:")
        .ok_or_else(|| unsupported("not a data URL"))?;
    let (meta, data) = rest
        .split_once(',')
        .ok_or_else(|| unsupported("data URL without data"))?;
    let media_type = meta
        .strip_suffix(";base64")
        .ok_or_else(|| unsupported("data URL is not base64 encoded"))?;
    from_base64(media_type, data)
}

/// Prüft Base64-Daten mit Medientyp.
pub fn from_base64(media_type: &str, base64: &str) -> Result<PreparedImage, ImageError> {
    let base64 = base64.trim();
    let bytes = decoded_len(base64).ok_or_else(|| ImageError::UnsupportedFormat {
        detail: "invalid base64 data".into(),
    })?;
    if bytes > MAX_IMAGE_BYTES {
        return Err(ImageError::TooLarge { bytes });
    }
    let media_type = media_type.trim().to_ascii_lowercase();
    check_media_type(&media_type)?;
    Ok(PreparedImage {
        data_url: format!("data:{media_type};base64,{base64}"),
        media_type,
        bytes,
    })
}

/// Kodiert rohe Bytes.
pub fn from_bytes(media_type: Option<&str>, bytes: &[u8]) -> Result<PreparedImage, ImageError> {
    let len = bytes.len() as u64;
    if len > MAX_IMAGE_BYTES {
        return Err(ImageError::TooLarge { bytes: len });
    }
    let media_type = sniff_media_type(bytes)
        .map(str::to_string)
        .or_else(|| {
            media_type.map(|m| {
                m.split(';')
                    .next()
                    .unwrap_or("")
                    .trim()
                    .to_ascii_lowercase()
            })
        })
        .unwrap_or_default();
    check_media_type(&media_type)?;
    Ok(PreparedImage {
        data_url: format!("data:{media_type};base64,{}", encode_base64(bytes)),
        media_type,
        bytes: len,
    })
}

fn check_media_type(media_type: &str) -> Result<(), ImageError> {
    if SUPPORTED_MEDIA_TYPES.contains(&media_type) {
        Ok(())
    } else {
        Err(ImageError::UnsupportedFormat {
            detail: format!("unsupported image type {media_type:?}"),
        })
    }
}

/// Lädt ein Bild; bricht ab, sobald mehr als [`MAX_IMAGE_BYTES`] angekündigt oder empfangen wurden.
async fn load(
    client: &reqwest::Client,
    url: &str,
) -> Result<(Option<String>, Vec<u8>), ImageError> {
    let mut response = client
        .get(url)
        .send()
        .await
        .map_err(|e| ImageError::Load(AttemptFailure::from_reqwest(&e)))?;
    let status = response.status();
    if !status.is_success() {
        return Err(ImageError::Load(AttemptFailure::from(
            crate::provider::ProviderError::Http {
                status: status.as_u16(),
                message: status.canonical_reason().unwrap_or("").to_string(),
            },
        )));
    }
    if let Some(len) = response.content_length() {
        if len > MAX_IMAGE_BYTES {
            return Err(ImageError::TooLarge { bytes: len });
        }
    }
    let media_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|e| ImageError::Load(AttemptFailure::from_reqwest(&e)))?
    {
        bytes.extend_from_slice(&chunk);
        if bytes.len() as u64 > MAX_IMAGE_BYTES {
            return Err(ImageError::TooLarge {
                bytes: bytes.len() as u64,
            });
        }
    }
    Ok((media_type, bytes))
}

/// Medientyp aus den ersten Bytes (PNG, JPEG, GIF, WebP).
pub fn sniff_media_type(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("image/png")
    } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some("image/jpeg")
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        Some("image/gif")
    } else if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some("image/webp")
    } else {
        None
    }
}

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Base64 (Standardalphabet, mit Auffüllung).
pub fn encode_base64(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(char::from(ALPHABET[((n >> (18 - 6 * i)) & 63) as usize]));
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// Dekodierte Länge gültiger Base64-Daten (Standardalphabet, Länge Vielfaches von 4, höchstens zwei `=` am Ende);
/// `None` bei ungültigen Daten.
pub fn decoded_len(base64: &str) -> Option<u64> {
    let bytes = base64.as_bytes();
    if bytes.is_empty() || !bytes.len().is_multiple_of(4) {
        return None;
    }
    let padding = bytes.iter().rev().take_while(|b| **b == b'=').count();
    if padding > 2 {
        return None;
    }
    let body = &bytes[..bytes.len() - padding];
    if !body
        .iter()
        .all(|b| b.is_ascii_alphanumeric() || *b == b'+' || *b == b'/')
    {
        return None;
    }
    Some((bytes.len() as u64 / 4) * 3 - padding as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_roundtrip_lengths() {
        assert_eq!(encode_base64(b""), "");
        assert_eq!(encode_base64(b"f"), "Zg==");
        assert_eq!(encode_base64(b"fo"), "Zm8=");
        assert_eq!(encode_base64(b"foo"), "Zm9v");
        assert_eq!(encode_base64(b"foobar"), "Zm9vYmFy");
        assert_eq!(encode_base64(&[0xFB, 0xFF]), "+/8=");
        for n in 1..20usize {
            let data = vec![7u8; n];
            assert_eq!(decoded_len(&encode_base64(&data)), Some(n as u64));
        }
        assert_eq!(decoded_len("Zm9v!A=="), None);
        assert_eq!(decoded_len("Zm9"), None);
    }
}
