# Build a todo applet

Build the shipped todo example in five stages: create a project, show a task, add tasks, add dates and controls, then save the list. You'll need a running glimpse panel, Node.js 22 or newer, npm and `just`. The panel starts your applet and talks to it over a socket; [How an applet runs](/developers/exec-applets) explains how.

## 1. Create and run the project

Run:

```sh
glimpsectl applet new
```

Answer the prompts with these values:

| Prompt | Enter |
| --- | --- |
| Applet id | `me.aresa.GlimpseTodo` |
| Name | `Todo` |
| Description | `A list of things to do` |
| Icon | `view-list-symbolic` |
| Directory | `glimpse-todo` |

Use a new or empty directory. The id must not already be installed. The command creates `.gitignore`, `README.md`, `biome.json`, `devconfig.toml`, `index.tsx`, `justfile`, `me.aresa.GlimpseTodo.desktop`, `package.json`, and `tsconfig.json`, plus the `glimpse-exec-applet` skill for [a coding agent](/applets/agents) in `.agents/skills/` and `.claude/skills`.

The applet is in `index.tsx`:

```tsx
import { Indicator, Popover, Row, run, Section } from "glimpse-sdk";

const ICON = "view-list-symbolic";
const NAME = "Todo";

function App() {
    return (
        <>
            <Indicator icon={ICON} tooltip={NAME}>
                {NAME}
            </Indicator>
            <Popover icon={ICON} title={NAME}>
                <Section title={NAME}>
                    <Row title="It works" />
                </Section>
            </Popover>
        </>
    );
}

await run(<App />);
```

```sh
cd glimpse-todo
just dev
```

Leave that terminal open. A Todo chip appears in your panel; click it to see an "It works" row. You don't need to install the desktop entry or change your panel configuration for this development run.

The generated TypeScript configuration uses `jsxImportSource: "glimpse-sdk"`. Keep that setting and the generated package scripts. Saving source rebuilds and restarts the applet; stderr appears in this terminal. The first build installs the npm dependencies, and later builds install them again only when `package.json` changes.

## 2. Show a task and an open count

Change `index.tsx` (lines starting with `+` are new, `-` are removed):

```diff
@@ -1,17 +1,18 @@
-import { Indicator, Popover, Row, run, Section } from "glimpse-sdk";
+import { Indicator, Popover, run, Section, ToggleRow, useState } from "glimpse-sdk";
 
 const ICON = "view-list-symbolic";
 const NAME = "Todo";
 
 function App() {
+    const [done, setDone] = useState(false);
     return (
         <>
-            <Indicator icon={ICON} tooltip={NAME}>
+            <Indicator icon={ICON} tooltip={NAME} badge={done ? "0" : "1"}>
                 {NAME}
             </Indicator>
             <Popover icon={ICON} title={NAME}>
-                <Section title={NAME}>
-                    <Row title="It works" />
+                <Section title={NAME} count={done ? "0" : "1"}>
+                    <ToggleRow title="Buy groceries" control="check" active={done} onToggle={setDone} />
                 </Section>
             </Popover>
         </>
```

Save, then click Todo. The badge shows one open task. Click the "Buy groceries" row and the badge becomes zero. `useState` keeps the done state while this process runs. `ToggleRow` with `control="check"` shows a check mark at the end of the row only while the task is done.

## 3. Add tasks from an entry

Change `index.tsx`:

```diff
@@ -1,19 +1,48 @@
-import { Indicator, Popover, run, Section, ToggleRow, useState } from "glimpse-sdk";
+import { Entry, Indicator, Popover, run, Section, ToggleRow, useState } from "glimpse-sdk";
 
 const ICON = "view-list-symbolic";
 const NAME = "Todo";
 
+type Item = { id: string; title: string; done: boolean };
+
 function App() {
-    const [done, setDone] = useState(false);
+    const [items, setItems] = useState<Item[]>([]);
+    const [draft, setDraft] = useState("");
+    const open = items.filter((item) => !item.done);
     return (
         <>
-            <Indicator icon={ICON} tooltip={NAME} badge={done ? "0" : "1"}>
+            <Indicator icon={ICON} tooltip={NAME} badge={String(open.length)}>
                 {NAME}
             </Indicator>
             <Popover icon={ICON} title={NAME}>
-                <Section title={NAME} count={done ? "0" : "1"}>
-                    <ToggleRow title="Buy groceries" control="check" active={done} onToggle={setDone} />
+                <Section title={NAME} count={String(open.length)}>
+                    {items.map((item) => (
+                        <ToggleRow
+                            key={item.id}
+                            title={item.title}
+                            control="check"
+                            active={item.done}
+                            onToggle={(done) =>
+                                setItems(
+                                    items.map((candidate) =>
+                                        candidate.id === item.id ? { ...candidate, done } : candidate,
+                                    ),
+                                )
+                            }
+                        />
+                    ))}
                 </Section>
+                <Entry
+                    placeholder="Add a task"
+                    value={draft}
+                    onChange={setDraft}
+                    onSubmit={(value) => {
+                        const title = value.trim();
+                        if (title === "") return;
+                        setItems([...items, { id: crypto.randomUUID(), title, done: false }]);
+                        setDraft("");
+                    }}
+                />
             </Popover>
         </>
     );
```

