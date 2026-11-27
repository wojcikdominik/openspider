//! Unabhängige Abnahmetests M4-V01 (1.1-Etappe): Kern der Optimierungsvorschläge – lokale Prüfung (AI-075),
//! Dublettenprüfung in zwei Stufen (AI-071), Ablauf je URL und Runde (AI-070, AI-075), Prüfen/Übernehmen (AI-076)
//! sowie die übergreifenden Regeln des Kerns (AI-013, AI-014, AI-015, AI-017, AI-018, AI-020, AI-021, AI-038).
//!
//! Erwartungen ausschließlich aus `docs/spec/08-ai.md` (Stand `main` 8ae20980) und `docs/spec/02-tabs-onpage.md`
//! (Zeichenlänge = Codepoints, Filterlogik ONPAGE-093 ff./103 ff.). Bericht, Tests und Commits des Bauers wurden nicht
//! gelesen. Der Anbieter ist ein geskriptetes Testdoppel hinter `policy::guard::execute` (siehe Support-Modul); die
//! Gegenprobe gegen den ai-mock steht in `acceptance_v01_mock.rs`.
// acceptance: independent

#[path = "acceptance_v01_support/mod.rs"]
mod support;

use std::collections::BTreeMap;
use std::sync::Mutex;

use os_ai::tasks::suggest::{
    self as sg, Checker, Element, ElementLimits, NoStyleRules, ReviewAction, RuleSet, RunError,
    SiteIndex, SuggestOptions, SuggestStore,
};
use os_ai::PreviewSwitch;
use os_model::budget::{BudgetLedger, CrawlBudgetLedger};
use os_store::ai::suggestions::{self as store, ElementRecord, Origin, ReviewStatus};
use support::*;

const VALID: &str = "Valid";
const NO_VALID: &str = "No valid suggestion";

fn checker<'a>(
    element: Element,
    limits: ElementLimits,
    rules: RuleSet,
    meter: &'a TableMeter,
    index: &'a SiteIndex,
) -> Checker<'a> {
    Checker {
        element,
        limits,
        rules,
        meter,
        index,
        style: &NoStyleRules,
    }
}

fn title_checker<'a>(meter: &'a TableMeter, index: &'a SiteIndex) -> Checker<'a> {
    checker(
        Element::Title,
        TITLE_LIMITS,
        RuleSet::default(),
        meter,
        index,
    )
}

fn rt() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(4)
        .enable_all()
        .build()
        .unwrap()
}

/// Angezeigter Kandidat 1 (= gewählter Kandidat, Festlegung des Orchestrators).
fn first_shown(r: &ElementRecord) -> String {
    r.displayed()
        .first()
        .map(|c| c.value.clone())
        .unwrap_or_default()
}

// =================================================================================================================
// AI-075 Regeln (1)/(2): Grenzwerte genau wie die Filterlogik aus 02.

// spec: AI-075 [M4]
// acceptance: independent
// „Title: 60 Zeichen gültig, 61 ungültig; 30 gültig, 29 ungültig“ (Pixelbreite dabei im erlaubten Bereich).
#[test]
fn title_length_limits_match_the_filters_plus_minus_one() {
    let meter = TableMeter::default();
    let index = SiteIndex::default();
    let c = title_checker(&meter, &index);
    for (n, valid) in [(60, true), (61, false), (30, true), (29, false), (45, true)] {
        let text = text_of("Wanderschuhe ", n);
        let checked = c.check(1, None, &text);
        assert_eq!(checked.length, n as u32, "Länge von {text:?}");
        assert_eq!(
            checked.valid(),
            valid,
            "{n} Zeichen: {:?}",
            checked.violations
        );
    }
}

// spec: AI-075 [M4]
// acceptance: independent
// „561 px gültig, 562 px ungültig“; untere Grenze 200 px gültig, 199 ungültig; Verstoßcode `pixel_width>561` (Beispiel
// der Spalte `… Violations`).
#[test]
fn title_pixel_limits_match_the_filters_plus_minus_one() {
    let t = |s: &str| text_of(s, 40);
    let meter = TableMeter::default()
        .with(&t("P561 "), 561)
        .with(&t("P562 "), 562)
        .with(&t("P200 "), 200)
        .with(&t("P199 "), 199);
    let index = SiteIndex::default();
    let c = title_checker(&meter, &index);
    for (text, px, valid) in [
        (t("P561 "), 561, true),
        (t("P562 "), 562, false),
        (t("P200 "), 200, true),
        (t("P199 "), 199, false),
    ] {
        let checked = c.check(1, None, &text);
        assert_eq!(checked.pixel_width, px);
        assert_eq!(checked.valid(), valid, "{px} px: {:?}", checked.violations);
    }
    let over = c.check(1, None, &t("P562 "));
    assert!(
        over.violations.iter().any(|v| v == "pixel_width>561"),
        "{:?}",
        over.violations
    );
}

// spec: AI-075 [M4]
// spec: AI-072 [M4]
// acceptance: independent
// Meta Description Standard 70–155 Zeichen und 400–985 px, je ±1.
#[test]
fn meta_description_limits_plus_minus_one() {
    let t = |s: &str| text_of(s, 120);
    let meter = TableMeter::default()
        .with(&t("P985 "), 985)
        .with(&t("P986 "), 986)
        .with(&t("P400 "), 400)
        .with(&t("P399 "), 399);
    let index = SiteIndex::default();
    let c = checker(
        Element::MetaDescription,
        META_LIMITS,
        RuleSet::default(),
        &meter,
        &index,
    );
    for (n, valid) in [(155, true), (156, false), (70, true), (69, false)] {
        let checked = c.check(1, None, &text_of("Beschreibung ", n));
        assert_eq!(
            checked.valid(),
            valid,
            "{n} Zeichen: {:?}",
            checked.violations
        );
    }
    for (prefix, valid) in [
        ("P985 ", true),
        ("P986 ", false),
        ("P400 ", true),
        ("P399 ", false),
    ] {
        let checked = c.check(1, None, &t(prefix));
        assert_eq!(checked.valid(), valid, "{prefix}: {:?}", checked.violations);
    }
}

// spec: AI-075 [M4]
// acceptance: independent
// „Regeln (1)–(6) einzeln abschaltbar außer (2)“: ohne Längenregel ist ein 61-Zeichen-Title gültig, die Pixelregel greift
// auch mit allen abgeschalteten Regeln.
#[test]
fn every_rule_but_pixel_width_can_be_switched_off() {
    let t = text_of("P562 ", 40);
    let meter = TableMeter::default().with(&t, 562);
    let index = SiteIndex::default();
    let off = RuleSet {
        length: false,
        duplicates: false,
        format: false,
        language: false,
        style: false,
    };
    let c = checker(Element::Title, TITLE_LIMITS, off, &meter, &index);
    assert!(c.check(1, None, &text_of("Lang ", 61)).valid());
    assert!(
        !c.check(1, None, &t).valid(),
        "Pixelregel ist nicht abschaltbar"
    );
    assert!(c
        .check(
            1,
            None,
            "{{x}} Platzhalter bleibt ohne Regel 4 erlaubt hier"
        )
        .valid());
}

