mod app;
mod events;
mod ipc;
mod protocol;
mod widgets;

/// Build a `Vec<TreeNode>` from heterogeneous widget expressions, wrapping
/// each in `TreeNode::from(...)` automatically.
///
/// ```ignore
/// use glimpse_sdk::{tree, Button, Card, Column, Hero, Text, TreeNode};
///
/// let column = Column::new(tree![
///     Hero::new("Counter", "Value: 0"),
///     Card::new(Some(Column::new(tree![
///         Text::new("Current"),
///         Button::new("increment").label("Increment"),
///     ]).into())),
/// ])
/// ```
#[macro_export]
macro_rules! tree {
    () => { ::std::vec::Vec::<$crate::TreeNode>::new() };
    ($($widget:expr),+ $(,)?) => {
        ::std::vec![$( $crate::TreeNode::from($widget) ),+]
    };
}

pub use app::{Applet, AppletError, AppletResult, run};
pub use events::{
    CallbackEvent, ChangeEvent, ClickEvent, InitEvent, InputEvent, PopoverEvent, ScrollEvent,
    ToggleEvent, parse_callback_event, parse_init_event,
};
pub use ipc::{Event, EventStream, Subscriber, ipc};
pub use protocol::StatusItem;
pub use widgets::{
    ActionItem, Align, Badge, BorderWidth, Button, ButtonVariant, Card, Checkbox, Color, Column,
    Container, ContentFit, Copyable, EmptyState, Expander, FontSize, FontWeight, Grid, GridChild,
    Hero, Icon, Item, LevelBar, LevelBarMode, LinkButton, Meter, Orientation, PagerAppearance,
    PagerItem, PagerStrip, Picture, PopoverScaffold, PopoverSize, Progress, PropertyList, Radius,
    Row, Scroll, Select, Separator, Slider, Space, Spinner, StatusDot, StatusVariant, Switch, Text,
    TextAlign, ToggleButton, TreeNode, Variant,
};
