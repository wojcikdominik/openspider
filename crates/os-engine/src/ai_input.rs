//! Eingabefelder nach 08 AI-032 für die KI-Warteschlange (M4-D05; Klärungen K4-24, K4-25 Variante A, K4-26).
//!
//! * **Während des Crawls** aus der geparsten Seite ([`from_page`]): Titel, Meta Description, alle H1, alle H2, Text des
//!   Inhaltsbereichs nach ONPAGE-140 (`os_parse::content::area_text`, dieselben Regeln wie Wortzahl und Textanteil),
//!   Wortzahl, `html_lang` (Rohwert von `<html lang>`, ohne `lang` `xml:lang`, wie ONPAGE-153), `hreflang_self`
//!   (selbstreferenzierende hreflang-Angaben), Typen der strukturierten Daten (nur bei eingeschalteter Extraktion, sonst
//!   „nicht vorhanden“) und je Bild der Bildkontext (Bildunterschrift der umgebenden `figure`, sonst Text des
//!   Elternelements, höchstens 300 Zeichen).
//! * **Nach dem Crawl** aus der Crawl-Datei ([`load_pages`]): Spalten von `urls` und `hreflang`, dazu (1) die
//!   gespeicherten KI-Eingaben, sonst (2) aus `page_html` mit derselben Aufbereitung abgeleitet (nichts wird
//!   nachgespeichert), sonst (3) [`InputSource::Unavailable`] (Fragen mit diesen Feldern: `Skipped (input
//!   unavailable)`). Es wird keine Seite neu abgerufen (INTEG-008).
//!
//! Grundmenge G (08 Konventionen): interne Seiten-URLs ohne Ressourcen – HTML-Antworten und Antworten ohne
//! verwertbaren Inhalt (3xx/4xx/5xx).
use std::collections::BTreeMap;
use std::path::Path;

use os_model::ai::hook::{ImageAiContext, InputSource, PageAiFields, IMAGE_CONTEXT_MAX_CHARS};
use os_model::{ContentClass, CrawlConfig, Indexability, ParsedPage};
use os_parse::{ParseConfig, ParseInput};
use os_store::{CrawlFile, OpenMode, StoreError};
use url::Url;

/// Ist die Extraktion strukturierter Daten eingeschaltet (CRAWL-065, TECH-180 ff.; Standard aus)?
pub fn structured_data_enabled(cfg: &CrawlConfig) -> bool {
    let sd = &cfg.extraction.structured_data;
    sd.json_ld || sd.microdata || sd.rdfa
}

/// Gehört eine Antwort zur Grundmenge G (Seite, keine Ressource)?
pub fn in_base_set(class: ContentClass, status: Option<u16>) -> bool {
    match class {
        ContentClass::Html => true,
        ContentClass::Unknown | ContentClass::Other => {
            status.is_some_and(|s| !(200..300).contains(&s))
        }
        _ => false,
    }
}

fn class_name(class: ContentClass) -> String {
    class.as_str().to_ascii_lowercase()
}

fn truncate(text: &str, max: usize) -> String {
    match text.char_indices().nth(max) {
        Some((byte, _)) => text[..byte].to_string(),
        None => text.to_string(),
    }
}