// spec: AI-075 [M4]
// spec: AI-071 [M4]
// acceptance: independent
// Zeichenlänge = Anzahl Unicode-Codepoints des normalisierten Texts (02, „Zeichenlänge“, Annahme A-2): Umlaute 1,
// CJK je 1, Familien-Emoji mit ZWJ = 5 Codepoints; Whitespace-Folgen zählen als ein Leerzeichen, Rand-Whitespace nicht.
#[test]
fn length_counts_codepoints_for_umlauts_cjk_and_emoji() {
    let meter = TableMeter::default();
    let index = SiteIndex::default();
    let c = title_checker(&meter, &index);
    assert_eq!(c.check(1, None, "Ärger über Öl").length, 13);
    assert_eq!(c.check(1, None, "漢字テスト").length, 5);
    assert_eq!(c.check(1, None, "👨\u{200d}👩\u{200d}👧").length, 5);
    assert_eq!(
        c.check(1, None, "Ärger über Öl 漢字テスト 👨\u{200d}👩\u{200d}👧")
            .length,
        25
    );
    assert_eq!(c.check(1, None, "  Viel   Raum  ").length, 9);
}

// =================================================================================================================
// AI-075 Regel (4) und AI-072.

// spec: AI-075 [M4]
// acceptance: independent
// Regel (4) für Titles: unaufgelöste Platzhalter (`{{`, `[`…`]`), Zeilenumbrüche, umschließendes Anführungszeichenpaar
// sind Verstöße.
#[test]
fn title_format_rule_flags_placeholders_line_breaks_and_quotes() {
    let meter = TableMeter::default();
    let index = SiteIndex::default();
    let c = title_checker(&meter, &index);
    for bad in [
        "{{brand}} Wanderschuhe für Damen im Test",
        "[Marke] Wanderschuhe für Damen im großen Test",
        "Wanderschuhe für Damen\nim großen Test 2026",
        "\"Wanderschuhe für Damen im großen Test\"",
    ] {
        assert!(
            !c.check(1, None, bad).valid(),
            "{bad:?} müsste ungültig sein"
        );
    }
    assert!(c
        .check(1, None, "Wanderschuhe für Damen im großen Test")
        .valid());
}

// spec: AI-072 [M4]
// acceptance: independent
// „Kandidaten enthalten keine Zeilenumbrüche und keine doppelten Anführungszeichen am Anfang/Ende (werden entfernt,
// bevor geprüft wird)“.
#[test]
fn meta_description_strips_line_breaks_and_surrounding_quotes_before_checking() {
    let meter = TableMeter::default();
    let index = SiteIndex::default();
    let c = checker(
        Element::MetaDescription,
        META_LIMITS,
        RuleSet::default(),
        &meter,
        &index,
    );
    let body = text_of("Robuste Wanderschuhe fuer Damen und Herren ", 90);
    let raw = format!("\"{}\n{}\"", &body[..40], &body[40..]);
    let checked = c.check(1, None, &raw);
    assert!(checked.valid(), "{:?}", checked.violations);
    assert!(!checked.value.contains('\n'), "{:?}", checked.value);
    assert!(!checked.value.starts_with('"') && !checked.value.ends_with('"'));
}

// =================================================================================================================
// AI-075 Regel (5): Sprache.

// spec: AI-075 [M4]
// spec: AI-071 [M4]
// acceptance: independent
// Verstoß `language:<erkannt>` nur bei einer anderen Sprache mit Konfidenz ≥ 0,95; Kandidaten unter 20 Zeichen werden
// nicht geprüft; maßgeblich ist die Seitensprache (Primär-Subtag eines BCP-47-Codes).
#[test]
fn language_rule_uses_page_language_and_skips_short_candidates() {
    let meter = TableMeter::default();
    let index = SiteIndex::default();
    let c = title_checker(&meter, &index);
    // Lange englische Description (eindeutig erkennbar), Seite deutsch.
    let m = checker(
        Element::MetaDescription,
        META_LIMITS,
        RuleSet::default(),
        &meter,
        &index,
    );
    let long = "Discover the best hiking boots for every season and trail, with honest reviews, \
                practical buying advice and the latest models from all the major brands.";
    let v = m.check(1, Some("de"), long);
    assert!(
        v.violations.iter().any(|x| x == "language:en"),
        "{:?}",
        v.violations
    );
    let v = m.check(1, Some("de-DE"), long);
    assert!(
        v.violations.iter().any(|x| x == "language:en"),
        "{:?}",
        v.violations
    );
    assert!(!m
        .check(1, Some("en"), long)
        .violations
        .iter()
        .any(|x| x.starts_with("language:")));
    // 19 Zeichen: keine Sprachprüfung.
    let short = "Best hiking boots!!";
    assert_eq!(short.chars().count(), 19);
    assert!(!c
        .check(1, Some("de"), short)
        .violations
        .iter()
        .any(|x| x.starts_with("language:")));
    // Ohne Seitensprache keine Sprachprüfung.
    assert!(!m
        .check(1, None, long)
        .violations
        .iter()
        .any(|x| x.starts_with("language:")));
}

// =================================================================================================================
// AI-071 Stufe 1.

// spec: AI-071 [M4]
// spec: AI-075 [M4]
// acceptance: independent
// Stufe 1: case-insensitiv nach Whitespace-Normalisierung gegen die Titles aller **anderen indexierbaren** URLs; der
// eigene Title und Titles nicht indexierbarer URLs sind keine Dublette.
#[test]
fn stage_one_duplicates_only_against_other_indexable_urls() {
    let dir = TempDir::new("stage1");
    let pages = vec![
        Page::new("/a/", Some("Wanderschuhe für Damen im großen Vergleich")),
        Page::new("/n/", Some("Regenjacken für Herren im großen Vergleich")).noindex(),
        Page::new("/b/", Some("Trekkingstöcke aus Carbon im großen Vergleich")),
    ];
    let (file, _) = crawl_file(&dir.0, &pages);
    let index = site_index(&file);
    let meter = TableMeter::default();
    let c = title_checker(&meter, &index);
    let dup = c.check(3, None, "  wanderschuhe FÜR damen   im großen Vergleich ");
    assert!(
        dup.violations
            .iter()
            .any(|v| v.starts_with("duplicate_of:") && v.contains("/a/")),
        "{:?}",
        dup.violations
    );
    assert!(c
        .check(3, None, "Regenjacken für Herren im großen Vergleich")
        .valid());
    assert!(c
        .check(3, None, "Trekkingstöcke aus Carbon im großen Vergleich")
        .valid());
}

// =================================================================================================================
// Ablauf je URL und Runde (AI-070, AI-075) mit dem geskripteten Anbieter.

// spec: AI-070 [M4]
// spec: AI-075 [M4]
// acceptance: independent
// Sofort gültig: genau ein Request; Kandidaten in der Reihenfolge der Antwort gespeichert; der erste gültige Kandidat
// ist gewählt und als Kandidat 1 angezeigt.
#[test]
fn immediately_valid_sends_one_request_and_keeps_answer_order() {
    let invalid = text_of("Zu lang ", 70);
    let (v1, v2) = (valid_title("eins"), valid_title("zwei"));
    let kit = Kit::new("valid", vec![Page::new("/p/", Some("Alt"))]);
    let provider = Scripted::new().script("/p/", vec![cands(&[&invalid, &v1, &v2])]);
    let meter = TableMeter::default();
    let summary = rt()
        .block_on(
            kit.engine(&provider, &meter)
                .run(&kit.job(Element::Title, options())),
        )
        .unwrap();
    assert_eq!(provider.count(), 1);
    assert_eq!(summary.requests, 1);
    let r = kit.record("/p/", Element::Title);
    assert_eq!(r.status, VALID);
    let values: Vec<&str> = r.candidates.iter().map(|c| c.value.as_str()).collect();
    assert_eq!(values, [invalid.as_str(), v1.as_str(), v2.as_str()]);
    assert_eq!(first_shown(&r), v1);
    assert!(!r.provisional, "nach dem Nachlauf nicht mehr vorläufig");
    assert_eq!(kit.log_rows("/p/").len(), 1);
}

