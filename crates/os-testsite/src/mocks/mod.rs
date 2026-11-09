//! Mock-Server für die API-Quellen von M2 (04 A, E – J; INTEG-019): Google OAuth, GA4, Search Console, PageSpeed,
//! Majestic, Ahrefs, Moz und Google Drive. In CI laufen nur Mocks, nie echte API-Aufrufe (ARCHITECTURE.md, KI & externe Daten).
//!
//! Aufbau: Jedes Modul `mocks/<name>.rs` meldet seinen Mock über `mock()`; [`registry`] sammelt sie in fester Reihenfolge.
//! [`MockServer::start`] hängt die Mocks unter `/<name>` an **einen** Listener (Loopback, ephemerer Port) und
//! liefert die Überschreibungen der Basis-URLs (`api.<quelle>.base_url`, INTEG-019), mit denen ein Test die
//! Konfiguration auf den Mock lenkt. Der SMTP-Mock (`smtp`) ist kein HTTP-Dienst und hat einen eigenen Einstieg.
//!
//! M2-F02 legt Gerüst und Registrierung an; die Mocks selbst liefern die Pakete I01 (`oauth`, `common`), I02 (`ga4`),
//! I03 (`gsc`), I04 (`psi`), I05 (`majestic`, `ahrefs`, `moz`), X03 (`drive`) und T02 (`smtp`).

use std::io;
use std::net::SocketAddr;
use std::sync::Arc;

use tokio::sync::oneshot;

pub mod ahrefs;
pub mod ai;
/// Chromium-Archiv mit Range-Unterstützung (M3-K01); nicht in der Registry, das Archiv erzeugt der Test.
pub mod chromium;
pub mod common;
pub mod drive;
pub mod ga4;
pub mod gsc;
pub mod majestic;
pub mod moz;
pub mod oauth;
pub mod psi;
pub mod smtp;
pub mod update;

/// Ein HTTP-Mock einer API-Quelle.
pub trait MockApi: Send + Sync {
    /// Name; der Mock ist unter dem Pfad `/<name>` erreichbar (`ga4`, `gsc`, `psi`, `oauth`, …).
    fn name(&self) -> &'static str;

    /// Konfigurationsschlüssel der Basis-URL, den der Mock überschreibt (`api.ga4.base_url`); `None`, wenn der Mock
    /// keine eigene Basis-URL hat (z. B. der OAuth-Mock, dessen Endpunkte die anderen Mocks nennen).
    fn base_url_key(&self) -> Option<&'static str>;

    /// Die Routen des Mocks (relativ zu `/<name>`).
    fn router(&self) -> axum::Router;

    /// INTEG-019: GET-Pfad für den quellenübergreifenden X-Scenario-Vertrag.
    /// I02–I05 passen ihn bei der Mock-Registrierung an ihren Prüfendpunkt an.
    fn scenario_path(&self) -> &'static str {
        ""
    }
}

/// Alle Mocks, die bereits eine Umsetzung haben, in fester Reihenfolge.
pub fn registry() -> Vec<Arc<dyn MockApi>> {
    let mut mocks: Vec<Arc<dyn MockApi>> = [
        oauth::mock(),
        ga4::mock(),
        gsc::mock(),
        psi::mock(),
        majestic::mock(),
        ahrefs::mock(),
        moz::mock(),
        drive::mock(),
        update::mock(),
    ]
    .into_iter()
    .flatten()
    .collect();
    mocks.extend(
        ai::AiMock::fixtures()
            .expect("valid AI answer fragments")
            .registrations(),
    );
    mocks
}

/// Laufender Mock-Server: ein Listener auf `127.0.0.1`, die Mocks unter `/<name>`.
pub struct MockServer {
    addr: SocketAddr,
    mocks: Vec<Arc<dyn MockApi>>,
    shutdown: Option<oneshot::Sender<()>>,
    task: Option<tokio::task::JoinHandle<()>>,
}

impl MockServer {
    /// Startet alle Mocks der [`registry`].
    pub async fn start_all() -> io::Result<MockServer> {
        MockServer::start(registry()).await
    }

