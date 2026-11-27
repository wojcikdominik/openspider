//! Dublettenprüfung in zwei Stufen (AI-071, AI-075 Regel 3).
//!
//! * **Stufe 1** ([`SiteIndex::duplicate_of`]): je URL sofort gegen die bestehenden Werte aller **anderen
//!   indexierbaren** URLs des Crawls – unabhängig von der Verarbeitungsreihenfolge; löst Reparaturrunden aus.
//! * **Stufe 2** ([`post_pass`]): Nachlauf nach Abschluss des Laufs ohne Request: URLs in aufsteigender
//!   Adressreihenfolge (Bytevergleich), der erste gültige Kandidat wird reserviert, ein identischer Kandidat einer
//!   späteren URL erhält `duplicate_candidate:<frühere Adresse>`, der nächste gültige rückt nach; bleibt keiner, gilt
//!   `No valid suggestion`. Das Ergebnis hängt nicht davon ab, welcher Worker zuerst fertig war.
//!
//! Vergleich jeweils case-insensitiv nach Whitespace-Normalisierung ([`normalized`]).
use super::element::Element;
use super::validate::{collapse_whitespace, DUPLICATE_CANDIDATE};
use os_store::ai::suggestions::{ElementRecord, PageRow, STATUS_NO_VALID, STATUS_VALID};
use std::collections::{BTreeMap, HashMap};

/// Vergleichsform: Whitespace zusammengefasst, getrimmt, in Kleinbuchstaben.
pub fn normalized(text: &str) -> String {
    collapse_whitespace(text).to_lowercase()
}

/// Bestehende Werte und Adressen aller internen HTML-Seiten eines Crawls.
#[derive(Debug, Clone, Default)]
pub struct SiteIndex {
    addresses: BTreeMap<i64, String>,
    /// Normalisierter Wert → (Adresse, URL-ID) der indexierbaren URLs, aufsteigend nach Adresse (Bytes).
    values: HashMap<(Element, String), Vec<(String, i64)>>,
}

impl SiteIndex {
    /// Aus den Seiten der Crawl-Datei ([`os_store::ai::suggestions::pages`]).
    pub fn from_pages(pages: &[PageRow]) -> Self {
        let mut index = Self::default();
        for p in pages {
            index.addresses.insert(p.url_id, p.address.clone());
            if !p.indexable {
                continue;
            }
            for (element, value) in [
                (Element::Title, p.title.as_deref()),
                (Element::MetaDescription, p.meta_description.as_deref()),
            ] {
                let Some(value) = value.map(normalized).filter(|v| !v.is_empty()) else {
                    continue;
                };
                index
                    .values
                    .entry((element, value))
                    .or_default()
                    .push((p.address.clone(), p.url_id));
            }
        }
        for list in index.values.values_mut() {
            list.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));
        }
        index
    }

    /// Adresse einer URL.
    pub fn address(&self, url_id: i64) -> Option<&str> {
        self.addresses.get(&url_id).map(String::as_str)
    }

    /// Stufe 1: Adresse der (nach Bytes) ersten **anderen** indexierbaren URL mit demselben Wert.
    pub fn duplicate_of(&self, element: Element, url_id: i64, candidate: &str) -> Option<&str> {
        self.values
            .get(&(element, normalized(candidate)))?
            .iter()
            .find(|(_, id)| *id != url_id)
            .map(|(address, _)| address.as_str())
    }
}

/// Der sichtbare Kandidat ohne gültigen: der mit den wenigsten Verstößen, bei Gleichstand der erste (AI-075).
pub fn fewest_violations(record: &ElementRecord) -> Option<u32> {
    record
        .candidates
        .iter()
        .min_by_key(|c| (c.violations.len(), c.index))
        .map(|c| c.index)
}

/// Setzt Auswahl, Anzeigereihenfolge, Status und sichtbare Verstöße eines Eintrags aus der Gültigkeit seiner
/// Kandidaten (erster gültiger in Antwortreihenfolge, sonst der mit den wenigsten Verstößen).
pub fn select(record: &mut ElementRecord, finished: bool) {
    let selected = record
        .candidates
        .iter()
        .filter(|c| c.valid)
        .map(|c| c.index)
        .min();
    record.selected = selected;
    let visible = selected.or_else(|| fewest_violations(record));
    record.order = visible.into_iter().collect();
    record.violations = visible
        .and_then(|k| record.candidates.iter().find(|c| c.index == k))
        .map(|c| c.violations.clone())
        .unwrap_or_default();
    if selected.is_some() {
        record.status = STATUS_VALID.into();
    } else if finished {
        record.status = STATUS_NO_VALID.into();
    }
}

/// Stufe 2 über alle Einträge eines Elements (ohne Request). Einträge mit Kandidaten und Status `Valid` bzw.
/// `No valid suggestion` nehmen teil; Fehler-, Pending- und übersprungene Einträge bleiben unverändert. Am Ende ist
/// kein Eintrag mehr vorläufig (`provisional = false`). `duplicates = false` (Regel 3 abgeschaltet) entfernt nur
/// frühere Nachlauf-Verstöße. Rückgabe: Einträge in Adressreihenfolge.
pub fn post_pass(
    mut records: Vec<ElementRecord>,
    index: &SiteIndex,
    duplicates: bool,
) -> Vec<ElementRecord> {
    let address = |r: &ElementRecord| index.address(r.url_id).unwrap_or("").to_string();
    records.sort_by(|a, b| address(a).as_bytes().cmp(address(b).as_bytes()));
    let mut reserved: HashMap<String, String> = HashMap::new();
    for record in &mut records {
        // Offene Einträge (`Pending`, z. B. nach Stoppen) nehmen nicht teil und reservieren nichts.
        let takes_part = !record.candidates.is_empty()
            && (record.status == STATUS_VALID || record.status == STATUS_NO_VALID);
        if !takes_part {
            record.provisional = false;
            continue;
        }
        let own = address(record);
        for c in &mut record.candidates {
            c.violations
                .retain(|v| !v.starts_with(&format!("{DUPLICATE_CANDIDATE}:")));
            c.valid = c.violations.is_empty();
        }
        if duplicates {
            let mut ordered: Vec<usize> = (0..record.candidates.len()).collect();
            ordered.sort_by_key(|i| record.candidates[*i].index);
            for i in ordered {
                let c = &mut record.candidates[i];
                if !c.valid {
                    continue;
                }
                let key = normalized(&c.value);
                match reserved.get(&key) {
                    Some(earlier) => {
                        c.violations
                            .push(format!("{DUPLICATE_CANDIDATE}:{earlier}"));
                        c.valid = false;
                    }
                    None => {
                        reserved.insert(key, own.clone());
                        break;
                    }
                }
            }
        }
        record.provisional = false;
        select(record, true);
    }
    records
}
