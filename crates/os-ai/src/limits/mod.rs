//! Ratenbegrenzung, Wiederholungen und Timeouts (AI-017).
//!
//! Führend: INTEG-005 (Backoff, Wiederholungen, `Retry-After`, gleichzeitige Anfragen), INTEG-171 (Anfragen je Minute
//! und Modell), INTEG-179 (Request-Timeout). Dazu nach AI-017: Tokens je Minute und Modell auf Basis der geschätzten
//! Eingabe-Tokens, Rate-Limit-Header der Anbieter ([`headers`]) und ein Schutzschalter je Anbieter. Transportfehler
//! ohne HTTP-Antwort sind vorübergehend und enden als `Error: Connection failed` (AI-020, K4-15).
//!
//! Einstieg ist [`Limiter::run`]: ein Eintrag = ein protokollierter Request (AI-014) mit allen Versuchen. Das
//! Ergebnis [`Execution`] liefert Endstatus, Versuchszahl (`Retries`), `HTTP Status`, `Error Code` und Dauer für das
//! Protokoll von M4-K05. Alle Wartezeiten laufen über eine injizierbare [`LimitClock`].
pub mod backoff;
pub mod clock;
pub mod headers;

use std::collections::{BTreeMap, HashMap, VecDeque};
use std::future::Future;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use os_model::ai::AiStatus;
use os_model::config::{AiAppSettings, ProviderEndpoint};

pub use backoff::{
    parse_http_date, parse_retry_after, BackoffPolicy, FixedJitter, JitterSource, RandomJitter,
};
pub use clock::{LimitClock, TokioClock};
pub use headers::{RateLimitSnapshot, TokenRounding};

use crate::provider::{AiProvider, ProviderError, Request, Response};

/// Länge des Fensters für Anfragen und Tokens je Minute.
pub const WINDOW: Duration = Duration::from_secs(60);

/// Ursache eines Transportfehlers ohne HTTP-Antwort (Protokollspalte `Error Code`, AI-020/K4-15).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TransportCause {
    /// Namensauflösung.
    Dns,
    /// Verbindungsaufbau (z. B. abgelehnt).
    Connect,
    /// TLS.
    Tls,
    /// Verbindung ohne Antwort abgebrochen.
    Closed,
}

impl TransportCause {
    /// `transport/dns`, `transport/connect`, `transport/tls` bzw. `transport/closed`.
    pub fn error_code(self) -> &'static str {
        match self {
            Self::Dns => "transport/dns",
            Self::Connect => "transport/connect",
            Self::Tls => "transport/tls",
            Self::Closed => "transport/closed",
        }
    }

    /// Ordnet eine Fehlermeldung (einschließlich ihrer Ursachenkette) einer Ursache zu.
    pub fn classify_message(message: &str) -> Self {
        let m = message.to_ascii_lowercase();
        if m.contains("dns")
            || m.contains("resolve")
            || m.contains("lookup")
            || m.contains("name or service not known")
            || m.contains("nodename nor servname")
        {
            Self::Dns
        } else if m.contains("tls")
            || m.contains("certificate")
            || m.contains("handshake")
            || m.contains("ssl")
        {
            Self::Tls
        } else if m.contains("closed")
            || m.contains("reset")
            || m.contains("broken pipe")
            || m.contains("eof")
            || m.contains("incomplete")
        {
            Self::Closed
        } else if m.contains("connect")
            || m.contains("refused")
            || m.contains("unreachable")
            || m.contains("no route")
        {
            Self::Connect
        } else {
            Self::Closed
        }
    }

    /// Ursache eines `reqwest`-Fehlers ohne HTTP-Antwort; `None` bei Zeitüberschreitung oder vorhandenem Status.
    pub fn from_reqwest(error: &reqwest::Error) -> Option<Self> {
        if error.is_timeout() || error.status().is_some() {
            return None;
        }
        // Nur die Ursachenkette: Die oberste Meldung enthält die URL, deren Text die Zuordnung verfälschen könnte.
        let mut chain = String::new();
        let mut source = std::error::Error::source(error);
        while let Some(s) = source {
            chain.push_str(": ");
            chain.push_str(&s.to_string());
            source = s.source();
        }
        let cause = Self::classify_message(&chain);
        Some(if error.is_connect() && cause == Self::Closed {
            Self::Connect
        } else {
            cause
        })
    }
}

