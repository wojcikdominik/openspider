//! Unabhängige Abnahme M4-D01 – Eingabeaufbereitung (AI-032): Felder, Kürzung an Zeichengrenzen, Satzende-Regel,
//! leere Eingaben, Bilder, Bündelung und Geltungsbereich (08 Konventionen, `Skipped (scope)`).
//! Erwartungen nur aus `docs/spec/08-ai.md`.
// acceptance: independent
mod acceptance_d01_support;

use acceptance_d01_support as s;
use os_ai::decisions::input::{
    build_values, image_dimensions, page_text_limit, prepare_image, resized_dimensions,
    truncate_chars, truncate_page_text, ImageData, SubjectData,
};
use os_ai::policy::InputField;
use os_ai::AiStatus;
use serde_json::json;

/// Text aus `n` Zeichen ohne Satzende (Wörter aus Kleinbuchstaben, Leerzeichen).
fn plain(n: usize) -> Vec<char> {
    "lorem ipsum dolor sit amet "
        .chars()
        .cycle()
        .take(n)
        .collect()
}

/// Setzt an Position `pos` (1-basiert, Zeichen) ein Satzende `.` mit folgendem Leerzeichen.
fn sentence_end_at(chars: &mut [char], pos: usize) {
    chars[pos - 1] = '.';
    chars[pos] = ' ';
}

/// `long.html` nachgebildet (AI-032 Testfixture): 20.000 Zeichen, Satzenden bei 4.900 und 7.912, Umlaut an Position
/// 4.999/5.000.
fn long_text() -> String {
    let mut chars = plain(20_000);
    sentence_end_at(&mut chars, 4_900);
    sentence_end_at(&mut chars, 7_912);
    chars[4_998] = 'ü';
    chars[4_999] = 'ö';
    chars.into_iter().collect()
}

fn page_question(fields: serde_json::Value) -> os_ai::decisions::format::QuestionDef {
    s::question(json!({
        "id": "input-check",
        "version": "1.0.0",
        "name": "Input Check",
        "type": "predicate",
        "instructions": "Check the input.",
        "subject": "page",
        "inputs": fields
    }))
}

// spec: AI-032 [M4]
// acceptance: independent
#[test]
fn ai032_truncation_cuts_at_code_point_boundaries() {
    // Emoji (4 Byte), kombinierende Zeichen, CJK, Umlaute.
    let text: String = "a😀e\u{301}漢字ü👩\u{200d}💻"
        .chars()
        .cycle()
        .take(9_999)
        .collect();
    for max in [1usize, 2, 3, 4, 5, 7, 100, 4_999, 5_000, 9_998] {
        let cut = truncate_chars(&text, max);
        assert_eq!(cut.chars().count(), max, "max = {max}");
        assert!(
            text.starts_with(cut),
            "Kürzung ist kein Präfix (max = {max})"
        );
    }
    assert_eq!(
        truncate_chars(&text, 20_000),
        text,
        "kurzer Text bleibt unverändert"
    );
    // INTEG-172: Umlaut an Position 4.999/5.000 → genau 5.000 Zeichen, das Zeichen bleibt ganz.
    let long = long_text();
    let cut = truncate_chars(&long, 5_000);
    assert_eq!(cut.chars().count(), 5_000);
    assert!(cut.ends_with("üö"));
}

