//! Szenario `hold` (08 Konventionen, Nachtrag 26.11.2026, K4-13): Der Mock nimmt die nächsten n Requests eines
//! Anbieters an, vermerkt sie sofort im Request-Protokoll (`held: true`) und beantwortet sie erst nach
//! `POST /__mock/release`. Die Antwort ist dann die, die ohne `hold` gekommen wäre. Bricht der Client die Verbindung
//! ab, wird der gehaltene Request verworfen und mit `aborted: true` protokolliert; er zählt bei `release` nicht.
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use tokio::sync::oneshot;

use super::Data;

/// Gehaltener Request.
struct Held {
    provider: String,
    /// Position im Request-Protokoll.
    log_index: usize,
    release: oneshot::Sender<()>,
}

/// Zustand der Funktion `hold`.
#[derive(Default)]
pub(super) struct Holds {
    /// Noch zu haltende Requests je Anbieter.
    remaining: HashMap<String, usize>,
    /// Gehaltene Requests in Eingangsreihenfolge.
    queue: Vec<Held>,
}

impl Holds {
    /// `POST /__mock/scenario {"scenario": "hold", "count": n}`: die nächsten `count` Requests des Anbieters halten.
    pub(super) fn arm(&mut self, provider: &str, count: usize) {
        self.remaining.insert(provider.to_string(), count);
    }

    /// Gibt die `count` ältesten gehaltenen Requests des Anbieters frei (`None` = alle); Rückgabe: Anzahl.
    pub(super) fn release(&mut self, provider: &str, count: Option<usize>) -> usize {
        let mut released = 0;
        let mut index = 0;
        while index < self.queue.len() && count.is_none_or(|c| released < c) {
            if self.queue[index].provider == provider {
                let held = self.queue.remove(index);
                // Ein schon verworfener Empfänger zählt nicht als freigegeben.
                if held.release.send(()).is_ok() {
                    released += 1;
                }
            } else {
                index += 1;
            }
        }
        released
    }

    /// Ob der nächste Request dieses Anbieters gehalten wird (verbraucht einen Platz).
    fn take(&mut self, provider: &str) -> bool {
        match self.remaining.get_mut(provider) {
            Some(n) if *n > 0 => {
                *n -= 1;
                true
            }
            _ => false,
        }
    }
}

/// Vermerkt beim Verwerfen eines noch gehaltenen Requests `aborted: true` und entfernt ihn aus der Warteschlange.
struct AbortGuard {
    data: Arc<Mutex<Data>>,
    log_index: usize,
    armed: bool,
}

impl Drop for AbortGuard {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        let mut data = self.data.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(record) = data.log.get_mut(self.log_index) {
            record.aborted = true;
        }
        let index = self.log_index;
        data.holds.queue.retain(|h| h.log_index != index);
    }
}

/// Hält den eben protokollierten Request `log_index`, falls für `provider` ein `hold` aktiv ist, bis zur Freigabe.
/// Wird der Aufrufer vorher verworfen (Client hat die Verbindung abgebrochen), vermerkt der Wächter `aborted`.
pub(super) async fn maybe_hold(data: &Arc<Mutex<Data>>, provider: &str, log_index: usize) {
    let receiver = {
        let mut d = data.lock().unwrap_or_else(|p| p.into_inner());
        if !d.holds.take(provider) {
            return;
        }
        if let Some(record) = d.log.get_mut(log_index) {
            record.held = true;
        }
        let (release, receiver) = oneshot::channel();
        d.holds.queue.push(Held {
            provider: provider.to_string(),
            log_index,
            release,
        });
        receiver
    };
    let mut guard = AbortGuard {
        data: data.clone(),
        log_index,
        armed: true,
    };
    let _ = receiver.await;
    guard.armed = false;
}
