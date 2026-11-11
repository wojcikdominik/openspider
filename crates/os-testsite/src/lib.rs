//! `os-testsite` – lokaler Fixture-Server für deterministische Tests.
//!
//! Der Server liefert ein Verzeichnis `tests/fixtures/<szenario>/` statisch aus. Optional
//! beschreibt eine `scenario.toml` im Szenario-Verzeichnis programmatische Antworten
//! (Statuscodes, Header, Weiterleitungen, Verzögerungen, Fehlerverhalten):
//!
//! ```toml
//! [[route]]
//! path = "/old-page"
//! status = 301                 # optional; mit `location` ist der Standard 302, sonst 200
//! location = "/about.html"     # setzt den Location-Header
//! body = "..."                 # optional; ohne body wird die statische Datei (falls vorhanden) genutzt
//! content_type = "text/html"   # optional
//! delay_ms = 250               # optional
//! [route.headers]
//! X-Robots-Tag = "noindex"
//! ```
//!
//! # Platzhalter
//!
//! In textuellen statischen Dateien (html, xml, txt, css, js, json) werden ersetzt:
//! `{{BASE_URL}}` (= `http://127.0.0.1:<port>`), `{{PORT}}` (Hauptport) und `{{PORT2}}`
//! (zweiter Listener derselben Site – „anderer Port“ desselben Hosts).
//!
//! # Virtuelle Hosts
//!
//! Der Host eines Requests ist der `Host`-Header (Kleinbuchstaben, ohne Port):
//!
//! * `[[route]] host = "site.test"` bindet eine Route an einen Host (`*.site.test` passt auf
//!   alle Subdomänen); Routen ohne `host` gelten für jeden Host, hostgebundene gewinnen.
//! * Existiert `hosts/<host>/`, liefert dieser Host **nur** Dateien aus diesem Verzeichnis;
//!   alle anderen Hosts (auch `127.0.0.1`, `localhost`) bedienen das Wurzelverzeichnis.
//!
//! Die Site lauscht auf `127.0.0.1` und `::1` (gleicher Port; ohne IPv6-Loopback nur IPv4).
//! `localhost` ist für den Crawler ein *externer* Host (F-HOST2). Hostnamen wie `site.test`
//! löst der Crawler per `network.resolve` (`*.test=127.0.0.1:<port>`) auf.
//!
//! # HTTPS
//!
//! Auf jedem Port werden HTTP und TLS bedient; das erste Byte der Verbindung entscheidet
//! (`0x16` = TLS-Handshake). [`TestSite::ca_cert_pem`] liefert die Test-CA, Host-Zertifikate
//! entstehen bei Bedarf anhand von SNI, `expired.<host>` bekommt ein abgelaufenes Zertifikat
//! (siehe [`tls`]). ALPN: nur `http/1.1`.
//!
//! # Route-Optionen (Fehler- und Randfälle)
//!
//! | Option | Wirkung |
//! |---|---|
//! | `host` | Route nur für diesen Host |
//! | `reason` | eigener Reason-Text der Statuszeile |
//! | `[route.headers]` | Werte als Text **oder Array** (mehrere gleichnamige Header) |
//! | `set_cookie` | Text oder Array: je Eintrag ein `Set-Cookie` |
//! | `no_content_type` | kein `Content-Type`-Header |
//! | `encoding` | `gzip`, `deflate`, `deflate-raw`, `br`, `broken-gzip` (abgeschnittener Strom) |
//! | `chunked` | `Transfer-Encoding: chunked` statt `Content-Length` |
//! | `trickle_bytes_per_s` | Body tröpfelt mit dieser Rate (Tokio-Zeit, mit `start_paused` simulierbar) |
//! | `hang` | Request wird angenommen, aber nie beantwortet |
//! | `abort_after_headers` | Kopf mit `Content-Length`, dann Verbindung schließen |
//! | `reset_mid_body` | halber Body, dann TCP-Reset |
//! | `sequence = [503, 200]` | n-ter Abruf liefert Status `sequence[n]` (danach der letzte) |
//! | `fail_first_n` (+ `fail_status`, Standard 503) | die ersten n Abrufe schlagen fehl |
//! | `echo = "headers"` | Body = JSON der Request-Header inklusive Cookies |
//! | `body_size` | Body exakt so viele Byte (auffüllen mit Leerzeichen / kürzen) |
//! | `png_size` | gültiges PNG mit exakt dieser Byte-Größe |
//! | `requires_chromium` | Markierung für Rendering-Tests, wird nur geparst |
//! | `informational = [103]` | 1xx-Zwischenantworten (nicht 101), die unmittelbar vor der finalen Antwort gesendet werden (nach `delay_ms`); nicht an HTTP/1.0-Clients |
//! | `informational_headers` | Header jeder Zwischenantwort (Text oder Array), z. B. `Link` bei 103 |
//!
//! Zustandsbehaftete Optionen (`sequence`, `fail_first_n`) zählen je Route (nicht je Host/Pfad)
//! und beginnen bei jedem Start der Site neu.
//!
//! **Nicht enthalten:** ein „Blackhole“ für Verbindungs-Timeouts. Ein deterministisches
//! Verhalten (Verbindung bleibt im SYN-Zustand) gibt es nicht plattformübergreifend: nicht
//! geroutete Adressen werden je nach Netz und OS sofort abgewiesen, und ein volles
//! Listen-Backlog verhält sich unter Linux, macOS und Windows verschieden. Tests für
//! Verbindungs-Timeouts nutzen stattdessen `hang` (Antwort-Timeout) bzw. simulierte Zeit.
//! Deshalb entfällt die Route `/hang-connect` der Fixture `crawl-no-response` (Entscheidung K-24);
//! die Berechnung des Verbindungs-Timeouts prüft `os-fetch` gegen einen Mini-Server.
//!
//! # Request-Log
//!
//! [`TestSite::requests`] liefert [`RequestRecord`]s (`seq, ts, conn_id, method, host,
//! path_and_query, headers, tls, sni, sent_bytes`), dazu [`TestSite::count`], [`TestSite::max_concurrency`] und
//! [`TestSite::connections`]. Das detaillierte Log ist bei Dateiszenarien immer an, bei
//! synthetischen Sites nur mit [`SiteOptions::log_requests`] (Last-Tests sollen nicht
//! mitwachsen). Mit `log_requests` zählt der Server zusätzlich Requests je Pfad
//! ([`TestSite::request_counts`], als JSON unter [`synthetic::REQUEST_LOG_PATH`]).
//!
//! * `sni` ist der Servername aus dem TLS-ClientHello der Verbindung (`None` bei Klartext und bei
//!   TLS ohne SNI, etwa per IP-Adresse).
//! * `sent_bytes` sind die Bytes, die der Server für den Request geschrieben hat (1xx-Antworten,
//!   Kopf, Body; bei TLS vor der Verschlüsselung), aufgeschlüsselt in `sent_framing_bytes`
//!   (Chunk-Rahmen bei `chunked`) und `sent_interim_bytes` (1xx-Zwischenantworten);
//!   [`RequestRecord::sent_payload_bytes`] ist der Rest = Kopf + Body der finalen Antwort. Der
//!   Server verbucht die Bytes erst nach dem Schreiben – wer unmittelbar nach dem Lesen der
//!   Antwort nachsieht, wartet kurz, bis `sent_bytes > 0` ist.
//! * `sent_body_bytes` (K-02) sind die gesendeten Nutzdaten des Bodys der finalen Antwort ohne
//!   Statuszeile, Header und Chunk-Rahmen – die Gegenstellung zu `transferred_bytes` des Clients.
//!   0 bei HEAD, 204 und 304.
//!
//! # Verbindungen (CRAWL-151)
//!
//! [`TestSite::connection_stats`] liefert je angenommener TCP-Verbindung Host, Protokoll, Zahl der
//! Requests und die höchste Zahl gleichzeitiger Requests auf ihr; [`TestSite::max_concurrency_for_host`]
//! die Gleichzeitigkeit je Host. Mit `max_connections_per_client = n` auf oberster Ebene der
//! `scenario.toml` nimmt der Server je Client-Adresse höchstens n offene Verbindungen an; jede weitere
//! wird direkt nach der Annahme per RST geschlossen und in [`TestSite::rejected_connections`] gezählt.
//!
//! # Golden-Dateien
//!
//! [`golden::normalize`] und [`golden::assert_golden`] (mit `UPDATE_GOLDEN=1`).
//!
//! # Synthetische Sites
//!
//! Enthält die `scenario.toml` eine Sektion `[synthetic]` (oder wird die Kurzform
//! `synthetic:n=1000000,links=30` übergeben), erzeugt der Server die Site deterministisch
//! on-the-fly aus `(seed, Pfad)` – ohne Dateien auf Platte, siehe [`synthetic`]:
//!
//! ```toml
//! [synthetic]
//! pages = 1000000
//! links_per_page = 30
//! seed = 42
//! depth_distribution = [10, 100, 1000, 10000]  # relative Gewichte je Ebene ab Tiefe 1
//! status_301_share = 0.02    # Anteile an allen Seiten (0..1)
//! status_404_share = 0.01
//! status_5xx_share = 0.005
//! noindex_share = 0.05
//! canonical_other_share = 0.03
//! duplicate_title_share = 0.10
//! large_page_share = 0.01
//! large_page_kb = 200
//! delay_ms = 250             # zusammen mit slow_share
//! slow_share = 0.01
//! ```

