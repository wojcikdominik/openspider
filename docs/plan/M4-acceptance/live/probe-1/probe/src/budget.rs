//! Kostenbremse: je Anbieter feste Höchstzahl an Anfragen und Kostengrenze, dazu eine Gesamtgrenze. Vor jeder
//! Anfrage wird ihre Obergrenze geprüft, danach die geschätzten Ist-Kosten verbucht; ist eine Grenze erreicht, bricht
//! der Anbieter ab (die übrigen Schritte werden als übersprungen aufgezeichnet).

use crate::plan::{Provider, PROVIDER_CAP_USD};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Spent {
    pub requests: usize,
    pub usd: f64,
}

#[derive(Debug, Clone)]
pub struct Ledger {
    provider_cap: f64,
    total_cap: f64,
    total: f64,
    per: HashMap<Provider, Spent>,
}

impl Ledger {
    /// `total_cap` ist bereits auf höchstens 6 USD begrenzt (Option `--max-total-usd`).
    pub fn new(total_cap: f64) -> Self {
        Self {
            provider_cap: PROVIDER_CAP_USD,
            total_cap,
            total: 0.0,
            per: HashMap::new(),
        }
    }

    /// Für Tests: kleinere Anbietergrenze (nie größer als die Konstante).
    pub fn with_provider_cap(mut self, cap: f64) -> Self {
        self.provider_cap = cap.min(PROVIDER_CAP_USD);
        self
    }

    pub fn spent(&self, provider: Provider) -> Spent {
        self.per.get(&provider).copied().unwrap_or_default()
    }

    pub fn total_usd(&self) -> f64 {
        self.total
    }

    pub fn total_requests(&self) -> usize {
        self.per.values().map(|s| s.requests).sum()
    }

    pub fn provider_cap(&self) -> f64 {
        self.provider_cap
    }

    pub fn total_cap(&self) -> f64 {
        self.total_cap
    }

    /// Darf eine Anfrage mit dieser Kostenobergrenze noch gesendet werden?
    pub fn admit(&self, provider: Provider, worst_case_usd: f64) -> Result<(), String> {
        let s = self.spent(provider);
        if s.requests >= provider.max_requests() {
            return Err(format!(
                "Höchstzahl {} Anfragen für {} erreicht",
                provider.max_requests(),
                provider.name()
            ));
        }
        if s.usd + worst_case_usd > self.provider_cap {
            return Err(format!(
                "Kostenbremse {}: bisher {:.4} USD + Obergrenze dieser Anfrage {:.4} USD > {:.2} USD",
                provider.name(),
                s.usd,
                worst_case_usd,
                self.provider_cap
            ));
        }
        if self.total + worst_case_usd > self.total_cap {
            return Err(format!(
                "Gesamtgrenze: bisher {:.4} USD + Obergrenze dieser Anfrage {:.4} USD > {:.2} USD",
                self.total, worst_case_usd, self.total_cap
            ));
        }
        Ok(())
    }

    /// Verbucht eine gesendete Anfrage (`requests` HTTP-Anfragen) mit ihren geschätzten Kosten.
    pub fn charge(&mut self, provider: Provider, requests: usize, usd: f64) {
        let s = self.per.entry(provider).or_default();
        s.requests += requests;
        s.usd += usd;
        self.total += usd;
    }

    /// Ist nach der letzten Buchung eine Grenze erreicht?
    pub fn reached(&self, provider: Provider) -> Option<String> {
        let s = self.spent(provider);
        if s.usd >= self.provider_cap {
            Some(format!(
                "Kostenbremse {}: {:.4} USD ≥ {:.2} USD",
                provider.name(),
                s.usd,
                self.provider_cap
            ))
        } else if self.total >= self.total_cap {
            Some(format!(
                "Gesamtgrenze: {:.4} USD ≥ {:.2} USD",
                self.total, self.total_cap
            ))
        } else {
            None
        }
    }
}
