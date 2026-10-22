# glimpse-agents — one process for every system prompt

Epic: glimpse-tz18r (children glimpse-39oj9 polkit, glimpse-tz18r.1 NetworkManager, glimpse-tz18r.2 BlueZ).
Companion files in `docs/plans/glimpse-agents/`: approved previews (`agents_*.blp` + `.css`),
`deletions.md` (verified removal list), `acceptance.md` (acceptance criteria).

## 1. Current state

- NetworkManager secret agent: `crates/glimpse-services/src/services/network/agent.rs` (register `187–211`,
  per-pid identifier `183–185`), switched on by `Dependencies { agent }` (`network/mod.rs:425–427`) and
  published as `NetworkState.secret` (`network/mod.rs:200`).
- BlueZ pairing agent: `crates/glimpse-services/src/services/bluetooth/agent.rs` (`KeyboardDisplay`, `register`
  `172–197` with `RequestDefaultAgent`), `Dependencies { agent }` (`bluetooth/mod.rs:329–331`), published as
  `BluetoothState.pairing` (`bluetooth/mod.rs:144`); `Command::Pair` is refused without a local agent
  (`bluetooth/mod.rs:686–692`, `Failure::NoAgent` `failure.rs:25`).
- glimpse-panel starts both with `agent: true` (`crates/glimpse-panel/src/services.rs:117–123`) and shows
  `PairingDialog` / `SecretDialog` (`crates/glimpse-panel/src/app.rs:379–465`), plain `adw::AlertDialog`
  toplevels with no parent. The network applet asks for a Wi-Fi password inline before joining
  (`applets/network/indicator.rs:82–87, 314–329, 439–485`).
- glimpse-lock starts both with `agent: false` (`crates/glimpse-lock/src/app.rs:810–813`).
- No polkit code exists (`grep -ri polkit crates data`: two test names and one README line only). On this
  machine nothing answers polkit, so `pkexec` fails at once.

This plan adds `crates/glimpse-agents` (binary `glimpse-agents`, unit `glimpse-agents.service`) that owns the
polkit authentication agent, the NetworkManager secret agent and the BlueZ pairing agent, and deletes every
agent from glimpse-services, glimpse-panel and glimpse-widgets.

## 2. Remote facts (tag: measured on this machine 2026-10-22, or source)

| Fact | Tag |
| --- | --- |
| polkit 127-3, NetworkManager 1.58.1, bluez 5.87, gtk4 4.24.1, libadwaita 1.10.0, gtk4-layer-shell 1.3.0 | measured |
| `/usr/lib/polkit-1/polkit-agent-helper-1` is mode 755 (not setuid); `/run/polkit/agent-helper.socket` is active, `Accept=yes`, one oneshot helper per connection | measured |
| Socket mode: the agent writes `<username>\n` then `<cookie>\n`; the helper reads the user from the stream and the caller's uid/pidfd from `SO_PEERCRED`/`SO_PEERPIDFD` (`polkitagenthelper-pam.c:143–195`) | source, checked by hand |
| Setuid mode (polkit < 127, Debian 13 / Ubuntu 24.04): spawn `polkit-agent-helper-1 <username>`, write `<cookie>\n` | source |
| Helper lines: `PAM_PROMPT_ECHO_OFF <text>`, `PAM_PROMPT_ECHO_ON <text>`, `PAM_ERROR_MSG <text>`, `PAM_TEXT_INFO <text>` (text C-escaped, `g_strcompress` to undo), then `SUCCESS` or `FAILURE`; a prompt is answered with `<response>\n`; one connection = one attempt; the helper reports the result to polkitd itself | source |
| Agent interface `org.freedesktop.PolicyKit1.AuthenticationAgent`: `BeginAuthentication(s action_id, s message, s icon_name, a{ss} details, s cookie, a(sa{sv}) identities)`, `CancelAuthentication(s cookie)`; polkitd calls with no timeout | source |
| `RegisterAuthenticationAgent((sa{sv}) subject, s locale, s object_path)`; subject `("unix-session", {"session-id": <s>})`; a second agent for the session fails with `org.freedesktop.PolicyKit1.Error.Failed` "An authentication agent already exists for the given subject" | measured (introspect) + source |
| polkitd expands groups to users before calling the agent; identities arrive as `("unix-user", {"uid": <u>})`; `auth_admin` = `unix-group:wheel` (`50-default.rules`); `wheel:x:998:alex` | measured + source |
| NM `GetSecrets` flags: `ALLOW_INTERACTION=0x1`, `REQUEST_NEW=0x2`, `USER_REQUESTED=0x4`; NM waits 120 s for the reply; `UserCanceled` ends the request with no other agent asked | measured (headers) + source |
| `AddAndActivateConnection2` carries a full connection dict, so `connection.autoconnect` can be set at creation | measured (introspect) |
| BlueZ `Agent1`: `Release, RequestPinCode, RequestPasskey, DisplayPinCode, DisplayPasskey, RequestConfirmation, RequestAuthorization, AuthorizeService, Cancel`; replies time out after 60 s (`src/agent.c:40`); last `RequestDefaultAgent` wins; no default agent ⇒ incoming pairing fails with nothing queued | source |
| logind `Session.LockedHint` readable on the session path; glimpse resolves the session with `glimpse_dbus::login1::session_path` (`crates/glimpse-dbus/src/clients/login1.rs:132–175`) | measured + local |

