//! Unabhängige Abnahmetests M4-K07: Datenflussübersicht und Einwilligung (AI-018).
//!
//! Erwartungen ausschließlich aus `docs/spec/08-ai.md` (AI-018, AI-020 Statuswert `Pending (consent)`) und den
//! Festlegungen des Orchestrators (Einwilligung deckt jede Teilmenge ihrer Feldmenge; Modellwechsel fragt nicht neu).
//! Der Weg zum Anbieter läuft über den Wächter `os_ai::policy::guard::execute` und einen Test-Adapter, der an den
//! ai-mock sendet; der Mock-Zähler ist die Zahl der Requests, die die Maschine verlassen haben.
// acceptance: independent

mod acceptance_k07_support;

use acceptance_k07_support::{extract_fields, read_fixture, Mock, GIVEN_AT_MS};
use os_ai::policy::guard::{self, GuardedOutcome};
use os_ai::policy::{
    ConsentBook, ConsentGate, ConsentKey, ConsentMode, ConsentStore, DataFlowOverview, FieldSet,
    FieldValues, InputField, MemoryConsentStore, PreparedInput, ProviderPolicies,
    RedactionSettings, Redactor, TaskDataFlow, COLUMNS,
};
use os_ai::provider::Outcome;
use os_ai::{AiStatus, PreviewSwitch};
use std::sync::Arc;

use InputField::*;

const FIELDS_PAGE: &str = "tests/fixtures/ai/k07-acceptance/field-markers.html";
const PAGE_URL: &str = "http://ai.test/acceptance/MARK-URL";

fn fields(list: &[InputField]) -> FieldSet {
    FieldSet::new(list.iter().copied())
}

fn page() -> FieldValues {
    extract_fields(PAGE_URL, &read_fixture(FIELDS_PAGE))
}

fn flow(task: &str, provider: &str, set: &FieldSet) -> TaskDataFlow {
    TaskDataFlow {
        task: task.into(),
        provider: provider.into(),
        model: "k07-model".into(),
        endpoint: "https://api.openai.com/v1".into(),
        region: None,
        fields: set.clone(),
        max_chars: None,
    }
}

async fn run(
    mock: &Mock,
    gate: &ConsentGate,
    provider: &'static str,
    task: &str,
    set: &FieldSet,
    model: &str,
) -> GuardedOutcome {
    let p = mock.provider(provider);
    let input = PreparedInput::new(&page(), set, &Redactor::default());
    guard::execute(PreviewSwitch::ON, gate, task, &input, &p, p.request(model)).await
}

fn assert_pending(out: &GuardedOutcome) {
    assert_eq!(out.outcome, Outcome::Status(AiStatus::PendingConsent));
    assert!(!out.sent, "ohne Einwilligung darf nichts gesendet werden");
}

fn assert_sent(out: &GuardedOutcome) {
    assert!(out.sent, "Request wurde nicht gesendet: {:?}", out.outcome);
    assert!(
        matches!(out.outcome, Outcome::Completed(_)),
        "{:?}",
        out.outcome
    );
}

fn title_fields() -> FieldSet {
    fields(&[Url, Title, MetaDescription, H1])
}

// ---------------------------------------------------------------------------------------------------------------
// Kriterium 1: Ohne Bestätigung kein Request, Einträge `Pending (consent)`, Mock-Zähler 0.

// spec: AI-018
#[tokio::test]
async fn ai018_k1_ohne_bestaetigung_kein_request_und_pending_consent() {
    let mock = Mock::start().await;
    let gate = ConsentGate::dialog(ConsentBook::new());
    for task in ["suggest-title", "summarize", "decision"] {
        for _ in 0..3 {
            let out = run(&mock, &gate, "openai", task, &title_fields(), "m1").await;
            assert_pending(&out);
            assert_eq!(out.redactions, 0);
        }
    }
    assert_eq!(mock.count(), 0, "Mock-Zähler muss 0 sein");
    assert_eq!(AiStatus::PendingConsent.to_string(), "Pending (consent)");
    let check = gate.check(&ConsentKey::new("openai", "suggest-title", title_fields()));
    assert!(!check.is_granted());
    assert_eq!(check.status(), Some(AiStatus::PendingConsent));
}

