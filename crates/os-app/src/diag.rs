//! Transport-independent diagnostic and log primitives (OUT-240 … OUT-244).
//! Temporary shared source until M2-N01 provides a common diagnostic crate (C7).
//! Keep this module independent of os-app types until that migration.
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, SystemTime},
};
use tracing_subscriber::{layer::Context, registry::LookupSpan, Layer};

pub fn sensitive(name: &str) -> bool {
    let n = name.to_ascii_lowercase().replace(['-', '_'], "");
    matches!(
        n.as_str(),
        "authorization"
            | "proxyauthorization"
            | "cookie"
            | "setcookie"
            | "setcookie2"
            | "apikey"
            | "xapikey"
            | "key"
            | "token"
            | "oauthtoken"
            | "accesstoken"
            | "refreshtoken"
            | "xauthtoken"
            | "xaccesstoken"
            | "password"
            | "passwd"
            | "passphrase"
            | "secret"
            | "clientsecret"
    )
}
/// Redact assignment values while retaining subsequent diagnostic fields.
/// Header values may contain multiple words/cookies, so keep masking those conservatively.
pub fn redact_text(text: &str) -> String {
    text.lines()
        .map(|line| {
            let clean = os_fetch::redact::text(line);
            let bytes = clean.as_bytes();
            let mut out = String::with_capacity(clean.len());
            let mut copied = 0;
            let mut i = 0;
            while i < bytes.len() {
                if !(bytes[i].is_ascii_alphanumeric() || matches!(bytes[i], b'-' | b'_')) {
                    i += 1;
                    continue;
                }
                let start = i;
                while i < bytes.len()
                    && (bytes[i].is_ascii_alphanumeric() || matches!(bytes[i], b'-' | b'_'))
                {
                    i += 1;
                }
                let name = &clean[start..i];
                if !sensitive(name) {
                    continue;
                }
                let mut value = i;
                // Also accept quoted JSON keys and spaces around the assignment.
                if value < bytes.len() && matches!(bytes[value], b'"' | b'\'') {
                    value += 1;
                }
                while value < bytes.len() && bytes[value].is_ascii_whitespace() {
                    value += 1;
                }
                if value == bytes.len() || !matches!(bytes[value], b'=' | b':') {
                    continue;
                }
                value += 1;
                while value < bytes.len() && bytes[value].is_ascii_whitespace() {
                    value += 1;
                }
                let quoted = bytes
                    .get(value)
                    .copied()
                    .filter(|c| matches!(c, b'"' | b'\''));
                if quoted.is_some() {
                    value += 1;
                }
                let header = os_fetch::redact::is_sensitive_header(name);
                let mut end = value;
                while end < bytes.len() {
                    if let Some(quote) = quoted {
                        if bytes[end] == quote {
                            break;
                        }
                        if bytes[end] == b'\\' && end + 1 < bytes.len() {
                            end += 1;
                        }
                    } else if matches!(bytes[end], b'"' | b'\'')
                        || (!header
                            && (bytes[end].is_ascii_whitespace()
                                || matches!(bytes[end], b';' | b'&')))
                    {
                        break;
                    }
                    end += 1;
                }
                out.push_str(&clean[copied..value]);
                out.push_str("<redacted>");
                copied = end;
                i = end;
            }
            out.push_str(&clean[copied..]);
            out
        })
        .collect::<Vec<_>>()
        .join("\n")
}
pub fn redact(value: &Value) -> Value {
    match value {
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(k, v)| {
                    (
                        k.clone(),
                        if sensitive(k)
                            || (k == "value"
                                && map
                                    .get("name")
                                    .and_then(Value::as_str)
                                    .is_some_and(sensitive))
                        {
                            json!("<redacted>")
                        } else {
                            redact(v)
                        },
                    )
                })
                .collect(),
        ),
        Value::Array(a) => Value::Array(a.iter().map(redact).collect()),
        Value::String(s) => json!(redact_text(s)),
        other => other.clone(),
    }
}
pub fn timestamp() -> String {
    let now = time::OffsetDateTime::now_local().unwrap_or_else(|_| time::OffsetDateTime::now_utc());
    now.format(
        &time::format_description::parse_borrowed::<2>(
            "[year]-[month]-[day]T[hour]:[minute]:[second].[subsecond digits:3][offset_hour sign:mandatory]:[offset_minute]",
        )
        .expect("static timestamp format"),
    )
    .unwrap_or_default()
}
pub fn file_stamp() -> String {
    time::OffsetDateTime::now_utc()
        .format(
            &time::format_description::parse_borrowed::<2>(
                "[year][month][day]-[hour][minute][second]",
            )
            .expect("static filename format"),
        )
        .unwrap_or_default()
}

