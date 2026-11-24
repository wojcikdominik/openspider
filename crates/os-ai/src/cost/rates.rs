//! Preissätze in ganzzahligen Einheiten (AI-010 als Eingabe der Kostenformel).
//!
//! Tokenpreise in Piko-USD je Token (1 USD je 1 Mio. Tokens = 1 µUSD je Token = 1.000.000 pUSD je Token), damit
//! Preise wie 0,01 USD je 1 Mio. Tokens ohne Gleitkomma exakt bleiben. Faktoren und Anteile in ppm
//! (1.000.000 = 1,0 bzw. 100 %). Den Katalog (Preistabelle, OpenRouter-API, Nutzerüberschreibung, Region) liest
//! M4-K04 und übergibt die aufgelösten Sätze für (Anbieter, Modell, Endpunkt, Basis-URL).
pub use os_store::ai::requests::PriceSource;

/// 1,0 als ppm-Faktor.
pub const PPM: u64 = 1_000_000;
/// Piko-USD je Mikro-USD.
pub const PICO_PER_MICRO: u64 = 1_000_000;

/// Langkontext-Preise ab einer Schwelle an Eingabe-Tokens (einschließlich), getrennte Faktoren.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LongContext {
    pub threshold_input_tokens: u64,
    pub input_factor_ppm: u64,
    pub output_factor_ppm: u64,
    pub cache_read_factor_ppm: u64,
}

/// Aufgelöste Preissätze eines Endpunkts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rates {
    pub input_pico: u64,
    pub output_pico: u64,
    pub cache_read_pico: u64,
    pub cache_write_pico: u64,
    /// Batch-Rabatt auf die Tokenkosten (500.000 = 50 %).
    pub batch_discount_ppm: u64,
    pub long_context: Option<LongContext>,
    /// Regionalzuschlag für die verwendete Basis-URL (100.000 = 10 %), auf Tokenkosten + Gebühren.
    pub regional_uplift_ppm: u64,
    pub per_request_pico: u64,
    pub per_image_pico: u64,
    /// Bild-Token-Schätzwert der Preistabelle (AI-012).
    pub image_token_estimate: u64,
    pub source: PriceSource,
}

impl Rates {
    /// Nur Eingabe-/Ausgabepreis in USD je 1 Mio. Tokens (Dezimaltext), alles andere 0.
    pub fn per_million(input_usd: &str, output_usd: &str, source: PriceSource) -> Option<Self> {
        Some(Self {
            input_pico: usd_per_million_to_pico(input_usd)?,
            output_pico: usd_per_million_to_pico(output_usd)?,
            ..Self::free(source)
        })
    }
    /// Kostenlose Sätze (z. B. Ollama).
    pub fn free(source: PriceSource) -> Self {
        Self {
            input_pico: 0,
            output_pico: 0,
            cache_read_pico: 0,
            cache_write_pico: 0,
            batch_discount_ppm: 0,
            long_context: None,
            regional_uplift_ppm: 0,
            per_request_pico: 0,
            per_image_pico: 0,
            image_token_estimate: 0,
            source,
        }
    }
    /// Langkontext-Faktoren, falls `input_tokens` die Schwelle erreicht.
    pub fn long_context_for(&self, input_tokens: u64) -> Option<&LongContext> {
        self.long_context
            .as_ref()
            .filter(|lc| input_tokens >= lc.threshold_input_tokens)
    }
}

/// Dezimaltext (auch mit Exponent, z. B. `1e-6`) × 10^`digits`, kaufmännisch gerundet. `None` bei negativen
/// oder ungültigen Werten und bei Überlauf.
pub fn parse_decimal_scaled(text: &str, digits: u32) -> Option<u64> {
    let text = text.trim();
    let (mantissa, exp) = match text.find(['e', 'E']) {
        Some(i) => (&text[..i], text[i + 1..].parse::<i32>().ok()?),
        None => (text, 0),
    };
    let mantissa = mantissa.strip_prefix('+').unwrap_or(mantissa);
    if mantissa.starts_with('-') || mantissa.is_empty() {
        return None;
    }
    let (int, frac) = mantissa.split_once('.').unwrap_or((mantissa, ""));
    if !int.chars().chain(frac.chars()).all(|c| c.is_ascii_digit())
        || (int.is_empty() && frac.is_empty())
    {
        return None;
    }
    let digits_all: String = format!("{int}{frac}");
    // Wert = digits_all × 10^(exp − len(frac) + digits)
    let shift = i64::from(exp) - frac.len() as i64 + i64::from(digits);
    let mut value: u128 = 0;
    for c in digits_all.chars() {
        value = value
            .checked_mul(10)?
            .checked_add(u128::from(c.to_digit(10)?))?;
    }
    let result = if shift >= 0 {
        value.checked_mul(10u128.checked_pow(u32::try_from(shift).ok()?)?)?
    } else {
        let div = 10u128.checked_pow(u32::try_from(-shift).ok()?);
        match div {
            Some(div) => (value + div / 2) / div,
            None => 0,
        }
    };
    u64::try_from(result).ok()
}

/// USD je 1 Mio. Tokens → Piko-USD je Token.
pub fn usd_per_million_to_pico(text: &str) -> Option<u64> {
    parse_decimal_scaled(text, 6)
}
/// USD je Token (OpenRouter `pricing.prompt`) bzw. USD je Request/Bild → Piko-USD.
pub fn usd_to_pico(text: &str) -> Option<u64> {
    parse_decimal_scaled(text, 12)
}
/// Anteil bzw. Faktor (z. B. `0.5`, `2`, `0.1`) → ppm.
pub fn fraction_to_ppm(text: &str) -> Option<u64> {
    parse_decimal_scaled(text, 6)
}
