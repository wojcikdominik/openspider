//! User-Agent-Voreinstellungen (CRAWL-114).
//!
//! Die Liste liegt als Datendatei vor (`resources/user-agents.toml`, zur Übersetzungszeit eingebettet)
//! und kann ohne Code-Änderung durch eine Datei gleichen Aufbaus ersetzt werden
//! ([`parse_presets`]). Getrennt vom HTTP-User-Agent gibt es das robots.txt-Token, unter dem
//! `robots.txt`-Gruppen und agentenspezifische Meta-Robots ausgewertet werden.
//!
//! [M4] **KI-Crawler aus dem Katalog (CRAWL-114 [M4], AI-120):** Die Voreinstellungen für KI-Crawler werden aus
//! `resources/ai/ai-crawlers.json` gespeist ([`merge_ai_presets`]): jeder nicht veraltete Eintrag der Kategorien
//! `search` und `user-fetch` sowie `GPTBot`, `ClaudeBot` und `Google-Extended` erscheint als Voreinstellung
//! (HTTP-User-Agent = `ua_example`, robots-Token = `token`), neue Einträge in der Gruppe „KI-Crawler“. Feste
//! Einträge mit einem Katalog-Token übernehmen Token und `ua_example` aus dem Katalog und erscheinen nicht doppelt.

use std::sync::OnceLock;

use os_model::UserAgentConfig;

/// Eingebettete Voreinstellungen.
const EMBEDDED: &str = include_str!("../resources/user-agents.toml");

/// Eingebetteter AI-Crawler-Katalog (AI-120; Aktualisierung mit App-Updates).
const EMBEDDED_AI_CATALOG: &str = include_str!("../../../resources/ai/ai-crawlers.json");

/// Gruppe der aus dem Katalog ergänzten Voreinstellungen.
pub const AI_PRESET_GROUP: &str = "KI-Crawler";

/// Tokens, die unabhängig von Kategorie und Status als Voreinstellung erscheinen (CRAWL-114 [M4]).
pub const AI_PRESET_EXTRA_TOKENS: [&str; 3] = ["GPTBot", "ClaudeBot", "Google-Extended"];

/// Kennung der Standard-Voreinstellung.
pub const DEFAULT_PRESET: &str = "openspider";
/// Kennung der benutzerdefinierten Voreinstellung.
pub const CUSTOM_PRESET: &str = "custom";
/// Standard-Token der robots.txt-Gruppe (K-12).
pub const DEFAULT_ROBOTS_TOKEN: &str = "OpenSpider";
/// Token, wenn kein eigenes gesetzt ist: nur die Gruppe `User-agent: *` greift.
pub const FALLBACK_ROBOTS_TOKEN: &str = "*";

/// Eine Voreinstellung.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserAgentPreset {
    /// Stabile Kennung (`user_agent.preset`).
    pub id: String,
    /// Anzeigename.
    pub name: String,
    /// Gruppe in der Auswahlliste.
    pub group: String,
    /// HTTP-User-Agent (`{version}` ersetzt); `None` bei „nur Token“ bzw. benutzerdefiniert.
    pub user_agent: Option<String>,
    /// robots.txt-Token; `*` = kein eigenes Token.
    pub robots_token: String,
}

/// Aufgelöster User-Agent einer Konfiguration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedUserAgent {
    /// HTTP-User-Agent-String (immer gesetzt).
    pub http: String,
    /// robots.txt-Token.
    pub robots_token: String,
    /// Hinweis für Protokoll und Oberfläche, z. B. „kein Token gesetzt“.
    pub warning: Option<String>,
}

/// Der Standard-User-Agent `Mozilla/5.0 (compatible; OpenSpider/<Version>; +https://openspider.de)`.
pub fn default_user_agent() -> String {
    format!(
        "Mozilla/5.0 (compatible; OpenSpider/{}; +https://openspider.de)",
        env!("CARGO_PKG_VERSION")
    )
}

/// Die eingebetteten Voreinstellungen samt KI-Crawlern aus dem eingebetteten Katalog (einmal geparst).
pub fn presets() -> &'static [UserAgentPreset] {
    static CELL: OnceLock<Vec<UserAgentPreset>> = OnceLock::new();
    CELL.get_or_init(|| {
        presets_with_catalogs(EMBEDDED, &[EMBEDDED_AI_CATALOG]).expect(
            "eingebettete user-agents.toml und ai-crawlers.json sind gültig (Test deckt das ab)",
        )
    })
}

