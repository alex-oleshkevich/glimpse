# SDK Golden Fixtures

Shared JSON fixtures used by all four SDK test suites to enforce
cross-language serialization parity.

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

## Adding a fixture

1. Drop the new `.json` file under `widgets/` or `events/`.
2. Add the same case name + corresponding builder to **all four** SDK test
   files (`sdk-rs/tests/golden.rs`, `sdk-ts/tests/golden.test.ts`,
   `sdk-py/tests/test_golden.py`, `sdk-go/sdk/golden_test.go`).
3. Run each SDK's test suite. If any SDK diverges, fix the SDK — never
   change the fixture to match an SDK.

## Canonical-shape rules

The fixture set encodes these rules. If a new fixture violates them,
re-think before adding.

| Field category | Rule |
|---|---|
| Common props (id, visible, hexpand, vexpand, halign, valign, tooltip, variant) | omit when unset |
| Optional icons / children / right-side accessories | omit when unset |
| Structural arrays (`children`, `body`, `items`, `rows`, `menu`) | always emit (`[]` when empty) |
| Structural strings on display widgets (`label` on Item/Meter; `subtitle`/`meta` on ActionRow; `message` on Toast; `subtitle` on EmptyState) | always emit (`""` when empty) |
| `Header.subtitle` | omit when empty (historical 4-of-4 agreement; documented exception) |
| Primary state booleans (`expanded`, `clickable`, `active`, `spinning`) | always emit |
| Modifier booleans (`wrap`, `selectable`, `show_text`, `interactive`, `draw_value`) | omit when false |
| `Box.orientation`, `Box.spacing`, `Row.spacing`, `Column.spacing` | always emit |
| Numerics with non-zero defaults (`Grid.row_spacing`, `Grid.column_spacing`) | always emit |