// spec: AI-075 [M4]
// spec: AI-014 [M4]
// acceptance: independent
// Akzeptanzkriterium: „Mock liefert im ersten Versuch 3 Kandidaten mit 612, 580 und 590 px, im zweiten einen mit 540 px →
// Status Valid, gültiger Kandidat 540 px, Protokoll zeigt 2 Requests, und der zweite Request enthält die drei Verstöße“
// (Form wie im Beispiel „612 px > 561 px“). Jede Runde hat ihre Protokollzeile mit Kosten.
#[test]
fn repair_round_example_612_580_590_then_540() {
    let c = |s: &str| text_of(s, 45);
    let (c1, c2, c3, c4) = (c("Erster "), c("Zweiter "), c("Dritter "), c("Vierter "));
    let meter = TableMeter::default()
        .with(&c1, 612)
        .with(&c2, 580)
        .with(&c3, 590)
        .with(&c4, 540);
    let kit = Kit::new("ex", vec![Page::new("/p/", Some("Alt"))]);
    let provider = Scripted::new().script("/p/", vec![cands(&[&c1, &c2, &c3]), cands(&[&c4])]);
    rt().block_on(
        kit.engine(&provider, &meter)
            .run(&kit.job(Element::Title, options())),
    )
    .unwrap();
    let calls = provider.calls_for("/p/");
    assert_eq!(calls.len(), 2, "2 Requests");
    for px in ["612 px > 561 px", "580 px > 561 px", "590 px > 561 px"] {
        assert!(
            calls[1].instructions.contains(px) || calls[1].input.contains(px),
            "zweiter Request ohne Verstoß {px}: {}",
            calls[1].instructions
        );
    }
    let r = kit.record("/p/", Element::Title);
    assert_eq!(r.status, VALID);
    let chosen = r.selected_candidate().expect("gewählter Kandidat");
    assert_eq!(chosen.value, c4);
    assert_eq!(chosen.pixel_width, Some(540));
    assert_eq!(first_shown(&r), c4);
    let rows = kit.log_rows("/p/");
    assert_eq!(rows.len(), 2, "eine Protokollzeile je Runde");
    assert!(
        rows.iter().all(|(hit, cost, _, _)| !hit && *cost > 0),
        "{rows:?}"
    );
}

// spec: AI-075 [M4]
// acceptance: independent
// Nach zwei Reparaturrunden gültig: 1 + 2 Requests, Status `Valid`.
#[test]
fn valid_after_two_repair_rounds() {
    let kit = Kit::new("two", vec![Page::new("/p/", Some("Alt"))]);
    let provider = Scripted::new().script(
        "/p/",
        vec![
            cands(&[&text_of("Eins zu lang ", 70)]),
            cands(&[&text_of("Zwei zu lang ", 71)]),
            cands(&[&valid_title("drei")]),
        ],
    );
    let meter = TableMeter::default();
    rt().block_on(
        kit.engine(&provider, &meter)
            .run(&kit.job(Element::Title, options())),
    )
    .unwrap();
    assert_eq!(provider.count(), 3);
    let r = kit.record("/p/", Element::Title);
    assert_eq!(r.status, VALID);
    assert_eq!(first_shown(&r), valid_title("drei"));
}

// spec: AI-075 [M4]
// acceptance: independent
// „Mock liefert dauerhaft ungültige Kandidaten → nach 1 + 2 Requests `No valid suggestion`, sichtbarer Kandidat = der mit
// den wenigsten Verstößen (bei Gleichstand der erste)“; der sichtbare Kandidat behält seine Verstoßliste.
#[test]
fn never_valid_ends_after_three_requests_with_fewest_violations_visible() {
    // 61/62 Zeichen = 1 Verstoß (Länge; 549/558 px), 70–72 Zeichen = 2 Verstöße (Länge und Pixel).
    let a = text_of("A zwei Verstöße ", 70);
    let b = text_of("B ein Verstoß ", 61);
    let c = text_of("C ein Verstoß ", 62);
    let d = text_of("D zwei Verstöße ", 71);
    let e = text_of("E zwei Verstöße ", 72);
    let kit = Kit::new("never", vec![Page::new("/p/", Some("Alt"))]);
    let provider = Scripted::new().script(
        "/p/",
        vec![cands(&[&a, &b]), cands(&[&c, &d]), cands(&[&e])],
    );
    let meter = TableMeter::default();
    rt().block_on(
        kit.engine(&provider, &meter)
            .run(&kit.job(Element::Title, options())),
    )
    .unwrap();
    assert_eq!(provider.count(), 3, "1 + 2 Requests");
    let r = kit.record("/p/", Element::Title);
    assert_eq!(r.status, NO_VALID);
    assert_eq!(first_shown(&r), b, "Gleichstand B/C → der erste");
    let b_violations = &r
        .candidates
        .iter()
        .find(|x| x.value == b)
        .unwrap()
        .violations;
    assert_eq!(b_violations.len(), 1);
    assert_eq!(
        &r.violations, b_violations,
        "Verstoßliste des sichtbaren Kandidaten"
    );
}

// spec: AI-075 [M4]
// acceptance: independent
// Reparaturrunden einstellbar 0–3: 0 → genau 1 Request, 3 → 4 Requests; außerhalb des Bereichs abgelehnt.
#[test]
fn repair_round_setting_bounds_the_requests() {
    for (rounds, expected) in [(0u8, 1usize), (3, 4)] {
        let kit = Kit::new("rounds", vec![Page::new("/p/", Some("Alt"))]);
        let replies = (0..5)
            .map(|i| cands(&[&text_of(&format!("Runde {i} zu lang "), 70)]))
            .collect();
        let provider = Scripted::new().script("/p/", replies);
        let meter = TableMeter::default();
        let opts = SuggestOptions {
            repair_rounds: rounds,
            ..options()
        };
        rt().block_on(
            kit.engine(&provider, &meter)
                .run(&kit.job(Element::Title, opts)),
        )
        .unwrap();
        assert_eq!(provider.count(), expected, "{rounds} Reparaturrunden");
        assert_eq!(kit.record("/p/", Element::Title).status, NO_VALID);
    }
    for bad in [
        SuggestOptions {
            repair_rounds: 4,
            ..options()
        },
        SuggestOptions {
            candidates: 0,
            ..options()
        },
        SuggestOptions {
            candidates: 6,
            ..options()
        },
    ] {
        let kit = Kit::new("bad", vec![Page::new("/p/", Some("Alt"))]);
        let provider = Scripted::new();
        let meter = TableMeter::default();
        let result = rt().block_on(
            kit.engine(&provider, &meter)
                .run(&kit.job(Element::Title, bad)),
        );
        assert!(
            matches!(result, Err(RunError::Options(_))),
            "{bad:?}: {result:?}"
        );
        assert_eq!(provider.count(), 0);
    }
}

