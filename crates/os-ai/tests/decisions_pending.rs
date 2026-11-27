//! Platzhalter von M4-D01 für Kriterien, die erst spätere Pakete prüfbar machen (Plan M4, E4; Auflösung durch
//! die genannten Pakete, Testinhalt dann unverändert übernehmen). Die Datei heißt nicht
//! `pending_m4_d01.rs`, weil `owns` von D01 nur `crates/os-ai/tests/decisions_*.rs` umfasst (Bericht d01).

// spec: AI-020 [M4]
// KI-Lauf in der Pause: automatische Ausführung (AI-039) aus, Crawl pausiert mit 10 gecrawlten URLs, „KI-Analyse
// starten“ → KI-Requests nur für Einträge dieser URLs, gecrawlte URLs bleiben 10, Crawl-Zustand `Paused`; bei
// laufendem Crawl ist die Schaltfläche deaktiviert.
#[test]
#[ignore = "wartet auf M4-D02: Schaltfläche „KI-Analyse starten“ (AI-039) und Anbindung an die Crawl-Verwaltung"]
fn ai_run_during_a_paused_crawl() {
    unimplemented!("DecisionRun über die URLs der pausierten Crawl-Datei aus der App");
}

// spec: AI-020 [M4]
// CLI: SIGINT während offener KI-Einträge → Exit-Code 130, Einträge `Pending`; `--load-crawl <ID> --resume` sendet
// genau diese und endet mit allen Einträgen fertig.
#[test]
#[ignore = "wartet auf M4-D05: CLI-Anbindung des KI-Laufs (SIGINT → RunControl::pause, Fortsetzen)"]
fn cli_sigint_and_resume() {
    unimplemented!("RunControl::pause aus dem SIGINT-Handler, Fortsetzen mit Selection::Open");
}

// spec: AI-020 [M4]
// Crawl-Ende ohne Stoppen (`limit_reached`, `limits.max_urls` = 10, `hold`): nach dem Ende werden alle offenen
// Einträge der 10 URLs gesendet, bis die Warteschlange leer ist; Mock-Log = Zahl der Einträge im Geltungsbereich.
#[test]
#[ignore = "wartet auf M4-D05: automatische Ausführung während des Crawls (Engine ↔ KI-Warteschlange, E-6 a)"]
fn crawl_end_without_stop_drains_the_queue() {
    unimplemented!("Engine-Hook ruft DecisionRun nach jeder gecrawlten URL bzw. nach dem Ende auf");
}

// spec: AI-020 [M4]
// Absturz per SIGKILL eines echten Prozesses, während der Mock Request n hält (das Verhalten der Crawl-Datei prüft
// `decisions_lifecycle::crash_while_a_request_is_held` im selben Prozess).
#[test]
#[ignore = "wartet auf M4-D05: Prozess mit KI-Lauf (CLI) für den SIGKILL-Test"]
fn crash_by_sigkill() {
    unimplemented!("CLI-Kindprozess mit hold, SIGKILL, Öffnen, Fortsetzen");
}

// spec: AI-020 [M4]
// Fremder Writer: Crawl-Datei mit `owner_kind = cli`, `owner_pid` und aktuellem (simuliertem) Heartbeat, offene
// KI-Einträge, alle Schlüssel verbunden → die App zeigt Ergebnisse und den Hinweis, alle KI-Aktionen deaktiviert, der
// ai-mock der App zählt 0 Requests.
#[test]
#[ignore = "wartet auf M4-D05: App mit KI-Lauf, nur lesend bei fremdem aktivem Besitzer, ai-mock-Zähler der App"]
fn foreign_writer_is_read_only() {
    unimplemented!("RunOwner::start meldet den fremden Besitzer; DecisionRun::run gibt den Fehler unverändert zurück");
}

// spec: AI-020 [M4]
// Fortsetzen nach der Pause mit Bestätigungsschwelle 0,0001 USD (Testprofil AI-012): kein Kostenvoranschlag im
// Dialog (die Engine selbst fragt beim Fortsetzen mit `Confirmation::Granted` nicht, siehe
// `decisions_lifecycle::pause_and_resume`).
#[test]
#[ignore = "wartet auf M4-U01: Dialog Kostenvoranschlag beim Fortsetzen"]
fn resume_shows_no_estimate_dialog() {
    unimplemented!("U01-Dialog");
}

// spec: AI-032 [M4]
// Bild 3.000 × 2.000 px → gesendetes Bild 2.048 × 1.365 px (`tests/fixtures/ai/site/img/big.jpg`). Die Maße und die
// Schnittstelle `ImageResizer` stehen (`decisions_input::image_limits`); es fehlt ein Bilddekoder in `os-ai`.
#[test]
#[ignore = "wartet auf M4-N01: Manifest os-ai (Bildbibliothek für die Verkleinerung) und Fixture big.jpg"]
fn big_image_is_resized_before_sending() {
    unimplemented!("ImageResizer mit Dekoder; gesendete Bytes 2048 × 1365");
}
