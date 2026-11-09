//! Hilfe der Abnahmetests B4-STARTFIX: zerlegt eine vom SMTP-Mock mitgeschnittene Nachricht (MIME, mehrteilig, Quoted-Printable
//! bzw. Base64) in Betreff und lesbare Textteile. So hängen die Prüfungen nicht an der Transportkodierung (Umlaute, weiche
//! Zeilenumbrüche mitten in einer Adresse).
#![allow(dead_code)]

/// Zerlegte Nachricht: entfalteter, dekodierter Betreff und je Text-Teil (Klartext, HTML) der dekodierte Inhalt.
#[derive(Debug, Clone)]
pub struct Message {
    pub subject: String,
    pub plain: Vec<String>,
    pub html: Vec<String>,
    pub raw: String,
}

impl Message {
    /// Alle Textteile hintereinander (Klartext und HTML).
    pub fn text(&self) -> String {
        self.plain
            .iter()
            .chain(self.html.iter())
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    }
}

pub fn parse(raw: &str) -> Message {
    let raw_norm = raw.replace("\r\n", "\n");
    let (headers, body) = split(&raw_norm);
    let subject = decode_words(&header(&headers, "subject").unwrap_or_default());
    let mut message = Message {
        subject,
        plain: Vec::new(),
        html: Vec::new(),
        raw: raw.to_owned(),
    };
    collect(&headers, body, &mut message);
    message
}

fn split(text: &str) -> (Vec<(String, String)>, &str) {
    let (head, body) = match text.find("\n\n") {
        Some(i) => (&text[..i], &text[i + 2..]),
        None => (text, ""),
    };
    let mut headers: Vec<(String, String)> = Vec::new();
    for line in head.lines() {
        if line.starts_with(' ') || line.starts_with('\t') {
            if let Some(last) = headers.last_mut() {
                last.1.push(' ');
                last.1.push_str(line.trim());
            }
        } else if let Some((k, v)) = line.split_once(':') {
            headers.push((k.trim().to_ascii_lowercase(), v.trim().to_owned()));
        }
    }
    (headers, body)
}

fn header(headers: &[(String, String)], name: &str) -> Option<String> {
    headers
        .iter()
        .find(|(k, _)| k == name)
        .map(|(_, v)| v.clone())
}

fn boundary(content_type: &str) -> Option<String> {
    let lower = content_type.to_ascii_lowercase();
    let at = lower.find("boundary=")?;
    let rest = &content_type[at + "boundary=".len()..];
    let rest = rest.trim_start();
    Some(match rest.strip_prefix('"') {
        Some(quoted) => quoted.split('"').next()?.to_owned(),
        None => rest
            .split(|c: char| c == ';' || c.is_whitespace())
            .next()?
            .to_owned(),
    })
}

fn collect(headers: &[(String, String)], body: &str, out: &mut Message) {
    let content_type = header(headers, "content-type").unwrap_or_else(|| "text/plain".into());
    let lower = content_type.to_ascii_lowercase();
    if lower.starts_with("multipart/") {
        let Some(boundary) = boundary(&content_type) else {
            return;
        };
        let marker = format!("--{boundary}");
        for part in body.split(&marker).skip(1) {
            if part.starts_with("--") {
                break;
            }
            let part = part.strip_prefix('\n').unwrap_or(part);
            let (h, b) = split(part);
            collect(&h, b, out);
        }
        return;
    }
    let encoding = header(headers, "content-transfer-encoding")
        .unwrap_or_default()
        .to_ascii_lowercase();
    let bytes = match encoding.as_str() {
        "quoted-printable" => qp(body),
        "base64" => base64(body),
        _ => body.as_bytes().to_vec(),
    };
    let text = String::from_utf8_lossy(&bytes).into_owned();
    if lower.starts_with("text/html") {
        out.html.push(text);
    } else if lower.starts_with("text/plain") {
        out.plain.push(text);
    }
    // Anhänge (ZIP usw.) sind kein Text und bleiben außen vor.
}

fn qp(body: &str) -> Vec<u8> {
    let mut out = Vec::new();
    let bytes = body.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'=' {
            if bytes.get(i + 1) == Some(&b'\n') {
                i += 2;
                continue;
            }
            if let (Some(h), Some(l)) = (bytes.get(i + 1), bytes.get(i + 2)) {
                if let (Some(h), Some(l)) = (hex(*h), hex(*l)) {
                    out.push(h * 16 + l);
                    i += 3;
                    continue;
                }
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    out
}

fn hex(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'A'..=b'F' => Some(c - b'A' + 10),
        b'a'..=b'f' => Some(c - b'a' + 10),
        _ => None,
    }
}

fn base64(body: &str) -> Vec<u8> {
    let mut out = Vec::new();
    let (mut acc, mut bits) = (0u32, 0u32);
    for c in body.bytes() {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            _ => continue,
        };
        acc = (acc << 6) | u32::from(v);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    out
}

/// RFC-2047-Wörter (`=?utf-8?B?…?=`, `=?utf-8?Q?…?=`) im Betreff.
fn decode_words(value: &str) -> String {
    let mut out = String::new();
    let mut rest = value;
    while let Some(start) = rest.find("=?") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        let parts: Vec<&str> = after.splitn(4, '?').collect();
        if parts.len() == 4 {
            if let Some(end) = parts[3].find("?=") {
                let payload = &parts[3][..end];
                let bytes = match parts[1].to_ascii_uppercase().as_str() {
                    "B" => base64(payload),
                    _ => qp(&payload.replace('_', " ")),
                };
                out.push_str(&String::from_utf8_lossy(&bytes));
                rest = &parts[3][end + 2..];
                // Zwischen zwei Wörtern steht nur Leerraum, der nicht zum Text gehört.
                rest = rest.trim_start();
                continue;
            }
        }
        out.push_str("=?");
        rest = after;
    }
    out.push_str(rest);
    out
}
