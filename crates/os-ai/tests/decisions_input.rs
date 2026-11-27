//! M4-D01: Eingabeaufbereitung (AI-032) – Feldauswahl, Kürzung, leere Eingaben, Bündelung, Bilder, `page_type`.
#[path = "decisions_support.rs"]
mod support;

use os_ai::decisions::exec::{RunControl, RunItem, RunOptions};
use os_ai::decisions::format::{self, InputSpec, QuestionDef};
use os_ai::decisions::input::{
    self, build_values, image_dimensions, prepare_image, resized_dimensions, truncate_chars,
    truncate_page_text, ImageData, SubjectData,
};
use os_ai::policy::InputField;
use os_ai::AiStatus;
use support::*;

fn long_text() -> String {
    page_data("http://ai.test:8080/long", &fixture_text("long.html"))
        .page_text
        .unwrap()
}

fn with_max(max: Option<u32>) -> QuestionDef {
    let mut q = question("soft-404");
    q.inputs = vec![InputSpec {
        field: InputField::PageText,
        max_chars: max,
    }];
    q
}

fn sent_page_text(q: &QuestionDef, data: &SubjectData, model: Option<u64>) -> String {
    build_values(q, data, None, model, None)
        .unwrap()
        .text(InputField::PageText)
        .unwrap()
        .to_string()
}

// spec: AI-032 [M4]
// `max_chars = 8000`, Seite mit 20.000 Zeichen → ≤ 8.000 Zeichen, Zeichengrenze, Satzende im letzten Zwanzigstel
// (7.600 … 8.000) → genau dort (`long.html`: Satzende bei 7.912 → 7.912 Zeichen).
#[test]
fn per_question_limit_with_sentence_end_rule() {
    let text = long_text();
    assert_eq!(text.chars().count(), 20_000);
    let data = SubjectData {
        page_text: Some(text.clone()),
        ..SubjectData::default()
    };
    let sent = sent_page_text(&with_max(Some(8000)), &data, None);
    assert_eq!(sent.chars().count(), 7_912);
    assert!(sent.ends_with('.'));
    assert!(text.starts_with(&sent));
}

// spec: AI-032 [M4]
// Ohne `max_chars` gilt die INTEG-172-Einstellung des Modells (Standard 5.000) plus Satzende-Regel (4.900 → 4.900).
// Die INTEG-172-Grundkürzung (Paritäts-Prompt aus 04) liefert unverändert genau 5.000 Zeichen an einer Zeichengrenze
// (Umlaut an Position 4.999/5.000).
#[test]
fn model_setting_default_and_integ_172_regression() {
    let text = long_text();
    let data = SubjectData {
        page_text: Some(text.clone()),
        ..SubjectData::default()
    };
    let sent = sent_page_text(&with_max(None), &data, None);
    assert_eq!(sent.chars().count(), 4_900);
    // Andere Modelleinstellung (INTEG-172 je Modell): 6.000 → Fenster 5.700 … 6.000 ohne Satzende → genau 6.000.
    assert_eq!(
        sent_page_text(&with_max(None), &data, Some(6000))
            .chars()
            .count(),
        6_000
    );
    let base = truncate_chars(&text, 5_000);
    assert_eq!(base.chars().count(), 5_000);
    assert!(base.ends_with("äö"));
    assert_eq!(input::DEFAULT_MAX_INPUT_CHARS, 5_000);
    // Ohne Satzende im Fenster: Schnitt an der Höchstlänge.
    assert_eq!(truncate_page_text(&text, 3_000).chars().count(), 3_000);
}

// spec: AI-032 [M4]
// Sind alle gewählten Felder leer (Title-Frage auf Seite ohne Title), wird nichts gesendet: `Skipped (empty input)`.
#[tokio::test]
async fn empty_input_is_skipped_without_request() {
    let mut q = question("soft-404");
    q.inputs = vec![InputSpec::field(InputField::Title)];
    let mut no_title = text_item(1, "Text ohne Title");
    no_title.data.title = None;
    let with_title = text_item(2, "MOCK-ID:home Text");
    assert_eq!(
        build_values(&q, &no_title.data, None, None, None),
        Err(AiStatus::SkippedEmptyInput)
    );
    let env = Env::new(&["openai"]).await;
    let items = vec![no_title, with_title];
    let dir = TempDir::new("empty");
    let (file, run) = crawl_file(&dir.0, &items);
    let ledger = support::ledger(file.conn(), 5_000_000);
    let qs = [active(q.clone())];
    env.run(
        file.conn(),
        run,
        &qs,
        &items,
        &ledger,
        RunControl::new(),
        RunOptions::default(),
    )
    .run()
    .await
    .unwrap();
    let c = counts(file.conn(), &q);
    assert_eq!(count(&c, "Skipped (empty input)"), 1);
    assert_eq!(count(&c, "Success"), 1);
    assert_eq!(env.mock.posts(DECISIONS).len(), 1);
}