pub mod client;
pub mod golden;
mod log;
mod media;
pub mod mocks;
mod respond;
mod server;
pub mod synthetic;
pub mod tls;

use std::collections::{BTreeMap, HashMap, HashSet};
use std::io;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, Mutex, OnceLock};

use serde::Deserialize;
use tokio::net::TcpListener;
use tokio::task::JoinHandle;

pub use log::{ConnectionStat, RequestRecord};
pub use media::{png_min_size, png_of_size, Encoding};
pub use synthetic::{SyntheticConfig, SyntheticSite};

/// Dateiname der optionalen Szenario-Beschreibung.
pub const SCENARIO_FILE: &str = "scenario.toml";

/// Platzhalter, der in textuellen Fixture-Dateien durch die Basis-URL ersetzt wird.
pub const BASE_URL_PLACEHOLDER: &str = "{{BASE_URL}}";

/// Platzhalter für den Hauptport.
pub const PORT_PLACEHOLDER: &str = "{{PORT}}";

/// Platzhalter für den Port des zweiten Listeners.
pub const PORT2_PLACEHOLDER: &str = "{{PORT2}}";

/// Unterverzeichnis mit den Dateien virtueller Hosts (`hosts/<host>/…`).
pub const HOSTS_DIR: &str = "hosts";