The list starts empty after the source restart. Open Todo, type "Buy groceries" in the entry, and press Enter. The row appears and the entry clears. Add "Book a dentist appointment", then check either row to reduce the open count.

The entry is controlled by `value` and `onChange`; `onSubmit` receives its text when you press Enter. Each task gets a stable id for its React key.

## 4. Add dates, options, and list controls

Change `index.tsx`:

```diff
@@ -1,25 +1,63 @@
-import { Entry, Indicator, Popover, run, Section, ToggleRow, useState } from "glimpse-sdk";
+import {
+    Button,
+    copy,
+    Entry,
+    Indicator,
+    Popover,
+    run,
+    Section,
+    ToggleRow,
+    useOptions,
+    usePlacement,
+    useState,
+} from "glimpse-sdk";
 
 const ICON = "view-list-symbolic";
 const NAME = "Todo";
 
-type Item = { id: string; title: string; done: boolean };
+type Item = { id: string; title: string; done: boolean; due: string | null };
+
+function today(): string {
+    const now = new Date();
+    const month = String(now.getMonth() + 1).padStart(2, "0");
+    const day = String(now.getDate()).padStart(2, "0");
+    return `${now.getFullYear()}-${month}-${day}`;
+}
+
+function splitDraft(text: string): { title: string; due: string | null } {
+    const trimmed = text.trim();
+    const match = /^(.*\S)\s+(\d{4}-\d{2}-\d{2})$/.exec(trimmed);
+    if (!match) return { title: trimmed, due: null };
+    return { title: match[1], due: match[2] };
+}
 
 function App() {
+    const options = useOptions();
+    const placement = usePlacement();
+    const configured = options.title;
+    const title = typeof configured === "string" && configured.trim() !== "" ? configured : NAME;
     const [items, setItems] = useState<Item[]>([]);
     const [draft, setDraft] = useState("");
+    const [showDone, setShowDone] = useState(false);
+
     const open = items.filter((item) => !item.done);
+    const day = today();
+    const overdue = open.some((item) => item.due !== null && item.due < day);
+    const vertical = placement?.orientation === "vertical";
+    const visible = showDone ? items : open;
+
     return (
         <>
-            <Indicator icon={ICON} tooltip={NAME} badge={String(open.length)}>
-                {NAME}
+            <Indicator icon={ICON} tooltip={title} badge={String(open.length)} dot={overdue ? "#e01b24" : undefined}>
+                {vertical ? null : title}
             </Indicator>
-            <Popover icon={ICON} title={NAME}>
-                <Section title={NAME} count={String(open.length)}>
-                    {items.map((item) => (
+            <Popover icon={ICON} title={title} subtitle={`${open.length} open`}>
+                <Section title={title} count={String(open.length)}>
+                    {visible.map((item) => (
                         <ToggleRow
                             key={item.id}
                             title={item.title}
+                            subtitle={item.due ?? undefined}
                             control="check"
                             active={item.done}
                             onToggle={(done) =>
@@ -37,12 +75,23 @@
                     value={draft}
                     onChange={setDraft}
                     onSubmit={(value) => {
-                        const title = value.trim();
-                        if (title === "") return;
-                        setItems([...items, { id: crypto.randomUUID(), title, done: false }]);
+                        const parsed = splitDraft(value);
+                        if (parsed.title === "") return;
+                        setItems([
+                            ...items,
+                            { id: crypto.randomUUID(), title: parsed.title, done: false, due: parsed.due },
+                        ]);
                         setDraft("");
                     }}
                 />
+                <ToggleRow title="Show done" active={showDone} onToggle={setShowDone} />
+                <Button
+                    onClick={() =>
+                        copy(items.map((item) => (item.due ? `${item.title} ${item.due}` : item.title)).join("\n"))
+                    }
+                >
+                    Copy all
+                </Button>
             </Popover>
         </>
     );
```

