//! Tests zu CRAWL-001 (Start-URL) und CRAWL-003 (Normalisierung) mit den Vektoren der Spec
//! (Fixtures `crawl-start-url` und `crawl-normalize`).

use os_url::{normalize, parse_start_url, resolve, NormUrl};
use url::Url;

fn start(input: &str) -> NormUrl {
    parse_start_url(input).unwrap_or_else(|e| panic!("{input:?} sollte gültig sein: {e}"))
}

fn link(input: &str) -> NormUrl {
    normalize(&Url::parse(input).unwrap())
}

// spec: CRAWL-001
#[test]
fn start_url_trims_whitespace_and_adds_scheme_and_root_path() {
    assert_eq!(start("site.test").encoded, "http://site.test/");
    assert_eq!(start("  HTTPS://SITE.test  ").encoded, "https://site.test/");
    assert_eq!(
        start("\t site.test/a b \n").encoded,
        "http://site.test/a%20b"
    );
    // Eingabe ohne Pfad erhält `/`
    assert_eq!(start("https://site.test").encoded, "https://site.test/");
    // Port ohne Schema ist kein Schema
    assert_eq!(start("localhost:8080/x").encoded, "http://localhost:8080/x");
    assert_eq!(start("127.0.0.1:3000").encoded, "http://127.0.0.1:3000/");
    assert_eq!(start("[::1]:8080/a").encoded, "http://[::1]:8080/a");
}

// spec: CRAWL-001
#[test]
fn start_url_lowercases_scheme_and_host_but_keeps_path() {
    assert_eq!(start("HTTPS://Site.TEST/a").encoded, "https://site.test/a");
    assert_eq!(
        start("https://site.test/Ab/C?Q=X").encoded,
        "https://site.test/Ab/C?Q=X"
    );
}

// spec: CRAWL-001
#[test]
fn start_url_rejects_invalid_input() {
    for bad in [
        "",
        "   ",
        "htp:/x",
        "http://",
        "http:///x",
        "https://exa mple.test/",
        "site .test",
        "ftp://site.test/",
        "ftp:site.test",
        "mailto:a@b.test",
        "javascript:alert(1)",
        "kein url",
        "http://[bad",
    ] {
        assert!(
            parse_start_url(bad).is_err(),
            "{bad:?} muss abgewiesen werden"
        );
    }
}

// spec: CRAWL-001
#[test]
fn start_url_idn_is_punycode_in_key_and_unicode_in_display() {
    let n = start("müller.test");
    assert_eq!(n.encoded, "http://xn--mller-kva.test/");
    assert_eq!(n.display, "http://müller.test/");
    assert_eq!(n.origin, "http://xn--mller-kva.test");
}

// spec: CRAWL-001
#[test]
fn start_url_keeps_http_so_that_redirects_are_followed_normally() {
    // Test-Vektor aus `crawl-start-url`: http bleibt http (die Weiterleitung auf https folgt im Crawl).
    assert_eq!(start("http://site.test/").encoded, "http://site.test/");
}

// spec: CRAWL-003
#[test]
fn address_and_encoded_address_for_non_ascii() {
    let n = link("https://site.test/café");
    assert_eq!(n.display, "https://site.test/café");
    assert_eq!(n.encoded, "https://site.test/caf%C3%A9");
    let n = link("https://müller.test/");
    assert_eq!(n.encoded, "https://xn--mller-kva.test/");
    assert_eq!(n.display, "https://müller.test/");
    // Query wird ebenfalls kodiert und angezeigt
    let n = link("https://site.test/s?q=über");
    assert_eq!(n.encoded, "https://site.test/s?q=%C3%BCber");
    assert_eq!(n.display, "https://site.test/s?q=über");
}

// spec: CRAWL-003
#[test]
fn ascii_urls_have_identical_address_and_encoded_address() {
    for u in [
        "https://site.test/",
        "https://site.test/a/b.html?x=1&y=2",
        "http://site.test:8081/a%20b",
        "https://site.test/a?",
        "https://site.test/a//b",
    ] {
        let n = link(u);
        assert_eq!(n.display, n.encoded, "{u}");
    }
}

