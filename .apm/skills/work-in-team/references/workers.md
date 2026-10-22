# Worker command lines

Every CLI runs from the worktree with stdin closed (`< /dev/null`), in the foreground of its own
background tool call.

- **codex** — `codex exec -m gpt-6.1-sol -C <worktree> --sandbox workspace-write "<brief>" < /dev/null`.
- **codex astra** — `codex exec -m gpt-6-astra --sandbox read-only -C <worktree> -o <out.md> - < <brief.md>`.
  Read-only; never a builder.
- **gridr** — `claude-gridr -p --model opus --effort high --allowedTools "Read" "Grep" ... < <brief.md>`.
  `--allowedTools` takes every following word, so the brief goes on stdin, never as a trailing
  argument. A path rule for writes is `Edit(<path>)`; `Write(<path>)` is ignored.
- **grok** — `grok --cwd <worktree> --prompt-file <brief.md> --always-approve`.
- **muse** — `env -C <worktree> muse exec --prompt-file <brief.md>`. Builds only; the first fallback
  when another builder CLI is broken or out of credits.
- **kimi** — `env -C <worktree> kimi -p "$(cat <brief.md>)"`. `-p` already edits and runs commands;
  it rejects `-y` and `--auto`. When its limit is reached it exits 1 with no output, even right after
  a passing probe. Never add a large MCP server (such as rea) to `~/.kimi-code/mcp.json`: its tools
  overflow Moonshot's schema limit (400 "schema exceeds maximum allowed size").
- **pi with DeepSeek** — build: `pi -p --no-session --model openrouter/deepseek/deepseek-v4-flash-latest
  "<brief>"`. Live test and analysis: add `-ne --no-mcp -ns -np`. After a run, `rm -rf
  <cwd>/.pi-lens-probe-home` and log the OpenRouter spend. The key has a monthly cap; a 403 "Key
  limit exceeded" means pi is out until it resets, and a tester killed by it leaves its processes
  running, so stop them by pid.