Save and add "Renew library books 2000-01-01". The date appears below the task and the chip gains a red dot because an open item is overdue. Check the task to hide it, then enable "Show done" to see it again. "Copy all" copies every task, including completed tasks and their dates.

Change the project's `devconfig.toml` to:

```toml
options = { title = "Groceries" }
```

Save it. The chip and popover title become "Groceries" without restarting the process. `useOptions` reads this title, and `usePlacement` lets the same component omit the chip's text on a vertical panel while keeping its badge.

## 5. Keep the list between restarts

Change `index.tsx` once more. Saving starts the persistent version with an empty list; add your tasks again.

```diff
@@ -1,3 +1,5 @@
+import { readFileSync, renameSync, rmSync, writeFileSync } from "node:fs";
+import { join } from "node:path";
 import {
     Button,
     copy,
@@ -6,10 +8,13 @@
     Popover,
     run,
     Section,
+    notify,
     ToggleRow,
+    useEffect,
     useOptions,
     usePlacement,
     useState,
+    useStateDir,
 } from "glimpse-sdk";
 
 const ICON = "view-list-symbolic";
@@ -17,6 +22,34 @@
 
 type Item = { id: string; title: string; done: boolean; due: string | null };
 
+function loadItems(stateDir: string): { items: Item[]; failed: boolean } {
+    if (!stateDir) return { items: [], failed: false };
+    try {
+        const raw = readFileSync(join(stateDir, "todo.json"), "utf8");
+        const parsed: unknown = JSON.parse(raw);
+        if (!Array.isArray(parsed) || !parsed.every(isItem)) {
+            return { items: [], failed: true };
+        }
+        return { items: parsed, failed: false };
+    } catch (error) {
+        if ((error as NodeJS.ErrnoException).code === "ENOENT") {
+            return { items: [], failed: false };
+        }
+        return { items: [], failed: true };
+    }
+}
+
+function isItem(value: unknown): value is Item {
+    if (typeof value !== "object" || value === null) return false;
+    const item = value as Record<string, unknown>;
+    return (
+        typeof item.id === "string" &&
+        typeof item.title === "string" &&
+        typeof item.done === "boolean" &&
+        (item.due === null || typeof item.due === "string")
+    );
+}
+
 function today(): string {
     const now = new Date();
     const month = String(now.getMonth() + 1).padStart(2, "0");
@@ -32,14 +65,29 @@
 }
 
 function App() {
+    const stateDir = useStateDir();
     const options = useOptions();
     const placement = usePlacement();
     const configured = options.title;
     const title = typeof configured === "string" && configured.trim() !== "" ? configured : NAME;
-    const [items, setItems] = useState<Item[]>([]);
+    const [loaded] = useState(() => loadItems(stateDir));
+    const [items, setItems] = useState<Item[]>(loaded.items);
     const [draft, setDraft] = useState("");
     const [showDone, setShowDone] = useState(false);
 
+    useEffect(() => {
+        if (!stateDir || (loaded.failed && items === loaded.items)) return;
+        const target = join(stateDir, "todo.json");
+        const temp = `${target}.${process.pid}.tmp`;
+        try {
+            writeFileSync(temp, JSON.stringify(items));
+            renameSync(temp, target);
+        } catch (error) {
+            rmSync(temp, { force: true });
+            notify({ summary: "Could not save Todo items", body: String(error) });
+        }
+    }, [items, stateDir, loaded]);
+
     const open = items.filter((item) => !item.done);
     const day = today();
     const overdue = open.some((item) => item.due !== null && item.due < day);
```

If a change does not apply cleanly, compare with the finished file, [`sdk/examples/todo/index.tsx`](https://github.com/alex-oleshkevich/glimpse/blob/master/sdk/examples/todo/index.tsx).

Add a task, then type `r` and Enter in the development terminal. Open the popover again: the task is still there.

`useStateDir()` supplies the persistent directory. This version validates the saved JSON before using it and writes through a temporary file before replacing `todo.json`. If loading fails, it leaves the saved file alone until you edit the list. A failed save requests a notification.

Run the linter and type check in another terminal from the project directory:

```sh
just lint
```

Press Ctrl-C in the development terminal to detach. To keep the applet on your panel, follow [Package and share an applet](/applets/exec/share).

