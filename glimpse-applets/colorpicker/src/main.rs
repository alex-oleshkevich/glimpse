use async_trait::async_trait;
use glimpse_sdk::{
    Applet, AppletResult, BoxedList, Column, EmptyState, Hero, MsgMapper, PopoverShell,
    PopoverSize, StatusItem, Text, Tile, TreeNode, close_popover, copy_to_clipboard, run, tree,
};
use std::process::Stdio;
use tokio::time::Duration;

#[derive(Debug, Default, Clone)]
struct State {
    items: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
enum Msg {
    Pick,
    CopyColor(String),
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
                tokio::time::sleep(Duration::from_millis(50)).await;

                let output = tokio::process::Command::new("hyprpicker")
                    .arg("--render-inactive")
                    .stdin(Stdio::null())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
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
        Text::new("No recent colors.").into()
    } else {
        BoxedList::new(
            state
                .items
                .iter()
                .map(|color| color_value_tile(color).into())
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
        Column::new(tree![pick_color_tile(), Text::new("Recent colors"), recent]),
    ]);
    shell.size = PopoverSize::Medium;
    shell.into()
}

fn pick_color_tile() -> Tile<Msg> {
    let mut tile = Tile::new("Pick color");
    tile.id = Some("pick-color".into());
    tile.left_icon = Some("color-select-symbolic".into());
    tile.activatable = true;
    tile.on_click = Some(MsgMapper::new(|()| Msg::Pick));
    tile
}

fn color_value_tile(color: &str) -> Tile<Msg> {
    let color = color.to_owned();
    let mut tile = Tile::new(color.clone());
    tile.id = Some(format!("copy-color-{}", color.trim_start_matches('#')));
    tile.left = Some(Box::new(Text::new(color.clone()).into()));
    tile.secondary = Some("Copy to clipboard".into());
    tile.activatable = true;
    tile.on_click = Some(MsgMapper::new(move |()| Msg::CopyColor(color.clone())));
    tile
}

fn is_hyprpicker_installed() -> bool {
    std::env::var_os("PATH")
        .map(|path| std::env::split_paths(&path).any(|dir| dir.join("hyprpicker").exists()))
        .unwrap_or(false)
}

#[tokio::main]
async fn main() -> AppletResult<()> {
    run(ColorPickerApplet, State::default()).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn popover_uses_tile_to_trigger_picker() {
        let state = State::default();
        let tree = serde_json::to_value(popover_tree(&state, true)).expect("serialize popover");

        assert_eq!(
            tree["data"]["children"][1]["data"]["children"][0],
            json!({
                "type": "tile",
                "data": {
                    "id": "pick-color",
                    "primary": "Pick color",
                    "left_icon": "color-select-symbolic",
                    "activatable": true
                }
            })
        );
    }

    #[test]
    fn recent_color_tile_shows_value_and_uses_left_slot() {
        let state = State {
            items: vec!["#336699".into()],
        };
        let tree = serde_json::to_value(popover_tree(&state, true)).expect("serialize popover");
        let tile = &tree["data"]["children"][1]["data"]["children"][2]["data"]["children"][0];

        assert_eq!(tile["type"], "tile");
        assert_eq!(tile["data"]["primary"], "#336699");
        assert_eq!(tile["data"]["left"]["type"], "text");
        assert_eq!(tile["data"]["left"]["data"]["text"], "#336699");
        assert_eq!(tile["data"]["activatable"], true);
    }
}
