# LLM-facing docs: llms.txt and llms-full.txt

| | |
| --- | --- |
| Bead | **glimpse-dx371**; tasks are sliced from §11 and carry `Source: docs/plans/llm-docs.md §N` |
| Sibling | **glimpse-tumwm**, `docs/plans/applet-author-skills.md`: the skills link here, this links to them (§5.4) |
| Status | Final; decisions in §4, open questions closed in §13. |
| Rule | Every claim is tagged **measured** (run on this machine at `ea2430ab`, 2026-10-28), **source** (`file:line` at `ea2430ab`) or **documented** (upstream docs, URL given). |

## 1. Goal

An agent that fetches `https://alex-oleshkevich.github.io/glimpse/llms.txt` finds two paths, one for
configuring glimpse and one for building an applet, each an ordered list of absolute links to the
Markdown copy of the pages it needs. Every config page it loads carries the settings tables a human
sees. Each section also has its own `llms-full.txt` small enough to load whole.

## 2. Current state (tree at `ea2430ab`)

| What | Where |
| --- | --- |
| `vitepress-plugin-llms` 1.14.0, loaded with no options | `website/package.json:15`; `website/.vitepress/config.mts:3`, `:50` (**source**) |
| Generated `llms.txt` is 5,244 bytes: the sidebar as a TOC, links relative (`/glimpse/guide.md`), every section including `contribute/` | `website/.vitepress/dist/llms.txt` (**measured**) |
| Generated `llms-full.txt` is 336,901 bytes: every page concatenated | (**measured**) |
| Per-page Markdown copies exist for every page (`dist/<page>.md`) | (**measured**) |
| `<ConfigTable path="…" />` appears 53 times on 13 pages; the Markdown copies drop it, so an agent reading `config/services/notifications.md` gets the prose and no setting at all | `website/.vitepress/dist/config/services/notifications.md` vs `website/config/services/notifications.md:8` (**measured**) |
| The tables are built from `data/config.schema.json` and `data/panel.schema.json` by a VitePress data loader | `website/.vitepress/schema.data.ts:5-6`, `Table`/`Option` at `:20-37` (**source**) |
| The plugin's transform runs `enforce: "pre"`, collects the page source it is given, and keeps `<llm-only>` blocks for LLM output while stripping them from HTML | `website/node_modules/vitepress-plugin-llms/dist/index.js:1062-1075`, `:1159-1168` (**source**) |
| `experimental: { depth: 2 }` writes `llms.txt` and `llms-full.txt` per top-level directory | (**measured**: `start` 35,941, `guide` 35,845, `config` 63,774, `applets` 118,704, `developers` 38,759, `contribute` 17,204, `ecosystem` 11,515 bytes for `llms-full.txt`) |
| `customLLMsTxtTemplate`, `customTemplateVariables`, `domain`, `ignoreFiles`, `ignoreFilesPerOutput` exist | `website/node_modules/vitepress-plugin-llms/dist/index.d.ts:73`, `:146`, `:171`, `:278`, `:305` (**source**) |
| A raw prompt file is already served from `public/` | `website/public/migrate-0.15.txt` (**source**) |
| `_old/docs/llms/` | reference only; never used |

## 3. Facts

- F1 (**measured**) The `.md` copies are what `llms.txt` links to; fixing the copies fixes both the
  per-page and the full files.
- F2 (**measured**) The largest single page is `applets/exec/reference/elements.md`, 53,151 bytes.
- F3 (**source**) `ConfigTable` takes `path` and an optional `describe` flag; the loader already
  turns each schema table into `Option` rows (`key`, `type`, `default`, `description`, `values`).

## 4. Decisions

- D1 Keep the plugin. No hand-written per-page agent copies: the human page is the fact, the copy is
  derived from it, so the docs-skill fact rules cover both.
- D2 Hand-write only the `llms.txt` head through `customLLMsTxtTemplate`: two ordered paths
  (configure, build an applet), then the generated `{toc}`.
- D3 `domain: "https://alex-oleshkevich.github.io"` so every link is absolute.
- D4 `experimental.depth: 2` for per-section bundles. The `applets/` bundle (118,704 bytes) is the
  ceiling; nothing larger is linked from the paths.
- D5 `ignoreFiles: ["contribute/**"]`: contributor pages serve no outside agent and live in the repo.
- D6 Settings tables reach the Markdown copies as `<llm-only>` Markdown tables generated from the
  same loader, inserted by a small Vite plugin ordered before `llmstxt()` (§5.2).
- D7 Publish `config.schema.json` and `panel.schema.json` at the site root (copied by
  `scripts/assets.mjs`) and link them from `llms.txt`, for validation by an agent's TOML tooling.

## 5. Architecture

### 5.1 Files

