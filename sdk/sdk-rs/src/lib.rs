mod app;
mod events;
mod protocol;
mod widgets;

/// Build a `Vec<TreeNode>` from heterogeneous widget expressions, wrapping
/// each in `TreeNode::from(...)` automatically.
///
/// ```ignore
/// use glimpse_sdk::{tree, Button, Column, Hero, Item, Section, TreeNode};
///
/// let column = Column::new(tree![
///     Hero::new("Counter", "Value: 0"),
///     Section::new("Controls", tree![
///         Item::new("Current"),
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
pub use events::{
    CallbackEvent, ChangeEvent, ClickEvent, InitEvent, InputEvent, PopoverEvent, ScrollEvent,
    ToggleEvent, parse_callback_event, parse_init_event,
};
pub use protocol::{Icon, MenuItem, StatusItem};
pub use widgets::{
    ActionMenu, ActionMenuItem, ActionRow, Align, Badge, BoxNode, Button, Card, Checkbox,
    Collapsible, CollapsibleItem, Column, Copyable, DetailGrid, DetailGridItem, Dropdown,
    DropdownItem, EmptyState, Grid, GridChild, Header, Hero, IconWidget, Image, Item, Label, Meter,
    Orientation, Progress, Row, Scale, Scroll, Section, Separator, Spinner, StatusDot, Switch,
    Toast, ToastAction, TreeNode, Variant,
};
