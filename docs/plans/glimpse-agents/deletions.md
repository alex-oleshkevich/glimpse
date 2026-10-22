# What goes away when the agents leave glimpse-services, glimpse-panel and glimpse-widgets

Decisions 2, 3, 11. Every row was checked against the working tree on 2026-10-22 (open file, line
confirmed). Line numbers are the current ones; apply deletions bottom-up per file.

## Dependencies: `()`, not a struct

`Service::Dependencies` is `type Dependencies: Send + 'static` (`crates/glimpse-services/src/service.rs:68`)
and `Running::spawn` / `Pending::start` take `S::Dependencies` by value (`service.rs:194-201`, `156`).
Twelve services already use `type Dependencies = ();` (compositor, heartbeat, tray, audio, mpris,
kdeconnect, notifications, calendar, places, printing, removable, and the test fixtures) and callers pass `()`
(`crates/glimpse-panel/src/services.rs:104`, `112-116`, `124`). **Network and Bluetooth become
`type Dependencies = ();`, `start(…, _dependencies: Self::Dependencies)`, and every caller passes `()`.**
`pub struct Dependencies` and the `NetworkDependencies` / `BluetoothDependencies` re-exports are deleted.
Nothing else in either struct (they hold only `agent: bool`).

## glimpse-services

### Whole files and modules

| path:lines | item | action | why |
| --- | --- | --- | --- |
| `crates/glimpse-services/src/services/network/agent.rs` (1-221) | `PATH`, `Request`, `Answer`, `AgentError`, `Agent`, `may_prompt`, `is_retry`, `serviceable`, `named`, `answer_for`, `register`, `unregister`, `identifier`, the `SecretAgent` zbus interface, `mod tests;` at 221 | **move to crates/glimpse-agents**, delete here | the NM secret agent |
| `crates/glimpse-services/src/services/network/agent/tests.rs` (1-224) | whole file (11 tests, see Tests) | **move** | exercises only the agent |
| `crates/glimpse-services/src/services/bluetooth/agent.rs` (1-213) | `PATH`, `CAPABILITY`, `AgentError`, `Agent`, `Answer` (67-73), the `Agent1` interface, `register`, `unregister`, `withdraw`, `mod tests;` at 213 | **move to crates/glimpse-agents**, delete here | the BlueZ agent |
| `crates/glimpse-services/src/services/bluetooth/agent/tests.rs` (1-226) | whole file (11 tests) | **move** | exercises only the agent |
| `crates/glimpse-services/src/services/network/tests/secrets.rs` (1-172) | whole file (4 tests) | **delete** | tests the service's single-slot secret state, which no longer exists |
| `crates/glimpse-services/src/services/bluetooth/tests/prompts.rs` (1-281) | whole file (9 tests) | **delete** (the AuthorizeService test moves) | every test drives `Event::Prompt`, `AnswerPairing`, `agent*` or `NoAgent` |

### network/mod.rs

| path:lines | item | action | why |
| --- | --- | --- | --- |
| `network/mod.rs:1` | `mod agent;` | delete | file gone |
| `network/mod.rs:22` | `pub use agent::{Answer, Request};` | delete | types gone |
| `network/mod.rs:25` | `pub type Secret = zeroize::Zeroizing<String>;` | delete | only the secret-carrying commands and the panel's join used it |
| `network/mod.rs:200` | `NetworkState.secret: Option<Request>` | delete | decision 11 |
| `network/mod.rs:327`, `333` | `secret: Option<Secret>` in `Command::ConnectAccessPoint` / `ConnectHidden` | delete the field | the join passes no secret |
| `network/mod.rs:357-360` | `Command::AnswerSecret { answer, reply }` | delete | agent-only |
| `network/mod.rs:399-404` | `Event::Secrets`, `Event::SecretsCancelled`, `Event::AgentRegistered` | delete | agent-only |
| `network/mod.rs:425-427` | `pub struct Dependencies { pub agent: bool }` | delete; `type Dependencies = ();` at 620 | see top |
| `network/mod.rs:448-450` | fields `agent_enabled`, `agent`, `secret` | delete | agent-only |
| `network/mod.rs:484-510` | `connect_access_point(id, secret)` / `connect_hidden(ssid, security, secret)` | change to `connect_access_point(id)` / `connect_hidden(ssid, security)` | decision 2 |
| `network/mod.rs:555-560` | `answer_secret` handle method | delete | agent-only |
| `network/mod.rs:652`, `673-675` | `dependencies` param, `agent_enabled`/`agent`/`secret` initialisers | rename param `_dependencies`; delete the three initialisers | |
| `network/mod.rs:679-683` | `async fn stop` (only unregisters the agent) | delete whole fn; trait default applies | nothing else to stop |
| `network/mod.rs:698-700` | `if !self.agent { self.reregister(ctx); }` in `Event::Enumerated` | delete | |
| `network/mod.rs:720` | `Event::AgentRegistered` arm | delete | |
| `network/mod.rs:726-747` | `Event::Secrets`, `Event::SecretsCancelled` arms | delete | |
| `network/mod.rs:756`, `761`, `764`, `767-769` | `self.agent = false` (x2), `self.reregister(ctx)`, `self.secret.take()` refusal inside `Event::NameOwner` | delete those lines | |
| `network/mod.rs:898-918` | `fn reregister` | delete | |
| `network/mod.rs:1303` | `secret: self.secret.as_ref()…` in `publish` | delete | |
| `network/mod.rs:1422`, `1458`; `1468-1473`, `1487` | `secret` binding in `Command::ConnectAccessPoint` / `ConnectHidden` arms and the `secret.as_ref().map(…)` arguments | delete; pass no secret to `call::add_and_activate` | |
| `network/mod.rs:1597-1608` | `Command::AnswerSecret` arm | delete | |
| `network/mod.rs:8` | `use tokio::sync::oneshot;` | **keep** | `type Reply` at 276 still uses it |

