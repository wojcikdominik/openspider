//! Native updater transport. The service owns all timing and crawl safety decisions.
use os_app::{
    update::{
        download::{Progress, UpdateBackend},
        UpdateContext, UpdateManifest,
    },
    App,
};
use std::sync::Arc;
use tauri::{
    plugin::{Builder, TauriPlugin},
    Manager, Runtime,
};
use tauri_plugin_updater::UpdaterExt;
struct Native<R: Runtime> {
    handle: tauri::AppHandle<R>,
    app: os_app::runtime::WeakApp,
}
impl<R: Runtime> Native<R> {
    fn restore(
        &self,
        manifest: &UpdateManifest,
        restart: bool,
    ) -> Result<tauri_plugin_updater::Update, String> {
        let context = UpdateContext::default();
        manifest.validate(context.beta)?;
        self.handle
            .updater_builder()
            .endpoints(vec![context
                .endpoint
                .parse()
                .map_err(|_| "Update-Endpunkt ungültig")?])
            .map_err(|_| "Update-Endpunkte fehlen oder sind ungültig")?
            .pubkey(context.pubkey)
            .restart_after_install(restart)
            .build()
            .and_then(|updater| {
                updater
                    .restore_update(serde_json::to_value(manifest).expect("manifest serializable"))
            })
            .map_err(|_| "Update-Metadaten ungültig".to_owned())?
            .ok_or_else(|| "Keine neuere Version".into())
    }
}
impl<R: Runtime> UpdateBackend for Native<R> {
    fn download(
        &self,
        manifest: UpdateManifest,
        progress: Progress,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Vec<u8>, String>> + Send>> {
        let handle = self.handle.clone();
        let app = self.app.clone();
        Box::pin(async move {
            let app = app.upgrade().ok_or("Anwendung beendet")?;
            let context = UpdateContext::default();
            manifest.validate(context.beta)?;
            let builder =
                std::sync::Mutex::new(Some(os_app::update::download::client_builder(&app).await?));
            let update = handle
                .updater_builder()
                .endpoints(vec![context
                    .endpoint
                    .parse()
                    .map_err(|_| "Update-Endpunkt ungültig")?])
                .map_err(|_| "Update-Endpunkte fehlen oder sind ungültig")?
                .pubkey(context.pubkey)
                .configure_client(move |fallback| {
                    builder
                        .lock()
                        .unwrap_or_else(|p| p.into_inner())
                        .take()
                        .unwrap_or(fallback)
                })
                .build()
                .and_then(|u| {
                    u.restore_update(
                        serde_json::to_value(&manifest).expect("manifest serializable"),
                    )
                })
                .map_err(|_| "Update-Metadaten ungültig")?
                .ok_or("Keine neuere Version")?;
            let mut done = 0;
            let bytes = update
                .download(
                    move |chunk, total| {
                        done += chunk as u64;
                        progress(done, total);
                    },
                    || {},
                )
                .await
                .map_err(|_| "Update-Download fehlgeschlagen")?;
            update
                .verify(&bytes)
                .map_err(|_| "Update-Signatur ungültig")?;
            Ok(bytes)
        })
    }

    fn install(
        &self,
        manifest: UpdateManifest,
        bytes: Vec<u8>,
        restart: bool,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), String>> + Send>> {
        let update = self.restore(&manifest, restart);
        let handle = self.handle.clone();
        Box::pin(async move {
            let update = update?;
            tauri::async_runtime::spawn_blocking(move || {
                update
                    .verify(&bytes)
                    .map_err(|_| "Update-Signatur ungültig")?;
                update
                    .install(&bytes)
                    .map_err(|_| "Update-Installation fehlgeschlagen")
            })
            .await
            .map_err(|_| "Update-Installation fehlgeschlagen")??;
            if restart {
                // M3-C01/Fable N5: Die Sitzungsmarke (OUT-246) entfernt nur `RunEvent::Exit` (lib.rs). Ob
                // `restart()` dieses Ereignis vor dem Neustart auslöst, ist nicht geprüft; sonst meldet der erste
                // Start nach einem Update fälschlich `unclean_exit`. Manuelle Prüfung im Protokoll M3-Q03.
                handle.restart();
            }
            Ok(())
        })
    }
}
/// B4-MSIUPD: Installationsart für die Update-Prüfung aus derselben eingebetteten Bundle-Kennung, mit der
/// tauri-plugin-updater beim Herunterladen das Paket wählt (MSI → `-msi`, NSIS → `-nsis`, sonst unbekannt).
///
/// B4-RELCHECK: Aufruf vor `App::new`. `App::new` startet bereits die erste Update-Prüfung
/// (`check_updates_on_startup`) als Aufgabe, und deren `UpdateContext::default()` liest die Kennung; eine spätere
/// Meldung könnte diese Prüfung verpassen und sie fälschlich als „unbekannte Installationsart“ abschalten.
pub fn register_installer() {
    let bundle_type = tauri::utils::platform::bundle_type().map(|b| b.to_string());
    os_app::update::policy::register_bundle_type(bundle_type.as_deref());
}
pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("openspider-update")
        .setup(|handle, _| {
            // B4-RELCHECK: Diagnosezeile (WARN) für den Support, wenn die Kennung fehlt; das Log ist eingerichtet.
            os_app::update::policy::log_unknown_installer();
            let app = handle.state::<App>().inner().clone();
            os_app::update::download::set_backend(
                &app,
                Arc::new(Native {
                    handle: handle.clone(),
                    app: app.downgrade(),
                }),
            );
            tauri::async_runtime::spawn(async move {
                os_app::update::pending::reopen(&app).await;
            });
            Ok(())
        })
        .build()
}

#[cfg(test)]
mod tests {
    use os_app::update::{policy::Installer, UpdateContext, UpdateManifest};
    use tauri::utils::config::BundleType;

    const RELEASE: &str = include_str!("../../../../tests/fixtures/update/release-manifest.json");

    // spec: OUT-262
    #[test]
    fn bundle_type_display_matches_the_installer_detection_in_os_app() {
        // B4-RELCHECK R2: `register_installer` reicht `BundleType::to_string()` an `Installer::detect` weiter. Ändert
        // Tauri diese Darstellung, gälte jede Windows-Installation als unbekannt und bekäme keine Updates.
        assert_eq!(BundleType::Msi.to_string(), "msi");
        assert_eq!(BundleType::Nsis.to_string(), "nsis");
        let manifest: UpdateManifest = serde_json::from_str(RELEASE).unwrap();
        for (bundle, installer, suffix) in [
            (BundleType::Msi, Installer::Msi, ".msi"),
            (BundleType::Nsis, Installer::Nsis, "-setup.exe"),
        ] {
            let detected = Installer::detect("windows", Some(&bundle.to_string()));
            assert_eq!(detected, installer, "{bundle}");
            let context = UpdateContext {
                os: "windows".into(),
                arch: "x86_64".into(),
                installer: detected,
                ..Default::default()
            };
            let (_, entry) = context
                .select(&manifest)
                .expect("Eintrag im Release-Manifest");
            assert!(entry.url.ends_with(suffix), "{bundle}: {}", entry.url);
        }
    }
}
