---
name: gortex-glimpse-widgets-src-29-dirs
description: "Work in the glimpse-widgets/src +29 dirs area — 381 symbols across 53 files (82% cohesion)"
---

# glimpse-widgets/src +29 dirs

381 symbols | 53 files | 82% cohesion

## When to Use

Use this skill when working on files in:
- `crates/glimpse-compositors/src/model.rs`
- `crates/glimpse-compositors/tests/live.rs`
- `crates/glimpse-config/src/schema/applets.rs`
- `crates/glimpse-config/src/schema/mod.rs`
- `crates/glimpse-panel/src/applet/runtime.rs`
- `crates/glimpse-services/src/subscription.rs`
- `crates/glimpse-widgets/examples/preview.rs`
- `crates/glimpse-widgets/src/calendar/grid.rs`
- `crates/glimpse-widgets/src/calendar/mod.rs`
- `crates/glimpse-widgets/src/calendar_popover/mod.rs`
- `crates/glimpse-widgets/src/choice_list/mod.rs`
- `crates/glimpse-widgets/src/dots.rs`
- `crates/glimpse-widgets/src/drawer.rs`
- `crates/glimpse-widgets/src/event_list/mod.rs`
- `crates/glimpse-widgets/src/event_list/row.rs`
- `crates/glimpse-widgets/src/fact_list/mod.rs`
- `crates/glimpse-widgets/src/forecast/day.rs`
- `crates/glimpse-widgets/src/forecast/mod.rs`
- `crates/glimpse-widgets/src/hero/imp.rs`
- `crates/glimpse-widgets/src/hero/mod.rs`
- `crates/glimpse-widgets/src/indicator/imp.rs`
- `crates/glimpse-widgets/src/indicator/mod.rs`
- `crates/glimpse-widgets/src/indicator_group/mod.rs`
- `crates/glimpse-widgets/src/lib.rs`
- `crates/glimpse-widgets/src/notice/imp.rs`
- `crates/glimpse-widgets/src/notice/mod.rs`
- `crates/glimpse-widgets/src/now_playing/imp.rs`
- `crates/glimpse-widgets/src/now_playing/mod.rs`
- `crates/glimpse-widgets/src/pager/imp.rs`
- `crates/glimpse-widgets/src/pager/item.rs`
- `crates/glimpse-widgets/src/pager/mod.rs`
- `crates/glimpse-widgets/src/placeholder/imp.rs`
- `crates/glimpse-widgets/src/placeholder/mod.rs`
- `crates/glimpse-widgets/src/player_list/mod.rs`
- `crates/glimpse-widgets/src/player_list/row.rs`
- `crates/glimpse-widgets/src/popover_shell/imp.rs`
- `crates/glimpse-widgets/src/popover_shell/mod.rs`
- `crates/glimpse-widgets/src/range_bar.rs`
- `crates/glimpse-widgets/src/readout/imp.rs`
- `crates/glimpse-widgets/src/readout/mod.rs`
- `crates/glimpse-widgets/src/row/imp.rs`
- `crates/glimpse-widgets/src/row/mod.rs`
- `crates/glimpse-widgets/src/scrubber/imp.rs`
- `crates/glimpse-widgets/src/scrubber/mod.rs`
- `crates/glimpse-widgets/src/section/imp.rs`
- `crates/glimpse-widgets/src/section/mod.rs`
- `crates/glimpse-widgets/src/split_row/mod.rs`
- `crates/glimpse-widgets/src/theme.rs`
- `crates/glimpse-widgets/src/transport/imp.rs`
- `crates/glimpse-widgets/src/transport/mod.rs`
- `crates/glimpse-widgets/src/workspaces_popover/mod.rs`
- `crates/glimpse-widgets/src/world_clock/mod.rs`
- `crates/glimpse-widgets/src/world_clock/row.rs`

## Key Files

