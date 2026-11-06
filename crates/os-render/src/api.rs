//! Verträge des Chromium-Renderers. Die Engine übergibt Richtlinien und Antworten;
//! dieses Crate steuert Browser, begrenzte Erfassung und Wiederherstellung.
use std::{collections::BTreeMap, path::PathBuf, sync::Arc};

use os_model::settings::NetworkSettings;
use os_model::{
    BlockedResource, ConsoleEntry, ImageRenderFacts, NetworkEntry, RenderTimings, ScreenshotTile,
};

#[path = "browser.rs"]
mod browser;
#[path = "capture.rs"]
mod capture;
#[path = "intercept.rs"]
mod intercept;
#[path = "memory.rs"]
mod memory;
#[path = "page.rs"]
mod page;
#[path = "trust.rs"]
mod trust;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RenderError {
    #[error("nicht implementiert: {0}")]
    NotImplemented(&'static str),
    #[error(
        "err-chromium-missing: Chromium fehlt unter {path}. Chromium unter Konfiguration › Spider › Rendering herunterladen, Chrome/Chromium installieren oder OPENSPIDER_CHROMIUM bzw. rendering.chromium_path setzen."
    )]
    ChromiumMissing { path: String },
    #[error("Chromium nicht startbar: {0}")]
    Launch(String),
    #[error("Zeitüberschreitung beim Rendern: {0}")]
    Timeout(String),
    #[error("Browser abgestürzt: {0}")]
    Crashed(String),
    #[error("Proxy-Authentifizierung fehlgeschlagen (407)")]
    ProxyAuthentication,
    #[error("Renderfehler: {0}")]
    Protocol(String),
}

