//! Optimierungsvorschläge in der App (M4-V01; 08 AI-070 … AI-076): Pixelbreiten nach UI-038, Tab „AI Suggestions“,
//! Standard-Geltungsbereich, Konsistenz mit den Filtern, Commands `ai.suggest*`, E2E-Crawl-Datei. Legt Crawl-Dateien an
//! und trägt deshalb die Ausnahme des Lints 2.3 Regel 1 (wie die übrigen Tests, die Crawl-Dateien anlegen).
#![allow(clippy::disallowed_types, clippy::disallowed_methods)]

use std::collections::{BTreeMap, BTreeSet};

use os_ai::cost::PriceBasis;
use os_ai::provider::Model;
use os_ai::tasks::suggest::{
    Checker, Element, ElementLimits, NoStyleRules, PageInput, RuleSet, SiteIndex, SuggestOptions,
};
use os_app::commands::ai_suggest::*;
use os_app::{App, CallCtx, CmdError, ErrorCode};
use os_model::CrawlConfig;
use os_store::ai::suggestions as store;
use os_store::CrawlFile;
use serde_json::{json, Value};

/// Fixture der Vorschläge (`tests/fixtures/ai/suggest/`): Crawl-Datei mit den Seiten aus `pages.json` (Pixelbreiten nach
/// UI-038) und Vorschlägen aus einem Lauf über den Kern mit den Antworten aus `mock-answers.d/v01.json`. Dient den
/// Tests unten und als Crawl-Datei der E2E-Tests (`tests/fixtures/ai/suggest/e2e/suggest.ospider`, neu erzeugt mit
/// `UPDATE_GOLDEN=1`).
mod fixture {
    use super::*;
    use os_ai::adapters::openai_responses::RESPONSES_CAPABILITIES;
    use os_ai::cache::CacheOptions;
    use os_ai::limits::{Limiter, LimitsConfig, TokioClock};
    use os_ai::policy::{ConsentBook, ConsentGate, ConsentKey, RedactionSettings, Redactor};
    use os_ai::provider::{AiProvider, Input, ProviderFuture, Request, Response, Usage};
    use os_ai::tasks::suggest::{ElementJob, Engine, FileStore, RunControl};
    use os_ai::PreviewSwitch;
    use std::path::{Path, PathBuf};
    use std::sync::Arc;

    pub const BASE: &str = "http://ai.test:8080";