// spec: AI-032 [M4]
// Eine Frage ohne Feld `page_text` sendet keinen Seitentext (Feldmarker im Mock-Log).
#[tokio::test]
async fn question_without_page_text_sends_no_page_text() {
    let mut q = question("soft-404");
    q.inputs = vec![
        InputSpec::field(InputField::Title),
        InputSpec::field(InputField::Url),
    ];
    let items = vec![text_item(1, "FIELD-MARKER-PAGE-TEXT im Inhalt")];
    let env = Env::new(&["openai"]).await;
    let dir = TempDir::new("marker");
    let (file, run) = crawl_file(&dir.0, &items);
    let ledger = support::ledger(file.conn(), 5_000_000);
    let qs = [active(q)];
    env.run(
        file.conn(),
        run,
        &qs,
        &items,
        &ledger,
        RunControl::new(),
        RunOptions::default(),
    )
    .run()
    .await
    .unwrap();
    let posts = env.mock.posts(DECISIONS);
    assert_eq!(posts.len(), 1);
    assert!(!posts[0].body.contains("FIELD-MARKER-PAGE-TEXT"));
    assert!(posts[0].body.contains("Title: Seite 1"));
}

fn same_fields(id: &str, name: &str) -> QuestionDef {
    let mut q = question("soft-404");
    q.id = id.into();
    q.name = name.into();
    q.inputs = vec![
        InputSpec::field(InputField::Title),
        InputSpec::field(InputField::H1),
    ];
    q
}

// spec: AI-032 [M4]
// Drei Seitenfragen mit gleicher Feldmenge und gleichem Weg → 1 Request je URL; eine vierte Frage mit zusätzlichem
// Feld `h2` → ein zweiter Request je URL.
#[tokio::test]
async fn identical_inputs_share_a_request() {
    let mut fourth = same_fields("q-four", "Q4");
    fourth.inputs.push(InputSpec::field(InputField::H2));
    let qs: Vec<_> = vec![
        same_fields("q-one", "Q1"),
        same_fields("q-two", "Q2"),
        same_fields("q-three", "Q3"),
        fourth,
    ]
    .into_iter()
    .map(active)
    .collect();
    let mut items: Vec<RunItem> = (1..=3).map(|i| text_item(i, "Text")).collect();
    for item in &mut items {
        item.data.h2 = vec!["Abschnitt".into(), "Zweiter".into()];
    }
    let env = Env::new(&["openai"]).await;
    let dir = TempDir::new("bundle");
    let (file, run) = crawl_file(&dir.0, &items);
    let ledger = support::ledger(file.conn(), 5_000_000);
    env.run(
        file.conn(),
        run,
        &qs,
        &items,
        &ledger,
        RunControl::new(),
        RunOptions::default(),
    )
    .run()
    .await
    .unwrap();
    let bodies = env.mock.bodies(DECISIONS);
    assert_eq!(bodies.len(), 6, "2 Requests je URL");
    let sizes: Vec<usize> = bodies
        .iter()
        .map(|b| b["questions"].as_array().unwrap().len())
        .collect();
    assert_eq!(sizes.iter().filter(|&&n| n == 3).count(), 3);
    assert_eq!(sizes.iter().filter(|&&n| n == 1).count(), 3);
    // H2 durch Zeilenumbruch getrennt.
    assert!(bodies
        .iter()
        .any(|b| b["input"].to_string().contains("H2: Abschnitt\\nZweiter")));
}

/// PNG-Kopf mit den Maßen `w` × `h` (nur der Kopf, für Maße und Formaterkennung).
fn png_header(w: u32, h: u32) -> Vec<u8> {
    let mut b = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
    b.extend_from_slice(&w.to_be_bytes());
    b.extend_from_slice(&h.to_be_bytes());
    b.extend_from_slice(&[8, 6, 0, 0, 0]);
    b
}