### network/call.rs, failure, tests

| path:lines | item | action | why |
| --- | --- | --- | --- |
| `network/call.rs:65`, `84-92` | `secret` param and the `psk` / `wep-key0` insertion in `wifi_settings` | delete the param and the `if let Some(secret)` block | the profile is created keyless; NM asks the agent |
| `network/call.rs:139`, `147` | `secret` param of `add_and_activate`, argument to `wifi_settings` | delete | |
| `network/failure.rs:16` `NeedSecrets` | `Failure::NeedSecrets` and its mappings (32, 48, 76, 88) | **keep** | NM reports it when the agent refuses; the panel words it (`applets/network/render.rs:88`) |
| `network/call/tests.rs:4-36` | `a_secured_network_carries_a_key_management_setting_and_a_secret_only_when_one_was_typed` | change: drop the `asked` and `wep` halves (lines 16-35) and the `Some(...)` argument; keep the key-mgmt and no-`psk` assertions (5-15); rename (no "typed") | the secret param is gone |
| `network/call/tests.rs:5, 17-21, 29, 40, 117, 127, 136, 142, 152` | `wifi_settings(…, None)` calls | drop the trailing `None` argument | signature change |
| `network/tests/service.rs:16-22` | `harness()` passing `Dependencies { agent: true }`, `harness_without_agent()` | `harness()` calls `started(())`; delete `harness_without_agent` | |
| `network/tests/service.rs:24` | `async fn started(dependencies: Dependencies)` | change to `started(dependencies: ())` or drop the parameter and pass `()` to `Network::start` at line 40ish | no struct |
| `network/tests/service.rs:240-262`, `264-278` | the two `…agent_registration` tests | delete | |
| `network/tests.rs:74` | `mod secrets;` | delete | |

### bluetooth/mod.rs

| path:lines | item | action | why |
| --- | --- | --- | --- |
| `bluetooth/mod.rs:1` | `mod agent;` | delete | |
| `bluetooth/mod.rs:16` | `pub use agent::Answer;` | delete | |
| `bluetooth/mod.rs:78-109` | `enum Prompt` and `impl Prompt { fn device }` | delete | only the agent and `BluetoothState.pairing` carried it |
| `bluetooth/mod.rs:144` | `BluetoothState.pairing: Option<Prompt>` | delete | decision 11 |
| `bluetooth/mod.rs:276-279` | `Command::AnswerPairing` | delete | |
| `bluetooth/mod.rs:309-316` | `Event::Prompt`, `PromptGone`, `AuthorizeService`, `AgentReleased`, `AgentRegistered` | delete | |
| `bluetooth/mod.rs:329-331` | `pub struct Dependencies { pub agent: bool }` | delete; `type Dependencies = ();` at 471 | see top |
| `bluetooth/mod.rs:341-343` | fields `agent_enabled`, `agent`, `prompt` | delete | |
| `bluetooth/mod.rs:434-439` | `answer_pairing` handle method | delete | |
| `bluetooth/mod.rs:498`, `508-510`, `517` | `dependencies` param, three initialisers, `service.reregister(ctx)` in `start` | `_dependencies`; delete | |
| `bluetooth/mod.rs:529-531` | `if self.agent { agent::unregister(connection).await; }` in `stop` | delete (keep the scan stop at 526-528) | |
| `bluetooth/mod.rs:556`, `560` | `self.agent = false`, `self.reregister(ctx)` in `NameOwner(Some)` | delete | |
| `bluetooth/mod.rs:563-564` | `self.agent = false; self.prompt = None;` in `NameOwner(None)` | delete | |
| `bluetooth/mod.rs:575-605` | `AgentRegistered`, `AgentReleased`, `Prompt`, `PromptGone`, `AuthorizeService` arms | delete | |
| `bluetooth/mod.rs:622` | `self.bonded(ctx, &path);` | delete | |
| `bluetooth/mod.rs:664` | `Command::AnswerPairing` arm in `run` | delete | |
| `bluetooth/mod.rs:686-692` | `Command::Pair { id, reply } if !self.agent` arm (sets `Failure::NoAgent`) | **delete**; the plain `Pair` arm at 782-787 then handles every pair | decision 3 |
| `bluetooth/mod.rs:754-759` | `Command::CancelPairing` dispatch arm | **keep** | BlueZ `Device1.CancelPairing`, not agent-only (see Unsure) |
| `bluetooth/mod.rs:799` | `Command::AnswerPairing { reply, .. } \| Command::DismissConfirmation { reply }` | change to `Command::DismissConfirmation { reply }` | exhaustiveness |
| `bluetooth/mod.rs:808-838` | `async fn answer` | delete | its `call::cancel_pairing` use goes with it; `Action::CancelPairing` stays for the command |
| `bluetooth/mod.rs:886-904` | `fn reregister` | delete | |
| `bluetooth/mod.rs:906-926` | `fn bonded` | delete | reads `self.prompt` only (see Unsure) |
| `bluetooth/mod.rs:922` | `self.prompt = None` | (inside `bonded`) | |
| `bluetooth/mod.rs:1086`, `1091`, `1121` | `let pairing = …`, `pairing,` x2 in `publish` | delete; the early-return literal keeps `confirm, ..Default::default()` | |
| `bluetooth/mod.rs:1189` | `\| Command::AnswerPairing { reply, .. }` in `reject` | delete | |
| `bluetooth/mod.rs:8` | `use tokio::sync::oneshot;` | **keep** | `type Reply` (212) and `trust_then_connect` (940) |
| `bluetooth/mod.rs:259-262`, `273-275`, `402-407`, `427-432` | `CancelPairing`, `DismissConfirmation` and their handle methods | **keep** | see KEEP list |