    pub fn root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/ai")
    }

    pub fn pages() -> Vec<Value> {
        let doc: Value = serde_json::from_str(
            &std::fs::read_to_string(root().join("suggest/pages.json")).unwrap(),
        )
        .unwrap();
        doc["pages"].as_array().unwrap().clone()
    }

    fn lit(v: &Value) -> String {
        match v {
            Value::Null => "NULL".into(),
            Value::Number(n) => n.to_string(),
            Value::String(s) => format!("'{}'", s.replace('\'', "''")),
            other => format!("'{}'", other.to_string().replace('\'', "''")),
        }
    }

    /// Leere Crawl-Datei mit den Fixture-Seiten (URL-IDs 1 … n in Fixture-Reihenfolge).
    pub fn crawl(path: &Path) -> CrawlFile {
        use os_model::time::{IdSource, SeededIds};
        let created = os_model::Timestamp::from_db("2026-11-26T08:00:00.000Z").unwrap();
        let mut cfg = CrawlConfig::default();
        cfg.ai.enabled = true;
        cfg.ai.suggestions.title = true;
        cfg.ai.suggestions.meta = true;
        let f = CrawlFile::create(
            path,
            &os_store::CrawlInit {
                crawl_id: SeededIds::new(71).new_crawl_id(created),
                mode: os_model::CrawlMode::Spider,
                start_url: Some(format!("{BASE}/")),
                list_source: None,
                project: "V01".into(),
                name: "ai-suggestions".into(),
                config: cfg,
                created_at: created,
            },
            os_store::StorageProfile::Balanced,
        )
        .unwrap();
        for (i, p) in pages().iter().enumerate() {
            let address = format!("{BASE}{}", p["path"].as_str().unwrap());
            let text = |k: &str| p[k].as_str().map(|s| s.to_string());
            let len = |v: &Option<String>| v.as_ref().map(|s| json!(s.chars().count()));
            let px = |v: &Option<String>, size| v.as_ref().map(|s| json!(ui038(s, size)));
            let (title, meta, h1) = (text("title"), text("meta"), text("h1"));
            let indexable = p["indexable"].as_bool().unwrap_or(true);
            let values = [
                json!(i + 1),
                json!(address),
                json!(address.len()),
                json!(if indexable {
                    "Indexable"
                } else {
                    "Non-Indexable"
                }),
                json!(if indexable {
                    Value::Null
                } else {
                    json!("noindex")
                }),
                json!(title.is_some() as i64),
                json!(title),
                len(&title).unwrap_or(Value::Null),
                px(&title, 20.0).unwrap_or(Value::Null),
                json!(meta.is_some() as i64),
                json!(meta),
                len(&meta).unwrap_or(Value::Null),
                px(&meta, 14.0).unwrap_or(Value::Null),
                json!(h1.is_some() as i64),
                json!(h1),
                len(&h1).unwrap_or(Value::Null),
            ];
            let v: Vec<String> = values.iter().map(lit).collect();
            f.conn()
                .execute_batch(&format!(
                    "INSERT INTO urls(id,url,address,origin,is_internal,state,discovered_via,folder_depth,url_length,
                       content_class,content_type,status_code,status,indexability,indexability_status,title_count,title_1,
                       title_1_len,title_1_px,meta_desc_count,meta_desc_1,meta_desc_1_len,meta_desc_1_px,h1_count,h1_1,h1_1_len)
                     VALUES({id},{a},{a},'{BASE}',1,'Done','Seed',1,{l},'HTML','text/html; charset=utf-8',200,'OK',{rest});",
                    id = v[0],
                    a = v[1],
                    l = v[2],
                    rest = v[3..].join(","),
                ))
                .unwrap();
        }
        f.set_meta("state", "Completed").unwrap();
        f.set_meta("end_reason", "queue_empty").unwrap();
        f
    }

    /// Antworten je MOCK-ID aus `mock-answers.d/v01.json`, ohne HTTP.
    pub struct Answers(BTreeMap<String, Value>);

    impl Answers {
        pub fn load() -> Self {
            let doc: BTreeMap<String, Value> = serde_json::from_str(
                &std::fs::read_to_string(root().join("mock-answers.d/v01.json")).unwrap(),
            )
            .unwrap();
            Self(
                doc.into_iter()
                    .map(|(k, v)| (k, v["default"].clone()))
                    .collect(),
            )
        }
    }

    impl AiProvider for Answers {
        fn provider_id(&self) -> &str {
            "openai"
        }
        fn models(&self) -> ProviderFuture<'_, Vec<Model>> {
            Box::pin(async { Ok(Vec::new()) })
        }
        fn execute(&self, request: Request) -> ProviderFuture<'_, Response> {
            Box::pin(async move {
                let text: String = request
                    .input
                    .iter()
                    .filter_map(|i| match i {
                        Input::Text { text } => Some(text.as_str()),
                        _ => None,
                    })
                    .collect();
                let id = text
                    .split("MOCK-ID:")
                    .nth(1)
                    .and_then(|r| r.split_whitespace().next())
                    .unwrap_or("");
                Ok(Response {
                    content: Some(self.0.get(id).cloned().unwrap_or(json!({"candidates": []}))),
                    usage: Usage {
                        input_tokens: Some(300),
                        output_tokens: Some(60),
                        cached_input_tokens: Some(0),
                        cache_write_tokens: Some(0),
                        reasoning_tokens: Some(0),
                    },
                    refusal: false,
                    provider_request_id: None,
                    duration_ms: 1,
                })
            })
        }
    }

    /// Lauf beider Elemente über den Kern (Title: /a/ … /g/, Meta Description: /m1/ … /m5/), Text aus `pages.json`.
    pub async fn run(file: CrawlFile) -> CrawlFile {
        let cfg = file.config().unwrap();
        let rows = store::pages(file.conn()).unwrap();
        let index = SiteIndex::from_pages(&rows);
        let fixture = pages();
        let store = FileStore::new(file);
        let ledger = store.with(|f| {
            os_store::ai::budget::open_ledger(
                f.conn(),
                os_model::budget::BudgetLimits {
                    unlimited: true,
                    ..Default::default()
                },
                &[],
            )
            .unwrap()
        });
        let run_id = store.with(|f| {
            store::begin_run(
                f.conn(),
                "2026-11-26T09:00:00Z",
                &json!({"task": "suggest"}),
            )
            .unwrap()
        });
        let consent = ConsentGate::dialog(ConsentBook::new());
        for e in Element::ALL {
            consent
                .grant(
                    ConsentKey::new("openai", e.task().as_str(), e.fields()),
                    0,
                    None,
                )
                .unwrap();
        }
        let redactor = Redactor::new(RedactionSettings::default());
        let limiter = Limiter::new(LimitsConfig::default(), Arc::new(TokioClock::new()));
        let clock = os_model::time::ManualClock::new(
            os_model::Timestamp::from_db("2026-11-26T09:00:00.000Z").unwrap(),
        );
        let control = RunControl::default();
        let provider = Answers::load();
        for (element, prefix) in [(Element::Title, "/"), (Element::MetaDescription, "/m")] {
            let (mut plan, rates, _) = catalog_plan(element, &cfg).expect("Katalogmodell");
            plan.model.capabilities = RESPONSES_CAPABILITIES;
            let inputs: Vec<PageInput> = rows
                .iter()
                .zip(&fixture)
                .filter(|(r, p)| {
                    let path = p["path"].as_str().unwrap();
                    r.in_standard_scope
                        && path.len() <= 4
                        && (prefix == "/m") == path.starts_with("/m")
                })
                .map(|(r, p)| PageInput {
                    url_id: r.url_id,
                    address: r.address.clone(),
                    title: r.title.clone(),
                    meta_description: r.meta_description.clone(),
                    h1: r.h1.clone(),
                    text: p["text"].as_str().map(str::to_string),
                    language: p["lang"].as_str().map(str::to_string),
                    ..PageInput::default()
                })
                .collect();
            let engine = Engine {
                switch: PreviewSwitch::ON,
                consent: &consent,
                redactor: &redactor,
                provider: &provider,
                limiter: &limiter,
                ledger: &ledger,
                plan: &plan,
                price: PriceBasis::Rates(&rates),
                store: &store,
                meter: &ui038,
                style: &NoStyleRules,
                clock: &clock,
                control: &control,
                run_id,
                base_url: "https://api.openai.com/v1".into(),
                cache: CacheOptions::default(),
                schema_overhead_tokens: 80,
                workers: 4,
            };
            engine
                .run(&ElementJob {
                    element,
                    pages: &inputs,
                    index: &index,
                    limits: ElementLimits::from_config(element, &cfg),
                    options: SuggestOptions::default(),
                })
                .await
                .unwrap();
        }
        store.with(|f| store::end_run(f.conn(), run_id, "2026-11-26T09:01:00Z").unwrap());
        store.into_inner()
    }
}