#[derive(Clone)]
pub struct LogFile {
    inner: Arc<Mutex<LogFileInner>>,
}
struct LogFileInner {
    path: PathBuf,
    rotate: bool,
    max_bytes: u64,
    archives: usize,
    file: Option<fs::File>,
    bytes: u64,
}
impl LogFile {
    pub fn new(path: PathBuf, rotate: bool) -> Self {
        Self::with_limits(path, rotate, 10 * 1024 * 1024, 5)
    }
    pub fn with_limits(path: PathBuf, rotate: bool, max_bytes: u64, archives: usize) -> Self {
        Self {
            inner: Arc::new(Mutex::new(LogFileInner {
                path,
                rotate,
                max_bytes,
                archives,
                file: None,
                bytes: 0,
            })),
        }
    }
    pub fn write_line(&self, line: &str) -> io::Result<()> {
        let mut s = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        if s.file.is_none() {
            if let Some(parent) = s.path.parent() {
                fs::create_dir_all(parent)?;
            }
            let file = fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&s.path)?;
            s.bytes = file.metadata()?.len();
            s.file = Some(file);
        }
        let rotated = s.rotate && s.bytes.saturating_add(line.len() as u64 + 1) > s.max_bytes;
        if rotated {
            // Close before renaming: Windows does not allow renaming an open log file.
            s.file.take();
            let archive = |n| s.path.with_file_name(format!("openspider.{n}.log"));
            if s.archives > 0 {
                let _ = fs::remove_file(archive(s.archives));
                for n in (1..s.archives).rev() {
                    if archive(n).exists() {
                        fs::rename(archive(n), archive(n + 1))?;
                    }
                }
                if s.path.exists() {
                    fs::rename(&s.path, archive(1))?;
                }
            } else if s.path.exists() {
                fs::remove_file(&s.path)?;
            }
            s.bytes = 0;
            s.file = Some(
                fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&s.path)?,
            );
        }
        let file = s.file.as_mut().expect("log opened above");
        writeln!(file, "{line}")?;
        file.flush()?;
        s.bytes = s.bytes.saturating_add(line.len() as u64 + 1);
        // Startup enforces retention and the folder cap. Subsequent directory scans
        // happen only at rotation, never in the per-event hot path.
        if rotated {
            if let Some(parent) = s.path.parent() {
                cleanup(parent, SystemTime::now(), u32::MAX, 500 * 1024 * 1024)?;
            }
        }
        Ok(())
    }
}