    /// Startet die genannten Mocks auf einem ephemeren Port.
    pub async fn start(mocks: Vec<Arc<dyn MockApi>>) -> io::Result<MockServer> {
        let mut app = axum::Router::new();
        for mock in &mocks {
            app = app.nest(&format!("/{}", mock.name()), mock.router());
        }
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0)).await?;
        let addr = listener.local_addr()?;
        let (tx, rx) = oneshot::channel::<()>();
        let task = tokio::spawn(async move {
            let _ = axum::serve(listener, app)
                .with_graceful_shutdown(async {
                    let _ = rx.await;
                })
                .await;
        });
        Ok(MockServer {
            addr,
            mocks,
            shutdown: Some(tx),
            task: Some(task),
        })
    }

    /// Adresse des Listeners.
    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    /// Basis-URL eines Mocks (`http://127.0.0.1:<port>/<name>`).
    pub fn base_url(&self, name: &str) -> String {
        format!("http://{}/{name}", self.addr)
    }

    /// Überschreibungen der Basis-URLs als `(Konfigurationsschlüssel, URL)` (INTEG-019).
    pub fn config_overrides(&self) -> Vec<(String, String)> {
        self.mocks
            .iter()
            .filter_map(|m| {
                m.base_url_key()
                    .map(|key| (key.to_string(), self.base_url(m.name())))
            })
            .collect()
    }

    /// Hält den Server an und wartet auf sein Ende.
    pub async fn stop(mut self) {
        if let Some(tx) = self.shutdown.take() {
            let _ = tx.send(());
        }
        if let Some(task) = self.task.take() {
            let _ = task.await;
        }
    }
}

impl Drop for MockServer {
    fn drop(&mut self) {
        if let Some(tx) = self.shutdown.take() {
            let _ = tx.send(());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::routing::get;

    struct Hello;

    impl MockApi for Hello {
        fn name(&self) -> &'static str {
            "hello"
        }

        fn base_url_key(&self) -> Option<&'static str> {
            Some("api.hello.base_url")
        }

        fn router(&self) -> axum::Router {
            axum::Router::new().route("/ping", get(|| async { "pong" }))
        }
    }

    async fn get_text(addr: SocketAddr, path: &str) -> String {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let mut s = tokio::net::TcpStream::connect(addr).await.unwrap();
        s.write_all(format!("GET {path} HTTP/1.0\r\nHost: x\r\n\r\n").as_bytes())
            .await
            .unwrap();
        let mut out = String::new();
        s.read_to_string(&mut out).await.unwrap();
        out
    }

    // Infrastruktur (M2-F02): Mocks hängen unter `/<name>`, die Basis-URL-Überschreibung (INTEG-019) nennt ihre URL.
    #[tokio::test]
    async fn mocks_are_mounted_under_their_name_and_override_the_base_url() {
        let server = MockServer::start(vec![Arc::new(Hello)]).await.unwrap();
        let base = server.base_url("hello");
        assert!(base.starts_with("http://127.0.0.1:"));
        assert!(base.ends_with("/hello"));
        assert_eq!(
            server.config_overrides(),
            vec![("api.hello.base_url".to_string(), base)]
        );
        let reply = get_text(server.addr(), "/hello/ping").await;
        assert!(
            reply.starts_with("HTTP/1.0 200") || reply.starts_with("HTTP/1.1 200"),
            "{reply}"
        );
        assert!(reply.ends_with("pong"), "{reply}");
        let missing = get_text(server.addr(), "/ping").await;
        assert!(missing.contains(" 404"), "{missing}");
        server.stop().await;
    }

    // Infrastruktur (M2-F02): Namen der registrierten Mocks sind eindeutig, Basis-URL-Schlüssel stehen unter `api.`.
    #[test]
    fn registered_mocks_have_unique_names_and_api_base_url_keys() {
        let mocks = registry();
        let mut names: Vec<&str> = mocks.iter().map(|m| m.name()).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), mocks.len());
        for m in &mocks {
            if let Some(key) = m.base_url_key() {
                assert!(
                    key.starts_with("api.") && key.ends_with(".base_url"),
                    "{key}"
                );
            }
        }
    }
}

/// Standalone form-authentication fixture; existing scenarios are unchanged.
pub mod form_login;