/// Header-Wert in einer Route: ein Text oder eine Liste (mehrere gleichnamige Header).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
pub enum HeaderValues {
    /// Ein einzelner Header.
    One(String),
    /// Mehrere Header gleichen Namens, in dieser Reihenfolge.
    Many(Vec<String>),
}

impl HeaderValues {
    /// Alle Werte in Reihenfolge.
    pub fn values(&self) -> Vec<&str> {
        match self {
            HeaderValues::One(v) => vec![v.as_str()],
            HeaderValues::Many(vs) => vs.iter().map(String::as_str).collect(),
        }
    }
}

impl PartialEq<&str> for HeaderValues {
    fn eq(&self, other: &&str) -> bool {
        matches!(self, HeaderValues::One(v) if v == other)
    }
}

/// Wert der Route-Option `echo`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Echo {
    /// Body ist ein JSON-Objekt mit Methode, Host, Pfad, Request-Headern und Cookies.
    Headers,
}

/// Programmatisch definierte Antwort für einen Pfad.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Route {
    /// Exakter URL-Pfad (ohne Query), z. B. `/old-page`.
    pub path: String,
    /// Nur für diesen Host (`site.test`, `*.site.test`); ohne Angabe für jeden Host.
    pub host: Option<String>,
    /// HTTP-Statuscode.
    pub status: Option<u16>,
    /// Eigener Reason-Text der Statuszeile (`HTTP/1.1 200 <reason>`).
    pub reason: Option<String>,
    /// Ziel für den `Location`-Header (Weiterleitung).
    pub location: Option<String>,
    /// Zusätzliche Antwort-Header; ein Array erzeugt mehrere gleichnamige Header.
    #[serde(default)]
    pub headers: BTreeMap<String, HeaderValues>,
    /// Antwort-Body; ohne Angabe wird die statische Datei zum Pfad verwendet.
    pub body: Option<String>,
    /// Wert für den `Content-Type`-Header.
    pub content_type: Option<String>,
    /// Keinen `Content-Type`-Header senden.
    #[serde(default)]
    pub no_content_type: bool,
    /// Künstliche Verzögerung in Millisekunden.
    pub delay_ms: Option<u64>,
    /// Body kodieren und `Content-Encoding` setzen.
    pub encoding: Option<Encoding>,
    /// Body mit `Transfer-Encoding: chunked` senden.
    #[serde(default)]
    pub chunked: bool,
    /// Body mit dieser Rate (Byte pro Sekunde) senden.
    pub trickle_bytes_per_s: Option<u64>,
    /// Request nie beantworten.
    #[serde(default)]
    pub hang: bool,
    /// Halben Body senden, dann TCP-Reset.
    #[serde(default)]
    pub reset_mid_body: bool,
    /// Nur den Kopf senden, dann die Verbindung schließen.
    #[serde(default)]
    pub abort_after_headers: bool,
    /// Status je Abruf (n-ter Abruf = n-ter Eintrag, danach der letzte).
    pub sequence: Option<Vec<u16>>,
    /// Die ersten n Abrufe liefern `fail_status`.
    pub fail_first_n: Option<u32>,
    /// Status für `fail_first_n` (Standard 503).
    pub fail_status: Option<u16>,
    /// Body als JSON der Request-Header ausgeben.
    pub echo: Option<Echo>,
    /// Body exakt so groß (Byte).
    pub body_size: Option<usize>,
    /// Gültiges PNG exakt dieser Größe (Byte).
    pub png_size: Option<usize>,
    /// `Set-Cookie`-Header (Text oder Liste).
    pub set_cookie: Option<HeaderValues>,
    /// 1xx-Zwischenantworten (z. B. `[103]`), die der Server unmittelbar vor der finalen Antwort
    /// sendet; erlaubt sind 100..=199 außer 101. Wird nicht an HTTP/1.0-Clients gesendet.
    #[serde(default)]
    pub informational: Vec<u16>,
    /// Header, die jede Zwischenantwort aus `informational` trägt (z. B. `Link` bei 103);
    /// Werte als Text oder Array wie bei `headers`. Nur zusammen mit `informational` erlaubt.
    #[serde(default)]
    pub informational_headers: BTreeMap<String, HeaderValues>,
    /// Route braucht einen Rendering-Browser; wird nur geparst, der Server ignoriert es.
    #[serde(default)]
    pub requires_chromium: bool,
}

