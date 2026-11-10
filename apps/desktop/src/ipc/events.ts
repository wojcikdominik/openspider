// Event-Bus: ein Kanal je Fenster (M2-ui-concept 2.5). Der Kern bündelt alles in `Tick`; hier wird nur verteilt.
import type { IpcClient } from "./client";
import type { Notice } from "./gen/Notice";
import type { Tick } from "./gen/Tick";
import type { UiEvent } from "./gen/UiEvent";
import { events } from "./events.commands";

export type { UiEvent };

type Handlers = {
  tick: (tick: Tick) => void;
  settingsChanged: (keys: string[]) => void;
  notice: (notice: Notice) => void;
  focusUrl: (event: Extract<UiEvent, { kind: "focusUrl" }>) => void;
};

export interface EventBus {
  on<K extends keyof Handlers>(kind: K, cb: Handlers[K]): () => void;
  emit(event: UiEvent): void;
  /** Anzahl der Abonnenten (Tests). */
  listenerCount(kind?: keyof Handlers): number;
}

export function createEventBus(): EventBus {
  const sets: { [K in keyof Handlers]: Set<Handlers[K]> } = {
    tick: new Set(),
    settingsChanged: new Set(),
    notice: new Set(),
    focusUrl: new Set(),
  };
  return {
    on(kind, cb) {
      (sets[kind] as Set<typeof cb>).add(cb);
      return () => {
        (sets[kind] as Set<typeof cb>).delete(cb);
      };
    },
    emit(event) {
      switch (event.kind) {
        case "focusUrl":
          for (const cb of [...sets.focusUrl]) cb(event);
          break;
        case "tick":
          for (const cb of [...sets.tick]) cb(event.tick);
          break;
        case "settingsChanged":
          for (const cb of [...sets.settingsChanged]) cb(event.keys);
          break;
        case "notice":
          for (const cb of [...sets.notice]) cb(event.notice);
          break;
      }
    },
    listenerCount(kind) {
      return kind ? sets[kind].size : sets.tick.size + sets.settingsChanged.size + sets.notice.size + sets.focusUrl.size;
    },
  };
}

/** Der Bus dieses Fensters. */
export const eventBus: EventBus = createEventBus();

/**
 * Verbindet ein Fenster mit dem Kern: genau ein Kanal je Fenster (`events.subscribe`), alle Ereignisse laufen über den Bus.
 * Gibt die Abmeldung zurück.
 */
export function connectWindowEvents(client: IpcClient, windowLabel: string, bus: EventBus = eventBus): () => void {
  return client.subscribe(windowLabel, (e) => bus.emit(e));
}

/**
 * Meldet die Sichtbarkeit des Fensters (UI-143, erste von zwei Quellen; die zweite sind die Fensterereignisse im Rust-Teil).
 * Der aktuelle Stand wird sofort gemeldet: Beim Neuladen meldet das alte Dokument zuletzt „verdeckt“, und ohne eigene
 * Meldung bliebe das Fenster für den Kern verdeckt (Ticks höchstens 1 Hz statt im Live-Intervall, UI-018; B4-LAGFIX2).
 * Gibt die Abmeldung zurück.
 */
export function bindVisibility(windowLabel: string, doc: Document = document): () => void {
  const report = () => void events.setVisibility({ window: windowLabel, visible: doc.visibilityState === "visible" }).catch(() => undefined);
  doc.addEventListener("visibilitychange", report);
  report();
  return () => doc.removeEventListener("visibilitychange", report);
}