// spec: CRAWL-003
#[test]
fn display_keeps_ascii_escapes_and_invalid_utf8() {
    // %20 bleibt kodiert (Eindeutigkeit der Anzeige); ungültiges UTF-8 bleibt kodiert.
    assert_eq!(
        link("https://site.test/a b").display,
        "https://site.test/a%20b"
    );
    assert_eq!(
        link("https://site.test/%FF%C3%BC").display,
        "https://site.test/%FFü"
    );
    // Steuerzeichen bleiben kodiert
    assert_eq!(
        link("https://site.test/a%E2%80%AEb").display,
        "https://site.test/a%E2%80%AEb"
    );
}

// spec: CRAWL-003
#[test]
fn scheme_and_host_lowercase_path_and_query_case_kept() {
    assert_eq!(
        link("HTTP://SITE.TEST/Page?Q=A").encoded,
        "http://site.test/Page?Q=A"
    );
    assert_ne!(
        link("https://site.test/Page").encoded,
        link("https://site.test/page").encoded
    );
    assert_ne!(
        link("https://site.test/A").encoded,
        link("https://site.test/a").encoded
    );
}

// spec: CRAWL-003
#[test]
fn default_ports_removed_other_ports_kept() {
    assert_eq!(link("HTTP://SITE.TEST:80/a").encoded, "http://site.test/a");
    assert_eq!(
        link("https://site.test:443/a").encoded,
        "https://site.test/a"
    );
    assert_eq!(
        link("https://site.test:80/a").encoded,
        "https://site.test:80/a"
    );
    assert_eq!(
        link("http://site.test:8081/").encoded,
        "http://site.test:8081/"
    );
    assert_eq!(
        link("http://site.test:8081/").origin,
        "http://site.test:8081"
    );
    assert_eq!(link("HTTP://SITE.TEST:80/a").origin, "http://site.test");
}

// spec: CRAWL-003
#[test]
fn trailing_dot_of_host_is_removed() {
    assert_eq!(link("https://site.test./a").encoded, "https://site.test/a");
    assert_eq!(
        link("https://site.test.:8080/a").encoded,
        "https://site.test:8080/a"
    );
    assert_eq!(start("site.test.").encoded, "http://site.test/");
}

// spec: CRAWL-003
#[test]
fn unreserved_percent_escapes_are_decoded_others_uppercased() {
    assert_eq!(
        link("https://site.test/%7Euser").encoded,
        "https://site.test/~user"
    );
    assert_eq!(
        link("https://site.test/%7euser").encoded,
        "https://site.test/~user"
    );
    assert_eq!(
        link("https://site.test/%41b").encoded,
        "https://site.test/Ab"
    );
    assert_eq!(
        link("https://site.test/a?x=%41%7e").encoded,
        "https://site.test/a?x=A~"
    );
    // Reservierte Zeichen werden nicht dekodiert, nur Hex vergrößert
    assert_eq!(
        link("https://site.test/a%2fb%3a").encoded,
        "https://site.test/a%2Fb%3A"
    );
    assert_eq!(
        link("https://site.test/caf%c3%a9").encoded,
        "https://site.test/caf%C3%A9"
    );
    assert_eq!(
        link("https://site.test/a?b=%3d%26").encoded,
        "https://site.test/a?b=%3D%26"
    );
    // Gleichheit der Schlüssel
    assert_eq!(
        link("https://site.test/%7Euser").encoded,
        link("https://site.test/~user").encoded
    );
    assert_eq!(
        link("https://site.test/caf%C3%A9").encoded,
        link("https://site.test/café").encoded
    );
}

// spec: CRAWL-003
#[test]
fn no_double_encoding() {
    assert_eq!(
        link("https://site.test/a%2520b").encoded,
        "https://site.test/a%2520b"
    );
    assert_eq!(
        link("https://site.test/50%").encoded,
        "https://site.test/50%"
    );
    assert_eq!(
        link("https://site.test/%zz").encoded,
        "https://site.test/%zz"
    );
    let once = link("https://site.test/caf%C3%A9 x");
    let twice = normalize(&Url::parse(&once.encoded).unwrap());
    assert_eq!(once.encoded, twice.encoded);
    assert_eq!(once.display, twice.display);
}

