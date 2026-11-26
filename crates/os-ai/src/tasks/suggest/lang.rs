//! Sprachprüfung der Kandidaten (AI-075 Regel 5) mit dem lokalen Detektor `whatlang` (Konfidenz in [0, 1], wie AI-057).
//!
//! Seitensprachen werden als BCP-47-Code geführt (z. B. `de`, `de-DE`, `en`); maßgeblich ist der Primär-Subtag.
//! Der Verstoß `language:<erkannt>` trägt den ISO-639-1-Code der erkannten Sprache, sofern es einen gibt (sonst den
//! ISO-639-3-Code des Detektors).
use whatlang::Lang;

/// Kandidaten unter dieser Länge (Zeichen) werden nicht geprüft.
pub const MIN_CHARS: usize = 20;
/// Mindestkonfidenz für einen Verstoß.
pub const MIN_CONFIDENCE: f64 = 0.95;

/// ISO 639-1 ↔ whatlang (ISO 639-3) für die Sprachen, die der Detektor kennt und die einen zweistelligen Code haben.
const CODES: &[(&str, Lang)] = &[
    ("af", Lang::Afr),
    ("ak", Lang::Aka),
    ("am", Lang::Amh),
    ("ar", Lang::Ara),
    ("az", Lang::Aze),
    ("be", Lang::Bel),
    ("bg", Lang::Bul),
    ("bn", Lang::Ben),
    ("ca", Lang::Cat),
    ("cs", Lang::Ces),
    ("cy", Lang::Cym),
    ("da", Lang::Dan),
    ("de", Lang::Deu),
    ("el", Lang::Ell),
    ("en", Lang::Eng),
    ("eo", Lang::Epo),
    ("es", Lang::Spa),
    ("et", Lang::Est),
    ("fa", Lang::Pes),
    ("fi", Lang::Fin),
    ("fr", Lang::Fra),
    ("gu", Lang::Guj),
    ("he", Lang::Heb),
    ("hi", Lang::Hin),
    ("hr", Lang::Hrv),
    ("hu", Lang::Hun),
    ("hy", Lang::Hye),
    ("id", Lang::Ind),
    ("it", Lang::Ita),
    ("ja", Lang::Jpn),
    ("jv", Lang::Jav),
    ("ka", Lang::Kat),
    ("km", Lang::Khm),
    ("kn", Lang::Kan),
    ("ko", Lang::Kor),
    ("la", Lang::Lat),
    ("lt", Lang::Lit),
    ("lv", Lang::Lav),
    ("mk", Lang::Mkd),
    ("ml", Lang::Mal),
    ("mr", Lang::Mar),
    ("my", Lang::Mya),
    ("ne", Lang::Nep),
    ("nl", Lang::Nld),
    ("nb", Lang::Nob),
    ("no", Lang::Nob),
    ("or", Lang::Ori),
    ("pa", Lang::Pan),
    ("pl", Lang::Pol),
    ("pt", Lang::Por),
    ("ro", Lang::Ron),
    ("ru", Lang::Rus),
    ("si", Lang::Sin),
    ("sk", Lang::Slk),
    ("sl", Lang::Slv),
    ("sn", Lang::Sna),
    ("sr", Lang::Srp),
    ("sv", Lang::Swe),
    ("ta", Lang::Tam),
    ("te", Lang::Tel),
    ("th", Lang::Tha),
    ("tk", Lang::Tuk),
    ("tl", Lang::Tgl),
    ("tr", Lang::Tur),
    ("uk", Lang::Ukr),
    ("ur", Lang::Urd),
    ("uz", Lang::Uzb),
    ("vi", Lang::Vie),
    ("yi", Lang::Yid),
    ("zh", Lang::Cmn),
    ("zu", Lang::Zul),
];

/// Primär-Subtag eines BCP-47-Codes in Kleinbuchstaben (`de-DE` → `de`); leer → `None`.
pub fn primary_subtag(code: &str) -> Option<String> {
    let primary = code.trim().split(['-', '_']).next()?.to_ascii_lowercase();
    (!primary.is_empty()).then_some(primary)
}

fn lang_of(code: &str) -> Option<Lang> {
    let primary = primary_subtag(code)?;
    CODES
        .iter()
        .find(|(c, _)| *c == primary)
        .map(|(_, l)| *l)
        .or_else(|| Lang::from_code(primary.as_str()))
}

fn code_of(lang: Lang) -> String {
    CODES
        .iter()
        .find(|(_, l)| *l == lang)
        .map_or_else(|| lang.code().to_string(), |(c, _)| (*c).to_string())
}

/// Erkannte Sprache eines Textes als ISO-639-1-Code, wenn der Detektor sie mit Konfidenz ≥ `min_confidence` erkennt.
pub fn detect(text: &str, min_confidence: f64) -> Option<String> {
    let info = whatlang::detect(text)?;
    (info.confidence() >= min_confidence).then(|| code_of(info.lang()))
}

/// Verstoß `language:<erkannt>` nach AI-075 (5): nur bei mindestens [`MIN_CHARS`] Zeichen, bekannter Seitensprache
/// und einer **anderen** erkannten Sprache mit Konfidenz ≥ [`MIN_CONFIDENCE`].
pub fn violation(candidate: &str, page_language: Option<&str>) -> Option<String> {
    if candidate.chars().count() < MIN_CHARS {
        return None;
    }
    let page = lang_of(page_language?)?;
    let info = whatlang::detect(candidate)?;
    if info.lang() == page || info.confidence() < MIN_CONFIDENCE {
        return None;
    }
    Some(format!("language:{}", code_of(info.lang())))
}

#[cfg(test)]
mod tests {
    use super::*;

    // spec: AI-075 [M4]
    #[test]
    fn language_rule_needs_twenty_chars_a_known_page_language_and_high_confidence() {
        let german = "Wanderschuhe für Damen und Herren im großen Vergleich mit Testberichten";
        let english = "Hiking boots for women and men compared in our big review with test reports";
        assert_eq!(violation(german, Some("de-DE")), None);
        assert_eq!(violation(english, Some("de")).as_deref(), Some("language:en"));
        assert_eq!(violation(english, None), None, "ohne Seitensprache keine Prüfung");
        assert_eq!(violation("Hiking boots for me", Some("de")), None, "19 Zeichen: keine Prüfung");
        assert_eq!(violation("Hiking boots", Some("de")), None, "kurz");
        assert_eq!(primary_subtag("de-DE").as_deref(), Some("de"));
        assert_eq!(primary_subtag(" "), None);
    }
}