mod tests {
    use super::fixture::{self, BASE};
    use super::*;
    use os_analyze::defs::ColumnSql;
    use os_app::pool::CrawlKey;
    use os_app::runtime::sessions::SessionInfo;
    use os_app::{AppConfig, WindowId};
    use std::sync::Arc;

    fn ctx_of(file: &CrawlFile) -> SiteContext {
        context_from(store::site_data(file, false).unwrap())
    }

    fn scope(
        file: &CrawlFile,
        cfg: &CrawlConfig,
        ctx: &SiteContext,
        element: Element,
        s: &Scope,
    ) -> Vec<i64> {
        scope_ids(
            ctx,
            element,
            s,
            &|tab, ids| {
                os_analyze::views::filter_members_in(&[tab], file, cfg, ids).map_err(|e| {
                    CmdError::invalid_input("err-view-query", "spec").with_detail(e.to_string())
                })
            },
            &|segment| Ok(store::segment_url_ids(file.conn(), segment).unwrap()),
        )
        .unwrap()
    }

    fn column_value(file: &CrawlFile, key: &str, url_id: i64) -> Value {
        let tab = os_analyze::tabs::ai_suggestions::tab();
        let column = tab
            .columns
            .iter()
            .find(|c| c.key == key)
            .unwrap_or_else(|| panic!("{key}"));
        let ColumnSql::Expr(sql) = column.sql else {
            panic!("{key}: kein Ausdruck")
        };
        file.conn()
            .query_row(
                &format!("SELECT {sql} FROM urls u WHERE u.id = ?1"),
                [url_id],
                |r| {
                    Ok(match r.get_ref(0)? {
                        rusqlite::types::ValueRef::Null => Value::Null,
                        rusqlite::types::ValueRef::Integer(n) => json!(n),
                        rusqlite::types::ValueRef::Text(t) => json!(String::from_utf8_lossy(t)),
                        other => json!(format!("{other:?}")),
                    })
                },
            )
            .unwrap()
    }

    async fn built() -> (tempfile::TempDir, CrawlFile) {
        let dir = tempfile::tempdir().unwrap();
        let file = fixture::crawl(&dir.path().join("suggest.ospider"));
        let file = fixture::run(file).await;
        (dir, file)
    }

