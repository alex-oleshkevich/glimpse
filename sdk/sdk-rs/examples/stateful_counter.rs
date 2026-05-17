use async_trait::async_trait;
use glimpse_sdk::{
    Applet, AppletResult, Button, ButtonVariant, CallbackEvent, Column, Hero, Label,
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
                .icon("view-refresh-symbolic")
                .label(state.count.to_string()),
        ])
    }

    async fn popover(&self, state: &Self::State) -> AppletResult<Option<TreeNode>> {
        Ok(Some(
            Column::new(tree![
                Hero::new("Counter", format!("Value: {}", state.count))
                    .icon("view-refresh-symbolic"),
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