// spec: AI-015 [M4]
// spec: AI-075 [M4]
// spec: AI-014 [M4]
// acceptance: independent
// Liefert der Anbieter in jeder Runde dieselben ungültigen Kandidaten, ist der Request der dritten Runde identisch mit
// dem der zweiten (gleiche Eingabe, gleiche Rückmeldung) → Cache-Treffer ohne neuen Request: 2 Requests, 3 Runden,
// dritte Protokollzeile `Cache Hit = true` mit `Cost = 0`.
#[test]
fn identical_repair_request_is_a_cache_hit() {
    let a = text_of("Immer gleich zu lang ", 70);
    let kit = Kit::new("same", vec![Page::new("/p/", Some("Alt"))]);
    let provider = Scripted::new().script("/p/", vec![cands(&[&a])]);
    let meter = TableMeter::default();
    let summary = rt()
        .block_on(
            kit.engine(&provider, &meter)
                .run(&kit.job(Element::Title, options())),
        )
        .unwrap();
    assert_eq!(provider.count(), 2, "dritte Runde aus dem Cache");
    assert_eq!(summary.cache_hits, 1);
    let rows = kit.log_rows("/p/");
    assert_eq!(rows.len(), 3, "{rows:?}");
    assert!(!rows[0].0 && !rows[1].0);
    assert!(rows[2].0 && rows[2].1 == 0, "{rows:?}");
    let r = kit.record("/p/", Element::Title);
    assert_eq!(r.status, NO_VALID);
    assert_eq!(r.rounds, 3);
}

// spec: AI-015 [M4]
// spec: AI-070 [M4]
// acceptance: independent
// Zweiter Lauf mit gleicher Eingabe und gleichem Modell → 0 Requests, Ergebnis unverändert, Protokollzeilen mit
// `Cache Hit = true` und `Cost = 0`.
#[test]
fn second_run_is_answered_from_the_cache() {
    let kit = Kit::new(
        "cache",
        vec![
            Page::new("/a/", Some("Alt A")),
            Page::new("/b/", Some("Alt B")),
        ],
    );
    let provider = Scripted::new()
        .script("/a/", vec![cands(&[&valid_title("a")])])
        .script("/b/", vec![cands(&[&valid_title("b")])]);
    let meter = TableMeter::default();
    let runtime = rt();
    runtime
        .block_on(
            kit.engine(&provider, &meter)
                .run(&kit.job(Element::Title, options())),
        )
        .unwrap();
    assert_eq!(provider.count(), 2);
    let second = runtime
        .block_on(
            kit.engine(&provider, &meter)
                .run(&kit.job(Element::Title, options())),
        )
        .unwrap();
    assert_eq!(provider.count(), 2, "kein neuer Request");
    assert_eq!(second.requests, 0);
    assert_eq!(second.cache_hits, 2);
    for p in ["/a/", "/b/"] {
        let rows = kit.log_rows(p);
        assert_eq!(rows.len(), 2);
        assert!(rows[1].0 && rows[1].1 == 0, "{rows:?}");
        assert_eq!(kit.record(p, Element::Title).status, VALID);
    }
}

// spec: AI-038 [M4]
// spec: AI-020 [M4]
// acceptance: independent
// Refusal → Status `Refusal`, kein Reparaturversuch; Abbruch an der Ausgabegrenze → `Error: Output limit reached`, genau
// 1 Request (AI-021 Nachtrag 24.11.2026).
#[test]
fn refusal_and_output_limit_end_without_repair() {
    let kit = Kit::new(
        "refusal",
        vec![
            Page::new("/r/", Some("Alt")),
            Page::new("/o/", Some("Alt O")),
        ],
    );
    let provider = Scripted::new()
        .script("/r/", vec![Reply::Refusal])
        .script("/o/", vec![Reply::OutputLimit]);
    let meter = TableMeter::default();
    rt().block_on(
        kit.engine(&provider, &meter)
            .run(&kit.job(Element::Title, options())),
    )
    .unwrap();
    assert_eq!(provider.calls_for("/r/").len(), 1);
    assert_eq!(provider.calls_for("/o/").len(), 1);
    assert_eq!(kit.record("/r/", Element::Title).status, "Refusal");
    assert_eq!(
        kit.record("/o/", Element::Title).status,
        "Error: Output limit reached"
    );
}

// spec: AI-021 [M4]
// acceptance: independent
// Schemaverstoß einmalig → genau ein Reparaturversuch (2 Requests), danach gültig; dauerhaft → nach 2 Requests
// `Error: Invalid output`.
#[test]
fn schema_violation_gets_exactly_one_repair_attempt() {
    let kit = Kit::new(
        "schema",
        vec![
            Page::new("/once/", Some("Alt")),
            Page::new("/always/", Some("Alt 2")),
        ],
    );
    let provider = Scripted::new()
        .script(
            "/once/",
            vec![Reply::Invalid, cands(&[&valid_title("once")])],
        )
        .script("/always/", vec![Reply::Invalid]);
    let meter = TableMeter::default();
    rt().block_on(
        kit.engine(&provider, &meter)
            .run(&kit.job(Element::Title, options())),
    )
    .unwrap();
    assert_eq!(provider.calls_for("/once/").len(), 2);
    assert_eq!(kit.record("/once/", Element::Title).status, VALID);
    assert_eq!(provider.calls_for("/always/").len(), 2);
    assert_eq!(
        kit.record("/always/", Element::Title).status,
        "Error: Invalid output"
    );
}

// spec: AI-017 [M4]
// acceptance: independent
// Neutrale Fälle des Schutzschalters (Nachtrag 26.11.2026): Refusals und `Error: Output limit reached` zählen nicht als
// aufeinanderfolgende endgültige Fehler – 12 solche Einträge hintereinander werden alle gesendet.
#[test]
fn refusals_and_output_limits_do_not_open_the_breaker() {
    for (reply, status) in [
        (Reply::Refusal, "Refusal"),
        (Reply::OutputLimit, "Error: Output limit reached"),
    ] {
        let pages: Vec<Page> = (0..12)
            .map(|i| Page::new(&format!("/n{i:02}/"), Some(&format!("Alt {i}"))))
            .collect();
        let mut provider = Scripted::new();
        for p in &pages {
            provider = provider.script(&p.path, vec![reply.clone()]);
        }
        let kit = Kit::new("breaker", pages);
        let meter = TableMeter::default();
        rt().block_on(
            kit.engine(&provider, &meter)
                .run(&kit.job(Element::Title, options())),
        )
        .unwrap();
        assert_eq!(provider.count(), 12, "{status}: alle 12 gesendet");
        for p in &kit.pages {
            assert_eq!(kit.record(&p.path, Element::Title).status, status);
        }
    }
}