// spec: AI-018
// Einwilligung gilt je (Anbieter, Aufgabentyp, Feldmenge): eine andere Aufgabe oder ein anderer Anbieter ist nicht
// abgedeckt.
#[tokio::test]
async fn ai018_k1_einwilligung_gilt_nur_fuer_ihren_anbieter_und_aufgabentyp() {
    let mock = Mock::start().await;
    let gate = ConsentGate::dialog(ConsentBook::new());
    gate.grant(
        ConsentKey::new("openai", "summarize", title_fields()),
        GIVEN_AT_MS,
        None,
    )
    .unwrap();
    gate.grant(
        ConsentKey::new("anthropic", "suggest-title", title_fields()),
        GIVEN_AT_MS,
        None,
    )
    .unwrap();
    // openai + suggest-title ist durch keine der beiden Einwilligungen gedeckt.
    let out = run(
        &mock,
        &gate,
        "openai",
        "suggest-title",
        &title_fields(),
        "m1",
    )
    .await;
    assert_pending(&out);
    assert_eq!(mock.count(), 0);
    // Gegenprobe: die tatsächlich bestätigten Paare senden.
    assert_sent(&run(&mock, &gate, "openai", "summarize", &title_fields(), "m1").await);
    assert_sent(
        &run(
            &mock,
            &gate,
            "anthropic",
            "suggest-title",
            &title_fields(),
            "m1",
        )
        .await,
    );
    assert_eq!(mock.count(), 2);
}

// ---------------------------------------------------------------------------------------------------------------
// Kriterium 2: Kommt bei „Title-Vorschläge" das Feld „Seitentext" hinzu, erscheint der Dialog erneut, bevor der
// nächste Request mit Seitentext gesendet wird.

// spec: AI-018
#[tokio::test]
async fn ai018_k2_title_vorschlaege_mit_seitentext_fragt_vor_dem_naechsten_request_erneut() {
    let mock = Mock::start().await;
    let policies = ProviderPolicies::embedded();
    let gate = ConsentGate::dialog(ConsentBook::new());
    let before = title_fields();
    let flows = [flow("suggest-title", "openai", &before)];

    // Erster Lauf: Dialog erscheint, Bestätigung, Request geht hinaus.
    let asks = DataFlowOverview::consent_requests(&flows, &policies, &gate, true);
    assert_eq!(asks.len(), 1);
    gate.grant(
        flows[0].consent_key(),
        GIVEN_AT_MS,
        Some(asks[0].row.clone()),
    )
    .unwrap();
    assert!(DataFlowOverview::consent_requests(&flows, &policies, &gate, true).is_empty());
    assert_sent(&run(&mock, &gate, "openai", "suggest-title", &before, "m1").await);
    assert_eq!(mock.count(), 1);

    // Seitentext kommt hinzu: Dialog erneut, mit genau diesem neuen Feld.
    let mut after = before.clone();
    after.insert(PageText);
    let flows = [flow("suggest-title", "openai", &after)];
    let asks = DataFlowOverview::consent_requests(&flows, &policies, &gate, true);
    assert_eq!(asks.len(), 1, "Dialog muss erneut erscheinen");
    assert_eq!(asks[0].added_fields, fields(&[PageText]));
    assert_eq!(asks[0].previous_fields.as_ref(), Some(&before));
    assert_eq!(asks[0].row.fields_sent, after);

    // Vor der Bestätigung geht kein Request mit Seitentext hinaus.
    assert_pending(&run(&mock, &gate, "openai", "suggest-title", &after, "m1").await);
    assert_eq!(mock.count(), 1);
    assert!(mock.bodies().iter().all(|b| !b.contains("MARK-TEXT")));

    // Nach der Bestätigung schon.
    gate.grant(
        flows[0].consent_key(),
        GIVEN_AT_MS + 60_000,
        Some(asks[0].row.clone()),
    )
    .unwrap();
    assert_sent(&run(&mock, &gate, "openai", "suggest-title", &after, "m1").await);
    assert_eq!(mock.count(), 2);
    assert!(mock.bodies()[1].contains("MARK-TEXT"));
}

