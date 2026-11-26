//! Platzhalter von M4-V01 (Optimierungsvorschläge I, 1.1-Etappe). Der Kern ist umgesetzt und in `suggest_*.rs`
//! getestet; hier stehen die Kriterien, die erst die 1.2-Etappe (AI-077), die Oberfläche von M4-U01
//! (Kostenvoranschlag, Einwilligung) bzw. die Integration M4-N01 (Ausführung in der App) prüfbar machen.
//!
//! Hinweis zum Grundtext des AI-077-Platzhalters: Ein `wartet auf M4-V01 …` für das eigene Paket wertet der
//! Spec-Coverage-Prüfer (`os-parity`, Plan M4 E4/D7) in jeder Datei als ungültig; der Grund beginnt deshalb mit der
//! zugehörigen Klärung K4-18 (Neuschnitt der 1.2-IDs) und trägt danach den vorgegebenen Text.

// spec: AI-077 [M4]
// Markenzusatz „ | Beispiel GmbH“ am Ende → jeder gültige Title-Kandidat endet damit, die Pixelbreite schließt ihn ein;
// verbotenes Wort „günstigste“ → Verstoß auch bei abweichender Groß-/Kleinschreibung; Segment „Shop“ mit Pflichtbegriff
// „kaufen“ → Kandidaten für Shop-URLs ohne „kaufen“ ungültig. Anschluss: `tasks::suggest::style::StyleRules`
// (`prompt_lines`, `violations`, `rendered`), Regel (6) in `validate::Checker`.
#[test]
#[ignore = "Klärung K4-18: wartet auf M4-V01 (1.2): AI-077 Stil- und Markenregeln im Prompt und lokal geprüft"]
fn style_and_brand_rules_flow_into_prompt_and_check() {
    unimplemented!("M4-V01 (1.2): AI-077");
}

// spec: AI-075 [M4]
// AI-075 Regel (6) (Stil- und Markenregeln) einzeln abschaltbar; mit Regeln aus AI-077 wirksam (heute ohne Regeln).
#[test]
#[ignore = "Klärung K4-18: wartet auf M4-V01 (1.2): AI-077 Regel 6 der Validierungsschleife mit echten Regeln"]
fn rule_six_uses_the_style_rules() {
    unimplemented!("M4-V01 (1.2): AI-077");
}

// spec: AI-070 [M4]
// spec: AI-012 [M4]
// „Vorschläge erzeugen“ in der App: Kostenvoranschlag-Dialog mit den Zahlen aus `ai.suggestRun` (Antwort `state =
// confirmationRequired`, Kern `tasks::suggest::estimate`), „Starten“ sendet den Lauf, „Abbrechen“ nichts.
#[test]
#[ignore = "wartet auf M4-U01: Dialog Kostenvoranschlag und Bestätigung vor dem Vorschlagslauf"]
fn app_run_asks_for_cost_confirmation() {
    unimplemented!("M4-U01");
}

// spec: AI-018 [M4]
// Einwilligung aus dem Speicher der App (`policy::ConsentStore`, Nutzerentscheidung 24.11.2026 je Installation) für den
// Vorschlagslauf; ohne Einwilligung `Pending (consent)` (Kern: suggest_mock.rs
// `without_consent_entries_wait_for_consent_and_nothing_is_sent`).
#[test]
#[ignore = "wartet auf M4-U01: Einwilligung aus dem App-Speicher für ai.suggestRun"]
fn app_run_uses_the_stored_consent() {
    unimplemented!("M4-U01");
}

// spec: AI-070 [M4]
// spec: AI-071 [M4]
// spec: AI-072 [M4]
// Ausführung von `ai.suggestRun` in der App gegen den ai-mock (Anbieter und Schlüssel aus der Modellwahl, Basis-URL
// `OPENSPIDER_API_BASE_OPENAI`): 1 Request je URL und Element, Ergebnisse im Tab „AI Suggestions“ (Kern:
// suggest_mock.rs).
#[test]
#[ignore = "wartet auf M4-N01: Ausführung von ai.suggestRun in der App (Anbieter, Schlüssel, Modellwahl)"]
fn app_run_against_the_mock() {
    unimplemented!("M4-N01");
}

// spec: AI-070 [M4]
// Standard-Geltungsbereich mit `Title Not Matching Content (AI)` bzw. `Meta Description Not Matching Content (AI)`
// aus den Antworten von AI-054/AI-055 (Antwortformat von M4-D01; heute gelesen als `false` bzw. `{"value": false}`).
#[test]
#[ignore = "wartet auf M4-N01: Antwortformat der Fragen title-matches-content und meta-description-matches-content"]
fn default_scope_includes_ai_mismatch_filters() {
    unimplemented!("M4-N01");
}

// spec: AI-020 [M4]
// Tooltip „Ausgabegrenze erreicht – max_tokens erhöhen oder Reasoning-Stufe senken (KI › Modellwahl)“ an der
// Statuszelle in der Tabelle des Tabs „AI Suggestions“ (die Prüfansicht zeigt ihn bereits; die Tabelle selbst hat
// keinen Zell-Tooltip-Anschluss).
#[test]
#[ignore = "wartet auf M4-N01: Tooltip der Statuszelle in der generischen Tabelle"]
fn status_cell_tooltip_in_the_table() {
    unimplemented!("M4-N01");
}