## 3. Local code this plan reuses (file:line)

| Need | Reuse |
| --- | --- |
| Standalone relm4 binary skeleton, bus name taken first and exit when taken | `crates/glimpse-notifications/src/main.rs:16–52`, `glimpse_dbus` provider `start` `own_name` + `DoNotQueue` (`crates/glimpse-dbus/src/dbus.rs:68–72`, `provider.rs:27–38`), exit path `glimpse-notifications/src/app.rs:245–249` |
| Unit shape | `data/systemd/glimpse-notifications.service:1–22` (minus `NoNewPrivileges`), `glimpse-session.target:6` `Wants=` |
| `check` subcommand | `crates/glimpse-lock/src/cli.rs:46–52`, `commands.rs:56–70` |
| Layer-shell overlay with exclusive keyboard | `crates/glimpse-lens/src/overlay.rs:186–198` for `init_layer_shell`, `Layer::Overlay`, `KeyboardMode::Exclusive` only. That surface anchors all four edges with an exclusive zone; this one sets no anchor and no zone |
| LockedHint | `Login1SessionProxy::locked_hint` / `receive_locked_hint_changed` (`crates/glimpse-dbus/src/clients/login1.rs:58–67`), connect as `crates/glimpse-lock/src/logind.rs:58–70` |
| User and real names | `/etc/passwd` parse as `crates/glimpse-lock/src/user.rs` `resolve`, `display_name` `user.rs:29–34`; `glimpse_dbus::login1::current_uid` |
| One-time notification | `crates/glimpse-lock/src/notify.rs:51–78` (`FreedesktopNotificationsProxy::notify`) |
| NM agent body (move, do not rewrite) | `network/agent.rs` `may_prompt` `77–79`, `is_retry` `81–83`, `serviceable` `85–87`, `named` `89–94`, `answer_for` `101–119`, interface `121–181`, tests `network/agent/tests.rs` |
| BlueZ agent body (move) | `bluetooth/agent.rs`, tests `bluetooth/agent/tests.rs` |
| Input validation | `glimpse-widgets` `network_popover/imp.rs:283–303` (WPA 8–63 or 64 hex), `bluetooth_pairing_dialog` `PIN_MAX = 16`, `PASSKEY_MAX = 999_999`, `accepts` `imp.rs:171` |
| Text caps | `glimpse_utils::clean` (as in `glimpse-lock/src/status.rs:54`) |
| Styles + resources | `glimpse_widgets::Styles::install` (`glimpse-widgets/src/theme.rs:92`), `register_resources` (`lib.rs:388–390`) |
| Preview host | `crates/glimpse-widgets/examples/preview.rs` `main 55–70`, `activate 72–143`, `ensure_types 232–318`, `build 320–374`, `compile 376–393` |

## 4. Architecture

```figure
 polkitd ──BeginAuthentication──┐
 NetworkManager ──GetSecrets────┤      glimpse-agents (one process, one bus name)
 bluetoothd ──Agent1.*──────────┤   ┌──────────────────────────────────────────┐
                                └──►│ agents (zbus interfaces, tokio)          │
 logind LockedHint ────────────────►│   └─ Ask{prompt, reply} ──► App (relm4)  │
                                    │        FIFO queue, lock gate             │
 polkit-agent-helper-1 ◄─socket/───│        AgentDialog on one layer-shell    │
   (PAM conversation)   setuid      │        overlay surface, focused output   │
                                    └──────────────────────────────────────────┘
 glimpse-panel: ActivateConnection / Pair only — no agent, no dialog
```

- **One process, one bus name** `me.aresa.Glimpse.Agents`, requested with `DoNotQueue` on the session bus
  (`glimpse-dbus/src/dbus.rs:68–72`) before any agent is exported or registered; a second instance
  exits 1. glimpse-agents exports nothing on that name, so it does not use `provider::start`, which
  exports before owning (`provider.rs:34–38`).
- **Agents** are zbus interfaces served on the *system* bus connection, where polkitd, NetworkManager and
  bluetoothd call them. Only the well-known name lives on the session bus. Each turns a D-Bus call into an
  `Ask` sent to the app and awaits a `oneshot` reply. They never touch GTK.
