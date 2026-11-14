//! „Link-Positionen speichern“ abschaltbar (INTEG-083): Linkzeilen ohne `Link Path`/`Link Position`, kleinere
//! Crawl-Datei, übrige Linkspalten und Zähler unverändert.
use os_engine::testing::{crawl_fixture_from, FixtureCrawl};
use os_model::CrawlConfig;

const SCENARIO: &str = "integ-link-positions";
const START: &str = "/many-links.html";
const LINKS: i64 = 1_000;

async fn crawl(store: bool) -> FixtureCrawl {
    let mut cfg = CrawlConfig::default();
    cfg.speed.threads = 1;
    cfg.link_positions.store = store;
    crawl_fixture_from(SCENARIO, START, cfg).await
}

fn scalar(fx: &FixtureCrawl, sql: &str) -> i64 {
    fx.file.conn().query_row(sql, [], |r| r.get(0)).unwrap()
}

const FROM_START: &str =
    "FROM links WHERE source_id = (SELECT id FROM urls WHERE url LIKE '%/many-links.html')";

/// Belegte Bytes der Crawl-Datei aus Sicht der Verbindung (inkl. noch nicht zurückgeschriebener WAL-Seiten).
fn used_bytes(fx: &FixtureCrawl) -> i64 {
    scalar(fx, "PRAGMA page_count") * scalar(fx, "PRAGMA page_size")
}

// spec: INTEG-083
#[test]
fn link_positions_are_stored_by_default() {
    assert!(CrawlConfig::default().link_positions.store);
}

// spec: INTEG-083
#[tokio::test]
async fn disabled_link_positions_leave_both_columns_empty_and_shrink_the_crawl_file() {
    let on = crawl(true).await;
    let off = crawl(false).await;

    for fx in [&on, &off] {
        assert_eq!(
            scalar(
                fx,
                &format!("SELECT count(*) {FROM_START} AND link_type = 'Hyperlink'")
            ),
            LINKS
        );
        // Ziel, Anker und Zähler hängen nicht an der Option.
        assert_eq!(
            scalar(
                fx,
                &format!(
                    "SELECT count(*) {FROM_START} AND target_id IS NOT NULL AND anchor LIKE '% %'"
                )
            ),
            LINKS
        );
        assert_eq!(
            scalar(
                fx,
                "SELECT c.inlinks FROM url_counters c JOIN urls u ON u.id = c.url_id WHERE u.url LIKE '%/target'"
            ),
            LINKS
        );
    }

    // An: jeder Link hat Pfad und Position; die Standardregeln liefern alle vier Bereiche der Fixture.
    assert_eq!(
        scalar(&on, &format!("SELECT count(*) {FROM_START} AND link_path LIKE '/html[1]/body[1]/%' AND link_position IS NOT NULL")),
        LINKS
    );
    assert_eq!(
        scalar(
            &on,
            &format!("SELECT count(DISTINCT link_position) {FROM_START}")
        ),
        4
    );

    // Aus: beide Spalten leer.
    assert_eq!(
        scalar(&off, &format!("SELECT count(*) {FROM_START} AND (link_path IS NOT NULL OR link_position IS NOT NULL)")),
        0
    );

    // Speicherumfang kleiner als mit Option (Messung auf der 1.000-Link-Fixture).
    let (bytes_on, bytes_off) = (used_bytes(&on), used_bytes(&off));
    let path_bytes = scalar(&on, &format!("SELECT sum(length(link_path)) {FROM_START}"));
    assert!(
        bytes_off < bytes_on,
        "Crawl-Datei ohne Linkpfade ({bytes_off} B) nicht kleiner als mit ({bytes_on} B)"
    );
    assert!(
        path_bytes > 50 * LINKS,
        "Fixture-Pfade zu kurz für eine aussagekräftige Messung: {path_bytes} B"
    );
}
