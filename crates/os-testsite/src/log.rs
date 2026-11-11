//! Request-Log der Testsite: Einträge je Request plus Zähler für Verbindungen und Gleichzeitigkeit.

use std::collections::{BTreeMap, HashMap};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Mutex;

use time::OffsetDateTime;

/// Ein geloggter Request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestRecord {
    /// Laufende Nummer ab 1 in Eingangsreihenfolge (seit dem letzten Zurücksetzen).
    pub seq: u64,
    /// Eingang des Requests, RFC 3339 in UTC mit Millisekunden (`2026-01-01T00:00:00.000Z`).
    pub ts: String,
    /// Nummer der TCP-Verbindung (ab 1, über Zurücksetzen hinweg eindeutig); mehrere Requests
    /// über Keep-Alive teilen sich die Nummer.
    pub conn_id: u64,
    /// Methode (`GET`, `HEAD` …).
    pub method: String,
    /// Wert des `Host`-Headers in Kleinbuchstaben ohne Port (leer, wenn nicht gesendet).
    pub host: String,
    /// Request-Ziel wie gesendet, Pfad inklusive Query (`/a?b=1`).
    pub path_and_query: String,
    /// Request-Header in Eingangsreihenfolge, Namen wie gesendet.
    pub headers: Vec<(String, String)>,
    /// Ob der Request über TLS einging.
    pub tls: bool,
    /// Servername aus dem TLS-ClientHello (SNI) der Verbindung, auf der der Request einging;
    /// `None` bei Klartext und bei TLS ohne SNI (Verbindung per IP-Adresse). Alle Requests einer
    /// Keep-Alive-Verbindung tragen denselben Wert.
    pub sni: Option<String>,
    /// Bytes, die der Server für diesen Request geschrieben hat: 1xx-Zwischenantworten, Kopf der
    /// finalen Antwort und Body, bei `chunked` samt Chunk-Rahmen. Gezählt auf HTTP-Ebene (bei TLS
    /// vor der Verschlüsselung). Der Wert steht erst fest, wenn die Antwort vollständig
    /// geschrieben ist; bis dahin ist er 0 (ein Client kann die letzten Bytes also schneller
    /// lesen, als der Server sie verbucht – in Tests kurz darauf warten).
    pub sent_bytes: u64,
    /// Teil von [`RequestRecord::sent_bytes`], der auf Chunk-Rahmen entfällt (Größenzeilen,
    /// CRLF, Schlusschunk); 0 ohne `chunked`.
    pub sent_framing_bytes: u64,
    /// Teil von [`RequestRecord::sent_bytes`], der auf 1xx-Zwischenantworten entfällt.
    pub sent_interim_bytes: u64,
    /// Teil von [`RequestRecord::sent_bytes`], der auf den Body der finalen Antwort entfällt: nur
    /// die Nutzdaten, ohne Statuszeile, Header und Chunk-Rahmen (K-02); 0 bei HEAD, 204 und 304.
    pub sent_body_bytes: u64,
}

impl RequestRecord {
    /// Pfad ohne Query.
    pub fn path(&self) -> &str {
        self.path_and_query
            .split_once('?')
            .map_or(self.path_and_query.as_str(), |(p, _)| p)
    }

    /// Alle Werte eines Headers (Name ohne Beachtung der Groß-/Kleinschreibung).
    pub fn header_values(&self, name: &str) -> Vec<&str> {
        self.headers
            .iter()
            .filter(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
            .collect()
    }

    /// Erster Wert eines Headers.
    pub fn header(&self, name: &str) -> Option<&str> {
        self.header_values(name).into_iter().next()
    }

    /// Gesendete Bytes von Kopf und Body der finalen Antwort ohne Chunk-Rahmen und ohne
    /// 1xx-Zwischenantworten – das, was ein Client für „Kopf + Body“ der Antwort zählt.
    pub fn sent_payload_bytes(&self) -> u64 {
        self.sent_bytes - self.sent_framing_bytes - self.sent_interim_bytes
    }
}

/// Eine angenommene TCP-Verbindung (CRAWL-151).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectionStat {
    /// Nummer wie [`RequestRecord::conn_id`].
    pub conn_id: u64,
    /// Host der Verbindung: SNI, sonst `Host` des ersten Requests (Kleinbuchstaben ohne Port); `None`, solange
    /// kein Request einging.
    pub host: Option<String>,
    /// HTTP/2 (nach ALPN).
    pub h2: bool,
    /// Requests auf dieser Verbindung.
    pub requests: u64,
    /// Höchste Zahl gleichzeitiger Requests auf dieser Verbindung.
    pub max_concurrent: usize,
}