/// Fehlschlag eines einzelnen Versuchs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttemptFailure {
    /// Fehlerklasse des Adapters (AI-001).
    pub error: ProviderError,
    /// Ursache bei Transportfehlern.
    pub cause: Option<TransportCause>,
    /// HTTP-Status der Antwort, falls der Adapter ihn kennt und die Fehlerklasse ihn nicht trägt.
    pub http_status: Option<u16>,
    /// Anbieterspezifischer Fehlercode für die Protokollspalte `Error Code` (z. B. `enforced_spend_limit_reached`).
    pub error_code: Option<String>,
    /// `Retry-After` der Antwort; hat Vorrang vor dem Backoff (INTEG-005).
    pub retry_after: Option<Duration>,
}

impl AttemptFailure {
    /// Transportfehler ohne HTTP-Antwort.
    pub fn transport(cause: TransportCause, message: impl Into<String>) -> Self {
        Self {
            error: ProviderError::Transport {
                message: message.into(),
            },
            cause: Some(cause),
            http_status: None,
            error_code: None,
            retry_after: None,
        }
    }

    /// Aus einem `reqwest`-Fehler ohne Antwort (Transport) bzw. mit Status (HTTP).
    pub fn from_reqwest(error: &reqwest::Error) -> Self {
        if let Some(status) = error.status() {
            return ProviderError::Http {
                status: status.as_u16(),
                message: status.canonical_reason().unwrap_or("").to_string(),
            }
            .into();
        }
        let cause = TransportCause::from_reqwest(error).unwrap_or(TransportCause::Closed);
        Self::transport(cause, error.to_string())
    }

    /// Setzt den Fehlercode für das Protokoll.
    pub fn with_error_code(mut self, code: impl Into<String>) -> Self {
        self.error_code = Some(code.into());
        self
    }

    /// Setzt `Retry-After`.
    pub fn with_retry_after(mut self, retry_after: Duration) -> Self {
        self.retry_after = Some(retry_after);
        self
    }

    /// Ob der Fehler vorübergehend ist und nach INTEG-005 wiederholt wird: 429 (außer den vom Adapter als
    /// `Quota exceeded` erkannten Fällen), 500/502/503/504, 529 (AI-002), Timeout (INTEG-179) und Transportfehler
    /// (K4-15). 400/401/403/404 und alle übrigen Klassen werden nie wiederholt.
    pub fn is_transient(&self) -> bool {
        match &self.error {
            ProviderError::RateLimited { .. }
            | ProviderError::Overloaded
            | ProviderError::Timeout { .. }
            | ProviderError::Transport { .. } => true,
            ProviderError::Http { status, .. } => {
                matches!(status, 429 | 500 | 502 | 503 | 504 | 529)
            }
            _ => false,
        }
    }

    fn status_code(&self) -> Option<u16> {
        match &self.error {
            ProviderError::Http { status, .. } => Some(*status),
            ProviderError::RateLimited { .. } => Some(429),
            ProviderError::Overloaded => Some(529),
            _ => self.http_status,
        }
    }
}

impl From<ProviderError> for AttemptFailure {
    fn from(error: ProviderError) -> Self {
        let cause = match &error {
            ProviderError::Transport { message } => Some(TransportCause::classify_message(message)),
            _ => None,
        };
        let retry_after = match &error {
            ProviderError::RateLimited {
                retry_after_seconds: Some(s),
            } => Some(Duration::from_secs(*s)),
            _ => None,
        };
        Self {
            error,
            cause,
            http_status: None,
            error_code: None,
            retry_after,
        }
    }
}