- **App** (relm4 component) owns the queue, the lock gate and the only surface. It shows the head of the
  queue in `AgentDialog`; nothing else draws.
- **Polkit conversation** runs in a tokio task per attempt (`helper::Attempt`), streaming `Line`s to the app.
- **Surface**: `gtk4::Window` with gtk4-layer-shell, `Layer::Overlay`, `KeyboardMode::Exclusive`, no
  anchors (centered), namespace `glimpse-agents`, no monitor set, so niri and Hyprland place it on the
  focused output and give it the keyboard (Q2, Q5). No scrim. Created when the queue becomes non-empty and the screen is unlocked,
  destroyed when it empties.
- **Widget**: `AgentDialog` (GObject, Blueprint template `crates/glimpse-agents/blueprints/agent_dialog.blp`)
  built only from stock libadwaita: `Adw.StatusPage` → one `boxed-list` (`Adw.ComboRow` account,
  `Adw.EntryRow` username, `Adw.PasswordEntryRow` secret, `Adw.SwitchRow` autoconnect, `Adw.ExpanderRow`
  details) → code label → note label → Cancel / confirm pills. Rust only fills and toggles visibility.
  Look = the approved previews (§6).

### Review record

| When | Who | Result |
| --- | --- | --- |
| 2026-10-22 | user, grill session | 16 decisions (glimpse-tz18r note); dialog look approved from previews |
| 2026-10-22 | orchestrator, source check | corrected the socket-helper protocol (username line first) and BlueZ timeout (60 s, not 25 s) |
| 2026-10-22 | abram (grok-4.7), source + measured | Q1–Q5 closed. Removed the focused-output fallback, added the autoconnect write path and the show-view's 60 s close |
| 2026-10-22 | rabi (grok-4.7), adversarial review | 2 blockers, 8 majors, 6 minors; all taken. Exactly-once replies by dropping the `Ask`, `Reply::Shown` for BlueZ show-kinds with in-place replace, `Withdraw { key: None }` for `Agent1.Cancel`, a polkit `Conversation` type, `answer_for` with username, `write_autoconnect`, hand-written redacting `Debug`, gettext on every literal, a LockedHint follow stream, the name owned before export, three citations fixed. Its trust-after-pair and Q1 findings were already settled by Q8 and Q1 |

## 5. Types

`crates/glimpse-agents/src/ask.rs`

```rust
use tokio::sync::oneshot;
use zeroize::Zeroizing;

pub type Secret = Zeroizing<String>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Source {
    Polkit,
    Network,
    Bluetooth,
}

pub struct Ask {
    pub source: Source,
    pub key: String,
    pub prompt: Prompt,
    pub reply: Reply,
}

pub enum Prompt {
    Polkit(crate::polkit::Request),
    Secret(crate::network::Request),
    Pairing(crate::bluetooth::Request),
}

pub enum Reply {
    Polkit(oneshot::Sender<crate::polkit::Outcome>),
    Secret(oneshot::Sender<crate::network::Answer>),
    Pairing(oneshot::Sender<crate::bluetooth::Answer>),
    Shown,
}

pub enum Input {
    Ask(Ask),
    Withdraw { source: Source, key: Option<String> },
    Line { key: String, line: crate::polkit::helper::Line },
    Locked(bool),
    Dialog(crate::dialog::Response),
}
```

`key` identifies a request: the polkit cookie, `"{path}:{setting}"` for NM, the device object path for BlueZ.
`Withdraw { key: None }` removes every request of that source. BlueZ uses it for `Agent1.Cancel` and
`Release`, which name no device, and BlueZ runs one pairing at a time.

**Every waiter is answered exactly once.** An `Ask` leaves the queue in one of two ways. Either the user's
answer is sent on its `Reply`, or the `Ask` is dropped (withdrawn, refused by the lock gate, or the app
shutting down). The interface method awaits its receiver on the zbus task, and a closed channel
(`RecvError`) is its cancel answer: `org.freedesktop.PolicyKit1.Error.Cancelled`, `SecretAgent.Error.UserCanceled`,
`org.bluez.Error.Canceled`. That is today's rule (`network/agent.rs:70–73`, `bluetooth/agent.rs:36–39`).
A failed `send` to the app gives the same answers. Because a oneshot sender is consumed by `send`, a
late dialog response finds no `Reply` and sends nothing. `Reply::Shown` marks a BlueZ show-view: its D-Bus
method has already returned, so dropping it sends nothing.

An `Ask` whose `(source, key)` matches a queued `Reply::Shown` entry replaces that entry in place. This is
how repeated `DisplayPasskey` calls update the `entered` count.

