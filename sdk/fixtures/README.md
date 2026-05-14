# SDK Golden Fixtures

Shared JSON fixtures used by all four SDK test suites and the Rust renderer
test to enforce cross-language serialization parity and renderability.

Each `widgets/*.json` file contains the exact JSON that the SDKs must emit
for the corresponding widget case. Each `events/*.json` file contains an
incoming event payload that the SDKs must parse identically.

## How tests use them

Each SDK has a `golden_test` test file that:

1. Iterates a hard-coded list of `(fixture_name, builder)` pairs.
2. For widget fixtures: builds the widget, serializes to JSON, parses both
   the serialized output and the fixture file, compares as JSON values
   (key order is irrelevant, but every key and every value must match).
3. For event fixtures: parses the fixture as the incoming JSON and asserts
   the SDK's parser returns the documented typed event.

The Rust renderer also has `golden_widget_fixtures_render_without_errors`.
That test reads every `widgets/*.json` file, deserializes it through the
exec protocol model, and sends it to the renderer. A fixture is not valid
unless it can be serialized by every SDK and rendered by the shell.

## Adding a fixture

1. Add the case to `generate.py` and regenerate the JSON fixture.
2. Add the same case name + corresponding builder to **all four** SDK test
   files (`sdk-rs/tests/golden.rs`, `sdk-ts/tests/golden.test.ts`,
   `sdk-py/tests/test_golden.py`, `sdk-go/sdk/golden_test.go`).
3. Run each SDK's test suite.
4. Run the Rust renderer fixture test from the repo root:

   ```sh
   cargo test -p glimpse-shell golden_widget_fixtures_render_without_errors -- --nocapture
   ```

If any SDK diverges, fix the SDK unless the fixture violates the documented
protocol. If the renderer rejects a fixture, either fix the fixture shape or
extend the renderer and protocol together.

## Canonical-shape rules

The fixture set encodes these rules. If a new fixture violates them,
re-think before adding.

| Field category | Rule |
|---|---|
| Common props (id, visible, hexpand, vexpand, halign, valign, tooltip, variant) | omit when unset |
| Optional icons / children / right-side accessories | omit when unset |
| Structural arrays (`children`, `body`, `items`, `rows`, `menu`) | always emit (`[]` when empty) |
| Structural strings on display widgets (`label` on Item/ActionItem/Meter; `subtitle` on EmptyState) | always emit (`""` when empty) |
| Primary state booleans (`expanded`, `clickable`, `active`, `spinning`) | always emit |
| Modifier booleans (`wrap`, `selectable`, `show_text`, `interactive`, `draw_value`) | omit when false |
| `Box.orientation`, `Box.spacing`, `Row.spacing`, `Column.spacing` | always emit |
| Numerics with non-zero defaults (`Grid.row_spacing`, `Grid.column_spacing`) | always emit |
| Interactive widgets (`button`, `switch`, `toggle_button`, `checkbox`, `slider`, `select`, interactive `meter`, `action_item`) | include a stable `id` |