#[derive(Debug, Default)]
struct ConnEntry {
    host: Option<String>,
    h2: bool,
    requests: u64,
    inflight: usize,
    max_inflight: usize,
}

/// Zeitstempel im Format von [`RequestRecord::ts`].
pub(crate) fn now_ts() -> String {
    let t = OffsetDateTime::now_utc();
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z",
        t.year(),
        u8::from(t.month()),
        t.day(),
        t.hour(),
        t.minute(),
        t.second(),
        t.millisecond()
    )
}

/// Interner Log-Zustand.
#[derive(Default)]
pub(crate) struct LogState {
    entries: Mutex<Vec<RequestRecord>>,
    next_conn: AtomicU64,
    connections: AtomicU64,
    inflight: AtomicUsize,
    max_inflight: AtomicUsize,
    /// Zählt Zurücksetzungen, damit ein verspätetes Verbuchen nicht in einen neuen Eintrag schreibt.
    generation: AtomicU64,
    /// Verbindungen seit dem letzten Zurücksetzen (CRAWL-151).
    conns: Mutex<BTreeMap<u64, ConnEntry>>,
    /// Laufende und höchste Gleichzeitigkeit je Host.
    hosts: Mutex<HashMap<String, (usize, usize)>>,
    /// Abgelehnte Verbindungen (`max_connections_per_client`).
    rejected: AtomicUsize,
}

/// Verweis auf einen Log-Eintrag, um die gesendeten Bytes nachzutragen.
#[derive(Debug, Clone, Copy)]
pub(crate) struct LogToken {
    generation: u64,
    index: usize,
}

/// Gesendete Bytes einer Antwort (siehe [`RequestRecord::sent_bytes`]).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Sent {
    pub total: u64,
    pub framing: u64,
    pub interim: u64,
    pub body: u64,
}

impl LogState {
    pub(crate) fn new_conn(&self) -> u64 {
        self.connections.fetch_add(1, Ordering::Relaxed);
        let id = self.next_conn.fetch_add(1, Ordering::Relaxed) + 1;
        lock(&self.conns).insert(id, ConnEntry::default());
        id
    }

    /// Ergebnis des TLS-Handshakes: SNI und ALPN der Verbindung.
    pub(crate) fn conn_tls(&self, conn_id: u64, sni: Option<&str>, h2: bool) {
        let mut conns = lock(&self.conns);
        let entry = conns.entry(conn_id).or_default();
        entry.h2 = h2;
        if let Some(sni) = sni {
            entry.host = Some(sni.to_ascii_lowercase());
        }
    }

    /// Eine wegen `max_connections_per_client` abgelehnte Verbindung.
    pub(crate) fn reject(&self) {
        self.rejected.fetch_add(1, Ordering::SeqCst);
    }

    pub(crate) fn rejected(&self) -> usize {
        self.rejected.load(Ordering::SeqCst)
    }

    pub(crate) fn connection_stats(&self) -> Vec<ConnectionStat> {
        lock(&self.conns)
            .iter()
            .map(|(id, e)| ConnectionStat {
                conn_id: *id,
                host: e.host.clone(),
                h2: e.h2,
                requests: e.requests,
                max_concurrent: e.max_inflight,
            })
            .collect()
    }

    pub(crate) fn max_concurrency_for_host(&self, host: &str) -> usize {
        lock(&self.hosts).get(host).map_or(0, |(_, max)| *max)
    }

