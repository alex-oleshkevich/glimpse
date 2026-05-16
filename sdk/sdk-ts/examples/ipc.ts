// Subscribe to shell events and dispatch an action.
//
// Run against a live Glimpse session:
//   npm run build && node dist/examples/ipc.js

import { ipc } from "../src/index.js";

// Cheap: resolves the socket path, no connection yet.
const sub = ipc("shell");

// One-shot connection; awaits the ack. Throws IpcError on ok=false.
const ack = await sub.dispatch("open_uri", { uri: "https://example.com" });
console.log("dispatch ack:", ack);

// Async iterable; the socket closes when the loop ends.
for await (const ev of sub.listen("audio.*")) {
  console.log(ev.name, ev.ts, ev.fields);
}
