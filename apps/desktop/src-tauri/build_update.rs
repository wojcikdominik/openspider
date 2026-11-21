//! Updater build overrides, shared with the Tauri-free regression tests.

use serde_json::{json, Value};

const DEFAULT_REPO: &str = "wojcikdominik/openspider";

/// Former build-time channel switch (builds up to 1.0.0). Since 1.0.1 the channel is the app setting
/// `updates.channel` (OUT-254, K3-16); the variable no longer exists for the build and is ignored.
pub const RETIRED_BETA_VARIABLE: &str = "OPENSPIDER_UPDATE_BETA";

/// Both fixed channel manifests for `repo`, stable first (OUT-254). The build never picks one of them.
pub fn channel_endpoints(repo: &str) -> [String; 2] {
    [
        format!("https://github.com/{repo}/releases/latest/download/latest.json"),
        format!("https://github.com/{repo}/releases/download/update-beta/latest-beta.json"),
    ]
}

/// Apply only explicitly requested overrides. Return whether the effective key is empty.
pub fn configure(
    config: &mut Value,
    base: &Value,
    key: Option<&str>,
    repo: Option<&str>,
) -> Result<bool, &'static str> {
    if let Some(key) = key {
        config["plugins"]["updater"]["pubkey"] = json!(key);
    }
    // A key-only build needs the default endpoints unless the caller supplied some.
    // A repo variable alone must not alter incoming updater configuration.
    if key.is_some() && config["plugins"]["updater"].get("endpoints").is_none() {
        config["plugins"]["updater"]["endpoints"] =
            json!(channel_endpoints(repo.unwrap_or(DEFAULT_REPO)));
    }
    let effective_key = config["plugins"]["updater"]
        .get("pubkey")
        .unwrap_or(&base["plugins"]["updater"]["pubkey"]);
    Ok(effective_key
        .as_str()
        .is_none_or(|key| key.trim().is_empty()))
}

pub fn warn_missing_key(profile: Option<&str>, missing_key: bool) -> bool {
    profile == Some("release") && missing_key
}

/// Warn when the retired build channel variable is still set; it has no effect (OUT-254).
pub fn retired_beta_warning(value: Option<&str>) -> Option<String> {
    value.map(|_| {
        format!(
            "{RETIRED_BETA_VARIABLE} is ignored since 1.0.1; the update channel is the app setting updates.channel (OUT-254)"
        )
    })
}
