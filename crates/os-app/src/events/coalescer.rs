//! One clock and one budget per window, including operations and hidden windows.
use crate::{events::MIN_TICK_INTERVAL_MS, runtime::hooks::TickPolicy, App, WindowId};
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
    time::Duration,
};

/// Tick budget for counters and status, capped at one second (K2-39).
/// Unlike `App::window_period`, this deadline does not follow table intervals
/// above one second. The session feed publishes row updates on the ticks of
/// this clock (at most once per configured live interval); hidden windows
/// receive at most one tick/s.
#[derive(Default)]
pub struct Coalescer {
    last: Mutex<HashMap<WindowId, Duration>>,
    /// B4-LAGFIX2 (UI-018): one prompt tick for visible windows after a crawl was attached, so the
    /// window learns the new crawl before its first URLs arrive. Still ≥ 100 ms apart (PERF-004).
    expedite: AtomicBool,
}

impl Coalescer {
    /// Due windows at `now`; `commit` starts their next period (and the clock of newly seen windows).
    fn select(&self, app: &App, commit: bool) -> Vec<WindowId> {
        let now = app.config().clock.monotonic();
        let windows = app.subscribed_windows();
        let mut last = self.last.lock().unwrap_or_else(|p| p.into_inner());
        last.retain(|w, _| windows.contains(w));
        let expedite = self.expedite.load(Ordering::SeqCst);
        let due: Vec<WindowId> = windows
            .iter()
            .filter(|w| {
                let visible = app.is_visible(w);
                let period = if !visible {
                    Duration::from_secs(1)
                } else if expedite {
                    Duration::from_millis(u64::from(MIN_TICK_INTERVAL_MS))
                } else {
                    Duration::from_millis(u64::from(app.live_interval_ms().clamp(100, 1000)))
                };
                match last.get(*w) {
                    Some(previous) => now.saturating_sub(*previous) >= period,
                    // A newly seen window starts its clock; only an expedited tick is sent at once.
                    None => expedite && visible,
                }
            })
            .cloned()
            .collect();
        // A newly seen window starts its clock here, also on a preview: the session loop flushes only due
        // windows. An expedited first tick keeps no entry until it is sent.
        for w in windows {
            if !due.contains(&w) {
                last.entry(w).or_insert(now);
            }
        }
        if commit {
            for w in &due {
                last.insert(w.clone(), now);
            }
            if !due.is_empty() {
                self.expedite.store(false, Ordering::SeqCst);
            }
        }
        due
    }
}

impl TickPolicy for Coalescer {
    fn due_windows(&self, app: &App) -> Vec<WindowId> {
        self.select(app, true)
    }

    fn any_due(&self, app: &App) -> bool {
        app.subscribed_windows().is_empty() || !self.select(app, false).is_empty()
    }

    fn expedite(&self) {
        self.expedite.store(true, Ordering::SeqCst);
    }
}
