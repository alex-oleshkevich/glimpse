# glimpse-agents acceptance

Acceptance criteria for the new `glimpse-agents` binary — one process owning the polkit authentication
agent, the NetworkManager secret agent and the BlueZ pairing agent, with one FIFO queue and one
layer-shell dialog — and for removing those agents from `glimpse-services` and `glimpse-panel`.

Every live check runs in a **nested** compositor, never the user's session: `timeout 300 niri -c
"$SCRATCH/niri.kdl" &`, then `export NIRI_SOCKET=$(ls -t "$XDG_RUNTIME_DIR"/niri.*.sock | head -1)`,
with every glimpse binary started as `XDG_CONFIG_HOME="$SCRATCH/home/.config" glimpse-agents
--config-dir "$SCRATCH/cfg"` and its log redirected outside `$SCRATCH/cfg`. The session lock is never
engaged for a test: `LockedHint` is fed from a fake logind proxy in a headless test. Session-dialog
restyling is `glimpse-0l06n` and is out of scope here.

## Polkit

```
AC-1  As a desktop user, I want a polkit request to arrive as one dialog describing the action, so that I know what I am authorizing.
      GIVEN BeginAuthentication for org.freedesktop.fwupd.update-internal WHEN the request reaches the queue THEN the dialog shows the request message as its description, a Password row, a collapsed Details row holding the action id and polkit's details map, and an Authorize button that turns sensitive once the password is non-empty.
      Verify by: headless test polkit/tests.rs::a_request_fills_the_dialog_from_the_action

AC-2  As a desktop user, I want a correct password to complete the request and clear the dialog, so that the app proceeds without me touching the window again.
      GIVEN the helper answered SUCCESS for the shown cookie WHEN the attempt completes THEN BeginAuthentication returns success (the helper has already reported to polkitd), the dialog closes, the helper connection is dropped, and the next queued request becomes the dialog.
      Verify by: headless test polkit/tests.rs::a_successful_attempt_answers_polkit_and_closes

AC-3  As a desktop user, I want unlimited retries on a wrong password, so that a typo never locks me out of my own machine.
      GIVEN the helper answered FAILURE WHEN the dialog returns THEN the Password row and a "Wrong password" caption carry error styling, the entry is cleared and focused, and the next Authorize opens a fresh helper connection — no counter, no lockout, no automatic close.
      Verify by: headless test polkit/tests.rs::a_wrong_password_retries_without_a_limit (five consecutive FAILUREs)

AC-4  As a desktop user, I want Cancel and Esc to abandon the request, so that a prompt nobody wants goes away and the caller learns it was refused.
      GIVEN a dialog is showing WHEN Cancel or Esc is pressed THEN BeginAuthentication returns org.freedesktop.PolicyKit1.Error.Cancelled, the helper connection is closed without a response, and the dialog closes.
      Verify by: headless test polkit/tests.rs::cancel_and_escape_refuse_the_request

AC-5  As a desktop user, I want a cancelled source request to vanish, so that I never answer a prompt whose owner has already given up.
      GIVEN CancelAuthentication for a showing cookie, or for a cookie still waiting in the queue WHEN it arrives THEN its BeginAuthentication returns Cancelled exactly once, a showing dialog closes, and a waiting request is dropped without ever being displayed, the dialog advancing straight to the next request.
      Verify by: headless test polkit/tests.rs::a_polkit_cancel_drops_the_request_it_names

AC-6  As an administrator sharing this computer, I want to choose which admin authenticates when the policy names more than one, so that the other admin can unlock a request I cannot answer.
      GIVEN polkit expanded the identity list to two unix-user entries WHEN the dialog is built THEN an Account combo row lists both by name and choosing the second one makes the helper handshake name that user, not the invoking user.
      Verify by: headless test polkit/tests.rs::a_second_admin_is_offered_and_authenticates_as_that_user