/// Endgültiger Fehler eines Eintrags nach allen Wiederholungen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FinalError {
    pub error: ProviderError,
    /// Endstatus nach AI-020.
    pub status: AiStatus,
    /// Protokollspalte `HTTP Status` (leer bei Transportfehlern und Timeouts).
    pub http_status: Option<u16>,
    /// Protokollspalte `Error Code`.
    pub error_code: Option<String>,
    /// Meldung für das Protokoll.
    pub message: String,
}

/// Wirkung eines Endergebnisses auf den Schutzschalter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BreakerEffect {
    /// Zählt als aufeinanderfolgender endgültiger Fehler.
    Count,
    /// Setzt die Zählung zurück (Antwort erhalten, auch Refusal).
    Reset,
    /// Weder noch: Kontingent-/Ausgabengrenze (beendet ohnehin alle offenen Einträge des Anbieters, AI-017), fehlender
    /// Schlüssel und Richtlinie (kein Request) sowie Antwortinhalt-Fehler.
    Neutral,
}

impl BreakerEffect {
    /// Wirkung eines endgültigen Fehlers.
    pub fn of(error: &ProviderError) -> Self {
        match error {
            ProviderError::Http { .. }
            | ProviderError::RateLimited { .. }
            | ProviderError::Overloaded
            | ProviderError::Timeout { .. }
            | ProviderError::Transport { .. } => Self::Count,
            ProviderError::Refusal => Self::Reset,
            ProviderError::QuotaExceeded
            | ProviderError::MissingApiKey
            | ProviderError::Policy
            | ProviderError::InvalidOutput
            | ProviderError::MissingAnswer
            | ProviderError::BatchExpired => Self::Neutral,
        }
    }
}

/// Art des Requests (INTEG-179: Bildgenerierung wird nach Timeout höchstens einmal wiederholt).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum RequestKind {
    #[default]
    Text,
    ImageGeneration,
}

/// Beschreibung eines Eintrags für die Begrenzung.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestSpec {
    pub provider: String,
    pub model: String,
    /// Geschätzte Eingabe-Tokens (AI-012) für TPM und Header-Auswertung.
    pub estimated_input_tokens: u64,
    pub kind: RequestKind,
}

impl RequestSpec {
    /// Text-Request.
    pub fn new(provider: impl Into<String>, model: impl Into<String>, tokens: u64) -> Self {
        Self {
            provider: provider.into(),
            model: model.into(),
            estimated_input_tokens: tokens,
            kind: RequestKind::Text,
        }
    }
}

/// Grenzen je Anbieter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProviderLimits {
    /// Gleichzeitige Anfragen (AI-017 Standard 4, Ollama 1).
    pub concurrent_requests: u32,
    /// Request-Timeout (INTEG-179: 120 s, Ollama 300 s).
    pub timeout: Duration,
}

impl Default for ProviderLimits {
    fn default() -> Self {
        Self::from_endpoint(&ProviderEndpoint::default())
    }
}

impl ProviderLimits {
    /// Aus den App-Einstellungen eines Anbieters.
    pub fn from_endpoint(endpoint: &ProviderEndpoint) -> Self {
        Self {
            concurrent_requests: u32::from(endpoint.concurrent_requests.max(1)),
            timeout: Duration::from_secs(u64::from(endpoint.timeout_seconds)),
        }
    }
}

/// Grenzen je Modell; `None` = unbegrenzt. Standardwerte kommen später aus `provider-limits.json` (M4-K04).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ModelLimits {
    /// Anfragen je Minute (INTEG-171).
    pub requests_per_minute: Option<u32>,
    /// Geschätzte Eingabe-Tokens je Minute (AI-017).
    pub tokens_per_minute: Option<u64>,
}

/// Konfiguration der Begrenzung.
#[derive(Debug, Clone, PartialEq)]
pub struct LimitsConfig {
    /// Maximale Wiederholungen (INTEG-005, Standard 5).
    pub max_retries: u32,
    pub backoff: BackoffPolicy,
    /// Schutzschalter an/aus (Standard an).
    pub circuit_breaker: bool,
    /// Aufeinanderfolgende endgültige Fehler bis zur Pause (10).
    pub breaker_threshold: u32,
    /// Pause des Schutzschalters (5 Minuten).
    pub breaker_pause: Duration,
    /// Grenzen je Anbieter; fehlt ein Anbieter, gilt [`LimitsConfig::default_provider`].
    pub providers: BTreeMap<String, ProviderLimits>,
    pub default_provider: ProviderLimits,
    /// Grenzen je (Anbieter, Modell).
    pub models: BTreeMap<(String, String), ModelLimits>,
}

