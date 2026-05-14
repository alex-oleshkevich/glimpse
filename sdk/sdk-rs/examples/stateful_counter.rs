use async_trait::async_trait;
use glimpse_sdk::{
    Applet, AppletResult, BoxNode, Button, ButtonVariant, CallbackEvent, Hero, Icon, Label,
    StatusItem, TreeNode, run, tree,
};

#[derive(Debug, Clone, Default)]
struct CounterState {
    count: u32,
}

struct CounterApplet;

#[async_trait]
impl Applet for CounterApplet {
    type State = CounterState;

    async fn status(&self, state: &Self::State) -> AppletResult<Vec<StatusItem>> {
        Ok(vec![
            StatusItem::new("counter")
                .icon(Icon::name("view-refresh-symbolic"))
                .label(state.count.to_string()),
        ])
    }

    async fn popover(&self, state: &Self::State) -> AppletResult<Option<TreeNode>> {
        Ok(Some(
            BoxNode::vertical(tree![
                Hero::new("Counter", format!("Value: {}", state.count))
                    .icon(Icon::name("view-refresh-symbolic")),
                Label::new(format!("Count = {}", state.count)),
                Button::new("increment")
                    .label("Increment")
                    .icon("list-add-symbolic")
                    .variant(ButtonVariant::Primary),
            ])
            .spacing(8)
            .into(),
        ))
    }

    async fn on_callback(
        &mut self,
        state: &mut Self::State,
        event: CallbackEvent,
    ) -> AppletResult<()> {
        if let CallbackEvent::Click(click) = event {
            if click.id == "increment" {
                state.count += 1;
            }
        }
        Ok(())
    }
}

#[tokio::main]
async fn main() -> AppletResult<()> {
    run(CounterApplet, CounterState::default()).await
}
