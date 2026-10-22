import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { createMarkdownRenderer, defineLoader } from "vitepress";

const schemaPath = fileURLToPath(new URL("../../data/config.schema.json", import.meta.url));
const panelSchemaPath = fileURLToPath(new URL("../../data/panel.schema.json", import.meta.url));

const hiddenApplets = new Set(["heartbeat", "widget-demo"]);


type Json = Record<string, any>;

export interface Value {
  value: string;
  description?: string;
}

export interface Option {
  key: string;
  type: string;
  default?: string;
  defaultToml?: string;
  description: string;
  values?: Value[];
  table?: string;
}

export interface Table {
  path: string;
  array?: boolean;
  description?: string;
  options: Option[];
}

export interface Data {
  tables: Record<string, Table>;
  applets: { name: string; description: string }[];
}

declare const data: Data;
export { data };

export default defineLoader({
  watch: [schemaPath, panelSchemaPath],
  async load(): Promise<Data> {
    const config: Json = JSON.parse(readFileSync(schemaPath, "utf8"));
    const panel: Json = JSON.parse(readFileSync(panelSchemaPath, "utf8"));
    const schema: Json = {
      ...config,
      properties: { ...config.properties, ...panel.properties },
      $defs: { ...config.$defs, ...panel.$defs },
    };
    const md = await createMarkdownRenderer(process.cwd());
    const defs: Json = schema.$defs;
    const tables: Record<string, Table> = {};
    const missing: string[] = [];

    const text = (raw: string) => raw.replace(/([^\n])\n(?!\n)/g, "$1 ");
    const block = (raw?: string) => (raw ? md.render(text(raw)) : undefined);
    const inline = (raw?: string) => (raw ? md.renderInline(text(raw)) : undefined);

    const deref = (node: Json): Json => {
      if (node?.$ref) return { ...defs[node.$ref.split("/").pop()], ...withoutRef(node) };
      return node ?? {};
    };
    const withoutRef = ({ $ref, ...rest }: Json) => rest;

    const literal = (value: unknown): string => {
      if (typeof value === "string") return value.includes("\n") ? value : JSON.stringify(value);
      if (Array.isArray(value)) {
        if (value.length === 0) return "[]";
        if (value.every((item) => typeof item !== "object")) return `[${value.map(literal).join(", ")}]`;
        return "see below";
      }
      if (value && typeof value === "object") {
        const entries = Object.entries(value);
        if (entries.length === 0) return "{}";
        if (entries.every(([, item]) => typeof item !== "object")) {
          return `{ ${entries.map(([key, item]) => `${key} = ${literal(item)}`).join(", ")} }`;
        }
        return "see below";
      }
      return String(value);
    };

    const toml = (entry: Json): string =>
      Object.entries(entry)
        .filter(([, value]) => value !== null && value !== undefined)
        .map(([key, value]) => `${key} = ${typeof value === "string" ? JSON.stringify(value) : literal(value)}`)
        .join("\n");

    const values = (node: Json): Value[] | undefined => {
      if (node.enum) return node.enum.map((value: unknown) => ({ value: literal(value) }));
      if (node.const !== undefined) return [{ value: literal(node.const), description: inline(node.description) }];
      if (node.oneOf && node.oneOf.every((alt: Json) => alt.type === "string" && (alt.enum || alt.const !== undefined))) {
        return node.oneOf.flatMap((alt: Json) => values(alt) ?? []);
      }
      return undefined;
    };

    const range = (node: Json) => {
      const { minimum: min, maximum: max } = node;
      if (min !== undefined && max !== undefined) return ` from ${min} to ${max}`;
      if (min !== undefined && min !== 0) return `, at least ${min}`;
      if (max !== undefined) return `, at most ${max}`;
      return "";
    };

    const typeName = (raw: Json): string => {
      const node = deref(raw);
      if (values(node)) return "one of";
      const types = ([] as string[]).concat(node.type ?? []).filter((t) => t !== "null");
      const type = types[0];
      if (type === "integer") return `integer${range(node)}`;
      if (type === "number") return `number${range(node)}`;
      if (type === "boolean") return "boolean";
      if (type === "string") return node.pattern ? "string, HH:MM" : "string";
      if (type === "array") {
        const item = deref(node.items);
        if (item.properties) return "list of tables";
        const name = typeName(node.items);
        return name === "one of" ? "list, each one of" : `list of ${name.replace(/^(\w+)/, "$1s")}`;
      }
      if (type === "object" && node.additionalProperties && !node.properties) {
        return `table of ${typeName(node.additionalProperties)}s`;
      }
      if (type === "object" || node.properties) return "table";
      if (node.oneOf) return "table";
      return type ?? "value";
    };

    const collect = (path: string, raw: Json, array = false) => {
      const node = deref(raw);
      const options: Option[] = [];
      for (const [key, propRaw] of Object.entries<Json>(node.properties ?? {})) {
        if (path.startsWith("applets.") && key === "extends") continue;
        const prop = deref(propRaw);
        const description = propRaw.description ?? prop.description;
        if (!description) missing.push(`${path}.${key}`);
        const item = prop.type === "array" || (Array.isArray(prop.type) && prop.type.includes("array")) ? deref(prop.items) : undefined;
        const nested = prop.properties ? prop : item?.properties ? item : undefined;
        const option: Option = {
          key,
          type: typeName(propRaw),
          description: block(description) ?? "",
          values: values(prop) ?? (item ? values(item) : undefined),
        };
        const fallback = propRaw.default ?? prop.default;
        const header = `${path}.${key}`;
        if (fallback === undefined || fallback === null) {
          if (!nested) option.default = "not set";
        } else if (Array.isArray(fallback) && fallback.some((entry) => entry && typeof entry === "object")) {
          option.defaultToml = fallback.map((entry: Json) => `[[${header}]]\n${toml(entry)}`).join("\n\n");
        } else if (!nested) {
          option.default = literal(fallback);
        }
        if (nested) {
          option.table = `${path}.${key}`;
          collect(option.table, nested, nested === item);
        }
        options.push(option);
      }
      tables[path] = { path, array, description: block(raw.description ?? node.description), options };
    };

    for (const [section, raw] of Object.entries<Json>(schema.properties)) {
      if (section === "applets") continue;
      const node = deref(raw);
      collect(section, node.type === "array" ? node.items : raw, node.type === "array");
    }

    const applets: Data["applets"] = [];
    for (const [name, raw] of Object.entries<Json>(schema.properties.applets.properties)) {
      if (hiddenApplets.has(name)) continue;
      if (!raw.description) missing.push(`applets.${name}`);
      applets.push({ name, description: inline(raw.description) ?? "" });
      collect(`applets.${name}`, raw);
    }
    if (missing.length > 0) {
      throw new Error(
        `the config schemas have settings with no description; add a doc comment in glimpse-config:\n  ${missing.join("\n  ")}`,
      );
    }
    return { tables, applets };
  },
});
