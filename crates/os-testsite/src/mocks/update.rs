//! Update fixtures and a logging CONNECT proxy; all sockets use loopback ports.
//! HTTP transport is provided by MockServer; HTTPS-required clients must reject it.
use super::MockApi;
use axum::{
    body::Body,
    http::{header, StatusCode},
    response::Response,
    routing::get,
    Json, Router,
};
use serde_json::Value;
use std::sync::Arc;

pub struct UpdateMock;
pub fn mock() -> Option<Arc<dyn MockApi>> {
    Some(Arc::new(UpdateMock))
}

fn manifest(version: &str, scheme: &str, signature: &str) -> Value {
    let mut value: Value = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/update/manifest.json"
    ))
    .expect("update fixture JSON");
    value["version"] = version.into();
    for platform in value["platforms"]
        .as_object_mut()
        .expect("platforms")
        .values_mut()
    {
        platform["url"] = format!("{scheme}://updates.test/update/artifact").into();
        platform["signature"] = signature.into();
    }
    value
}
impl MockApi for UpdateMock {
    fn name(&self) -> &'static str {
        "update"
    }
    fn base_url_key(&self) -> Option<&'static str> {
        None
    }
    fn router(&self) -> Router {
        Router::new()
            .route(
                "/valid/latest.json",
                get(|| async {
                    Json(manifest(
                        "1.0.1",
                        "https",
                        include_str!("../../../../tests/fixtures/update/artifact.sig").trim(),
                    ))
                }),
            )
            .route(
                "/tampered/latest.json",
                get(|| async { Json(manifest("9.0.0", "https", "tampered")) }),
            )
            .route(
                "/older/latest.json",
                get(|| async {
                    Json(manifest(
                        "0.0.1",
                        "https",
                        include_str!("../../../../tests/fixtures/update/artifact.sig").trim(),
                    ))
                }),
            )
            .route(
                "/http/latest.json",
                get(|| async {
                    Json(manifest(
                        "1.0.1",
                        "http",
                        include_str!("../../../../tests/fixtures/update/artifact.sig").trim(),
                    ))
                }),
            )
            .route("/artifact", get(|| async { "fixture update artifact" }))
            .route(
                "/interrupted",
                get(|| async {
                    let chunks = futures_util::stream::unfold(0, |step| async move {
                        match step {
                            0 => Some((
                                Ok::<_, std::io::Error>(bytes::Bytes::from_static(b"partial")),
                                1,
                            )),
                            1 => {
                                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                                Some((
                                    Err(std::io::Error::new(
                                        std::io::ErrorKind::ConnectionReset,
                                        "fixture interruption",
                                    )),
                                    2,
                                ))
                            }
                            _ => None,
                        }
                    });
                    Response::builder()
                        .status(StatusCode::OK)
                        .header(header::CONTENT_LENGTH, "4096")
                        .body(Body::from_stream(chunks))
                        .unwrap()
                }),
            )
    }
}

/// TLS fixture using TestSite's ephemeral CA and full request log.
pub async fn https_fixture() -> std::io::Result<crate::TestSite> {
    crate::TestSite::start("update").await
}

/// Manifest path of the `stable` channel on the channel fixture (OUT-254).
pub const STABLE_CHANNEL_PATH: &str = "/stable/latest.json";
/// Manifest path of the `beta` channel on the channel fixture (OUT-254).
pub const BETA_CHANNEL_PATH: &str = "/beta/latest-beta.json";

/// Manifest JSON of `version` with the signed fixture artifact (HTTPS URLs), as served on the channel paths.
pub fn channel_manifest(version: &str) -> Value {
    manifest(
        version,
        "https",
        include_str!("../../../../tests/fixtures/update/artifact.sig").trim(),
    )
}