AC-7  As the only administrator, I want no account choice and no name of my own on screen, so that the dialog says only what the request needs.
      GIVEN the identity list holds exactly one unix-user WHEN the dialog is built THEN there is no Account row, and neither the dialog nor its details names the current user.
      Verify by: headless test polkit/tests.rs::a_single_admin_gets_no_account_row

AC-8  As a desktop user with a fingerprint reader, I want the PAM conversation's own text on the dialog, so that I know to touch the reader and can still fall back to the password.
      GIVEN the helper emits PAM_TEXT_INFO or PAM_ERROR_MSG ("Place your finger on the fingerprint reader") WHEN it is received THEN it replaces the caption under the rows as capped plain text while the Password row stays usable, and a following PAM_PROMPT_ECHO_OFF is answered over the same connection.
      Verify by: headless test polkit/tests.rs::pam_info_text_is_shown_beside_a_usable_password_row

AC-9  As a desktop user, I want the socket-activated helper used when it exists, so that no setuid binary runs on this machine.
      GIVEN /run/polkit/agent-helper.socket exists WHEN an attempt starts THEN the agent connects to it and writes the user name line, then the cookie line, then reads escaped PAM lines and writes the plain answer — while an unparsable line or a dead stream fails that attempt only: the dialog stays with an error caption and the next Authorize opens a new connection.
      Verify by: headless test polkit/tests.rs::socket_mode_writes_the_user_then_the_cookie and polkit/tests.rs::an_unreadable_helper_line_fails_one_attempt_only

AC-10 As a desktop user on a machine without the socket, I want the setuid helper to work, so that polkit prompts still work everywhere.
      GIVEN /run/polkit/agent-helper.socket is absent WHEN an attempt starts THEN the first existing of /usr/lib/polkit-1/polkit-agent-helper-1 and /usr/libexec/polkit-agent-helper-1 is spawned with the user name as its argument and the cookie on its stdin, and the installed unit sets no NoNewPrivileges= or user-namespace restriction that would strip the setuid bit.
      Verify by: headless test polkit/tests.rs::the_helper_without_a_socket_is_spawned_with_the_user ; inspection timeout 10 grep -c '^NoNewPrivileges' data/systemd/glimpse-agents.service (0)

AC-11 As a desktop user who already runs another polkit agent, I want glimpse to keep working for Wi-Fi and Bluetooth and tell me once, so that installing it never costs me my working setup.
      GIVEN RegisterAuthenticationAgent fails with org.freedesktop.PolicyKit1.Error.Failed because an agent already exists WHEN the failure is handled THEN the process stays up, NM and BlueZ stay registered, exactly one notification says polkit is handled elsewhere, no dialog was opened, and `timeout 20 glimpse-agents check` prints that duplicate on its polkit line and exits non-zero.
      Verify by: headless test polkit/tests.rs::a_duplicate_agent_leaves_the_other_two_running ; live check timeout 20 glimpse-agents check in the nested session
```

## Wi-Fi/VPN

```
AC-12 As a desktop user, I want the panel's Wi-Fi click to end in this one password dialog, so that the network gets a password and I am not asked twice.
      GIVEN the panel activates a secured network with no stored secret WHEN NetworkManager sends GetSecrets for 802-11-wireless-security with ALLOW_INTERACTION THEN the dialog titles the network from the request, offers Password and a Connect automatically switch that is on by default, keeps Join insensitive while the entry is empty, and joining answers under the key that key management expects.
      Verify by: headless test network/tests.rs::a_wireless_secret_is_answered_under_its_key_management ; live check timeout 40 nmcli device wifi connect '<ssid>' in the nested session, then timeout 20 nmcli -g GENERAL.STATE connection show '<ssid>' (activated)

AC-13 As a desktop user, I want to turn Connect automatically off in the dialog, so that a network I join once does not follow me onto it again.
      GIVEN the switch is turned off WHEN Join is pressed THEN the profile ends with connection.autoconnect false and still joins with the saved password when activated by hand. With the switch left on, nothing beyond the secret is written.
      Verify by: headless test network/tests.rs::the_switch_becomes_the_profile_autoconnect ; live check timeout 20 nmcli -g connection.autoconnect connection show '<ssid>' (no, then yes)

