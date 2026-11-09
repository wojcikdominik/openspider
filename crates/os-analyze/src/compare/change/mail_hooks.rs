//! In os-cli::run_task eingebundener Mail-Hook; kein Abhängigkeitszyklus zu os-sched.
use super::{RunRecord, RunResult, Task, TaskHooks};
use os_sched::mail::completion::CompletionSender;
use std::{path::PathBuf, sync::Arc};

/// Abschlussmail-Anbindung für `run_in` / `run_with`.
pub struct ComparisonMailHooks {
    pub crawls: PathBuf,
    pub sender: Arc<dyn CompletionSender>,
}

impl TaskHooks for ComparisonMailHooks {
    fn notify(&self, task: &Task, record: &RunRecord) -> Result<(), String> {
        if !requested(task, record) {
            return Ok(());
        }
        let id = record
            .crawl_id
            .as_deref()
            .ok_or("Completion mail: no crawl available")?;
        let crawl = os_store::registry::list_crawls(&self.crawls)
            .map_err(|e| e.to_string())?
            .into_iter()
            .find(|c| c.meta.crawl_id.to_string() == id)
            .ok_or("Completion mail: crawl not found")?;
        let file = os_store::CrawlFile::open(&crawl.path, os_store::OpenMode::ReadOnly)
            .map_err(|e| e.to_string())?;
        let failed = matches!(record.result, RunResult::Failed | RunResult::Aborted);
        // OUT-281: Ein fehlgeschlagener Lauf hat oft keinen Vergleich (z. B. ein als `Failed` beendeter Crawl, CRAWL-128); die
        // Fehler-Mail geht dann ohne Änderungstabelle hinaus, statt ganz zu entfallen.
        let content = os_analyze::compare::change::notification::content(
            &file,
            &task.name,
            task.execution.auto_compare && !failed,
            task.effective_exports().skip_empty,
        )
        .map_err(|e| e.to_string())?;
        let (mut text, mut html) = (content.text, content.html);
        if failed {
            // Fehlermeldung des Laufs (OUT-281) vor dem Überblick.
            let message = record
                .message
                .clone()
                .unwrap_or_else(|| record.result.label().into());
            text = format!("Error: {message}\n{text}");
            html = format!("<p>Error: {}</p>{html}", html_escape(&message));
        }
        self.sender.send(
            &task.notify.to,
            // OUT-281: Betreff-Präfix `[FAILED]` wie bei der Abschlussmail ohne Vergleich.
            &format!(
                "{}{}: {}",
                if failed { "[FAILED] " } else { "" },
                task.name,
                record.result.label()
            ),
            &text,
            &html,
        )
    }
}

fn requested(task: &Task, record: &RunRecord) -> bool {
    !task.notify.to.is_empty()
        && match record.result {
            RunResult::Completed | RunResult::CompletedWithWarnings => task.notify.on_success,
            RunResult::Failed | RunResult::Aborted => task.notify.on_failure,
            _ => false,
        }
}

/// Standardversand für C02-Aufgaben: gespeichertes Konto statt injiziertem Transport.
pub fn notify_configured(task: &Task, record: &RunRecord) -> Result<(), String> {
    if !task.execution.auto_compare || !requested(task, record) {
        return Ok(());
    }
    let paths = os_store::paths::app_paths();
    let secrets = os_fetch::secret_store_from_env();
    let id = task
        .notify
        .account
        .as_deref()
        .ok_or("Completion mail: no SMTP account selected")?;
    let sender = os_sched::mail::completion::SmtpCompletionSender::from_account(
        &paths.settings,
        id,
        secrets.as_ref(),
    )?;
    ComparisonMailHooks {
        crawls: paths.crawls,
        sender: Arc::new(sender),
    }
    .notify(task, record)
}

fn html_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
