//! Eigener HTTP/1.1-Server: Annahme-Schleife, TLS-Erkennung am ersten Byte, Request-Parser und
//! Antwort-Ausgabe. Handgeschrieben, weil die Fehlerrouten (eigener Reason-Text, abgebrochene
//! Antworten, TCP-Reset, tröpfelnder Body, mehrfache Header) unter hyper/axum nicht möglich sind.
//!
//! TLS prefers h2 for ordinary scenarios. If ANY route requires chunked transfer,
//! trickle, abort/reset, or informational responses, the WHOLE scenario advertises
//! only http/1.1. ALPN is negotiated before the route is known; this preserves all
//! wire-fixture semantics, including on reused connections.

use std::io::{self, Write};
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;

use bytes::{Buf, BytesMut};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, ReadBuf};
use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinSet;
use tokio_rustls::server::TlsStream;
use tokio_rustls::TlsAcceptor;

use crate::log::{RequestRecord, Sent};
use crate::respond::{build_reply, Mode, Reply, Request};
use crate::{lock, synthetic, AppState};

/// Größtes akzeptiertes Request-Kopfstück.
const MAX_HEAD: usize = 64 * 1024;
/// Größter Request-Body, der gelesen und verworfen wird.
const MAX_REQUEST_BODY: usize = 16 * 1024 * 1024;
/// Stückgröße des Bodys bei `chunked` ohne `trickle_bytes_per_s`.
const CHUNK_SIZE: usize = 128;

/// Unterliegender Strom, unverschlüsselt oder TLS.
enum Stream {
    Plain(TcpStream),
    Tls(Box<TlsStream<TcpStream>>),
}

/// Verbindung, die alle geschriebenen Bytes zählt (für `sent_bytes` im Request-Log).
struct Conn {
    stream: Stream,
    /// Alle bisher geschriebenen Bytes (auf HTTP-Ebene, bei TLS vor der Verschlüsselung).
    written: u64,
    /// Davon Chunk-Rahmen.
    framing: u64,
    /// Davon 1xx-Zwischenantworten.
    interim: u64,
    /// Davon Body-Nutzdaten der finalen Antwort (ohne Kopf und Chunk-Rahmen).
    body: u64,
}

impl Conn {
    fn new(stream: Stream) -> Self {
        Self {
            stream,
            written: 0,
            framing: 0,
            interim: 0,
            body: 0,
        }
    }

    fn counters(&self) -> (u64, u64, u64, u64) {
        (self.written, self.framing, self.interim, self.body)
    }

    /// Schließt die Verbindung hart (RST statt FIN).
    fn reset(self) {
        let tcp = match self.stream {
            Stream::Plain(t) => t,
            Stream::Tls(t) => (*t).into_inner().0,
        };
        // SO_LINGER mit 0 s blockiert beim Schließen nicht (das tut nur ein Wert > 0), sondern
        // verwirft ungesendete Daten und sendet einen Reset – genau das wird hier gebraucht.
        #[allow(deprecated)]
        let _ = tcp.set_linger(Some(Duration::ZERO));
        drop(tcp);
    }
}

impl AsyncRead for Conn {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        match &mut self.get_mut().stream {
            Stream::Plain(s) => Pin::new(s).poll_read(cx, buf),
            Stream::Tls(s) => Pin::new(&mut **s).poll_read(cx, buf),
        }
    }
}

impl AsyncWrite for Conn {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        let this = self.get_mut();
        let polled = match &mut this.stream {
            Stream::Plain(s) => Pin::new(s).poll_write(cx, buf),
            Stream::Tls(s) => Pin::new(&mut **s).poll_write(cx, buf),
        };
        if let Poll::Ready(Ok(n)) = &polled {
            this.written += *n as u64;
        }
        polled
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match &mut self.get_mut().stream {
            Stream::Plain(s) => Pin::new(s).poll_flush(cx),
            Stream::Tls(s) => Pin::new(&mut **s).poll_flush(cx),
        }
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match &mut self.get_mut().stream {
            Stream::Plain(s) => Pin::new(s).poll_shutdown(cx),
            Stream::Tls(s) => Pin::new(&mut **s).poll_shutdown(cx),
        }
    }
}