AC-14 As a desktop user, I want a rejected Wi-Fi password retried without limit, so that a mistyped passphrase costs one more try and nothing else.
      GIVEN NetworkManager re-asks with REQUEST_NEW WHEN the dialog returns THEN the Password row and the caption ("That password didn't work") carry error styling, the button reads Try again, the entry is cleared, and any number of these rounds is answered.
      Verify by: headless test network/tests.rs::a_request_new_round_keeps_the_dialog_and_hints_at_the_error

AC-15 As a desktop user, I want Cancel to stop NetworkManager asking, so that no other agent and no second prompt takes over.
      GIVEN a GetSecrets dialog is showing WHEN Cancel or Esc is pressed THEN the reply is org.freedesktop.NetworkManager.SecretAgent.Error.UserCanceled, no secret is sent, and the dialog closes.
      Verify by: headless test network/tests.rs::a_cancelled_request_answers_user_canceled

AC-16 As a desktop user, I want a request NetworkManager withdrew to leave my screen, so that I am not typing into a dialog whose answer nobody will read.
      GIVEN CancelGetSecrets arrives for the shown request (NetworkManager also sends it when its own 120 s timeout runs out) WHEN it is handled THEN the dialog closes, the parked GetSecrets returns UserCanceled once, and a submit arriving afterwards sends nothing.
      Verify by: headless test network/tests.rs::a_withdrawn_request_closes_its_dialog

AC-17 As a desktop user joining a VPN, I want the user name asked alongside the password, so that a profile that needs both is answered in one dialog.
      GIVEN GetSecrets for the vpn setting WHEN the dialog is built THEN it offers a Username row filled from the request's hints and a Password row, its title names the profile, and the answer is placed under the hint's key inside vpn.secrets; the agent registers with the VPN-hints capability so those hints arrive.
      Verify by: headless test network/tests.rs::a_vpn_request_offers_a_user_name_and_answers_under_the_hint ; live check timeout 40 nmcli connection up id '<vpn>' in the nested session

AC-18 As a desktop user, I want non-interactive and unserviceable secret requests refused at once, so that a background reconnect never raises a dialog and never waits.
      GIVEN flags without ALLOW_INTERACTION and REQUEST_NEW, or a setting other than 802-11-wireless-security and vpn WHEN GetSecrets arrives THEN an empty secrets map is returned immediately with no dialog, no queue entry, and no prompt left behind.
      Verify by: headless test network/tests.rs::a_request_nobody_can_answer_is_refused_without_a_dialog
```

## Bluetooth

```
AC-19 As a desktop user pairing a phone, I want the numeric comparison code shown large and tabular, so that I can compare six digits at a glance and confirm.
      GIVEN RequestConfirmation WHEN the dialog is built THEN it names the device, shows the code split 3+3 in the large tabular style of agents_bt_1, and offers Cancel and Pair — Pair returning success and Cancel returning org.bluez.Error.Rejected.
      Verify by: headless test bluetooth/tests.rs::a_numeric_comparison_shows_the_code_and_confirms

AC-20 As a desktop user pairing a keyboard, I want a code to type on the keyboard with no button to press, so that pairing completes when the device does.
      GIVEN DisplayPinCode or DisplayPasskey WHEN it arrives THEN BlueZ is answered at once, and the dialog shows the code with the "type this code on the device" description and only Cancel; it closes on Agent1.Cancel, on the device's Paired turning true, on Cancel (which calls Device1.CancelPairing), or 60 s after it opened.
      Verify by: headless test bluetooth/tests.rs::a_keyboard_code_is_displayed_with_only_cancel

AC-21 As a desktop user pairing an old headset, I want to type its PIN, so that devices with a fixed PIN can still pair.
      GIVEN RequestPinCode WHEN the digits are submitted THEN the typed PIN is returned as a string and the dialog closes; Cancelling returns org.bluez.Error.Rejected.
      Verify by: headless test bluetooth/tests.rs::a_pin_request_returns_the_typed_digits

