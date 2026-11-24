//! Platzhalter M4-K07 (Plan M4 E4): Kriterien aus AI-018 und AI-024, die erst mit späteren Paketen auf Oberflächen-,
//! CLI-, Lauf- oder Protokollebene prüfbar sind, sowie Teile, die AI-019 (Release 1.2) voraussetzen. Die Kernteile
//! sind in `policy_consent.rs`, `policy_redaction.rs` und `policy_mock.rs` geprüft. Aufgelöst durch M4-N01/N02.

// spec: AI-018
// Vor dem ersten Lauf einer KI-Aufgabe erscheint der Dialog „Datenfluss bestätigen“ mit den Zeilen aus
// `DataFlowOverview::consent_requests`; Bestätigen ruft `ConsentGate::grant` mit der gezeigten Zeile auf, die
// Übersicht „KI-Datenfluss“ ist jederzeit aufrufbar, Widerruf je Anbieter. Command-Rumpf
// `crates/os-app/src/commands/ai_consent.rs` und Speicherung des `ConsentBook` gehören M4-U01.
#[test]
#[ignore = "wartet auf M4-U01: Dialog, Übersicht, Widerruf und Persistenz der Einwilligung"]
fn consent_dialog_overview_and_revocation_in_the_app() {
    panic!("E2E über apps/desktop/e2e/ai-consent; Kern: policy_consent.rs, policy_mock.rs");
}

// spec: AI-018
// Export der Datenflusstabelle als CSV aus der Oberfläche (Kern `DataFlowOverview::to_csv` geprüft).
#[test]
#[ignore = "wartet auf M4-U01: CSV-Export der Datenflusstabelle aus der Übersicht"]
fn dataflow_csv_export_from_the_app() {
    panic!("Command-Rumpf ai_consent.rs mit Export; Kern: policy_consent.rs");
}

// spec: AI-018
// Im CLI ersetzt `--ai-consent` den Dialog; ohne ihn laufen keine KI-Aufgaben, und der Lauf meldet das
// (`ConsentGate::cli`, `CLI_CONSENT_MISSING`; Exit-Code 2 vor dem Crawl nach AI-022).
#[test]
#[ignore = "wartet auf M4-C01: Schalter --ai-consent in os-cli (AI-022)"]
fn cli_ai_consent_switch_end_to_end() {
    panic!(
        "CLI-Test gegen tests/fixtures/ai/site + ai-mock; Kern: policy_consent.rs, policy_mock.rs"
    );
}

// spec: AI-018
// Mock-Zähler 0 und Feldmarker für einen echten Aufgabenlauf über die Produktivadapter (Fixture-Site mit
// `tests/fixtures/ai/site/fields.html`; der Kern ist mit derselben Seite in policy_mock.rs geprüft).
#[test]
#[ignore = "wartet auf M4-D01: Aufgabenlauf über os-ai::decisions mit policy::guard vor jedem Adapter"]
fn consent_and_field_markers_in_a_real_task_run() {
    panic!("Lauf der Seitentyp-Frage ohne/mit Einwilligung; Kern: policy_mock.rs");
}

// spec: AI-018
// Spalte `Region` und „Richtlinie in Datenflussübersicht“ (EU erzwingen / Nur lokal, „EU nicht freigeschaltet“)
// setzen AI-019 voraus. Bis dahin bleibt `TaskDataFlow::region` leer, sofern der Aufrufer keine Region festlegt.
#[test]
#[ignore = "wartet auf M4-K07 (1.2-Etappe, AI-019): Region und Richtlinie in der Datenflussübersicht"]
fn dataflow_shows_region_and_policy() {
    panic!("AI-019 gehört zur 1.2-Etappe (K4-18)");
}

// spec: AI-024
// Protokollspalte `Redactions` je Request (AI-014): `GuardedOutcome::redactions` in `ai_requests.redactions`.
#[test]
#[ignore = "wartet auf M4-D01: Protokollzeile je Request mit Redactions aus policy::guard"]
fn redactions_column_in_request_log() {
    panic!("Seite pii.html → Protokollzeile Redactions = 3; Kern: policy_mock.rs");
}

// spec: AI-024
// Eine ungültige eigene Regex wird beim Speichern in den Einstellungen abgelehnt (Zeile und Position). Die Prüfung
// selbst (`PatternList::parse`, `RedactionSettings::from_app_settings`, fancy-regex) ist in policy_redaction.rs
// geprüft; offen ist nur der Speichern-Weg in der Oberfläche.
#[test]
#[ignore = "wartet auf M4-U01: Speichern der Musterliste in den KI-Einstellungen"]
fn invalid_custom_regex_rejected_on_save() {
    panic!("Einstellungsdialog ruft PatternList::parse auf und zeigt Zeile/Position; Kern: policy_redaction.rs");
}
