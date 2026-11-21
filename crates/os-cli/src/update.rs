//! Explicit-only update entry point. No call exists in crawl or scheduled-run paths.
use crate::exit::CliError;
#[path = "../../os-app/src/update/manifest.rs"]
pub mod manifest;
#[path = "../../os-app/src/update/policy.rs"]
pub mod policy;
#[path = "../../os-app/src/update/verify.rs"]
pub mod verify;

pub async fn check_with_context(ctx: &policy::UpdateContext) -> Result<i32, CliError> {
    match policy::check(ctx, true).await.map_err(CliError::internal)? {
        policy::Outcome::Current => {
            println!("OpenSpider ist aktuell ({})", ctx.current);
            Ok(0)
        }
        policy::Outcome::Available(m) => {
            println!("OpenSpider {} verfügbar", m.version);
            Ok(10)
        }
        policy::Outcome::Disabled(reason) => Err(CliError::usage(format!(
            "Update-Prüfung deaktiviert: {reason:?}"
        ))),
        policy::Outcome::Throttled => Err(CliError::internal("Manuelle Update-Prüfung gedrosselt")),
    }
}
/// OUT-254: context for `--check-update` from `<settings_dir>/settings.toml`. Reads `updates.channel` and applies the
/// same standard rule as the desktop app; the CLI never writes `settings.toml`.
pub fn context_from_settings(
    settings_dir: &std::path::Path,
) -> Result<policy::UpdateContext, CliError> {
    let settings = std::fs::read_to_string(os_model::settings::AppSettings::path_in(settings_dir))
        .ok()
        .map(|s| os_model::settings::AppSettings::from_toml_str(&s))
        .transpose()
        .map_err(|_| CliError::usage("Ungültige App-Einstellungen"))?
        .unwrap_or_default();
    let password = if settings.network.proxy.username.is_some() {
        os_fetch::secret_store_from_env().get(os_fetch::net::PROXY_PASSWORD)
    } else {
        None
    };
    Ok(policy::UpdateContext {
        settings: settings.updates,
        network: settings.network,
        proxy_password: password,
        // Das CLI meldet nur, ob eine neuere Version existiert, und installiert nie (OUT-250).
        installer: policy::Installer::AnyReportOnly,
        ..Default::default()
    })
}
pub fn check() -> Result<i32, CliError> {
    let ctx = context_from_settings(&os_store::paths::app_paths().settings)?;
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|_| CliError::internal("Update-Laufzeit nicht verfügbar"))?
        .block_on(check_with_context(&ctx))
}