/// Nimmt Verbindungen an, bis die Aufgabe abgebrochen wird (dann enden auch alle Verbindungen).
pub(crate) async fn accept_loop(listener: TcpListener, state: Arc<AppState>) {
    let mut conns: JoinSet<()> = JoinSet::new();
    loop {
        tokio::select! {
            accepted = listener.accept() => match accepted {
                Ok((tcp, peer)) => {
                    // CRAWL-151: Grenze offener Verbindungen je Client-Adresse, gezählt bei der Annahme.
                    match ClientSlot::take(&state, peer.ip()) {
                        Some(slot) => {
                            conns.spawn(async move {
                                let _slot = slot;
                                serve(tcp, Arc::clone(&_slot.state)).await;
                            });
                        }
                        None => {
                            state.log.reject();
                            #[allow(deprecated)]
                            let _ = tcp.set_linger(Some(Duration::ZERO));
                            drop(tcp);
                        }
                    }
                }
                Err(_) => tokio::time::sleep(Duration::from_millis(10)).await,
            },
            Some(_) = conns.join_next(), if !conns.is_empty() => {}
        }
    }
}

/// Belegt eine offene Verbindung der Client-Adresse; gibt sie beim Drop frei.
struct ClientSlot {
    state: Arc<AppState>,
    ip: std::net::IpAddr,
}

impl ClientSlot {
    fn take(state: &Arc<AppState>, ip: std::net::IpAddr) -> Option<Self> {
        let mut open = state.client_conns.lock().unwrap_or_else(|p| p.into_inner());
        let n = open.entry(ip).or_insert(0);
        if state
            .max_connections_per_client
            .is_some_and(|max| *n >= max)
        {
            return None;
        }
        *n += 1;
        Some(ClientSlot {
            state: Arc::clone(state),
            ip,
        })
    }
}

impl Drop for ClientSlot {
    fn drop(&mut self) {
        let mut open = self
            .state
            .client_conns
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        if let Some(n) = open.get_mut(&self.ip) {
            *n = n.saturating_sub(1);
        }
    }
}

async fn serve(tcp: TcpStream, state: Arc<AppState>) {
    let _ = tcp.set_nodelay(true);
    let conn_id = state.log.new_conn();
    // Erstes Byte 0x16 = TLS-Handshake (ClientHello), alles andere ist HTTP.
    let mut first = [0u8; 1];
    // `peek` wartet auf das erste Byte; 0 Byte oder ein Fehler bedeutet: Verbindung ohne Request.
    if !matches!(tcp.peek(&mut first).await, Ok(n) if n > 0) {
        return;
    }
    let (conn, tls, sni) = if first[0] == 0x16 {
        let Ok(material) = state.tls() else { return };
        let config = if state.routes.iter().any(|r| {
            r.chunked
                || r.trickle_bytes_per_s.is_some()
                || r.abort_after_headers
                || r.reset_mid_body
                || !r.informational.is_empty()
        }) {
            let mut config = (*material.server_config).clone();
            config.alpn_protocols = vec![b"http/1.1".to_vec()];
            Arc::new(config)
        } else {
            Arc::clone(&material.server_config)
        };
        let acceptor = TlsAcceptor::from(config);
        match acceptor.accept(tcp).await {
            Ok(stream) => {
                let h2 = stream.get_ref().1.alpn_protocol() == Some(b"h2");
                state
                    .log
                    .conn_tls(conn_id, stream.get_ref().1.server_name(), h2);
                if h2 {
                    serve_h2(stream, state, conn_id).await;
                    return;
                }
                // SNI aus dem ClientHello; fehlt er (Verbindung per IP-Adresse), bleibt es `None`.
                let sni = stream.get_ref().1.server_name().map(str::to_string);
                (Conn::new(Stream::Tls(Box::new(stream))), true, sni)
            }
            Err(_) => return,
        }
    } else {
        (Conn::new(Stream::Plain(tcp)), false, None)
    };
    let _ = run(conn, tls, sni, conn_id, &state).await;
}

