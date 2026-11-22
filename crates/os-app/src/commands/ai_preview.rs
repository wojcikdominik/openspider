//! Schalter „KI (Vorschau)“ in der Service-Schicht (Plan M4 E1, Nutzerentscheidung E-2 b; M4-F02).
//!
//! Mit ausgeschaltetem Schalter (Standard) liefert kein Command KI-Oberfläche an das Frontend: Die Tab-, Filter-,
//! Zähler- und Exportlisten lassen die KI-Tabs und ihre Exporte weg ([`visible_tabs`], [`is_ai_name`]), und jeder
//! Crawl bzw. jede Crawl-Analyse läuft auf einer Laufkopie, in der [`os_ai::PreviewSwitch::restrict`] die KI-Schritte
//! abgeschaltet hat ([`restrict`]). Das gleiche Muster wie `os_mcp::tools::offered` für die MCP-Werkzeuge.
use os_ai::PreviewSwitch;
use os_analyze::defs::TabDef;
use os_model::CrawlConfig;

use crate::App;

/// Kennungen der KI-Tabs (UI-004 [M4]: AI, AI Decisions, AI Suggestions, AI Search) aus dem Tab-Register.
pub fn ai_tab_ids() -> [&'static str; 4] {
    use os_analyze::tabs::{ai, ai_decisions, ai_search, ai_suggestions};
    [
        ai::tab().id,
        ai_decisions::tab().id,
        ai_suggestions::tab().id,
        ai_search::tab().id,
    ]
}

/// Ist `tab` einer der KI-Tabs?
pub fn is_ai_tab(tab: &str) -> bool {
    ai_tab_ids().contains(&tab)
}

/// Gehört ein Export-, Report- oder Kennzahlname (`<Tab>:<Name>`) zu einem KI-Tab? Alle KI-Exporte und -Reports
/// tragen den Namen ihres KI-Tabs als ersten Teil (`AI:Embeddings`, `AI Decisions:All Answers`, `AI:Action Plan`).
pub fn is_ai_name(name: &str) -> bool {
    name.split_once(':')
        .is_some_and(|(group, _)| is_ai_tab(group))
}

/// Der Schalter aus den aktuellen App-Einstellungen.
pub fn switch(app: &App) -> PreviewSwitch {
    PreviewSwitch::from_settings(&app.settings_snapshot())
}

/// Schaltet in der Laufkopie `cfg` bei ausgeschaltetem Schalter alle KI-Schritte ab (vor jedem Crawl, jeder
/// Fortsetzung und jeder Crawl-Analyse).
pub fn restrict(app: &App, cfg: &mut CrawlConfig) {
    switch(app).restrict(cfg);
}

/// Konfiguration für eine Fortsetzung: `None`, wenn der Schalter an der gespeicherten Konfiguration nichts ändert
/// (die Engine liest sie dann wie bisher aus der Crawl-Datei), sonst die eingeschränkte Laufkopie.
pub fn restricted_override(switch: PreviewSwitch, stored: &CrawlConfig) -> Option<CrawlConfig> {
    let mut run = stored.clone();
    switch.restrict(&mut run);
    (run != *stored).then_some(run)
}

/// Die Tabs, die das Frontend sieht: ohne die KI-Tabs, solange der Schalter aus ist.
pub fn visible_tabs(switch: PreviewSwitch, tabs: Vec<TabDef>) -> Vec<TabDef> {
    if switch.is_on() {
        return tabs;
    }
    tabs.into_iter().filter(|t| !is_ai_tab(t.id)).collect()
}

/// Ob ein Name (Tab, Export, Report, Kennzahl) bei diesem Schalter angezeigt wird.
pub fn shows_name(switch: PreviewSwitch, name: &str) -> bool {
    switch.is_on() || !(is_ai_tab(name) || is_ai_name(name))
}

#[cfg(test)]
mod tests {
    use super::*;

