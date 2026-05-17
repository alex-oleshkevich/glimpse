mod app;
mod events;
mod ipc;
mod protocol;
mod widgets;

/// Build a `Vec<TreeNode>` from heterogeneous widget expressions, wrapping
/// each in `TreeNode::from(...)` automatically.
///
/// ```ignore
/// use glimpse_sdk::{tree, Button, Column, Hero, Label, Section, TreeNode};
///
/// let column = Column::new(tree![
///     Hero::new("Counter", "Value: 0"),
///     Section::new("Controls", tree![
///         Label::new("Current"),
///         Button::new("increment").label("Increment"),
///     ]),
/// ])
/// .spacing(8);
/// ```
#[macro_export]
macro_rules! tree {
    () => { ::std::vec::Vec::<$crate::TreeNode>::new() };
    ($($widget:expr),+ $(,)?) => {
        ::std::vec![$( $crate::TreeNode::from($widget) ),+]
    };
}

pub use app::{Applet, AppletError, AppletResult, run};
pub use ipc::{Event, EventStream, Subscriber, ipc};
pub use events::{
    CallbackEvent, ChangeEvent, ClickEvent, InitEvent, InputEvent, PopoverEvent, ScrollEvent,
    ToggleEvent, parse_callback_event, parse_init_event,
};
pub use protocol::StatusItem;
pub use widgets::{
    ActionItem, Align, Badge, Button, ButtonVariant, Card, Checkbox, Column, Copyable,
    ContentFit, EmptyState, Expander, Grid, GridChild, Hero, Icon, Item, Label, LevelBar,
    LevelBarMode, LinkButton, ListBox, MenuButton, Meter, Orientation, Overlay, PagerAppearance,
    PagerItem, PagerStrip, Picture, Progress, PropertyList, Row, Scroll, Section, Select,
    Separator, Slider, Spinner, StatusDot, Switch, ToggleButton, TreeExpander,
    TreeNode, Variant,
};
