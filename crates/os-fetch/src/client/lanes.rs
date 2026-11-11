//! Getrennte Verbindungen pro Host (CRAWL-151).
//!
//! Ein reqwest-Client hält je Host höchstens eine HTTP/2-Verbindung. Für mehrere Verbindungen je Host gibt es
//! deshalb bis zu n Clients („Spuren“); Spur `i` jedes Hosts ist Client `i`. Die Spuren teilen sich alle Hosts,
//! weil reqwest seine Verbindungen ohnehin je Host (Schema, Host, Port) verwaltet. Je Host führt [`Lanes`] Buch,
//! welche Spur wie viele Anfragen trägt und welches Protokoll sie spricht, und vergibt danach (Verhalten 1–4):
//!
//! 1. Eine freie Spur (keine laufende Anfrage) wird zuerst genommen: Leerlauf-Verbindungen werden
//!    wiederverwendet.
//! 2. Sind alle Spuren belegt und ist die Grenze nicht erreicht, entsteht eine neue Spur (neue Verbindung).
//!    Solange der Host noch nie zwei Verbindungen gleichzeitig angenommen hat, wird höchstens eine neue Spur auf
//!    einmal geöffnet: Lehnt der Server die zweite Verbindung ab, folgt kein weiterer Versuch (Verhalten 3).
//! 3. Sonst geht die Anfrage unter HTTP/2 an die Spur mit den wenigsten laufenden Anfragen, solange sie damit
//!    nicht mehr trägt als ein gleichmäßiger Anteil aller laufenden Anfragen des Hosts. Unter HTTP/1.1 und
//!    solange das Protokoll einer Spur unbekannt ist (vor der ersten Antwort), trägt eine Spur genau eine
//!    Anfrage; die Anfrage wartet dann, statt eine weitere Verbindung zu öffnen.
//!
//! Lehnt der Server eine zusätzliche Verbindung ab (Verbindungsfehler, TLS-Fehler, `GOAWAY`/`REFUSED_STREAM` vor
//! der ersten Antwort einer neuen Spur), während eine andere Spur zum Host besteht, wird die Spur verworfen und
//! die Grenze des Hosts für den Rest des Abrufers auf die bestehenden Spuren gesenkt ([`Lease::fall_back`]).
//!
//! Die Wartezeit auf eine Spur liegt vor dem Rate-Limit und vor dem Start der Antwortzeit.

use std::collections::HashMap;
use std::pin::pin;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use os_model::NoResponseReason;
use tokio::sync::Notify;

use super::classify::Failure;
use crate::error::FetchError;

/// Ab dieser Leerlaufzeit gilt das Protokoll einer Spur wieder als unbekannt: Die Verbindung kann inzwischen
/// geschlossen sein, und eine neue Verbindung soll nicht von mehreren Anfragen zugleich aufgebaut werden.
const STALE_AFTER: Duration = Duration::from_secs(2);

/// Baut einen weiteren Client mit derselben Konfiguration wie Spur 0.
pub(crate) type ClientFactory =
    Arc<dyn Fn() -> Result<reqwest::Client, FetchError> + Send + Sync + 'static>;

/// Host im Sinne der Grenze: Schema, Host, Port.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct HostKey {
    scheme: String,
    host: String,
    port: u16,
}