    // spec: UI-004 [M4]
    // Plan M4 E1 (E-2 b): Schalter „KI (Vorschau)“ aus → keine KI-Tabs in den Listen für das Frontend.
    #[test]
    fn the_four_ai_tabs_are_hidden_only_while_the_switch_is_off() {
        let all = || os_analyze::tabs::configured(&CrawlConfig::default());
        let ids = |tabs: &[TabDef]| tabs.iter().map(|t| t.id).collect::<Vec<_>>();
        let off = visible_tabs(PreviewSwitch::OFF, all());
        let on = visible_tabs(PreviewSwitch::ON, all());
        let all = all();
        assert_eq!(ids(&on), ids(&all));
        assert_eq!(off.len(), all.len() - 4);
        for tab in ["AI", "AI Decisions", "AI Suggestions", "AI Search"] {
            assert!(ids(&on).contains(&tab), "{tab}");
            assert!(!ids(&off).contains(&tab), "{tab}");
        }
        assert!(ids(&off).contains(&"Internal"));
    }

    // spec: UI-004 [M4]
    // Plan M4 E1 (E-2 b): `catalog.tabs` über den Command-Weg – Standard-Einstellungen (leerer Datenordner) ohne KI-Tabs,
    // nach `settings.set` mit `aiApp.previewEnabled` mit allen vier, danach wieder ohne.
    #[tokio::test]
    async fn catalog_tabs_follow_the_switch_in_the_app_settings() {
        let dir = tempfile::tempdir().unwrap();
        let app = App::new(crate::AppConfig::new(dir.path())).unwrap();
        let tabs = || async {
            let v = app
                .dispatch(
                    crate::CallCtx::for_window("main"),
                    "catalog.tabs",
                    serde_json::Value::Null,
                )
                .await
                .unwrap();
            v.as_array()
                .unwrap()
                .iter()
                .map(|t| t["id"].as_str().unwrap().to_string())
                .collect::<Vec<_>>()
        };
        let set = |on: bool| {
            app.dispatch(
                crate::CallCtx::for_window("main"),
                "settings.set",
                serde_json::json!({ "patch": { "aiApp": { "previewEnabled": on } } }),
            )
        };
        let ai = |ids: &[String]| ids.iter().filter(|id| is_ai_tab(id)).count();
        let off = tabs().await;
        assert_eq!(ai(&off), 0, "{off:?}");
        assert!(off.contains(&"Internal".to_string()));
        assert_eq!(switch(&app), PreviewSwitch::OFF);
        set(true).await.unwrap();
        assert_eq!(switch(&app), PreviewSwitch::ON);
        let on = tabs().await;
        assert_eq!(ai(&on), 4, "{on:?}");
        assert_eq!(on.len(), off.len() + 4);
        set(false).await.unwrap();
        assert_eq!(ai(&tabs().await), 0);
    }

    // spec: UI-004 [M4]
    #[test]
    fn ai_names_are_recognized_by_their_tab_prefix_only() {
        for name in [
            "AI:All",
            "AI:Embeddings",
            "AI Decisions:All Answers",
            "AI Suggestions:Titles",
            "AI Search:Missing llms.txt",
            "AI:Action Plan",
        ] {
            assert!(is_ai_name(name), "{name}");
            assert!(!shows_name(PreviewSwitch::OFF, name), "{name}");
            assert!(shows_name(PreviewSwitch::ON, name), "{name}");
        }
        for name in [
            "Internal:All",
            "AMP:All",
            "Accessibility:All",
            "Crawl Overview",
            "Response Codes:Internal Client Error (4xx) Inlinks",
        ] {
            assert!(!is_ai_name(name), "{name}");
            assert!(shows_name(PreviewSwitch::OFF, name), "{name}");
        }
    }

    // spec: AI-001
    // Plan M4 E1 (E-2 b): Schalter aus → die Fortsetzung läuft ohne KI-Schritte, sonst wie bisher ohne Override.
    #[test]
    fn a_resume_gets_an_override_only_when_the_switch_changes_the_stored_config() {
        let mut stored = CrawlConfig::default();
        PreviewSwitch::OFF.restrict(&mut stored);
        assert_eq!(restricted_override(PreviewSwitch::OFF, &stored), None);
        let mut with_ai = CrawlConfig::default();
        with_ai.ai.enabled = true;
        let run = restricted_override(PreviewSwitch::OFF, &with_ai).expect("restricted");
        assert!(!run.ai.enabled && !run.analysis.embeddings && !run.analysis.ai_search);
        assert_eq!(restricted_override(PreviewSwitch::ON, &with_ai), None);
    }
}
