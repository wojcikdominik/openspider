#[path = "framework_common.rs"]
mod common;
use common::*;
use os_analyze::views::{query_view, ViewQuery};
use os_model::{CrawlConfig, Value};

// spec: ONPAGE-142 [P1] [M2]
#[test]
fn pdf_readability_and_language_totals_are_in_the_enabled_default_export() {
    let dir = tempfile::tempdir().unwrap();
    let (_, file) = create(dir.path(), "n08.ospider", 8);
    insert(
        &file,
        &[row(1, "doc.pdf")
            .text("content_class", "PDF")
            .int("status_code", 200)
            .int("word_count", 6)],
    );
    file.conn()
        .execute_batch(
            "INSERT INTO url_ext(url_id,key,v_int,v_real,v_text) VALUES
        (1,'readability.sentences',1,NULL,NULL),
        (1,'readability.average',NULL,6,NULL),
        (1,'readability.flesch',NULL,100,NULL),
        (1,'readability.label',NULL,NULL,'Very Easy'),
        (1,'spelling.spelling_errors',2,NULL,NULL),
        (1,'spelling.grammar_errors',3,NULL,NULL);",
        )
        .unwrap();
    let mut cfg = CrawlConfig::default();
    cfg.pdf.store = true;
    cfg.spelling.enabled = true;
    cfg.grammar.enabled = true;
    let query = ViewQuery::new("Content", "All");
    let page = query_view(&file, &cfg, &query).unwrap();
    for (key, expected) in [
        ("Word Count", Value::Int(6)),
        ("Sentence Count", Value::Int(1)),
        ("Average Words Per Sentence", Value::Float(6.0)),
        ("Flesch Reading Ease Score", Value::Float(100.0)),
        ("Readability", Value::Text("Very Easy".into())),
        ("Hash", Value::Null),
        ("Total Language Errors", Value::Int(5)),
    ] {
        let index = page.columns.iter().position(|c| c == key).unwrap();
        assert_eq!(page.rows[0][index], expected, "{key}");
    }
    for (spelling, grammar) in [(true, false), (false, true)] {
        cfg.spelling.enabled = spelling;
        cfg.grammar.enabled = grammar;
        let page = query_view(&file, &cfg, &query).unwrap();
        for key in ["Total Language Errors", "Language"] {
            assert!(page.columns.iter().any(|c| c == key));
        }
        assert_eq!(
            page.columns.iter().any(|c| c == "Spelling Errors"),
            spelling
        );
        assert_eq!(page.columns.iter().any(|c| c == "Grammar Errors"), grammar);
    }
    cfg.extraction.readability = false;
    let page = query_view(&file, &cfg, &query).unwrap();
    assert!(!page.columns.iter().any(|c| c == "Sentence Count"));
    cfg.spelling.enabled = false;
    cfg.grammar.enabled = false;
    let page = query_view(&file, &cfg, &query).unwrap();
    for key in [
        "Total Language Errors",
        "Spelling Errors",
        "Grammar Errors",
        "Language",
    ] {
        assert!(!page.columns.iter().any(|c| c == key));
    }
}

// spec: ONPAGE-147
#[test]
fn soft404_default_export_keeps_address_and_matched_phrase() {
    let dir = tempfile::tempdir().unwrap();
    let (_, file) = create(dir.path(), "n08-soft.ospider", 8);
    insert(
        &file,
        &[row(1, "missing.html")
            .text("content_class", "HTML")
            .int("status_code", 200)
            .text("indexability", "Indexable")],
    );
    file.conn().execute("INSERT INTO url_ext(url_id,key,v_text) VALUES (1,'content.soft404_text','seite nicht gefunden')",[]).unwrap();
    let page = query_view(
        &file,
        &CrawlConfig::default(),
        &ViewQuery::new("Content", "Soft 404 Pages"),
    )
    .unwrap();
    assert_eq!(page.total, 1);
    assert_eq!(page.columns[0], "Address");
    let index = page
        .columns
        .iter()
        .position(|c| c == "Matched Phrase")
        .unwrap();
    assert_eq!(
        page.rows[0][index],
        Value::Text("seite nicht gefunden".into())
    );
}
