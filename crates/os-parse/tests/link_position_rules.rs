use os_model::{CrawlConfig, LinkPosition, LinkPositionRule, LinkPositionsConfig};
use os_parse::{dom::Document, link_position, links, ParseConfig};

const PAGE: &str = include_str!("../../../tests/fixtures/integ-link-positions/page.html");
const BREADCRUMB: &str =
    include_str!("../../../tests/fixtures/integ-link-positions/breadcrumb.html");

fn extract(html: &str, rules: LinkPositionsConfig) -> Vec<os_model::ExtractedLink> {
    let doc = Document::parse(html);
    let base = url::Url::parse("https://site.test/").unwrap();
    let mut cfg = ParseConfig::from_crawl_config(&CrawlConfig::default());
    cfg.link_positions = rules;
    links::extract(&doc, &base, &cfg)
}

// spec: INTEG-080
// spec: INTEG-081
#[test]
fn defaults_classify_all_landmarks_in_rule_order() {
    let links = extract(PAGE, LinkPositionsConfig::default());
    let actual: Vec<_> = links
        .iter()
        .map(|link| [link.href_raw.clone(), link.position.to_string()])
        .collect();
    let expected: Vec<[String; 2]> = serde_json::from_str(include_str!(
        "../../../tests/fixtures/integ-link-positions/positions.json"
    ))
    .unwrap();
    assert_eq!(actual, expected);
}

// spec: INTEG-080
#[test]
fn paths_keep_indices_ids_classes_and_xpath_quoting() {
    let links = extract(PAGE, LinkPositionsConfig::default());
    let path = |href| {
        links
            .iter()
            .find(|link| link.href_raw == href)
            .unwrap()
            .xpath
            .as_str()
    };
    assert_eq!(
        path("/sidebar-id"),
        "/html[1]/body[1]/div[1][@id='left-sidebar']/a[1]"
    );
    assert_eq!(
        path("/sidebar-class"),
        "/html[1]/body[1]/div[2][@class='widget sidebar']/a[1]"
    );
    assert_eq!(
        path("/case"),
        "/html[1]/body[1]/div[3][@class='Sidebar']/a[1]"
    );
    assert_eq!(
        path("/quotes"),
        "/html[1]/body[1]/div[4][@id=concat('a',\"'\",'b\"c')][@class='widget']/a[1]"
    );
    assert_eq!(path("/style.css"), "//head/link[1]");
    let second = extract(PAGE, LinkPositionsConfig::default());
    assert_eq!(links, second);
}

// spec: INTEG-082
#[test]
fn custom_rules_support_multiple_substrings_case_and_reordering() {
    let mut rules = LinkPositionsConfig::default();
    rules.rules.insert(
        0,
        LinkPositionRule {
            position: "Breadcrumb".into(),
            contains: vec!["not-present".into(), "breadcrumb".into()],
        },
    );
    let links = extract(BREADCRUMB, rules.clone());
    assert_eq!(
        links
            .iter()
            .map(|l| l.position.as_str())
            .collect::<Vec<_>>(),
        ["Head", "Breadcrumb", "Breadcrumb", "Content", "Breadcrumb"]
    );
    rules.rules.swap(0, 1);
    let reordered = extract(BREADCRUMB, rules.clone());
    assert_eq!(reordered.last().unwrap().position, LinkPosition::Navigation);
    assert_eq!(
        reordered[1].position,
        LinkPosition::Custom("Breadcrumb".into())
    );
    rules.rules[1].contains = vec!["Breadcrumb".into()];
    assert_eq!(
        extract(BREADCRUMB, rules)[1].position,
        LinkPosition::Content
    );
}

// spec: INTEG-081
#[test]
fn defaults_and_config_validation_enforce_the_final_content_rule() {
    let mut cfg = CrawlConfig::default();
    assert_eq!(
        cfg.link_positions
            .rules
            .iter()
            .map(|r| (
                r.position.as_str(),
                r.contains.iter().map(String::as_str).collect::<Vec<_>>()
            ))
            .collect::<Vec<_>>(),
        vec![
            ("Navigation", vec!["nav", "menu"]),
            ("Header", vec!["header"]),
            ("Aside", vec!["aside"]),
            ("Sidebar", vec!["sidebar"]),
            ("Footer", vec!["footer"]),
            ("Content", vec![]),
        ]
    );
    cfg.link_positions.rules.swap(0, 5);
    assert!(!cfg.validate().errors.is_empty());
    cfg.link_positions = LinkPositionsConfig::default();
    assert_eq!(
        extract(PAGE, cfg.link_positions),
        extract(PAGE, LinkPositionsConfig::default())
    );
}

// spec: INTEG-080
// spec: INTEG-081
#[test]
fn empty_substring_is_a_catch_all_and_body_does_not_use_the_nearest_landmark() {
    let rules = LinkPositionsConfig {
        rules: vec![LinkPositionRule {
            position: "Content".into(),
            contains: vec![String::new()],
        }],
        ..Default::default()
    };
    for link in extract(PAGE, rules.clone()) {
        assert_eq!(
            link.position,
            if link.in_head {
                LinkPosition::Head
            } else {
                LinkPosition::Content
            }
        );
    }
    assert_eq!(
        link_position::classify(
            "/html/body/a",
            LinkPosition::Footer,
            &LinkPositionsConfig {
                rules: vec![],
                ..Default::default()
            }
        ),
        LinkPosition::Content
    );
}