/// Der eingebettete AI-Crawler-Katalog (Text von `resources/ai/ai-crawlers.json`).
pub fn embedded_ai_catalog() -> &'static str {
    EMBEDDED_AI_CATALOG
}

/// Voreinstellungen aus einer Voreinstellungsdatei und einem oder mehreren Katalogen (mitgelieferter Katalog,
/// danach Benutzerkataloge; bei gleichem Token gilt der erste Eintrag). So erscheint ein im Katalog ergänzter
/// Eintrag beim nächsten Start ohne Code-Änderung.
pub fn presets_with_catalogs(
    presets_toml: &str,
    catalogs: &[&str],
) -> Result<Vec<UserAgentPreset>, String> {
    let base = parse_presets(presets_toml)?;
    let mut crawlers: Vec<AiCatalogCrawler> = Vec::new();
    for (n, text) in catalogs.iter().enumerate() {
        for c in parse_ai_catalog(text).map_err(|e| format!("Katalog {}: {e}", n + 1))? {
            if !crawlers
                .iter()
                .any(|known| known.token.eq_ignore_ascii_case(&c.token))
            {
                crawlers.push(c);
            }
        }
    }
    Ok(merge_ai_presets(base, &crawlers))
}

/// Die für Voreinstellungen nötigen Felder eines Katalogeintrags (AI-120).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiCatalogCrawler {
    /// robots.txt-Token.
    pub token: String,
    /// `training`, `search`, `user-fetch` oder `control-token`.
    pub category: String,
    /// Beispiel-User-Agent der Betreiberdokumentation.
    pub ua_example: Option<String>,
    /// Veraltet.
    pub deprecated: bool,
}

impl AiCatalogCrawler {
    /// Gehört der Eintrag zu den KI-Crawler-Voreinstellungen (CRAWL-114 [M4])?
    pub fn is_preset(&self) -> bool {
        (!self.deprecated && matches!(self.category.as_str(), "search" | "user-fetch"))
            || AI_PRESET_EXTRA_TOKENS
                .iter()
                .any(|t| t.eq_ignore_ascii_case(&self.token))
    }
}

/// Liest die Einträge eines AI-Crawler-Katalogs (`{"schema_version": 1, "crawlers": [...]}`). Geprüft werden nur
/// die hier benötigten Felder; die vollständige Prüfung nach AI-120 liegt in `os_analyze::ai_search::catalog`.
pub fn parse_ai_catalog(text: &str) -> Result<Vec<AiCatalogCrawler>, String> {
    let root = json::parse(text)?;
    let crawlers = root
        .get("crawlers")
        .and_then(json::Value::as_array)
        .ok_or("Feld `crawlers` fehlt oder ist keine Liste")?;
    crawlers
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let text_field = |key: &str| -> Result<Option<String>, String> {
                match c.get(key) {
                    None | Some(json::Value::Null) => Ok(None),
                    Some(json::Value::String(s)) => Ok(Some(s.clone())),
                    Some(_) => Err(format!("Eintrag {i}: `{key}` ist keine Zeichenkette")),
                }
            };
            let token = text_field("token")?
                .filter(|t| !t.trim().is_empty())
                .ok_or_else(|| format!("Eintrag {i}: `token` fehlt"))?;
            let category =
                text_field("category")?.ok_or_else(|| format!("Eintrag {i}: `category` fehlt"))?;
            let deprecated = match c.get("deprecated") {
                None | Some(json::Value::Null) => false,
                Some(json::Value::Bool(b)) => *b,
                Some(_) => return Err(format!("Eintrag {i}: `deprecated` ist kein Wahrheitswert")),
            };
            Ok(AiCatalogCrawler {
                token,
                category,
                ua_example: text_field("ua_example")?.filter(|s| !s.trim().is_empty()),
                deprecated,
            })
        })
        .collect()
}

