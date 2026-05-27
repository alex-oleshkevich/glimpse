use async_trait::async_trait;
use glimpse_sdk::{
    Applet, AppletResult, Column, Hero, Label, MsgMapper, StatusItem, Tile, TreeNode, run, tree,
};

#[derive(Debug, Clone, Default)]
struct CounterState {
    count: u32,
}

struct CounterApplet;

#[derive(Debug, Clone, PartialEq)]
enum Msg {
    Increment,
}

#[async_trait]
impl Applet for CounterApplet {
    type State = CounterState;
    type Msg = Msg;

    async fn status(&self, state: &Self::State) -> AppletResult<Vec<StatusItem>> {
        Ok(vec![
            StatusItem::new("counter")
                .icon("view-refresh-symbolic")
                .label(state.count.to_string()),
        ])
    }

    async fn update(&mut self, state: &mut CounterState, msg: Msg) -> AppletResult<()> {
        if msg == Msg::Increment {
            state.count += 1;
        }
        Ok(())
    }

    async fn popover(&self, state: &Self::State) -> AppletResult<Option<TreeNode<Msg>>> {
        Ok(Some(
            Column::new(tree![
                {
                    let mut hero = Hero::new("__NAME__", format!("Value: {}", state.count));
                    hero.icon = Some("view-refresh-symbolic".into());
                    hero
                },
                Label::new(format!("Count = {}", state.count)),
                {
                    let mut tile = Tile::new("Increment");
                    tile.left_icon = Some("list-add-symbolic".into());
                    tile.on_click = Some(MsgMapper::new(|()| Msg::Increment));
                    tile
                },
            ])
            .into(),
        ))
    }
}

#[tokio::main]
async fn main() -> AppletResult<()> {
    run(CounterApplet, CounterState::default()).await
}