impl Default for LimitsConfig {
    fn default() -> Self {
        Self::from_app_settings(&AiAppSettings::default())
    }
}

impl LimitsConfig {
    /// Standardwerte der Spec mit den Anbieter-Einstellungen aus den App-Einstellungen (Timeout, gleichzeitige
    /// Anfragen).
    pub fn from_app_settings(settings: &AiAppSettings) -> Self {
        Self {
            max_retries: 5,
            backoff: BackoffPolicy::INTEG_005,
            circuit_breaker: true,
            breaker_threshold: 10,
            breaker_pause: Duration::from_secs(300),
            providers: settings
                .provider_endpoints
                .iter()
                .map(|(id, endpoint)| (id.clone(), ProviderLimits::from_endpoint(endpoint)))
                .collect(),
            default_provider: ProviderLimits::default(),
            models: BTreeMap::new(),
        }
    }

    /// Grenzen eines Anbieters.
    pub fn provider(&self, provider: &str) -> ProviderLimits {
        self.providers
            .get(provider)
            .copied()
            .unwrap_or(self.default_provider)
    }

    /// Grenzen eines Modells.
    pub fn model(&self, provider: &str, model: &str) -> ModelLimits {
        self.models
            .get(&(provider.to_string(), model.to_string()))
            .copied()
            .unwrap_or_default()
    }

    /// Setzt die Grenzen eines Modells.
    pub fn set_model(&mut self, provider: &str, model: &str, limits: ModelLimits) {
        self.models
            .insert((provider.to_string(), model.to_string()), limits);
    }
}

/// Grund einer Pause.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PauseReason {
    /// Rate-Limit-Header oder `Retry-After`.
    RateLimit,
    /// Schutzschalter nach aufeinanderfolgenden endgültigen Fehlern.
    CircuitBreaker,
}

/// Hinweis für die API-Fortschrittsanzeige (INTEG-007).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PauseNotice {
    pub provider: String,
    pub reason: PauseReason,
    /// Ende der Pause als Wanduhrzeit (Unix-Millisekunden).
    pub until_unix_millis: i64,
}

impl PauseNotice {
    /// AI-017: „<Anbieter> pausiert bis <Uhrzeit> (Rate-Limit)“; die Uhrzeit formatiert die Oberfläche.
    pub fn rate_limit_text(&self, provider_name: &str, time: &str) -> String {
        format!("{provider_name} pausiert bis {time} (Rate-Limit)")
    }
}

/// Ergebnis eines Eintrags mit allen Versuchen.
#[derive(Debug, Clone, PartialEq)]
pub struct Execution<T> {
    pub result: Result<T, FinalError>,
    /// Gesendete Requests (1 + Wiederholungen); 0, wenn nie gesendet wurde.
    pub attempts: u32,
    /// Startzeitpunkte der Versuche auf der Uhr der Begrenzung.
    pub attempt_starts: Vec<Duration>,
    /// Zeit vom ersten Versuch bis zum Endergebnis (Protokollspalte `Duration ms`).
    pub duration: Duration,
    /// Zeit, die der Eintrag vor dem ersten Versuch auf Grenzen gewartet hat.
    pub queued: Duration,
}

impl<T> Execution<T> {
    /// Protokollspalte `Retries`.
    pub fn retries(&self) -> u32 {
        self.attempts.saturating_sub(1)
    }

    /// Endstatus bei Fehler (AI-020); `None` bei Erfolg (den Erfolgsstatus legt der Aufrufer nach Auswertung fest).
    pub fn error_status(&self) -> Option<&AiStatus> {
        self.result.as_ref().err().map(|e| &e.status)
    }

    /// Protokollspalte `HTTP Status`.
    pub fn http_status(&self) -> Option<u16> {
        self.result.as_ref().err().and_then(|e| e.http_status)
    }