/// Ergänzt die KI-Crawler-Voreinstellungen aus dem Katalog (CRAWL-114 [M4]).
///
/// * Feste Einträge mit einem Katalog-Token (gleiches robots-Token, ohne Rücksicht auf Groß-/Kleinschreibung)
///   übernehmen Token und – falls vorhanden – `ua_example` des Katalogs; Kennung, Name und Gruppe bleiben.
///   Ohne `ua_example` im Katalog bleibt der HTTP-User-Agent des festen Eintrags.
/// * Jeder übrige Katalogeintrag der Auswahl wird als neue Voreinstellung (Kennung = Token in Kleinbuchstaben,
///   Name = Token, Gruppe „KI-Crawler“) hinter dem letzten Eintrag der Gruppe „KI-Crawler“ eingefügt (sonst vor
///   „Benutzerdefiniert“); ohne `ua_example` ist sie „nur Token“.
pub fn merge_ai_presets(
    mut base: Vec<UserAgentPreset>,
    crawlers: &[AiCatalogCrawler],
) -> Vec<UserAgentPreset> {
    let mut added = Vec::new();
    for c in crawlers.iter().filter(|c| c.is_preset()) {
        if let Some(fixed) = base
            .iter_mut()
            .find(|p| p.robots_token.eq_ignore_ascii_case(&c.token))
        {
            fixed.robots_token = c.token.clone();
            if let Some(ua) = &c.ua_example {
                fixed.user_agent = Some(ua.clone());
            }
            continue;
        }
        let id = c.token.to_ascii_lowercase();
        if base
            .iter()
            .chain(added.iter())
            .any(|p: &UserAgentPreset| p.id == id)
        {
            continue;
        }
        added.push(UserAgentPreset {
            id,
            name: c.token.clone(),
            group: AI_PRESET_GROUP.to_string(),
            user_agent: c.ua_example.clone(),
            robots_token: c.token.clone(),
        });
    }
    let at = base
        .iter()
        .rposition(|p| p.group == AI_PRESET_GROUP)
        .map(|i| i + 1)
        .or_else(|| base.iter().position(|p| p.id == CUSTOM_PRESET))
        .unwrap_or(base.len());
    base.splice(at..at, added);
    base
}

/// Die Voreinstellungen, deren robots-Token im Katalog steht (= KI-Crawler-Voreinstellungen, CRAWL-114 [M4]).
pub fn ai_presets<'a>(
    list: &'a [UserAgentPreset],
    crawlers: &[AiCatalogCrawler],
) -> Vec<&'a UserAgentPreset> {
    list.iter()
        .filter(|p| {
            crawlers
                .iter()
                .any(|c| c.token.eq_ignore_ascii_case(&p.robots_token))
        })
        .collect()
}

/// Sucht eine Voreinstellung nach Kennung oder Anzeigename (Groß-/Kleinschreibung und Trennzeichen egal).
pub fn find_preset<'a>(
    presets: &'a [UserAgentPreset],
    id_or_name: &str,
) -> Option<&'a UserAgentPreset> {
    let wanted = squash(id_or_name);
    if wanted.is_empty() {
        return None;
    }
    presets
        .iter()
        .find(|p| squash(&p.id) == wanted)
        .or_else(|| presets.iter().find(|p| squash(&p.name) == wanted))
}

fn squash(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

/// Bestimmt HTTP-User-Agent und robots-Token aus der Konfiguration.
///
/// - HTTP-User-Agent: `user_agent.http`, sonst der der Voreinstellung, sonst der Standard.
/// - Token: `user_agent.robots_token`, sonst das der Voreinstellung. Ein frei eingegebener User-Agent
///   (Voreinstellung `custom` oder unbekannt) ohne Token verwendet `*` und liefert eine Warnung.
pub fn resolve(cfg: &UserAgentConfig) -> ResolvedUserAgent {
    resolve_with(cfg, presets())
}

/// Wie [`resolve`] mit einer eigenen Voreinstellungsliste.
pub fn resolve_with(cfg: &UserAgentConfig, list: &[UserAgentPreset]) -> ResolvedUserAgent {
    let preset = find_preset(list, &cfg.preset);
    let custom_http = cfg
        .http
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    let explicit_token = cfg
        .robots_token
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);

    let http = custom_http
        .clone()
        .or_else(|| preset.and_then(|p| p.user_agent.clone()))
        .unwrap_or_else(default_user_agent);

    let mut warning = None;
    if preset.is_none() && custom_http.is_none() {
        warning = Some(format!(
            "unbekannte User-Agent-Voreinstellung {:?}, es gilt der Standard",
            cfg.preset
        ));
    }
    let robots_token = if let Some(t) = explicit_token {
        t
    } else if let Some(p) = preset.filter(|p| p.id != CUSTOM_PRESET) {
        p.robots_token.clone()
    } else {
        if custom_http.is_some() {
            warning = Some(
                "eigener User-Agent ohne robots.txt-Token: es gilt nur die Gruppe „User-agent: *“"
                    .to_string(),
            );
        }
        FALLBACK_ROBOTS_TOKEN.to_string()
    };
    ResolvedUserAgent {
        http,
        robots_token,
        warning,
    }
}