// spec: CRAWL-003
#[test]
fn query_order_empty_query_and_double_slash_are_kept() {
    assert_eq!(
        link("https://site.test/a?b=2&a=1").encoded,
        "https://site.test/a?b=2&a=1"
    );
    assert_ne!(
        link("https://site.test/a?b=2&a=1").encoded,
        link("https://site.test/a?a=1&b=2").encoded
    );
    // leere Query bleibt eigene URL
    assert_eq!(link("https://site.test/a?").encoded, "https://site.test/a?");
    assert_ne!(
        link("https://site.test/a?").encoded,
        link("https://site.test/a").encoded
    );
    // `//` im Pfad wird nicht zusammengefasst (ONPAGE-074)
    assert_eq!(
        link("https://site.test/a//b").encoded,
        "https://site.test/a//b"
    );
    assert_eq!(
        link("http://site.test:81//a").encoded,
        "http://site.test:81//a"
    );
}

// spec: CRAWL-003
#[test]
fn distinct_urls_stay_distinct() {
    assert_ne!(
        link("http://site.test/a").encoded,
        link("https://site.test/a").encoded
    );
    assert_ne!(
        link("https://site.test/ordner").encoded,
        link("https://site.test/ordner/").encoded
    );
}

// spec: CRAWL-003
#[test]
fn fragment_is_removed() {
    assert_eq!(link("https://site.test/a#x").encoded, "https://site.test/a");
    assert_eq!(
        link("https://site.test/a?q=1#x").encoded,
        "https://site.test/a?q=1"
    );
    assert_eq!(
        link("https://site.test/a#x").encoded,
        link("https://site.test/a").encoded
    );
}

// spec: CRAWL-003
#[test]
fn normalization_is_idempotent_for_the_fixture_links() {
    for u in [
        "HTTP://SITE.TEST:80/a",
        "/a",
        "/%7Euser",
        "/~user",
        "/A",
        "/a?",
        "/a?b=2&a=1",
        "/a?a=1&b=2",
        "/ordner",
        "/ordner/",
        "/caf%C3%A9",
        "/café",
    ] {
        let base = Url::parse("https://site.test/").unwrap();
        let abs = resolve(&base, u).unwrap();
        let once = normalize(&abs);
        let twice = normalize(&once.url);
        assert_eq!(once, twice, "{u}");
    }
}

// spec: CRAWL-003
#[test]
fn fixture_crawl_normalize_dedup_groups() {
    // Aus der Fixture: HTTP://SITE.TEST:80/a ≙ /a (relativ zu http://site.test/), /%7Euser ≙ /~user,
    // /caf%C3%A9 ≙ /café; `/A`, `/a?`, die Query-Varianten und `/ordner(/)` sind je eigene URLs.
    let base = Url::parse("http://site.test/").unwrap();
    let key = |h: &str| normalize(&resolve(&base, h).unwrap()).encoded;
    assert_eq!(key("HTTP://SITE.TEST:80/a"), key("/a"));
    assert_eq!(key("/%7Euser"), key("/~user"));
    assert_eq!(key("/caf%C3%A9"), key("/café"));
    let mut distinct = vec![
        key("/a"),
        key("/A"),
        key("/a?"),
        key("/a?b=2&a=1"),
        key("/a?a=1&b=2"),
        key("/ordner"),
        key("/ordner/"),
        key("/~user"),
        key("/café"),
    ];
    distinct.sort();
    distinct.dedup();
    assert_eq!(distinct.len(), 9);
}

// spec: CRAWL-003
// Regression zu RUSTSEC-2026-0592: `Url::origin()` stieg bis url 2.5.8 bei verschachtelten
// `blob:`-URLs je Präfix eine Ebene tiefer; eine solche URL aus fremdem HTML brachte den Prozess
// per Stapelüberlauf zum Absturz. Ab 2.5.9 ergibt ein innerer `blob:`-Wert einen opaken Ursprung.
#[test]
fn deeply_nested_blob_url_normalizes_without_stack_overflow() {
    let nested = format!("{}https://site.test/", "blob:".repeat(100_000));
    let n = link(&nested);
    assert_eq!(n.origin, "null");
    assert_eq!(n.encoded, nested);
}