/// Deletes only recognized log files; never follows symlinks or touches crawl/support data.
pub fn log_files(dir: &Path) -> io::Result<Vec<(PathBuf, SystemTime, u64)>> {
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut files = Vec::new();
    for e in fs::read_dir(dir)? {
        let e = e?;
        let name = e.file_name().to_string_lossy().into_owned();
        if !e.file_type()?.is_file()
            || !name.ends_with(".log")
            || !(name == "openspider.log"
                || name.starts_with("openspider.")
                || name.starts_with("cli-")
                || name.starts_with("task-"))
        {
            continue;
        }
        let m = e.metadata()?;
        files.push((e.path(), m.modified()?, m.len()));
    }
    files.sort_by(|a, b| a.1.cmp(&b.1).then(a.0.cmp(&b.0)));
    Ok(files)
}
pub fn cleanup(dir: &Path, now: SystemTime, retention_days: u32, max_bytes: u64) -> io::Result<()> {
    let files = log_files(dir)?;
    let mut total: u64 = files.iter().map(|f| f.2).sum();
    for (p, mtime, size) in files {
        let run = p.file_name().is_some_and(|n| {
            let n = n.to_string_lossy();
            n.starts_with("cli-") || n.starts_with("task-")
        });
        if (run
            && now.duration_since(mtime).unwrap_or_default()
                > Duration::from_secs(u64::from(retention_days) * 86400))
            || total > max_bytes
        {
            fs::remove_file(p)?;
            total = total.saturating_sub(size);
        }
    }
    Ok(())
}

pub struct LogLayer {
    pub file: LogFile,
    pub json: bool,
    pub stderr: bool,
}
#[derive(Default)]
struct Fields(BTreeMap<String, Value>);
impl tracing::field::Visit for Fields {
    fn record_debug(&mut self, field: &tracing::field::Field, v: &dyn std::fmt::Debug) {
        self.0.insert(field.name().into(), json!(format!("{v:?}")));
    }
    fn record_str(&mut self, field: &tracing::field::Field, v: &str) {
        self.0.insert(field.name().into(), json!(v));
    }
    fn record_u64(&mut self, field: &tracing::field::Field, v: u64) {
        self.0.insert(field.name().into(), json!(v));
    }
    fn record_i64(&mut self, field: &tracing::field::Field, v: i64) {
        self.0.insert(field.name().into(), json!(v));
    }
    fn record_bool(&mut self, field: &tracing::field::Field, v: bool) {
        self.0.insert(field.name().into(), json!(v));
    }
}
impl<S: tracing::Subscriber + for<'a> LookupSpan<'a>> Layer<S> for LogLayer {
    fn on_event(&self, event: &tracing::Event<'_>, _: Context<'_, S>) {
        let mut fields = Fields::default();
        event.record(&mut fields);
        let fields = redact(&json!(fields.0));
        let meta = event.metadata();
        let line = if self.json {
            json!({"timestamp":timestamp(),"level":meta.level().as_str(),"module":meta.target(),"fields":fields}).to_string()
        } else {
            let message = fields.get("message").and_then(Value::as_str).unwrap_or("");
            let extra = fields
                .as_object()
                .into_iter()
                .flatten()
                .filter(|(k, _)| k.as_str() != "message")
                .map(|(k, v)| format!("{k}={v}"))
                .collect::<Vec<_>>()
                .join(" ");
            format!(
                "{} {} {}: {} [{}]",
                timestamp(),
                meta.level(),
                meta.target(),
                message.replace('\n', "\\n"),
                extra
            )
        };
        if let Err(e) = self.file.write_line(&line) {
            eprintln!("Logdatei nicht schreibbar: {}", e.kind());
        }
        if self.stderr {
            let _ = writeln!(io::stderr().lock(), "{line}");
        }
    }
}

