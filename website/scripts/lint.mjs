import { readdirSync, readFileSync } from "node:fs";
import { dirname, join, relative } from "node:path";
import { fileURLToPath } from "node:url";

const website = join(dirname(fileURLToPath(import.meta.url)), "..");

const rules = [
  { pattern: /[\u2013\u2014]/, reason: "em or en dash; use a period, comma, colon or parentheses", prose: true },
  { pattern: /[\u201c\u201d\u2018\u2019]/, reason: "curly quote; use a straight one", prose: true },
  { pattern: /\b(foo|foobar|baz|hello[ ,-]?world|pomodoro|example\.com|me\.example)\b/i, reason: "placeholder example; use a real use case" },
  { pattern: /\b(niri|hyprland)\b/i, reason: "compositor name; say compositor, or use the compositor badge", prose: true },
  { pattern: /\[sandbox\]|\.sandbox\b|\blandlock\b|\bunconfined\s*=/i, reason: "removed sandbox setting" },
  {
    pattern: /\b(no longer|formerly|deprecated|(added|removed|renamed|changed|introduced) in (version |v)?\d|since (version |v)?\d|(sdk|glimpse) v?\d+\.\d+)/i,
    reason: "version history; describe only the current version",
    prose: true,
  },
  {
    pattern: /\b(seamless(ly)?|robust|powerful|delve|leverag(e|es|ing)|utiliz(e|es|ing)|effortless(ly)?|cutting-edge|vibrant|showcas(e|es|ing)|testament|crucial|pivotal|tapestry|in today's)\b/i,
    reason: "filler word",
    prose: true,
  },
];

function* pages(dir) {
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    if (entry.name.startsWith(".") || entry.name === "node_modules" || entry.name === "public") continue;
    const path = join(dir, entry.name);
    if (entry.isDirectory()) yield* pages(path);
    else if (entry.name.endsWith(".md")) yield path;
  }
}

function lines(text) {
  let fenced = false;
  return text.split("\n").map((line, index) => {
    const fence = /^\s*(```|~~~)/.test(line);
    if (fence) fenced = !fenced;
    return { line, number: index + 1, code: fence || fenced };
  });
}

let failures = 0;
for (const path of pages(website)) {
  const text = readFileSync(path, "utf8");
  const generated = text.startsWith("<!-- Generated");
  for (const { line, number, code } of lines(text)) {
    const prose = line.replace(/`[^`]*`/g, "");
    for (const rule of rules) {
      if (rule.prose && code) continue;
      if (generated && code) continue;
      if (rule.pattern.test(rule.prose ? prose : line)) {
        console.error(`${relative(website, path)}:${number}: ${rule.reason}\n    ${line.trim()}`);
        failures += 1;
      }
    }
  }
}

if (failures > 0) {
  console.error(`\n${failures} problem(s) found`);
  process.exit(1);
}