impl HostKey {
    pub(crate) fn of(url: &url::Url) -> Self {
        HostKey {
            scheme: url.scheme().to_ascii_lowercase(),
            host: url.host_str().unwrap_or("").to_ascii_lowercase(),
            port: url.port_or_known_default().unwrap_or(0),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Proto {
    /// Noch keine Antwort auf dieser Spur (oder lange Leerlauf): eine Anfrage zur Zeit.
    Unknown,
    Http1,
    Http2,
}

#[derive(Debug)]
struct Lane {
    inflight: usize,
    proto: Proto,
    /// Die Spur hat schon eine Antwort von diesem Host erhalten (die Verbindung wurde angenommen).
    confirmed: bool,
    /// Vom Server abgelehnt (Verhalten 3); wird nie wieder vergeben.
    dead: bool,
    idle_since: Instant,
}

#[derive(Debug)]
struct Host {
    limit: usize,
    lanes: Vec<Lane>,
    /// Der Host hat schon zwei Verbindungen gleichzeitig angenommen.
    multi_ok: bool,
    notify: Arc<Notify>,
}

impl Host {
    fn new(limit: usize) -> Self {
        Host {
            limit,
            lanes: Vec::new(),
            multi_ok: false,
            notify: Arc::new(Notify::new()),
        }
    }

    /// Wählt eine Spur oder `None` (warten).
    fn pick(&mut self, max_lanes: usize, now: Instant) -> Option<usize> {
        for lane in &mut self.lanes {
            if lane.inflight == 0
                && lane.proto != Proto::Unknown
                && now.saturating_duration_since(lane.idle_since) >= STALE_AFTER
            {
                lane.proto = Proto::Unknown;
            }
        }
        // 1. freie Spur
        if let Some(i) = self.lanes.iter().position(|l| !l.dead && l.inflight == 0) {
            return Some(i);
        }
        // 2. neue Spur
        let live_n = self.lanes.iter().filter(|l| !l.dead).count();
        let unconfirmed = self
            .lanes
            .iter()
            .any(|l| !l.dead && l.proto == Proto::Unknown);
        if live_n < self.limit
            && self.lanes.len() < max_lanes
            && (live_n == 0 || self.multi_ok || !unconfirmed)
        {
            self.lanes.push(Lane {
                inflight: 0,
                proto: Proto::Unknown,
                confirmed: false,
                dead: false,
                idle_since: now,
            });
            return Some(self.lanes.len() - 1);
        }
        // 3. HTTP/2: wenigste laufende Anfragen, höchstens ein gleichmäßiger Anteil
        let total: usize = self
            .lanes
            .iter()
            .filter(|l| !l.dead)
            .map(|l| l.inflight)
            .sum();
        let share = (total + 1).div_ceil(live_n.max(1));
        self.lanes
            .iter()
            .enumerate()
            .filter(|(_, l)| !l.dead && l.proto == Proto::Http2)
            .min_by_key(|(i, l)| (l.inflight, *i))
            .filter(|(_, l)| l.inflight < share)
            .map(|(i, _)| i)
    }
}

/// Spuren eines Abrufers.
pub(crate) struct Lanes {
    /// Grenze je Host (1–6).
    limit: usize,
    factory: ClientFactory,
    clients: Mutex<Vec<Option<reqwest::Client>>>,
    hosts: Mutex<HashMap<HostKey, Host>>,
}

impl std::fmt::Debug for Lanes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Lanes").field("limit", &self.limit).finish()
    }
}

impl Lanes {
    /// `first` ist Spur 0; weitere Spuren baut `factory` bei Bedarf.
    pub(crate) fn new(limit: usize, first: reqwest::Client, factory: ClientFactory) -> Self {
        let limit = limit.max(1);
        let mut clients = vec![None; limit];
        clients[0] = Some(first);
        Lanes {
            limit,
            factory,
            clients: Mutex::new(clients),
            hosts: Mutex::new(HashMap::new()),
        }
    }

    fn hosts(&self) -> MutexGuard<'_, HashMap<HostKey, Host>> {
        self.hosts.lock().unwrap_or_else(|p| p.into_inner())
    }

    fn client(&self, lane: usize) -> Result<reqwest::Client, Failure> {
        if let Some(client) = self
            .clients
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .get(lane)
            .cloned()
            .flatten()
        {
            return Ok(client);
        }
        // Außerhalb der Sperre bauen; ein gleichzeitig gebauter Client gewinnt, der andere entfällt ungenutzt.
        let built = (self.factory)().map_err(|e| Failure {
            proxy_auth_required: false,
            reason: NoResponseReason::ConnectionError,
            detail: e.to_string(),
        })?;
        let mut clients = self.clients.lock().unwrap_or_else(|p| p.into_inner());
        Ok(clients[lane].get_or_insert(built).clone())
    }

    /// Wartet auf eine Spur für `url` (ohne Zeitgrenze: jede belegte Spur endet mit ihrer eigenen Anfrage).
    pub(crate) async fn acquire(self: &Arc<Self>, url: &url::Url) -> Result<Lease, Failure> {
        let key = HostKey::of(url);
        loop {
            let notify = Arc::clone(
                &self
                    .hosts()
                    .entry(key.clone())
                    .or_insert_with(|| Host::new(self.limit))
                    .notify,
            );
            // Erst anmelden, dann prüfen: Eine Freigabe dazwischen geht so nicht verloren.
            let mut notified = pin!(notify.notified());
            notified.as_mut().enable();
            let picked = self.hosts().get_mut(&key).and_then(|host| {
                let lane = host.pick(self.limit, Instant::now())?;
                host.lanes[lane].inflight += 1;
                Some(lane)
            });
            if let Some(lane) = picked {
                let mut lease = Lease {
                    lanes: Arc::clone(self),
                    key,
                    lane,
                    client: None,
                };
                lease.client = Some(self.client(lane)?);
                return Ok(lease);
            }
            notified.await;
        }
    }