    /// Wie [`LogState::begin`], zusätzlich je Verbindung und je Host gezählt.
    pub(crate) fn begin_on(&self, conn_id: u64, host: &str) -> RequestGuard<'_> {
        let inflight = self.begin();
        {
            let mut conns = lock(&self.conns);
            let entry = conns.entry(conn_id).or_default();
            if entry.host.is_none() && !host.is_empty() {
                entry.host = Some(host.to_string());
            }
            entry.requests += 1;
            entry.inflight += 1;
            entry.max_inflight = entry.max_inflight.max(entry.inflight);
        }
        {
            let mut hosts = lock(&self.hosts);
            let (now, max) = hosts.entry(host.to_string()).or_default();
            *now += 1;
            *max = (*max).max(*now);
        }
        RequestGuard {
            _inflight: inflight,
            log: self,
            conn_id,
            host: host.to_string(),
        }
    }

    pub(crate) fn push(&self, mut rec: RequestRecord) -> LogToken {
        let mut entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
        rec.seq = entries.len() as u64 + 1;
        let token = LogToken {
            generation: self.generation.load(Ordering::SeqCst),
            index: entries.len(),
        };
        entries.push(rec);
        token
    }

    /// Trägt die gesendeten Bytes nach; nach einem Zurücksetzen des Logs ohne Wirkung.
    pub(crate) fn set_sent(&self, token: LogToken, sent: Sent) {
        let mut entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
        if token.generation != self.generation.load(Ordering::SeqCst) {
            return;
        }
        if let Some(rec) = entries.get_mut(token.index) {
            rec.sent_bytes = sent.total;
            rec.sent_framing_bytes = sent.framing;
            rec.sent_interim_bytes = sent.interim;
            rec.sent_body_bytes = sent.body;
        }
    }

    pub(crate) fn requests(&self) -> Vec<RequestRecord> {
        self.entries
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    pub(crate) fn connections(&self) -> u64 {
        self.connections.load(Ordering::Relaxed)
    }

    pub(crate) fn max_concurrency(&self) -> usize {
        self.max_inflight.load(Ordering::Relaxed)
    }

    pub(crate) fn reset(&self) {
        let mut entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
        entries.clear();
        self.generation.fetch_add(1, Ordering::SeqCst);
        drop(entries);
        self.connections.store(0, Ordering::Relaxed);
        self.max_inflight
            .store(self.inflight.load(Ordering::Relaxed), Ordering::Relaxed);
        self.rejected.store(0, Ordering::SeqCst);
        // Verbindungen, die gerade Requests bearbeiten, bleiben mit ihrem laufenden Stand erhalten.
        lock(&self.conns).retain(|_, e| e.inflight > 0);
        for e in lock(&self.conns).values_mut() {
            e.requests = 0;
            e.max_inflight = e.inflight;
        }
        for (now, max) in lock(&self.hosts).values_mut() {
            *max = *now;
        }
    }

    /// Markiert einen Request als in Bearbeitung; der Guard beendet ihn beim Drop.
    pub(crate) fn begin(&self) -> InflightGuard<'_> {
        let now = self.inflight.fetch_add(1, Ordering::SeqCst) + 1;
        self.max_inflight.fetch_max(now, Ordering::SeqCst);
        InflightGuard(self)
    }
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// Guard eines Requests aus [`LogState::begin_on`].
pub(crate) struct RequestGuard<'a> {
    _inflight: InflightGuard<'a>,
    log: &'a LogState,
    conn_id: u64,
    host: String,
}

impl Drop for RequestGuard<'_> {
    fn drop(&mut self) {
        if let Some(e) = lock(&self.log.conns).get_mut(&self.conn_id) {
            e.inflight = e.inflight.saturating_sub(1);
        }
        if let Some((now, _)) = lock(&self.log.hosts).get_mut(&self.host) {
            *now = now.saturating_sub(1);
        }
    }
}

pub(crate) struct InflightGuard<'a>(&'a LogState);