    // spec: AI-071 [M4]
    // spec: AI-072 [M4]
    // `Title Suggestion k Pixel Width` bzw. `Meta Description Suggestion k Pixel Width` sind für jeden Kandidaten der
    // Fixture identisch mit dem Wert, den UI-038 für denselben Text liefert (14 px bzw. 20 px, Desktop-Profil); die
    // Basisspalten `… 1 Pixel Width` bei gleichem Text ebenso.
    #[tokio::test]
    async fn stored_and_shown_pixel_widths_equal_ui038() {
        let (_dir, file) = built().await;
        let mut compared = 0;
        for element in Element::ALL {
            let prefix = if element == Element::Title {
                "Title"
            } else {
                "Meta Description"
            };
            for r in store::load_all(file.conn(), element.id()).unwrap() {
                for (slot, c) in r.displayed().iter().enumerate().take(5) {
                    let measured = c.value.split_whitespace().collect::<Vec<_>>().join(" ");
                    let expected = ui038(&measured, element.font_px());
                    assert_eq!(c.pixel_width, Some(expected), "{}", c.value);
                    let shown = column_value(
                        &file,
                        &format!("{prefix} Suggestion {} Pixel Width", slot + 1),
                        r.url_id,
                    );
                    assert_eq!(shown, json!(expected), "{prefix} {}: {}", slot + 1, c.value);
                    compared += 1;
                }
            }
        }
        assert_eq!(compared, 7 * 3 + 5 * 3);
        // Gleicher Text wie die Basisspalte → gleicher Wert (Meta Description 1 Pixel Width aus der Extraktion).
        let meta = file
            .conn()
            .query_row("SELECT meta_desc_1 FROM urls WHERE id = 1", [], |r| {
                r.get::<_, String>(0)
            })
            .unwrap();
        assert_eq!(
            column_value(&file, "Meta Description 1 Pixel Width", 1),
            json!(ui038(&meta, 14.0))
        );
    }

    // spec: AI-071 [M4]
    // spec: AI-070 [M4]
    // Tab „AI Suggestions“ zeigt bei /b/ nach dem Nachlauf „Y“ als Kandidat 1, bei /c/ (ohne Title) Kandidaten mit
    // leerem `Title 1`; Filter `Titles`, `Meta Descriptions`, `Open`.
    #[tokio::test]
    async fn tab_columns_and_filters_reflect_the_run() {
        let (_dir, file) = built().await;
        assert_eq!(
            column_value(&file, "Title Suggestion 1", 2),
            json!("Rucksäcke für Wanderungen im großen Vergleich")
        );
        assert_eq!(
            column_value(&file, "Title Suggestion 2", 2),
            json!("Leichte Wanderschuhe für Damen und Herren")
        );
        assert_eq!(column_value(&file, "Title 1", 3), Value::Null);
        assert_eq!(
            column_value(&file, "Title Suggestion Status", 3),
            json!("Valid")
        );
        assert_eq!(column_value(&file, "Review Status", 3), json!("Open"));
        let tab = os_analyze::tabs::ai_suggestions::tab();
        let count = |filter: &str| -> i64 {
            let f = tab.filters.iter().find(|f| f.id == filter).unwrap();
            file.conn()
                .query_row(
                    &format!(
                        "SELECT count(*) FROM urls u WHERE ({}) AND ({})",
                        tab.population, f.predicate
                    ),
                    [],
                    |r| r.get(0),
                )
                .unwrap()
        };
        assert_eq!(count("All"), 12);
        assert_eq!(count("Titles"), 7);
        assert_eq!(count("Meta Descriptions"), 5);
        assert_eq!(count("Open"), 12);
        assert_eq!(count("Accepted"), 0);
        assert_eq!(count("Errors"), 0);
    }