// spec: AI-018
// Festlegung des Orchestrators: Eine Einwilligung deckt jede Teilmenge ihrer Feldmenge; ein Modellwechsel fragt
// nicht neu (der Schlüssel ist Anbieter, Aufgabentyp, Feldmenge).
#[tokio::test]
async fn ai018_k2_teilmenge_gedeckt_und_modellwechsel_fragt_nicht_neu() {
    let mock = Mock::start().await;
    let policies = ProviderPolicies::embedded();
    let gate = ConsentGate::dialog(ConsentBook::new());
    let full = fields(&[Url, Title, MetaDescription, H1, PageText]);
    gate.grant(
        ConsentKey::new("openai", "suggest-title", full.clone()),
        GIVEN_AT_MS,
        None,
    )
    .unwrap();
    let subset = fields(&[Url, Title]);
    let flows = [
        flow("suggest-title", "openai", &subset),
        flow("suggest-title", "openai", &full),
    ];
    assert!(DataFlowOverview::consent_requests(&flows, &policies, &gate, true).is_empty());
    assert_sent(&run(&mock, &gate, "openai", "suggest-title", &subset, "model-a").await);
    assert_sent(&run(&mock, &gate, "openai", "suggest-title", &full, "model-b").await);
    assert_eq!(mock.count(), 2);
}

// ---------------------------------------------------------------------------------------------------------------
// Kriterium 3: Die angezeigten Felder entsprechen exakt den gesendeten (Feldmarker-Seite).

const MARKERS: [(InputField, &str); 8] = [
    (Url, "MARK-URL"),
    (Title, "MARK-TITLE"),
    (MetaDescription, "MARK-META"),
    (H1, "MARK-H1"),
    (H2, "MARK-H2"),
    (PageText, "MARK-TEXT"),
    (Alt, "MARK-ALT"),
    (ImageContext, "MARK-IMGCTX"),
];
/// Base64 des Bildes der Feldmarker-Seite (`MARK-IMAGE`).
const IMAGE_MARKER: &str = "TUFSSy1JTUFHRQ==";