impl Drop for InflightGuard<'_> {
    fn drop(&mut self) {
        self.0.inflight.fetch_sub(1, Ordering::SeqCst);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timestamp_format() {
        let ts = now_ts();
        assert_eq!(ts.len(), "2026-01-01T00:00:00.000Z".len(), "{ts}");
        assert!(ts.ends_with('Z'));
    }

    #[test]
    fn concurrency_and_seq() {
        let log = LogState::default();
        {
            let _a = log.begin();
            let _b = log.begin();
        }
        let _c = log.begin();
        assert_eq!(log.max_concurrency(), 2);
        log.push(RequestRecord {
            seq: 0,
            ts: String::new(),
            conn_id: 1,
            method: "GET".into(),
            host: String::new(),
            path_and_query: "/a?b=1".into(),
            headers: vec![("X-A".into(), "1".into()), ("x-a".into(), "2".into())],
            tls: false,
            sni: None,
            sent_bytes: 0,
            sent_framing_bytes: 0,
            sent_interim_bytes: 0,
            sent_body_bytes: 0,
        });
        let r = &log.requests()[0];
        assert_eq!(r.seq, 1);
        assert_eq!(r.path(), "/a");
        assert_eq!(r.header_values("X-A"), vec!["1", "2"]);
    }

    // spec: CRAWL-151
    #[test]
    fn connections_hosts_and_rejections_are_counted() {
        let log = LogState::default();
        let a = log.new_conn();
        let b = log.new_conn();
        log.conn_tls(b, Some("B.test"), true);
        {
            let _x = log.begin_on(a, "a.test");
            let _y = log.begin_on(b, "b.test");
            let _z = log.begin_on(b, "b.test");
        }
        let _w = log.begin_on(b, "b.test");
        log.reject();
        let stats = log.connection_stats();
        assert_eq!(stats.len(), 2);
        assert_eq!(
            (
                stats[0].host.as_deref(),
                stats[0].h2,
                stats[0].max_concurrent
            ),
            (Some("a.test"), false, 1)
        );
        assert_eq!(
            (
                stats[1].host.as_deref(),
                stats[1].h2,
                stats[1].requests,
                stats[1].max_concurrent
            ),
            (Some("b.test"), true, 3, 2)
        );
        assert_eq!(log.max_concurrency_for_host("b.test"), 2);
        assert_eq!(log.rejected(), 1);
        log.reset();
        assert_eq!(log.rejected(), 0);
        // die laufende Verbindung bleibt, mit ihrem laufenden Request
        assert_eq!(log.connection_stats().len(), 1);
        assert_eq!(log.max_concurrency_for_host("b.test"), 1);
    }

    fn record() -> RequestRecord {
        RequestRecord {
            seq: 0,
            ts: String::new(),
            conn_id: 1,
            method: "GET".into(),
            host: String::new(),
            path_and_query: "/".into(),
            headers: Vec::new(),
            tls: false,
            sni: None,
            sent_bytes: 0,
            sent_framing_bytes: 0,
            sent_interim_bytes: 0,
            sent_body_bytes: 0,
        }
    }

    #[test]
    fn sent_bytes_are_booked_on_the_entry_and_ignored_after_a_reset() {
        let log = LogState::default();
        let first = log.push(record());
        log.set_sent(
            first,
            Sent {
                total: 100,
                framing: 12,
                interim: 20,
                body: 40,
            },
        );
        let r = &log.requests()[0];
        assert_eq!((r.sent_bytes, r.sent_payload_bytes()), (100, 68));
        assert_eq!(r.sent_body_bytes, 40);
        // Nach dem Zurücksetzen darf ein verspätetes Verbuchen den neuen Eintrag nicht treffen.
        log.reset();
        let second = log.push(record());
        log.set_sent(
            first,
            Sent {
                total: 5,
                ..Sent::default()
            },
        );
        assert_eq!(log.requests()[0].sent_bytes, 0);
        log.set_sent(
            second,
            Sent {
                total: 7,
                ..Sent::default()
            },
        );
        assert_eq!(log.requests()[0].sent_bytes, 7);
    }
}
