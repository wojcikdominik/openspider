//! `--set pfad=wert` (OUT-186, Bibliotheksteil) und Voreinstellungsgruppen (CRAWL-140).
//!
//! Die CLI-Seite (Exit-Code 2, Ausgabe) testet M1-P26; hier geht es um Typ, Bereich und Vorschläge.

use os_model::*;
use serde_json::Value;

fn set(key: &str, value: &str) -> Result<CrawlConfig, SetError> {
    let mut c = CrawlConfig::default();
    c.set_value(key, value)?;
    Ok(c)
}

fn err(key: &str, value: &str) -> String {
    set(key, value).unwrap_err().to_string()
}

// spec: CRAWL-132
#[test]
fn short_keys_and_full_paths() {
    assert_eq!(set("threads", "10").unwrap().speed.threads, 10);
    assert_eq!(set("speed.threads", "10").unwrap().speed.threads, 10);
    assert_eq!(set("max_urls", "250").unwrap().limits.max_urls, 250);
    assert_eq!(set("max_depth", "3").unwrap().limits.max_depth, Some(3));
    assert_eq!(
        set("limits.max_depth", "0").unwrap().limits.max_depth,
        Some(0)
    );
    // "none" hebt die Begrenzung wieder auf
    let mut c = set("max_depth", "3").unwrap();
    c.apply_set("max_depth=none").unwrap();
    assert_eq!(c.limits.max_depth, None);
    // Leerzeichen um Schlüssel/Wert stören nicht
    assert_eq!(set(" threads ", " 12 ").unwrap().speed.threads, 12);
    // alle Kurzschlüssel zeigen auf existierende Schlüssel
    for (short, full) in SHORT_KEYS {
        assert!(settable_keys().iter().any(|k| k == full), "{short}");
    }
}

// spec: CRAWL-132
#[test]
fn set_overrides_a_loaded_profile() {
    // `--config a.osconfig --set threads=10`: a.osconfig setzt 5, wirksam ist 10
    let mut c = CrawlConfig::load_str("[speed]\nthreads = 5\nmax_per_host = 2\n")
        .unwrap()
        .config;
    assert_eq!(c.speed.threads, 5);
    c.apply_set("threads=10").unwrap();
    assert_eq!(c.speed.threads, 10);
    assert_eq!(c.speed.max_per_host, Some(2)); // übrige Werte bleiben
                                               // mehrere Zuweisungen nacheinander, die letzte gewinnt
    c.apply_set("threads=20").unwrap();
    c.apply_set("limits.max_urls=99").unwrap();
    assert_eq!((c.speed.threads, c.limits.max_urls), (20, 99));
    // Wert darf selbst ein "=" enthalten
    c.apply_set("user_agent.http=Bot/1.0 (a=b)").unwrap();
    assert_eq!(c.user_agent.http.as_deref(), Some("Bot/1.0 (a=b)"));
}

// spec: CRAWL-132
#[test]
fn invalid_integer_values_name_key_and_expected_range() {
    assert_eq!(
        err("threads", "abc"),
        "ungültiger Wert für threads: Ganzzahl 1–200 erwartet"
    );
    for bad in ["0", "201", "-3", "1.5", "", "zehn"] {
        assert_eq!(
            err("threads", bad),
            "ungültiger Wert für threads: Ganzzahl 1–200 erwartet",
            "{bad:?}"
        );
    }
    // der Schlüssel erscheint so, wie der Nutzer ihn eingegeben hat
    assert_eq!(
        err("speed.threads", "abc"),
        "ungültiger Wert für speed.threads: Ganzzahl 1–200 erwartet"
    );
    assert_eq!(
        err("max_urls", "-1"),
        "ungültiger Wert für max_urls: Ganzzahl ≥ 1 erwartet"
    );
    assert_eq!(
        err("max_depth", "x"),
        "ungültiger Wert für max_depth: Ganzzahl ≥ 0 oder none erwartet"
    );
    assert_eq!(
        err("duplicates.near_threshold_pct", "20"),
        "ungültiger Wert für duplicates.near_threshold_pct: Ganzzahl 50–100 erwartet"
    );
    // größer als der Rust-Typ (u32) → ebenfalls Fehler statt Überlauf
    assert!(matches!(
        set("links.high_crawl_depth", "99999999999"),
        Err(SetError::InvalidValue { .. })
    ));
    // ein Fehler lässt die Konfiguration unverändert
    let mut c = CrawlConfig::default();
    assert!(c.apply_set("threads=abc").is_err());
    assert_eq!(c, CrawlConfig::default());
}