// spec: AI-032 [M4]
// acceptance: independent
#[test]
fn ai032_sentence_end_in_last_twentieth() {
    let long = long_text();
    // max_chars = 8000, Satzende bei 7.912 → 7.912 Zeichen.
    let cut = truncate_page_text(&long, 8_000);
    assert_eq!(cut.chars().count(), 7_912);
    assert!(cut.ends_with('.'));
    // Ohne max_chars gilt 5.000; Satzende bei 4.900 → 4.900 Zeichen.
    let cut = truncate_page_text(&long, 5_000);
    assert_eq!(cut.chars().count(), 4_900);
    // Der Paritäts-Prompt aus 04 (INTEG-172 unverändert) erhält genau 5.000 Zeichen.
    assert_eq!(truncate_chars(&long, 5_000).chars().count(), 5_000);
    // Kein Satzende im letzten Zwanzigstel → harte Grenze.
    let none: String = plain(20_000).into_iter().collect();
    assert_eq!(truncate_page_text(&none, 8_000).chars().count(), 8_000);
    // Grenzen des Fensters [7.600; 8.000].
    let mut at_7599 = plain(20_000);
    sentence_end_at(&mut at_7599, 7_599);
    let at_7599: String = at_7599.into_iter().collect();
    assert_eq!(truncate_page_text(&at_7599, 8_000).chars().count(), 8_000);
    let mut at_7600 = plain(20_000);
    sentence_end_at(&mut at_7600, 7_600);
    let at_7600: String = at_7600.into_iter().collect();
    assert_eq!(truncate_page_text(&at_7600, 8_000).chars().count(), 7_600);
    // Texte bis zur Höchstlänge bleiben unverändert.
    let short: String = plain(7_999).into_iter().collect();
    assert_eq!(truncate_page_text(&short, 8_000), short);
}

// spec: AI-032 [M4]
// acceptance: independent
#[test]
fn ai032_limit_per_question_or_model_setting() {
    let with_max = page_question(json!([{"field": "page_text", "max_chars": 8000}]));
    assert_eq!(page_text_limit(&with_max, Some(3_000)), 8_000);
    let without = page_question(json!(["page_text"]));
    assert_eq!(page_text_limit(&without, Some(3_000)), 3_000);
    assert_eq!(page_text_limit(&without, None), 5_000);
    // build_values wendet die Grenze an: Seite mit 20.000 Zeichen → 7.912 bzw. 4.900 Zeichen.
    let data = SubjectData {
        url: "https://ai.test:8080/long".into(),
        page_text: Some(long_text()),
        ..SubjectData::default()
    };
    let v = build_values(&with_max, &data, None, None, None).unwrap();
    assert_eq!(v.text(InputField::PageText).unwrap().chars().count(), 7_912);
    let v = build_values(&without, &data, None, None, None).unwrap();
    assert_eq!(v.text(InputField::PageText).unwrap().chars().count(), 4_900);
}

fn marked_subject() -> SubjectData {
    SubjectData {
        url: "https://ai.test:8080/fields?MARK-URL".into(),
        title: Some("Title MARK-TITLE".into()),
        meta_description: Some("Meta MARK-META".into()),
        h1: vec!["Heading MARK-H1".into()],
        h2: vec!["First MARK-H2A".into(), "Second MARK-H2B".into()],
        page_text: Some("Body MARK-PAGETEXT.".into()),
        word_count: Some(321),
        html_lang: Some("de".into()),
        hreflang_self: vec!["de".into()],
        structured_data_types: vec!["Product".into()],
        ..SubjectData::default()
    }
}

// spec: AI-032 [M4]
// acceptance: independent
#[test]
fn ai032_only_selected_fields_reach_the_input() {
    let q = page_question(json!(["title", "h1"]));
    let values = build_values(&q, &marked_subject(), None, None, None).unwrap();
    let fields: Vec<InputField> = values.fields().iter().collect();
    assert_eq!(fields, vec![InputField::Title, InputField::H1]);
    let block = values.text_block();
    assert!(block.contains("MARK-TITLE") && block.contains("MARK-H1"));
    for absent in ["MARK-PAGETEXT", "MARK-H2A", "MARK-META", "MARK-URL"] {
        assert!(
            !block.contains(absent),
            "{absent} gesendet, obwohl nicht gewählt"
        );
    }
    // h2: alle, durch Zeilenumbruch getrennt.
    let q = page_question(json!(["h2"]));
    let values = build_values(&q, &marked_subject(), None, None, None).unwrap();
    assert_eq!(
        values.text(InputField::H2),
        Some("First MARK-H2A\nSecond MARK-H2B")
    );
}

// spec: AI-032 [M4]
// acceptance: independent
#[test]
fn ai032_all_selected_fields_empty_is_skipped_empty_input() {
    let q = page_question(json!(["title", "meta_description"]));
    let empty = SubjectData {
        url: "https://ai.test:8080/empty".into(),
        title: None,
        meta_description: Some(String::new()),
        page_text: Some("There is text, but it is not selected.".into()),
        ..SubjectData::default()
    };
    assert_eq!(
        build_values(&q, &empty, None, None, None),
        Err(AiStatus::SkippedEmptyInput)
    );
}