### bluetooth/failure.rs, tests

| path:lines | item | action | why |
| --- | --- | --- | --- |
| `bluetooth/failure.rs:25` | `Failure::NoAgent` | delete | decision 3 |
| `bluetooth/failure.rs:10, 22-24` | `Action::CancelPairing`, `PairingRejected/Timeout/Canceled` | keep | BlueZ `Pair()` / `CancelPairing()` results |
| `bluetooth/tests.rs:21`, `24-25` | `bluetooth()` passing `Dependencies { agent: true }`; `started(dependencies: Dependencies)` | `started(())` / drop the parameter | |
| `bluetooth/tests.rs:167` | `pairing: None` in the `state` fixture | delete | field gone |
| `bluetooth/tests.rs:180` | `mod prompts;` | delete | |
| `bluetooth/tests/mirror.rs:49-68` | `a_prompt_survives_an_adapter_the_service_has_not_seen_yet` | delete | drives `Event::Prompt` |
| `bluetooth/tests/pairing.rs` | all four tests | keep (no prompt/agent use; `only_a_pairing_this_service_was_waiting_on_trusts_and_connects` covers `Settled`) | |

### Re-exports, manifest, docs

| path:lines | item | action | why |
| --- | --- | --- | --- |
| `services/mod.rs:77-79` (the `pub use bluetooth::{ … }` block) | `Answer`, `Prompt`, `Dependencies as BluetoothDependencies` | delete the three names | |
| `services/mod.rs:121-125` | `Answer as SecretAnswer`, `Dependencies as NetworkDependencies`, `Request as SecretRequest`, `Secret as NetworkSecret` | delete the four names | |
| `crates/glimpse-services/Cargo.toml:44` | `zeroize.workspace = true` | delete | its only users were `network/agent.rs`, `network/mod.rs:25`, `network/tests/secrets.rs` |
| `crates/glimpse-services/README.md:146-153` | bluetooth paragraph "a pairing prompt and a confirmation in one state value" | change to "a confirmation"; **Pairing ends connected** stays | `state.pairing` gone |
| `crates/glimpse-services/README.md:155-161` | network paragraph: "the secret prompt in one state value"; the sentence "NetworkManager stores every secret … a password typed before a join travels in the profile … `REQUEST_NEW` implies interaction …" | delete "and the secret prompt"; delete the secret sentence (it is agent knowledge; moves to glimpse-agents README) | no secret path in this crate |
| `docs/handbook.md:148-152` | "the third exception: an incoming service from a bonded device is authorized" (`Agent1.AuthorizeService`) | move to the glimpse-agents rule set; delete here | `AuthorizeService` leaves the crate |
| `docs/handbook.md:139` | "a settled pairing trusts and connects" | keep | `Settled` handler stays |

## glimpse-panel