/// Inhalt einer `scenario.toml`.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Scenario {
    /// Alle `[[route]]`-Einträge.
    #[serde(default, rename = "route")]
    pub routes: Vec<Route>,
    /// Optionale synthetische Site (`[synthetic]`); ersetzt die statischen Dateien.
    pub synthetic: Option<SyntheticConfig>,
    /// Höchstzahl gleichzeitig offener TCP-Verbindungen je Client-Adresse (CRAWL-151); weitere Verbindungen
    /// werden sofort nach der Annahme hart geschlossen (RST) und in [`TestSite::rejected_connections`] gezählt.
    pub max_connections_per_client: Option<usize>,
}

impl Scenario {
    /// Parst eine `scenario.toml`.
    pub fn parse(text: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(text)
    }

    /// Prüft die Routen auf unmögliche Kombinationen (Fehlertext nennt Pfad und Option).
    pub fn validate(&self) -> Result<(), String> {
        for r in &self.routes {
            let at = |what: &str| format!("Route {:?}: {what}", r.path);
            if !r.path.starts_with('/') {
                return Err(at("path muss mit / beginnen"));
            }
            let bad_status = |s: u16| !(100..=999).contains(&s);
            if r.status.is_some_and(bad_status) || r.fail_status.is_some_and(bad_status) {
                return Err(at("Statuscode außerhalb 100..=999"));
            }
            if let Some(seq) = &r.sequence {
                if seq.is_empty() || seq.iter().copied().any(bad_status) {
                    return Err(at("sequence braucht Statuscodes 100..=999"));
                }
            }
            if let Some(n) = r.png_size {
                if n < media::png_min_size() {
                    return Err(at(&format!(
                        "png_size {n} ist kleiner als das Minimum {}",
                        media::png_min_size()
                    )));
                }
            }
            if r.trickle_bytes_per_s == Some(0) {
                return Err(at("trickle_bytes_per_s muss > 0 sein"));
            }
            if r.informational
                .iter()
                .any(|c| !(100..=199).contains(c) || *c == 101)
            {
                return Err(at(
                    "informational braucht Statuscodes 100..=199 außer 101 (Protokollwechsel)",
                ));
            }
            if r.informational.is_empty() && !r.informational_headers.is_empty() {
                return Err(at("informational_headers braucht informational"));
            }
        }
        Ok(())
    }
}

