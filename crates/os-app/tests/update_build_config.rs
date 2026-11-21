// Test the actual build helper without linking Tauri or its Linux plugins.
#[path = "../../../apps/desktop/src-tauri/build_update.rs"]
mod build_update;

use build_update::{
    channel_endpoints, configure, retired_beta_warning, warn_missing_key, RETIRED_BETA_VARIABLE,
};
use serde_json::{json, Value};

fn incoming() -> Value {
    serde_json::from_str(include_str!(
        "../../../tests/fixtures/update/build-config.json"
    ))
    .unwrap()
}

fn base() -> Value {
    serde_json::from_str(include_str!(
        "../../../apps/desktop/src-tauri/tauri.conf.json"
    ))
    .unwrap()
}

// spec: OUT-250 [M3]
#[test]
fn no_build_variables_preserve_the_entire_incoming_config() {
    for mut config in [incoming(), json!({})] {
        let original = config.clone();
        configure(&mut config, &base(), None, None).unwrap();
        assert_eq!(config, original);
        configure(&mut config, &base(), None, Some("example/alternate")).unwrap();
        assert_eq!(config, original, "repo alone must not enable overrides");
    }
}

// spec: OUT-250 [M3]
#[test]
fn key_override_preserves_custom_endpoints_and_other_plugin_options() {
    let mut config = incoming();
    let mut expected = config.clone();
    expected["plugins"]["updater"]["pubkey"] = json!("");
    assert!(configure(&mut config, &base(), Some(""), None).unwrap());
    assert_eq!(config, expected);
}

// spec: OUT-254
#[test]
fn channel_endpoints_are_exactly_the_two_fixed_manifest_urls() {
    assert_eq!(
        channel_endpoints("wojcikdominik/openspider"),
        [
            "https://github.com/wojcikdominik/openspider/releases/latest/download/latest.json",
            "https://github.com/wojcikdominik/openspider/releases/download/update-beta/latest-beta.json",
        ]
    );
}

// spec: OUT-254
#[test]
fn key_override_with_repo_supplies_both_channel_endpoints_without_a_build_channel() {
    let mut config = json!({});
    assert!(!configure(&mut config, &base(), Some("key"), Some("example/alternate")).unwrap());
    assert_eq!(
        config["plugins"]["updater"]["endpoints"],
        json!([
            "https://github.com/example/alternate/releases/latest/download/latest.json",
            "https://github.com/example/alternate/releases/download/update-beta/latest-beta.json",
        ])
    );
}

// spec: OUT-250 [M3]
// spec: OUT-254
#[test]
fn key_only_builds_supply_the_default_endpoint_when_no_override_exists() {
    let fixture = incoming();
    let mut config = json!({});
    assert!(!configure(
        &mut config,
        &base(),
        fixture["plugins"]["updater"]["pubkey"].as_str(),
        None
    )
    .unwrap());
    // spec: OUT-254: both channels, stable first; the build no longer picks one.
    assert_eq!(
        config["plugins"]["updater"]["endpoints"],
        json!(channel_endpoints("wojcikdominik/openspider"))
    );
    assert!(!base().to_string().contains("github.com"));
}

// spec: OUT-250 [M3]
#[test]
fn release_warning_uses_the_effective_key_including_base_config() {
    for (mut config, base, missing) in [
        (json!({}), base(), true),
        (incoming(), base(), false),
        (json!({}), incoming(), false),
        (
            json!({"plugins":{"updater":{"pubkey":""}}}),
            incoming(),
            true,
        ),
    ] {
        let missing_key = configure(&mut config, &base, None, None).unwrap();
        assert_eq!(missing_key, missing);
        assert_eq!(warn_missing_key(Some("release"), missing_key), missing);
        assert!(!warn_missing_key(Some("debug"), missing_key));
        assert!(!warn_missing_key(None, missing_key));
    }
}

// spec: OUT-254
#[test]
fn retired_beta_variable_never_changes_the_configuration_and_only_warns() {
    assert_eq!(RETIRED_BETA_VARIABLE, "OPENSPIDER_UPDATE_BETA");
    assert_eq!(retired_beta_warning(None), None);
    for value in ["0", "1", "true"] {
        let warning = retired_beta_warning(Some(value)).unwrap();
        assert!(warning.contains(RETIRED_BETA_VARIABLE) && warning.contains("updates.channel"));
    }
    // configure() has no channel input at all: incoming endpoints survive a key override unchanged.
    let mut config = incoming();
    let original = config.clone();
    configure(&mut config, &base(), Some("key"), None).unwrap();
    assert_eq!(
        config["plugins"]["updater"]["endpoints"],
        original["plugins"]["updater"]["endpoints"]
    );
}
