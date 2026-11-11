// Liest Typ, Wertebereich und Aufzählungswerte eines Feldes aus dem JSON-Schema des Kerns (`config.schema`, OUT-221).
// Die Oberfläche kennt keine eigenen Wertebereiche: Grenzen kommen aus dem Schema, die Prüfung selbst macht `config.validate`.
import type { JsonValue } from "../../../ipc/gen/serde_json/JsonValue";

type Node = { [key: string]: unknown };

export interface FieldMeta {
  type: "boolean" | "integer" | "number" | "string" | "array" | "object" | "unknown";
  /** Wert darf `null` sein (nicht gesetzt = unbegrenzt bzw. automatisch). */
  nullable: boolean;
  minimum?: number;
  maximum?: number;
  enum?: readonly string[];
  /** Der Wert ist eine vorzeichenlose ganze Zahl (`format: uintN`). */
  unsigned?: boolean;
  /** Größter Wert, den der Typ im Kern aufnimmt (aus `format`), falls das Schema kein `maximum` nennt. */
  typeMaximum?: number;
  /** Werkseinstellung laut Schema. */
  default?: unknown;
  /** Elementtyp bei Listen. */
  items?: "string" | "object" | "unknown";
  /**
   * Schlüsselwörter neben einer Ganzzahl (`anyOf` aus Text-Enum und Ganzzahl, z. B. `"auto"` oder 1–6 bei
   * `speed.connections_per_host`, CRAWL-151); `type` ist dann `integer` mit den Grenzen der Ganzzahl.
   */
  choices?: readonly string[];
}

export type ConfigSchema = Node;

export function asSchema(raw: JsonValue | null | undefined): ConfigSchema | null {
  return raw !== null && typeof raw === "object" && !Array.isArray(raw) ? (raw as ConfigSchema) : null;
}

function resolve(schema: ConfigSchema, node: Node | undefined): Node | undefined {
  if (!node) return undefined;
  const ref = node.$ref;
  if (typeof ref !== "string") return node;
  const target = ref.replace(/^#\//, "").split("/").reduce<unknown>((acc, seg) => (acc as Node | undefined)?.[seg], schema) as Node | undefined;
  // Schlüssel am Verweis selbst (z. B. `default`, `description`) haben Vorrang vor dem Ziel.
  const { $ref: _ignored, ...own } = node;
  void _ignored;
  return target ? { ...target, ...own } : own;
}

/** Schema-Knoten eines Punktpfads (`limits.max_urls`); Verweise werden aufgelöst. */
export function nodeAt(schema: ConfigSchema | null, path: string): Node | undefined {
  if (!schema) return undefined;
  let node: Node | undefined = resolve(schema, schema);
  for (const seg of path.split(".")) {
    const props = node?.properties as Record<string, Node> | undefined;
    node = resolve(schema, props?.[seg]);
    if (!node) return undefined;
  }
  return node;
}

/** `anyOf` aus genau einem Text-Enum und einer Ganzzahl: Schlüsselwörter und Grenzen. */
function wordOrInt(schema: ConfigSchema, node: Node): FieldMeta | null {
  const alts = Array.isArray(node.anyOf) ? (node.anyOf as Node[]).map((a) => resolve(schema, a)) : null;
  if (!alts || alts.length !== 2) return null;
  const words = alts.find((a) => a?.type === "string" && Array.isArray(a.enum));
  const int = alts.find((a) => a?.type === "integer");
  if (!words || !int) return null;
  const meta: FieldMeta = { type: "integer", nullable: false, choices: (words.enum as unknown[]).filter((v): v is string => typeof v === "string") };
  if (typeof int.minimum === "number") meta.minimum = int.minimum;
  if (typeof int.maximum === "number") meta.maximum = int.maximum;
  if ("default" in node) meta.default = node.default;
  return meta;
}

export function fieldMeta(schema: ConfigSchema | null, path: string): FieldMeta | null {
  const node = nodeAt(schema, path);
  if (!node) return null;
  const choice = wordOrInt(schema as ConfigSchema, node);
  if (choice) return choice;
  const rawType = node.type;
  const types = Array.isArray(rawType) ? (rawType as string[]) : typeof rawType === "string" ? [rawType] : [];
  const nullable = types.includes("null");
  const main = types.find((t) => t !== "null");
  const en = Array.isArray(node.enum) ? (node.enum as string[]).filter((v) => typeof v === "string") : undefined;
  const itemsNode = resolve(schema as ConfigSchema, node.items as Node | undefined);
  const meta: FieldMeta = {
    type: en ? "string" : ((main as FieldMeta["type"]) ?? "unknown"),
    nullable,
  };
  const format = typeof node.format === "string" ? node.format : "";
  if (format.startsWith("uint")) {
    meta.unsigned = true;
    meta.typeMaximum = format === "uint8" ? 255 : format === "uint16" ? 65535 : format === "uint32" ? 4294967295 : Number.MAX_SAFE_INTEGER;
  }
  if (typeof node.minimum === "number") meta.minimum = node.minimum;
  if (typeof node.maximum === "number") meta.maximum = node.maximum;
  if (en) meta.enum = en;
  if ("default" in node) meta.default = node.default;
  if (main === "array") meta.items = itemsNode?.type === "string" ? "string" : itemsNode?.type === "object" || itemsNode?.properties ? "object" : "unknown";
  return meta;
}

/** Werkseinstellungen als Konfigurationsobjekt, abgeleitet aus den `default`-Angaben des Schemas (Testhilfe und Vorbelegung). */
export function defaultsFromSchema(schema: ConfigSchema): Record<string, unknown> {
  const build = (node: Node | undefined): unknown => {
    const n = resolve(schema, node);
    if (!n) return undefined;
    if ("default" in n) return n.default;
    const props = n.properties as Record<string, Node> | undefined;
    if (!props) return undefined;
    const out: Record<string, unknown> = {};
    for (const [k, v] of Object.entries(props)) {
      const val = build(v);
      if (val !== undefined) out[k] = val;
    }
    return out;
  };
  return (build(schema) as Record<string, unknown>) ?? {};
}