pub fn system_info() -> Value {
    let system = sysinfo::System::new_all();
    json!({"version":env!("CARGO_PKG_VERSION"),"build":option_env!("OPENSPIDER_BUILD").unwrap_or("dev"),
        "buildNumber":os_model::build_number::BUILD_NUMBER,
        "versionDisplay":os_model::build_number::display(env!("CARGO_PKG_VERSION"), os_model::build_number::BUILD_NUMBER),
        "os":sysinfo::System::long_os_version().unwrap_or_else(|| std::env::consts::OS.into()),
        "architecture":std::env::consts::ARCH,"cpuCores":system.cpus().len(),
        "ramTotalBytes":system.total_memory(),"ramAvailableBytes":system.available_memory(),
        "chromiumConfigured":std::env::var_os("OPENSPIDER_CHROMIUM").is_some(),
        "chromiumAvailable":std::env::var_os("OPENSPIDER_CHROMIUM").is_some_and(|p| Path::new(&p).is_file())})
}
/// Recursive merge-style diff; callers redact the result before writing it.
pub fn config_diff(current: &Value, default: &Value) -> Value {
    if current == default {
        return json!({});
    }
    if let (Some(c), Some(d)) = (current.as_object(), default.as_object()) {
        Value::Object(
            c.iter()
                .filter(|(k, v)| d.get(*k) != Some(*v))
                .map(|(k, v)| (k.clone(), config_diff(v, d.get(k).unwrap_or(&Value::Null))))
                .collect(),
        )
    } else {
        current.clone()
    }
}
/// Snapshots the exact file set used by preview and bundle. File contents are redacted again at the boundary.
pub fn bundle_entries(
    logs: &Path,
    system: Value,
    diff: Value,
    tasks: Value,
    now: SystemTime,
) -> io::Result<Vec<(String, Vec<u8>)>> {
    let mut entries = vec![
        (
            "system.txt".into(),
            serde_json::to_vec_pretty(&redact(&system))?,
        ),
        (
            "config-diff.json".into(),
            serde_json::to_vec_pretty(&redact(&diff))?,
        ),
        (
            "tasks.json".into(),
            serde_json::to_vec_pretty(&redact(&tasks))?,
        ),
    ];
    for (path, modified, _) in log_files(logs)? {
        if now.duration_since(modified).unwrap_or_default() > Duration::from_secs(7 * 86400) {
            continue;
        }
        let raw = fs::read_to_string(&path)?;
        let clean = raw
            .lines()
            .map(|l| match serde_json::from_str::<Value>(l) {
                Ok(v) => redact(&v).to_string(),
                Err(_) => redact_text(l),
            })
            .collect::<Vec<_>>()
            .join("\n");
        entries.push((
            format!(
                "logs/{}",
                path.file_name().unwrap_or_default().to_string_lossy()
            ),
            clean.into_bytes(),
        ));
    }
    // OUT-246/OUT-244 (M3-C01, additiv): gespeicherte Absturzberichte, an der Grenze erneut redigiert.
    if let Ok(dir) = fs::read_dir(logs.join("crashes")) {
        let mut reports: Vec<PathBuf> = dir
            .flatten()
            .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
            .map(|e| e.path())
            .filter(|p| {
                p.file_name().is_some_and(|n| {
                    let n = n.to_string_lossy();
                    n.starts_with("crash-") && n.ends_with(".json")
                })
            })
            .collect();
        reports.sort();
        for path in reports {
            let Ok(raw) = fs::read_to_string(&path) else {
                continue;
            };
            entries.push((
                format!(
                    "crashes/{}",
                    path.file_name().unwrap_or_default().to_string_lossy()
                ),
                redact_text(&raw).into_bytes(),
            ));
        }
    }
    Ok(entries)
}
pub fn write_bundle(
    dest: &Path,
    entries: &[(String, Vec<u8>)],
    cancelled: impl Fn() -> bool,
) -> io::Result<u64> {
    // Exclusive temporary file in the same directory: cancellation never publishes an incomplete ZIP.
    let tmp = dest.with_extension(format!(
        "{}-{}.tmp",
        std::process::id(),
        time::OffsetDateTime::now_utc().unix_timestamp_nanos()
    ));
    let result = (|| {
        let file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp)?;
        let mut zip = zip::ZipWriter::new(file);
        for (name, bytes) in entries {
            if cancelled() {
                return Err(io::Error::new(io::ErrorKind::Interrupted, "Abgebrochen"));
            }
            zip.start_file(name, zip::write::SimpleFileOptions::default())?;
            for part in bytes.chunks(64 * 1024) {
                if cancelled() {
                    return Err(io::Error::new(io::ErrorKind::Interrupted, "Abgebrochen"));
                }
                zip.write_all(part)?;
            }
        }
        zip.finish()?.sync_all()?;
        if cancelled() {
            return Err(io::Error::new(io::ErrorKind::Interrupted, "Abgebrochen"));
        }
        // Prefer atomic no-clobber publication; support FAT/exFAT/SMB as well.
        publish_bundle(&tmp, dest, |source, target| fs::hard_link(source, target))?;
        fs::metadata(dest).map(|m| m.len())
    })();
    let _ = fs::remove_file(tmp);
    result
}

