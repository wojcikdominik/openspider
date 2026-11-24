//! Hilfsmodul der Policy-Tests (eingebunden per `#[path]`): Feldwerte aus den einfachen Seiten der Fixture-Site
//! `tests/fixtures/ai/site/` (Title, Meta Description, H1, H2, Text des `<main>`, Sprache, hreflang, JSON-LD-Typen,
//! erstes Bild mit Alt und Bildunterschrift). Ersetzt keinen Parser; die Extraktion für echte Läufe liefert D01.
#![allow(dead_code)]
use os_ai::policy::{FieldValue, FieldValues, InputField};
use std::path::PathBuf;

pub fn repo(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../")
        .join(name)
}

fn between(html: &str, a: &str, b: &str) -> Option<String> {
    let start = html.find(a)? + a.len();
    let end = start + html[start..].find(b)?;
    Some(html[start..end].to_string())
}

fn all_between(html: &str, a: &str, b: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = html;
    while let Some(i) = rest.find(a) {
        let tail = &rest[i + a.len()..];
        let Some(j) = tail.find(b) else { break };
        out.push(tail[..j].to_string());
        rest = &tail[j + b.len()..];
    }
    out
}

fn strip_tags(html: &str) -> String {
    let mut text = String::new();
    let mut in_tag = false;
    for c in html.chars() {
        match c {
            '<' => {
                in_tag = true;
                text.push(' ');
            }
            '>' => in_tag = false,
            c if !in_tag => text.push(c),
            _ => {}
        }
    }
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn site_html(file: &str) -> String {
    std::fs::read_to_string(repo(&format!("tests/fixtures/ai/site/{file}"))).unwrap()
}

/// Field values of a fixture page; only fields present in the page are set (plus `url` and `word_count`).
pub fn site_page(file: &str) -> FieldValues {
    let html = site_html(file);
    let mut v = FieldValues::new().with_text(
        InputField::Url,
        format!("http://ai.test:8080/{}", file.trim_end_matches(".html")),
    );
    let mut text = |field, value: Option<String>| {
        if let Some(value) = value.filter(|s| !s.is_empty()) {
            v.insert(field, FieldValue::Text(value));
        }
    };
    text(InputField::Title, between(&html, "<title>", "</title>"));
    text(
        InputField::MetaDescription,
        between(&html, "name=\"description\" content=\"", "\""),
    );
    text(
        InputField::H1,
        between(&html, "<h1>", "</h1>").map(|h| strip_tags(&h)),
    );
    let h2 = all_between(&html, "<h2>", "</h2>");
    text(
        InputField::H2,
        Some(
            h2.iter()
                .map(|h| strip_tags(h))
                .collect::<Vec<_>>()
                .join("\n"),
        ),
    );
    let main = between(&html, "<main>", "</main>").map(|m| strip_tags(&m));
    text(
        InputField::WordCount,
        main.as_ref()
            .map(|m| m.split_whitespace().count().to_string()),
    );
    text(InputField::PageText, main);
    text(InputField::HtmlLang, between(&html, "<html lang=\"", "\""));
    text(
        InputField::HreflangSelf,
        between(&html, "hreflang=\"", "\""),
    );
    text(
        InputField::StructuredDataTypes,
        Some(all_between(&html, "\"@type\":\"", "\"").join(", ")),
    );
    text(InputField::Alt, between(&html, " alt=\"", "\""));
    text(
        InputField::ImageContext,
        between(&html, "<figcaption>", "</figcaption>").map(|c| strip_tags(&c)),
    );
    if let Some(src) = between(&html, "src=\"data:", "\"") {
        let (media_type, base64) = src.split_once(";base64,").unwrap();
        v.insert(
            InputField::Image,
            FieldValue::Image {
                media_type: media_type.into(),
                base64: base64.into(),
            },
        );
    }
    v
}