/// Ein geparster Request-Kopf.
struct Head {
    method: String,
    target: String,
    http11: bool,
    headers: Vec<(String, String)>,
}

impl Head {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    fn has_token(&self, name: &str, token: &str) -> bool {
        self.headers
            .iter()
            .filter(|(k, _)| k.eq_ignore_ascii_case(name))
            .flat_map(|(_, v)| v.split(','))
            .any(|t| t.trim().eq_ignore_ascii_case(token))
    }
}

async fn run(
    mut conn: Conn,
    tls: bool,
    sni: Option<String>,
    conn_id: u64,
    state: &AppState,
) -> io::Result<()> {
    let mut buf = BytesMut::with_capacity(4096);
    loop {
        let head = match read_head(&mut conn, &mut buf).await? {
            HeadResult::Head(h) => h,
            HeadResult::Closed => return Ok(()),
            HeadResult::Malformed => {
                conn.write_all(
                    b"HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                )
                .await?;
                return Ok(());
            }
        };

        // Request-Body lesen und verwerfen, damit Keep-Alive-Verbindungen im Takt bleiben.
        let mut keep_alive = if head.http11 {
            !head.has_token("connection", "close")
        } else {
            head.has_token("connection", "keep-alive")
        };
        if head.header("transfer-encoding").is_some() {
            keep_alive = false; // gechunkte Request-Bodies werden nicht geparst
        }
        if let Some(len) = head
            .header("content-length")
            .and_then(|v| v.trim().parse::<usize>().ok())
        {
            if len > MAX_REQUEST_BODY {
                return Ok(());
            }
            while buf.len() < len {
                if conn.read_buf(&mut buf).await? == 0 {
                    return Ok(());
                }
            }
            buf.advance(len);
        }

        let (target, host_from_target) = split_target(&head.target);
        // Absolute Form des Ziels hat Vorrang vor dem Host-Header (RFC 9112, 3.2.2).
        let host = host_from_target
            .or_else(|| head.header("host").map(host_only))
            .unwrap_or_default();
        let path = target
            .split_once('?')
            .map_or(target, |(p, _)| p)
            .to_string();
        let req = Request {
            method: head.method.clone(),
            target: target.to_string(),
            path,
            host,
            headers: head.headers.clone(),
            tls,
        };

        if state.counts.is_some() && req.path == synthetic::REQUEST_LOG_PATH {
            let reply = request_log_reply(state);
            if write_reply(&mut conn, &reply, req.method == "HEAD", keep_alive, false).await?
                == Next::Close
            {
                return Ok(());
            }
            continue;
        }

        let (written_before, framing_before, interim_before, body_before) = conn.counters();
        let _inflight = state.log.begin_on(conn_id, &req.host);
        if let Some(counts) = &state.counts {
            *lock(counts).entry(req.path.clone()).or_insert(0) += 1;
        }
        let token = state.detailed_log.then(|| {
            state.log.push(RequestRecord {
                seq: 0,
                ts: crate::log::now_ts(),
                conn_id,
                method: req.method.clone(),
                host: req.host.clone(),
                path_and_query: req.target.clone(),
                headers: req.headers.clone(),
                tls,
                sni: sni.clone(),
                sent_bytes: 0,
                sent_framing_bytes: 0,
                sent_interim_bytes: 0,
                sent_body_bytes: 0,
            })
        });

        let reply = build_reply(state, &req).await;
        if reply.mode == Mode::Hang {
            // Nie antworten; die Aufgabe endet erst, wenn die Site beendet wird.
            std::future::pending::<()>().await;
        }
        // Zwischenantworten sind erst ab HTTP/1.1 erlaubt (RFC 9110, 15.2).
        let written = write_reply(
            &mut conn,
            &reply,
            req.method == "HEAD",
            keep_alive,
            head.http11,
        )
        .await;
        if let Some(token) = token {
            let (w, f, i, b) = conn.counters();
            state.log.set_sent(
                token,
                Sent {
                    total: w - written_before,
                    framing: f - framing_before,
                    interim: i - interim_before,
                    body: b - body_before,
                },
            );
        }
        match written? {
            Next::KeepAlive => {}
            Next::Close => return Ok(()),
            Next::Reset => {
                conn.reset();
                return Ok(());
            }
        }
    }
}

enum HeadResult {
    Head(Head),
    Closed,
    Malformed,
}

async fn read_head(conn: &mut Conn, buf: &mut BytesMut) -> io::Result<HeadResult> {
    let mut searched = 0;
    let end = loop {
        // Führende Leerzeilen zwischen Requests ignorieren (RFC 9112, 2.2).
        while buf.starts_with(b"\r\n") {
            buf.advance(2);
            searched = 0;
        }
        if let Some(pos) = find_head_end(buf, searched) {
            break pos;
        }
        searched = buf.len().saturating_sub(3);
        if buf.len() > MAX_HEAD {
            return Ok(HeadResult::Malformed);
        }
        match conn.read_buf(buf).await {
            Ok(0) | Err(_) => return Ok(HeadResult::Closed),
            Ok(_) => {}
        }
    };
    let raw = buf.split_to(end + 4);
    let text = String::from_utf8_lossy(&raw[..end]);
    let mut lines = text.split("\r\n");
    let Some(request_line) = lines.next() else {
        return Ok(HeadResult::Malformed);
    };
    let mut parts = request_line.split(' ');
    let (Some(method), Some(target), Some(version), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return Ok(HeadResult::Malformed);
    };
    if !version.starts_with("HTTP/1.") || method.is_empty() || target.is_empty() {
        return Ok(HeadResult::Malformed);
    }
    let mut headers = Vec::new();
    for line in lines {
        let Some((name, value)) = line.split_once(':') else {
            return Ok(HeadResult::Malformed);
        };
        headers.push((name.trim().to_string(), value.trim().to_string()));
    }
    Ok(HeadResult::Head(Head {
        method: method.to_string(),
        target: target.to_string(),
        http11: version == "HTTP/1.1",
        headers,
    }))
}

fn find_head_end(buf: &[u8], from: usize) -> Option<usize> {
    buf.get(from..)?
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .map(|p| p + from)
}

/// Zerlegt ein Request-Ziel; die absolute Form (`http://host/pfad`) liefert den Host mit.
fn split_target(target: &str) -> (&str, Option<String>) {
    for scheme in ["http://", "https://"] {
        if let Some(rest) = target.strip_prefix(scheme) {
            let (authority, path) = rest
                .find('/')
                .map_or((rest, "/"), |i| (&rest[..i], &rest[i..]));
            return (path, Some(host_only(authority)));
        }
    }
    (target, None)
}

/// Host ohne Port in Kleinbuchstaben (`[::1]:80` → `[::1]`).
fn host_only(value: &str) -> String {
    let value = value.trim();
    let host = if value.starts_with('[') {
        value.find(']').map_or(value, |i| &value[..=i])
    } else {
        value.split(':').next().unwrap_or(value)
    };
    host.to_ascii_lowercase()
}

fn request_log_reply(state: &AppState) -> Reply {
    let counts = state.counts.as_ref().map(|m| {
        lock(m)
            .iter()
            .map(|(k, v)| (k.clone(), *v))
            .collect::<std::collections::BTreeMap<_, _>>()
    });
    let counts = counts.unwrap_or_default();
    let total: u64 = counts.values().sum();
    let json = serde_json::json!({ "total": total, "paths": counts });
    Reply {
        informational: Vec::new(),
        status: 200,
        reason: "OK".to_string(),
        headers: vec![("Content-Type".to_string(), "application/json".to_string())],
        body: json.to_string().into_bytes(),
        chunked: false,
        trickle: None,
        mode: Mode::Normal,
    }
}

#[derive(Debug, PartialEq, Eq)]
enum Next {
    KeepAlive,
    Close,
    /// Verbindung hart zurücksetzen (RST).
    Reset,
}

fn has_header(headers: &[(String, String)], name: &str) -> bool {
    headers.iter().any(|(k, _)| k.eq_ignore_ascii_case(name))
}

/// Schreibt die Antwort gemäß [`Reply::mode`].
async fn write_reply(
    conn: &mut Conn,
    reply: &Reply,
    head_only: bool,
    keep_alive: bool,
    interim_allowed: bool,
) -> io::Result<Next> {
    if interim_allowed {
        write_informational(conn, reply).await?;
    }
    let bodyless = reply.status == 204 || reply.status == 304 || reply.status < 200;
    let custom_framing = has_header(&reply.headers, "content-length")
        || has_header(&reply.headers, "transfer-encoding");
    let close = !keep_alive
        || custom_framing
        || matches!(reply.mode, Mode::AbortAfterHeaders | Mode::ResetMidBody);

    let mut head: Vec<u8> = Vec::with_capacity(256);
    let _ = write!(head, "HTTP/1.1 {} {}\r\n", reply.status, reply.reason);
    for (name, value) in &reply.headers {
        let _ = write!(head, "{name}: {value}\r\n");
    }
    if !bodyless && !custom_framing {
        if reply.chunked {
            head.extend_from_slice(b"Transfer-Encoding: chunked\r\n");
        } else {
            let _ = write!(head, "Content-Length: {}\r\n", reply.body.len());
        }
    }
    if close {
        head.extend_from_slice(b"Connection: close\r\n");
    }
    head.extend_from_slice(b"\r\n");

    let body: &[u8] = if bodyless || head_only {
        &[]
    } else {
        &reply.body
    };
    let chunked = reply.chunked && !bodyless && !custom_framing;

    match reply.mode {
        Mode::AbortAfterHeaders => {
            conn.write_all(&head).await?;
            conn.flush().await?;
            let _ = conn.shutdown().await;
            return Ok(Next::Close);
        }
        Mode::ResetMidBody => {
            let half = &reply.body[..reply.body.len() / 2];
            conn.write_all(&head).await?;
            if chunked {
                write_chunk(conn, half).await?;
            } else {
                conn.body += half.len() as u64;
                conn.write_all(half).await?;
            }
            conn.flush().await?;
            // Dem Client Zeit geben, Kopf und halben Body zu lesen, bevor der Reset kommt.
            tokio::time::sleep(Duration::from_millis(20)).await;
            return Ok(Next::Reset);
        }
        Mode::Hang | Mode::Normal => {}
    }

    match (reply.trickle, chunked) {
        (None, false) => {
            // Häufigster Fall: Kopf und Body in einem Schreibvorgang.
            conn.body += body.len() as u64;
            if body.len() <= 1024 * 1024 {
                head.extend_from_slice(body);
                conn.write_all(&head).await?;
            } else {
                conn.write_all(&head).await?;
                conn.write_all(body).await?;
            }
        }
        (None, true) => {
            conn.write_all(&head).await?;
            for piece in body.chunks(CHUNK_SIZE) {
                write_chunk(conn, piece).await?;
            }
            if !head_only {
                write_last_chunk(conn).await?;
            }
        }
        (Some(rate), _) => {
            conn.write_all(&head).await?;
            conn.flush().await?;
            let (piece_len, interval) = trickle_plan(rate);
            let mut first = true;
            for piece in body.chunks(piece_len) {
                if !first {
                    tokio::time::sleep(interval).await;
                }
                first = false;
                if chunked {
                    write_chunk(conn, piece).await?;
                } else {
                    conn.body += piece.len() as u64;
                    conn.write_all(piece).await?;
                }
                conn.flush().await?;
            }
            if chunked && !head_only {
                write_last_chunk(conn).await?;
            }
        }
    }
    conn.flush().await?;
    if close {
        let _ = conn.shutdown().await;
        return Ok(Next::Close);
    }
    Ok(Next::KeepAlive)
}

/// Sendet die 1xx-Zwischenantworten der Route, jede sofort (damit der Client sie vor der finalen
/// Antwort sieht).
async fn write_informational(conn: &mut Conn, reply: &Reply) -> io::Result<()> {
    for info in &reply.informational {
        let mut out: Vec<u8> = Vec::with_capacity(128);
        let _ = write!(out, "HTTP/1.1 {} {}\r\n", info.status, info.reason);
        for (name, value) in &info.headers {
            let _ = write!(out, "{name}: {value}\r\n");
        }
        out.extend_from_slice(b"\r\n");
        conn.write_all(&out).await?;
        conn.flush().await?;
        conn.interim += out.len() as u64;
    }
    Ok(())
}

async fn write_chunk(conn: &mut Conn, piece: &[u8]) -> io::Result<()> {
    if piece.is_empty() {
        return Ok(());
    }
    let mut out = Vec::with_capacity(piece.len() + 16);
    let _ = write!(out, "{:x}\r\n", piece.len());
    out.extend_from_slice(piece);
    out.extend_from_slice(b"\r\n");
    conn.framing += (out.len() - piece.len()) as u64;
    conn.body += piece.len() as u64;
    conn.write_all(&out).await
}

/// Schlusschunk (`0\r\n\r\n`), zählt vollständig als Chunk-Rahmen.
async fn write_last_chunk(conn: &mut Conn) -> io::Result<()> {
    const LAST: &[u8] = b"0\r\n\r\n";
    conn.framing += LAST.len() as u64;
    conn.write_all(LAST).await
}

/// Stückgröße und Pause, die zusammen `rate` Byte pro Sekunde ergeben.
fn trickle_plan(rate: u64) -> (usize, Duration) {
    if rate >= 10 {
        (
            usize::try_from(rate / 10).unwrap_or(usize::MAX).max(1),
            Duration::from_millis(100),
        )
    } else {
        (1, Duration::from_millis(1000 / rate.max(1)))
    }
}

/// Dieselben Fixture-Routen über h2, ausschließlich nach ALPN im TLS-Pfad.
async fn serve_h2(stream: TlsStream<TcpStream>, state: Arc<AppState>, conn_id: u64) {
    let sni = stream.get_ref().1.server_name().map(str::to_string);
    let records = Arc::new(std::sync::Mutex::new(std::collections::HashMap::new()));
    let stream = H2Logged {
        stream,
        state: state.clone(),
        records: records.clone(),
        pending: BytesMut::new(),
    };
    let Ok(mut connection) = h2::server::handshake(stream).await else {
        return;
    };
    let mut tasks = JoinSet::new();
    loop {
        tokio::select! {
            incoming = connection.accept() => {
                let Some(Ok((request, mut respond))) = incoming else { break; };
                let state = state.clone();
                let sni = sni.clone();
                let records = records.clone();
                let stream_id = respond.stream_id().as_u32();
                tasks.spawn(async move {
                    let req = Request {
                        method: request.method().to_string(), target: request.uri().path_and_query().map_or("/", |p| p.as_str()).into(),
                        path: request.uri().path().into(), host: request.uri().host().unwrap_or("").to_ascii_lowercase(),
                        headers: request.headers().iter().map(|(n,v)| (n.to_string(), String::from_utf8_lossy(v.as_bytes()).into_owned())).collect(), tls: true,
                    };
                    let _inflight = state.log.begin_on(conn_id, &req.host);
                    if let Some(counts) = &state.counts { *lock(counts).entry(req.path.clone()).or_insert(0) += 1; }
                    let token = state.detailed_log.then(|| state.log.push(RequestRecord { seq: 0, ts: crate::log::now_ts(), conn_id, method: req.method.clone(), host: req.host.clone(), path_and_query: req.target.clone(), headers: req.headers.clone(), tls: true, sni, sent_bytes: 0, sent_framing_bytes: 0, sent_interim_bytes: 0, sent_body_bytes: 0 }));
                    if let Some(token) = token { records.lock().unwrap_or_else(|p| p.into_inner()).insert(stream_id, (token, Sent::default())); }
                    let reply = build_reply(&state, &req).await;
                    if reply.mode == Mode::Hang { std::future::pending::<()>().await; }
                    let mut response = axum::http::Response::builder().status(reply.status);
                    for (name, value) in &reply.headers {
                        if !["connection", "transfer-encoding", "keep-alive"].iter().any(|n| name.eq_ignore_ascii_case(n)) { response = response.header(name, value); }
                    }
                    let Ok(response) = response.body(()) else { return; };
                    let empty = req.method == "HEAD" || reply.body.is_empty();
                    let Ok(mut send) = respond.send_response(response, empty) else { return; };
                    if !empty {
                        let mut body = bytes::Bytes::from(reply.body);
                        while !body.is_empty() {
                            send.reserve_capacity(body.len().min(16 * 1024));
                            let Some(Ok(capacity)) = futures_util::future::poll_fn(|cx| send.poll_capacity(cx)).await else { return; };
                            let n = capacity.min(body.len());
                            if send.send_data(body.split_to(n), body.is_empty()).is_err() { return; }
                        }
                    }
                });
            },
            Some(_) = tasks.join_next(), if !tasks.is_empty() => {}
        }
    }
}

type H2Records = std::sync::Mutex<std::collections::HashMap<u32, (crate::log::LogToken, Sent)>>;

/// Zählt tatsächlich geschriebene HTTP/2-Frames vor TLS, getrennt je Stream.
struct H2Logged {
    stream: TlsStream<TcpStream>,
    state: Arc<AppState>,
    records: Arc<H2Records>,
    pending: BytesMut,
}
impl AsyncRead for H2Logged {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut self.stream).poll_read(cx, buf)
    }
}
impl AsyncWrite for H2Logged {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bytes: &[u8],
    ) -> Poll<io::Result<usize>> {
        let result = Pin::new(&mut self.stream).poll_write(cx, bytes);
        if let Poll::Ready(Ok(n)) = result {
            self.pending.extend_from_slice(&bytes[..n]);
            while self.pending.len() >= 9 {
                let length = (usize::from(self.pending[0]) << 16)
                    | (usize::from(self.pending[1]) << 8)
                    | usize::from(self.pending[2]);
                if self.pending.len() < length + 9 {
                    break;
                }
                let frame = self.pending.split_to(length + 9);
                let stream = u32::from_be_bytes(frame[5..9].try_into().unwrap()) & 0x7fff_ffff;
                if let Some((token, sent)) = self
                    .records
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .get_mut(&stream)
                {
                    sent.total += (length + 9) as u64;
                    sent.framing += 9;
                    if frame[3] == 0 {
                        sent.body += length as u64;
                    }
                    self.state.log.set_sent(*token, *sent);
                }
            }
        }
        result
    }
    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.stream).poll_flush(cx)
    }
    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.stream).poll_shutdown(cx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_parsing() {
        assert_eq!(host_only("Site.Test:8080"), "site.test");
        assert_eq!(host_only("[::1]:80"), "[::1]");
        assert_eq!(host_only("127.0.0.1"), "127.0.0.1");
        let (path, host) = split_target("http://A.test:81/x?y=1");
        assert_eq!((path, host.as_deref()), ("/x?y=1", Some("a.test")));
        assert_eq!(split_target("/plain").0, "/plain");
    }

    #[test]
    fn trickle_rates() {
        assert_eq!(trickle_plan(1000), (100, Duration::from_millis(100)));
        assert_eq!(trickle_plan(5), (1, Duration::from_millis(200)));
        assert_eq!(trickle_plan(1), (1, Duration::from_millis(1000)));
    }

    #[test]
    fn head_end_search() {
        assert_eq!(find_head_end(b"GET / HTTP/1.1\r\n\r\nrest", 0), Some(14));
        assert_eq!(find_head_end(b"GET /", 0), None);
    }
}