// spec: AI-070 [M4]
// acceptance: independent
// Schalter „KI (Vorschau)“ aus → kein Request, kein Eintrag, keine Protokollzeile.
#[test]
fn preview_switch_off_sends_nothing() {
    let mut kit = Kit::new("off", vec![Page::new("/p/", Some("Alt"))]);
    kit.switch = PreviewSwitch::OFF;
    let provider = Scripted::new();
    let meter = TableMeter::default();
    let result = rt().block_on(
        kit.engine(&provider, &meter)
            .run(&kit.job(Element::Title, options())),
    );
    assert!(matches!(result, Err(RunError::PreviewOff)), "{result:?}");
    assert_eq!(provider.count(), 0);
    assert_eq!(kit.count("SELECT count(*) FROM ai_suggestions"), 0);
    assert_eq!(kit.count("SELECT count(*) FROM ai_requests"), 0);
}

// spec: AI-018 [M4]
// acceptance: independent
// Ohne Einwilligung wird kein Request gesendet; die Einträge stehen auf `Pending (consent)`; keine Reservierung bleibt
// offen.
#[test]
fn without_consent_no_request_and_pending_consent() {
    let mut kit = Kit::new(
        "consent",
        vec![
            Page::new("/a/", Some("Alt")),
            Page::new("/b/", Some("Alt B")),
        ],
    );
    kit.consent = os_ai::policy::ConsentGate::cli(false);
    let provider = Scripted::new();
    let meter = TableMeter::default();
    rt().block_on(
        kit.engine(&provider, &meter)
            .run(&kit.job(Element::Title, options())),
    )
    .unwrap();
    assert_eq!(provider.count(), 0, "Mock-Zähler 0");
    for p in ["/a/", "/b/"] {
        assert_eq!(kit.record(p, Element::Title).status, "Pending (consent)");
    }
    let snap = kit.ledger.snapshot().unwrap();
    assert_eq!(snap.reserved.0, 0);
    assert_eq!(snap.spent.0, 0);
    assert_eq!(
        kit.count("SELECT count(*) FROM ai_requests WHERE cost_microusd > 0"),
        0
    );
}

// spec: AI-070 [M4]
// acceptance: independent
// „Ist ein Fokus-Begriff für eine URL importiert, enthält der Request ihn (Mock-Log); URLs ohne Eintrag werden ohne ihn
// angefragt.“ Dazu der Kontext: aktueller Wert, Title, H1, Meta Description, Seitentext-Auszug, Seitensprache.
#[test]
fn focus_keyword_and_context_reach_the_request() {
    let pages = vec![
        Page::new("/f/", Some("Bergschuhe Übersicht"))
            .meta("Alles über Bergschuhe für den Alpenraum")
            .lang("de-CH")
            .focus("Trekkingstiefel"),
        Page::new("/g/", Some("Wanderrucksack Übersicht")),
    ];
    let kit = Kit::new("focus", pages);
    let provider = Scripted::new()
        .script("/f/", vec![cands(&[&valid_title("f")])])
        .script("/g/", vec![cands(&[&valid_title("g")])]);
    let meter = TableMeter::default();
    rt().block_on(
        kit.engine(&provider, &meter)
            .run(&kit.job(Element::Title, options())),
    )
    .unwrap();
    let f = &provider.calls_for("/f/")[0];
    let g = &provider.calls_for("/g/")[0];
    let all_f = format!("{}\n{}", f.instructions, f.input);
    let all_g = format!("{}\n{}", g.instructions, g.input);
    assert!(all_f.contains("Trekkingstiefel"), "{all_f}");
    assert!(!all_g.contains("Trekkingstiefel"), "{all_g}");
    for part in [
        "Bergschuhe Übersicht",
        "Alles über Bergschuhe für den Alpenraum",
        "Überschrift /f/",
        "Seitentext der Seite /f/",
        "de-CH",
    ] {
        assert!(all_f.contains(part), "Kontext ohne {part:?}: {all_f}");
    }
}

// =================================================================================================================
// AI-071 Stufe 2 (Nachlauf) und Determinismus.

fn x() -> String {
    valid_title("X gleich")
}
fn y() -> String {
    valid_title("Y zweiter")
}

// spec: AI-071 [M4]
// acceptance: independent
// „Mock liefert für `/a/` und `/b/` denselben ersten Kandidaten „X“ (bei `/b/` als zweiten Kandidaten „Y“) → nach dem
// Nachlauf ist bei `/a/` „X“ Kandidat 1, bei `/b/` trägt „X“ den Verstoß `duplicate_candidate:http://ai.test:8080/a/` und
// „Y“ ist Kandidat 1“; der Nachlauf sendet keinen Request.
#[test]
fn post_pass_reserves_by_address_order() {
    let kit = Kit::new(
        "stage2",
        vec![
            Page::new("/b/", Some("Alt B")),
            Page::new("/a/", Some("Alt A")),
        ],
    );
    let provider = Scripted::new()
        .script("/a/", vec![cands(&[&x(), &valid_title("A zwei")])])
        .script("/b/", vec![cands(&[&x(), &y()])]);
    let meter = TableMeter::default();
    rt().block_on(
        kit.engine(&provider, &meter)
            .run(&kit.job(Element::Title, options())),
    )
    .unwrap();
    assert_eq!(provider.count(), 2, "Nachlauf ohne Request");
    let a = kit.record("/a/", Element::Title);
    let b = kit.record("/b/", Element::Title);
    assert_eq!(first_shown(&a), x());
    assert_eq!(first_shown(&b), y());
    let bx = b.candidates.iter().find(|c| c.value == x()).unwrap();
    assert!(!bx.valid);
    assert!(
        bx.violations
            .iter()
            .any(|v| v == "duplicate_candidate:http://ai.test:8080/a/"),
        "{:?}",
        bx.violations
    );
    assert_eq!(a.status, VALID);
    assert_eq!(b.status, VALID);
    assert!(!a.provisional && !b.provisional);
}

// spec: AI-071 [M4]
// acceptance: independent
// „Hat `/b/` nur den doppelten Kandidaten, endet `/b/` mit `No valid suggestion`; der Nachlauf sendet keinen Request
// (Mock-Zähler unverändert).“
#[test]
fn post_pass_without_alternative_gives_no_valid_suggestion_without_request() {
    let kit = Kit::new(
        "stage2b",
        vec![
            Page::new("/a/", Some("Alt A")),
            Page::new("/b/", Some("Alt B")),
        ],
    );
    let provider = Scripted::new()
        .script("/a/", vec![cands(&[&x()])])
        .script("/b/", vec![cands(&[&x()])]);
    let meter = TableMeter::default();
    rt().block_on(
        kit.engine(&provider, &meter)
            .run(&kit.job(Element::Title, options())),
    )
    .unwrap();
    assert_eq!(provider.count(), 2);
    assert_eq!(kit.record("/a/", Element::Title).status, VALID);
    assert_eq!(kit.record("/b/", Element::Title).status, NO_VALID);
}

/// Ergebnis eines Laufs als Vergleichsform: Adresse → (Status, Kandidat 1, Verstöße je Kandidat).
type Outcome = BTreeMap<String, (String, String, Vec<Vec<String>>)>;

fn outcome(kit: &Kit) -> Outcome {
    kit.pages
        .iter()
        .map(|p| {
            let r = kit.record(&p.path, Element::Title);
            (
                p.path.clone(),
                (
                    r.status.clone(),
                    first_shown(&r),
                    r.candidates.iter().map(|c| c.violations.clone()).collect(),
                ),
            )
        })
        .collect()
}