AC-22 As a desktop user pairing a device that asks for a passkey, I want a numeric entry, so that the passkey reaches the device unchanged.
      GIVEN RequestPasskey WHEN the entry is submitted THEN a u32 is returned, non-digits keep the button insensitive, and Cancel returns org.bluez.Error.Rejected.
      Verify by: headless test bluetooth/tests.rs::a_passkey_request_returns_a_number

AC-23 As a desktop user, I want authorized services granted for bonded devices without a prompt, so that a paired device reconnecting is not a dialog each time — and refused for anything unbonded.
      GIVEN RequestAuthorization THEN one Allow/Deny prompt answers it; GIVEN AuthorizeService for a bonded device THEN it is allowed with no dialog, and for an unbonded device it is refused with org.bluez.Error.Rejected.
      Verify by: headless test bluetooth/tests.rs::service_authorization_is_automatic_only_for_bonded_devices

AC-24 As a desktop user, I want a pairing the device abandoned to leave my screen, so that a stale code is never confirmed.
      GIVEN Agent1.Cancel (BlueZ also sends it when its own 60 s timeout runs out) or Release WHEN it arrives THEN every BlueZ request leaves the queue, a parked method returns org.bluez.Error.Canceled once, show-views close, and a late button press sends nothing.
      Verify by: headless test bluetooth/tests.rs::a_cancelled_pairing_leaves_the_screen

AC-25 As a desktop user, I want the agent registered as the default one, so that an incoming pairing reaches this dialog and not nothing.
      GIVEN the agent starts WHEN bluetoothd is reachable THEN RegisterAgent with KeyboardDisplay then RequestDefaultAgent are called on /org/bluez, and when org.bluez gains a new owner both calls are made again.
      Verify by: headless test bluetooth/tests.rs::the_agent_registers_as_default_and_again_after_bluetoothd_restarts
```

## Queue & lock

```
AC-26 As a desktop user, I want one dialog at a time in arrival order, so that three apps asking at once do not stack windows or race for my keyboard.
      GIVEN a polkit, a NetworkManager and a BlueZ request arrive together WHEN they are queued THEN exactly one dialog exists, its request is the earliest, and answering or refusing it reveals the next one in the same order — nothing is answered concurrently.
      Verify by: headless test queue/tests.rs::requests_are_answered_one_at_a_time_in_arrival_order

AC-27 As a desktop user, I want a source's own cancel handled without a flicker, so that a queue under load never flashes prompts whose owner already gave up.
      GIVEN a source cancels the request at the head of the queue (CancelAuthentication, CancelGetSecrets, Agent1.Cancel) WHEN it is handled THEN that request is dropped and the next one is shown with no intermediate window, and a cancel for a request still waiting removes it without it ever being displayed.
      Verify by: headless test queue/tests.rs::a_source_cancel_drops_its_request_and_advances_the_queue

AC-28 As a desktop user who locks the screen, I want a polkit prompt to wait and come back clean, so that secrets are never typed over a lock screen and nothing I typed before locking is reused.
      GIVEN logind LockedHint is true WHEN a polkit request arrives THEN it is held with no surface, a dialog showing at that moment closes unanswered, and unlocking opens one fresh dialog with empty entries and no carried-over text.
      Verify by: headless test lock/tests.rs::a_locked_session_holds_polkit_and_reopens_it_fresh (LockedHint fed from a fake logind proxy)

AC-29 As a desktop user who locks the screen, I want Wi-Fi and Bluetooth requests refused at once, so that a background reconnect cannot park a prompt behind the lock or steal focus.
      GIVEN logind LockedHint is true WHEN GetSecrets or a BlueZ agent request arrives THEN it is answered with UserCanceled or org.bluez.Error.Rejected immediately, no dialog is created, and the queue stays empty; both work again after unlock.
      Verify by: headless test lock/tests.rs::a_locked_session_refuses_wireless_and_bluetooth_at_once
