//! Lokaler Lighthouse-Lauf (INTEG-132, Klärung K2-09: vorhandene Node-/Lighthouse-Installation über einen
//! konfigurierten Pfad; die Bündelung entscheidet M3).
//!
//! Jeder Lauf startet einen eigenen Lighthouse-Prozess (mit eigener Chrome-Instanz, getrennt vom Render-Browser des
//! Crawls) ohne Shell. Die Parallelität ist durch einen Semaphor begrenzt; ein Lauf, der das Timeout je URL
//! überschreitet, wird samt Prozess beendet. Bricht ein Prozess ab, erhält nur seine URL einen Fehler.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use os_model::{CrawlConfig, PsiDevice};
use tokio::sync::Semaphore;

use super::lighthouse::{parse_lighthouse, Outcome};
use crate::ApiError;

/// Status, wenn keine Laufzeit gefunden wird (INTEG-132).
pub const RUNTIME_MISSING: &str = "Fehler: Lighthouse-Laufzeit nicht gefunden";
/// Standard-Timeout je URL.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(90);
/// Erlaubter Bereich des Timeouts je URL in Sekunden.
pub const TIMEOUT_RANGE_S: std::ops::RangeInclusive<u64> = 30..=600;

/// Einstellungen des lokalen Laufs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalOptions {
    /// Parallele Lighthouse-Läufe (1 … ⌊CPU-Kerne/2⌋).
    pub parallel: u32,
    /// Timeout je URL.
    pub timeout: Duration,
    /// Pfad zur Lighthouse-CLI (`cli/index.js` bzw. ausführbare Datei).
    pub lighthouse: Option<PathBuf>,
    /// Pfad zu Node (sonst `node` aus `PATH`, nur für eine Skript-CLI).
    pub node: Option<PathBuf>,
}

impl Default for LocalOptions {
    fn default() -> Self {
        LocalOptions {
            parallel: 1,
            timeout: DEFAULT_TIMEOUT,
            lighthouse: None,
            node: None,
        }
    }
}

/// Höchstwert „Parallele Lighthouse-Läufe“ im Dialog: ⌊Kerne/2⌋, mindestens 1.
pub fn max_parallel(cores: usize) -> u32 {
    u32::try_from(cores / 2).unwrap_or(u32::MAX).max(1)
}

/// Kerne dieses Rechners.
pub fn cores() -> usize {
    std::thread::available_parallelism().map_or(1, usize::from)
}

impl LocalOptions {
    /// Optionen des Crawls (INTEG-132). `api.psi` trägt bisher weder Parallelität, Timeout noch Pfade zu
    /// Node/Lighthouse; es gelten die Standardwerte (Parallelität 1, Timeout 90 s, kein Pfad).
    pub fn from_config(_cfg: &os_model::config::PsiConfig) -> LocalOptions {
        LocalOptions::default()
    }

    /// Prüft Parallelität und Timeout gegen die Grenzen des Dialogs.
    pub fn validate(&self, cores: usize) -> Result<(), String> {
        if !(1..=max_parallel(cores)).contains(&self.parallel) {
            return Err(format!(
                "Parallele Lighthouse-Läufe: 1 bis {}",
                max_parallel(cores)
            ));
        }
        if !TIMEOUT_RANGE_S.contains(&self.timeout.as_secs()) {
            return Err("Timeout je URL: 30 bis 600 s".into());
        }
        Ok(())
    }
}

/// Ein Lighthouse-Lauf.
#[derive(Debug, Clone)]
pub struct Job {
    /// Zu prüfende URL.
    pub url: String,
    /// Gerätetyp (INTEG-131).
    pub device: PsiDevice,
    /// Datei mit den zusätzlichen Request-Headern (JSON-Objekt) oder `None`.
    pub headers_file: Option<PathBuf>,
    /// Ausgabedatei des JSON-Berichts.
    pub output: PathBuf,
}

/// Baut den Prozess eines Laufs. Die Standardumsetzung ist [`NodeLighthouse`]; Tests setzen einen Ersatzprozess ein.
pub trait Launcher: Send + Sync {
    /// Befehl für einen Lauf (ohne Shell).
    fn command(&self, job: &Job) -> Command;
    /// Mobiler Lauf; vorhandene Test-/Spezialläufer behalten standardmäßig ihr Verhalten.
    fn mobile_command(&self, job: &Job) -> Command {
        self.command(job)
    }
    /// Befehl, der die Lighthouse-Version auf stdout ausgibt.
    fn version_command(&self) -> Command;
}