/// Wurzelverzeichnis der Fixtures (`<workspace>/tests/fixtures`).
pub fn fixtures_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures")
}

/// Optionen beim Start eines Servers.
#[derive(Debug, Clone, Default)]
pub struct SiteOptions {
    /// Port; `0` wählt einen freien Port.
    pub port: u16,
    /// Requests je Pfad zählen (und bei synthetischen Sites das detaillierte Log führen).
    pub log_requests: bool,
}

/// Verbindungsgrenzen aus der `scenario.toml`.
#[derive(Debug, Clone, Copy, Default)]
struct Limits {
    max_connections_per_client: Option<usize>,
}

pub(crate) struct AppState {
    pub(crate) dir: PathBuf,
    pub(crate) base_url: String,
    pub(crate) port: u16,
    pub(crate) port2: u16,
    pub(crate) routes: Vec<Route>,
    route_index: HashMap<String, Vec<usize>>,
    pub(crate) route_hits: Vec<AtomicU64>,
    pub(crate) host_dirs: HashSet<String>,
    pub(crate) synthetic: Option<SyntheticSite>,
    pub(crate) counts: Option<Mutex<HashMap<String, u64>>>,
    pub(crate) detailed_log: bool,
    pub(crate) log: log::LogState,
    tls: OnceLock<Result<tls::TlsMaterial, String>>,
    /// Grenze offener Verbindungen je Client-Adresse (CRAWL-151).
    pub(crate) max_connections_per_client: Option<usize>,
    /// Offene Verbindungen je Client-Adresse.
    pub(crate) client_conns: Mutex<HashMap<std::net::IpAddr, usize>>,
}

impl AppState {
    pub(crate) fn tls(&self) -> io::Result<&tls::TlsMaterial> {
        self.tls
            .get_or_init(|| tls::TlsMaterial::new().map_err(|e| e.to_string()))
            .as_ref()
            .map_err(|e| io::Error::other(e.clone()))
    }

    /// Passende Route für Host und Pfad: hostgleiche vor Wildcard-Hosts vor hostlosen.
    pub(crate) fn find_route(&self, host: &str, path: &str) -> Option<usize> {
        let candidates = self.route_index.get(path)?;
        let rank = |idx: usize| match self.routes[idx].host.as_deref() {
            Some(h) if h == host => Some(0),
            Some(h)
                if h.starts_with("*.") && host.ends_with(&h[1..]) && host.len() > h.len() - 1 =>
            {
                Some(1)
            }
            Some(_) => None,
            None => Some(2),
        };
        candidates
            .iter()
            .copied()
            .filter_map(|i| rank(i).map(|r| (r, i)))
            .min()
            .map(|(_, i)| i)
    }
}

/// Laufender Testserver. Wird beim Drop beendet.
pub struct TestSite {
    addr: SocketAddr,
    addr2: SocketAddr,
    addr6: Option<SocketAddr>,
    tasks: Vec<JoinHandle<()>>,
    state: Arc<AppState>,
}

impl TestSite {
    /// Startet das Szenario `tests/fixtures/<name>` auf einem zufälligen freien Port.
    pub async fn start(name: &str) -> io::Result<Self> {
        Self::start_dir(fixtures_root().join(name), 0).await
    }

    /// Startet einen Server für ein beliebiges Verzeichnis; `port = 0` wählt einen freien Port.
    pub async fn start_dir(dir: impl Into<PathBuf>, port: u16) -> io::Result<Self> {
        Self::start_dir_with(
            dir,
            SiteOptions {
                port,
                ..Default::default()
            },
        )
        .await
    }

