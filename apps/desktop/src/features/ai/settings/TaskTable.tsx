// Tabelle Aufgabe → Modell (AI-011) im Konfigurationsdialog, Seite „KI“: Modell, Spalte „Reasoning“ (Dropdown mit den für das
// gewählte Modell verfügbaren Stufen und `model-default`, Nachtrag 24.11.2026), Ausgabegrenze (Hinweis „knapp“), geordnete
// Ersatzliste (Ziehen und Ablegen, dazu Auf/Ab für die Tastatur), Status und „Auf Standard zurücksetzen“. Die Werte stehen im
// Entwurf (`ai.tasks.<aufgabe>`, Teil von Profil, Export und Import); die Zeilen rechnet der Kern (`ai.settingsSet` mit dem
// Entwurf), die Oberfläche rechnet nichts nach.
import { useEffect, useState } from "react";
import { useT, type TFn } from "../../../i18n";
import { toCmdError } from "../../../ipc/errors";
import { Button, Select, TextField } from "../../../ui/primitives";
import { useConfig } from "../../config/frame/context";
import { aiApi, type Level, type ModelOption, type ModelRef, type TaskRow } from "./api";

type Json = Record<string, unknown>;
const obj = (v: unknown): Json => (v !== null && typeof v === "object" && !Array.isArray(v) ? (v as Json) : {});

/** Schlüssel der Auswahl „Standard“ bzw. „keine Auswahl“ in den Dropdowns. */
const DEFAULT = "__default";
const NONE = "__none";

export const refKey = (r: ModelRef): string => `${r.provider}|${r.model}|${r.endpoint}`;
export const refLabel = (r: ModelRef): string => `${r.provider} · ${r.model} (${r.endpoint})`;
const fromKey = (k: string): ModelRef => {
  const [provider, model, endpoint] = k.split("|");
  return { provider, model, endpoint } as ModelRef;
};

export const levelLabel = (t: TFn, l: Level): string => t(`ai-level-${l}`);

/** Text der Warnung der Spalte „Reasoning“. */
export function warningText(t: TFn, row: TaskRow): string | null {
  const w = row.reasoning?.warning;
  if (!w) return null;
  if (w.kind === "off-unavailable") return t("ai-task-warn-off", { effective: levelLabel(t, w.effective) });
  return t("ai-task-warn-stored", { stored: levelLabel(t, w.stored), effective: levelLabel(t, w.effective) });
}