fn publish_bundle(
    tmp: &Path,
    dest: &Path,
    hard_link: impl FnOnce(&Path, &Path) -> io::Result<()>,
) -> io::Result<()> {
    match hard_link(tmp, dest) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() != io::ErrorKind::AlreadyExists => {
            // Reserve exclusively: an existing destination must never be overwritten.
            let reservation = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(dest)?;
            drop(reservation);
            if let Err(e) = fs::rename(tmp, dest) {
                let _ = fs::remove_file(dest);
                return Err(e);
            }
            Ok(())
        }
        Err(e) => Err(e),
    }
}

#[cfg(test)]
mod publication_tests {
    use super::*;

    // spec: OUT-244
    #[test]
    fn filesystems_without_hard_links_publish_complete_bundles_without_clobbering() {
        for error in [
            io::Error::from(io::ErrorKind::Unsupported),
            io::Error::from(io::ErrorKind::PermissionDenied),
            io::Error::from(io::ErrorKind::Other),
            // Windows FAT/exFAT: ERROR_INVALID_FUNCTION is not mapped to Unsupported.
            #[cfg(windows)]
            io::Error::from_raw_os_error(1),
            // An unassigned Unix errno exercises the same unmapped OS-error path.
            #[cfg(unix)]
            io::Error::from_raw_os_error(i32::MAX),
        ] {
            let hard_link_error = || match error.raw_os_error() {
                Some(code) => io::Error::from_raw_os_error(code),
                None => io::Error::from(error.kind()),
            };
            let dir = tempfile::tempdir().unwrap();
            let tmp = dir.path().join("bundle.tmp");
            let dest = dir.path().join("bundle.zip");
            fs::write(&tmp, "complete ZIP fixture").unwrap();
            publish_bundle(&tmp, &dest, |_, _| Err(hard_link_error())).unwrap();
            assert_eq!(fs::read_to_string(&dest).unwrap(), "complete ZIP fixture");
            assert!(!tmp.exists());
            fs::write(&tmp, "replacement").unwrap();
            let error = publish_bundle(&tmp, &dest, |_, _| Err(hard_link_error())).unwrap_err();
            assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
            assert_eq!(fs::read_to_string(&dest).unwrap(), "complete ZIP fixture");
            // A failed rename cleans up its reservation.
            let absent = dir.path().join("absent.tmp");
            let failed = dir.path().join("failed.zip");
            assert!(publish_bundle(&absent, &failed, |_, _| Err(hard_link_error())).is_err());
            assert!(!failed.exists());
        }

        let dir = tempfile::tempdir().unwrap();
        let tmp = dir.path().join("bundle.tmp");
        let dest = dir.path().join("bundle.zip");
        fs::write(&tmp, "complete ZIP fixture").unwrap();
        // AlreadyExists must propagate even if no destination is visible: no fallback.
        let error = publish_bundle(&tmp, &dest, |_, _| {
            Err(io::Error::from(io::ErrorKind::AlreadyExists))
        })
        .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
        assert!(!dest.exists());
        assert_eq!(fs::read_to_string(&tmp).unwrap(), "complete ZIP fixture");

        fs::write(&dest, "existing destination").unwrap();
        let error = publish_bundle(&tmp, &dest, |_, _| {
            Err(io::Error::from(io::ErrorKind::AlreadyExists))
        })
        .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
        assert_eq!(fs::read_to_string(&dest).unwrap(), "existing destination");
        assert_eq!(fs::read_to_string(&tmp).unwrap(), "complete ZIP fixture");
    }
}