// spec: CRAWL-132
#[test]
fn unknown_keys_get_the_closest_suggestion() {
    let e = set("threds", "4").unwrap_err();
    assert_eq!(
        e,
        SetError::UnknownKey {
            key: "threds".into(),
            suggestion: Some("threads".into())
        }
    );
    assert_eq!(
        e.to_string(),
        "unbekannter Schlüssel \"threds\" – meinten Sie \"threads\"?"
    );
    for (typo, wanted) in [
        ("speed.threds", "speed.threads"),
        ("limits.max_url", "limits.max_urls"),
        ("max_dept", "max_depth"),
        ("Speed.Threads", "speed.threads"),
        ("robots.retry-delay-s", "robots.retry_delay_s"),
        ("scope.excludes", "scope.exclude"),
    ] {
        match set(typo, "1") {
            Err(SetError::UnknownKey { suggestion, .. }) => {
                assert_eq!(suggestion.as_deref(), Some(wanted), "{typo}")
            }
            other => panic!("{typo}: {other:?}"),
        }
    }
    // zu weit von allen Schlüsseln entfernt: kein Vorschlag, aber eine klare Meldung
    let e = set("unbekannt", "1").unwrap_err();
    assert_eq!(
        e,
        SetError::UnknownKey {
            key: "unbekannt".into(),
            suggestion: None
        }
    );
    assert_eq!(e.to_string(), "unbekannter Schlüssel \"unbekannt\"");
    assert_eq!(suggest_key("unbekannt"), None);
    assert_eq!(suggest_key("threads").as_deref(), Some("threads"));
}

// spec: CRAWL-132
#[test]
fn bool_enum_and_float_values() {
    assert!(
        !set("spider.images.crawl", "false")
            .unwrap()
            .spider
            .images
            .crawl
    );
    assert!(
        set("scope.all_subdomains", "YES")
            .unwrap()
            .scope
            .all_subdomains
    );
    assert!(
        set("scope.all_subdomains", "ja")
            .unwrap()
            .scope
            .all_subdomains
    );
    assert_eq!(
        err("scope.all_subdomains", "vielleicht"),
        "ungültiger Wert für scope.all_subdomains: true oder false erwartet"
    );

    assert_eq!(set("mode", "list").unwrap().mode, CrawlMode::List);
    assert_eq!(set("mode", "LIST").unwrap().mode, CrawlMode::List);
    assert_eq!(
        set("robots.mode", "ignore-but-report").unwrap().robots.mode,
        RobotsMode::IgnoreButReport
    );
    assert_eq!(
        set("export.line_ending", "lf").unwrap().export.line_ending,
        LineEnding::Lf
    );
    assert_eq!(
        err("mode", "turbo"),
        "ungültiger Wert für mode: einer von spider, list, serp erwartet"
    );

    assert_eq!(
        set("speed.max_urls_per_sec", "0.5")
            .unwrap()
            .speed
            .max_urls_per_sec,
        Some(0.5)
    );
    assert_eq!(
        set("speed.max_urls_per_sec", "0,5")
            .unwrap()
            .speed
            .max_urls_per_sec,
        Some(0.5)
    );
    assert_eq!(
        set("speed.max_urls_per_sec", "2")
            .unwrap()
            .speed
            .max_urls_per_sec,
        Some(2.0)
    );
    let mut c = set("speed.max_urls_per_sec", "3").unwrap();
    c.set_value("speed.max_urls_per_sec", "none").unwrap();
    assert_eq!(c.speed.max_urls_per_sec, None);
    for bad in ["0", "-1", "abc", "nan", "inf"] {
        assert_eq!(
            err("speed.max_urls_per_sec", bad),
            "ungültiger Wert für speed.max_urls_per_sec: Dezimalzahl > 0 oder none erwartet",
            "{bad}"
        );
    }
}

