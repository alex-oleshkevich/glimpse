use async_trait::async_trait;
use serde::Serialize;
use tokio::io::{self, AsyncBufReadExt, AsyncWriteExt, BufReader};

use crate::{
    events::{
        CallbackEvent, InitEvent, parse_callback_event, parse_incoming_line, parse_init_event,
    },
    protocol::StatusItem,
    widgets::TreeNode,
};

pub type AppletError = Box<dyn std::error::Error + Send + Sync>;
pub type AppletResult<T> = Result<T, AppletError>;

#[derive(Debug, Clone, PartialEq, Serialize, Default)]
pub struct RenderResult {
    #[serde(default)]
    pub status: Vec<StatusItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tree: Option<TreeNode>,
}

#[derive(Debug, Clone)]
pub struct StateStore<State> {
    state: State,
    popover_open: bool,
}

impl<State> StateStore<State> {
    pub fn new(state: State) -> Self {
        Self {
            state,
            popover_open: false,
        }
    }

    pub fn state(&self) -> &State {
        &self.state
    }

    pub fn state_mut(&mut self) -> &mut State {
        &mut self.state
    }

    pub fn set_state<F>(&mut self, patch: F)
    where
        F: FnOnce(&mut State),
    {
        patch(&mut self.state);
    }

    pub fn is_popover_open(&self) -> bool {
        self.popover_open
    }

    pub fn set_popover_open(&mut self, open: bool) {
        self.popover_open = open;
    }
}

#[async_trait]
pub trait Applet: Send {
    type State: Send + Sync + 'static;

    fn store(&self) -> &StateStore<Self::State>;
    fn store_mut(&mut self) -> &mut StateStore<Self::State>;

    async fn render(&self) -> AppletResult<RenderResult>;

    async fn on_start(&mut self) -> AppletResult<()> {
        Ok(())
    }

    async fn on_init(&mut self, _event: InitEvent) -> AppletResult<()> {
        Ok(())
    }

    async fn on_callback(&mut self, _event: CallbackEvent) -> AppletResult<()> {
        Ok(())
    }

    fn state(&self) -> &Self::State {
        self.store().state()
    }

    fn state_mut(&mut self) -> &mut Self::State {
        self.store_mut().state_mut()
    }

    fn set_state<F>(&mut self, patch: F)
    where
        F: FnOnce(&mut Self::State),
    {
        self.store_mut().set_state(patch);
    }

    fn is_popover_open(&self) -> bool {
        self.store().is_popover_open()
    }

    fn set_popover_open(&mut self, open: bool) {
        self.store_mut().set_popover_open(open);
    }
}

#[derive(Debug, Serialize, PartialEq)]
struct TreePayload {
    root: Option<TreeNode>,
}

pub async fn run<A>(mut applet: A) -> AppletResult<()>
where
    A: Applet,
{
    let mut stdout = io::stdout();
    let stdin = BufReader::new(io::stdin());
    let mut lines = stdin.lines();
    let mut last_render: Option<RenderResult> = None;

    applet.on_start().await?;
    let initial = applet.render().await?;
    flush_render(&mut stdout, &mut last_render, initial, applet.is_popover_open()).await?;

    while let Some(line) = lines.next_line().await? {
        if line.trim().is_empty() {
            continue;
        }

        let incoming = match parse_incoming_line(&line) {
            Ok(msg) => msg,
            Err(err) => {
                eprintln!("glimpse-sdk: ignoring malformed input: {err}");
                continue;
            }
        };
        let result: AppletResult<()> = match incoming.kind.as_str() {
            "init" => match parse_init_event(incoming.data) {
                Ok(evt) => applet.on_init(evt).await,
                Err(err) => {
                    eprintln!("glimpse-sdk: ignoring malformed init: {err}");
                    continue;
                }
            },
            "event" => match parse_callback_event(incoming.data) {
                Ok(event) => {
                    if let CallbackEvent::Popover(popover) = &event {
                        applet.set_popover_open(popover.open);
                    }
                    applet.on_callback(event).await
                }
                Err(err) => {
                    eprintln!("glimpse-sdk: ignoring malformed event: {err}");
                    continue;
                }
            },
            _ => continue,
        };
        result?;

        let rendered = applet.render().await?;
        flush_render(
            &mut stdout,
            &mut last_render,
            rendered,
            applet.is_popover_open(),
        )
        .await?;
    }

    Ok(())
}

