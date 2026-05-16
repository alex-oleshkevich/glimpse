// Minimal client for the Glimpse IPC socket.
//
// `ipc(service)` resolves a `Subscriber` (no I/O — the connection is opened
// lazily). `Subscriber.listen` subscribes to an event channel and yields
// decoded `Event`s; `Subscriber.dispatch` sends an action and awaits the
// server ack on a one-shot connection. The wire protocol matches the
// `glimpse-shell watch` / `dispatch` CLIs.

import { createConnection, type Socket } from "node:net";
import { createInterface, type Interface } from "node:readline";
import { join } from "node:path";

/** One decoded event line; `fields` values are unescaped. */
export interface Event {
  name: string;
  ts: number;
  fields: Record<string, string>;
}

/** Thrown on connection failure or a rejected dispatch (`ok=false`). */
export class IpcError extends Error {}

interface Conn {
  socket: Socket;
  rl: Interface;
  lines: AsyncIterator<string>;
}

/** A resolved IPC endpoint. Cheap to create; holds only the socket path. */
export class Subscriber {
  constructor(private readonly socketPath: string) {}

  /** Subscribe to `channel` (an exact name, `prefix.*`, or `*`) and yield
   * events until the server closes the connection. */
  async *listen(channel: string): AsyncGenerator<Event> {
    const conn = await this.connect();
    try {
      conn.socket.write(`subscribe ${channel}\n`);
      for (;;) {
        const next = await conn.lines.next();
        if (next.done) return;
        const line = next.value.trim();
        if (line) yield parseEvent(line);
      }
    } finally {
      close(conn);
    }
  }

  /** Dispatch `action` with `params` on a fresh connection and await the
   * ack. Resolves with the extra ack fields; rejects with `IpcError` if the
   * server replies `ok=false`. */
  async dispatch(
    action: string,
    params: Record<string, string> = {},
  ): Promise<Record<string, string>> {
    validateToken("action", action, false);
    const entries = Object.entries(params);
    for (const [k] of entries) validateToken("param key", k, true);
    const conn = await this.connect();
    try {
      let line = action;
      for (const [k, v] of entries) {
        line += ` ${k}=${escape(v)}`;
      }
      conn.socket.write(line + "\n");
      const next = await conn.lines.next();
      if (next.done) {
        throw new IpcError("IPC server closed connection without ack");
      }
      return parseAck(next.value.trim());
    } finally {
      close(conn);
    }
  }

  private connect(): Promise<Conn> {
    return new Promise<Conn>((resolve, reject) => {
      const socket = createConnection({ path: this.socketPath });
      let connected = false;
      // This handler stays attached for the socket's whole lifetime. Never
      // remove it: an unhandled 'error' event on a Socket throws and crashes
      // the process. `close()` only destroys the socket, it does not detach.
      socket.on("error", (err: Error) => {
        if (connected) {
          // Post-connect transport error: end the stream cleanly so the
          // line iterator completes instead of crashing the process.
          socket.destroy();
        } else {
          reject(
            new IpcError(
              `cannot connect to IPC socket at ${this.socketPath}: ${err.message}`,
            ),
          );
        }
      });
      socket.once("connect", () => {
        connected = true;
        const rl = createInterface({ input: socket, crlfDelay: Infinity });
        const lines = rl[Symbol.asyncIterator]();
        void (async () => {
          const hello = await lines.next();
          if (hello.done || !hello.value.startsWith("hello")) {
            rl.close();
            socket.destroy();
            reject(
              new IpcError(
                hello.done
                  ? "IPC server closed connection before hello"
                  : `unexpected IPC greeting: ${hello.value}`,
              ),
            );
            return;
          }
          resolve({ socket, rl, lines });
        })();
      });
    });
  }
}

/** Resolve the {@link Subscriber} for `service` (`"shell"` for the panel).
 *
 * The socket is `<dir>/<service>.sock` (`shell` maps to `ipc.sock`) where
 * `<dir>` is `$GLIMPSE_IPC_DIR`, else `$XDG_RUNTIME_DIR/glimpse`. No
 * connection is made here. */
export function ipc(service = "shell"): Subscriber {
  return new Subscriber(socketPath(service));
}

function socketPath(service: string): string {
  const override = process.env.GLIMPSE_IPC_DIR;
  let dir: string;
  if (override) {
    dir = override;
  } else {
    const runtime = process.env.XDG_RUNTIME_DIR;
    if (!runtime) {
      throw new IpcError(
        "neither GLIMPSE_IPC_DIR nor XDG_RUNTIME_DIR is set; " +
          "cannot locate the Glimpse IPC socket",
      );
    }
    dir = join(runtime, "glimpse");
  }
  return join(dir, service === "shell" ? "ipc.sock" : `${service}.sock`);
}

function close(conn: Conn): void {
  conn.rl.close();
  conn.socket.destroy();
}

// The wire protocol splits client lines on whitespace and never unescapes
// the command name or a field key, so an `action`/key with whitespace would
// forge extra tokens or whole client lines. Values are safe (escaped).
function validateToken(label: string, token: string, forbidEq: boolean): void {
  if (token.length === 0) {
    throw new IpcError(`IPC ${label} must not be empty`);
  }
  if (/\s/.test(token)) {
    throw new IpcError(`IPC ${label} "${token}" must not contain whitespace`);
  }
  if (forbidEq && token.includes("=")) {
    throw new IpcError(`IPC param key "${token}" must not contain '='`);
  }
}

function escape(s: string): string {
  return s
    .replaceAll("\\", "\\\\")
    .replaceAll("\n", "\\n")
    .replaceAll("\t", "\\t")
    .replaceAll(" ", "\\s");
}

function unescape(s: string): string {
  let out = "";
  for (let i = 0; i < s.length; i++) {
    if (s[i] !== "\\") {
      out += s[i];
      continue;
    }
    const next = s[++i];
    if (next === "s") out += " ";
    else if (next === "n") out += "\n";
    else if (next === "t") out += "\t";
    else if (next === "\\") out += "\\";
    else if (next === undefined) out += "\\";
    else out += "\\" + next;
  }
  return out;
}

function parseEvent(line: string): Event {
  const tokens = line.split(/\s+/);
  const name = tokens[0] ?? "";
  let ts = 0;
  const fields: Record<string, string> = {};
  for (const token of tokens.slice(1)) {
    const eq = token.indexOf("=");
    if (eq < 0) continue;
    const key = token.slice(0, eq);
    const value = unescape(token.slice(eq + 1));
    if (key === "ts" && /^\d+$/.test(value)) {
      ts = Number(value);
      continue;
    }
    fields[key] = value;
  }
  return { name, ts, fields };
}

function parseAck(line: string): Record<string, string> {
  const tokens = line.split(/\s+/);
  if (tokens[0] !== "ack") {
    throw new IpcError(`expected an ack, got: ${line}`);
  }
  let ok = false;
  const fields: Record<string, string> = {};
  for (const token of tokens.slice(1)) {
    const eq = token.indexOf("=");
    if (eq < 0) continue;
    const key = token.slice(0, eq);
    const value = unescape(token.slice(eq + 1));
    if (key === "ok") ok = value === "true";
    else fields[key] = value;
  }
  if (!ok) {
    throw new IpcError(fields.error ?? "command failed");
  }
  return fields;
}