    // spec: AI-070 [M4]
    // spec: AI-072 [M4]
    // Standard-Geltungsbereich Titles = Vereinigung von Missing, Duplicate, Over/Below Characters, Over/Below Pixels,
    // Same as H1 (aktuelle Grenzwerte) und Title Not Matching Content (AI); Meta Descriptions entsprechend; jede URL
    // höchstens einmal; nicht indexierbare URLs nie.
    #[test]
    fn default_scope_is_the_union_of_the_problem_filters() {
        let dir = tempfile::tempdir().unwrap();
        let file = fixture::crawl(&dir.path().join("scope.ospider"));
        // AI-054: /m1/ (ID 8) hat einen unpassenden Title, die noindex-Seite (ID 13) ebenso (bleibt außen vor).
        file.conn()
            .execute_batch(
                "INSERT INTO ai_questions VALUES('title-matches-content','1.0.0','{}');
                 INSERT INTO ai_answers(url_id,question_id,question_version,answer_json,status)
                   VALUES(8,'title-matches-content','1.0.0','{\"value\":false}','Success'),
                         (9,'title-matches-content','1.0.0','{\"value\":true}','Success'),
                         (13,'title-matches-content','1.0.0','false','Success');",
            )
            .unwrap();
        let cfg = CrawlConfig::default();
        let ctx = ctx_of(&file);
        let titles = scope(&file, &cfg, &ctx, Element::Title, &Scope::Default);
        assert_eq!(titles, vec![1, 2, 3, 4, 5, 6, 7, 8]);
        let metas = scope(&file, &cfg, &ctx, Element::MetaDescription, &Scope::Default);
        assert_eq!(metas, vec![8, 9, 10, 11, 12]);
        // Mit geänderten Grenzwerten (Title max. 40 Zeichen) wächst der Bereich entsprechend.
        let mut strict = cfg.clone();
        strict.title.max_chars = 40;
        let ctx = ctx_of(&file);
        let more = scope(&file, &strict, &ctx, Element::Title, &Scope::Default);
        assert!(more.len() > titles.len(), "{more:?}");
        assert_eq!(
            scope(
                &file,
                &cfg,
                &ctx,
                Element::Title,
                &Scope::Selected {
                    url_ids: vec![2, 13, 2]
                }
            ),
            vec![2]
        );
        assert_eq!(
            scope(&file, &cfg, &ctx, Element::Title, &Scope::All).len(),
            12
        );
    }

