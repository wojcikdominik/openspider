//! Token-Schätzung aus der Zeichenzahl der aufbereiteten Eingabe (AI-012).

/// ⌈Zeichen / 3,5⌉ für Text (Unicode-Zeichen, nicht Bytes).
pub fn text_tokens(chars: u64) -> u64 {
    // ⌈c / 3,5⌉ = ⌈2c / 7⌉
    (u128::from(chars) * 2).div_ceil(7) as u64
}

/// Zeichenzahl eines Textes in Unicode-Zeichen.
pub fn char_count(text: &str) -> u64 {
    text.chars().count() as u64
}

/// Geschätzte Eingabe-Tokens eines Requests ohne Korrekturfaktor:
/// ⌈Zeichen / 3,5⌉ + Bild-Schätzwert × Bilder + fester Anteil für Anweisungen.
pub fn estimate_input_tokens(
    chars: u64,
    images: u64,
    image_token_estimate: u64,
    instruction_tokens: u64,
) -> u64 {
    text_tokens(chars)
        .saturating_add(images.saturating_mul(image_token_estimate))
        .saturating_add(instruction_tokens)
}