function useRows(draftAi: unknown): { rows: TaskRow[] | null; error: string | null } {
  const key = JSON.stringify(obj(draftAi).tasks ?? {});
  const [state, setState] = useState<{ rows: TaskRow[] | null; error: string | null }>({ rows: null, error: null });
  useEffect(() => {
    let alive = true;
    const id = setTimeout(() => {
      aiApi
        .evaluate(draftAi)
        .then((r) => alive && setState({ rows: r.tasks, error: null }))
        .catch((e: unknown) => {
          if (!alive) return;
          setState((s) => ({ rows: s.rows, error: toCmdError(e).messageKey }));
        });
    }, 120);
    return () => {
      alive = false;
      clearTimeout(id);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps -- Vergleich über den serialisierten Abschnitt
  }, [key]);
  return state;
}

function FallbackList({ row, onChange }: { row: TaskRow; onChange: (next: ModelRef[]) => void }) {
  const t = useT();
  const [drag, setDrag] = useState<number | null>(null);
  const list = row.effectiveFallback;
  const move = (from: number, to: number) => {
    if (to < 0 || to >= list.length || from === to) return;
    const next = [...list];
    const [item] = next.splice(from, 1);
    next.splice(to, 0, item);
    onChange(next);
  };
  const candidates = row.models.filter((m) => m.assignable && !list.some((r) => refKey(r) === refKey(m)) && (!row.effectiveModel || refKey(row.effectiveModel) !== refKey(m)));
  return (
    <div className="os-ai-fallback" data-ai-fallback>
      <ol aria-label={t("ai-task-fallback-list", { task: row.task })}>
        {list.map((r, i) => (
          <li
            key={refKey(r)}
            draggable
            onDragStart={() => setDrag(i)}
            onDragOver={(e) => e.preventDefault()}
            onDrop={() => {
              if (drag !== null) move(drag, i);
              setDrag(null);
            }}
            data-model={refKey(r)}
          >
            <span>{refLabel(r)}</span>
            <Button variant="tool" aria-label={t("ai-task-fallback-up", { model: r.model })} isDisabled={i === 0} onPress={() => move(i, i - 1)}>↑</Button>
            <Button variant="tool" aria-label={t("ai-task-fallback-down", { model: r.model })} isDisabled={i === list.length - 1} onPress={() => move(i, i + 1)}>↓</Button>
            <Button variant="tool" aria-label={t("ai-task-fallback-remove", { model: r.model })} onPress={() => onChange(list.filter((_, j) => j !== i))}>×</Button>
          </li>
        ))}
      </ol>
      {candidates.length > 0 && (
        <Select
          label={t("ai-task-fallback-add")}
          value={NONE}
          options={[{ value: NONE, label: "–" }, ...candidates.map((m) => ({ value: refKey(m), label: refLabel(m) }))]}
          onChange={(k) => k !== NONE && onChange([...list, fromKey(k)])}
        />
      )}
    </div>
  );
}

function MaxTokens({ row, onChange }: { row: TaskRow; onChange: (v: number | null) => void }) {
  const t = useT();
  const m = row.maxTokens;
  const [text, setText] = useState(m?.user != null ? String(m.user) : "");
  useEffect(() => setText(m?.user != null ? String(m.user) : ""), [m?.user]);
  if (!m) return <span>–</span>;
  const hint = m.standard !== null ? t("ai-task-max-standard", { standard: m.standard }) : t("ai-task-max-formula");
  return (
    <div>
      <TextField
        label={t("ai-task-max-tokens", { task: row.task })}
        value={text}
        inputMode="numeric"
        description={hint}
        onChange={(v) => {
          setText(v);
          if (v.trim() === "") onChange(null);
          else if (/^\d+$/.test(v.trim()) && Number(v) > 0) onChange(Number(v));
        }}
      />
      {m.tight && <p className="os-hint os-ai-warning" role="note" data-ai-tight>{t("ai-task-max-tight")}</p>}
    </div>
  );
}

function modelOptions(t: TFn, row: TaskRow): { value: string; label: string }[] {
  const def = row.defaultModel ? refLabel(row.defaultModel) : t("ai-task-no-default");
  return [
    { value: DEFAULT, label: t("ai-task-model-default", { model: def }) },
    ...row.models.filter((m) => m.assignable).map((m) => ({ value: refKey(m), label: refLabel(m) })),
  ];
}

function unassignable(row: TaskRow): ModelOption[] {
  return row.models.filter((m) => !m.assignable);
}

function TaskLine({ row, setTask, reset }: { row: TaskRow; setTask: (change: Json) => void; reset: () => void }) {
  const t = useT();
  const r = row.reasoning;
  const warning = warningText(t, row);
  const blocked = unassignable(row);
  return (
    <tr data-task={row.task}>
      <th scope="row">{row.task}</th>
      <td>
        <Select
          label={t("ai-task-model", { task: row.task })}
          value={row.model ? refKey(row.model) : DEFAULT}
          options={modelOptions(t, row)}
          onChange={(k) => {
            if (k === DEFAULT) setTask({ model: null, fallback: [] });
            else setTask({ model: fromKey(k), fallback: row.model ? row.fallback : row.defaultFallback });
          }}
        />
        {blocked.length > 0 && (
          <p className="os-hint" data-ai-unassignable>
            {t("ai-task-unassignable", { models: blocked.map((m) => `${m.model} (${m.missing.join(", ")})`).join("; ") })}
          </p>
        )}
        {row.assignmentError && <p className="os-field-error" role="alert">{t("ai-task-incapable", { missing: row.assignmentError.join(", ") })}</p>}
      </td>
      <td data-col="Reasoning">
        {r ? (
          <>
            <Select
              label={t("ai-task-reasoning", { task: row.task })}
              value={r.stored ?? DEFAULT}
              options={[
                { value: DEFAULT, label: t("ai-task-reasoning-default", { level: levelLabel(t, r.requested) }) },
                ...r.dropdown.map((l) => ({ value: l, label: levelLabel(t, l) })),
              ]}
              onChange={(v) => setTask({ effort: v === DEFAULT ? null : v })}
            />
            <p className="os-hint" data-ai-effective={r.effective}>{t("ai-task-effective", { level: levelLabel(t, r.effective) })}</p>
            {!r.levelsKnown && <p className="os-hint" data-ai-levels-unknown>{t("ai-task-levels-unknown")}</p>}
            {warning && <p className="os-hint os-ai-warning" role="note" data-ai-warning={r.warning?.kind}>{warning}</p>}
          </>
        ) : (
          <span>{t("ai-task-no-level")}</span>
        )}
      </td>
      <td>
        <MaxTokens row={row} onChange={(v) => setTask({ max_tokens: v })} />
      </td>
      <td>
        <FallbackList row={row} onChange={(next) => setTask({ model: row.effectiveModel, fallback: next })} />
      </td>
      <td data-col="Status">
        {row.status ?? (row.plannedModel ? refLabel(row.plannedModel) : "")}
      </td>
      <td>
        <Button isDisabled={row.isDefault} onPress={reset}>{t("ai-task-reset")}</Button>
      </td>
    </tr>
  );
}

/** Darstellung der Tabelle (ohne Entwurf; Tests rufen sie mit Fixture-Zeilen auf). */
export function TaskTableView({ rows, error, onSetTask, onReset }: { rows: TaskRow[] | null; error?: string | null; onSetTask: (task: string, change: Json) => void; onReset: (task: string) => void }) {
  const t = useT();
  return (
    <section aria-labelledby="ai-tasks-heading" data-ai-tasks data-setting-anchor="ai.tasks">
      <h4 id="ai-tasks-heading" className="os-settings-heading">{t("ai-tasks-title")}</h4>
      <p className="os-hint">{t("ai-tasks-hint")}</p>
      {error && <p className="os-field-error" role="alert">{t("ai-tasks-error")}</p>}
      {rows && (
        <div className="os-ai-scroll" role="region" aria-label={t("ai-tasks-title")} tabIndex={0}>
        <table className="os-ai-table os-ai-tasks">
          <thead>
            <tr>
              <th scope="col">{t("ai-col-task")}</th>
              <th scope="col">{t("ai-col-model")}</th>
              <th scope="col">{t("ai-col-reasoning")}</th>
              <th scope="col">{t("ai-col-max-tokens")}</th>
              <th scope="col">{t("ai-col-fallback")}</th>
              <th scope="col">{t("ai-col-status")}</th>
              <th scope="col"><span className="os-visually-hidden">{t("ai-col-actions")}</span></th>
            </tr>
          </thead>
          <tbody>
            {rows.map((row) => <TaskLine key={row.task} row={row} setTask={(c) => onSetTask(row.task, c)} reset={() => onReset(row.task)} />)}
          </tbody>
        </table>
        </div>
      )}
    </section>
  );
}

/** Neuer Wert von `ai.tasks` nach einer Änderung einer Aufgabe (Form `TaskModelConfig` des Profils). */
export function patchTasks(tasks: Json, task: string, change: Json): Json {
  const base = { model: null, fallback: [], max_tokens: null, temperature: null, effort: null, max_input_chars: null, ...obj(tasks[task]) };
  return { ...tasks, [task]: { ...base, ...change } };
}

/** `ai.tasks` ohne die Aufgabe („Auf Standard zurücksetzen“). */
export function resetTask(tasks: Json, task: string): Json {
  const next = { ...tasks };
  delete next[task];
  return next;
}

/** Tabelle Aufgabe → Modell auf der Konfigurationsseite „KI“. */
export function TaskTable() {
  const { draft } = useConfig();
  const draftAi = draft.get<unknown>("ai");
  const { rows, error } = useRows(draftAi);
  const tasks = obj(obj(draftAi).tasks);
  return (
    <TaskTableView
      rows={rows}
      error={error}
      onSetTask={(task, change) => draft.set("ai.tasks", patchTasks(tasks, task, change))}
      onReset={(task) => draft.set("ai.tasks", resetTask(tasks, task))}
    />
  );
}