    /// Wie [`TestSite::start_dir`], aber mit [`SiteOptions`].
    pub async fn start_dir_with(dir: impl Into<PathBuf>, opts: SiteOptions) -> io::Result<Self> {
        let dir: PathBuf = dir.into();
        if !dir.is_dir() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("Szenario-Verzeichnis nicht gefunden: {}", dir.display()),
            ));
        }
        let scenario = match std::fs::read_to_string(dir.join(SCENARIO_FILE)) {
            Ok(text) => Scenario::parse(&text).map_err(|e| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("{}: {e}", dir.join(SCENARIO_FILE).display()),
                )
            })?,
            Err(e) if e.kind() == io::ErrorKind::NotFound => Scenario::default(),
            Err(e) => return Err(e),
        };
        scenario
            .validate()
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

        let synthetic = scenario
            .synthetic
            .map(SyntheticSite::new)
            .transpose()
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        let host_dirs = list_host_dirs(&dir);
        let limits = Limits {
            max_connections_per_client: scenario.max_connections_per_client,
        };
        Self::launch(dir, scenario.routes, host_dirs, synthetic, opts, limits).await
    }

    /// Startet eine synthetische Site ohne Dateien auf Platte.
    pub async fn start_synthetic(cfg: SyntheticConfig, opts: SiteOptions) -> io::Result<Self> {
        let site =
            SyntheticSite::new(cfg).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        Self::launch(
            PathBuf::new(),
            Vec::new(),
            HashSet::new(),
            Some(site),
            opts,
            Limits::default(),
        )
        .await
    }

    /// Startet anhand einer Szenario-Angabe: `synthetic:n=1000,links=30` (Kurzform) oder
    /// der Name eines Fixture-Verzeichnisses unter `tests/fixtures`.
    pub async fn start_spec(spec: &str, opts: SiteOptions) -> io::Result<Self> {
        if spec.starts_with("synthetic:") {
            let cfg = SyntheticConfig::parse_short(spec)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
            Self::start_synthetic(cfg, opts).await
        } else {
            Self::start_dir_with(fixtures_root().join(spec), opts).await
        }
    }

    async fn launch(
        dir: PathBuf,
        mut routes: Vec<Route>,
        host_dirs: HashSet<String>,
        synthetic: Option<SyntheticSite>,
        opts: SiteOptions,
        limits: Limits,
    ) -> io::Result<Self> {
        let listener = TcpListener::bind(("127.0.0.1", opts.port)).await?;
        let addr = listener.local_addr()?;
        let listener2 = TcpListener::bind(("127.0.0.1", 0)).await?;
        let addr2 = listener2.local_addr()?;
        // IPv6-Loopback auf denselben Ports; fehlt IPv6, bleibt es bei IPv4.
        let listener6 = TcpListener::bind(("::1", addr.port())).await.ok();
        let listener6_2 = TcpListener::bind(("::1", addr2.port())).await.ok();
        let addr6 = listener6.as_ref().and_then(|l| l.local_addr().ok());

        let mut route_index: HashMap<String, Vec<usize>> = HashMap::new();
        for (i, r) in routes.iter_mut().enumerate() {
            r.host = r.host.take().map(|h| h.to_ascii_lowercase());
            route_index.entry(r.path.clone()).or_default().push(i);
        }
        let route_hits = routes.iter().map(|_| AtomicU64::new(0)).collect();
        let detailed_log = opts.log_requests || synthetic.is_none();
        let state = Arc::new(AppState {
            dir,
            base_url: format!("http://{addr}"),
            port: addr.port(),
            port2: addr2.port(),
            routes,
            route_index,
            route_hits,
            host_dirs,
            synthetic,
            counts: opts.log_requests.then(|| Mutex::new(HashMap::new())),
            detailed_log,
            log: log::LogState::default(),
            tls: OnceLock::new(),
            max_connections_per_client: limits.max_connections_per_client,
            client_conns: Mutex::new(HashMap::new()),
        });

        let tasks = [Some(listener), Some(listener2), listener6, listener6_2]
            .into_iter()
            .flatten()
            .map(|l| tokio::spawn(server::accept_loop(l, Arc::clone(&state))))
            .collect();

        Ok(Self {
            addr,
            addr2,
            addr6,
            tasks,
            state,
        })
    }

    /// Socket-Adresse des Hauptlisteners (IPv4).
    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    /// Adresse des zweiten Listeners (IPv4, anderer Port).
    pub fn addr2(&self) -> SocketAddr {
        self.addr2
    }

    /// Adresse des IPv6-Listeners (`[::1]:<port>`, gleicher Port wie [`TestSite::addr`]),
    /// `None`, wenn das System kein IPv6-Loopback hat.
    pub fn addr6(&self) -> Option<SocketAddr> {
        self.addr6
    }

    /// Hauptport (`{{PORT}}`).
    pub fn port(&self) -> u16 {
        self.addr.port()
    }

    /// Port des zweiten Listeners (`{{PORT2}}`).
    pub fn port2(&self) -> u16 {
        self.addr2.port()
    }

    /// Basis-URL ohne abschließenden Slash, z. B. `http://127.0.0.1:34567`.
    pub fn base_url(&self) -> String {
        format!("http://{}", self.addr)
    }

    /// Basis-URL des zweiten Listeners.
    pub fn base_url2(&self) -> String {
        format!("http://{}", self.addr2)
    }

    /// Basis-URL über TLS auf dem Hauptport (`https://127.0.0.1:<port>`).
    pub fn https_base_url(&self) -> String {
        format!("https://{}", self.addr)
    }

    /// Vollständige URL für einen Pfad (`/index.html`).
    pub fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url(), path)
    }

    /// Alle Basis-URLs in HTTP-, HTTPS- und IPv6-Form beider Ports, zum Übergeben an
    /// [`golden::normalize`].
    pub fn golden_bases(&self) -> Vec<String> {
        let mut bases = Vec::new();
        for port in [self.port(), self.port2()] {
            bases.push(format!("http://127.0.0.1:{port}"));
            bases.push(format!("https://127.0.0.1:{port}"));
            bases.push(format!("http://[::1]:{port}"));
            bases.push(format!("https://[::1]:{port}"));
        }
        bases
    }

    /// Test-CA dieser Site als PEM (für `extra_root_certs` bzw. TLS-Clients).
    ///
    /// # Panics
    /// Wenn die Zertifikatserzeugung scheitert (kommt in der Praxis nicht vor).
    pub fn ca_cert_pem(&self) -> String {
        self.state
            .tls()
            .expect("Test-CA erzeugen")
            .ca_pem()
            .to_string()
    }

    /// Test-CA im DER-Format.
    ///
    /// # Panics
    /// Wie [`TestSite::ca_cert_pem`].
    pub fn ca_cert_der(&self) -> Vec<u8> {
        self.state
            .tls()
            .expect("Test-CA erzeugen")
            .ca_der()
            .to_vec()
    }

    /// TLS-Client-Konfiguration, die nur der Test-CA dieser Site vertraut.
    pub fn tls_client_config(&self) -> io::Result<Arc<rustls::ClientConfig>> {
        tls::client_config(self.state.tls()?.ca_der())
    }

    /// Die synthetische Site, falls dieses Szenario eine ist.
    pub fn synthetic(&self) -> Option<&SyntheticSite> {
        self.state.synthetic.as_ref()
    }

    /// Requests je Pfad (leer, wenn das Logging nicht aktiviert wurde).
    pub fn request_counts(&self) -> BTreeMap<String, u64> {
        self.state
            .counts
            .as_ref()
            .map(|m| lock(m).iter().map(|(k, v)| (k.clone(), *v)).collect())
            .unwrap_or_default()
    }

    /// Gesamtzahl geloggter Requests.
    pub fn total_requests(&self) -> u64 {
        self.state
            .counts
            .as_ref()
            .map_or(0, |m| lock(m).values().sum())
    }

    /// Pfade, die mehr als einmal abgerufen wurden (Duplikat-Abrufe), mit Anzahl.
    pub fn duplicate_requests(&self) -> Vec<(String, u64)> {
        self.request_counts()
            .into_iter()
            .filter(|(_, c)| *c > 1)
            .collect()
    }

    /// Alle geloggten Requests in Eingangsreihenfolge (siehe Modul-Doku zu den Bedingungen).
    pub fn requests(&self) -> Vec<RequestRecord> {
        self.state.log.requests()
    }

    /// Anzahl der Requests für `host` (ohne Port, Groß-/Kleinschreibung egal; `""` = Request
    /// ohne Host-Header) und `path`. Enthält `path` kein `?`, wird nur der Pfadteil verglichen
    /// (Query ignoriert), sonst `path_and_query` vollständig.
    pub fn count(&self, host: &str, path: &str) -> usize {
        let host = host.to_ascii_lowercase();
        let with_query = path.contains('?');
        self.requests()
            .iter()
            .filter(|r| {
                r.host == host
                    && if with_query {
                        r.path_and_query == path
                    } else {
                        r.path() == path
                    }
            })
            .count()
    }

    /// Höchste Zahl gleichzeitig bearbeiteter Requests seit Start bzw. Zurücksetzen.
    pub fn max_concurrency(&self) -> usize {
        self.state.log.max_concurrency()
    }

    /// Zahl der angenommenen TCP-Verbindungen seit Start bzw. Zurücksetzen (Keep-Alive
    /// führt mehrere Requests über dieselbe Verbindung).
    pub fn connections(&self) -> usize {
        usize::try_from(self.state.log.connections()).unwrap_or(usize::MAX)
    }

    /// Angenommene TCP-Verbindungen seit Start bzw. Zurücksetzen mit Host (SNI bzw. `Host` des ersten Requests),
    /// Protokoll, Zahl der Requests und höchster Zahl gleichzeitiger Requests auf der Verbindung (CRAWL-151).
    pub fn connection_stats(&self) -> Vec<ConnectionStat> {
        self.state.log.connection_stats()
    }

    /// Wegen `max_connections_per_client` abgelehnte Verbindungen seit Start bzw. Zurücksetzen.
    pub fn rejected_connections(&self) -> usize {
        self.state.log.rejected()
    }

    /// Höchste Zahl gleichzeitig bearbeiteter Requests an `host` (ohne Port, Kleinbuchstaben).
    pub fn max_concurrency_for_host(&self, host: &str) -> usize {
        self.state
            .log
            .max_concurrency_for_host(&host.to_ascii_lowercase())
    }

    /// Setzt das Request-Log (Pfadzähler, Einträge, Verbindungs- und Gleichzeitigkeitszähler)
    /// zurück. Zustände von `sequence`/`fail_first_n` bleiben erhalten.
    pub fn reset_request_log(&self) {
        if let Some(m) = &self.state.counts {
            lock(m).clear();
        }
        self.state.log.reset();
    }
}