```

## Process & desktop integration

```
AC-30 As a desktop user, I want the prompt as one overlay over the focused output with the keyboard, so that it is where I am looking and Esc works without clicking.
      GIVEN a request is shown WHEN it maps THEN niri lists one glimpse-agents layer surface on the focused output with keyboard interactivity exclusive, no scrim and no second window, the dialog appears with no preceding notification, and Esc closes it.
      Verify by: live check timeout 30 pkcheck --action-id org.freedesktop.policykit.exec --process $$ --allow-user-interaction with glimpse-agents running in the nested session, then timeout 10 niri msg layers (test tooling, not glimpse code)

AC-31 As a desktop user, I want exactly one agent process, so that a second launch can never split the queue or fight over registrations.
      GIVEN the binary is already running WHEN it is started again THEN the second instance fails to take me.aresa.Glimpse.Agents, exits 1 before registering anything and opens no window; the running one keeps all three registrations, and on SIGTERM it exits 0 and its registrations vanish with its bus connection.
      Verify by: live check timeout 10 glimpse-agents started a second time in the nested session (exit 1, no window)

AC-32 As a desktop user, I want the agent started by my session, so that prompts work after login without me launching anything.
      GIVEN a graphical login WHEN the user session is up THEN data/systemd/glimpse-agents.service is a user unit wanted by the session target that starts after the compositor, `timeout 120 just check-units` passes, and it is in the justfile binaries list and the glimpse-package assets.
      Verify by: inspection of data/systemd/glimpse-agents.service, data/systemd/glimpse-session.target, justfile and crates/glimpse-package/Cargo.toml ; timeout 120 just check-units

AC-33 As a desktop user, I want a crashed agent to come back registered, so that a bug in one prompt does not leave me without any password UI.
      GIVEN the process was killed with SIGKILL and systemd restarted it WHEN it starts again THEN polkit, NetworkManager and BlueZ are all registered again and a fresh pkcheck in the nested session is answered by a dialog.
      Verify by: live check timeout 10 kill -9 $(pgrep -x glimpse-agents), then timeout 30 pkcheck --action-id org.freedesktop.policykit.exec --process $$ --allow-user-interaction in the nested session

AC-34 As a desktop user, I want the agent to re-register when NetworkManager or bluetoothd restarts, so that a daemon upgrade does not silently break prompts.
      GIVEN NetworkManager or bluetoothd is restarted and its bus name comes back WHEN the new owner appears THEN the corresponding agent re-registers (RegisterWithCapabilities / RegisterAgent and RequestDefaultAgent) without restarting glimpse-agents, and the next request prompts.
      Verify by: live check — the user restarts NetworkManager and bluetooth by hand, then a Wi-Fi join from the panel and a pairing each open the dialog

AC-35 As a desktop user restarting my panel, I want prompts to survive it, so that a panel crash never strands a password dialog or a pending request.
      GIVEN a dialog is showing and the queue holds one request WHEN glimpse-panel is stopped and started again THEN the dialog stays, its answer still reaches polkit/NM/BlueZ, and the queue drains in order.
      Verify by: live check timeout 10 kill <panel pid> and a fresh GLIMPSE_PANEL_APP_ID restart while a pkcheck dialog is showing in the nested session

AC-36 As a developer running a second panel, I want it to register no agents at all, so that a test panel can never take over the user's prompts.
      GIVEN a panel starts with a fresh GLIMPSE_PANEL_APP_ID WHEN it is up THEN its log holds no RegisterAgent, RequestDefaultAgent or RegisterWithCapabilities line, and no service in glimpse-services or glimpse-panel names those interfaces anymore.
      Verify by: live check timeout 20 rg 'RegisterAgent|RegisterWithCapabilities' "$SCRATCH/panel.log" (no hits) ; inspection timeout 10 rg -n 'org.bluez.Agent1|NetworkManager.SecretAgent' crates/glimpse-services crates/glimpse-panel (no hits)