fn determinism_kit(delays: &[u64]) -> (Kit, Scripted) {
    let paths = ["/a/", "/b/", "/c/", "/d/", "/e/"];
    let pages: Vec<Page> = paths
        .iter()
        .rev()
        .map(|p| Page::new(p, Some(&format!("Alt {p}"))))
        .collect();
    let mut kit = Kit::new("det", pages);
    kit.workers = 4;
    let c2 = valid_title("C zwei");
    let mut provider = Scripted::new()
        .script("/a/", vec![cands(&[&x(), &valid_title("A zwei")])])
        .script("/b/", vec![cands(&[&x(), &y()])])
        .script("/c/", vec![cands(&[&y(), &c2])])
        .script("/d/", vec![cands(&[&valid_title("D eins")])])
        .script("/e/", vec![cands(&[&x()])]);
    for (p, d) in paths.iter().zip(delays) {
        provider = provider.delay(p, *d);
    }
    (kit, provider)
}

// spec: AI-071 [M4]
// acceptance: independent
// Determinismustest: 20 Wiederholungen mit 4 Workern und zufälliger (gesäter) Verzögerung je URL → identisches Ergebnis;
// es gilt auch, wenn `/b/` vor `/a/` fertig wird.
#[test]
fn post_pass_is_deterministic_over_20_runs_with_4_workers() {
    let runtime = rt();
    let mut seed: u64 = 0x5eed_0071;
    let mut next = || {
        seed = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (seed >> 33) % 40
    };
    let mut results = Vec::new();
    for _ in 0..20 {
        let delays: Vec<u64> = (0..5).map(|_| next()).collect();
        let (kit, provider) = determinism_kit(&delays);
        let meter = TableMeter::default();
        runtime
            .block_on(
                kit.engine(&provider, &meter)
                    .run(&kit.job(Element::Title, options())),
            )
            .unwrap();
        assert_eq!(provider.count(), 5);
        results.push(outcome(&kit));
    }
    // `/b/` deutlich vor `/a/`.
    let (kit, provider) = determinism_kit(&[300, 0, 0, 0, 0]);
    let meter = TableMeter::default();
    runtime
        .block_on(
            kit.engine(&provider, &meter)
                .run(&kit.job(Element::Title, options())),
        )
        .unwrap();
    results.push(outcome(&kit));
    let first = &results[0];
    for (i, r) in results.iter().enumerate() {
        assert_eq!(r, first, "Lauf {i} weicht ab");
    }
    assert_eq!(first["/a/"].1, x());
    assert_eq!(first["/b/"].1, y());
    assert_eq!(
        first["/c/"].1,
        valid_title("C zwei"),
        "Y ist für /b/ reserviert"
    );
    assert_eq!(first["/d/"].0, VALID);
    assert_eq!(first["/e/"].0, NO_VALID);
}

// spec: AI-071 [M4]
// acceptance: independent
// „Bis der Nachlauf abgeschlossen ist, sind Status und Kandidat 1 als vorläufig gekennzeichnet.“ Während des Laufs ist ein
// schon fertiger Eintrag vorläufig, nach dem Lauf nicht mehr.
#[test]
fn entries_are_provisional_until_the_post_pass() {
    let pages = vec![
        Page::new("/a/", Some("Alt A")),
        Page::new("/b/", Some("Alt B")),
    ];
    let kit = Kit::new("prov", pages);
    let provider = Scripted::new()
        .script("/a/", vec![cands(&[&valid_title("a")])])
        .script("/b/", vec![cands(&[&valid_title("b")])]);
    let meter = TableMeter::default();
    let tap = TapStore::new(&kit.store, None);
    let mut engine = kit.engine(&provider, &meter);
    engine.store = &tap;
    rt().block_on(engine.run(&kit.job(Element::Title, options())))
        .unwrap();
    let committed = tap.committed.lock().unwrap().clone();
    let with_result: Vec<_> = committed
        .iter()
        .filter(|(_, status, _)| status == VALID)
        .collect();
    assert_eq!(with_result.len(), 2, "{committed:?}");
    assert!(
        with_result.iter().all(|(_, _, provisional)| *provisional),
        "Ergebnis vor dem Nachlauf vorläufig: {committed:?}"
    );
    assert!(!kit.record("/a/", Element::Title).provisional);
    assert!(!kit.record("/b/", Element::Title).provisional);
}

// =================================================================================================================
// Stoppen, Budget, Atomarität (AI-013, AI-020 Nachtrag 26.11.2026).

fn five_pages() -> Vec<Page> {
    (1..=5)
        .map(|i| Page::new(&format!("/p{i}/"), Some(&format!("Alt {i}"))))
        .collect()
}

fn five_valid() -> Scripted {
    let mut provider = Scripted::new();
    for i in 1..=5 {
        provider = provider.script(
            &format!("/p{i}/"),
            vec![cands(&[&valid_title(&format!("p{i}"))])],
        );
    }
    provider
}

// spec: AI-020 [M4]
// acceptance: independent
// „Gestoppt (endgültig)“: Nach dem Stoppen geht kein weiterer Request hinaus; der laufende Request wird übernommen
// (Protokollzeile, Ergebnis); offene Einträge bleiben `Pending` ohne Protokollzeile.
#[test]
fn stop_sends_nothing_more_and_keeps_open_entries_pending() {
    let mut kit = Kit::new("stop", five_pages());
    kit.workers = 1;
    let control = kit.control.clone();
    let mut provider = five_valid();
    provider = provider.on_call(move |n, _| {
        if n == 2 {
            control.stop();
        }
    });
    let summary = rt()
        .block_on(
            kit.engine(&provider, &meter_default())
                .run(&kit.job(Element::Title, options())),
        )
        .unwrap();
    assert!(summary.stopped);
    assert_eq!(provider.count(), 2, "kein Request nach dem Stoppen");
    assert_eq!(kit.record("/p1/", Element::Title).status, VALID);
    assert_eq!(
        kit.record("/p2/", Element::Title).status,
        VALID,
        "laufender Request übernommen"
    );
    assert_eq!(kit.log_rows("/p2/").len(), 1);
    for p in ["/p3/", "/p4/", "/p5/"] {
        assert_eq!(kit.record(p, Element::Title).status, "Pending");
        assert!(kit.log_rows(p).is_empty());
    }
    let snap = kit.ledger.snapshot().unwrap();
    assert_eq!(
        snap.reserved.0, 0,
        "offene Einträge belegen keine Reservierung"
    );
}

fn meter_default() -> TableMeter {
    TableMeter::default()
}

// spec: AI-020 [M4]
// acceptance: independent
// Gestoppt vor dem ersten Request: 0 Requests, alle Einträge `Pending`.
#[test]
fn stopped_before_start_sends_nothing() {
    let kit = Kit::new("stop0", five_pages());
    kit.control.stop();
    let provider = five_valid();
    rt().block_on(
        kit.engine(&provider, &meter_default())
            .run(&kit.job(Element::Title, options())),
    )
    .unwrap();
    assert_eq!(provider.count(), 0);
    for i in 1..=5 {
        assert_eq!(
            kit.record(&format!("/p{i}/"), Element::Title).status,
            "Pending"
        );
    }
}

