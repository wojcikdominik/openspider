// Sichtbarkeit des Statusleistenfelds `Kosten` (UI-013 [M4]; Plan M4 3.3, Nachtrag 7): nur mit Schalter „KI (Vorschau)“ an UND
// aktiven KI-Aufgaben (08) für den Crawl des Fensters.
//
// Festlegung M4-F02 (aus 08 und dem Konfigurationsmodell `os-model` `AiConfig`): KI-Aufgaben sind aktiv, wenn in der
// Konfiguration `ai.enabled = true` ist und mindestens eine Aufgabe gewählt ist – eine aktive Frage der Decision Engine
// (`ai.active_questions` nicht leer, AI-030 ff.) oder ein Vorschlagstyp (`ai.suggestions.title|meta|h1|alt`, AI-06x).
// Nutzerentscheidung 24.11.2026 (Plan M4.md 7, M4-U01): Embeddings (`embeddings.enabled`, 04) und eigene Prompts
// (`ai.prompts`, Paritäts-Prompts aus 04; ein Eintrag mit `enabled: false` zählt nicht) gelten ebenfalls als aktive KI-Aufgabe.
// Maßgeblich ist die Konfiguration, die `config.get` für das Fenster liefert: die des geöffneten bzw. laufenden Crawls, sonst
// der Entwurf des Fensters, sonst das Standardprofil. Bei Schalter aus läuft ein Crawl auf einer eingeschränkten Laufkopie
// (`ai.enabled = false`), dessen Konfiguration zeigt also keine aktiven KI-Aufgaben.
import { useEffect, useState } from "react";
import { config as configApi } from "../../../ipc/config";
import { useConfigDialog } from "../../config/frame/store";
import { useSession } from "../../../state/session";

/** Nachlese-Abstand bei eingeschaltetem Schalter. */
export const REFRESH_MS = 3000;

type Json = Record<string, unknown>;
const obj = (v: unknown): Json => (v !== null && typeof v === "object" && !Array.isArray(v) ? (v as Json) : {});

/** Sind in dieser Konfiguration (Abschnitte wie in `CrawlConfigDto.config`) KI-Aufgaben aktiv? */
export function aiTasksActive(config: unknown): boolean {
  const ai = obj(obj(config).ai);
  if (obj(obj(config).embeddings).enabled === true) return true;
  if (Array.isArray(ai.prompts) && ai.prompts.some((p) => obj(p).enabled !== false)) return true;
  if (ai.enabled !== true) return false;
  const questions = Array.isArray(ai.active_questions) && ai.active_questions.length > 0;
  const suggestions = obj(ai.suggestions);
  return questions || ["title", "meta", "h1", "alt"].some((k) => suggestions[k] === true);
}

/** Warnschwelle des Crawl-Budgets in Prozent (`ai.budget.warning_percent`, Standard 80, AI-013). */
export function warningPercent(config: unknown): number {
  const v = obj(obj(obj(config).ai).budget).warning_percent;
  return typeof v === "number" ? v : 80;
}

/**
 * Aktive KI-Aufgaben für den Crawl des Fensters; liest `config.get` nur, solange `enabled` (Schalter an) gilt. Neu gelesen wird
 * beim Einschalten, bei einem anderen Crawl oder Crawl-Zustand, nach dem Schließen des Konfigurationsdialogs (Speichern), wenn
 * das Fenster den Fokus zurückerhält und bei Schalter an alle [`REFRESH_MS`] ms.
 */
export function useAiTasksActive(enabled: boolean): boolean {
  return useAiWindowConfig(enabled).active;
}

/** Wie {@link useAiTasksActive}, dazu die Warnschwelle des Budgets und ein Zähler je Nachlesen (für abhängige Abfragen). */
export function useAiWindowConfig(enabled: boolean): { active: boolean; warningPercent: number; tick: number } {
  const [active, setActive] = useState(false);
  const [percent, setPercent] = useState(80);
  const crawlId = useSession((s) => s.crawl?.crawlId ?? null);
  const phase = useSession((s) => s.crawl?.phase ?? null);
  const dialogOpen = useConfigDialog((s) => s.open);
  const [focus, setFocus] = useState(0);

  useEffect(() => {
    const bump = () => setFocus((n) => n + 1);
    window.addEventListener("focus", bump);
    // Änderungen ohne Ereignis (z. B. `config.set` aus einem anderen Fenster): bei Schalter an alle 3 s nachlesen, solange das
    // Fenster sichtbar ist (ein lokaler Command ohne Netz).
    const timer = enabled
      ? setInterval(() => {
          if (typeof document === "undefined" || document.visibilityState !== "hidden") bump();
        }, REFRESH_MS)
      : null;
    return () => {
      window.removeEventListener("focus", bump);
      if (timer !== null) clearInterval(timer);
    };
  }, [enabled]);

  useEffect(() => {
    if (!enabled || dialogOpen) {
      if (!enabled) setActive(false);
      return;
    }
    const abort = new AbortController();
    configApi
      .get(abort.signal)
      .then((dto) => {
        if (!abort.signal.aborted) {
          setActive(aiTasksActive(dto.config));
          setPercent(warningPercent(dto.config));
        }
      })
      .catch(() => {
        if (!abort.signal.aborted) setActive(false);
      });
    return () => abort.abort();
  }, [enabled, crawlId, phase, dialogOpen, focus]);

  return { active: enabled && active, warningPercent: percent, tick: focus };
}
