//! Feste Start- und Änderungsreihenfolge der App-Dienste.
use super::{lifecycle_bus::CrawlEvent, App};
pub fn start(app: &App) {
    crate::assets::init(app);
    crate::commands::settings::on_startup(app);
    crate::session::init(app);
    crate::mcp::init(app);
    crate::commands::segment::on_startup(app);
    crate::commands::sidebar_ext::on_startup(app);
    crate::commands::detail_spelling::on_startup(app);
    os_engine::memory::apply_settings(&app.settings_snapshot().storage);
    crate::commands::settings_network::on_startup(app);
    crate::logging::init_for_app(app);
    crate::commands::api::on_startup(app);
    crate::commands::crawls::start_retention(app);
    crate::commands::tasks::on_startup(app);
    crate::commands::mail::on_startup(app);
    crate::commands::update::on_startup(app);
    app.crawl_events().subscribe(Box::new(|app, event| {
        if let CrawlEvent::ConfigChanged { key, old, new } = event {
            crate::commands::api::on_config_changed(app, key, old, new);
            crate::commands::sidebar_ext::on_config_changed(app, key, old, new);
            // M4-V01 (AI-075): Neuprüfung der Vorschläge nach Grenzwertänderung.
            crate::commands::ai_suggest::on_config_changed(app, key, old, new);
        }
    }));
}
pub fn settings_changed(app: &App) {
    crate::commands::settings::on_settings_changed(app);
    os_engine::memory::apply_settings(&app.settings_snapshot().storage);
    crate::logging::apply_settings(app);
    crate::commands::crawls::on_settings_changed(app);
}