/// Liest Voreinstellungen aus dem TOML-Teilformat der Datendatei (`[[preset]]` mit Zeichenketten-Werten).
pub fn parse_presets(text: &str) -> Result<Vec<UserAgentPreset>, String> {
    #[derive(Default)]
    struct Raw {
        id: Option<String>,
        name: Option<String>,
        group: Option<String>,
        user_agent: Option<String>,
        robots_token: Option<String>,
    }
    fn finish(raw: Raw, out: &mut Vec<UserAgentPreset>) -> Result<(), String> {
        let id = raw.id.ok_or("Eintrag ohne `id`")?;
        let version = env!("CARGO_PKG_VERSION");
        out.push(UserAgentPreset {
            name: raw.name.unwrap_or_else(|| id.clone()),
            group: raw.group.unwrap_or_default(),
            user_agent: raw
                .user_agent
                .filter(|s| !s.trim().is_empty())
                .map(|s| s.replace("{version}", version)),
            robots_token: raw
                .robots_token
                .unwrap_or_else(|| FALLBACK_ROBOTS_TOKEN.to_string()),
            id,
        });
        Ok(())
    }

    let mut out = Vec::new();
    let mut current: Option<Raw> = None;
    for (idx, line) in text.lines().enumerate() {
        let n = idx + 1;
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line == "[[preset]]" {
            if let Some(raw) = current.take() {
                finish(raw, &mut out).map_err(|e| format!("Zeile {n}: {e}"))?;
            }
            current = Some(Raw::default());
            continue;
        }
        let Some((key, rest)) = line.split_once('=') else {
            return Err(format!("Zeile {n}: `Schlüssel = \"Wert\"` erwartet"));
        };
        let value = parse_string(rest.trim()).map_err(|e| format!("Zeile {n}: {e}"))?;
        let raw = current
            .as_mut()
            .ok_or_else(|| format!("Zeile {n}: Wert außerhalb von [[preset]]"))?;
        match key.trim() {
            "id" => raw.id = Some(value),
            "name" => raw.name = Some(value),
            "group" => raw.group = Some(value),
            "user_agent" => raw.user_agent = Some(value),
            "robots_token" => raw.robots_token = Some(value),
            other => return Err(format!("Zeile {n}: unbekannter Schlüssel {other:?}")),
        }
    }
    if let Some(raw) = current.take() {
        finish(raw, &mut out).map_err(|e| format!("Dateiende: {e}"))?;
    }
    Ok(out)
}

/// Eine einfach gequotete TOML-Zeichenkette mit den Escapes `\"`, `\\`, `\n`, `\t`; Rest der Zeile
/// nach dem schließenden Anführungszeichen darf nur ein Kommentar sein.
fn parse_string(s: &str) -> Result<String, String> {
    let mut chars = s.chars();
    if chars.next() != Some('"') {
        return Err("Zeichenkette in Anführungszeichen erwartet".to_string());
    }
    let mut out = String::new();
    loop {
        match chars.next() {
            None => return Err("Zeichenkette nicht geschlossen".to_string()),
            Some('"') => break,
            Some('\\') => match chars.next() {
                Some('"') => out.push('"'),
                Some('\\') => out.push('\\'),
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                other => return Err(format!("unbekannte Escape-Folge {other:?}")),
            },
            Some(c) => out.push(c),
        }
    }
    let rest: String = chars.collect();
    let rest = rest.trim();
    if rest.is_empty() || rest.starts_with('#') {
        Ok(out)
    } else {
        Err(format!("unerwartet nach der Zeichenkette: {rest:?}"))
    }
}

/// Kleiner JSON-Leser (RFC 8259) für den AI-Crawler-Katalog; `os-fetch` hat keine JSON-Abhängigkeit, und
/// die Datei soll wie `user-agents.toml` ohne neue Abhängigkeit gelesen werden.
mod json {
    /// Ein JSON-Wert (Zahlen bleiben Text, sie werden hier nicht gebraucht).
    #[derive(Debug, Clone, PartialEq)]
    pub enum Value {
        Null,
        Bool(bool),
        Number(String),
        String(String),
        Array(Vec<Value>),
        Object(Vec<(String, Value)>),
    }