/// Vorhandene Lighthouse-Installation: entweder `node <cli.js>` oder eine ausführbare Datei.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeLighthouse {
    /// Programm (Node oder Lighthouse selbst).
    pub program: PathBuf,
    /// Argumente vor den Lighthouse-Optionen (z. B. der Pfad der CLI).
    pub prefix: Vec<OsString>,
}

fn find_in_path(name: &str) -> Option<PathBuf> {
    let file = format!("{name}{}", std::env::consts::EXE_SUFFIX);
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths)
            .map(|dir| dir.join(&file))
            .find(|p| p.is_file())
    })
}

impl NodeLighthouse {
    /// Sucht die konfigurierte Laufzeit; ohne Pfad oder bei fehlender Datei [`RUNTIME_MISSING`].
    pub fn discover(opts: &LocalOptions) -> Result<NodeLighthouse, ApiError> {
        let missing = || ApiError::NotConnected(RUNTIME_MISSING.into());
        let cli = opts
            .lighthouse
            .as_ref()
            .filter(|p| p.is_file())
            .ok_or_else(missing)?;
        let script = cli
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| matches!(e, "js" | "mjs" | "cjs"));
        if !script {
            return Ok(NodeLighthouse {
                program: cli.clone(),
                prefix: Vec::new(),
            });
        }
        let node = match &opts.node {
            Some(node) => Some(node.clone()).filter(|p| p.is_file()),
            None => find_in_path("node"),
        }
        .ok_or_else(missing)?;
        Ok(NodeLighthouse {
            program: node,
            prefix: vec![cli.clone().into_os_string()],
        })
    }

    /// Lighthouse-Optionen eines Laufs (ohne Programm und Präfix).
    pub fn args(job: &Job) -> Vec<OsString> {
        let mut args: Vec<OsString> = vec![
            job.url.clone().into(),
            "--output=json".into(),
            {
                let mut o = OsString::from("--output-path=");
                o.push(&job.output);
                o
            },
            "--quiet".into(),
            "--only-categories=performance".into(),
            "--chrome-flags=--headless=new --no-first-run --no-default-browser-check".into(),
        ];
        match job.device {
            PsiDevice::Desktop => args.push("--preset=desktop".into()),
            _ => args.push("--form-factor=mobile".into()),
        }
        if let Some(file) = &job.headers_file {
            let mut o = OsString::from("--extra-headers=");
            o.push(file);
            args.push(o);
        }
        args
    }
}

impl Launcher for NodeLighthouse {
    fn command(&self, job: &Job) -> Command {
        let mut cmd = Command::new(&self.program);
        cmd.args(&self.prefix).args(Self::args(job));
        cmd
    }

    fn version_command(&self) -> Command {
        let mut cmd = Command::new(&self.program);
        cmd.args(&self.prefix).arg("--version");
        cmd
    }

    fn mobile_command(&self, job: &Job) -> Command {
        let mut cmd = Command::new(&self.program);
        cmd.args(&self.prefix).args(
            Self::args(job)
                .into_iter()
                .filter(|a| a != "--only-categories=performance"),
        );
        cmd.arg("--only-categories=performance,seo,accessibility");
        cmd
    }
}

/// Request-Header der Crawl-Konfiguration für eine URL (CRAWL-115): Einträge ohne Host gelten überall, sonst nur für
/// den genannten Host (INTEG-132: Authentifizierung wird übernommen).
pub fn config_headers(cfg: &CrawlConfig, url: &str) -> Vec<(String, String)> {
    let host = url::Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(str::to_ascii_lowercase));
    cfg.http
        .headers
        .iter()
        .filter(|h| !h.name.trim().is_empty())
        .filter(|h| {
            h.host
                .as_deref()
                .is_none_or(|want| host.as_deref() == Some(&want.to_ascii_lowercase()))
        })
        .map(|h| (h.name.clone(), h.value.clone()))
        .collect()
}