| path:lines | item | action | why |
| --- | --- | --- | --- |
| `crates/glimpse-panel/src/services.rs:15`, `20` | `BluetoothDependencies`, `NetworkDependencies` imports | delete | |
| `crates/glimpse-panel/src/services.rs:117-121`, `122-123` | `Running::<Bluetooth>::spawn(…, BluetoothDependencies { agent: true })`, `Running::<Network>::spawn(…, NetworkDependencies { agent: true })` | change to `…spawn(document, buses.clone(), ())` | |
| `crates/glimpse-panel/src/app.rs:9` | `use zeroize::Zeroizing;` | delete | only user was `show_secret` |
| `crates/glimpse-panel/src/app.rs:15-18` | imports `Answer`, `BluetoothError`, `BluetoothHandle`, `NetworkError`, `NetworkHandle` | delete those five (keep `CommandError, ExecState, SessionAction, SessionActionsHandle`) | all used only by pairing/secret code |
| `crates/glimpse-panel/src/app.rs:21-24` | `NetworkEntered, PairingAnswer, PairingDialog, PairingEntry as Entry, SecretAnswer, SecretDialog` | delete; keep `Styles` | |
| `crates/glimpse-panel/src/app.rs:33` | `applets::bluetooth::{NAME_CAP, prompt_entry}` | delete the import | |
| `crates/glimpse-panel/src/app.rs:2` | `use adw::prelude::{AdwDialogExt, AlertDialogExt}` | **keep** | `show_session_dialog` uses `present` and `AlertDialog` |
| `crates/glimpse-panel/src/app.rs:40-46` | `struct SecretPrompt` | delete | |
| `crates/glimpse-panel/src/app.rs:63` | `type Pairing` | delete | |
| `crates/glimpse-panel/src/app.rs:67-68`, `85-86` | `AppInput::BluetoothPrompt`, `AppInput::NetworkSecret` and their `Debug` names | delete | |
| `crates/glimpse-panel/src/app.rs:110-111`, `190-191`, `253-260`, `350-355` | `bluetooth_watch`, `network_watch` fields, initialisers, spawns in `ServicesReady`, aborts in `shutdown` | delete | the watches only fed the two dialogs |
| `crates/glimpse-panel/src/app.rs:114-115`, `194-195` | fields `pairing`, `secret` and initialisers | delete | |
| `crates/glimpse-panel/src/app.rs:274-289` | `AppInput::BluetoothPrompt`, `AppInput::NetworkSecret` arms | delete | |
| `crates/glimpse-panel/src/app.rs:379-414` | `fn show_pairing` | delete | decision 11 |
| `crates/glimpse-panel/src/app.rs:416-465` | `fn show_secret` (incl. its doc comment) | delete | decision 11 |
| `crates/glimpse-panel/src/app.rs:536-543` | `fn prompt_report` | delete | only the two dialogs used it |
| `crates/glimpse-panel/src/app.rs:545-556` | `fn is_open`, `fn close` | delete | only the two dialogs used them |
| `crates/glimpse-panel/src/app.rs:565-572` | `fn answered` | delete | |
| `crates/glimpse-panel/src/app.rs:574-597` | `fn spawn_network_watch` | delete | |
| `crates/glimpse-panel/src/app.rs:599-630` | `fn spawn_bluetooth_watch` | delete | |
| `crates/glimpse-panel/src/app.rs:19-20`, `31-38` | `ComponentActions`, `color_scheme`, `spawn_reported`, `Report` and the rest | keep | session dialog / exec watch still use them |
| `crates/glimpse-panel/Cargo.toml:37` | `zeroize.workspace = true` | delete | no remaining use in `crates/glimpse-panel/src` |
| `crates/glimpse-panel/src/applets/bluetooth/mod.rs:5` | `pub(crate) use render::{NAME_CAP, prompt_entry};` | delete | `NAME_CAP` was re-exported only for `app.rs` |
| `crates/glimpse-panel/src/applets/bluetooth/render.rs:3` | `Prompt` in the `glimpse_services` import | delete | |
| `crates/glimpse-panel/src/applets/bluetooth/render.rs:5-6` | `PASSKEY_MAX, PIN_MAX, PairingEntry` | delete | |
| `crates/glimpse-panel/src/applets/bluetooth/render.rs:68-89` | `pub fn prompt_entry` | delete | |
| `crates/glimpse-panel/src/applets/bluetooth/render.rs:136-139` | `fn digits` | delete | only `prompt_entry` used it |
| `crates/glimpse-panel/src/applets/bluetooth/render.rs:323-325` | `Failure::NoAgent` arm of `wording` | delete | variant gone |
| `crates/glimpse-panel/src/applets/bluetooth/render.rs:11` | `pub const NAME_CAP` | change to private `const` if nothing outside uses it (it is used in-file at 22, 25, 132, 228) | re-export gone |
| `crates/glimpse-panel/src/applets/bluetooth/render.rs:90-120` | `asked`, `waiting`, `prompt` (the Forget confirmation) | **keep** | the Forget flow, not an agent |
| `crates/glimpse-panel/src/applets/network/indicator.rs:11` | `NetworkSecret` in the `glimpse_services` import | delete | |
| `crates/glimpse-panel/src/applets/network/indicator.rs:12` | `NetworkEntered` | delete if the Hidden page stays dropdown-only (see Unsure); otherwise keep | |
| `crates/glimpse-panel/src/applets/network/indicator.rs:22-29` | `enum Asking` | change: delete `Joining { id, name }`; `Name` and `Hidden { ssid }` stay; fix the doc comment ("asks twice … password") | the join asks nothing |
| `crates/glimpse-panel/src/applets/network/indicator.rs:47-54` | `pub fn entered_for` | delete | only `app.rs:585` and one test used it |
| `crates/glimpse-panel/src/applets/network/indicator.rs:79-87` | `fn asks_first` (and its doc comment) | delete | the inline password row |
| `crates/glimpse-panel/src/applets/network/indicator.rs:198-209` | `fn joining_entry` | delete | |
| `crates/glimpse-panel/src/applets/network/indicator.rs:232-240` | `Asking::Joining` arm of `ask()` | delete | |
| `crates/glimpse-panel/src/applets/network/indicator.rs:223-231` | `Asking::Hidden` arm of `ask()` (`entered: NetworkEntered::Secret`, question "…then type its password.") | change per Unsure | password entry goes |
| `crates/glimpse-panel/src/applets/network/indicator.rs:325-329` | `if let Some(name) = asks_first(&state, &id) { … return; }` in `connect_activated` | delete | |
| `crates/glimpse-panel/src/applets/network/indicator.rs:363` | `network.connect_access_point(id, None)` | `network.connect_access_point(id)` | |
| `crates/glimpse-panel/src/applets/network/indicator.rs:454-469` | `(Some(Asking::Hidden { ssid }), true)` arm: `secret` binding and `connect_hidden(ssid, security, secret)` | `network.connect_hidden(ssid, security)`; delete `secret` | |
| `crates/glimpse-panel/src/applets/network/indicator.rs:470-480` | `(Some(Asking::Joining { id, .. }), true)` arm | delete | |
| `crates/glimpse-panel/src/applets/network/mod.rs:6` | `pub use indicator::{Network, entered_for};` | `pub use indicator::Network;` | |
| `crates/glimpse-panel/src/applets/network/render.rs:222` | `network.security.needs_a_secret()` (row lock icon) | **keep** (so keep `Security::needs_a_secret` in glimpse-dbus) | not an agent path |
| `crates/glimpse-panel/README.md:184-185` | "**Every pairing prompt is the app's `PairingDialog`, never a popover page**, so it opens once however many bars carry the applet; only the Forget confirmation lives in the popover." | change to "only the Forget confirmation lives in the popover" | dialog gone |
| `crates/glimpse-panel/README.md:187-188` | "**The password is asked for before the join**, because NetworkManager drops the working connection the moment activation is requested." | delete | inline row gone |