    // spec: AI-075 [M4]
    // Konsistenz: Für 50 Texte stimmt „gültig nach Länge und Pixelbreite“ mit „in keinem der Filter Over/Below X
    // Characters und Over/Below X Pixels (ONPAGE-093 – 096)“ überein.
    #[test]
    fn validity_matches_the_page_title_filters_for_fifty_texts() {
        let dir = tempfile::tempdir().unwrap();
        let file = fixture::crawl(&dir.path().join("consistency.ospider"));
        file.conn().execute_batch("DELETE FROM urls").unwrap();
        let mut texts = Vec::new();
        for n in [28, 29, 30, 31, 45, 59, 60, 61, 62, 70] {
            texts.push("a".repeat(n));
            texts.push(format!("Ti{}", "W".repeat(n.min(40) - 2)));
            texts.push(format!(
                "{} {}",
                "Wanderschuhe".repeat(n / 12),
                "x".repeat(n % 12)
            ));
            texts.push("i".repeat(n * 2));
            texts.push(format!("Seite {n:02} – Ratgeber für {}", "Ü".repeat(n / 3)));
        }
        assert_eq!(texts.len(), 50);
        for (i, t) in texts.iter().enumerate() {
            let collapsed = t.split_whitespace().collect::<Vec<_>>().join(" ");
            file.conn()
                .execute(
                    "INSERT INTO urls(id,url,address,origin,is_internal,state,discovered_via,folder_depth,url_length,
                       content_class,status_code,indexability,title_count,title_1,title_1_len,title_1_px)
                     VALUES(?1,?2,?2,'http://ai.test:8080',1,'Done','Seed',1,30,'HTML',200,'Indexable',1,?3,?4,?5)",
                    rusqlite::params![
                        i as i64 + 1,
                        format!("{BASE}/t{i}/"),
                        t,
                        collapsed.chars().count() as i64,
                        ui038(&collapsed, 20.0)
                    ],
                )
                .unwrap();
        }
        let cfg = CrawlConfig::default();
        let ids: Vec<i64> = (1..=50).collect();
        let members = os_analyze::views::filter_members_in(
            &[os_analyze::tabs::page_titles::tab()],
            &file,
            &cfg,
            &ids,
        )
        .unwrap();
        let filtered: BTreeSet<i64> = members
            .into_iter()
            .filter(|m| TITLE_FILTERS[2..6].contains(&m.filter.as_str()))
            .flat_map(|m| m.ids)
            .collect();
        let index = SiteIndex::default();
        let check = Checker {
            rules: RuleSet {
                duplicates: false,
                format: false,
                language: false,
                style: false,
                length: true,
            },
            ..checker(Element::Title, &cfg, &index)
        };
        let (mut valid, mut invalid) = (0, 0);
        for (i, t) in texts.iter().enumerate() {
            let ok = check.check(i as i64 + 1, None, t).valid();
            let in_filter = filtered.contains(&(i as i64 + 1));
            assert_eq!(ok, !in_filter, "{t:?}: gültig {ok}, in Filter {in_filter}");
            if ok {
                valid += 1
            } else {
                invalid += 1
            }
        }
        assert!(valid > 5 && invalid > 5, "{valid}/{invalid}");
    }

    fn app_for(file: CrawlFile, dir: &std::path::Path) -> App {
        let path = file.path().to_path_buf();
        let cfg = file.config().unwrap();
        file.checkpoint_and_truncate().unwrap();
        drop(file);
        let app = App::new(AppConfig::new(dir.join("home"))).unwrap();
        let key = CrawlKey("v01".into());
        app.register_crawl(key.clone(), path.clone());
        app.sessions().set_current(
            WindowId("main".into()),
            SessionInfo {
                key,
                path,
                config: Arc::new(cfg),
                state: os_model::CrawlState::Completed,
                writable: true,
                engine: None,
            },
        );
        app
    }

    async fn call(app: &App, name: &str, args: Value) -> Result<Value, CmdError> {
        app.dispatch(CallCtx::for_window("main"), name, args).await
    }

    async fn preview(app: &App, on: bool) {
        call(
            app,
            "settings.set",
            json!({"patch": {"aiApp": {"previewEnabled": on}}}),
        )
        .await
        .unwrap();
    }

    // Plan M4 E1 (E-2 b): mit Schalter „KI (Vorschau)“ aus antwortet jeder Vorschlags-Command ohne Wirkung.
    // spec: AI-070 [M4]
    #[tokio::test]
    async fn every_command_refuses_while_the_preview_switch_is_off() {
        let (dir, file) = built().await;
        let app = app_for(file, dir.path());
        for name in [
            "ai.suggestRun",
            "ai.suggestPlan",
            "ai.suggestDetail",
            "ai.suggestReview",
            "ai.suggestCheck",
            "ai.suggestRecheck",
            "ai.suggestFocusImport",
        ] {
            let err = call(&app, name, json!({"urlId": 1})).await.unwrap_err();
            assert_eq!(err.code, ErrorCode::Unsupported, "{name}");
            assert_eq!(err.message_key, "err-ai-preview-off", "{name}");
        }
    }

    // spec: AI-076 [M4]
    // Prüfansicht, Live-Prüfung (Pixelbreite aus dem Kern), Übernehmen, Bearbeiten mit Verstoß (`Edited` mit
    // Verstoßliste), Massenaktion mit gezählten URLs ohne gültigen Kandidaten, Zurücksetzen.
    #[tokio::test]
    async fn review_commands_accept_edit_bulk_and_reset() {
        let (dir, file) = built().await;
        let app = app_for(file, dir.path());
        preview(&app, true).await;
        let detail = call(&app, "ai.suggestDetail", json!({"urlId": 2}))
            .await
            .unwrap();
        let title = &detail["elements"][0];
        assert_eq!(title["element"], "title");
        assert_eq!(
            title["candidates"][0]["value"],
            "Rucksäcke für Wanderungen im großen Vergleich"
        );
        assert_eq!(title["candidates"][0]["selected"], true);
        assert_eq!(
            title["candidates"][1]["violations"][0],
            format!("duplicate_candidate:{BASE}/a/")
        );
        let check = call(
            &app,
            "ai.suggestCheck",
            json!({"urlId": 2, "element": "title", "value": "Viel zu kurz"}),
        )
        .await
        .unwrap();
        assert_eq!(check["pixelWidth"], json!(ui038("Viel zu kurz", 20.0)));
        assert_eq!(check["valid"], false);
        let edited = call(
            &app,
            "ai.suggestReview",
            json!({"urlIds": [2], "element": "title", "action": "edit", "value": "Viel zu kurz"}),
        )
        .await
        .unwrap();
        assert_eq!(edited["updated"], 1);
        let detail = call(&app, "ai.suggestDetail", json!({"urlId": 2}))
            .await
            .unwrap();
        assert_eq!(detail["elements"][0]["review"], "Edited");
        assert_eq!(
            detail["elements"][0]["accepted"]["violations"],
            check["violations"]
        );
        // „Kandidat 1 übernehmen“ für /a/ … /g/ und /noindex/ (ohne Eintrag): alle sieben gültig.
        let bulk = call(
            &app,
            "ai.suggestReview",
            json!({"urlIds": [1, 3, 4, 5, 6, 7, 13], "element": "title", "action": "acceptFirst"}),
        )
        .await
        .unwrap();
        assert_eq!(
            (bulk["accepted"].clone(), bulk["skipped"].clone()),
            (json!(6), json!(0))
        );
        let reset = call(
            &app,
            "ai.suggestReview",
            json!({"urlIds": [1], "element": "title", "action": "reset"}),
        )
        .await
        .unwrap();
        assert_eq!(reset["updated"], 1);
        let detail = call(&app, "ai.suggestDetail", json!({"urlId": 1}))
            .await
            .unwrap();
        assert_eq!(detail["elements"][0]["review"], "Open");
        let bad = call(
            &app,
            "ai.suggestReview",
            json!({"urlIds": [1], "element": "h9", "action": "reset"}),
        )
        .await
        .unwrap_err();
        assert_eq!(bad.code, ErrorCode::InvalidInput);
    }

    // spec: AI-075 [M4]
    // Neuprüfung ohne Request nach Grenzwertänderung (Title max. 40 Zeichen): Status ändert sich entsprechend.
    #[tokio::test]
    async fn recheck_applies_new_limits_without_a_request() {
        let (dir, file) = built().await;
        let app = app_for(file, dir.path());
        preview(&app, true).await;
        let requests_before = {
            let s = app.sessions().current(&WindowId("main".into())).unwrap();
            let f = CrawlFile::open(&s.path, os_store::OpenMode::ReadOnly).unwrap();
            f.conn()
                .query_row("SELECT count(*) FROM ai_requests", [], |r| {
                    r.get::<_, i64>(0)
                })
                .unwrap()
        };
        let mut s = app.sessions().current(&WindowId("main".into())).unwrap();
        let mut cfg = (*s.config).clone();
        cfg.title.max_chars = 40;
        s.config = Arc::new(cfg);
        app.sessions()
            .set_current(WindowId("main".into()), s.clone());
        let out = call(&app, "ai.suggestRecheck", Value::Null).await.unwrap();
        assert_eq!(out["updated"], 12);
        let detail = call(&app, "ai.suggestDetail", json!({"urlId": 3}))
            .await
            .unwrap();
        // /c/: alle Kandidaten über 40 Zeichen.
        assert_eq!(detail["elements"][0]["status"], "No valid suggestion");
        assert!(detail["elements"][0]["violations"][0]
            .as_str()
            .unwrap()
            .starts_with("length>40"));
        let f = CrawlFile::open(&s.path, os_store::OpenMode::ReadOnly).unwrap();
        let after: i64 = f
            .conn()
            .query_row("SELECT count(*) FROM ai_requests", [], |r| r.get(0))
            .unwrap();
        assert_eq!(after, requests_before);
    }

    async fn set_title_max(app: &App, max: u32) {
        let mut dto = call(app, "config.get", Value::Null).await.unwrap();
        dto["config"]["title"]["max_chars"] = json!(max);
        call(app, "config.set", dto).await.unwrap();
    }

    // spec: AI-075 [M4]
    // Grenzwertänderung über die Crawl-Konfiguration (`config.set`, Title max. 40 Zeichen): die Kandidaten werden ohne
    // Nutzeraktion und ohne Request neu geprüft (kein `ai.suggestRecheck`); mit Schalter aus geschieht nichts.
    #[tokio::test]
    async fn changing_a_limit_in_the_crawl_config_rechecks_without_a_call() {
        let (dir, file) = built().await;
        let app = app_for(file, dir.path());
        let requests = |app: &App| -> i64 {
            let s = app.sessions().current(&WindowId("main".into())).unwrap();
            let f = CrawlFile::open(&s.path, os_store::OpenMode::ReadOnly).unwrap();
            f.conn()
                .query_row("SELECT count(*) FROM ai_requests", [], |r| r.get(0))
                .unwrap()
        };
        let before = requests(&app);
        let status = |d: &Value| d["elements"][0]["status"].as_str().unwrap().to_string();
        let detail = |app: &App| {
            let app = app.clone();
            async move {
                call(&app, "ai.suggestDetail", json!({"urlId": 3}))
                    .await
                    .unwrap()
            }
        };
        // Schalter aus: keine Neuprüfung (die Einträge bleiben unverändert).
        set_title_max(&app, 40).await;
        preview(&app, true).await;
        assert_eq!(status(&detail(&app).await), "Valid");
        set_title_max(&app, 60).await;
        // Schalter an: `config.set` allein genügt.
        set_title_max(&app, 40).await;
        let d = detail(&app).await;
        assert_eq!(status(&d), "No valid suggestion", "{d}");
        assert!(d["elements"][0]["violations"][0]
            .as_str()
            .unwrap()
            .starts_with("length>40"));
        set_title_max(&app, 60).await;
        assert_eq!(status(&detail(&app).await), "Valid");
        assert_eq!(requests(&app), before, "keine Requests");
    }

    // spec: AI-070 [M4]
    // spec: AI-012 [M4]
    // Fokus-Begriffe aus `Address,Keyword`; Voranschlag je Element (Zahlen für den Dialog von M4-U01); `ai.suggestRun`
    // legt die Einträge des Geltungsbereichs mit `Pending (confirmation)` an.
    #[tokio::test]
    async fn focus_import_plan_and_run_prepare_the_confirmation() {
        let dir = tempfile::tempdir().unwrap();
        let file = fixture::crawl(&dir.path().join("plan.ospider"));
        let app = app_for(file, dir.path());
        preview(&app, true).await;
        let csv = format!("Address,Keyword\n{BASE}/a/,wanderschuhe damen\n{BASE}/zz/,unbekannt\n");
        let imported = call(&app, "ai.suggestFocusImport", json!({"csv": csv}))
            .await
            .unwrap();
        assert_eq!(imported["imported"], 1);
        assert_eq!(imported["unknown"], json!([format!("{BASE}/zz/")]));
        let plan = call(
            &app,
            "ai.suggestPlan",
            json!({"elements": ["title", "meta_description"]}),
        )
        .await
        .unwrap();
        assert_eq!(plan["elements"][0]["entries"], 7);
        assert_eq!(plan["elements"][1]["entries"], 5);
        let est = &plan["elements"][0]["estimate"];
        assert_eq!(est["requests"], 7);
        assert_eq!(est["executable"], "Missing API key");
        assert!(est["expectedMicroUsd"].as_u64().unwrap() > 0);
        let handle = call(&app, "ai.suggestRun", json!({"tasks": ["suggest-title"]}))
            .await
            .unwrap();
        let op_id = handle["opId"].as_str().unwrap().to_string();
        let status =
            tokio::time::timeout(std::time::Duration::from_secs(20), app.ops().wait(&op_id))
                .await
                .unwrap()
                .unwrap();
        let result = status.result.unwrap();
        assert_eq!(result["state"], "pendingConfirmation");
        assert_eq!(result["elements"][0]["entries"], 7);
        let detail = call(&app, "ai.suggestDetail", json!({"urlId": 1}))
            .await
            .unwrap();
        assert_eq!(detail["elements"][0]["status"], "Pending (confirmation)");
        assert_eq!(detail["elements"][0]["focus"], "wanderschuhe damen");
    }

    // Crawl-Datei der E2E-Tests (`apps/desktop/e2e/ai-suggestions`): mit `UPDATE_GOLDEN=1` neu erzeugt, sonst geprüft.
    // spec: AI-070 [M4]
    #[tokio::test]
    async fn e2e_fixture_crawl_is_current() {
        let target = fixture::root().join("suggest/e2e/suggest.ospider");
        if std::env::var("UPDATE_GOLDEN").is_ok_and(|v| !v.is_empty() && v != "0") {
            let _ = std::fs::remove_file(&target);
            std::fs::create_dir_all(target.parent().unwrap()).unwrap();
            let file = fixture::run(fixture::crawl(&target)).await;
            file.checkpoint_and_truncate().unwrap();
            drop(file);
            for ext in ["-wal", "-shm"] {
                let _ = std::fs::remove_file(format!("{}{ext}", target.display()));
            }
        }
        let dir = tempfile::tempdir().unwrap();
        let copy = dir.path().join("copy.ospider");
        std::fs::copy(&target, &copy).expect("Fixture fehlt (UPDATE_GOLDEN=1)");
        let file = CrawlFile::open(&copy, os_store::OpenMode::ReadOnly).unwrap();
        assert_eq!(store::load_all(file.conn(), "title").unwrap().len(), 7);
        assert_eq!(
            store::load_all(file.conn(), "meta_description")
                .unwrap()
                .len(),
            5
        );
        assert_eq!(store::pages(file.conn()).unwrap().len(), 13);
    }

    // spec: AI-070 [M4]
    #[test]
    fn focus_csv_and_html_lang_are_parsed() {
        let csv = "\u{feff}Address,Keyword\nhttp://ai.test:8080/a/,wanderschuhe damen\n\"http://ai.test:8080/b/\";\"rucksack\"\n,leer\n";
        assert_eq!(
            parse_focus_csv(csv),
            vec![
                (
                    "http://ai.test:8080/a/".to_string(),
                    "wanderschuhe damen".to_string()
                ),
                ("http://ai.test:8080/b/".to_string(), "rucksack".to_string()),
            ]
        );
        assert_eq!(
            html_lang("<!doctype html><HTML LANG=\"de-DE\"><head>").as_deref(),
            Some("de-DE")
        );
        assert_eq!(html_lang("<html lang=en class=x>").as_deref(), Some("en"));
        assert_eq!(html_lang("<html><body lang=\"fr\">"), None);
    }
}
