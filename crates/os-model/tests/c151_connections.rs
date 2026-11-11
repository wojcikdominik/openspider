//! CRAWL-151: Konfigurationsschlüssel `speed.connections_per_host` ∈ {`auto`, 1–6}, Standard `auto`.

use os_model::{ConnectionsPerHost, CrawlConfig};

// spec: CRAWL-151
#[test]
fn default_is_auto_and_resolves_to_threads_capped_at_six() {
    let cfg = CrawlConfig::default();
    assert_eq!(cfg.speed.connections_per_host, ConnectionsPerHost::Auto);
    assert_eq!(ConnectionsPerHost::Auto.resolve(5), 5);
    assert_eq!(ConnectionsPerHost::Auto.resolve(1), 1);
    assert_eq!(ConnectionsPerHost::Auto.resolve(20), 6);
    assert_eq!(ConnectionsPerHost::Fixed(2).resolve(10), 2);
    assert_eq!(ConnectionsPerHost::Fixed(1).resolve(5), 1);
    assert_eq!(ConnectionsPerHost::Auto.to_string(), "auto");
    assert_eq!(ConnectionsPerHost::Fixed(4).to_string(), "4");
}

// spec: CRAWL-151
#[test]
fn toml_and_json_carry_auto_as_text_and_numbers_as_integers() {
    let cfg = CrawlConfig::load_str("[speed]\nconnections_per_host = 3\n")
        .unwrap()
        .config;
    assert_eq!(cfg.speed.connections_per_host, ConnectionsPerHost::Fixed(3));
    let cfg = CrawlConfig::load_str("[speed]\nconnections_per_host = \"auto\"\n").unwrap();
    assert!(cfg.warnings.is_empty(), "{:?}", cfg.warnings);
    assert_eq!(
        cfg.config.speed.connections_per_host,
        ConnectionsPerHost::Auto
    );

    let mut cfg = CrawlConfig::default();
    cfg.speed.connections_per_host = ConnectionsPerHost::Fixed(2);
    let text = cfg.to_osconfig_string().unwrap();
    assert!(text.contains("connections_per_host = 2"), "{text}");
    assert_eq!(CrawlConfig::load_str(&text).unwrap().config, cfg);
    let json = serde_json::to_value(CrawlConfig::default()).unwrap();
    assert_eq!(json["speed"]["connections_per_host"], "auto");
    let back: CrawlConfig = serde_json::from_value(json).unwrap();
    assert_eq!(back.speed.connections_per_host, ConnectionsPerHost::Auto);
}

// spec: CRAWL-151
#[test]
fn values_outside_one_to_six_are_rejected_with_the_field_path() {
    for bad in [0, 7] {
        let mut cfg = CrawlConfig::default();
        cfg.speed.connections_per_host = ConnectionsPerHost::Fixed(bad);
        let errors = cfg.validate().errors;
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert_eq!(errors[0].path, "speed.connections_per_host");
    }
    for ok in 1..=6 {
        let mut cfg = CrawlConfig::default();
        cfg.speed.connections_per_host = ConnectionsPerHost::Fixed(ok);
        assert!(cfg.validate().is_ok());
    }
    assert!(CrawlConfig::load_str("[speed]\nconnections_per_host = \"viele\"\n").is_err());
}

// spec: CRAWL-151
#[test]
fn set_accepts_auto_and_one_to_six() {
    let mut cfg = CrawlConfig::default();
    cfg.set_value("speed.connections_per_host", "4").unwrap();
    assert_eq!(cfg.speed.connections_per_host, ConnectionsPerHost::Fixed(4));
    cfg.set_value("speed.connections_per_host", "AUTO").unwrap();
    assert_eq!(cfg.speed.connections_per_host, ConnectionsPerHost::Auto);
    for bad in ["0", "7", "viele"] {
        let err = cfg
            .set_value("speed.connections_per_host", bad)
            .unwrap_err()
            .to_string();
        assert!(err.contains("auto oder Ganzzahl 1–6"), "{bad}: {err}");
    }
    assert!(os_model::settable_keys()
        .iter()
        .any(|k| k == "speed.connections_per_host"));
}

// spec: CRAWL-151
#[test]
fn schema_offers_auto_or_one_to_six_with_default_auto() {
    let schema = os_model::json_schema();
    let field = &schema["$defs"]["SpeedConfig"]["properties"]["connections_per_host"];
    assert_eq!(field["default"], "auto");
    let def = &schema["$defs"]["ConnectionsPerHost"]["anyOf"];
    assert_eq!(def[0]["enum"], serde_json::json!(["auto"]));
    assert_eq!(
        (def[1]["minimum"].as_i64(), def[1]["maximum"].as_i64()),
        (Some(1), Some(6))
    );
}