// spec: AI-032 [M4]
// Bildgrößen: Bild 3.000 × 2.000 px → Zielmaß 2.048 × 1.365 px; über 20 MB → `Skipped (image too large)`, kein Bild
// → `Skipped (unsupported format)`; `image_context` höchstens 300 Zeichen.
#[test]
fn image_limits() {
    assert_eq!(resized_dimensions(3000, 2000, 2048), (2048, 1365));
    let png = png_header(3000, 2000);
    assert_eq!(image_dimensions(&png), Some((3000, 2000)));
    let prepared = prepare_image(
        &ImageData {
            media_type: None,
            bytes: png,
        },
        None,
    )
    .unwrap();
    assert_eq!(prepared.target, Some((2048, 1365)));
    let big = ImageData {
        media_type: None,
        bytes: {
            let mut b = png_header(10, 10);
            b.resize(20 * 1024 * 1024 + 1, 0);
            b
        },
    };
    assert_eq!(
        prepare_image(&big, None),
        Err(AiStatus::SkippedImageTooLarge)
    );
    let svg = ImageData {
        media_type: Some("image/svg+xml".into()),
        bytes: b"<svg/>".to_vec(),
    };
    assert_eq!(
        prepare_image(&svg, None),
        Err(AiStatus::SkippedUnsupportedFormat)
    );
    let q = question("alt-fits");
    let data = SubjectData {
        image: Some(ImageData {
            media_type: None,
            bytes: png_header(4, 4),
        }),
        alt: Some("Rucksack".into()),
        image_context: Some("x".repeat(500)),
        ..SubjectData::default()
    };
    let values = build_values(&q, &data, None, None, None).unwrap();
    assert_eq!(
        values
            .text(InputField::ImageContext)
            .unwrap()
            .chars()
            .count(),
        300
    );
    assert_eq!(values.images().len(), 1);
}

// spec: AI-032 [M4]
// Der Cache-Schlüssel basiert auf den gesendeten Bytes (nach einer Verkleinerung): eine Verkleinerung mit anderem
// Ergebnis ändert den Schlüssel, gleiche gesendete Bytes ergeben denselben.
#[test]
fn image_cache_key_uses_sent_bytes() {
    struct Shrink(Vec<u8>);
    impl input::ImageResizer for Shrink {
        fn resize(&self, _: &[u8], m: &str, _: u32, _: u32) -> Option<(Vec<u8>, String)> {
            Some((self.0.clone(), m.to_string()))
        }
    }
    let q = question("alt-fits");
    let original = ImageData {
        media_type: None,
        bytes: png_header(3000, 2000),
    };
    let data = SubjectData {
        image: Some(original),
        alt: Some("Rucksack".into()),
        ..SubjectData::default()
    };
    let small = Shrink(png_header(2048, 1365));
    let resized = build_values(&q, &data, None, None, Some(&small)).unwrap();
    let unchanged = build_values(&q, &data, None, None, None).unwrap();
    let hash = |v: &os_ai::policy::FieldValues| {
        os_ai::cache::input_hash(
            &os_ai::policy::PreparedInput::new(
                v,
                &q.field_set(),
                &os_ai::policy::Redactor::new(os_ai::policy::RedactionSettings::off()),
            )
            .inputs(),
        )
    };
    assert_ne!(hash(&resized), hash(&unchanged));
    assert_eq!(
        hash(&resized),
        hash(&build_values(&q, &data, None, None, Some(&small)).unwrap())
    );
    let sent = prepare_image(&data.image.clone().unwrap(), Some(&small)).unwrap();
    assert_eq!(sent.dimensions, Some((2048, 1365)));
}

// spec: AI-032 [M4]
// `page_type` als Eingabe einer Frage, die über der Seitentyp-Frage steht, wird beim Speichern abgelehnt; unterhalb
// erhält sie das Ergebnis der Seitentyp-Frage.
#[tokio::test]
async fn page_type_input_requires_the_page_type_question_above() {
    let pt = question("page-type");
    let dependent = question("type-title");
    let errors = format::validate_active(&[dependent.clone(), pt.clone()]).unwrap_err();
    assert_eq!(errors[0].field, "/0/inputs");
    assert!(format::validate_active(std::slice::from_ref(&dependent)).is_err());
    assert!(format::validate_active(&[pt.clone(), dependent.clone()]).is_ok());
    // Im Lauf: die abhängige Frage bekommt den Seitentyp der URL.
    let env = Env::new(&["openai"]).await;
    let items = vec![text_item(1, "MOCK-ID:product-1 Wanderrucksack")];
    let dir = TempDir::new("pt");
    let (file, run) = crawl_file(&dir.0, &items);
    let ledger = support::ledger(file.conn(), 5_000_000);
    let qs = [active(pt), active(dependent)];
    env.run(
        file.conn(),
        run,
        &qs,
        &items,
        &ledger,
        RunControl::new(),
        RunOptions::default(),
    )
    .run()
    .await
    .unwrap();
    let bodies = env.mock.bodies(DECISIONS);
    assert_eq!(bodies.len(), 2);
    assert!(bodies[1]["input"]
        .to_string()
        .contains("Page Type: product"));
}