// spec: AI-013 [M4]
// acceptance: independent
// Budget 0 → kein Request; alle Einträge `Budget exceeded`.
#[test]
fn zero_budget_sends_nothing() {
    let mut kit = Kit::new("budget0", five_pages());
    kit.ledger = CrawlBudgetLedger::new(budget(0));
    let provider = five_valid();
    rt().block_on(
        kit.engine(&provider, &meter_default())
            .run(&kit.job(Element::Title, options())),
    )
    .unwrap();
    assert_eq!(provider.count(), 0);
    for i in 1..=5 {
        assert_eq!(
            kit.record(&format!("/p{i}/"), Element::Title).status,
            "Budget exceeded"
        );
    }
}

// spec: AI-013 [M4]
// acceptance: independent
// Budget knapp: Ein Request wird nur gesendet, wenn seine Reservierung in das Restbudget passt; ist das Crawl-Budget
// erreicht, erhalten alle offenen Einträge `Budget exceeded`. Reservierung und Ist-Kosten je Request liest der Test aus
// einem Vorlauf (Protokollspalten); das Budget deckt genau zwei Requests.
#[test]
fn tight_budget_sends_exactly_what_fits_and_marks_the_rest() {
    // Vorlauf: Reservierung und Kosten je Request.
    let probe = Kit::new("probe", five_pages());
    rt().block_on(
        probe
            .engine(&five_valid(), &meter_default())
            .run(&probe.job(Element::Title, options())),
    )
    .unwrap();
    let r: Vec<(i64, i64)> = (1..=5)
        .map(|i| {
            let row = probe.log_rows(&format!("/p{i}/"))[0].clone();
            (row.2, row.1)
        })
        .collect();
    assert!(
        r.iter().all(|(res, cost)| *res >= *cost && *cost > 0),
        "{r:?}"
    );
    let limit = (r[0].1 + r[1].1 + r[2].0 - 1) as u64;
    assert!(
        r[0].0 as u64 <= limit && (r[1].0 + r[0].1) as u64 <= limit,
        "{r:?}"
    );
    let mut kit = Kit::new("tight", five_pages());
    kit.ledger = CrawlBudgetLedger::new(budget(limit));
    kit.workers = 1;
    let provider = five_valid();
    rt().block_on(
        kit.engine(&provider, &meter_default())
            .run(&kit.job(Element::Title, options())),
    )
    .unwrap();
    assert_eq!(provider.count(), 2, "genau zwei Requests passen");
    let statuses: Vec<String> = (1..=5)
        .map(|i| kit.record(&format!("/p{i}/"), Element::Title).status)
        .collect();
    assert_eq!(
        statuses,
        [
            VALID,
            VALID,
            "Budget exceeded",
            "Budget exceeded",
            "Budget exceeded"
        ]
    );
    let spent = kit.count("SELECT COALESCE(sum(cost_microusd), 0) FROM ai_requests");
    assert!(spent as u64 <= limit);
    let snap = kit.ledger.snapshot().unwrap();
    assert_eq!(snap.reserved.0, 0);
    assert_eq!(
        snap.spent.0 as i64, spent,
        "Ledger = Summe der Protokollzeilen"
    );
}

/// Speicher, der jede Übernahme mitschreibt (URL, Status, vorläufig) und optional die Übernahme einer Antwort für eine
/// URL scheitern lässt (Fehlerinjektion vor dem Commit).
struct TapStore<'a> {
    inner: &'a os_ai::tasks::suggest::FileStore,
    fail_url: Option<i64>,
    committed: Mutex<Vec<(i64, String, bool)>>,
}

impl<'a> TapStore<'a> {
    fn new(inner: &'a os_ai::tasks::suggest::FileStore, fail_url: Option<i64>) -> Self {
        Self {
            inner,
            fail_url,
            committed: Mutex::new(Vec::new()),
        }
    }
}

impl SuggestStore for TapStore<'_> {
    fn cached(&self, key: String) -> sg::run::StoreFuture<'_, Option<sg::CachedAnswer>> {
        self.inner.cached(key)
    }
    fn commit(&self, commit: store::SuggestionCommit) -> sg::run::StoreFuture<'_, Vec<i64>> {
        if Some(commit.record.url_id) == self.fail_url && !commit.requests.is_empty() {
            return Box::pin(async { Err("Fehlerinjektion vor dem Commit".to_string()) });
        }
        self.committed.lock().unwrap().push((
            commit.record.url_id,
            commit.record.status.clone(),
            commit.record.provisional,
        ));
        self.inner.commit(commit)
    }
    fn load_all(&self, element: Element) -> sg::run::StoreFuture<'_, Vec<ElementRecord>> {
        self.inner.load_all(element)
    }
    fn save_all(&self, records: Vec<ElementRecord>) -> sg::run::StoreFuture<'_, ()> {
        self.inner.save_all(records)
    }
    fn mark_pending(
        &self,
        element: Element,
        url_ids: Vec<i64>,
        run_id: i64,
    ) -> sg::run::StoreFuture<'_, ()> {
        self.inner.mark_pending(element, url_ids, run_id)
    }
}

// spec: AI-020 [M4]
// spec: AI-014 [M4]
// spec: AI-015 [M4]
// acceptance: independent
// „Verarbeitung einer Antwort ist atomar“: Scheitert die Übernahme einer Antwort, gibt es für den Eintrag weder Ergebnis
// noch Cache-Eintrag noch Protokollzeile noch verbuchte Kosten; der Eintrag bleibt `Pending`, die Reservierung ist frei.
// Ohne Injektion sind alle vier vorhanden.
#[test]
fn failed_commit_leaves_no_partial_trace() {
    let mut kit = Kit::new("atomic", five_pages());
    kit.workers = 1;
    let provider = five_valid();
    let meter = meter_default();
    let failing = TapStore::new(&kit.store, Some(kit.id_of("/p3/")));
    let mut engine = kit.engine(&provider, &meter);
    engine.store = &failing;
    let _ = rt().block_on(engine.run(&kit.job(Element::Title, options())));
    assert!(kit.log_rows("/p3/").is_empty(), "keine Protokollzeile");
    assert_eq!(kit.record("/p3/", Element::Title).status, "Pending");
    assert!(kit.record("/p3/", Element::Title).candidates.is_empty());
    for p in ["/p1/", "/p2/", "/p4/", "/p5/"] {
        assert_eq!(kit.log_rows(p).len(), 1, "{p}");
        assert_eq!(kit.record(p, Element::Title).status, VALID, "{p}");
    }
    assert_eq!(
        kit.count("SELECT count(*) FROM ai_cache"),
        4,
        "kein Cache-Eintrag für /p3/"
    );
    let snap = kit.ledger.snapshot().unwrap();
    assert_eq!(snap.reserved.0, 0, "Reservierung freigegeben");
    let spent = kit.count("SELECT COALESCE(sum(cost_microusd), 0) FROM ai_requests");
    assert_eq!(
        snap.spent.0 as i64, spent,
        "keine verbuchten Kosten für /p3/"
    );
}

// =================================================================================================================
// AI-075 Neuprüfung und AI-076 Prüfen/Übernehmen.

