use async_trait::async_trait;
use glimpse_sdk::{
    Applet, AppletResult, BoxedList, CircleBox, Column, EmptyState, Hero, MsgMapper, PopoverShell,
    PopoverSize, SegmentedTile, StatusItem, Tile, TreeNode, close_popover, copy_to_clipboard, run,
    tree,
};
use std::collections::HashSet;
use std::process::Stdio;
use tokio::time::Duration;

#[derive(Debug, Default, Clone)]
struct State {
    items: Vec<String>,
    expanded: HashSet<String>,
}

#[derive(Debug, Clone, PartialEq)]
enum Msg {
    Pick,
    CopyColor(String),
    ToggleColor(String, bool),
}

struct ColorPickerApplet;

#[async_trait]
impl Applet for ColorPickerApplet {
    type State = State;
    type Msg = Msg;

    async fn status(&self, _state: &State) -> AppletResult<Vec<StatusItem>> {
        Ok(vec![
            StatusItem::new("colorpicker").icon("color-select-symbolic"),
        ])
    }

    async fn update(&mut self, state: &mut State, msg: Msg) -> AppletResult<()> {
        match msg {
            Msg::Pick => {
                close_popover().await?;
                tokio::time::sleep(Duration::from_millis(300)).await;

                let output = tokio::process::Command::new("hyprpicker")
                    .arg("--render-inactive")
                    .stdin(Stdio::null())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::inherit())
                    .output()
                    .await?;

                if output.status.success() {
                    let color = String::from_utf8_lossy(&output.stdout).trim().to_string();
                    if !color.is_empty() {
                        copy_to_clipboard(&color).await.ok();
                        state.items.insert(0, color);
                    }
                }
            }
            Msg::CopyColor(color) => {
                copy_to_clipboard(&color).await.ok();
            }
            Msg::ToggleColor(color_id, expanded) => {
                if expanded {
                    state.expanded.insert(color_id);
                } else {
                    state.expanded.remove(&color_id);
                }
            }
        }
        Ok(())
    }

    async fn popover(&self, state: &State) -> AppletResult<Option<TreeNode<Msg>>> {
        Ok(Some(popover_tree(state, is_hyprpicker_installed())))
    }
}

fn popover_tree(state: &State, picker_installed: bool) -> TreeNode<Msg> {
    if !picker_installed {
        let mut empty = EmptyState::new("Not installed");
        empty.subtitle = Some("hyprpicker is not installed".into());
        return empty.into();
    }

    let recent: TreeNode<Msg> = if state.items.is_empty() {
        Column::new(vec![]).into()
    } else {
        BoxedList::new(
            state
                .items
                .iter()
                .map(|color| {
                    let id_suffix = color_id_suffix(color);
                    color_segmented_tile(color, state.expanded.contains(&id_suffix)).into()
                })
                .collect(),
        )
        .into()
    };

    let mut shell = PopoverShell::new(tree![
        {
            let mut hero = Hero::new("Color picker", "Pick a color");
            hero.icon = Some("color-select-symbolic".into());
            hero
        },
        Column::new(tree![pick_color_tile(), recent]),
    ]);
    shell.size = PopoverSize::Medium;
    shell.into()
}

fn pick_color_tile() -> Tile<Msg> {
    let mut tile = Tile::new("Pick color");
    tile.id = Some("pick-color".into());
    tile.on_click = Some(MsgMapper::new(|()| Msg::Pick));
    tile
}

fn color_segmented_tile(color: &str, expanded: bool) -> SegmentedTile<Msg> {
    let hex = color.to_owned();
    let id_suffix = color_id_suffix(&hex);
    let formats = color_formats(&hex);

    let children: Vec<TreeNode<Msg>> = formats
        .into_iter()
        .map(|(label, value)| color_format_tile(&id_suffix, label, value).into())
        .collect();

    let copy_value = hex.clone();
    let mut tile = SegmentedTile::new(hex.clone());
    tile.id = Some(format!("color-{id_suffix}"));
    tile.left = Some(Box::new(
        {
            let mut circle = CircleBox::new(hex.clone());
            circle.common.css_classes = vec!["colorpicker-swatch".into()];
            circle
        }
        .into(),
    ));
    tile.child = Some(Box::new(BoxedList::new(children).into()));
    tile.expanded = expanded;
    tile.on_click = Some(MsgMapper::new(move |()| Msg::CopyColor(copy_value.clone())));
    tile.on_toggle = Some(MsgMapper::new(move |expanded| {
        Msg::ToggleColor(id_suffix.clone(), expanded)
    }));
    tile
}