/// Zugangsdaten stammen aus dem SecretStore des Aufrufers und werden hier nicht gespeichert.
#[derive(Clone, PartialEq, Eq)]
pub struct Credentials {
    pub username: String,
    pub password: String,
}
impl std::fmt::Debug for Credentials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Credentials([redacted])")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchOpts {
    /// MCP gateway mode: disable network paths that bypass the mandatory proxy.
    pub protected_network: bool,
    pub chromium_path: Option<PathBuf>,
    pub headless: bool,
    pub user_data_dir: Option<PathBuf>,
    /// E05 bestimmt den Standard aus RAM, Threads und Profil (K2-36); mindestens ein Tab.
    pub max_tabs: usize,
    /// Null deaktiviert den jeweiligen Neustartauslöser.
    pub restart_after_pages: u64,
    pub memory_limit_mb: u64,
    pub network: NetworkSettings,
    pub proxy_credentials: Option<Credentials>,
    /// Chromium-Regeln aus der Host-Zuordnung des Crawls.
    pub host_resolver_rules: Vec<String>,
    /// Block native cookie storage in the renderer's fresh, temporary profile.
    pub disable_cookie_storage: bool,
    /// Engine requests supply language explicitly, with no browser default header.
    pub suppress_default_accept_language: bool,
    pub launch_timeout_ms: u32,
    /// Explizite Freigabe für Container ohne Chromium-Sandbox.
    pub no_sandbox: bool,
}
impl Default for LaunchOpts {
    fn default() -> Self {
        Self {
            protected_network: false,
            chromium_path: None,
            headless: true,
            user_data_dir: None,
            max_tabs: 1,
            restart_after_pages: 500,
            memory_limit_mb: 1500,
            network: NetworkSettings::default(),
            proxy_credentials: None,
            host_resolver_rules: Vec::new(),
            disable_cookie_storage: false,
            suppress_default_accept_language: false,
            // Kaltstart auf langsamen Rechnern: 20 s reichten auf frischen Windows-Runnern nicht (Trockenlauf beta.3).
            launch_timeout_ms: 60_000,
            no_sandbox: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Device {
    GooglebotMobile,
    GooglebotDesktop,
    Fixed,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScreenshotFormat {
    Png,
    Jpeg,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScreenshotOpts {
    pub size: Option<(u32, u32)>,
    pub fit_content: bool,
    pub resize_wait_ms: u32,
    pub format: ScreenshotFormat,
    pub jpeg_quality: u8,
}
impl Default for ScreenshotOpts {
    fn default() -> Self {
        Self {
            size: None,
            fit_content: false,
            resize_wait_ms: 300,
            format: ScreenshotFormat::Png,
            jpeg_quality: 80,
        }
    }
}

/// Bereits von der Engine abgerufene Antwort, über CDP an der Original-URL eingespielt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MainResponse {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    /// Dekodierter Inhalt; Content-Encoding/Length werden beim Einspielen entfernt.
    pub body: Vec<u8>,
}
/// robots-Richtlinie der Engine einschließlich eigener Regeln und Ignorieren-Schalter.
/// Bei false gelangt die Anfrage nicht ins Netz.
pub trait ResourcePolicy: Send + Sync + std::fmt::Debug {
    /// Typed engine policy; existing standalone implementations keep their behavior.
    fn allowed_resource<'a>(
        &'a self,
        url: &'a str,
        _resource_type: &'a str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = bool> + Send + 'a>> {
        self.allowed(url)
    }

    /// Reason for denying a resource; standalone policies retain their robots default.
    fn blocked_reason<'a>(
        &'a self,
        url: &'a str,
        resource_type: &'a str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Option<&'static str>> + Send + 'a>>
    {
        Box::pin(async move {
            if self.allowed_resource(url, resource_type).await {
                None
            } else {
                Some("robots")
            }
        })
    }

    fn allowed<'a>(
        &'a self,
        url: &'a str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = bool> + Send + 'a>>;
}

/// Cookie der Engine; ohne Bindung an Chromium/CDP. Ohne Domain gilt die Render-URL.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Cookie {
    pub name: String,
    pub value: String,
    pub domain: Option<String>,
    pub path: Option<String>,
    pub secure: bool,
    pub http_only: bool,
    /// Unix-Zeit in Sekunden; None bezeichnet ein Sitzungscookie.
    pub expires: Option<i64>,
}

/// Renderoptionen enthalten eine dynamische Ressourcenrichtlinie und implementieren
/// deshalb bewusst kein PartialEq.
#[derive(Debug, Clone)]
pub struct RenderOpts {
    /// Custom JavaScript, already gated by response status, scope and MIME type.
    pub snippets: crate::snippets::RunOptions,
    /// Run the bundled accessibility audit in this render pass.
    pub accessibility: bool,
    /// Opt in to at most two retries after a navigation destroys the DOM context.
    /// False preserves existing callers, including custom request identities.
    pub retry_document_context: bool,
    /// Origin-scoped storage restored before scripts run (CRAWL-120).
    pub login_storage: Vec<crate::login::OriginStorage>,
    /// Engine opt-in for JavaScript cookie provenance; standalone rendering is unchanged.
    pub capture_cookie_assignments: bool,
    /// Disable all browser cookie writes and outgoing Cookie headers.
    pub disable_cookies: bool,
    /// Return all context cookies, including HTTP-only and off-page resource paths.
    pub return_cookies: bool,
    /// HTTP language preference; applied before document scripts execute.
    pub accept_language: Option<String>,
    /// Optional URL-scoped identity; absent preserves standalone renderer behavior.
    pub request_identity: Option<Arc<dyn RequestIdentity>>,
    pub viewport: (u32, u32),
    pub ajax_timeout_ms: u32,
    pub screenshots: bool,
    pub user_agent: Option<String>,
    pub device: Device,
    pub device_scale_factor: f64,
    pub mobile: bool,
    pub touch: bool,
    pub response_timeout_ms: u32,
    /// Harte Grenze auch bei blockierter JS-Engine. Null leitet sie aus Laden,
    /// AJAX und Screenshot sowie fünf Sekunden für die Extraktion ab.
    pub page_timeout_ms: u32,
    pub screenshot: ScreenshotOpts,
    pub js_error_reporting: bool,
    pub headers: BTreeMap<String, String>,
    pub cookies: Vec<Cookie>,
    pub credentials: Option<Credentials>,
    pub resource_policy: Option<Arc<dyn ResourcePolicy>>,
    pub main_response: Option<MainResponse>,
}
impl Default for RenderOpts {
    fn default() -> Self {
        Self {
            snippets: crate::snippets::RunOptions::default(),
            accessibility: false,
            retry_document_context: false,
            login_storage: Vec::new(),
            capture_cookie_assignments: false,
            disable_cookies: false,
            return_cookies: false,
            accept_language: None,
            request_identity: None,
            viewport: (411, 731),
            ajax_timeout_ms: 5_000,
            screenshots: true,
            user_agent: None,
            device: Device::GooglebotMobile,
            device_scale_factor: 1.0,
            mobile: true,
            touch: true,
            response_timeout_ms: 20_000,
            page_timeout_ms: 0,
            screenshot: ScreenshotOpts::default(),
            js_error_reporting: true,
            headers: BTreeMap::new(),
            cookies: Vec::new(),
            credentials: None,
            resource_policy: None,
            main_response: None,
        }
    }
}
impl RenderOpts {
    pub fn googlebot_desktop() -> Self {
        Self {
            viewport: (1024, 768),
            device: Device::GooglebotDesktop,
            mobile: false,
            touch: false,
            ..Self::default()
        }
    }
    pub fn fixed_viewport(width: u32, height: u32) -> Self {
        Self {
            viewport: (width, height),
            device: Device::Fixed,
            mobile: false,
            touch: false,
            ..Self::default()
        }
    }
    fn validate(&self) -> Result<(), RenderError> {
        if self.viewport.0 == 0
            || self.viewport.1 == 0
            || self.viewport.0 > 16_384
            || self.viewport.1 > 16_384
            || !self.device_scale_factor.is_finite()
            || !(0.1..=4.0).contains(&self.device_scale_factor)
            || self.ajax_timeout_ms > 60_000
            || !self.ajax_timeout_ms.is_multiple_of(100)
            || self.response_timeout_ms == 0
            || self.response_timeout_ms > 3_600_000
            || self.screenshot.resize_wait_ms > 10_000
            || self.screenshot.jpeg_quality > 100
            || self
                .screenshot
                .size
                .is_some_and(|(w, h)| w == 0 || h == 0 || w > 16_384 || h > 16_384)
        {
            return Err(RenderError::Protocol(
                "Ungültige Rendering-Einstellungen".into(),
            ));
        }
        Ok(())
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RenderedPage {
    /// Ordered snippet values and execution trace, including empty/error results.
    pub snippets: Vec<crate::snippets::SnippetResult>,
    /// Complete axe result; absent when accessibility extraction is disabled.
    pub accessibility: Option<String>,
    /// Body-only transfer sizes; excludes HTTP headers (CRAWL-022).
    pub body_network: Option<Vec<NetworkEntry>>,
    /// JavaScript cookie assignments, retained separately from HTTP Set-Cookie.
    pub cookie_assignments: Vec<String>,
    /// Complete isolated context snapshot. None preserves injected-renderer behavior.
    pub cookies: Option<Vec<Cookie>>,
    /// URL of the loaded document, unaffected by history.pushState.
    pub navigation_url: Option<String>,
    pub dom_html: String,
    pub console: Vec<ConsoleEntry>,
    pub console_dropped: u64,
    pub blocked: Vec<BlockedResource>,
    pub network: Vec<NetworkEntry>,
    pub screenshot_tiles: Vec<ScreenshotTile>,
    pub timings: RenderTimings,
    /// Kein load-Ereignis; das bis dahin verfügbare DOM wird dennoch zurückgegeben.
    pub load_timed_out: bool,
    pub final_url: String,
    /// DOM-Bildreihenfolge einschließlich gleicher URLs mit verschiedenen Anzeigegrößen.
    pub images: Vec<(String, ImageRenderFacts)>,
    /// Computed CSS image references, including stylesheet and pseudo-element rules (ONPAGE-178).
    pub resource_images: Vec<os_model::ImageInstance>,
    /// Icons and social image metadata are separate from content image instances (ONPAGE-170).
    pub non_content_image_urls: Vec<String>,
}

pub use browser::Renderer;

fn protocol(error: impl std::fmt::Display) -> RenderError {
    RenderError::Protocol(error.to_string())
}

/// Verifies availability before creating a crawl file.
pub fn check_available(opts: &LaunchOpts) -> Result<(), RenderError> {
    browser::executable(opts)?;
    let opts = opts.clone();
    // This synchronous API is also called inside Tokio. A separate bounded runtime
    // verifies an actual CDP session without nesting runtimes or invoking a shell.
    std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| RenderError::Launch(e.to_string()))?;
        let result = runtime.block_on(async {
            tokio::time::timeout(
                std::time::Duration::from_millis(u64::from(opts.launch_timeout_ms) + 3_000),
                async {
                    let renderer = Renderer::launch(opts).await?;
                    renderer.close().await;
                    Ok(())
                },
            )
            .await
            .map_err(|_| RenderError::Launch("Startprüfung überschreitet Zeitgrenze".into()))?
        });
        runtime.shutdown_timeout(std::time::Duration::from_secs(1));
        result
    })
    .join()
    .map_err(|_| RenderError::Launch("Startprüfung abgebrochen".into()))?
}

/// Scoped request headers. Cookies/auth are resolved separately for each URL.
pub trait RequestIdentity: Send + Sync + std::fmt::Debug {
    /// Retry an authorized GET challenge through the scoped HTTP transport.
    /// Native Chromium origin-auth caching would bypass path-prefix boundaries.
    fn auth_response<'a>(
        &'a self,
        _url: &'a str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Option<MainResponse>> + Send + 'a>>
    {
        Box::pin(async { None })
    }
    fn headers(&self, url: &str) -> BTreeMap<String, String>;
}

/// All cookies of a dedicated login browser, including HTTP-only cookies.
pub(crate) async fn login_cookies(
    ws: &str,
    page: &chromiumoxide::Page,
) -> Result<Vec<chromiumoxide::cdp::browser_protocol::network::Cookie>, RenderError> {
    intercept::cookies(ws, page.target_id()).await
}

/// Reuses the renderer's proxy-only challenge handling for interactive navigation.
pub(crate) async fn login_proxy(
    ws: &str,
    page: &chromiumoxide::Page,
    url: &str,
    launch: &LaunchOpts,
) -> Result<impl Send + Sync, RenderError> {
    intercept::Intercept::start(
        ws,
        page,
        url,
        &RenderOpts::default(),
        launch,
        Default::default(),
    )
    .await
}

#[cfg(test)]
mod document_read_tests {
    use super::*;

    // Ein langsamer Kaltstart darf nicht als „Chromium nicht startbar“ gelten.
    // spec: CRAWL-078
    #[test]
    fn default_launch_timeout_allows_a_slow_cold_start() {
        assert_eq!(LaunchOpts::default().launch_timeout_ms, 60_000);
    }

    #[derive(Debug)]
    struct Identity;
    impl RequestIdentity for Identity {
        fn headers(&self, _: &str) -> BTreeMap<String, String> {
            BTreeMap::new()
        }
    }

    // spec: CRAWL-016
    // spec: CRAWL-078
    #[tokio::test(start_paused = true)]
    async fn engine_context_retry_recovers_is_bounded_and_preserves_other_errors_and_defaults() {
        for (retry, engine, message, recover, expected) in [
            (true, true, "Cannot find context with specified id", true, 2),
            (true, true, "Execution context was destroyed", false, 3),
            (true, true, "Permission denied", false, 1),
            (
                false,
                true,
                "Cannot find context with specified id",
                true,
                1,
            ),
            (
                false,
                false,
                "Cannot find context with specified id",
                true,
                1,
            ),
        ] {
            let opts = RenderOpts {
                retry_document_context: retry,
                request_identity: engine.then(|| Arc::new(Identity) as Arc<dyn RequestIdentity>),
                ..Default::default()
            };
            let attempts = std::cell::Cell::new(0);
            let result = page::read_document(&opts, || {
                let n = attempts.get();
                attempts.set(n + 1);
                std::future::ready(if recover && n > 0 {
                    Ok("document")
                } else {
                    Err(RenderError::Protocol(message.into()))
                })
            })
            .await;
            assert_eq!(attempts.get(), expected);
            if retry && recover {
                assert_eq!(result.unwrap(), "document");
            } else {
                assert_eq!(result.unwrap_err(), RenderError::Protocol(message.into()));
            }
        }
    }
}