## glimpse-widgets

| path:lines | item | action | why |
| --- | --- | --- | --- |
| `crates/glimpse-widgets/src/bluetooth_pairing_dialog/` (`mod.rs`, `imp.rs`, `tests.rs`) | `PairingDialog`, `PairingAnswer`, `Entry`, `PIN_MAX`, `PASSKEY_MAX`, `accepts`, `answer` | **delete** (design reborn in glimpse-agents blueprints) | decision 11 |
| `crates/glimpse-widgets/src/network_secret_dialog/` (`mod.rs`, `imp.rs`, `tests.rs`) | `SecretDialog`, `SecretAnswer`, `answer` | **delete** | decision 11 |
| `crates/glimpse-widgets/blueprints/bluetooth_pairing_dialog.blp` | `template $PairingDialog` | delete | |
| `crates/glimpse-widgets/blueprints/network_secret_dialog.blp` | `template $SecretDialog` | delete | |
| `crates/glimpse-widgets/build.rs:35-38` (entry at 36-37) | `bluetooth_pairing_dialog.blp → .ui` tuple | delete | |
| `crates/glimpse-widgets/build.rs:59-62` (entry at 60-61) | `network_secret_dialog.blp → .ui` tuple | delete | |
| `crates/glimpse-widgets/resources/glimpse-widgets.gresource.xml:7`, `13` | `widgets/bluetooth_pairing_dialog.ui`, `widgets/network_secret_dialog.ui` | delete | |
| `crates/glimpse-widgets/resources/widgets/{bluetooth_pairing_dialog,network_secret_dialog}.ui` | generated | nothing to do | gitignored (`.gitignore:30`); regenerated by build.rs |
| `crates/glimpse-widgets/src/lib.rs:7`, `40` | `mod bluetooth_pairing_dialog;`, `mod network_secret_dialog;` | delete | |
| `crates/glimpse-widgets/src/lib.rs:106-108` | `pub use bluetooth_pairing_dialog::{Entry as PairingEntry, PASSKEY_MAX, PIN_MAX, PairingAnswer, PairingDialog};` | delete | |
| `crates/glimpse-widgets/src/lib.rs:145` | `pub use network_secret_dialog::{SecretAnswer, SecretDialog};` | delete | |
| `crates/glimpse-widgets/src/tests.rs:3` | `use crate::bluetooth_pairing_dialog::PIN_MAX;` | delete (check no other test in the file needs it) | |
| `crates/glimpse-widgets/src/tests.rs:109`, `442` | `pairing_dialog_checks::check();`, `mod pairing_dialog_checks;` | delete | |
| `crates/glimpse-widgets/src/tests/pairing_dialog_checks.rs` | whole file (`check()`) | **delete** (cases move to glimpse-agents) | |
| `crates/glimpse-widgets/README.md:112-115` | "**A question a service raises is a dialog**, so it opens once however many bars exist: `PairingDialog` covers every pairing kind and clears its entry only when the **device** changes, not the name, so a credential cannot reach the next device." | delete the two sentences after "starts is a page of its own popover"; keep the first sentence (`NetworkPopover::set_prompt` clears the box on key change) | the rule is agent-owned now |
| `crates/glimpse-widgets/styles/glimpse.css` | selectors for the dialogs | none exist (no `pairing` / `secret-dialog` selector); `.prompt*` (945-990) are the **popover** prompt page and `.prompt__accept--danger` is read by `bluetooth_popover/mod.rs:15` | nothing to remove |
| `crates/glimpse-widgets/examples/preview.rs` `ensure_types` (232-318) | n/a | none | neither dialog is named there |
| `crates/glimpse-widgets/Cargo.toml:21` | `zeroize` | **keep** | `PasswordPrompt::take_text` |

### NetworkPopover prompt page (consequence of "no password in the panel")

These are dead once no `NetworkAsk` carries a password. Whether the Hidden security step survives is
in Unsure; rows marked * apply in either case.