    fn update(&self, key: &HostKey, f: impl FnOnce(&mut Host)) {
        let mut hosts = self.hosts();
        if let Some(host) = hosts.get_mut(key) {
            f(host);
            host.notify.notify_waiters();
        }
    }

    /// Grenze und offene Spuren eines Hosts (für Tests und Diagnose).
    #[cfg(test)]
    fn state(&self, url: &str) -> Option<(usize, usize, Vec<usize>)> {
        let url = url::Url::parse(url).ok()?;
        let hosts = self.hosts();
        let host = hosts.get(&HostKey::of(&url))?;
        Some((
            host.limit,
            host.lanes.iter().filter(|l| !l.dead).count(),
            host.lanes.iter().map(|l| l.inflight).collect(),
        ))
    }
}

/// Vergebene Spur; gibt sie beim Drop frei.
pub(crate) struct Lease {
    lanes: Arc<Lanes>,
    key: HostKey,
    lane: usize,
    client: Option<reqwest::Client>,
}

impl Lease {
    pub(crate) fn client(&self) -> &reqwest::Client {
        self.client
            .as_ref()
            .expect("Spur ohne Client wird nie ausgegeben")
    }

    /// Erste Antwort (Kopf) auf dieser Spur: Protokoll merken, Verbindung gilt als angenommen.
    pub(crate) fn confirm(&self, version: reqwest::Version) {
        let proto = if version == reqwest::Version::HTTP_2 {
            Proto::Http2
        } else {
            Proto::Http1
        };
        let lane = self.lane;
        self.lanes.update(&self.key, |host| {
            if let Some(l) = host.lanes.get_mut(lane) {
                l.proto = proto;
                l.confirmed = true;
            }
            if host.lanes.iter().filter(|l| l.confirmed && !l.dead).count() >= 2 {
                host.multi_ok = true;
            }
        });
    }

    /// Verhalten 3: Ist `failure` die Ablehnung einer zusätzlichen Verbindung (neue Spur ohne bisherige Antwort,
    /// während eine andere Spur zum Host besteht), wird die Spur verworfen, die Grenze des Hosts auf die
    /// bestehenden Spuren gesenkt und `true` geliefert: Der Aufrufer stellt die Anfrage über eine bestehende
    /// Spur erneut, ohne Fehlerzeile und ohne Wiederholungszähler.
    pub(crate) fn fall_back(&self, failure: &Failure) -> bool {
        if failure.proxy_auth_required
            || !matches!(
                failure.reason,
                NoResponseReason::ConnectionRefused
                    | NoResponseReason::ConnectionError
                    | NoResponseReason::TlsError
            )
        {
            return false;
        }
        let mut done = false;
        let lane = self.lane;
        self.lanes.update(&self.key, |host| {
            let others = host
                .lanes
                .iter()
                .enumerate()
                .filter(|(i, l)| *i != lane && !l.dead)
                .count();
            let Some(l) = host.lanes.get_mut(lane) else {
                return;
            };
            if l.confirmed || l.dead || others == 0 {
                return;
            }
            l.dead = true;
            host.limit = others;
            done = true;
            tracing::debug!(
                host = %self.key.host,
                port = self.key.port,
                connections = others,
                "zusätzliche Verbindung abgelehnt, Host bleibt bei den bestehenden Verbindungen"
            );
        });
        done
    }

    /// Gibt die Spur frei. Unter HTTP/1.1 kehrt die Verbindung erst nach einem Wechsel der Aufgabe in den Pool
    /// zurück; ohne diesen Vorlauf könnte die nächste Anfrage der Spur sie noch nicht vorfinden und eine zweite
    /// Verbindung beginnen.
    pub(crate) async fn finish(self) {
        let http1 = {
            let hosts = self.lanes.hosts();
            hosts
                .get(&self.key)
                .and_then(|h| h.lanes.get(self.lane))
                .is_some_and(|l| l.proto == Proto::Http1)
        };
        if http1 {
            for _ in 0..4 {
                tokio::task::yield_now().await;
            }
        }
        drop(self);
    }
}