| File | Symbols |
|------|---------|
| `crates/glimpse-compositors/src/model.rs` | an_overlong_title_truncates_on_a_char_boundary, an_app_id_is_capped_shorter_than_a_title |
| `crates/glimpse-compositors/tests/live.rs` | switching_the_layout_is_reported_back_on_the_event_stream, a_live_snapshot_describes_the_session |
| `crates/glimpse-config/src/schema/applets.rs` | every_common_setting_is_taken_off_the_table |
| `crates/glimpse-config/src/schema/mod.rs` | a_clock_reads_its_world_clock_zones |
| `crates/glimpse-panel/src/applet/runtime.rs` | Builder |
| `crates/glimpse-services/src/subscription.rs` | len |
| `crates/glimpse-widgets/examples/preview.rs` | pager, root, alert_placeholder |
| `crates/glimpse-widgets/src/calendar/grid.rs` | a_month_is_always_six_weeks |
| `crates/glimpse-widgets/src/calendar/mod.rs` | connect_day_selected, new, default, F, handler, ... |
| `crates/glimpse-widgets/src/calendar_popover/mod.rs` | events, label, set_footer, title, set_day |
| `crates/glimpse-widgets/src/choice_list/mod.rs` | selected, set_choices, build_row, index, none_if_empty, ... |
| `crates/glimpse-widgets/src/dots.rs` | orientation, _for_size, measure |
| `crates/glimpse-widgets/src/drawer.rs` | drawer, open, set |
| `crates/glimpse-widgets/src/event_list/mod.rs` | connect_activated, text, render, set_max_rows, none_if_empty, ... |
| `crates/glimpse-widgets/src/event_list/row.rs` | when, ParentType, set_when |
| `crates/glimpse-widgets/src/fact_list/mod.rs` | set_facts, new, value, facts, label |
| `crates/glimpse-widgets/src/forecast/day.rs` | chance, ParentType, default, set_precipitation, new, ... |
| `crates/glimpse-widgets/src/forecast/mod.rs` | index, connect_activated, default, render, build_row, ... |
| `crates/glimpse-widgets/src/hero/imp.rs` | builder, child, set_title, set_icon_name, add_child, ... |
| `crates/glimpse-widgets/src/hero/mod.rs` | new, default, set_slot, next, icon, ... |
| `crates/glimpse-widgets/src/indicator/imp.rs` | obj, icon, label, class_init, gicon, ... |
| `crates/glimpse-widgets/src/indicator/mod.rs` | set_label, default, set_icon, spec, icon, ... |
| `crates/glimpse-widgets/src/indicator_group/mod.rs` | set_orientation, items, set_items, orientation |
| `crates/glimpse-widgets/src/lib.rs` | all_named, label_of, widgets, parent, T, ... |
| `crates/glimpse-widgets/src/notice/imp.rs` | Warning, severity, set_severity, Severity, Error, ... |
| `crates/glimpse-widgets/src/notice/mod.rs` | default, new |
| `crates/glimpse-widgets/src/now_playing/imp.rs` | source, set_source |
| `crates/glimpse-widgets/src/now_playing/mod.rs` | new, default |
| `crates/glimpse-widgets/src/pager/imp.rs` | constructed |
| `crates/glimpse-widgets/src/pager/item.rs` | label, new, object, shape, class_init, ... |
| `crates/glimpse-widgets/src/pager/mod.rs` | set_shape, f, F, new, render, ... |
| `crates/glimpse-widgets/src/placeholder/imp.rs` | set_icon_name, set_error, name, description, set_description, ... |
| `crates/glimpse-widgets/src/placeholder/mod.rs` | default, new |
| `crates/glimpse-widgets/src/player_list/mod.rs` | F, players, set_players, none_if_empty, render, ... |
| `crates/glimpse-widgets/src/player_list/row.rs` | ParentType |
| `crates/glimpse-widgets/src/popover_shell/imp.rs` | kind, builder, child, add_child |
| `crates/glimpse-widgets/src/popover_shell/mod.rs` | widget, clear_footer, hero, content, new, ... |
| `crates/glimpse-widgets/src/range_bar.rs` | high, measure, new, low, NAME, ... |
| `crates/glimpse-widgets/src/readout/imp.rs` | set_unit, unit |
| `crates/glimpse-widgets/src/readout/mod.rs` | new, default |
| `crates/glimpse-widgets/src/row/imp.rs` | set_subtitle, selectable, selectable, lead, instance_init, ... |
| `crates/glimpse-widgets/src/row/mod.rs` | set_lead, widget, slot, fill, new, ... |
| `crates/glimpse-widgets/src/scrubber/imp.rs` | seconds, set_position |
| `crates/glimpse-widgets/src/scrubber/mod.rs` | default, new |
| `crates/glimpse-widgets/src/section/imp.rs` | child, empty, set_empty, builder, add_child, ... |
| `crates/glimpse-widgets/src/section/mod.rs` | content, new, set_content |
| `crates/glimpse-widgets/src/split_row/mod.rs` | row |
| `crates/glimpse-widgets/src/theme.rs` | text, referenced |
| `crates/glimpse-widgets/src/transport/imp.rs` | repeat, repeat, set_repeat |
| `crates/glimpse-widgets/src/transport/mod.rs` | new, default |
| `crates/glimpse-widgets/src/workspaces_popover/mod.rs` | row_for, workspaces, id, set_workspaces, close_drawer, ... |
| `crates/glimpse-widgets/src/world_clock/mod.rs` | here, there, now, twelve_hour, zones, ... |
| `crates/glimpse-widgets/src/world_clock/row.rs` | default, ParentType, new |