```

## Panel & services clean-up

AC-38 is struck: no agent config key exists to remove (decision 13).

```
AC-37 As a maintainer, I want the agent code to live in one place, so that there is one owner of every prompt and one place to fix it.
      GIVEN the new binary exists WHEN the tree is inspected THEN glimpse-services' network and bluetooth services expose no SecretAgent or Agent1 object and hold no secret/pairing prompt state, while still reporting security, pairing, busy and failure state decoded from the bus.
      Verify by: inspection timeout 10 rg -n 'SecretAgent|Agent1|RequestDefaultAgent' crates/glimpse-services (no hits)


AC-39 As a desktop user, I want the panel to stop owning password and pairing dialogs, so that one click means one prompt from the one binary that can answer it.
      GIVEN a secured network row or a pair action in the panel WHEN it is activated THEN the panel sends the join or pair command without a secret and without opening a prompt of its own, and PairingDialog, SecretDialog, show_pairing, show_secret and the inline password row no longer exist.
      Verify by: inspection timeout 10 rg -n 'PairingDialog|show_pairing|show_secret|SecretDialog' crates/glimpse-panel crates/glimpse-widgets (no hits) ; live check timeout 40 nmcli device wifi connect '<ssid>' with only the panel and glimpse-agents running in the nested session

AC-40 As a desktop user, I want a failed pair to read as a notification, so that a row does not grow a banner about an agent the panel no longer runs.
      GIVEN pairing fails WHEN the failure reaches the panel THEN it is surfaced as a notification naming the device, no "could not register a pairing agent" state exists, and UI updates optimistically while the row shows busy then the outcome from the bus.
      Verify by: inspection timeout 10 rg -n 'NoAgent' crates/glimpse-services crates/glimpse-panel (no hits) ; live check a pairing refused on the device in the nested session
```

## Text safety

```
AC-41 As a desktop user, I want text from other applications to stay inside the dialog, so that a hostile SSID, device alias, action id or PAM message cannot resize, mark up or misdirect the prompt.
      GIVEN a 4 KiB name carrying control characters, "<b>", bidi overrides and no spaces WHEN the dialog is built THEN each string is capped and ellipsized to one plain-text line, nothing is parsed as markup, the surface stays inside its clamp, and the buttons remain reachable.
      Verify by: headless test text/tests.rs::hostile_names_stay_one_plain_line and text/tests.rs::a_pam_message_is_never_markup

AC-42 As a desktop user, I want passwords, PINs, passkeys and cookies never written down, so that a prompt cannot leak a secret into the journal.
      GIVEN any attempt, successful or not WHEN it completes THEN the password, PIN, passkey and cookie appear in no log line and no Debug output (secrets render as redacted), and the collected secret is zeroized once the answer is sent.
      Verify by: headless test secret/tests.rs::a_secret_never_reaches_the_log_or_a_debug_line
```

## Previews

```
AC-43 As a designer, I want every approved dialog preview to render from the same host, so that the shipped dialog and the approved look cannot drift apart.
      GIVEN the per-crate example calls glimpse_widgets::preview::run WHEN timeout 120 just check-examples runs THEN crates/glimpse-agents/blueprints/agent_dialog.blp compiles, and AgentDialog filled with each View of plan §5 under `just preview` with --scheme light and --scheme dark matches the approved docs/plans/glimpse-agents/agents_{polkit,wifi,bt}_*.blp row for row.
      Verify by: timeout 120 just check-examples ; live check each state, both schemes, side by side with docs/plans/glimpse-agents/ (user approval)

AC-44 As a designer, I want two previews open at once, so that comparing two variants never hands off to the window I already had.
      GIVEN one preview window is open WHEN a second preview is started on another dialog THEN both windows exist on their own, neither process exits 0 without a window, and editing one file reloads only its own window.
      Verify by: live check two concurrent timeout 60 just preview var/widget_examples/agents_polkit_1.blp and agents_bt_1.blp in the nested session
```