`crates/glimpse-agents/src/polkit/mod.rs`

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Identity {
    pub uid: u32,
    pub user: String,
    pub name: String,
}

#[derive(Clone)]
pub struct Request {
    pub action_id: String,
    pub message: String,
    pub details: Vec<(String, String)>,
    pub cookie: String,
    pub identities: Vec<Identity>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Authorized,
    Cancelled,
}
```

`identities` is in polkitd's order, except that the session user (`glimpse_dbus::login1::current_uid()`) comes
first when present (decision 8). `user` and `name` come from `/etc/passwd` (the login name and the first
gecos field), read with `tokio::fs` and parsed the way `glimpse-lock/src/user.rs` `resolve` does. A uid
missing from the file is dropped from the list. `details` drops every key that starts with `polkit.`.
`Request` implements `Debug` by hand and prints `cookie: <redacted>`.

```rust
pub struct Conversation {
    pub account: usize,
    pub prompt: Option<(String, bool)>,
    pub note: Option<crate::dialog::Note>,
    pub busy: bool,
    pub attempt: Option<tokio::task::AbortHandle>,
    pub responder: Option<tokio::sync::mpsc::Sender<crate::ask::Secret>>,
}
```

The app keeps one `Conversation` for the polkit request at the head of the queue. `prompt` holds the
last `Line::Prompt`'s text and echo flag. `responder` feeds the attempt task, which owns the `Attempt`. That
task loops on `next()` and forwards each line as `Input::Line`. On each `Prompt` it awaits `responder` and
calls `respond`. After `Success` or `Failure` it exits. `Outcome::Authorized` is sent after the helper prints `SUCCESS` (the helper
has already told polkitd); `Cancelled` maps to `org.freedesktop.PolicyKit1.Error.Cancelled`.

`crates/glimpse-agents/src/polkit/helper.rs`

```rust
pub const SOCKET: &str = "/run/polkit/agent-helper.socket";
pub const SETUID: &[&str] = &[
    "/usr/lib/polkit-1/polkit-agent-helper-1",
    "/usr/libexec/polkit-agent-helper-1",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Line {
    Prompt { text: String, echo: bool },
    Error(String),
    Info(String),
    Success,
    Failure,
}

pub fn parse(line: &str) -> Option<Line>;

pub struct Attempt { /* stream + child, private */ }

impl Attempt {
    pub async fn start(user: &str, cookie: &str) -> std::io::Result<Self>;
    pub async fn next(&mut self) -> std::io::Result<Option<Line>>;
    pub async fn respond(&mut self, secret: &Secret) -> std::io::Result<()>;
}
```

`start` connects to `SOCKET` and writes `user\ncookie\n`; when `SOCKET` does not exist it spawns the first
existing `SETUID` path with `user` as `argv[1]` and writes `cookie\n`. Dropping an `Attempt` closes the
stream and kills a spawned child.

`crates/glimpse-agents/src/network.rs` (moved from `glimpse-services/.../network/agent.rs`)

```rust
pub const IDENTIFIER: &str = "me.aresa.glimpse.agents";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    pub name: String,
    pub path: String,
    pub setting: String,
    pub retry: bool,
    pub hints: Vec<String>,
    pub autoconnect: bool,
}

#[derive(Clone)]
pub enum Answer {
    Secret { secret: crate::ask::Secret, username: Option<String>, autoconnect: bool },
    Refused,
}

pub fn answer_for(setting: &str, hints: &[String], secret: &str, username: Option<&str>) -> Secrets;

pub async fn write_autoconnect(system: &zbus::Connection, path: &str, autoconnect: bool) -> zbus::Result<()>;
```

`Request` keeps today's fields (`agent.rs:22–28`) and adds `hints`, so that a VPN `username` hint shows the
username row (shown when `hints` contains the literal key `username`, an assumption about the VPN plugins),
and `autoconnect`, read from the `connection.autoconnect` of the dict GetSecrets carries
(absent = true). When `Answer::Secret.autoconnect` differs from `Request.autoconnect`, Q1's write follows.
Otherwise nothing is written.

`crates/glimpse-agents/src/bluetooth.rs` (moved from `glimpse-services/.../bluetooth/agent.rs`)

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    pub device: String,
    pub name: String,
    pub kind: Kind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kind {
    Confirm { passkey: u32 },
    Authorize,
    Pin,
    Passkey,
    ShowPin { pin: String },
    ShowPasskey { passkey: u32, entered: u16 },
}

#[derive(Clone, PartialEq, Eq)]
pub enum Answer {
    Confirm,
    Deny,
    Pin(String),
    Passkey(u32),
}
```