## Entry Points

- `crates/glimpse-widgets/src/lib.rs::widgets`
- `crates/glimpse-compositors/tests/live.rs::switching_the_layout_is_reported_back_on_the_event_stream`
- `crates/glimpse-compositors/tests/live.rs::a_live_snapshot_describes_the_session`

## Connected Communities

- **src/calendar +8 dirs** (20 cross-edges)
- **src/applet +16 dirs** (16 cross-edges)
- **src/scrubber** (8 cross-edges)
- **glimpse-widgets · Transport** (8 cross-edges)
- **src/notice** (4 cross-edges)
- **glimpse-widgets/src · PlayerRow** (3 cross-edges)
- **src/placeholder** (3 cross-edges)
- **glimpse-widgets · pager_case** (3 cross-edges)
- **glimpse-compositors/src · sanitize** (3 cross-edges)
- **glimpse-widgets/src · row_for** (3 cross-edges)
- **src/now_playing** (3 cross-edges)
- **src/section · Section** (3 cross-edges)
- **src/applet +2 dirs** (3 cross-edges)
- **glimpse-widgets/src · EventRow** (2 cross-edges)
- **src/split_row · SplitRow** (2 cross-edges)
- **glimpse-widgets/src +1 dirs** (2 cross-edges)
- **glimpse-widgets · find** (2 cross-edges)
- **glimpse-widgets/src · Dots** (2 cross-edges)
- **src/readout** (2 cross-edges)
- **src/section · fill** (2 cross-edges)
- **glimpse-widgets/src +7 dirs** (2 cross-edges)
- **glimpse-widgets/src · set_tooltip** (2 cross-edges)
- **src/world_clock** (2 cross-edges)
- **src/forecast · ForecastDay** (2 cross-edges)
- **src/forecast · ForecastHour** (2 cross-edges)
- **src/indicator** (2 cross-edges)
- **src/schema +26 dirs** (2 cross-edges)
- **glimpse-lock/src +15 dirs** (2 cross-edges)
- **glimpse-widgets/src · set_css_class** (1 cross-edges)
- **glimpse-compositors/src +4 dirs · Event** (1 cross-edges)
- **glimpse-lock/src +1 dirs** (1 cross-edges)
- **src/player_list** (1 cross-edges)
- **src/pager · step** (1 cross-edges)
- **src/hero** (1 cross-edges)
- **glimpse-widgets/src · constructed** (1 cross-edges)
- **src/calendar_popover +2 dirs** (1 cross-edges)

## How to Explore

```
analyze(operation:"communities", id:"community-146")
explore(operation:"context", task:"understand glimpse-widgets/src +29 dirs", format:"gcx")
relations(operation:"usages", target:{symbol:"crates/glimpse-widgets/src/lib.rs::widgets"}, format:"gcx")
```

_`format: "gcx"` returns the [GCX1 compact wire format](../../docs/wire-format.md) — round-trippable, ~27% fewer tokens than JSON. Drop it for JSON output; agents using `@gortex/wire` or the Go `github.com/gortexhq/gcx-go` package decode either._