| File | Change |
| --- | --- |
| `website/.vitepress/config.mts` | `llmstxt({ domain, experimental: { depth: 2 }, ignoreFiles, customLLMsTxtTemplate })`; add `configTablesForLlms()` before it in `vite.plugins` |
| `website/.vitepress/llms.ts` | new: the template string and the `configTablesForLlms` plugin |
| `website/.vitepress/schema.data.ts` | export the pure `tables()` builder the loader already runs, so the plugin reuses it |
| `website/scripts/assets.mjs` | copy `data/config.schema.json`, `data/panel.schema.json` to `public/` |

### 5.2 How the pieces connect

`configTablesForLlms` is a Vite plugin with `enforce: "pre"`. For each `website/**/*.md` it replaces
every `<ConfigTable path="P" [describe] />` with the same tag followed by
`<llm-only>` + a Markdown table rendered from `tables()[P]` + `</llm-only>`. `llmstxt()` runs next,
sees the expanded source, strips the `<llm-only>` block from the HTML (humans still get the Vue
component) and keeps it in the `.md` copy and the bundles.

```ts
// website/.vitepress/llms.ts
export function configTablesForLlms(): Plugin;      // enforce: "pre"
export function tableMarkdown(table: Table): string; // | Key | Type | Default | Description |
export const llmsTemplate: string;                    // uses {title}, {description}, {toc}
```

### 5.3 The `llms.txt` head

```markdown
# glimpse

> {description}

## Configure glimpse
1. How configuration works: …/config.md
2. Panels: …/config/panels.md, then the applet page for each applet you change
3. Services: …/config/services/<name>.md
Schemas: …/config.schema.json, …/panel.schema.json
Moving from 0.15: …/migrate-0.15.txt

## Build an applet
1. Choosing an applet kind: …/applets.md
2. Command applets: …/applets/command.md
3. Exec applets: …/applets/exec.md, then …/applets/exec/reference/*.md
Whole sections: …/config/llms-full.txt, …/applets/llms-full.txt, …/developers/llms-full.txt
Agent skills: see §5.4

{toc}
```

### 5.4 Link with the applet-author skills

The skills (glimpse-tumwm) carry workflow and traps and link to these `.md` pages for reference.
`llms.txt` names where the skills are installed (`/usr/share/glimpse/sdk/skills/`) and does not
repeat their content.

## 6. Config

None. No glimpse setting changes.

## 9. Flow

Agent fetches `llms.txt` → follows the configure path → fetches
`config/services/notifications.md` → reads the prose and the `<llm-only>` table with every key,
type, default and description → writes `config.toml` → validates against the published schema or
with `glimpsectl config validate`.

## 10. Acceptance criteria

```
AC-1  As an agent configuring glimpse, I want the settings on each config page, so that I do not guess keys.
      GIVEN a build WHEN I read dist/config/services/notifications.md THEN it lists keep, suppress, enabled, monitor, edge, hide-delay, max-items with defaults
      Verify by: grep after `just docs-build`
AC-2  As a human reader, I want the page unchanged.
      GIVEN the HTML page WHEN rendered THEN the ConfigTable component shows once and no Markdown copy of it appears
      Verify by: browser check, light and dark
AC-3  As an agent, I want absolute links.
      GIVEN dist/llms.txt WHEN I read any link THEN it starts with https://alex-oleshkevich.github.io/glimpse/
      Verify by: grep
AC-4  As an agent with a small context, I want section bundles.
      GIVEN a build THEN dist/{config,applets,developers}/llms-full.txt exist and none exceeds 160 KB
      Verify by: wc -c
AC-5  (error path) A ConfigTable naming a path the schema lacks fails the build rather than emitting an empty table.
      Verify by: temporary bad path, `npm run build` exits non-zero
AC-6  No contributor page appears in llms.txt or any bundle.
      Verify by: grep for contribute/
```

## 11. Slicing

1. `configTablesForLlms` + `tables()` export + AC-1, AC-2, AC-5 (§5.1, §5.2).
2. Plugin options, template, schema publishing + AC-3, AC-4, AC-6 (§4 D2-D5, D7, §5.3).

## 12. Rejected

- Hand-written agent pages under `website/llms/`: a second copy of every fact, drifting from the page.
- Rendering `ConfigTable` server-side into the HTML Markdown: changes what humans see.
- One curated `llms-full.txt`: still 337 KB; section bundles are smaller and need no curation.

## 13. Open questions

- ~~Does the plugin see transformed source from an earlier `pre` plugin?~~ Yes: it collects the
  `content` its own `transform` receives (`index.js:1062-1075`), and Vite passes the previous
  plugin's output.
- ~~Per-section bundles need a custom script?~~ No: `experimental.depth` (measured above).

## 14. Review record

- 2026-10-28: written with the plan-precision and docs skills; sizes and plugin behavior measured at
  `ea2430ab`.
