# Keep data between restarts

Use `useStateDir()` to store an applet's data outside its source directory. The host creates this persistent directory with mode `0700`; the SDK supplies its path before mounting your component.

## Load once and save changes

This complete `index.tsx` keeps a task title across restarts. It treats a missing file as a new list, preserves an unreadable file until you edit, and writes through a temporary file before replacing the saved one:

```tsx
import { readFileSync, renameSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import {
  Entry, Indicator, notify, Popover, run, useEffect, useState, useStateDir,
} from "glimpse-sdk";

function Task() {
  const stateDir = useStateDir();
  const [loaded] = useState(() => {
    try {
      return { value: readFileSync(join(stateDir, "task.txt"), "utf8"), failed: false };
    } catch (error) {
      const missing = (error as NodeJS.ErrnoException).code === "ENOENT";
      if (!missing) console.error("Could not read task:", error);
      return { value: "", failed: !missing };
    }
  });
  const [value, setValue] = useState(loaded.value);

  useEffect(() => {
    if (!stateDir || loaded.failed && value === loaded.value) return;
    const target = join(stateDir, "task.txt");
    const temp = `${target}.${process.pid}.tmp`;
    try {
      writeFileSync(temp, value);
      renameSync(temp, target);
    } catch (error) {
      rmSync(temp, { force: true });
      notify({ summary: "Could not save task", body: String(error) });
    }
  }, [value, stateDir, loaded]);

  return (
    <>
      <Indicator icon="view-list-symbolic" tooltip="Saved task" />
      <Popover title="Saved task">
        <Entry placeholder="Task title" value={value} onChange={setValue} />
      </Popover>
    </>
  );
}

await run(<Task />);
```

Run it with `glimpsectl applet run --dev .`, enter a title, then type `r` and Enter in the terminal. Open the popover again to see the saved title.

## Choose filenames for shared data

The host directory is keyed by desktop-file id, so instances of the same applet share it. Use distinct filenames if those instances need separate data; `useAppletName()` supplies the configured instance name. The [todo tutorial](/applets/exec/#_5-keep-the-list-between-restarts) shows JSON validation for structured data.