/// Header für HTTP-Basic-Auth.
pub fn basic_auth_header(user: &str, password: &str) -> (String, String) {
    use base64::Engine;
    let token = base64::engine::general_purpose::STANDARD.encode(format!("{user}:{password}"));
    ("Authorization".into(), format!("Basic {token}"))
}

/// Erste Versionsangabe `x.y.z` in einer Ausgabe.
pub fn parse_version(stdout: &str) -> Option<String> {
    stdout.split_whitespace().find_map(|tok| {
        let tok = tok.trim_start_matches('v');
        let parts: Vec<&str> = tok.split('.').collect();
        (parts.len() == 3
            && parts
                .iter()
                .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit())))
        .then(|| tok.to_string())
    })
}

/// Führt Lighthouse-Läufe mit begrenzter Parallelität aus.
pub struct LocalRunner {
    launcher: Arc<dyn Launcher>,
    permits: Semaphore,
    timeout: Duration,
    active: AtomicUsize,
    peak: AtomicUsize,
    runs: AtomicU64,
    run_ms: AtomicU64,
}

/// Zählt einen laufenden Prozess; beendet ihn beim Abbruch des Futures.
struct Running<'a> {
    runner: &'a LocalRunner,
    child: Option<std::process::Child>,
}

impl Drop for Running<'_> {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        self.runner.active.fetch_sub(1, Ordering::SeqCst);
    }
}

static NEXT_FILE: AtomicU64 = AtomicU64::new(0);

/// Eindeutiger Pfad im temporären Verzeichnis.
fn temp_path(stem: &str) -> PathBuf {
    let n = NEXT_FILE.fetch_add(1, Ordering::SeqCst);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    std::env::temp_dir().join(format!(
        "openspider-lh-{}-{nanos}-{n}-{stem}",
        std::process::id()
    ))
}

impl LocalRunner {
    /// Läufer mit `opts.parallel` gleichzeitigen Prozessen und `opts.timeout` je URL.
    pub fn new(launcher: Arc<dyn Launcher>, opts: &LocalOptions) -> LocalRunner {
        LocalRunner {
            launcher,
            permits: Semaphore::new(opts.parallel.max(1) as usize),
            timeout: opts.timeout,
            active: AtomicUsize::new(0),
            peak: AtomicUsize::new(0),
            runs: AtomicU64::new(0),
            run_ms: AtomicU64::new(0),
        }
    }

    /// Läufer gegen die konfigurierte Installation; ohne Laufzeit [`RUNTIME_MISSING`].
    pub fn discover(opts: &LocalOptions) -> Result<LocalRunner, ApiError> {
        Ok(LocalRunner::new(
            Arc::new(NodeLighthouse::discover(opts)?),
            opts,
        ))
    }

    /// Gerade laufende Lighthouse-Prozesse.
    pub fn active(&self) -> usize {
        self.active.load(Ordering::SeqCst)
    }

    /// Höchste bisher gleichzeitig laufende Zahl.
    pub fn peak(&self) -> usize {
        self.peak.load(Ordering::SeqCst)
    }

    /// Mittlere Laufzeit je URL (für die Restzeit, INTEG-142); `None` vor dem ersten Lauf.
    pub fn mean_run(&self) -> Option<Duration> {
        let n = self.runs.load(Ordering::SeqCst);
        (n > 0).then(|| Duration::from_millis(self.run_ms.load(Ordering::SeqCst) / n))
    }

    /// Lighthouse-Version der Installation (Kontodialog, INTEG-132).
    pub async fn version(&self) -> Result<String, ApiError> {
        let mut cmd = self.launcher.version_command();
        cmd.stdin(Stdio::null()).stderr(Stdio::null());
        let out = tokio::task::spawn_blocking(move || cmd.output())
            .await
            .map_err(|_| ApiError::NotConnected(RUNTIME_MISSING.into()))?
            .map_err(|_| ApiError::NotConnected(RUNTIME_MISSING.into()))?;
        parse_version(&String::from_utf8_lossy(&out.stdout))
            .ok_or_else(|| ApiError::NotConnected(RUNTIME_MISSING.into()))
    }