`Kind` maps 1:1 to today's `Prompt` (`bluetooth/mod.rs:79–96`), with `entered` kept. `Answer` is today's
(`agent.rs:67–73`). Show-kinds send `Reply::Shown` and return `Ok(())` before the app has seen them.
Every other kind awaits its `Reply::Pairing`. `Answer` and `network::Answer` implement `Debug` by hand and
redact every secret, as today's does (`network/agent.rs:36–42`).
`AuthorizeService` never reaches the queue: allowed when the device's `Bonded` property is true, refused
otherwise (today's rule, `bluetooth/mod.rs:599–605`). The agent reads `Bonded`, and `Alias` for
`Request.name`, itself through `glimpse_dbus::bluez`'s `Device1` proxy, with a 2 s timeout. A failed read
means not bonded and an empty name (the view then says "this device").

`crates/glimpse-agents/src/dialog/mod.rs` (render types the widget owns)

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    Lock,
    Wifi,
    Vpn,
    Bluetooth,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Field {
    Password,
    Visible,
    WifiKey,
    Pin,
    Passkey,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Note {
    Info(String),
    Error(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct View {
    pub icon: Icon,
    pub title: String,
    pub body: String,
    pub accounts: Vec<String>,
    pub username: Option<String>,
    pub secret: Option<(String, Field)>,
    pub autoconnect: Option<bool>,
    pub code: Option<String>,
    pub details: Vec<(String, String)>,
    pub note: Option<Note>,
    pub busy: bool,
    pub confirm: Option<String>,
    pub cancel: String,
}

pub enum Response {
    Confirm { account: usize, username: String, secret: crate::ask::Secret, autoconnect: bool },
    Cancel,
}
```

Visibility rules: account row when `accounts.len() > 1`; username row when `Some`; secret row when `Some`
(title = the `String`, validation = the `Field`; `Field::Visible` is a `PAM_PROMPT_ECHO_ON` answer and
shows the text as typed); switch row when `Some`; details row when non-empty; code
label when `Some`; confirm pill when `Some` (BlueZ show-kinds have Cancel only). Confirm is insensitive
while `busy` or while the secret fails its `Field` rule. Enter activates confirm, Escape is Cancel.
`AgentDialog::show(&View)` compares before every write; `connect_response(Fn(&AgentDialog, Response))` is
the one signal. Every string from another program passes `glimpse_utils::clean` with a cap
(title 120, body 240, detail value 240, account 64). Labels are set with plain `set_text`, never markup,
because `clean` keeps `<b>`. Every literal in the table below is `gettext("…")` with named placeholders,
for example `gettext("Join “{name}”")`. The process calls `init_translations` as
`glimpse-notifications/src/main.rs:36` does.

### View per request

| Request | icon | title | body | rows | confirm / cancel |
| --- | --- | --- | --- | --- | --- |
| polkit | Lock | "Authentication needed" | `message` | accounts (if >1), secret = PAM prompt text or "Password" (`Field::Password`, `Field::Visible` for an echo-on prompt), details = `action_id` + details map | "Authorize" / "Cancel" |
| NM Wi-Fi | Wifi | "Join “{name}”" | "The network needs a password" / retry: "That password didn't work" (error note) | secret "Password" `Field::WifiKey`, autoconnect `Some(request.autoconnect)` | "Join" / "Try again" ; "Cancel" |
| NM VPN | Vpn | "Sign in to “{name}”" | — | username when a hint asks for it, secret "Password" `Field::Password` | "Connect" / "Cancel" |
| BlueZ Confirm | Bluetooth | "Pair with “{name}”?" | "Check that the device shows the same code" | code = passkey `%06u` split 3+3 | "Pair" / "Cancel" |
| BlueZ Pin | Bluetooth | "Pair with “{name}”" | "Enter the PIN from the device's manual — often 0000" | secret "PIN" `Field::Pin` | "Pair" / "Cancel" |
| BlueZ Passkey | Bluetooth | "Pair with “{name}”" | "Enter the code shown on the device" | secret "Code" `Field::Passkey` | "Pair" / "Cancel" |
| BlueZ ShowPin/ShowPasskey | Bluetooth | "Pair with “{name}”" | "Type this code on the device, then press Enter" | code | — / "Cancel" |
| BlueZ Authorize | Bluetooth | "“{name}” wants to connect" | "It has never been paired with this computer" | — | "Allow" / "Deny" |

## 6. Mockups

Approved previews (user, 2026-10-22), copied from `var/widget_examples/` into
`docs/plans/glimpse-agents/`: `agents_polkit_{1..6}` (asking, details open, two admins, checking, wrong
password, fingerprint), `agents_wifi_{1..3}` (first ask, wrong password, VPN), `agents_bt_{1..4}` (compare
code, PIN, type on keyboard, unknown device), `agents_session_{1..3}` (out of scope, glimpse-0l06n). Render
with `just preview docs/plans/glimpse-agents/agents_polkit_1.blp`. Look: stock libadwaita on `--gl-surface`,
`--gl-radius`, `--gl-elevation-floating`, 1 px top highlight, 200 ms scale-fade in, `min-width: 27rem`, icon
1.6 rem white in an accent-tinted circle, no bold, no dimmed text, the user's own name never shown by
default. Corrections to the previews: Wi-Fi uses `network-wireless-symbolic`; the details rows show
`action_id` and polkit's `details` map, not a vendor (polkit does not send one).

## 7. Flows

### F1 Polkit — happy path
1. Start: `RegisterAuthenticationAgent(("unix-session", {"session-id": <logind Id>}), $LANG or "C", "/org/freedesktop/PolicyKit1/AuthenticationAgent")`.
2. `pkexec true` → polkitd → `BeginAuthentication`. The interface resolves identities (`unix-user` uid →
   `/etc/passwd` user + AccountsService `RealName`), orders them, and sends `Ask{Polkit, key = cookie}`.
3. App queues it. When it is the head and the screen is unlocked, the app starts `Attempt::start(user,
   cookie)` for the selected account and shows the view with `busy = false`. View = `view_for(&Prompt, Option<&Conversation>)`.
4. Helper `PAM_PROMPT_ECHO_OFF Password:` → secret row title "Password" (trailing colon stripped). User
   types, Enter → `busy = true`, `Attempt::respond`.
5. Another `PAM_PROMPT_*` on the same connection (for example OTP after password) shows the row again
   with its text. `SUCCESS` → reply `Authorized`, the dialog closes, the next queued request opens.

### F2 Polkit — failure, cancel, info
- `FAILURE` → error note "Wrong password", the field clears and shakes, `busy = false`, a fresh `Attempt`
  starts. No limit on attempts.
- `PAM_TEXT_INFO` / `PAM_ERROR_MSG` → info / error note with the helper's text (fingerprint hints).
- `PAM_PROMPT_ECHO_ON` → the secret row shows its text with visible input.
- Choosing another account in the Account row drops the current `Attempt` and starts one for that user.
- Cancel / Escape → drop `Attempt`, reply `Cancelled`.
- `CancelAuthentication(cookie)` → `Withdraw`: open → close dialog and drop `Attempt`; queued → removed
  silently.
- `Attempt::start` fails (no socket, no helper) → error note "Can't verify passwords" and Authorize stays
  insensitive; the request ends only on Cancel or `CancelAuthentication`.

### F3 Wi-Fi join (panel)
1. The user picks an unsaved WPA network; the panel calls `connect_access_point(id)` with no secret.
2. NM → `GetSecrets(flags with USER_REQUESTED|ALLOW_INTERACTION)` → `Ask{Network, key = path:setting}` →
   Wi-Fi view; Join is insensitive until the key is 8–63 characters or 64 hex digits.
3. Join → reply `Answer::Secret` → `answer_for` shapes the secret by setting and hint (moved code). When
   the switch differs from the profile, the agent then calls `Settings.Connection.GetSettings` on the
   request's path and `Update2(settings with connection.autoconnect set, flags TO_DISK|NO_REAPPLY = 0x41, {})`.
   GetSettings returns no secrets, and an update without secrets keeps the stored ones (Q1). The profile
   already exists at GetSecrets time, because `AddAndActivateConnection2` adds it before activating.
4. Wrong key → NM calls `GetSecrets` again with `REQUEST_NEW` → retry view ("That password didn't work").
5. Cancel → `Answer::Refused` → `UserCanceled`; NM stops asking.
6. `CancelGetSecrets` → `Withdraw`. NM's own 120 s timeout cancels the call the same way.
7. `SaveSecrets` / `DeleteSecrets` answer `InternalError` (moved as-is, `network/agent.rs:166–180`): the agent
   stores nothing, NetworkManager keeps system-owned secrets itself.

### F4 Wi-Fi nobody asked for
NM reconnects to a saved network whose password changed → `GetSecrets(REQUEST_NEW)` → the same dialog at
once (decision 5); it takes the keyboard like every other request.

### F5 VPN
`GetSecrets(setting = "vpn", hints)` → VPN view; a `username`-like hint shows the username row; the answer
goes out as `vpn.secrets` keyed by hint (moved `answer_for`).

### F6 Bluetooth
- Start: `RegisterAgent(PATH, "KeyboardDisplay")` then `RequestDefaultAgent` (moved). Re-register when
  `org.bluez` reappears.
- `RequestConfirmation` / `RequestPinCode` / `RequestPasskey` / `RequestAuthorization` → `Ask{Bluetooth,
  key = device path}` → view → `Answer`; Cancel → `org.bluez.Error.Rejected`.
- `DisplayPinCode` / `DisplayPasskey` reply at once (as today) and queue a show-view. BlueZ sends no
  `Cancel` for these (Q4), so the view closes on the device's `Paired` turning true (a `Device1`
  property watch), on the user's Cancel (which calls `Device1.CancelPairing`), or 60 s after it opened,
  BlueZ's own request bound, because a failed pairing changes no property.
- `AuthorizeService` → bonded allowed, otherwise refused; never a dialog.
- BlueZ's 60 s timeout surfaces as `Agent1.Cancel` → `Withdraw` of the open pairing; `Release` withdraws
  every BlueZ request.

### F7 Queue and lock
- One FIFO across sources. The head is shown; the rest wait. `Withdraw` of a waiting request removes it
  silently; of the head closes the dialog and opens the next.
- `lock.rs` reads `Login1SessionProxy::locked_hint` once, then follows `receive_locked_hint_changed()` and
  sends `Input::Locked` on every change (`glimpse_dbus::login1`, `clients/login1.rs:58–67`).
- `LockedHint` true: the surface is destroyed. An open polkit request goes back to the queue front, its
  `Conversation` dropped (aborting the attempt); it reopens fresh after unlock. Every NM request (open
  or queued) is answered `Answer::Refused` (`UserCanceled`), every waiting BlueZ request `Answer::Deny`
  (`Rejected`), and BlueZ show-views are dropped. New NM/BlueZ requests while locked are answered the
  same way at once; new polkit requests queue.
- `LockedHint` false: the head is shown again.

### F8 Process and desktop integration
- `glimpse-agents.service` (`Type=dbus`, `BusName=me.aresa.Glimpse.Agents`, `PartOf=` / `Requisite=` /
  `After=glimpse-session.target`, `Restart=on-failure`), listed in `glimpse-session.target` `Wants=`, the
  justfile `binaries` list (`justfile:10`), `scripts/check-units.sh` and `crates/glimpse-package/Cargo.toml`
  assets. The unit carries no `NoNewPrivileges=` and no user-namespace hardening, because the setuid
  fallback must keep working.
- Polkit registration fails with "already exists" → log once, post one notification ("Another
  authentication agent is running; glimpse can't ask for admin passwords"), keep the NM and BlueZ agents.
- `glimpse-agents check` prints one line per agent: polkit helper mode (socket / setuid / missing), polkit
  registration (ok / another agent / failed — probed by register+unregister when glimpse-agents is not
  running, otherwise "running"), NM AgentManager reachable, BlueZ AgentManager reachable, logind session
  found. Exit 1 when any line fails.
- NetworkManager or bluetoothd restart → re-register on `NameOwnerChanged` (today's `reregister` logic,
  `network/mod.rs:898–917`, `bluetooth/mod.rs:886–903`).

### F9 Panel after the move
The panel and glimpse-lock run `Network` and `Bluetooth` with `type Dependencies = ()` and no agent code.
Joining passes no secret (`connect_access_point(id)`, `connect_hidden(ssid, security)`). `Pair` always goes
to BlueZ, and a pairing that fails because nothing answered is an ordinary pairing failure notification.
glimpse-services drops its `zeroize` dependency. The full removal list, with callers and tests, is
`deletions.md`. Q6–Q8 settle its "Unsure" items 1, 2 and 4. Items 6–8 are left to the compiler and clippy.

### F10 Previews
`glimpse_widgets::preview::run(ensure_types: fn())` holds today's host (`examples/preview.rs`), compiling
to a per-process path (`glimpse-preview-{pid}.ui`, fixes glimpse-7780n). `crates/glimpse-widgets/examples/
preview.rs` and `crates/glimpse-agents/examples/preview.rs` each call it with their own types. The justfile
`preview` recipe uses `-p glimpse-agents` when the blueprint path is under `crates/glimpse-agents/` or
`docs/plans/glimpse-agents/`, else `-p glimpse-widgets`.

## 8. Config

None. glimpse-agents reads `Config` only for appearance (theme, variant, animation speed), exactly as
glimpse-notifications does (`main.rs:33–35`, `app.rs:218`). No new table, no new field (decision 13).

## 9. How the pieces connect

polkitd, NetworkManager and bluetoothd call the agent interfaces; each interface converts its call into
`Input::Ask` on the app's relm4 sender and awaits the matching `Reply`. The app keeps
`queue: VecDeque<Ask>` and `locked: bool`; it renders the head through `View` (built by
`view_for(&Prompt, &State)`) into `AgentDialog`, and `AgentDialog::connect_response` comes back as
`Input::Dialog(Response)`. For polkit the app owns the head's `Attempt` task, whose lines return as
`Input::Line`. Source cancellations arrive as `Input::Withdraw`. The logind follower sends
`Input::Locked`. No other process talks to glimpse-agents.

## 10. Struck

- ~~Notification-first prompts~~ (decision 5: every request is a dialog at once).
- ~~Scrim~~ (decision 1). ~~xdg toplevel / `Adw.Dialog`~~ (focus not guaranteed; `Adw.Dialog` renders
  empty in the preview host).
- ~~Per-agent config switches~~ (decision 13).
- ~~Retry cap~~ (decision 9). ~~Showing the user's own name/avatar~~ (decision 8).
- ~~Reusing `PasswordPrompt`~~ — the approved look is stock `Adw.PasswordEntryRow`.
- ~~libpolkit-agent-1 bindings / `polkit-agent-rs`~~ — the helper protocol is five line types; a GLib
  binding would pull GObject polkit types into a tokio process.
- ~~Separate binaries per agent~~ (epic rationale). ~~Requesting app icon~~ (polkit often sends none).
- ~~Session dialogs~~ → glimpse-0l06n.
- ~~Trusting a device that paired itself~~ (Q8).
- ~~D-Bus activation file for `me.aresa.Glimpse.Agents`~~ — nobody activates it by name; the session
  target starts it.

## 11. Open questions

- ~~Q1 How does "Connect automatically" reach NetworkManager?~~ `Settings.Connection.Update2` with the full
  `GetSettings` profile, `connection.autoconnect` changed, no secrets, flags `0x41`. NM merges the stored and
  agent secrets back only when the update has none (`nm-settings-connection.c:1536–1554`, 1.58.1, source).
  `BLOCK_AUTOCONNECT` (0x20) is not the property, so it is not used.
- ~~Q2 Which output does a layer surface with no monitor get?~~ The focused one: niri
  `layout.active_output()` (`src/handlers/layer_shell.rs:28–36`, 26.04, source; niri 26.04 measured), and
  Hyprland `focusState()->monitor()` (`LayerSurface.cpp:29`, source). There is no fallback code.
- ~~Q3 Which polkit details are shown?~~ pkexec 127 sends `user`, `user.gecos`, `user.display`, `program`,
  `command_line`, `cmdline_short` and `polkit.*` (`pkexec.c:855–900`, source). The dialog shows every key
  except those starting with `polkit.`, sorted by key, each value capped.
- ~~Q4 How does a BlueZ show-kind learn that pairing finished?~~ `Device1.Paired` turns true
  (`device.c:7204–7230`, 5.87, source). `DisplayPasskey` is fire-and-forget and never gets `Cancel`
  (`agent.c:788–812`, `307–315`), and a failure sets nothing, hence the 60 s close in F6.
- ~~Q5 Does the surface get the keyboard on map?~~ Yes, on the focused output, at the refresh after map
  (`src/niri.rs:1174–1219`, source), unless the lock screen, a screenshot UI or the exit dialog holds it.
  Hyprland takes focus in `onMap` (`LayerSurface.cpp:206–222`).
- ~~Q6 Does the panel's hidden-network page survive?~~ Yes, as a security dropdown only: NetworkManager can't
  guess the key management of a hidden SSID (`call::key_mgmt`), and the type is not a secret. The password
  box, `open_choice` and `Entered::{Secret, Passphrase}` go (`deletions.md` "NetworkPopover prompt page",
  reading (a)).
- ~~Q7 `Command::CancelPairing` loses its only caller — keep it?~~ No. Delete the command, its handle method,
  `Action::CancelPairing` and `call::cancel_pairing` from glimpse-services. glimpse-agents calls
  `Device1.CancelPairing` itself through `glimpse_dbus::bluez` (F6). The proxy stays in glimpse-dbus.
- ~~Q8 Who trusts a device that a remote pairing bonded (`Bluetooth::bonded`, `bluetooth/mod.rs:906–926`)?~~
  Nobody. A pairing the panel started still trusts and connects through `Settled` (`bluetooth/mod.rs:635–637`).
  A device that paired itself stays untrusted, and its services are still allowed because `AuthorizeService`
  allows every bonded device.
- ~~Q9 Where do the D-Bus proxies live?~~ In glimpse-dbus. `bluez::AgentManager1` (`clients/bluez.rs:105`),
  `network_manager::AgentManager` (`clients/network_manager.rs:631`), `AGENT_CAPABILITY_VPN_HINTS` and
  `SECRET_FLAG_*` stay and are consumed by glimpse-agents. A new `crates/glimpse-dbus/src/clients/polkit.rs`
  holds `AuthorityProxy` (`register_authentication_agent`, `unregister_authentication_agent`). The agent's
  interface is served by glimpse-agents.