// spec: AI-018
#[tokio::test]
async fn ai018_k3_angezeigte_felder_entsprechen_exakt_den_gesendeten() {
    // Fixture-Plausibilität: jeder Marker steht genau in seinem Feld.
    let values = page();
    for (field, marker) in MARKERS {
        for (other, _) in MARKERS {
            let text = values.text(other).unwrap_or_default();
            assert_eq!(
                text.contains(marker),
                field == other,
                "{marker} in {other:?}"
            );
        }
    }

    let mut tasks: Vec<(String, FieldSet)> = InputField::ALL
        .iter()
        .map(|f| (format!("single-{}", f.id()), fields(&[*f])))
        .collect();
    tasks.push(("suggest-title".into(), title_fields()));
    tasks.push(("summarize".into(), fields(&[Url, Title, H1, H2, PageText])));
    tasks.push((
        "suggest-alt".into(),
        fields(&[Url, Image, Alt, ImageContext]),
    ));
    tasks.push(("everything".into(), FieldSet::new(InputField::ALL)));

    let mock = Mock::start().await;
    let policies = ProviderPolicies::embedded();
    let gate = ConsentGate::dialog(ConsentBook::new());
    for (i, (task, set)) in tasks.iter().enumerate() {
        let flows = [flow(task, "openai", set)];
        let overview = DataFlowOverview::build(&flows, &policies, &gate, true);
        let shown = &overview.rows[0];
        assert_eq!(&shown.fields_sent, set, "{task}: angezeigte Felder");
        assert_eq!(shown.images, set.contains(Image), "{task}: Spalte Images");
        gate.grant(flows[0].consent_key(), GIVEN_AT_MS, Some(shown.clone()))
            .unwrap();

        assert_sent(&run(&mock, &gate, "openai", task, set, "m1").await);
        let bodies = mock.bodies();
        assert_eq!(bodies.len(), i + 1);
        let body = &bodies[i];
        for (field, marker) in MARKERS {
            assert_eq!(
                body.contains(marker),
                shown.fields_sent.contains(field),
                "{task}: {marker} im Request = {}, Feld angezeigt = {}",
                body.contains(marker),
                shown.fields_sent.contains(field)
            );
        }
        assert_eq!(
            body.contains(IMAGE_MARKER),
            shown.fields_sent.contains(Image),
            "{task}: Bilddaten"
        );
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Kriterium 4: Widerruf für einen Anbieter stoppt sofort alle offenen Aufgaben dieses Anbieters.

// spec: AI-018
#[tokio::test]
async fn ai018_k4_widerruf_stoppt_sofort_alle_offenen_aufgaben_des_anbieters() {
    let mock = Mock::start().await;
    let gate = ConsentGate::dialog(ConsentBook::new());
    let set = title_fields();
    for (provider, task) in [
        ("openai", "suggest-title"),
        ("openai", "summarize"),
        ("anthropic", "suggest-title"),
    ] {
        gate.grant(
            ConsentKey::new(provider, task, set.clone()),
            GIVEN_AT_MS,
            None,
        )
        .unwrap();
    }
    // Je Aufgabe zehn offene Einträge; nach dem zweiten Eintrag wird openai widerrufen (über einen Klon des Tors,
    // wie aus einem anderen Worker oder der Oberfläche).
    let worker_gate = gate.clone();
    let mut revoked = false;
    let mut pending = 0;
    for entry in 0..10 {
        if entry == 2 {
            let keys = gate.revoke_provider("openai").unwrap();
            assert_eq!(keys.len(), 2, "beide openai-Einwilligungen widerrufen");
            revoked = true;
        }
        for (provider, task) in [
            ("openai", "suggest-title"),
            ("openai", "summarize"),
            ("anthropic", "suggest-title"),
        ] {
            let out = run(&mock, &worker_gate, provider, task, &set, "m1").await;
            if revoked && provider == "openai" {
                assert_pending(&out);
                pending += 1;
            } else {
                assert_sent(&out);
            }
        }
    }
    assert_eq!(pending, 16);
    assert_eq!(
        mock.count_for("openai"),
        4,
        "nach dem Widerruf kein openai-Request"
    );
    assert_eq!(
        mock.count_for("anthropic"),
        10,
        "andere Anbieter laufen weiter"
    );

    // Übersicht: openai-Zeilen ohne Einwilligungszeit und wieder im Dialog.
    let policies = ProviderPolicies::embedded();
    let flows = [
        flow("suggest-title", "openai", &set),
        flow("summarize", "openai", &set),
        flow("suggest-title", "anthropic", &set),
    ];
    let overview = DataFlowOverview::build(&flows, &policies, &gate, true);
    assert_eq!(overview.rows[0].consent_given_at_ms, None);
    assert_eq!(overview.rows[1].consent_given_at_ms, None);
    assert_eq!(overview.rows[2].consent_given_at_ms, Some(GIVEN_AT_MS));
    let asks = DataFlowOverview::consent_requests(&flows, &policies, &gate, true);
    assert_eq!(asks.len(), 2);
    assert!(asks.iter().all(|a| a.row.provider == "openai"));
}

// spec: AI-018
// Einwilligung und Widerruf überdauern den Neustart („vor dem ersten Lauf" fragen setzt gespeicherte Einwilligungen
// voraus; ein Widerruf darf nach dem Neustart nicht wieder aufleben).
#[tokio::test]
async fn ai018_k4_einwilligung_und_widerruf_bleiben_gespeichert() {
    let store = Arc::new(MemoryConsentStore::default());
    let key = ConsentKey::new("openai", "suggest-title", title_fields());
    {
        let gate = ConsentGate::with_store(store.clone()).unwrap();
        gate.grant(key.clone(), GIVEN_AT_MS, None).unwrap();
    }
    let gate = ConsentGate::with_store(store.clone()).unwrap();
    assert!(gate.check(&key).is_granted(), "Einwilligung nach Neustart");
    gate.revoke_provider("openai").unwrap();
    let gate = ConsentGate::with_store(store.clone()).unwrap();
    assert!(!gate.check(&key).is_granted(), "Widerruf nach Neustart");
    assert!(store.load().unwrap().records().is_empty());
}

// ---------------------------------------------------------------------------------------------------------------
// Kriterium 5: Schwärzung an + `page_text` → `Redaction = on` ohne PII-Hinweis; aus → `off` mit Hinweis.

// spec: AI-018
#[test]
fn ai018_k5_dialog_zeigt_schwaerzung_und_pii_hinweis_nur_bei_aus() {
    let policies = ProviderPolicies::embedded();
    let gate = ConsentGate::dialog(ConsentBook::new());
    let with_text = fields(&[Url, Title, PageText]);

    let on = Redactor::new(RedactionSettings::default());
    let off = Redactor::new(RedactionSettings::off());
    let flows = [flow("suggest-title", "openai", &with_text)];

    let ask_on = &DataFlowOverview::consent_requests(&flows, &policies, &gate, on.is_enabled())[0];
    assert_eq!(ask_on.row.redaction_cell(), "on");
    assert!(!ask_on.pii_notice, "Schwärzung an: kein PII-Hinweis");

    let ask_off =
        &DataFlowOverview::consent_requests(&flows, &policies, &gate, off.is_enabled())[0];
    assert_eq!(ask_off.row.redaction_cell(), "off");
    assert!(
        ask_off.pii_notice,
        "Schwärzung aus + page_text: PII-Hinweis"
    );
}

// spec: AI-018
// „Nur wenn die Schwärzung aus ist und die Feldmenge `page_text`, `h2` oder `image_context` enthält": jeder der drei
// Auslöser einzeln, und keine Meldung bei Feldmengen ohne sie (der Stand der Schwärzung wird trotzdem gezeigt).
#[test]
fn ai018_k5_pii_hinweis_genau_bei_page_text_h2_oder_image_context() {
    let policies = ProviderPolicies::embedded();
    let gate = ConsentGate::dialog(ConsentBook::new());
    for field in InputField::ALL {
        let set = fields(&[Url, field]);
        let flows = [flow("t", "openai", &set)];
        for redaction in [true, false] {
            let ask = &DataFlowOverview::consent_requests(&flows, &policies, &gate, redaction)[0];
            let expected = !redaction && matches!(field, PageText | H2 | ImageContext);
            assert_eq!(
                ask.pii_notice, expected,
                "{field:?}, Schwärzung {redaction}"
            );
            assert_eq!(
                ask.row.redaction_cell(),
                if redaction { "on" } else { "off" }
            );
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Kriterium 6 (Bibliotheksteil): `--ai-consent` ersetzt den Dialog; ohne ihn laufen keine KI-Aufgaben, und der Lauf
// hat eine Meldung für die Ausgabe.

// spec: AI-018
#[tokio::test]
async fn ai018_k6_cli_ohne_schalter_keine_ki_aufgaben_und_meldung() {
    let mock = Mock::start().await;
    let gate = ConsentGate::cli(false);
    assert_eq!(gate.mode(), ConsentMode::Cli { flag: false });
    for task in ["suggest-title", "decision"] {
        assert_pending(&run(&mock, &gate, "openai", task, &title_fields(), "m1").await);
    }
    assert_eq!(mock.count(), 0);
    let msg = gate.cli_message().expect("Meldung für die CLI-Ausgabe");
    assert!(!msg.trim().is_empty());
}

// spec: AI-018
#[tokio::test]
async fn ai018_k6_cli_mit_schalter_ersetzt_den_dialog() {
    let mock = Mock::start().await;
    let gate = ConsentGate::cli(true);
    assert_eq!(gate.cli_message(), None);
    assert_sent(
        &run(
            &mock,
            &gate,
            "openai",
            "suggest-title",
            &title_fields(),
            "m1",
        )
        .await,
    );
    let mut more = title_fields();
    more.insert(PageText);
    assert_sent(&run(&mock, &gate, "openai", "summarize", &more, "m1").await);
    assert_eq!(mock.count(), 2);
}

// ---------------------------------------------------------------------------------------------------------------
// Ausgabe: Datenflusstabelle und CSV-Export; Protokollierung der Einwilligung.

// spec: AI-018
#[test]
fn ai018_ausgabe_datenflusstabelle_spalten_und_csv_export() {
    assert_eq!(
        COLUMNS,
        [
            "Task",
            "Provider",
            "Model",
            "Host",
            "Region",
            "Fields Sent",
            "Max Chars",
            "Images",
            "Redaction",
            "Training Use (per provider policy)",
            "Consent Given At",
        ]
    );
    let policies = ProviderPolicies::embedded();
    let gate = ConsentGate::dialog(ConsentBook::new());
    let mut f1 = flow(
        "summarize",
        "openai",
        &fields(&[Url, Title, PageText, Image]),
    );
    f1.max_chars = Some(8000);
    let mut f2 = flow("suggest-title", "anthropic", &title_fields());
    f2.endpoint = "https://api.anthropic.com/v1".into();
    f2.region = Some("eu".into());
    gate.grant(f2.consent_key(), GIVEN_AT_MS, None).unwrap();
    let flows = [f1, f2];

    let overview = DataFlowOverview::build(&flows, &policies, &gate, true);
    assert_eq!(overview.rows.len(), 2);
    let a = overview.rows[0].cells();
    assert_eq!(a[0], "summarize");
    assert_eq!(a[1], "openai");
    assert_eq!(a[2], "k07-model");
    assert_eq!(a[3], "api.openai.com", "Host aus dem Endpunkt");
    assert_eq!(a[4], "", "Region nur falls festgelegt");
    assert!(!a[5].is_empty(), "Fields Sent");
    assert_eq!(a[6], "8000");
    assert_eq!(a[8], "on");
    let openai = policies
        .get("openai")
        .expect("openai in provider-policies.json");
    assert!(a[9].contains(&openai.verified_at), "Prüfdatum: {}", a[9]);
    if let Some(src) = &openai.source_url {
        assert!(a[9].contains(src.as_str()), "Quelle: {}", a[9]);
    }
    assert_eq!(a[10], "", "ohne Einwilligung keine Zeit");

    let b = overview.rows[1].cells();
    assert_eq!(b[3], "api.anthropic.com");
    assert_eq!(b[4], "eu");
    assert_ne!(a[7], b[7], "Spalte Images unterscheidet Bild/kein Bild");
    assert!(
        b[10].starts_with("2026-11-24"),
        "Consent Given At: {}",
        b[10]
    );

    let csv = overview.to_csv();
    let lines: Vec<&str> = csv.lines().collect();
    assert_eq!(lines.len(), 3, "{csv}");
    assert_eq!(
        lines[0].trim_end_matches('\r'),
        COLUMNS.join(","),
        "Kopfzeile des CSV-Exports"
    );
    assert!(lines[1].starts_with("summarize,openai,k07-model,api.openai.com,"));
    assert!(lines[2].starts_with("suggest-title,anthropic,k07-model,api.anthropic.com,eu,"));
}

// spec: AI-018
// „dieser Eintrag macht die Angaben konkret und protokolliert sie": die Einwilligung hält Schlüssel, Zeit und die
// gezeigten Angaben fest.
#[test]
fn ai018_einwilligung_wird_mit_gezeigten_angaben_protokolliert() {
    let policies = ProviderPolicies::embedded();
    let gate = ConsentGate::dialog(ConsentBook::new());
    let flows = [flow("suggest-title", "openai", &title_fields())];
    let ask = DataFlowOverview::consent_requests(&flows, &policies, &gate, true).remove(0);
    gate.grant(flows[0].consent_key(), GIVEN_AT_MS, Some(ask.row.clone()))
        .unwrap();
    let book = gate.book();
    assert_eq!(book.records().len(), 1);
    let rec = &book.records()[0];
    assert_eq!(rec.key, flows[0].consent_key());
    assert_eq!(rec.given_at_ms, GIVEN_AT_MS);
    assert_eq!(rec.given_at(), "2026-11-24T09:30:00Z");
    assert_eq!(rec.shown.as_ref(), Some(&ask.row));
    // Das Protokoll ist speicherbar.
    let back = ConsentBook::from_json(&book.to_json()).unwrap();
    assert_eq!(back, book);
}