impl Drop for Lease {
    fn drop(&mut self) {
        let lane = self.lane;
        self.lanes.update(&self.key, |host| {
            if let Some(l) = host.lanes.get_mut(lane) {
                l.inflight = l.inflight.saturating_sub(1);
                if l.inflight == 0 {
                    l.idle_since = Instant::now();
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lanes(limit: usize) -> Arc<Lanes> {
        let factory: ClientFactory = Arc::new(|| Ok(reqwest::Client::new()));
        Arc::new(Lanes::new(limit, reqwest::Client::new(), factory))
    }

    fn url() -> url::Url {
        url::Url::parse("https://site.test/").unwrap()
    }

    fn refused() -> Failure {
        Failure {
            proxy_auth_required: false,
            reason: NoResponseReason::ConnectionError,
            detail: "reset".into(),
        }
    }

    async fn now_or_never(l: &Arc<Lanes>) -> Option<Lease> {
        tokio::time::timeout(Duration::from_millis(20), l.acquire(&url()))
            .await
            .ok()
            .map(Result::unwrap)
    }

    // spec: CRAWL-151
    #[tokio::test]
    async fn a_second_lane_opens_only_after_the_first_answered_and_http2_balances_the_rest() {
        let l = lanes(3);
        let a = l.acquire(&url()).await.unwrap();
        assert!(
            now_or_never(&l).await.is_none(),
            "erste Verbindung ohne Antwort: warten"
        );
        a.confirm(reqwest::Version::HTTP_2);
        let b = now_or_never(&l).await.expect("zweite Spur");
        assert_eq!(b.lane, 1);
        // Spur 1 noch ohne Antwort: höchstens ein gleichmäßiger Anteil auf Spur 0 (2 von 3)
        let c = now_or_never(&l).await.expect("geteilte HTTP/2-Spur");
        assert_eq!(c.lane, 0);
        assert!(now_or_never(&l).await.is_none());
        b.confirm(reqwest::Version::HTTP_2);
        let d = now_or_never(&l)
            .await
            .expect("dritte Spur nach zwei angenommenen");
        assert_eq!(d.lane, 2);
        d.confirm(reqwest::Version::HTTP_2);
        let e = now_or_never(&l).await.unwrap();
        assert_eq!(e.lane, 1, "wenigste laufende Anfragen");
        assert_eq!(l.state("https://site.test/").unwrap().2, vec![2, 2, 1]);
        // anderer Port: eigener Host
        let other = url::Url::parse("https://site.test:8443/").unwrap();
        let o = l.acquire(&other).await.unwrap();
        assert_eq!(o.lane, 0);
        drop((a, c, e, o));
    }

    // spec: CRAWL-151
    #[tokio::test]
    async fn http1_lanes_carry_one_request_and_waiters_get_the_freed_lane() {
        let l = lanes(2);
        let a = l.acquire(&url()).await.unwrap();
        a.confirm(reqwest::Version::HTTP_11);
        let b = now_or_never(&l).await.unwrap();
        b.confirm(reqwest::Version::HTTP_11);
        assert!(now_or_never(&l).await.is_none(), "Grenze 2 unter HTTP/1.1");
        let waiter = {
            let l = l.clone();
            tokio::spawn(async move { l.acquire(&url()).await.unwrap().lane })
        };
        tokio::time::sleep(Duration::from_millis(10)).await;
        a.finish().await;
        assert_eq!(waiter.await.unwrap(), 0);
        drop(b);
    }

    // spec: CRAWL-151
    #[tokio::test]
    async fn a_refused_additional_connection_lowers_the_limit_for_good() {
        let l = lanes(6);
        let a = l.acquire(&url()).await.unwrap();
        a.confirm(reqwest::Version::HTTP_2);
        let b = now_or_never(&l).await.unwrap();
        assert!(b.fall_back(&refused()));
        drop(b);
        assert_eq!(l.state("https://site.test/").unwrap().0, 1);
        // weiter über die eine Spur, nie wieder eine neue
        let c = now_or_never(&l).await.unwrap();
        assert_eq!(c.lane, 0);
        // die einzige Spur selbst fällt nicht zurück; eine Antwortzeitüberschreitung auch nicht
        assert!(!a.fall_back(&refused()));
        let timeout = Failure {
            reason: NoResponseReason::ResponseTimeout,
            ..refused()
        };
        assert!(!c.fall_back(&timeout));
        drop((a, c));
        let fresh = lanes(6);
        let only = fresh.acquire(&url()).await.unwrap();
        assert!(!only.fall_back(&refused()), "kein anderer Weg zum Host");
    }
}