| path:lines | item | action | why |
| --- | --- | --- | --- |
| `network_popover/imp.rs:31-37` | `Entered::{Secret, Passphrase}` * | delete both; only `Name` remains (the enum itself can go if nothing else reads it) | no password is typed in the panel |
| `network_popover/imp.rs:283`, `285-287`, `296-301` | `ENTRY_MAX`, `PASSPHRASE_MIN/MAX/HEX`, the `Passphrase` and `Secret` arms of `accepts` * | delete | used only for passwords |
| `network_popover/imp.rs:94-95`, `210`, `223-227` | `prompt_secret: TemplateChild<gtk4::PasswordEntry>`, its `notify::text` hookup, its `activate` hookup * | delete | |
| `network_popover/imp.rs:253-258`, `268-280` | `typed()` fallback to `prompt_secret`, `revalidate()` secret branch * | simplify to the name entry only | |
| `network_popover/mod.rs:176`, `221` | `imp.prompt_secret.set_text("")`, `grab_focus` * | delete | |
| `blueprints/network_popover.blp:145-147` | `Gtk.PasswordEntry prompt_secret` * | delete | |
| `network_popover/mod.rs:9` | `pub use imp::{Ask, Details, Entered, Entry, Line, Place, accepts}` | drop `accepts` if no outside user remains (only `network_secret_dialog` used it) | |
| `crates/glimpse-widgets/src/lib.rs:142` | `Entered as NetworkEntered` re-export | delete with the enum | |
| `src/tests/network_popover_checks.rs:7`, `458-514` | `type_in` secret branch; the `secret:`, `secret:Long`, `802-11-wireless-security:` retry cases | delete | |
| `src/tests/network_popover_checks.rs:516-555` | the `hidden-secret:` case with `open_choice` / `prompt_security` | change per Unsure | |
| `network_popover/imp.rs:46-47`, `260-266`; `mod.rs:138-140` | `Ask.open_choice`, `open_chosen`, `chosen()` | delete only if the security step is deleted | `open_choice` existed to hide the password box |

## Callers that must change

| path:line | now | new form |
| --- | --- | --- |
| `crates/glimpse-panel/src/services.rs:117-121` | `Running::<Bluetooth>::spawn(document, buses.clone(), BluetoothDependencies { agent: true })` | `Running::<Bluetooth>::spawn(document, buses.clone(), ())` |
| `crates/glimpse-panel/src/services.rs:122-123` | `…NetworkDependencies { agent: true }` | `Running::<Network>::spawn(document, buses.clone(), ())` |
| `crates/glimpse-panel/src/applet/runtime/tests.rs:707`, `720` | imports `BluetoothDependencies`; passes `BluetoothDependencies { agent: true }` | drop the import; pass `()` |
| `crates/glimpse-lock/src/app.rs:21`, `23` | imports `BluetoothDependencies`, `NetworkDependencies` | drop both |
| `crates/glimpse-lock/src/app.rs:810-811` | `Running::<Network>::spawn(&config, buses.clone(), NetworkDependencies { agent: false })` | `…spawn(&config, buses.clone(), ())` |
| `crates/glimpse-lock/src/app.rs:812-813` | `…BluetoothDependencies { agent: false }` | `…spawn(&config, buses, ())` |
| `crates/glimpse-lock/README.md:41-43` | "**`Network` and `Bluetooth` run with `agent: false`**, since a locked screen that registered an agent would accept a pairing or hand out a Wi-Fi password prompt to whoever is standing at it." | delete the sentence (no flag left; the lock-time refusal is decision 7, owned by glimpse-agents) |
| `crates/glimpse-panel/src/applets/network/indicator.rs:363` | `connect_access_point(id, None)` | `connect_access_point(id)` |
| `crates/glimpse-panel/src/applets/network/indicator.rs:467` | `connect_hidden(ssid, security, secret)` | `connect_hidden(ssid, security)` |
| `crates/glimpse-panel/src/applets/network/indicator.rs:478` | `connect_access_point(id, Some(secret))` | arm deleted |
| `crates/glimpse-panel/src/applets/bluetooth/indicator.rs:115` | `bluetooth.pair(id)` | unchanged (keep) |
| `crates/glimpse-panel/src/applets/bluetooth/indicator.rs:180-181`, `226-227` | `bluetooth.dismiss_confirmation()` | unchanged (keep) |
| `crates/glimpse-panel/src/applets/bluetooth/render/tests.rs:34` | `pairing: None` in the state fixture | delete the field |
| `crates/glimpse-panel/src/applets/bluetooth/render/tests.rs:332-336` | `fn asking(prompt) -> BluetoothState` | delete |

No other crate reads `state.secret`, `state.pairing`, `answer_secret`, `answer_pairing`,
`SecretAnswer`, `SecretRequest`, `NetworkSecret`, `Prompt`, `Failure::NoAgent`, `BluetoothDependencies`
or `NetworkDependencies` (`grep` over `crates`, `sdk`, `docs`, `data`, `scripts`, `website` outside
glimpse-services, glimpse-panel, glimpse-widgets, glimpse-lock).

## Tests

Paths are under `crates/glimpse-services/src/services/` unless stated.

### Move to crates/glimpse-agents