    /// Ein Lauf für `url`. Fehler (Start, Timeout, Absturz, Laufzeitfehler) sind Ergebnisse mit `PSI Status = Error`.
    pub async fn run(&self, url: &str, device: PsiDevice, headers: &[(String, String)]) -> Outcome {
        self.run_selected(url, device, headers, false).await
    }

    /// Lauf mit zusätzlicher Mobile-Audit-Auswahl (TECH-230, INTEG-140).
    pub async fn run_selected(
        &self,
        url: &str,
        device: PsiDevice,
        headers: &[(String, String)],
        mobile: bool,
    ) -> Outcome {
        let Ok(_permit) = self.permits.acquire().await else {
            return Outcome::error("LIGHTHOUSE_CLOSED Lighthouse-Läufer beendet");
        };
        let output = temp_path("report.json");
        let headers_file = if headers.is_empty() {
            None
        } else {
            let map: serde_json::Map<String, serde_json::Value> = headers
                .iter()
                .map(|(k, v)| (k.clone(), serde_json::Value::String(v.clone())))
                .collect();
            let path = temp_path("headers.json");
            if std::fs::write(&path, serde_json::Value::Object(map).to_string()).is_err() {
                return Outcome::error("LIGHTHOUSE_START_FAILED Header-Datei nicht schreibbar");
            }
            Some(path)
        };
        let job = Job {
            url: url.to_string(),
            device,
            headers_file: headers_file.clone(),
            output: output.clone(),
        };
        let started = Instant::now();
        let outcome = if mobile {
            self.execute_mobile(&job).await
        } else {
            self.execute(&job).await
        };
        let ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
        self.runs.fetch_add(1, Ordering::SeqCst);
        self.run_ms.fetch_add(ms, Ordering::SeqCst);
        for path in [Some(output), headers_file].into_iter().flatten() {
            let _ = std::fs::remove_file(path);
        }
        outcome
    }

    async fn execute(&self, job: &Job) -> Outcome {
        let mut cmd = self.launcher.command(job);
        self.execute_command(job, &mut cmd).await
    }

    async fn execute_mobile(&self, job: &Job) -> Outcome {
        let mut cmd = self.launcher.mobile_command(job);
        self.execute_command(job, &mut cmd).await
    }

    async fn execute_command(&self, job: &Job, cmd: &mut Command) -> Outcome {
        cmd.stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let child = match cmd.spawn() {
            Ok(child) => child,
            Err(_) => {
                return Outcome::error("LIGHTHOUSE_START_FAILED Lighthouse-Prozess nicht startbar")
            }
        };
        let now = self.active.fetch_add(1, Ordering::SeqCst) + 1;
        self.peak.fetch_max(now, Ordering::SeqCst);
        let mut running = Running {
            runner: self,
            child: Some(child),
        };
        let deadline = Instant::now() + self.timeout;
        let status = loop {
            let child = running.child.as_mut().expect("child");
            match child.try_wait() {
                Ok(Some(status)) => break Some(status),
                Ok(None) if Instant::now() >= deadline => break None,
                Ok(None) => tokio::time::sleep(Duration::from_millis(25)).await,
                Err(_) => break None,
            }
        };
        let Some(status) = status else {
            // Timeout: Prozess (und mit ihm Chrome) beenden, bevor die nächste URL startet.
            if let Some(mut child) = running.child.take() {
                let _ = child.kill();
                let _ = child.wait();
            }
            drop(running);
            return Outcome::error("Timeout");
        };
        running.child = None;
        drop(running);
        match read_report(&job.output) {
            Some(lhr) => parse_lighthouse(&lhr),
            None if status.success() => {
                Outcome::error("LIGHTHOUSE_NO_REPORT Lighthouse lieferte keinen Bericht")
            }
            None => Outcome::error(format!(
                "LIGHTHOUSE_CRASHED Lighthouse-Prozess abgebrochen ({})",
                status
                    .code()
                    .map_or_else(|| "Signal".to_string(), |c| format!("Exit {c}"))
            )),
        }
    }
}

fn read_report(path: &Path) -> Option<serde_json::Value> {
    let bytes = std::fs::read(path).ok()?;
    let value: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    // Die CLI schreibt das LHR direkt; eine PSI-Hülle wird ebenfalls akzeptiert.
    Some(value.get("lighthouseResult").cloned().unwrap_or(value))
}
