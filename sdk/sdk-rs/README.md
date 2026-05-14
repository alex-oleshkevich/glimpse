# Glimpse Applet Rust SDK

Small async framework for building Glimpse `exec` applets without touching stdio or raw JSON.

## Install

```toml
[dependencies]
async-trait = "0.1"
glimpse-sdk = "0.3"
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

## Develop

Create and live-run a Rust applet project with the Glimpse tooling:

```sh
glimpse-applet new counter --lang rust
cd counter
glimpse-applet dev
```

Read `docs/custom-applets/tooling.md` for project layout, `applet.toml`, dev applets, linking, and diagnostics.

## Goals

- typed protocol models
- typed widget builders
- async runtime
- trait-based applet API: `status(&state)`, `popover(&state)`, and event handlers receive `&mut state`
- state owned by the runtime; mutate it directly in handlers

## Example

```rust
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
                Hero::new("Counter", format!("Value: {}", state.count)),
                Label::new(format!("Count = {}", state.count)),
                Button::new("increment")
                    .label("Increment")
                    .icon("list-add-symbolic")
                    .variant(ButtonVariant::Primary),
            ])
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
```