    impl Value {
        pub fn get(&self, key: &str) -> Option<&Value> {
            match self {
                Value::Object(fields) => fields.iter().find(|(k, _)| k == key).map(|(_, v)| v),
                _ => None,
            }
        }

        pub fn as_array(&self) -> Option<&Vec<Value>> {
            match self {
                Value::Array(items) => Some(items),
                _ => None,
            }
        }
    }

    /// Höchste Verschachtelungstiefe.
    const MAX_DEPTH: usize = 64;

    pub fn parse(text: &str) -> Result<Value, String> {
        let mut p = Parser {
            s: text.strip_prefix('\u{feff}').unwrap_or(text).as_bytes(),
            i: 0,
        };
        let v = p.value(0)?;
        p.ws();
        if p.i != p.s.len() {
            return Err(p.err("Text nach dem JSON-Wert"));
        }
        Ok(v)
    }

    struct Parser<'a> {
        s: &'a [u8],
        i: usize,
    }

    impl Parser<'_> {
        fn err(&self, what: &str) -> String {
            format!("JSON: {what} an Byte {}", self.i)
        }

        fn ws(&mut self) {
            while self
                .s
                .get(self.i)
                .is_some_and(|b| matches!(b, b' ' | b'\t' | b'\n' | b'\r'))
            {
                self.i += 1;
            }
        }

        fn eat(&mut self, lit: &str) -> bool {
            if self.s[self.i..].starts_with(lit.as_bytes()) {
                self.i += lit.len();
                true
            } else {
                false
            }
        }

        fn value(&mut self, depth: usize) -> Result<Value, String> {
            if depth > MAX_DEPTH {
                return Err(self.err("zu tief verschachtelt"));
            }
            self.ws();
            match self.s.get(self.i).copied() {
                None => Err(self.err("Wert erwartet")),
                Some(b'{') => {
                    self.i += 1;
                    let mut fields = Vec::new();
                    self.ws();
                    if self.eat("}") {
                        return Ok(Value::Object(fields));
                    }
                    loop {
                        self.ws();
                        if self.s.get(self.i) != Some(&b'"') {
                            return Err(self.err("Schlüssel erwartet"));
                        }
                        let key = self.string()?;
                        self.ws();
                        if !self.eat(":") {
                            return Err(self.err("`:` erwartet"));
                        }
                        let v = self.value(depth + 1)?;
                        fields.push((key, v));
                        self.ws();
                        if self.eat(",") {
                            continue;
                        }
                        if self.eat("}") {
                            return Ok(Value::Object(fields));
                        }
                        return Err(self.err("`,` oder `}` erwartet"));
                    }
                }
                Some(b'[') => {
                    self.i += 1;
                    let mut items = Vec::new();
                    self.ws();
                    if self.eat("]") {
                        return Ok(Value::Array(items));
                    }
                    loop {
                        items.push(self.value(depth + 1)?);
                        self.ws();
                        if self.eat(",") {
                            continue;
                        }
                        if self.eat("]") {
                            return Ok(Value::Array(items));
                        }
                        return Err(self.err("`,` oder `]` erwartet"));
                    }
                }
                Some(b'"') => self.string().map(Value::String),
                Some(b't') if self.eat("true") => Ok(Value::Bool(true)),
                Some(b'f') if self.eat("false") => Ok(Value::Bool(false)),
                Some(b'n') if self.eat("null") => Ok(Value::Null),
                Some(b'-' | b'0'..=b'9') => self.number(),
                Some(_) => Err(self.err("unerwartetes Zeichen")),
            }
        }

        fn number(&mut self) -> Result<Value, String> {
            let start = self.i;
            if self.s.get(self.i) == Some(&b'-') {
                self.i += 1;
            }
            match self.s.get(self.i).copied() {
                Some(b'0') => self.i += 1,
                Some(b'1'..=b'9') => {
                    self.digits();
                }
                _ => return Err(self.err("Ziffer erwartet")),
            }
            if self.s.get(self.i) == Some(&b'.') {
                self.i += 1;
                if !self.digits() {
                    return Err(self.err("Ziffer nach `.` erwartet"));
                }
            }
            if matches!(self.s.get(self.i), Some(b'e' | b'E')) {
                self.i += 1;
                if matches!(self.s.get(self.i), Some(b'+' | b'-')) {
                    self.i += 1;
                }
                if !self.digits() {
                    return Err(self.err("Exponent erwartet"));
                }
            }
            let text = std::str::from_utf8(&self.s[start..self.i]).unwrap_or_default();
            Ok(Value::Number(text.to_string()))
        }

        /// Liest Ziffern; `true`, wenn mindestens eine da war.
        fn digits(&mut self) -> bool {
            let from = self.i;
            while self.s.get(self.i).is_some_and(u8::is_ascii_digit) {
                self.i += 1;
            }
            self.i > from
        }

        fn hex4(&mut self) -> Result<u32, String> {
            let digits = self
                .s
                .get(self.i..self.i + 4)
                .and_then(|h| std::str::from_utf8(h).ok())
                .filter(|h| h.bytes().all(|b| b.is_ascii_hexdigit()))
                .ok_or_else(|| self.err("vier Hex-Ziffern erwartet"))?;
            let v = u32::from_str_radix(digits, 16).map_err(|_| self.err("Hex-Ziffern"))?;
            self.i += 4;
            Ok(v)
        }

        fn string(&mut self) -> Result<String, String> {
            self.i += 1; // öffnendes Anführungszeichen
            let mut out = String::new();
            loop {
                let start = self.i;
                while self
                    .s
                    .get(self.i)
                    .is_some_and(|b| *b != b'"' && *b != b'\\' && *b >= 0x20)
                {
                    self.i += 1;
                }
                out.push_str(
                    std::str::from_utf8(&self.s[start..self.i])
                        .map_err(|_| self.err("ungültiges UTF-8"))?,
                );
                match self.s.get(self.i).copied() {
                    None => return Err(self.err("Zeichenkette nicht geschlossen")),
                    Some(b'"') => {
                        self.i += 1;
                        return Ok(out);
                    }
                    Some(b'\\') => {
                        self.i += 1;
                        let esc = self.s.get(self.i).copied();
                        self.i += 1;
                        match esc {
                            Some(b'"') => out.push('"'),
                            Some(b'\\') => out.push('\\'),
                            Some(b'/') => out.push('/'),
                            Some(b'b') => out.push('\u{8}'),
                            Some(b'f') => out.push('\u{c}'),
                            Some(b'n') => out.push('\n'),
                            Some(b'r') => out.push('\r'),
                            Some(b't') => out.push('\t'),
                            Some(b'u') => {
                                let hi = self.hex4()?;
                                let code = if (0xD800..0xDC00).contains(&hi) {
                                    if !self.eat("\\u") {
                                        return Err(self.err("zweite Hälfte des Ersatzpaars fehlt"));
                                    }
                                    let lo = self.hex4()?;
                                    if !(0xDC00..0xE000).contains(&lo) {
                                        return Err(self.err("ungültiges Ersatzpaar"));
                                    }
                                    0x10000 + ((hi - 0xD800) << 10) + (lo - 0xDC00)
                                } else {
                                    hi
                                };
                                out.push(
                                    char::from_u32(code)
                                        .ok_or_else(|| self.err("ungültiges Zeichen"))?,
                                );
                            }
                            _ => return Err(self.err("unbekannte Escape-Folge")),
                        }
                    }
                    Some(_) => return Err(self.err("Steuerzeichen in Zeichenkette")),
                }
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        // spec: CRAWL-114
        #[test]
        fn reads_values_escapes_and_rejects_garbage() {
            let v = parse(
                "\u{feff} {\"a\": [1, -2.5e3, true, false, null], \"s\": \"x\\\"\\u00e9\\ud83d\\ude00\\n\"} ",
            )
            .unwrap();
            assert_eq!(v.get("a").and_then(Value::as_array).map(Vec::len), Some(5));
            assert_eq!(v.get("s"), Some(&Value::String("x\"é😀\n".into())));
            for bad in [
                "",
                "{",
                "[1,]",
                "{\"a\" 1}",
                "\"\\x\"",
                "01",
                "{} x",
                "\"a\nb\"",
                "tru",
            ] {
                assert!(parse(bad).is_err(), "{bad:?}");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(preset: &str, http: Option<&str>, token: Option<&str>) -> UserAgentConfig {
        UserAgentConfig {
            preset: preset.to_string(),
            http: http.map(str::to_string),
            robots_token: token.map(str::to_string),
        }
    }

    // spec: CRAWL-114
    #[test]
    fn embedded_list_contains_all_required_presets() {
        let ids: Vec<&str> = presets().iter().map(|p| p.id.as_str()).collect();
        for required in [
            "openspider",
            "googlebot-smartphone",
            "googlebot-desktop",
            "googlebot-image",
            "googlebot-news",
            "googlebot-video",
            "google-inspectiontool",
            "google-extended",
            "bingbot-mobile",
            "bingbot-desktop",
            "yandex",
            "baiduspider",
            "duckduckbot",
            "applebot",
            "gptbot",
            "claudebot",
            "perplexitybot",
            "chrome-desktop",
            "chrome-mobile",
            "safari-iphone",
            "firefox",
            "edge",
            "custom",
        ] {
            assert!(ids.contains(&required), "Voreinstellung {required} fehlt");
        }
        let mut sorted = ids.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), ids.len(), "Kennungen müssen eindeutig sein");
    }

    // spec: CRAWL-114
    #[test]
    fn default_preset_is_openspider_with_version_and_token() {
        let r = resolve(&UserAgentConfig::default());
        assert_eq!(r.http, default_user_agent());
        assert!(r.http.starts_with("Mozilla/5.0 (compatible; OpenSpider/"));
        assert!(r.http.contains(env!("CARGO_PKG_VERSION")));
        assert!(!r.http.contains("{version}"));
        assert_eq!(r.robots_token, "OpenSpider");
        assert_eq!(r.warning, None);
    }

    // spec: CRAWL-114
    #[test]
    fn googlebot_smartphone_uses_googlebot_token() {
        let r = resolve(&cfg("googlebot-smartphone", None, None));
        assert_eq!(r.robots_token, "Googlebot");
        assert!(r.http.contains("Googlebot/2.1"));
        // Anzeigename wird ebenfalls erkannt
        let r2 = resolve(&cfg("Googlebot Smartphone", None, None));
        assert_eq!(r, r2);
    }

    // spec: CRAWL-114
    #[test]
    fn free_user_agent_without_token_falls_back_to_star_with_warning() {
        let r = resolve(&cfg("custom", Some("MeinBot/1.0"), None));
        assert_eq!(r.http, "MeinBot/1.0");
        assert_eq!(r.robots_token, "*");
        assert!(r.warning.is_some());
        // mit Token keine Warnung
        let r = resolve(&cfg("custom", Some("MeinBot/1.0"), Some("MeinBot")));
        assert_eq!(r.robots_token, "MeinBot");
        assert_eq!(r.warning, None);
    }

    // spec: CRAWL-114
    #[test]
    fn token_only_and_unknown_presets_fall_back_to_default_agent() {
        let r = resolve(&cfg("google-extended", None, None));
        assert_eq!(r.http, default_user_agent());
        assert_eq!(r.robots_token, "Google-Extended");
        let r = resolve(&cfg("gibt-es-nicht", None, None));
        assert_eq!(r.http, default_user_agent());
        assert!(r.warning.is_some());
    }

    // spec: CRAWL-114
    #[test]
    fn explicit_http_overrides_preset_but_keeps_its_token() {
        let r = resolve(&cfg("openspider", Some("Spezial/2"), None));
        assert_eq!(r.http, "Spezial/2");
        assert_eq!(r.robots_token, "OpenSpider");
    }

    // spec: CRAWL-114
    #[test]
    fn list_can_be_replaced_without_code_change() {
        let list = parse_presets(
            "# eigene Liste\n[[preset]]\nid = \"x\"\nname = \"X \\\"Bot\\\"\"\nuser_agent = \"XBot/{version}\" # Kommentar\nrobots_token = \"XBot\"\n",
        )
        .unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "X \"Bot\"");
        assert_eq!(
            list[0].user_agent.as_deref(),
            Some(format!("XBot/{}", env!("CARGO_PKG_VERSION")).as_str())
        );
        let r = resolve_with(&cfg("x", None, None), &list);
        assert_eq!(r.robots_token, "XBot");
        assert!(parse_presets("id = \"a\"").is_err());
        assert!(parse_presets("[[preset]]\nid = a").is_err());
        assert!(parse_presets("[[preset]]\nfoo = \"a\"").is_err());
        assert!(parse_presets("[[preset]]\nname = \"a\"").is_err());
    }
}