// spec: AI-075 [M4]
// acceptance: independent
// „Grenzwertänderung in den Preferences (Title max. 50 Zeichen) → bestehende Kandidaten werden ohne Request neu geprüft;
// Status ändert sich entsprechend.“
#[test]
fn recheck_with_new_limits_changes_status_without_request() {
    let kit = Kit::new("recheck", vec![Page::new("/p/", Some("Alt"))]);
    let c55 = text_of("Fünfundfünfzig Zeichen ", 55);
    let provider = Scripted::new().script("/p/", vec![cands(&[&c55])]);
    let meter = meter_default();
    rt().block_on(
        kit.engine(&provider, &meter)
            .run(&kit.job(Element::Title, options())),
    )
    .unwrap();
    assert_eq!(kit.record("/p/", Element::Title).status, VALID);
    let limits = ElementLimits {
        max_chars: 50,
        ..TITLE_LIMITS
    };
    let c = checker(
        Element::Title,
        limits,
        RuleSet::default(),
        &meter,
        &kit.index,
    );
    let records = kit
        .store
        .with(|f| store::load_all(f.conn(), Element::Title.id()).unwrap());
    let languages: BTreeMap<i64, Option<String>> = BTreeMap::new();
    let out = sg::revalidate(records, &c, &languages, &kit.index);
    let r = out.iter().find(|r| r.url_id == kit.id_of("/p/")).unwrap();
    assert_eq!(r.status, NO_VALID);
    assert_eq!(provider.count(), 1, "kein Request");
}

fn review_kit() -> (Kit, Scripted) {
    let pages: Vec<Page> = (1..=10)
        .map(|i| Page::new(&format!("/r{i:02}/"), Some(&format!("Alt {i}"))))
        .collect();
    let mut provider = Scripted::new();
    for (i, p) in pages.iter().enumerate() {
        let reply = if i < 7 {
            vec![cands(&[&valid_title(&format!("r{i}"))])]
        } else {
            (0..3)
                .map(|k| cands(&[&text_of(&format!("Zu lang {i} {k} "), 70)]))
                .collect()
        };
        provider = provider.script(&p.path, reply);
    }
    let kit = Kit::new("review", pages);
    (kit, provider)
}

// spec: AI-076 [M4]
// acceptance: independent
// „Massenaktion „Kandidat 1 übernehmen“ auf 10 URLs übernimmt nur gültige Kandidaten; URLs mit `No valid suggestion`
// bleiben `Open` und werden gezählt gemeldet.“
#[test]
fn bulk_accept_first_takes_only_valid_and_counts_the_rest() {
    let (kit, provider) = review_kit();
    rt().block_on(
        kit.engine(&provider, &meter_default())
            .run(&kit.job(Element::Title, options())),
    )
    .unwrap();
    let mut records = kit
        .store
        .with(|f| store::load_all(f.conn(), Element::Title.id()).unwrap());
    assert_eq!(records.len(), 10);
    let result = sg::accept_first(&mut records, "2026-11-27T12:00:00Z");
    assert_eq!(result.accepted, 7);
    assert_eq!(result.skipped, 3);
    for r in &records {
        if r.status == VALID {
            assert_eq!(r.review, ReviewStatus::Accepted);
            let a = r.accepted.as_ref().unwrap();
            assert_eq!(a.origin, Origin::Ai);
            assert_eq!(a.value, first_shown(r));
        } else {
            assert_eq!(r.status, NO_VALID);
            assert_eq!(r.review, ReviewStatus::Open);
            assert!(r.accepted.is_none());
        }
    }
}

// spec: AI-076 [M4]
// acceptance: independent
// Zustandsübergänge offen → übernommen → zurückgesetzt → abgelehnt → bearbeitet → zurückgesetzt; Übernehmen ohne gültigen
// Kandidaten wird abgelehnt.
#[test]
fn review_state_transitions() {
    let (kit, provider) = review_kit();
    rt().block_on(
        kit.engine(&provider, &meter_default())
            .run(&kit.job(Element::Title, options())),
    )
    .unwrap();
    let meter = meter_default();
    let c = title_checker(&meter, &kit.index);
    let now = "2026-11-27T12:00:00Z";
    let mut r = kit.record("/r01/", Element::Title);
    assert_eq!(r.review, ReviewStatus::Open);
    sg::apply(
        &mut r,
        &ReviewAction::Accept { candidate: None },
        &c,
        None,
        now,
    )
    .unwrap();
    assert_eq!(r.review, ReviewStatus::Accepted);
    assert_eq!(r.accepted.as_ref().unwrap().value, first_shown(&r));
    sg::apply(&mut r, &ReviewAction::Reset, &c, None, now).unwrap();
    assert_eq!(r.review, ReviewStatus::Open);
    assert!(r.accepted.is_none());
    sg::apply(&mut r, &ReviewAction::Reject, &c, None, now).unwrap();
    assert_eq!(r.review, ReviewStatus::Rejected);
    let edited = valid_title("eigener Text");
    sg::apply(
        &mut r,
        &ReviewAction::Edit {
            value: edited.clone(),
        },
        &c,
        None,
        now,
    )
    .unwrap();
    assert_eq!(r.review, ReviewStatus::Edited);
    let a = r.accepted.as_ref().unwrap();
    assert_eq!(a.value, edited);
    assert_eq!(a.origin, Origin::Edited);
    assert!(a.violations.is_empty());
    sg::apply(&mut r, &ReviewAction::Reset, &c, None, now).unwrap();
    assert_eq!(r.review, ReviewStatus::Open);
    let mut none = kit.record("/r09/", Element::Title);
    assert_eq!(none.status, NO_VALID);
    assert!(sg::apply(
        &mut none,
        &ReviewAction::Accept { candidate: None },
        &c,
        None,
        now
    )
    .is_err());
    assert_eq!(none.review, ReviewStatus::Open);
}

// spec: AI-076 [M4]
// acceptance: independent
// „Eine bearbeitete Übernahme, die eine Regel verletzt, kann gespeichert werden, ist aber als `Edited` mit Verstoßliste
// gekennzeichnet (keine stille Korrektur).“ Live-Prüfung inkl. Pixelbreite.
#[test]
fn edited_value_with_violations_is_saved_unchanged_and_marked() {
    let (kit, provider) = review_kit();
    rt().block_on(
        kit.engine(&provider, &meter_default())
            .run(&kit.job(Element::Title, options())),
    )
    .unwrap();
    let meter = meter_default();
    let c = title_checker(&meter, &kit.index);
    let mut r = kit.record("/r09/", Element::Title);
    let raw = format!("\"{}\"", text_of("Viel zu langer eigener Title ", 68));
    let live = sg::check_text(&c, r.url_id, None, &raw);
    assert!(!live.valid());
    assert_eq!(live.pixel_width, 9 * 70, "Pixelbreite der Live-Prüfung");
    sg::apply(
        &mut r,
        &ReviewAction::Edit { value: raw.clone() },
        &c,
        None,
        "2026-11-27T12:00:00Z",
    )
    .unwrap();
    assert_eq!(r.review, ReviewStatus::Edited);
    let a = r.accepted.as_ref().unwrap();
    assert_eq!(a.value, raw, "keine stille Korrektur");
    assert!(!a.violations.is_empty());
    kit.store.with(|f| store::save(f.conn(), &r).unwrap());
    let again = kit.record("/r09/", Element::Title);
    assert_eq!(again.review, ReviewStatus::Edited);
    assert_eq!(again.accepted.as_ref().unwrap().value, raw);
    assert_eq!(again.accepted.as_ref().unwrap().violations, a.violations);
}
