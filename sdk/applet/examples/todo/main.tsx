import React from "npm:react@19.3.0";
import {
  Button,
  copy,
  Entry,
  Footer,
  Hero,
  Indicator,
  Popover,
  Row,
  run,
  Section,
  SwitchRow,
  useEffect,
  useOptions,
  usePlacement,
  useState,
} from "../../mod.ts";

type Item = {
  id: string;
  title: string;
  done: boolean;
  due: string | null;
};

const STORAGE_KEY = "glimpse.todo";

function loadItems(): Item[] {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return [];
    const parsed: unknown = JSON.parse(raw);
    if (!Array.isArray(parsed)) return [];
    return parsed.filter(isItem);
  } catch {
    return [];
  }
}

function isItem(value: unknown): value is Item {
  if (typeof value !== "object" || value === null) return false;
  const item = value as Record<string, unknown>;
  return typeof item.id === "string" &&
    typeof item.title === "string" &&
    typeof item.done === "boolean" &&
    (item.due === null || typeof item.due === "string");
}

function today(): string {
  const now = new Date();
  const month = String(now.getMonth() + 1).padStart(2, "0");
  const day = String(now.getDate()).padStart(2, "0");
  return `${now.getFullYear()}-${month}-${day}`;
}

function splitDraft(text: string): { title: string; due: string | null } {
  const trimmed = text.trim();
  const match = /^(.*\S)\s+(\d{4}-\d{2}-\d{2})$/.exec(trimmed);
  if (!match) return { title: trimmed, due: null };
  return { title: match[1], due: match[2] };
}

function Todo() {
  const options = useOptions();
  const placement = usePlacement();
  const configured = options.title;
  const title = typeof configured === "string" && configured.trim() !== ""
    ? configured
    : "Todo";
  const [items, setItems] = useState<Item[]>(loadItems);
  const [draft, setDraft] = useState("");
  const [showDone, setShowDone] = useState(false);

  useEffect(() => {
    try {
      localStorage.setItem(STORAGE_KEY, JSON.stringify(items));
    } catch {
      return;
    }
  }, [items]);

  const open = items.filter((item) => !item.done);
  const day = today();
  const overdue = open.some((item) => item.due !== null && item.due < day);
  const vertical = placement?.orientation === "vertical";
  const visible = showDone ? items : open;

  return (
    <>
      <Indicator
        icon="view-list-symbolic"
        tooltip={title}
        badge={String(open.length)}
        dot={overdue ? "#e01b24" : undefined}
      >
        {vertical ? null : title}
      </Indicator>
      <Popover>
        <Hero
          icon="view-list-symbolic"
          title={title}
          subtitle={`${open.length} open`}
        />
        <Section title={title} count={String(open.length)}>
          {visible.map((item) => (
            <Row
              key={item.id}
              title={item.title}
              subtitle={item.due ?? undefined}
              selected={item.done ? true : undefined}
              onActivate={() => {
                setItems(items.map((candidate) =>
                  candidate.id === item.id
                    ? { ...candidate, done: !candidate.done }
                    : candidate
                ));
              }}
            />
          ))}
        </Section>
        <Entry
          placeholder="Add a task"
          value={draft}
          onChange={setDraft}
          onSubmit={(value) => {
            const parsed = splitDraft(value);
            if (parsed.title === "") return;
            setItems([
              ...items,
              {
                id: crypto.randomUUID(),
                title: parsed.title,
                done: false,
                due: parsed.due,
              },
            ]);
            setDraft("");
          }}
        />
        <SwitchRow title="Show done" active={showDone} onToggle={setShowDone} />
        <Footer>
          <Button
            onClick={() => {
              copy(
                items.map((
                  item,
                ) => (item.due ? `${item.title} ${item.due}` : item.title))
                  .join(
                    "\n",
                  ),
              );
            }}
          >
            Copy all
          </Button>
        </Footer>
      </Popover>
    </>
  );
}

await run(<Todo />);