fn collapse(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Zusammengefasster Text unter einem Knoten des Dokumentbaums (als Makro, weil die Engine die Knotentypen von
/// `scraper` nicht benennt).
macro_rules! text_of {
    ($node:expr) => {{
        let mut s = String::new();
        for d in $node.descendants() {
            if let Some(t) = d.value().as_text() {
                let t: &str = t;
                s.push_str(t);
                s.push(' ');
            }
        }
        collapse(&s)
    }};
}

/// Bildkontext je `<img>` der Seite in Dokumentreihenfolge.
fn image_contexts(doc: &os_parse::dom::Document, base: &Url) -> Vec<ImageAiContext> {
    let html = doc.html();
    let mut out = Vec::new();
    for node in html.root_element().descendants() {
        let Some(el) = node.value().as_element() else {
            continue;
        };
        if el.name() != "img" {
            continue;
        }
        let src = el.attr("src").unwrap_or_default();
        let resolved = base
            .join(src)
            .map(|u| u.to_string())
            .unwrap_or_else(|_| src.to_string());
        // 1. Bildunterschrift der umgebenden `figure`.
        let figure = node
            .ancestors()
            .find(|a| a.value().as_element().is_some_and(|e| e.name() == "figure"));
        let caption = figure.and_then(|f| {
            f.descendants()
                .find(|d| {
                    d.value()
                        .as_element()
                        .is_some_and(|e| e.name() == "figcaption")
                })
                .map(|n| text_of!(n))
        });
        // 2. Sonst Text des Elternelements.
        let context = caption.filter(|c| !c.is_empty()).or_else(|| {
            node.parent()
                .filter(|p| {
                    p.value()
                        .as_element()
                        .is_some_and(|e| !matches!(e.name(), "body" | "html"))
                })
                .map(|n| text_of!(n))
                .filter(|c| !c.is_empty())
        });
        out.push(ImageAiContext {
            src: resolved,
            alt: el.attr("alt").map(str::to_string),
            context: context.map(|c| truncate(&c, IMAGE_CONTEXT_MAX_CHARS)),
        });
    }
    out
}

/// Typen der strukturierten Daten in Dokumentreihenfolge (Elemente ohne übergeordnetes Element), ohne Doppelte.
fn structured_types(page: &ParsedPage) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for item in &page.structured_data {
        if item.parse_error.is_some() || item.type_path.is_empty() || item.type_path.contains('/') {
            continue;
        }
        if !out.contains(&item.type_path) {
            out.push(item.type_path.clone());
        }
    }
    out
}

/// Eingabefelder einer geparsten HTML-Seite (`html` = dekodierter Quelltext, aus dem `page` geparst wurde).
#[allow(clippy::too_many_arguments)]
pub fn from_page(
    url_id: i64,
    url: &Url,
    status: u16,
    indexable: bool,
    html: &str,
    page: &ParsedPage,
    pcfg: &ParseConfig,
    sd_enabled: bool,
) -> PageAiFields {
    let doc = os_parse::dom::Document::parse(html);
    let base = Url::parse(&page.base_url).unwrap_or_else(|_| url.clone());
    let page_text = os_parse::content::area_text(&doc, &pcfg.content_area);
    let address = url.as_str();
    PageAiFields {
        url_id,
        url: address.to_string(),
        content_class: "html".into(),
        status_code: Some(status),
        indexable,
        title: page.titles.first().map(|t| t.text.clone()),
        meta_description: page.meta_descriptions.first().map(|t| t.text.clone()),
        h1: page.h1.iter().map(|t| t.text.clone()).collect(),
        h2: page.h2.iter().map(|t| t.text.clone()).collect(),
        page_text: (!page_text.is_empty()).then_some(page_text),
        word_count: page.word_count.map(u64::from),
        html_lang: os_parse::spelling::lang::declared_html(&doc),
        hreflang_self: page
            .hreflang
            .iter()
            .filter(|h| {
                h.resolved
                    .as_deref()
                    .is_some_and(|r| os_url::eq_for_compare(r, address))
            })
            .map(|h| h.code_raw.clone())
            .collect(),
        structured_data_types: sd_enabled.then(|| structured_types(page)),
        images: image_contexts(&doc, &base),
        source: InputSource::Parsed,
    }
}

/// Eine URL aus `urls` (Grundmenge G).
struct UrlRow {
    id: i64,
    address: String,
    status: Option<u16>,
    class: ContentClass,
    indexable: bool,
    title: Option<String>,
    meta: Option<String>,
    h1: Vec<String>,
    word_count: Option<u64>,
}

fn url_rows(file: &CrawlFile) -> Result<Vec<UrlRow>, StoreError> {
    let mut stmt = file.conn().prepare(
        // Ressourcen gehören nicht zu G: robots.txt sowie URLs, auf die nur Ressourcenverweise zeigen (Bild, CSS,
        // JavaScript, Medien) – wie `Purpose::Resource` während des Crawls.
        "SELECT u.id, u.address, u.status_code, u.content_class, u.indexability, u.title_1, u.meta_desc_1, u.h1_1, \
         u.h1_2, u.word_count FROM urls u WHERE u.is_internal = 1 AND u.status_code IS NOT NULL \
         AND u.discovered_via <> 'robots_txt' \
         AND NOT (EXISTS (SELECT 1 FROM links l WHERE l.target_id = u.id AND l.link_type IN ('Image','CSS','JavaScript','Media')) \
              AND NOT EXISTS (SELECT 1 FROM links l WHERE l.target_id = u.id \
                              AND l.link_type NOT IN ('Image','CSS','JavaScript','Media'))) \
         AND NOT EXISTS (SELECT 1 FROM image_instances i WHERE i.image_id = u.id) \
         ORDER BY u.id",
    )?;
    let rows = stmt
        .query_map([], |r| {
            let class: String = r.get(3)?;
            let h1: Vec<String> = [
                r.get::<_, Option<String>>(7)?,
                r.get::<_, Option<String>>(8)?,
            ]
            .into_iter()
            .flatten()
            .collect();
            Ok(UrlRow {
                id: r.get(0)?,
                address: r.get(1)?,
                status: r
                    .get::<_, Option<i64>>(2)?
                    .and_then(|s| u16::try_from(s).ok()),
                class: class.parse().unwrap_or_default(),
                indexable: r.get::<_, Option<String>>(4)?.as_deref()
                    == Some(Indexability::Indexable.as_str()),
                title: r.get(5)?,
                meta: r.get(6)?,
                h1,
                word_count: r
                    .get::<_, Option<i64>>(9)?
                    .and_then(|w| u64::try_from(w).ok()),
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows
        .into_iter()
        .filter(|r| in_base_set(r.class, r.status))
        .collect())
}

fn hreflang_self(file: &CrawlFile) -> Result<BTreeMap<i64, Vec<String>>, StoreError> {
    let mut stmt = file.conn().prepare(
        "SELECT h.url_id, h.code_raw, h.url, h.target_id, u.address FROM hreflang h JOIN urls u ON u.id = h.url_id \
         ORDER BY h.url_id, h.source, h.position",
    )?;
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Option<String>>(2)?,
                r.get::<_, Option<i64>>(3)?,
                r.get::<_, String>(4)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let mut out: BTreeMap<i64, Vec<String>> = BTreeMap::new();
    for (id, code, url, target, address) in rows {
        let own = target == Some(id)
            || url
                .as_deref()
                .is_some_and(|u| os_url::eq_for_compare(u, &address));
        if own {
            out.entry(id).or_default().push(code);
        }
    }
    Ok(out)
}

/// Eingabefelder aller Seiten-URLs der Grundmenge G einer Crawl-Datei (Reihenfolge nach `urls.id`).
pub fn load_pages(path: &Path, cfg: &CrawlConfig) -> Result<Vec<PageAiFields>, StoreError> {
    let file = CrawlFile::open(path, OpenMode::ReadOnly)?;
    let stored = os_store::ai::inputs::read_all(file.conn())?;
    let hreflang = hreflang_self(&file)?;
    let pcfg = ParseConfig::from_crawl_config(cfg);
    let sd_enabled = structured_data_enabled(cfg);
    let mut out = Vec::new();
    for row in url_rows(&file)? {
        let mut fields = PageAiFields {
            url_id: row.id,
            url: row.address.clone(),
            content_class: class_name(row.class),
            status_code: row.status,
            indexable: row.indexable,
            title: row.title,
            meta_description: row.meta,
            h1: row.h1,
            word_count: row.word_count,
            hreflang_self: hreflang.get(&row.id).cloned().unwrap_or_default(),
            source: InputSource::Unavailable,
            ..PageAiFields::default()
        };
        let html_200 = row.class == ContentClass::Html && row.status == Some(200);
        if let Some(s) = stored.get(&row.id) {
            // (1) Gespeicherte KI-Eingaben.
            fields.page_text = s.page_text.clone();
            fields.h2 = s.h2.clone();
            fields.html_lang = s.html_lang.clone();
            fields.structured_data_types = s.structured_data_types.clone();
            fields.images = s
                .images
                .iter()
                .map(|i| ImageAiContext {
                    src: i.src.clone(),
                    alt: i.alt.clone(),
                    context: i.context.clone(),
                })
                .collect();
            fields.source = InputSource::Stored;
        } else if html_200 {
            // (2) Aus gespeichertem HTML abgeleitet (nichts wird nachgespeichert).
            if let Some(html) = os_store::queries::stored_html(&file, os_model::UrlId(row.id))? {
                if let Ok(url) = Url::parse(&row.address) {
                    let content_type = html
                        .charset
                        .as_deref()
                        .map(|c| format!("text/html; charset={c}"));
                    let page = os_parse::parse_html(&ParseInput {
                        status: 200,
                        url: &url,
                        body: &html.bytes,
                        content_type: content_type.as_deref(),
                        headers: &[],
                        cfg: &pcfg,
                    });
                    let (text, _) =
                        os_parse::charset::decode(&html.bytes, content_type.as_deref(), &[]);
                    let derived = from_page(
                        row.id,
                        &url,
                        200,
                        row.indexable,
                        &text,
                        &page,
                        &pcfg,
                        sd_enabled,
                    );
                    fields.page_text = derived.page_text;
                    fields.h2 = derived.h2;
                    fields.html_lang = derived.html_lang;
                    fields.structured_data_types = derived.structured_data_types;
                    fields.images = derived.images;
                    fields.source = InputSource::Derived;
                }
            }
        }
        out.push(fields);
    }
    Ok(out)
}

/// [`load_pages`] als Leser der Warteschlange (`os_model::ai::hook::PageLoader`).
pub fn page_loader() -> os_model::ai::hook::PageLoader {
    std::sync::Arc::new(|path: &Path, cfg: &CrawlConfig| {
        load_pages(path, cfg).map_err(|e| e.to_string())
    })
}