| file | tests |
| --- | --- |
| `network/agent/tests.rs` | all 11: `request_new_implies_interaction_even_without_allow_interaction`, `only_a_request_new_is_a_retry`, `glimpse_services_a_psk_and_a_vpn_and_refuses_enterprise`, `a_request_with_no_interaction_bit_answers_empty_without_asking_anyone`, `an_enterprise_request_is_answered_empty_and_never_prompts`, `a_retry_reaches_the_panel_marked_as_one`, `a_refused_prompt_is_a_user_cancellation_and_not_an_empty_answer`, `a_prompt_that_is_dropped_cancels_rather_than_hanging`, `an_answer_is_shaped_by_the_setting_and_named_by_the_hint`, `an_answer_never_prints_the_secret`, `the_agent_identifier_is_per_process` (the ones that talk to the service via `Event::Secrets` are rewritten against the agent's own queue; the pure ones move as they are) |
| `bluetooth/agent/tests.rs` | all 11: `a_prompt_whose_answer_is_dropped_is_canceled_rather_than_panicking`, `a_denied_confirmation_is_rejected_rather_than_canceled`, `a_pin_that_comes_back_as_a_passkey_is_refused`, `a_display_only_prompt_answers_at_once_and_parks_nothing`, `a_service_of_a_bonded_device_is_authorized`, `a_service_of_an_unbonded_device_is_refused`, `a_service_nobody_answers_is_refused_rather_than_hanging`, `release_tells_the_service_rather_than_unregistering`, `cancel_withdraws_the_prompt`, `registering_asks_bluez_to_make_the_agent_the_default_after_registering`, `a_refused_default_request_keeps_the_registration` |
| `bluetooth/tests/prompts.rs` | `an_incoming_service_is_authorized_by_the_bond_and_by_nothing_else` (bond-only `AuthorizeService` rule; the agent now reads `Bonded` from BlueZ itself) |
| `crates/glimpse-widgets/src/bluetooth_pairing_dialog/tests.rs` | `a_pin_is_one_to_sixteen_alphanumeric_characters`, `a_passkey_stops_at_six_nines`, `a_multibyte_name_is_capped_by_characters`, `a_typed_answer_is_read_back_from_the_response_and_its_value`, `a_prompt_without_an_entry_answers_with_a_bare_accept`, `each_prompt_kind_offers_exactly_its_own_buttons` (#[ignore]), `a_dropped_pairing_dialog_is_finalized` (#[ignore]) (rewritten against the new blueprints) |
| `crates/glimpse-widgets/src/network_secret_dialog/tests.rs` | `a_wireless_secret_is_held_to_the_passphrase_lengths_and_a_vpn_token_is_not`, `cancelling_is_a_refusal_and_never_an_empty_password`, `connecting_returns_what_was_typed`, `a_retry_clears_the_rejected_password_and_says_so` (#[ignore]), `a_hostile_network_name_is_capped_by_characters` |
| `crates/glimpse-widgets/src/tests/pairing_dialog_checks.rs` | `check()` (called only from `widgets()`), the cases for the pairing dialog |
| `bluetooth/render` (panel) `prompt_entry` tests, see below | the code-formatting expectations: `418 209`, `018 402\n3 of 6 entered`, clamp at 6 |

### Delete

| file | tests | why |
| --- | --- | --- |
| `network/tests/secrets.rs` | `a_second_secret_request_is_refused_rather_than_queued`, `answering_a_prompt_resolves_the_agent_and_clears_the_state`, `a_cancellation_naming_another_request_leaves_the_open_prompt_alone`, `networkmanager_leaving_the_bus_refuses_an_open_prompt` | the single-slot queue is replaced by decision 6; behaviour is re-specified in glimpse-agents |
| `network/tests/service.rs:240`, `264` | `mirror_only_mode_never_enables_agent_registration`, `the_panel_path_still_enables_agent_registration` | the flag is gone |
| `bluetooth/tests/prompts.rs` | `a_prompt_reaches_the_state_and_an_answer_clears_it`, `an_answer_with_nothing_waiting_is_refused`, `a_second_prompt_replaces_the_first_and_cancels_its_call`, `a_bond_clears_the_prompt_that_only_told_the_user_what_to_type`, `a_prompt_waiting_on_an_answer_is_not_retired_by_the_bond`, `mirror_only_mode_never_enables_agent_registration`, `the_panel_path_still_enables_agent_registration`, `pairing_without_an_agent_is_refused_in_its_own_words` | state, flag and `NoAgent` are gone |
| `bluetooth/tests/mirror.rs:50` | `a_prompt_survives_an_adapter_the_service_has_not_seen_yet` | `Event::Prompt` is gone |
| `crates/glimpse-panel/src/app/tests.rs:96-109` | `a_closed_prompt_dialog_is_not_open` (#[ignore]) | `is_open` and `PairingDialog` are gone |
| `crates/glimpse-panel/src/applets/network/indicator/tests.rs:26-33` | `a_vpn_token_is_not_held_to_a_wifi_passphrase_length` (`entered_for`) | `entered_for` is gone (the VPN/Wi-Fi distinction moves to the agents' dialog) |
| `crates/glimpse-panel/src/applets/network/indicator/tests.rs:117-150` | `a_join_that_would_need_a_password_asks_before_anything_is_torn_down` (`asks_first`) | `asks_first` is gone |
| `crates/glimpse-panel/src/applets/bluetooth/render/tests.rs:351-374` | `a_pairing_prompt_never_becomes_a_page_or_a_waiting_chip` | `Prompt` is gone; keep its last three assertions (`waiting(...)` is `None` with no confirmation, `prompt/waiting(&forgetting())` are `Some`) as the Forget-only case if `forgetting()` has no other test |
| `crates/glimpse-panel/src/applets/bluetooth/render/tests.rs:376-407` | `every_prompt_has_a_dialog_mode_and_the_code_it_shows` | `prompt_entry` gone (code formatting moves with it) |
| `crates/glimpse-panel/src/applets/bluetooth/render/tests.rs:409-422` | `a_passkey_is_six_zero_padded_digits_and_its_progress_stops_at_six` | `prompt_entry` gone (moves, see above) |
| `crates/glimpse-widgets/src/tests/network_popover_checks.rs:458-514` | the password cases inside `check()` | see the popover table |

### Change

| file | test | change |
| --- | --- | --- |
| `network/call/tests.rs:4` | `a_secured_network_carries_a_key_management_setting_and_a_secret_only_when_one_was_typed` | keep the first half (`key-mgmt` present, no `psk`); drop the typed-secret and WEP halves; rename |
| `network/call/tests.rs` (9 call sites) | `wifi_settings(…, None)` | drop the fourth argument |
| `bluetooth/tests.rs`, `network/tests/service.rs` | harness `started(Dependencies { … })` | `()` |
| `crates/glimpse-panel/src/applets/network/indicator/tests.rs:36-45` | `every_security_the_hidden_page_offers_has_a_label_and_the_first_is_the_open_one` | assertion message mentions `open_choice` suppressing the password box; adjust per Unsure |

Not touched: `a_device_that_is_pairing_spins_on_its_own_row` (`render/tests.rs:176`) and
`a_pairing_that_will_not_survive_a_restart_is_said_on_the_row` (`render/tests.rs:274`) test `Busy::Pairing`
and `bond != paired`, not the agent; `bluetooth/tests/confirmation.rs` (forget); `network/failure/tests.rs`
(`NeedSecrets`).

Panel runtime test `applet/runtime/tests.rs:416` (`an_applet_reaches_its_group`) stays; only the helper's
`BluetoothDependencies` use changes (Callers).

Translations: `check-strings` / the extractor regenerates `po/glimpse.pot` and `po/*.po`; do not hand-edit.
Strings that leave the three crates: "The pairing answer was not delivered.", "Bluetooth pairing" (dialog
title), "The password was not delivered.", "Network password", "Could not answer the request", "this device"
(`app.rs:614`), "{entered} of 6 entered", the `NoAgent` sentence in `wording`, "The network needs a password
before this computer can join it.", the dialog strings ("Enter the passkey", "Type the passkey shown on
{device}.", "Allow pairing", "Type this passkey", "That password did not work", "Enter the password for
{network}"). They reappear in glimpse-agents.

## Unsure

1. **The Hidden-network step.** Decision 2 deletes "the hidden-network password entry", but the Hidden page
   (`indicator.rs:223-231`, `HIDDEN_SECURITY`, `hidden_choices`, `Ask.choices`, `prompt_security`, `chosen()`)
   also collects the *security type*, which NM cannot guess for a hidden SSID and which `call::key_mgmt`
   needs. Two readings: (a) the security dropdown stays as a dropdown-only page (then `open_choice` and the
   password box go, and `Asking::Hidden` keeps `ssid`); (b) the whole second step goes and hidden joins
   with a fixed or NM-derived security. The tables above delete everything that is password-only under
   either reading; the choice-related items are marked "per Unsure". I did not pick.
2. **`Command::CancelPairing` has no caller after this change.** `grep` finds no `cancel_pairing(` in the
   panel; the only in-crate user is `answer()` (`mod.rs:835`), which is deleted. It is a BlueZ `Device1`
   call and not agent-only, so it is kept as instructed, but it is a public handle method with no consumer.
   `Action::CancelPairing`, `call::cancel_pairing` (`call.rs:150`) and the `bluez.rs:66` proxy method stay.
3. **`DismissConfirmation`** is keep: panel uses it at `bluetooth/indicator.rs:180`, `226`.
4. **`Bluetooth::bonded` (`mod.rs:906-926`) is deleted with `prompt`**, but it also trusted-and-connected a
   device whose pairing finished while a display-only prompt was showing. The `Settled` path
   (`mod.rs:635-637`) still trusts every pairing this service started; a pairing started by the *remote*
   device (an incoming `RequestAuthorization`) is no longer trusted by this service. Whether glimpse-agents
   or this service owns that trust step is a design call (handbook "second exception" at `docs/handbook.md:139`
   says a settled pairing trusts and connects).
5. **glimpse-dbus (outside the three crates, not touched):** `bluez::AgentManager1`
   (`crates/glimpse-dbus/src/clients/bluez.rs:105-`), `network_manager::AgentManager`
   (`network_manager.rs:631-`), `AGENT_CAPABILITY_VPN_HINTS` (36) and `SECRET_FLAG_*` (38-40) had the agents
   as their only consumers. They are client definitions; I assume glimpse-agents consumes them from there.
   `SECRET_FLAG_USER_REQUESTED` (40) may be unused already.
6. **`#[allow(clippy::large_enum_variant, clippy::enum_variant_names)]` at `app.rs:65`** may become
   unnecessary once `BluetoothPrompt`/`NetworkSecret` go; leave it unless clippy says otherwise.
7. **`render::name_of`** (`applets/network/render.rs`) was used by `asks_first` at `indicator.rs:86`; confirm
   it still has another caller when the function goes.
8. **`widgets/src/tests.rs:3`** (`PIN_MAX` import): I did not read the rest of that file for another use.