/// OUT-254: HTTPS mock with one manifest path per channel ([`STABLE_CHANNEL_PATH`], [`BETA_CHANNEL_PATH`]), versions
/// set per test (`None` = path answers 404) and a request log per path. Uses TestSite's ephemeral CA.
pub struct ChannelFixture {
    pub site: crate::TestSite,
}
impl ChannelFixture {
    pub async fn start(stable: Option<&str>, beta: Option<&str>) -> std::io::Result<Self> {
        #[derive(serde::Serialize)]
        struct Route {
            path: &'static str,
            body: String,
            content_type: &'static str,
        }
        #[derive(serde::Serialize)]
        struct Scenario {
            route: Vec<Route>,
        }
        let route = [(STABLE_CHANNEL_PATH, stable), (BETA_CHANNEL_PATH, beta)]
            .into_iter()
            .filter_map(|(path, version)| {
                version.map(|v| Route {
                    path,
                    body: channel_manifest(v).to_string(),
                    content_type: "application/json",
                })
            })
            .collect();
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "os-update-channels-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir)?;
        let scenario = toml::to_string(&Scenario { route })
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        std::fs::write(dir.join("scenario.toml"), scenario)?;
        let site = crate::TestSite::start_dir(&dir, 0).await;
        // Routes live in memory; without the directory every other path answers 404.
        let _ = std::fs::remove_dir_all(&dir);
        Ok(Self { site: site? })
    }
    /// HTTPS manifest URL of the `stable` channel.
    pub fn stable_url(&self) -> String {
        format!("{}{STABLE_CHANNEL_PATH}", self.site.https_base_url())
    }
    /// HTTPS manifest URL of the `beta` channel.
    pub fn beta_url(&self) -> String {
        format!("{}{BETA_CHANNEL_PATH}", self.site.https_base_url())
    }
    /// Requests on the `stable` path (query ignored).
    pub fn stable_requests(&self) -> usize {
        self.requests_on(STABLE_CHANNEL_PATH)
    }
    /// Requests on the `beta` path (query ignored).
    pub fn beta_requests(&self) -> usize {
        self.requests_on(BETA_CHANNEL_PATH)
    }
    fn requests_on(&self, path: &str) -> usize {
        self.site
            .requests()
            .iter()
            .filter(|r| r.path() == path)
            .count()
    }
}
/// Loopback-only CONNECT proxy: forwards TLS unchanged, records CONNECT request headers.
pub struct FixtureProxy {
    pub addr: std::net::SocketAddr,
    log: Arc<std::sync::Mutex<Vec<String>>>,
    task: tokio::task::JoinHandle<()>,
}
impl FixtureProxy {
    pub async fn start(target: std::net::SocketAddr) -> std::io::Result<Self> {
        assert!(target.ip().is_loopback());
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0)).await?;
        let addr = listener.local_addr()?;
        let log = Arc::new(std::sync::Mutex::new(Vec::new()));
        let requests = log.clone();
        let task = tokio::spawn(async move {
            use tokio::io::{AsyncReadExt, AsyncWriteExt};
            while let Ok((mut client, _)) = listener.accept().await {
                let requests = requests.clone();
                tokio::spawn(async move {
                    let mut head = Vec::new();
                    while !head.ends_with(b"\r\n\r\n") && head.len() < 16384 {
                        let mut byte = [0];
                        if client.read_exact(&mut byte).await.is_err() {
                            return;
                        }
                        head.push(byte[0]);
                    }
                    requests
                        .lock()
                        .unwrap()
                        .push(String::from_utf8_lossy(&head).into_owned());
                    if !head.starts_with(b"CONNECT ") {
                        return;
                    }
                    let Ok(mut upstream) = tokio::net::TcpStream::connect(target).await else {
                        return;
                    };
                    if client
                        .write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")
                        .await
                        .is_ok()
                    {
                        let _ = tokio::io::copy_bidirectional(&mut client, &mut upstream).await;
                    }
                });
            }
        });
        Ok(Self { addr, log, task })
    }
    pub fn requests(&self) -> Vec<String> {
        self.log.lock().unwrap().clone()
    }
}
impl Drop for FixtureProxy {
    fn drop(&mut self) {
        self.task.abort();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mocks::MockServer;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    async fn request(server: &MockServer, route: &str) -> String {
        let mut stream = tokio::net::TcpStream::connect(server.addr()).await.unwrap();
        stream
            .write_all(
                format!(
                    "GET /update/{route} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n"
                )
                .as_bytes(),
            )
            .await
            .unwrap();
        let mut data = Vec::new();
        stream.read_to_end(&mut data).await.unwrap();
        String::from_utf8(data).unwrap()
    }

    // spec: OUT-254
    #[tokio::test]
    async fn channel_fixture_serves_one_manifest_per_channel_and_logs_each_path() {
        let fixture = ChannelFixture::start(Some("1.0.2"), Some("1.1.0-beta.1"))
            .await
            .unwrap();
        assert!(fixture.stable_url().ends_with("/stable/latest.json"));
        assert!(fixture.beta_url().ends_with("/beta/latest-beta.json"));
        assert!(fixture.stable_url().starts_with("https://"));
        for (path, version) in [
            (STABLE_CHANNEL_PATH, "1.0.2"),
            (BETA_CHANNEL_PATH, "1.1.0-beta.1"),
        ] {
            let mut stream = tokio::net::TcpStream::connect(fixture.site.addr())
                .await
                .unwrap();
            stream
                .write_all(
                    format!("GET {path}?version=1.0.1 HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
                        .as_bytes(),
                )
                .await
                .unwrap();
            let mut data = Vec::new();
            stream.read_to_end(&mut data).await.unwrap();
            let raw = String::from_utf8(data).unwrap();
            assert!(raw.starts_with("HTTP/1.1 200"), "{raw}");
            let body: Value = serde_json::from_str(raw.split_once("\r\n\r\n").unwrap().1).unwrap();
            assert_eq!(body["version"], version);
        }
        assert_eq!((fixture.stable_requests(), fixture.beta_requests()), (1, 1));
        // A channel without a manifest answers 404 and still counts its request.
        let only_stable = ChannelFixture::start(Some("1.0.1"), None).await.unwrap();
        let mut stream = tokio::net::TcpStream::connect(only_stable.site.addr())
            .await
            .unwrap();
        stream
            .write_all(
                format!("GET {BETA_CHANNEL_PATH} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
                    .as_bytes(),
            )
            .await
            .unwrap();
        let mut data = Vec::new();
        stream.read_to_end(&mut data).await.unwrap();
        assert!(String::from_utf8(data).unwrap().starts_with("HTTP/1.1 404"));
        assert_eq!(
            (only_stable.stable_requests(), only_stable.beta_requests()),
            (0, 1)
        );
    }

    // spec: OUT-250 [M3]
    #[tokio::test]
    async fn update_routes_serve_fixture_scenarios() {
        let server = MockServer::start(vec![mock().unwrap()]).await.unwrap();
        for (route, version, scheme, signature) in [
            (
                "valid/latest.json",
                "1.0.1",
                "https",
                include_str!("../../../../tests/fixtures/update/artifact.sig").trim(),
            ),
            ("tampered/latest.json", "9.0.0", "https", "tampered"),
            (
                "older/latest.json",
                "0.0.1",
                "https",
                include_str!("../../../../tests/fixtures/update/artifact.sig").trim(),
            ),
            (
                "http/latest.json",
                "1.0.1",
                "http",
                include_str!("../../../../tests/fixtures/update/artifact.sig").trim(),
            ),
        ] {
            let raw = request(&server, route).await;
            assert!(raw.starts_with("HTTP/1.1 200"));
            let value: Value = serde_json::from_str(raw.split_once("\r\n\r\n").unwrap().1).unwrap();
            assert_eq!(value["version"], version);
            assert_eq!(value["platforms"]["darwin-aarch64"]["signature"], signature);
            assert!(value["platforms"]["darwin-aarch64"]["url"]
                .as_str()
                .unwrap()
                .starts_with(scheme));
        }
        assert!(request(&server, "artifact")
            .await
            .contains("fixture update artifact"));
        let interrupted = request(&server, "interrupted").await;
        let (headers, body) = interrupted.split_once("\r\n\r\n").unwrap();
        assert!(headers.contains("content-length: 4096"));
        assert_eq!(body, "partial");
        server.stop().await;
    }
}
