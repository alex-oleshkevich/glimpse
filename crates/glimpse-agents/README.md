# glimpse-agents

The one process that answers system prompts for the session: the polkit authentication agent, the
NetworkManager secret agent and the BlueZ pairing agent, all drawing into one dialog.

**The session interface is exported before the bus name is owned.** `me.aresa.Glimpse.Agents` is
requested with `DoNotQueue` before agents register or windows exist. The system-bus agents start
before GTK; requests wait on the app channel during startup. A second instance cannot take the name.
`me.aresa.Glimpse.Session.RequestAction` accepts only `reboot` and `power-off` and acknowledges
admission within 5 s. One independent confirmation slot rejects requests while authentication or
another power action is active. Authentication, lock uncertainty, locking, Cancel and focus loss
cancel that slot; it never enters the authentication FIFO or reopens automatically. Initial focus
must arrive within 5 s. Confirmation destroys its exclusive overlay before interactive logind
execution, which rechecks lock state and capability and has a 120 s authorization deadline.
Failures after admission are notifications; logind owns inhibitor enforcement.

**One process owns every prompt, and one dialog shows at a time.** Agents never touch GTK: each
turns its D-Bus call into an `Ask` sent to the app and awaits a oneshot reply. The app keeps one
FIFO across sources and shows only its head. A source's own cancel removes its request; a waiting
request leaves without ever being drawn, the head is replaced by the next one in the same window.
A `Withdraw` with no key removes every request of that source.

**Every waiter is answered exactly once.** An `Ask` leaves the queue either with the user's answer
sent on its reply, or dropped. A dropped reply closes its channel, and the waiting interface method
turns the closed channel into its cancel error. A response whose source and key are not the shown
request is dropped, and a oneshot sender is consumed by its send, so a second response for a request
that has already left the queue sends nothing. A show-only request (`Reply::Shown`) has already
returned; a repeat with the same source and key replaces it in place, and it closes 60 s after it
opened. The paired watch for that view is armed when the view opens and aborted when the view ends,
and the watch itself stops after 60 s without withdrawing. SIGTERM and SIGINT quit the application
so the process exits 0 and dropping the queue answers every request still waiting.

**The BlueZ agent is `KeyboardDisplay` at `/me/aresa/glimpse/bluetooth/agent`.** `RegisterAgent` is
followed by `RequestDefaultAgent`; a refusal of the default request leaves the registration in
place. Both calls run again when `org.bluez` gains an owner, which is also how a `Release` comes
back. `AuthorizeService` reads `Device1.Bonded` with a 2 s timeout and never queues: bonded is
allowed, and a failed or false read is `org.bluez.Error.Rejected`. A device name is `Alias` read
with the same timeout, and a failed read is an empty name. `DisplayPinCode` and `DisplayPasskey`
return as soon as the show-view is queued; that view is withdrawn when `Device1.Paired` becomes
true, when the user cancels, or 60 s after it opened. `Cancel` and `Release` withdraw every
Bluetooth request. A dropped reply is
`org.bluez.Error.Canceled`; a denial is `org.bluez.Error.Rejected`.

**A locked session never shows a prompt.** The app follows logind's `LockedHint`. While locked there
is no surface: polkit requests wait and reopen fresh after unlock, with no conversation and no typed
text carried over; NetworkManager requests are refused and BlueZ requests denied at once, queued or
new, and show-only requests are dropped. Session confirmations require a known unlocked state;
failed reads and lost owners cancel them. Authentication retains its last known lock policy.
Every lock-observation call has a 5 s deadline; failures retry from 1 s up to 30 s and resolve the
session again. `LockedHint` changes use `glimpse_dbus::events`, and missed deliveries reread it.

**The surface is one layer-shell overlay**, keyboard exclusive, anchored to all four edges with an
exclusive zone of -1 and no monitor, so it covers the focused output, including the panel, and takes
the keyboard. It is created when there is something to show and destroyed after its dismissal animation,
or immediately when animations are off. A new request replaces a closing surface. The
dialog card stays centered at its natural size: the status page's internal scrolled window propagates
that height and never scrolls vertically, so its minimum is the content's and a note or an open
Details row grows the card with the button row inside it. A new head gets a new `AgentDialog`, so
no entry text crosses from one request to the next.

**`AgentDialog` renders a `View` and emits a `response` signal.** It is one Blueprint template of
stock libadwaita rows; Rust fills it and toggles visibility. `show` compares before every write, and
user-editable rows are written only when the view's value for them changes, never on a re-render.
The secret entry clears when its prompt changes and when `busy` falls. A view with a secret entry
gives that entry the keyboard focus, including after a failure clears it and when a new head is
shown on an already mapped surface. A view with no entry gives focus to the confirm button, so
Enter confirms. Confirm also waits a fixed 1000 ms after a new ask or focus regain, and capture drops keys for the first 400 ms of a new ask except Escape; confirm is insensitive while busy, while the helper never started, or while the secret fails its field rule. Enter confirms, Escape cancels, and each of those emits again on the
same view. The app stamps the shown request's source and key into that response
and drops it unless that request is still the one shown. The account row emits the selected account
when the user changes it. Filling the row does not.

**Text from other programs is hostile.** Titles, bodies, account names, details, PAM prompts and
device names pass `glimpse_utils::clean` with a cap and are set with plain `set_text`, never markup.
The first polkit detail is Command, read from `/proc/<subject-pid>/cmdline` (falling back to the caller
PID when the subject PID is absent) because polkitd forwards no command line.

**Secrets never reach a log.** Passwords, PINs, passkeys and polkit cookies are redacted from every
`Debug` output, input messages print no request key, and a collected secret is a `Zeroizing` string.

**The polkit helper is the socket when that socket exists.** An attempt writes the user name, then
the cookie, and reads C-unescaped lines capped at 4096 bytes. When the socket is absent, the attempt
spawns the first helper that has the setuid bit, with the user as its argument and the cookie on
stdin. A helper without that bit is not spawned. When the attempt cannot start, or the connection
ends before any line, the note is "Can't verify passwords", Authorize stays insensitive, and the
password row is not marked as a wrong password. The request stays until Cancel.
Dropping an attempt closes the stream and kills that child. Choosing another account drops the
running attempt and starts one for that user with no secret. Authorize submits the secret only when
that attempt's account is still the selected one. A password confirmed before the helper prompts
waits for that prompt on the same attempt; only a failure, a dead stream or another account starts
a new one. A cancel that arrives after the ask is queued and before the attempt starts does not open
a helper, and the authentication reply stays open until the withdraw drops the ask.

**A duplicate polkit registration is logged once and notified once, and the other agents keep
running.** `check` reports running when this process owns its bus name. Otherwise it registers and
unregisters a cancelling stub, so the polkit line says ok or another agent.

**A network secret is returned only after an autoconnect change is written.** `GetSecrets` writes
`connection.autoconnect` before it returns the secret. A failure or a 5 s timeout is a warning, and
the secret is still returned.

**The unit carries no hardening.** No `NoNewPrivileges=`, no user namespace and no `Protect*` or
`Restrict*` keys: on systems without the polkit helper socket the agent spawns the setuid
`polkit-agent-helper-1`, and any of those keys strips its setuid bit.