// spec: AI-032 [M4]
// acceptance: independent
#[test]
fn ai032_image_context_is_capped_at_300_characters() {
    let q = s::question(json!({
        "id": "alt-fits",
        "version": "1.0.0",
        "name": "Alt Fits",
        "type": "predicate",
        "instructions": "The alt text describes the image.",
        "subject": "image",
        "inputs": ["image", "alt", "image_context"]
    }));
    let data = SubjectData {
        url: "https://ai.test:8080/img.png".into(),
        image: Some(ImageData {
            media_type: Some("image/png".into()),
            bytes: s::PNG_1X1.to_vec(),
        }),
        alt: Some("A red shoe".into()),
        image_context: Some("ü".repeat(500)),
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
    assert_eq!(values.images().len(), 1, "genau ein Bild");
}

// spec: AI-032 [M4]
// acceptance: independent
#[test]
fn ai032_image_size_rules() {
    assert_eq!(resized_dimensions(3_000, 2_000, 2_048), (2_048, 1_365));
    assert_eq!(resized_dimensions(2_000, 3_000, 2_048), (1_365, 2_048));
    assert_eq!(resized_dimensions(1_024, 768, 2_048), (1_024, 768));
    assert_eq!(resized_dimensions(2_048, 10, 2_048), (2_048, 10));
    assert_eq!(
        image_dimensions(&s::PNG_HEAD_3000X2000),
        Some((3_000, 2_000))
    );
    // Über 20 MB → übersprungen.
    let mut huge = s::PNG_1X1.to_vec();
    huge.resize(21 * 1024 * 1024, 0);
    let big = ImageData {
        media_type: Some("image/png".into()),
        bytes: huge,
    };
    assert_eq!(
        prepare_image(&big, None).map(|_| ()),
        Err(AiStatus::SkippedImageTooLarge)
    );
    // Kein Bildformat → nicht unterstützt.
    let text = ImageData {
        media_type: Some("text/plain".into()),
        bytes: b"this is not an image at all".to_vec(),
    };
    assert_eq!(
        prepare_image(&text, None).map(|_| ()),
        Err(AiStatus::SkippedUnsupportedFormat)
    );
    // 3.000 × 2.000 → Ziel 2.048 × 1.365.
    let head = ImageData {
        media_type: Some("image/png".into()),
        bytes: s::PNG_HEAD_3000X2000.to_vec(),
    };
    let prepared = prepare_image(&head, None).expect("PNG-Kopf wird angenommen");
    assert_eq!(prepared.target, Some((2_048, 1_365)));
}

// spec: AI-032 [M4]
// acceptance: independent
#[tokio::test]
#[ignore = "wartet auf M4-N01: Bildbibliothek zur Verkleinerung im Manifest von os-ai (D01 sendet ohne Resizer die Originalbytes; Befund im Bericht d01-tests)"]
async fn ai032_big_image_is_sent_as_2048x1365() {
    let bench = s::Bench::new(&["openai"], 1).await;
    let q = s::question(json!({
        "id": "alt-fits",
        "version": "1.0.0",
        "name": "Alt Fits",
        "type": "predicate",
        "instructions": "The alt text describes the image.",
        "subject": "image",
        "inputs": ["image", "alt"]
    }));
    let items = [s::image(1, s::PNG_HEAD_3000X2000.to_vec(), "Big", "")];
    bench.run(&[s::active(q)], &items, s::options()).await;
    let bodies = bench.mock.bodies(s::DECISIONS);
    assert_eq!(bodies.len(), 1);
    let text = bodies[0].to_string();
    let start = text.find("base64,").expect("Data-URL") + "base64,".len();
    let end = start + text[start..].find('"').unwrap();
    let sent = s::base64_decode(&text[start..end]);
    assert_eq!(image_dimensions(&sent), Some((2_048, 1_365)));
}

// spec: AI-032 [M4]
// acceptance: independent
#[tokio::test]
async fn ai032_same_fields_bundle_extra_field_second_request() {
    let bench = s::Bench::new(&["openai"], 1).await;
    let mut items = [
        s::page(1, "Shoes", "Red shoes for sale."),
        s::page(2, "Boots", "Brown boots for sale."),
    ];
    for item in &mut items {
        item.data.h2 = vec!["Second level heading".into()];
    }
    let questions = [
        s::active(s::predicate("q-a", "Question A", &["title", "page_text"])),
        s::active(s::predicate("q-b", "Question B", &["title", "page_text"])),
        s::active(s::choice(
            "q-c",
            "Question C",
            &["x", "y", "other"],
            &["title", "page_text"],
        )),
        s::active(s::predicate(
            "q-d",
            "Question D",
            &["title", "page_text", "h2"],
        )),
    ];
    bench.run(&questions, &items, s::options()).await;
    let bodies = bench.mock.bodies(s::DECISIONS);
    assert_eq!(bodies.len(), 4, "2 Requests je URL");
    for url in ["Shoes", "Boots"] {
        let mine: Vec<_> = bodies
            .iter()
            .filter(|b| s::decision_input_text(b).contains(url))
            .collect();
        assert_eq!(mine.len(), 2, "{url}: 2 Requests");
        let mut sizes: Vec<usize> = mine.iter().map(|b| s::decision_names(b).len()).collect();
        sizes.sort();
        assert_eq!(
            sizes,
            vec![1, 3],
            "{url}: 3 Fragen gebündelt, die vierte einzeln"
        );
        let single = mine
            .iter()
            .find(|b| s::decision_names(b).len() == 1)
            .unwrap();
        assert_eq!(s::decision_names(single), vec!["q_d".to_string()]);
    }
    for q in ["q-a", "q-b", "q-c", "q-d"] {
        assert_eq!(bench.count(q, "Success"), 2, "{q}");
    }
}

// spec: AI-032 [M4]
// acceptance: independent
#[tokio::test]
async fn ai032_unselected_field_markers_never_leave() {
    let bench = s::Bench::new(&["openai"], 1).await;
    let mut item = s::page(1, "Title MARK-TITLE", "Body MARK-PAGETEXT.");
    item.data = marked_subject();
    let q = s::predicate("title-only", "Title Only", &["title"]);
    bench.run(&[s::active(q)], &[item], s::options()).await;
    let log = serde_json::to_string(&bench.mock.log()).unwrap();
    assert!(log.contains("MARK-TITLE"));
    for absent in ["MARK-PAGETEXT", "MARK-H2A", "MARK-META", "MARK-H1"] {
        assert!(!log.contains(absent), "{absent} hat die Maschine verlassen");
    }
}

// spec: AI-032 [M4]
// acceptance: independent
#[tokio::test]
async fn ai032_scope_outside_s_is_skipped_without_request() {
    let bench = s::Bench::new(&["openai"], 1).await;
    let items = s::site(|i| format!("Content of page {i}."));
    let q = s::page_type(&["title", "page_text"]);
    bench.run(&[s::active(q)], &items, s::options()).await;
    assert_eq!(bench.count("page-type", "Success"), 22);
    assert_eq!(bench.count("page-type", "Skipped (scope)"), 3);
    for id in [23, 24, 25] {
        assert_eq!(bench.status(id, "page-type"), "Skipped (scope)");
    }
    assert_eq!(
        bench.mock.posts(s::DECISIONS).len(),
        22,
        "kein Request außerhalb von S"
    );
    let log = serde_json::to_string(&bench.mock.log()).unwrap();
    assert!(!log.contains("MARK-NOINDEX") && !log.contains("MARK-404"));
    // Jede Statuszeile enthält nur Werte aus AI-020.
    for row in bench.answers("page-type") {
        assert!(s::is_spec_status(&row.status), "Status {}", row.status);
    }
}

// spec: AI-032 [M4]
// acceptance: independent
#[tokio::test]
async fn ai032_empty_input_in_run_sends_nothing() {
    let bench = s::Bench::new(&["openai"], 1).await;
    let mut no_title = s::page(1, "", "Text.");
    no_title.data.title = None;
    let with_title = s::page(2, "Has a title", "Text.");
    let q = s::predicate("title-q", "Title Q", &["title"]);
    bench
        .run(&[s::active(q)], &[no_title, with_title], s::options())
        .await;
    assert_eq!(bench.status(1, "title-q"), "Skipped (empty input)");
    assert_eq!(bench.status(2, "title-q"), "Success");
    assert_eq!(bench.mock.posts(s::DECISIONS).len(), 1);
}

// spec: AI-032 [M4]
// acceptance: independent
#[tokio::test]
async fn ai032_page_type_result_feeds_lower_question() {
    let bench = s::Bench::new(&["openai"], 1).await;
    let pt = s::choice(
        "page-type",
        "Page Type",
        &["zzfirst", "zzsecond"],
        &["title"],
    );
    let dependent = s::predicate("is-shop", "Is Shop", &["title", "page_type"]);
    let items = [s::page(1, "Some page", "Text.")];
    bench
        .run(&[s::active(pt), s::active(dependent)], &items, s::options())
        .await;
    assert_eq!(bench.status(1, "page-type"), "Success");
    assert_eq!(bench.status(1, "is-shop"), "Success");
    let bodies = bench.mock.bodies(s::DECISIONS);
    let dep = bodies
        .iter()
        .find(|b| s::decision_names(b).contains(&"is_shop".to_string()))
        .expect("Request der abhängigen Frage");
    assert!(
        s::decision_input_text(dep).contains("zzfirst"),
        "Ergebnis der Seitentyp-Frage fehlt in der Eingabe"
    );
}

// spec: AI-032 [M4]
// acceptance: independent
#[tokio::test]
async fn ai032_segment_only_miss_is_skipped_segment() {
    // Nachtrag 26.11.2026 (Auslegung 11 bestätigt): verfehlt eine URL nur das Segment, `Skipped (segment)`.
    let bench = s::Bench::new(&["openai"], 1).await;
    let q = s::question(json!({
        "id": "shop-only",
        "version": "1.0.0",
        "name": "Shop Only",
        "type": "predicate",
        "instructions": "Is this a shop page?",
        "subject": "page",
        "inputs": ["title"],
        "scope": {"segment": "shop"}
    }));
    let mut in_segment = s::page(1, "Shop page", "Text.");
    in_segment.segments = vec!["shop".into()];
    let outside = s::page(2, "Blog page", "Text.");
    let mut missing = s::page(3, "Gone", "Text.");
    missing.status_code = Some(404);
    bench
        .run(
            &[s::active(q)],
            &[in_segment, outside, missing],
            s::options(),
        )
        .await;
    assert_eq!(bench.status(1, "shop-only"), "Success");
    assert_eq!(bench.status(2, "shop-only"), "Skipped (segment)");
    assert_eq!(bench.status(3, "shop-only"), "Skipped (scope)");
    assert_eq!(bench.mock.posts(s::DECISIONS).len(), 1);
}

// spec: AI-032 [M4]
// acceptance: independent
#[tokio::test]
async fn ai032_image_scope_ignores_indexability() {
    // Nachtrag 26.11.2026: ohne Angabe in `scope` gilt eine Bildfrage für interne Bilder mit Status 200.
    let bench = s::Bench::new(&["openai"], 1).await;
    let q = s::question(json!({
        "id": "alt-check",
        "version": "1.0.0",
        "name": "Alt Check",
        "type": "predicate",
        "instructions": "The alt text describes the image.",
        "subject": "image",
        "inputs": ["image", "alt"]
    }));
    let mut not_indexable = s::image(1, s::png_variant(1), "Alt one", "");
    not_indexable.indexable = false;
    let mut broken = s::image(2, s::png_variant(2), "Alt two", "");
    broken.status_code = Some(404);
    let page = s::page(3, "A page, not an image", "Text.");
    bench
        .run(
            &[s::active(q)],
            &[not_indexable, broken, page],
            s::options(),
        )
        .await;
    assert_eq!(bench.status(1, "alt-check"), "Success");
    assert_eq!(bench.status(2, "alt-check"), "Skipped (scope)");
    assert_eq!(bench.mock.posts(s::DECISIONS).len(), 1);
}
