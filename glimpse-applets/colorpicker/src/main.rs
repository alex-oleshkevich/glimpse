use async_trait::async_trait;
use glimpse_sdk::{
    ActionItem, Align, Applet, AppletResult, Button, ButtonVariant, CallbackEvent, Color, Column,
    Container, EmptyState, Hero, PopoverScaffold, Radius, StatusItem, Text, TreeNode, close_popover,
    copy_to_clipboard, run, tree,
};
use std::process::Stdio;
use tokio::time::Duration;

#[derive(Debug, Default, Clone)]
struct State {
    items: Vec<String>,
}

struct ColorPickerApplet;

#[async_trait]
impl Applet for ColorPickerApplet {
    type State = State;

    async fn status(&self, _state: &Self::State) -> AppletResult<Vec<StatusItem>> {
        Ok(vec![StatusItem::new("colorpicker").icon("color-select-symbolic")])
    }

    async fn popover(&self, state: &Self::State) -> AppletResult<Option<TreeNode>> {
        if !is_hyprpicker_installed() {
            return Ok(Some(
                EmptyState::new("Not installed")
                    .subtitle("hyprpicker is not installed")
                    .into(),
            ));
        }

        let recent: TreeNode = if state.items.is_empty() {
            Text::new("No recent colors.").into()
        } else {
            let mut children: Vec<TreeNode> = vec![{
                let mut header =
                    Container::new(Some(Text::new("Recent colors").css_class("header").into()));
                header.common.halign = Some(Align::Start);
                header.into()
            }];
            for color in &state.items {
                let mut swatch = Container::new(None)
                    .min_width(20)
                    .min_height(20)
                    .border_color(Color::MutedFg)
                    .border_radius(Radius::Pill)
                    .style("background-color", color.as_str());
                swatch.common.hexpand = Some(true);
                swatch.common.vexpand = Some(true);
                children.push(
                    ActionItem::new(format!("pick_{color}"), color.clone())
                        .left(swatch)
                        .into(),
                );
            }
            Column::new(children).spacing(0).into()
        };

        let mut pick_button = Button::new("pick")
            .label("Pick color")
            .variant(ButtonVariant::Primary);
        pick_button.common.hexpand = Some(true);

        let mut body = Column::new(tree![pick_button, recent]).spacing(16);
        body.common.halign = Some(Align::Fill);

        Ok(Some(
            PopoverScaffold::new(body)
                .hero(Hero::new("Color picker", "Pick a color").icon("color-select-symbolic"))
                .into(),
        ))
    }

    async fn on_callback(
        &mut self,
        state: &mut Self::State,
        event: CallbackEvent,
    ) -> AppletResult<()> {
        let CallbackEvent::Click(click) = event else {
            return Ok(());
        };
        if click.id == "pick" {
            close_popover().await?;
            tokio::time::sleep(Duration::from_millis(300)).await;

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
                    let mut items = vec![color];
                    items.extend(std::mem::take(&mut state.items));
                    state.items = items;
                }
            }
        } else if let Some(color) = click.id.strip_prefix("pick_") {
            copy_to_clipboard(color).await.ok();
        }
        Ok(())
    }
}

fn is_hyprpicker_installed() -> bool {
    std::env::var_os("PATH")
        .map(|path| {
            std::env::split_paths(&path).any(|dir| dir.join("hyprpicker").exists())
        })
        .unwrap_or(false)
}

#[tokio::main]
async fn main() -> AppletResult<()> {
    run(ColorPickerApplet, State::default()).await
}