fn color_id_suffix(color: &str) -> String {
    color.trim_start_matches('#').to_ascii_lowercase()
}

fn color_format_tile(id_suffix: &str, label: &'static str, value: String) -> Tile<Msg> {
    let copy_value = value.clone();
    let mut tile = Tile::new(value);
    tile.id = Some(format!("copy-{label}-{id_suffix}"));
    tile.on_click = Some(MsgMapper::new(move |()| Msg::CopyColor(copy_value.clone())));
    tile
}

fn color_formats(hex: &str) -> Vec<(&'static str, String)> {
    let mut out = vec![("hex", hex.to_owned())];
    if let Some(rgb) = parse_hex(hex) {
        out.push(("rgb", rgb_string(rgb)));
        out.push(("hsl", hsl_string(rgb_to_hsl(rgb))));
        out.push(("oklch", oklch_string(rgb_to_oklch(rgb))));
    }
    out
}

fn parse_hex(value: &str) -> Option<(u8, u8, u8)> {
    let s = value.strip_prefix('#')?;
    let s = match s.len() {
        3 => s
            .chars()
            .flat_map(|c| std::iter::repeat(c).take(2))
            .collect::<String>(),
        6 => s.to_owned(),
        8 => s[..6].to_owned(),
        _ => return None,
    };
    let r = u8::from_str_radix(&s[0..2], 16).ok()?;
    let g = u8::from_str_radix(&s[2..4], 16).ok()?;
    let b = u8::from_str_radix(&s[4..6], 16).ok()?;
    Some((r, g, b))
}

fn rgb_string((r, g, b): (u8, u8, u8)) -> String {
    format!("rgb({r}, {g}, {b})")
}

fn rgb_to_hsl((r, g, b): (u8, u8, u8)) -> (f64, f64, f64) {
    let rf = r as f64 / 255.0;
    let gf = g as f64 / 255.0;
    let bf = b as f64 / 255.0;
    let max = rf.max(gf).max(bf);
    let min = rf.min(gf).min(bf);
    let l = (max + min) / 2.0;
    if (max - min).abs() < f64::EPSILON {
        return (0.0, 0.0, l);
    }
    let d = max - min;
    let s = if l > 0.5 {
        d / (2.0 - max - min)
    } else {
        d / (max + min)
    };
    let h = if max == rf {
        ((gf - bf) / d + if gf < bf { 6.0 } else { 0.0 }) * 60.0
    } else if max == gf {
        ((bf - rf) / d + 2.0) * 60.0
    } else {
        ((rf - gf) / d + 4.0) * 60.0
    };
    (h, s, l)
}

fn hsl_string((h, s, l): (f64, f64, f64)) -> String {
    format!(
        "hsl({:.0}, {:.0}%, {:.0}%)",
        h.round(),
        (s * 100.0).round(),
        (l * 100.0).round()
    )
}

fn rgb_to_oklch((r, g, b): (u8, u8, u8)) -> (f64, f64, f64) {
    let lr = srgb_to_linear(r as f64 / 255.0);
    let lg = srgb_to_linear(g as f64 / 255.0);
    let lb = srgb_to_linear(b as f64 / 255.0);

    let l = 0.412_165_612 * lr + 0.536_275_208 * lg + 0.051_445_995 * lb;
    let m = 0.211_859_107 * lr + 0.680_718_654 * lg + 0.107_406_579 * lb;
    let s = 0.088_309_516 * lr + 0.281_847_959 * lg + 0.629_958_510 * lb;

    let l_ = l.cbrt();
    let m_ = m.cbrt();
    let s_ = s.cbrt();

    let l_ok = 0.210_454_255_3 * l_ + 0.793_617_785_0 * m_ - 0.004_072_046_8 * s_;
    let a = 1.977_998_495_1 * l_ - 2.428_592_205_0 * m_ + 0.450_593_709_9 * s_;
    let bb = 0.025_904_037_1 * l_ + 0.782_771_766_2 * m_ - 0.808_675_766_0 * s_;

    let c = (a * a + bb * bb).sqrt();
    let mut h = bb.atan2(a).to_degrees();
    if h < 0.0 {
        h += 360.0;
    }
    (l_ok, c, h)
}