async fn flush_render(
    stdout: &mut io::Stdout,
    previous: &mut Option<RenderResult>,
    next: RenderResult,
    popover_open: bool,
) -> AppletResult<()> {
    let changed = previous.as_ref();
    if changed
        .map(|prev| prev.status != next.status)
        .unwrap_or(true)
    {
        write_message(
            stdout,
            "status",
            &serde_json::json!({ "items": next.status }),
        )
        .await?;
    }
    let publish_popover = popover_open || previous.is_none() || next.tree.is_none();
    if publish_popover && changed.map(|prev| prev.tree != next.tree).unwrap_or(true) {
        write_message(
            stdout,
            "popover",
            &TreePayload {
                root: next.tree.clone(),
            },
        )
        .await?;
    }

    let mut stored = next;
    if !publish_popover {
        stored.tree = previous.as_ref().and_then(|prev| prev.tree.clone());
    }
    *previous = Some(stored);
    Ok(())
}

async fn write_message<T: Serialize>(
    stdout: &mut io::Stdout,
    command: &str,
    payload: &T,
) -> AppletResult<()> {
    let encoded = serde_json::to_vec(payload)?;
    stdout.write_all(command.as_bytes()).await?;
    stdout.write_all(b" ").await?;
    stdout.write_all(&encoded).await?;
    stdout.write_all(b"\n").await?;
    stdout.flush().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::{
        ActionRow, BoxNode, Button, CallbackEvent, ClickEvent, Icon, Item, Label, MenuItem,
        StatusItem, TreeNode,
    };

    struct DemoApplet {
        store: StateStore<DemoState>,
    }

    #[derive(Debug, Clone)]
    struct DemoState {
        version: String,
        clicks: u32,
    }

    #[async_trait]
    impl Applet for DemoApplet {
        type State = DemoState;

        fn store(&self) -> &StateStore<Self::State> {
            &self.store
        }

        fn store_mut(&mut self) -> &mut StateStore<Self::State> {
            &mut self.store
        }

        async fn render(&self) -> AppletResult<RenderResult> {
            Ok(RenderResult {
                status: vec![
                    StatusItem::new("demo")
                        .icon(Icon::name("demo-symbolic"))
                        .label(self.state().version.clone()),
                ],
                tree: Some(TreeNode::from(BoxNode::vertical(vec![
                    TreeNode::from(crate::Hero::new("Demo", self.state().version.clone())),
                    TreeNode::from(Label::new(self.state().version.clone())),
                    TreeNode::from(Button::new("submit").label("Submit")),
                ]))),
            })
        }

        async fn on_callback(&mut self, event: CallbackEvent) -> AppletResult<()> {
            match event {
                CallbackEvent::Click(click) if click.id == "submit" => {
                    self.set_state(|state| state.clicks += 1);
                    self.set_state(|state| state.version = "v2".into());
                }
                _ => {}
            }
            Ok(())
        }
    }

    #[test]
    fn render_result_defaults_are_empty() {
        let result = RenderResult::default();
        assert!(result.status.is_empty());
        assert!(result.tree.is_none());
    }

    #[test]
    fn parse_callback_event_returns_typed_click_variant() {
        let event = parse_callback_event(json!({
            "id": "submit",
            "type": "click",
            "button": "left"
        }))
        .expect("click event should parse");

        assert_eq!(
            event,
            CallbackEvent::Click(ClickEvent {
                id: "submit".into(),
                button: Some("left".into()),
            })
        );
    }

    #[test]
    fn parse_callback_event_returns_typed_popover_variant() {
        let event = parse_callback_event(json!({
            "id": "popover",
            "type": "open",
            "source": "popover"
        }))
        .expect("popover event should parse");

        assert_eq!(
            event,
            CallbackEvent::Popover(crate::PopoverEvent { open: true })
        );
    }

    #[test]
    fn dropdown_like_tree_nodes_serialize() {
        let node =
            crate::Dropdown::new("env", vec![crate::DropdownItem::new("prod", "Production")]);
        let payload = serde_json::to_value(TreeNode::from(node)).expect("tree should serialize");
        assert_eq!(payload["type"], "dropdown");
        assert_eq!(payload["data"]["items"][0]["id"], "prod");
    }

    #[test]
    fn items_serialize_menu_items() {
        let item = Item::clickable("run", "Run").menu(vec![
            MenuItem::new("open", "Open"),
            MenuItem::new("cancel", "Cancel").enabled(false),
        ]);

        let payload = serde_json::to_value(TreeNode::from(item)).expect("item should serialize");
        assert_eq!(payload["type"], "item");
        assert_eq!(payload["data"]["menu"][0]["id"], "open");
        assert_eq!(payload["data"]["menu"][1]["enabled"], false);
    }

    #[test]
    fn action_rows_use_canonical_protocol_name() {
        let payload = serde_json::to_value(TreeNode::from(ActionRow::new("open", "Open")))
            .expect("action row serializes");
        assert_eq!(payload["type"], "action_row");
    }

    #[test]
    fn status_dot_serializes_as_status_protocol_name() {
        let payload = serde_json::to_value(TreeNode::from(crate::StatusDot::new()))
            .expect("status dot serializes");
        assert_eq!(payload["type"], "status");
    }

    #[test]
    fn row_and_column_serialize_as_layout_protocol_names() {
        let row =
            serde_json::to_value(TreeNode::from(crate::Row::new(vec![]))).expect("row serializes");
        assert_eq!(row["type"], "row");

        let column = serde_json::to_value(TreeNode::from(crate::Column::new(vec![])))
            .expect("column serializes");
        assert_eq!(column["type"], "column");
    }

    #[test]
    fn action_menu_serializes_with_items() {
        let menu = crate::ActionMenu::new(vec![
            crate::ActionMenuItem::new("a", "Alpha").checked(true),
            crate::ActionMenuItem::new("b", "Beta"),
        ])
        .header("Pick one");
        let payload =
            serde_json::to_value(TreeNode::from(menu)).expect("action_menu serializes");
        assert_eq!(payload["type"], "action_menu");
        assert_eq!(payload["data"]["header"], "Pick one");
        assert_eq!(payload["data"]["items"][0]["checked"], true);
    }

    #[test]
    fn spinner_serializes_with_default_spinning() {
        let payload = serde_json::to_value(TreeNode::from(crate::Spinner::new()))
            .expect("spinner serializes");
        assert_eq!(payload["type"], "spinner");
        assert_eq!(payload["data"]["spinning"], true);
    }

    #[test]
    fn variant_serializes_as_semantic_protocol_value() {
        let mut label = Label::new("Warning");
        label.common.variant = Some(crate::Variant::Warning);
        let payload = serde_json::to_value(TreeNode::from(label)).expect("tree should serialize");
        assert_eq!(payload["data"]["variant"], "warning");
    }

    #[tokio::test]
    async fn set_state_updates_rendered_status() {
        let mut applet = DemoApplet {
            store: StateStore::new(DemoState {
                version: "v1".into(),
                clicks: 0,
            }),
        };

        applet
            .on_callback(CallbackEvent::Click(ClickEvent {
                id: "submit".into(),
                button: Some("left".into()),
            }))
            .await
            .expect("callback should update state");

        let rendered = applet.render().await.expect("render should succeed");
        assert_eq!(rendered.status[0].label.as_deref(), Some("v2"));
    }
}