    /// Protokollspalte `Error Code`.
    pub fn error_code(&self) -> Option<&str> {
        self.result
            .as_ref()
            .err()
            .and_then(|e| e.error_code.as_deref())
    }

    /// Protokollspalte `Duration ms`.
    pub fn duration_ms(&self) -> u64 {
        u64::try_from(self.duration.as_millis()).unwrap_or(u64::MAX)
    }
}

#[derive(Debug, Default)]
struct ModelState {
    /// (Start, geschätzte Tokens) der Requests im laufenden Fenster.
    window: VecDeque<(Duration, u64)>,
}

impl ModelState {
    fn prune(&mut self, now: Duration) {
        while self
            .window
            .front()
            .is_some_and(|(start, _)| *start + WINDOW <= now)
        {
            self.window.pop_front();
        }
    }

    fn rpm_wait(&self, now: Duration, rpm: u32) -> Duration {
        let rpm = usize::try_from(rpm.max(1)).unwrap_or(usize::MAX);
        if self.window.len() < rpm {
            return Duration::ZERO;
        }
        let blocking = self.window[self.window.len() - rpm].0;
        (blocking + WINDOW).saturating_sub(now)
    }

    fn tpm_wait(&self, now: Duration, tpm: u64, tokens: u64) -> Duration {
        let mut used: u64 = self.window.iter().map(|(_, t)| *t).sum();
        if used.saturating_add(tokens) <= tpm || self.window.is_empty() {
            return Duration::ZERO;
        }
        for (start, t) in &self.window {
            used -= t;
            if used.saturating_add(tokens) <= tpm {
                return (*start + WINDOW).saturating_sub(now);
            }
        }
        // Erst wenn das Fenster leer ist; ein einzelner Request über dem Limit läuft dann allein.
        self.window.back().map_or(Duration::ZERO, |(start, _)| {
            (*start + WINDOW).saturating_sub(now)
        })
    }
}

#[derive(Debug, Clone, Copy)]
struct HeaderLimit {
    remaining: u64,
    reset_at: Duration,
    rounding: TokenRounding,
}

#[derive(Debug, Default)]
struct ProviderState {
    /// Pause aus `Retry-After` (gilt für alle Einträge des Anbieters, INTEG-005).
    pause_until: Duration,
    requests: Option<HeaderLimit>,
    input_tokens: Option<HeaderLimit>,
    consecutive_failures: u32,
    breaker_until: Option<Duration>,
}

impl ProviderState {
    fn header_wait(&self, now: Duration, tokens: u64) -> Duration {
        let mut wait = Duration::ZERO;
        if let Some(limit) = self.requests {
            if now < limit.reset_at && limit.remaining == 0 {
                wait = wait.max(limit.reset_at - now);
            }
        }
        if let Some(limit) = self.input_tokens {
            if now < limit.reset_at && limit.rounding.upper_bound(limit.remaining) < tokens {
                wait = wait.max(limit.reset_at - now);
            }
        }
        wait
    }

    fn rate_limit_until(&self, now: Duration, tokens: u64) -> Option<Duration> {
        let header = self.header_wait(now, tokens);
        let until = (now + header).max(self.pause_until);
        (until > now).then_some(until)
    }
}

#[derive(Default)]
struct State {
    providers: HashMap<String, ProviderState>,
    models: HashMap<(String, String), ModelState>,
}

/// Ratenbegrenzung, Wiederholung und Timeout für alle Anbieter eines Laufs.
pub struct Limiter {
    config: LimitsConfig,
    clock: Arc<dyn LimitClock>,
    jitter: Arc<dyn JitterSource>,
    state: Mutex<State>,
    semaphores: Mutex<HashMap<String, Arc<tokio::sync::Semaphore>>>,
}

impl Limiter {
    /// Begrenzung mit zufälligem Jitter.
    pub fn new(config: LimitsConfig, clock: Arc<dyn LimitClock>) -> Self {
        Self::with_jitter(config, clock, Arc::new(RandomJitter))
    }