fn srgb_to_linear(value: f64) -> f64 {
    if value <= 0.040_45 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}

fn oklch_string((l, c, h): (f64, f64, f64)) -> String {
    format!("oklch({:.3} {:.3} {:.1})", l, c, h)
}

fn is_hyprpicker_installed() -> bool {
    std::env::var_os("PATH")
        .map(|path| std::env::split_paths(&path).any(|dir| dir.join("hyprpicker").exists()))
        .unwrap_or(false)
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> AppletResult<()> {
    run(ColorPickerApplet, State::default()).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn pick_tile_has_no_left_icon() {
        let state = State::default();
        let tree = serde_json::to_value(popover_tree(&state, true)).expect("serialize popover");

        assert_eq!(
            tree["data"]["children"][1]["data"]["children"][0],
            json!({
                "type": "tile",
                "data": {
                    "id": "pick-color",
                    "primary": "Pick color",
                }
            })
        );
    }

    #[test]
    fn recent_color_segmented_tile_has_circle_left_copy_action_and_format_children() {
        let state = State {
            items: vec!["#336699".into()],
            ..State::default()
        };
        let tree = serde_json::to_value(popover_tree(&state, true)).expect("serialize popover");
        let body_children = tree["data"]["children"][1]["data"]["children"]
            .as_array()
            .expect("content column must have children");
        assert_eq!(body_children.len(), 2);
        let entry = &body_children[1]["data"]["children"][0];

        assert_eq!(entry["type"], "segmented_tile");
        assert_eq!(entry["data"]["primary"], "#336699");
        assert!(entry["data"]["secondary"].is_null());
        assert_eq!(entry["data"]["left"]["type"], "circle_box");
        assert_eq!(entry["data"]["left"]["data"]["color"], "#336699");
        assert_eq!(
            entry["data"]["left"]["data"]["css_classes"],
            serde_json::json!(["colorpicker-swatch"])
        );

        let children = entry["data"]["child"]["data"]["children"]
            .as_array()
            .expect("segmented tile child must be a boxed list");
        let labels: Vec<&str> = children
            .iter()
            .map(|c| c["data"]["primary"].as_str().unwrap_or(""))
            .collect();
        assert_eq!(labels.len(), 4);
        assert_eq!(labels[0], "#336699");
        assert!(labels[1].starts_with("rgb("));
        assert!(labels[2].starts_with("hsl("));
        assert!(labels[3].starts_with("oklch("));
    }

    #[test]
    fn parse_hex_round_trips_known_points() {
        assert_eq!(parse_hex("#000000"), Some((0, 0, 0)));
        assert_eq!(parse_hex("#ffffff"), Some((255, 255, 255)));
        assert_eq!(parse_hex("#336699"), Some((51, 102, 153)));
        assert_eq!(parse_hex("#369"), Some((51, 102, 153)));
        assert_eq!(parse_hex("#336699ff"), Some((51, 102, 153)));
        assert_eq!(parse_hex("not-a-color"), None);
    }

    #[test]
    fn hsl_string_matches_known_points() {
        assert_eq!(hsl_string(rgb_to_hsl((0, 0, 0))), "hsl(0, 0%, 0%)");
        assert_eq!(hsl_string(rgb_to_hsl((255, 255, 255))), "hsl(0, 0%, 100%)");
        assert_eq!(hsl_string(rgb_to_hsl((255, 0, 0))), "hsl(0, 100%, 50%)");
    }

    #[tokio::test]
    async fn expanded_color_row_serializes_expanded_state() {
        let mut state = State {
            items: vec!["#1c1b29".into()],
            ..State::default()
        };
        ColorPickerApplet
            .update(&mut state, Msg::ToggleColor("1c1b29".into(), true))
            .await
            .expect("toggle color");

        let tree = serde_json::to_value(popover_tree(&state, true)).expect("serialize popover");
        let entry = &tree["data"]["children"][1]["data"]["children"][1]["data"]["children"][0];

        assert_eq!(entry["data"]["expanded"], true);
    }
}
