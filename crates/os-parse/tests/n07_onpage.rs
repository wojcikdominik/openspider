use os_model::{CrawlConfig, ImageSource, RenderingMode};
use os_parse::{ParseConfig, ParseInput};

fn parse(cfg: &CrawlConfig) -> os_model::ParsedPage {
    let url = url::Url::parse("https://site.test/").unwrap();
    let mut cfg = ParseConfig::from_crawl_config(cfg);
    cfg.pixel = true;
    os_parse::parse_html(&ParseInput {
        status: 200,
        url: &url,
        body: include_bytes!("../../../tests/fixtures/n07-onpage/inline.html"),
        content_type: Some("text/html"),
        headers: &[],
        cfg: &cfg,
    })
}

// spec: CRAWL-039 [M2]
// spec: CRAWL-040
// spec: ONPAGE-178
#[test]
fn css_discovery_and_image_analysis_are_independent() {
    for discovery in [false, true] {
        for analysis in [false, true] {
            let mut cfg = CrawlConfig::default();
            cfg.spider.css_style_attributes = discovery;
            cfg.images.parse_inline_css_backgrounds = analysis;
            let page = parse(&cfg);
            assert!(page.links.iter().any(|l| l.href_raw == "block.png"));
            assert_eq!(
                page.links.iter().any(|l| l.href_raw == "attribute.png"),
                discovery
            );
            let css: Vec<_> = page
                .images
                .iter()
                .filter(|i| i.source == ImageSource::Css)
                .map(|i| i.src_raw.as_str())
                .collect();
            assert_eq!(
                css,
                if analysis {
                    vec!["block.png", "attribute.png"]
                } else {
                    vec![]
                }
            );
        }
    }
    assert!(!CrawlConfig::default().spider.css_style_attributes);
    let mut cfg = CrawlConfig::default();
    cfg.rendering.mode = RenderingMode::JavaScript;
    assert!(parse(&cfg)
        .links
        .iter()
        .any(|l| l.href_raw == "attribute.png"));
    let text = cfg.to_toml_string().unwrap();
    assert_eq!(
        CrawlConfig::from_toml_str(&text)
            .unwrap()
            .spider
            .css_style_attributes,
        cfg.spider.css_style_attributes
    );
}

// spec: ONPAGE-090
// spec: UI-038
#[test]
fn sigma_calibration_is_specific_to_regular_title_profile() {
    let page = parse(&CrawlConfig::default());
    assert_eq!(page.titles[0].px, Some(33));
    for c in ["Σ", "σ", "ς"] {
        assert_eq!(os_parse::pixel::width_px(c, 20.0, false), 11);
        assert_eq!(os_parse::pixel::width_px(&c.repeat(3), 20.0, false), 33);
    }
    // The 14 px profile and bold/custom sizes retain their own font metrics.
    assert_eq!(os_parse::pixel::width_px("Σ", 14.0, false), 9);
    assert_eq!(os_parse::pixel::width_px("ς", 14.0, false), 7);
    assert_eq!(os_parse::pixel::width_px("Σ", 30.0, false), 19);
}

// spec: INTEG-081
// spec: OUT-031 [P1]
#[test]
fn aside_precedes_sidebar_and_link_positions_keep_public_values() {
    let page = parse(&CrawlConfig::default());
    for (href, expected) in [
        ("/aside", "Aside"),
        ("/nested", "Aside"),
        ("/sidebar", "Sidebar"),
        ("/navigation", "Navigation"),
    ] {
        let link = page.links.iter().find(|l| l.href_raw == href).unwrap();
        assert_eq!(link.position.as_str(), expected);
    }
}