    /// Begrenzung mit eigener Jitter-Quelle.
    pub fn with_jitter(
        config: LimitsConfig,
        clock: Arc<dyn LimitClock>,
        jitter: Arc<dyn JitterSource>,
    ) -> Self {
        Self {
            config,
            clock,
            jitter,
            state: Mutex::new(State::default()),
            semaphores: Mutex::new(HashMap::new()),
        }
    }

    /// Die Konfiguration.
    pub fn config(&self) -> &LimitsConfig {
        &self.config
    }

    /// Die Uhr.
    pub fn clock(&self) -> &Arc<dyn LimitClock> {
        &self.clock
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|p| p.into_inner())
    }

    fn semaphore(&self, provider: &str) -> Arc<tokio::sync::Semaphore> {
        let mut map = self.semaphores.lock().unwrap_or_else(|p| p.into_inner());
        map.entry(provider.to_string())
            .or_insert_with(|| {
                let permits = self.config.provider(provider).concurrent_requests.max(1);
                Arc::new(tokio::sync::Semaphore::new(
                    usize::try_from(permits).unwrap_or(1),
                ))
            })
            .clone()
    }

    /// Übernimmt die Rate-Limit-Angaben einer Antwort des Anbieters (AI-017 Punkt 2). Ersetzt frühere Angaben.
    pub fn observe_rate_limits(&self, provider: &str, snapshot: &RateLimitSnapshot) {
        let now = self.clock.now();
        let mut state = self.lock();
        let p = state.providers.entry(provider.to_string()).or_default();
        if let (Some(remaining), Some(reset_in)) =
            (snapshot.requests_remaining, snapshot.requests_reset_in)
        {
            p.requests = Some(HeaderLimit {
                remaining,
                reset_at: now + reset_in,
                rounding: TokenRounding::Exact,
            });
        }
        if let (Some(remaining), Some(reset_in)) = (
            snapshot.input_tokens_remaining,
            snapshot.input_tokens_reset_in,
        ) {
            p.input_tokens = Some(HeaderLimit {
                remaining,
                reset_at: now + reset_in,
                rounding: snapshot.rounding,
            });
        }
    }

    /// Wertet die Header einer Antwort aus (Anthropic, OpenAI) und übernimmt sie.
    pub fn observe_headers(&self, provider: &str, headers: &reqwest::header::HeaderMap) {
        if let Some(snapshot) =
            headers::from_header_map(provider, headers, self.clock.wall_unix_millis())
        {
            self.observe_rate_limits(provider, &snapshot);
        }
    }

    /// Laufende Pausen (für die API-Fortschrittsanzeige, INTEG-007). Header-Pausen hängen von der Größe des nächsten
    /// Requests ab; `next_input_tokens` ist dessen Schätzung.
    pub fn pauses(&self, next_input_tokens: u64) -> Vec<PauseNotice> {
        let now = self.clock.now();
        let wall = self.clock.wall_unix_millis();
        let to_wall = |until: Duration| {
            wall.saturating_add(i64::try_from((until - now).as_millis()).unwrap_or(i64::MAX))
        };
        let state = self.lock();
        let mut out = Vec::new();
        for (provider, p) in &state.providers {
            if let Some(until) = p.breaker_until.filter(|u| *u > now) {
                out.push(PauseNotice {
                    provider: provider.clone(),
                    reason: PauseReason::CircuitBreaker,
                    until_unix_millis: to_wall(until),
                });
            }
            if let Some(until) = p.rate_limit_until(now, next_input_tokens) {
                out.push(PauseNotice {
                    provider: provider.clone(),
                    reason: PauseReason::RateLimit,
                    until_unix_millis: to_wall(until),
                });
            }
        }
        out.sort_by(|a, b| a.provider.cmp(&b.provider));
        out
    }

    /// Ob der Schutzschalter des Anbieters gerade pausiert.
    pub fn breaker_open(&self, provider: &str) -> bool {
        let now = self.clock.now();
        self.lock()
            .providers
            .get(provider)
            .and_then(|p| p.breaker_until)
            .is_some_and(|until| until > now)
    }

    /// Wartezeit bis zum nächsten zulässigen Start; bei 0 wird der Start sofort gebucht.
    fn try_reserve(&self, spec: &RequestSpec, book: bool) -> Duration {
        let now = self.clock.now();
        let model_limits = self.config.model(&spec.provider, &spec.model);
        let mut state = self.lock();
        let p = state.providers.entry(spec.provider.clone()).or_default();
        let mut wait = Duration::ZERO;
        if let Some(until) = p.breaker_until {
            if until > now {
                wait = wait.max(until - now);
            } else {
                p.breaker_until = None;
                p.consecutive_failures = 0;
            }
        }
        wait = wait.max(p.pause_until.saturating_sub(now));
        wait = wait.max(p.header_wait(now, spec.estimated_input_tokens));
        let m = state
            .models
            .entry((spec.provider.clone(), spec.model.clone()))
            .or_default();
        m.prune(now);
        if let Some(rpm) = model_limits.requests_per_minute {
            wait = wait.max(m.rpm_wait(now, rpm));
        }
        if let Some(tpm) = model_limits.tokens_per_minute {
            wait = wait.max(m.tpm_wait(now, tpm, spec.estimated_input_tokens));
        }
        if !wait.is_zero() || !book {
            return wait;
        }
        m.window.push_back((now, spec.estimated_input_tokens));
        let p = state.providers.entry(spec.provider.clone()).or_default();
        if let Some(limit) = p.requests.as_mut() {
            limit.remaining = limit.remaining.saturating_sub(1);
        }
        if let Some(limit) = p.input_tokens.as_mut() {
            limit.remaining = limit.remaining.saturating_sub(spec.estimated_input_tokens);
        }
        Duration::ZERO
    }

    /// Wartet ohne Platz auf alle Grenzen, nimmt dann einen Platz der gleichzeitigen Anfragen und bucht den Start
    /// atomar. Ein wartender Eintrag hält keinen Platz, damit andere Modelle desselben Anbieters nicht blockiert werden.
    async fn reserve(
        &self,
        spec: &RequestSpec,
        semaphore: &Arc<tokio::sync::Semaphore>,
    ) -> Option<tokio::sync::OwnedSemaphorePermit> {
        loop {
            let wait = self.try_reserve(spec, false);
            if !wait.is_zero() {
                self.clock.sleep(wait).await;
                continue;
            }
            let permit = semaphore.clone().acquire_owned().await.ok();
            let wait = self.try_reserve(spec, true);
            if wait.is_zero() {
                return permit;
            }
            drop(permit);
            self.clock.sleep(wait).await;
        }
    }

    fn record_final(&self, provider: &str, effect: BreakerEffect) {
        if !self.config.circuit_breaker {
            return;
        }
        let now = self.clock.now();
        let threshold = self.config.breaker_threshold.max(1);
        let pause = self.config.breaker_pause;
        let mut state = self.lock();
        let p = state.providers.entry(provider.to_string()).or_default();
        match effect {
            BreakerEffect::Reset => p.consecutive_failures = 0,
            BreakerEffect::Neutral => {}
            BreakerEffect::Count => {
                p.consecutive_failures += 1;
                if p.consecutive_failures >= threshold {
                    p.breaker_until = Some(now + pause);
                    p.consecutive_failures = 0;
                }
            }
        }
    }

    fn set_provider_pause(&self, provider: &str, retry_after: Duration) {
        let until = self.clock.now() + retry_after;
        let mut state = self.lock();
        let p = state.providers.entry(provider.to_string()).or_default();
        p.pause_until = p.pause_until.max(until);
    }

    /// Führt einen Eintrag mit allen Versuchen aus: wartet auf Schutzschalter, `Retry-After`, Header-Grenzen, RPM und
    /// TPM, hält höchstens die eingestellte Zahl gleichzeitiger Anfragen je Anbieter, bricht jeden Versuch nach dem
    /// Timeout des Anbieters ab und wiederholt vorübergehende Fehler nach INTEG-005. `attempt` erhält die
    /// Versuchsnummer (1 = erster Versuch).
    pub async fn run<T, F, Fut>(&self, spec: &RequestSpec, mut attempt: F) -> Execution<T>
    where
        F: FnMut(u32) -> Fut,
        Fut: Future<Output = Result<T, AttemptFailure>>,
    {
        let provider_limits = self.config.provider(&spec.provider);
        let timeout = provider_limits.timeout;
        let semaphore = self.semaphore(&spec.provider);
        let queued_since = self.clock.now();
        let mut first_start: Option<Duration> = None;
        let mut starts = Vec::new();
        let mut number = 0u32;
        let mut timeouts = 0u32;
        loop {
            number += 1;
            let permit = self.reserve(spec, &semaphore).await;
            let start = self.clock.now();
            first_start.get_or_insert(start);
            starts.push(start);
            let sleep = self.clock.sleep(timeout);
            let result = tokio::select! {
                biased;
                result = attempt(number) => result,
                () = sleep => Err(AttemptFailure::from(ProviderError::Timeout {
                    seconds: timeout.as_secs(),
                })),
            };
            drop(permit);
            let failure = match result {
                Ok(value) => {
                    self.record_final(&spec.provider, BreakerEffect::Reset);
                    return self.finish(Ok(value), number, starts, first_start, queued_since);
                }
                Err(failure) => failure,
            };
            let is_timeout = matches!(failure.error, ProviderError::Timeout { .. });
            if is_timeout {
                timeouts += 1;
            }
            let timeout_cap_reached =
                is_timeout && spec.kind == RequestKind::ImageGeneration && timeouts > 1;
            if failure.is_transient() && number <= self.config.max_retries && !timeout_cap_reached {
                let delay = match failure.retry_after {
                    Some(retry_after) => {
                        self.set_provider_pause(&spec.provider, retry_after);
                        retry_after
                    }
                    None => self.config.backoff.delay(number, self.jitter.unit()),
                };
                self.clock.sleep(delay).await;
                continue;
            }
            self.record_final(&spec.provider, BreakerEffect::of(&failure.error));
            let final_error = final_error(failure);
            return self.finish(Err(final_error), number, starts, first_start, queued_since);
        }
    }

    fn finish<T>(
        &self,
        result: Result<T, FinalError>,
        attempts: u32,
        attempt_starts: Vec<Duration>,
        first_start: Option<Duration>,
        queued_since: Duration,
    ) -> Execution<T> {
        let now = self.clock.now();
        let first = first_start.unwrap_or(now);
        Execution {
            result,
            attempts,
            attempt_starts,
            duration: now.saturating_sub(first),
            queued: first.saturating_sub(queued_since),
        }
    }

    /// [`Limiter::run`] für einen Adapter: ruft `provider.execute` je Versuch mit einer Kopie von `request` auf. Refusals
    /// kommen als `Ok` zurück (`Response::refusal`); sie werden nicht wiederholt.
    pub async fn run_provider(
        &self,
        provider: &dyn AiProvider,
        request: &Request,
        estimated_input_tokens: u64,
    ) -> Execution<Response> {
        let spec = RequestSpec::new(
            provider.provider_id(),
            request.model.id.clone(),
            estimated_input_tokens,
        );
        self.run(&spec, |_| {
            let request = request.clone();
            async move {
                provider
                    .execute(request)
                    .await
                    .map_err(AttemptFailure::from)
            }
        })
        .await
    }
}

fn final_error(failure: AttemptFailure) -> FinalError {
    let http_status = failure.status_code();
    let error_code = failure
        .error_code
        .clone()
        .or_else(|| failure.cause.map(|c| c.error_code().to_string()));
    let status = failure.error.status().unwrap_or(AiStatus::ConnectionFailed);
    let message = match &failure.error {
        ProviderError::Transport { message } => message.clone(),
        ProviderError::Http { message, .. } => message.clone(),
        other => other.to_string(),
    };
    FinalError {
        http_status: match failure.error {
            ProviderError::Transport { .. } | ProviderError::Timeout { .. } => None,
            _ => http_status,
        },
        error: failure.error,
        status,
        error_code,
        message,
    }
}