pub(crate) fn lock(
    m: &Mutex<HashMap<String, u64>>,
) -> std::sync::MutexGuard<'_, HashMap<String, u64>> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// Namen (Kleinbuchstaben) der Unterverzeichnisse von `<dir>/hosts/`.
fn list_host_dirs(dir: &Path) -> HashSet<String> {
    std::fs::read_dir(dir.join(HOSTS_DIR))
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().is_dir())
        .filter_map(|e| e.file_name().into_string().ok())
        .map(|n| n.to_ascii_lowercase())
        .collect()
}

impl Drop for TestSite {
    fn drop(&mut self) {
        for t in &self.tasks {
            t.abort();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_scenario_toml() {
        let scenario = Scenario::parse(
            r#"
            [[route]]
            path = "/old"
            status = 301
            location = "/new"
            [route.headers]
            X-Test = "1"
            "#,
        )
        .unwrap();
        assert_eq!(scenario.routes.len(), 1);
        assert_eq!(scenario.routes[0].status, Some(301));
        assert_eq!(scenario.routes[0].headers["X-Test"], "1");
    }

    #[test]
    fn parses_all_route_options() {
        // spec: TECH-007
        let scenario = Scenario::parse(
            r#"
            [[route]]
            path = "/x"
            host = "Site.Test"
            reason = "Fancy"
            set_cookie = ["a=1", "b=2"]
            no_content_type = true
            encoding = "deflate-raw"
            chunked = true
            trickle_bytes_per_s = 100
            hang = false
            reset_mid_body = false
            abort_after_headers = false
            sequence = [503, 200]
            fail_first_n = 2
            echo = "headers"
            body_size = 10
            png_size = 500
            requires_chromium = true
            [route.headers]
            Link = ["<a>; rel=x", "<b>; rel=y"]
            "#,
        )
        .unwrap();
        let r = &scenario.routes[0];
        assert_eq!(r.encoding, Some(Encoding::DeflateRaw));
        assert_eq!(r.echo, Some(Echo::Headers));
        assert_eq!(r.headers["Link"].values().len(), 2);
        assert_eq!(r.set_cookie.as_ref().unwrap().values(), ["a=1", "b=2"]);
        assert!(r.requires_chromium && r.chunked && r.no_content_type);
        scenario.validate().unwrap();
    }

    #[test]
    fn validate_rejects_impossible_routes() {
        let bad = |toml: &str| Scenario::parse(toml).unwrap().validate().is_err();
        assert!(bad("[[route]]\npath = \"x\""));
        assert!(bad("[[route]]\npath = \"/x\"\nsequence = []"));
        assert!(bad("[[route]]\npath = \"/x\"\npng_size = 5"));
        assert!(bad("[[route]]\npath = \"/x\"\ntrickle_bytes_per_s = 0"));
        assert!(Scenario::parse("[[route]]\npath = \"/x\"\nencoding = \"zstd\"").is_err());
    }
}
