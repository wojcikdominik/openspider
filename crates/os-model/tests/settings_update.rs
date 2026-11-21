use os_model::{settings::AppSettings, CrawlConfig};

// spec: OUT-250 [M3]
// spec: OUT-222 [M3]
// spec: OUT-224 [M3]
#[test]
fn updates_defaults_roundtrip_and_profile_separation() {
    let old = AppSettings::from_toml_str("").unwrap();
    assert!(old.updates.check);
    assert_eq!(old.updates.last_check, None);
    let loaded =
        AppSettings::from_toml_str(include_str!("../../../tests/fixtures/update/settings.toml"))
            .unwrap();
    assert!(!loaded.updates.check);
    assert_eq!(
        loaded.updates.last_check.as_deref(),
        Some("2026-10-30T12:00:00Z")
    );
    let text = loaded.to_toml_string().unwrap();
    assert_eq!(AppSettings::from_toml_str(&text).unwrap(), loaded);
    assert_eq!(
        AppSettings::from_toml_str(&text)
            .unwrap()
            .to_toml_string()
            .unwrap(),
        text
    );
    let profile = CrawlConfig::default().to_osconfig_string().unwrap();
    assert!(!profile.contains("[updates]"));
    assert!(!serde_json::to_value(CrawlConfig::default())
        .unwrap()
        .as_object()
        .unwrap()
        .contains_key("updates"));
    assert!(AppSettings::restart_required(["updates.check", "updates.last_check"]).is_empty());
}

// spec: OUT-222 [M3]
#[test]
fn settings_schema_golden() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/spec/schemas/settings.schema.json");
    let schema = schemars::schema_for!(AppSettings);
    let expected = format!("{}\n", serde_json::to_string_pretty(&schema).unwrap());
    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        std::fs::write(&path, &expected).unwrap();
    }
    assert_eq!(std::fs::read_to_string(path).unwrap(), expected);
}

// spec: OUT-222
// spec: OUT-254
#[test]
fn update_channel_has_no_written_default_and_keeps_any_stored_value() {
    // Stand 1.0.0: no key, read without migration; writing does not add a default value.
    let old = AppSettings::from_toml_str("[updates]\ncheck = true\n").unwrap();
    assert_eq!(old.updates.channel, None);
    let written = old.to_toml_string().unwrap();
    assert!(!written.contains("channel"), "{written}");
    // Valid and invalid values round-trip unchanged; the model never rejects or rewrites them.
    for value in ["stable", "beta", "nightly"] {
        let text = format!("[updates]\ncheck = true\nchannel = \"{value}\"\n");
        let loaded = AppSettings::from_toml_str(&text).unwrap();
        assert_eq!(loaded.updates.channel.as_deref(), Some(value));
        assert!(loaded.validate().is_empty());
        let again = loaded.to_toml_string().unwrap();
        assert!(again.contains(&format!("channel = \"{value}\"")), "{again}");
        assert_eq!(AppSettings::from_toml_str(&again).unwrap(), loaded);
    }
    assert_eq!(os_model::settings::UPDATE_CHANNELS, ["stable", "beta"]);
    // App setting only: no crawl profile carries it, and changing it needs no restart.
    let profile = CrawlConfig::default().to_osconfig_string().unwrap();
    assert!(!profile.contains("channel ="));
    assert!(AppSettings::restart_required(["updates.channel"]).is_empty());
    let mut changed = AppSettings::default();
    changed.updates.channel = Some("beta".into());
    assert_eq!(
        AppSettings::default().changed_keys(&changed),
        vec!["updates.channel".to_string()]
    );
}