// spec: CRAWL-132
#[test]
fn string_and_list_values() {
    assert_eq!(
        set("user_agent.preset", "custom")
            .unwrap()
            .user_agent
            .preset,
        "custom"
    );
    assert_eq!(
        set("user_agent.http", "Foo/1.0")
            .unwrap()
            .user_agent
            .http
            .as_deref(),
        Some("Foo/1.0")
    );
    let mut c = set("user_agent.http", "Foo/1.0").unwrap();
    c.set_value("user_agent.http", "none").unwrap();
    assert_eq!(c.user_agent.http, None);

    // TOML-Array-Schreibweise
    let c = set("scope.exclude", r#"[".*/a/.*", ".*/b/.*"]"#).unwrap();
    assert_eq!(c.scope.exclude, [".*/a/.*", ".*/b/.*"]);
    // ein einzelner Wert ist eine Liste mit einem Element – Kommas gehören zum Muster
    let c = set("scope.include", "a{1,2}").unwrap();
    assert_eq!(c.scope.include, ["a{1,2}"]);
    // leer = leere Liste
    let c = set("content.soft404_phrases", "").unwrap();
    assert!(c.content.soft404_phrases.is_empty());
    assert_eq!(
        set("scope.exclude", "[1, 2]").unwrap_err().to_string(),
        "ungültiger Wert für scope.exclude: Liste der Form [\"a\", \"b\"] oder ein einzelner Wert erwartet"
    );
}

// spec: CRAWL-132
#[test]
fn sections_and_structured_values_are_not_settable() {
    match set("spider.images", "off").unwrap_err() {
        SetError::Section { key, children } => {
            assert_eq!(key, "spider.images");
            assert_eq!(children, ["spider.images.crawl", "spider.images.store"]);
        }
        other => panic!("{other:?}"),
    }
    assert!(matches!(set("speed", "1"), Err(SetError::Section { .. })));
    assert_eq!(
        set("auth.basic", "x").unwrap_err(),
        SetError::NotSettable {
            key: "auth.basic".into()
        }
    );
    assert!(matches!(
        CrawlConfig::default().apply_set("threads"),
        Err(SetError::Format { .. })
    ));
}

// spec: CRAWL-132
#[test]
fn every_settable_key_exists_in_the_configuration() {
    let json = serde_json::to_value(CrawlConfig::default()).unwrap();
    let keys = settable_keys();
    assert!(keys.len() > 90, "{}", keys.len());
    for key in keys {
        assert!(
            json.pointer(&format!("/{}", key.replace('.', "/")))
                .is_some(),
            "{key}"
        );
        assert_eq!(suggest_key(key).as_deref(), Some(key.as_str()));
    }
    // jedes Blatt der Konfiguration (außer Listen von Tabellen) ist setzbar
    fn leaves(v: &Value, prefix: &str, out: &mut Vec<String>) {
        match v {
            Value::Object(m) => {
                for (k, v) in m {
                    let p = if prefix.is_empty() {
                        k.clone()
                    } else {
                        format!("{prefix}.{k}")
                    };
                    leaves(v, &p, out);
                }
            }
            _ => out.push(prefix.to_string()),
        }
    }
    let mut all = Vec::new();
    leaves(&json, "", &mut all);
    for key in ["limits.max_depth", "speed.max_per_host", "user_agent.http"] {
        assert!(
            all.contains(&key.to_string()),
            "Null-Werte gehören dazu: {key}"
        );
    }
    // Listen von Tabellen bzw. Zahlen und Zuordnungen lassen sich nur in der Datei bearbeiten (NotSettable)
    const STRUCTURED: [&str; 14] = [
        "auth.basic",
        "auth.form",
        "custom_search.filters",
        "custom_extraction.extractors",
        "custom_js.snippets",
        "link_positions.rules",
        "rewrite.regex_replace",
        "http.headers",
        "segments.list",
        "compare.mapping",
        // M4-F01: Fragen-/Promptdokumente (AI-030) und Embedding-Regeln (INTEG-180) nur in der Datei
        "ai.questions",
        "ai.prompts",
        "embeddings.rules",
        "embeddings.pair_rules",
    ];
    for key in all {
        if STRUCTURED.contains(&key.as_str())
            || key.starts_with("spelling.default_variants.")
            || key == "sitemap_gen.priority.by_depth"
        {
            continue;
        }
        assert!(keys.contains(&key), "{key} fehlt in settable_keys()");
    }
}

// spec: CRAWL-140
#[test]
fn preset_group_keys_are_settable_and_stored_in_the_profile() {
    let toml_text = CrawlConfig::default().to_osconfig_string().unwrap();
    let doc: toml::Table = toml_text.parse().unwrap();
    for group in PresetGroup::ALL {
        assert_eq!(PresetGroup::from_id(group.id()), Some(group));
        assert!(!group.keys().is_empty());
        for key in group.keys() {
            assert!(settable_keys().iter().any(|k| k == key), "{key}");
            // jeder Wert steht unter seinem Schlüssel im Profil
            let mut node = Some(&toml::Value::Table(doc.clone()));
            for seg in key.split('.') {
                node = node.and_then(|n| n.get(seg));
            }
            assert!(node.is_some(), "{key} fehlt im Profil");
        }
    }
    let covered: Vec<_> = PresetGroup::ALL.iter().flat_map(|g| g.keys()).collect();
    for key in [
        "title.max_chars",
        "meta_description.max_px",
        "url.max_chars",
        "h1.max_chars",
        "h2.max_chars",
        "image.max_kb",
        "image.alt_max_chars",
        "content.low_word_count",
        "content.soft404_phrases",
        "links.high_internal_outlinks",
        "links.high_external_outlinks",
        "links.high_crawl_depth",
        "validation.html_doc_max_bytes",
        "validation.resource_max_bytes",
    ] {
        assert!(covered.contains(&&key), "{key}");
    }
    assert_eq!(PresetGroup::from_id("nope"), None);
}

/// Verändert jeden Wert der Gruppe (Zahl + 1, Liste + "zz").
fn perturb(cfg: &mut CrawlConfig, group: PresetGroup) {
    let mut json = serde_json::to_value(&*cfg).unwrap();
    for key in group.keys() {
        let slot = json
            .pointer_mut(&format!("/{}", key.replace('.', "/")))
            .unwrap();
        match slot {
            Value::Number(n) => *slot = Value::from(n.as_u64().unwrap() + 1),
            Value::Array(a) => a.push(Value::from("zz")),
            other => panic!("{key}: {other}"),
        }
    }
    *cfg = serde_json::from_value(json).unwrap();
}

// spec: CRAWL-140
#[test]
fn resetting_a_preset_group_leaves_everything_else_alone() {
    for group in PresetGroup::ALL {
        let mut c = CrawlConfig::default();
        c.speed.threads = 9; // gehört zu keiner Gruppe
        c.scope.exclude.push(".*/privat/.*".into());
        perturb(&mut c, group);
        let perturbed = c.clone();

        group.reset(&mut c);

        let mut expected = CrawlConfig::default();
        expected.speed.threads = 9;
        expected.scope.exclude.push(".*/privat/.*".into());
        assert_eq!(c, expected, "{group:?}: nur die Gruppe wird zurückgesetzt");
        assert_ne!(perturbed, expected, "{group:?}: die Änderung war wirksam");
    }
}
