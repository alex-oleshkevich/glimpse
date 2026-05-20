mod app;
mod events;
mod ipc;
mod protocol;
mod widgets;

pub type AppletError = Box<dyn std::error::Error + Send + Sync>;
pub type AppletResult<T> = Result<T, AppletError>;

#[macro_export]
macro_rules! tree {
    () => { ::std::vec::Vec::new() };
    ($($widget:expr),+ $(,)?) => {
        ::std::vec![$( $crate::TreeNode::from($widget) ),+]
    };
}

pub use app::{
    Applet, CommandResult, close_popover, copy_to_clipboard, open_uri, run, run_command,
    show_notification,
};
pub use tokio::sync::mpsc;
pub use events::{
    CallbackEvent, ChangeEvent, ClickEvent, InitEvent, InputEvent, PopoverEvent, ScrollEvent,
    ToggleEvent, parse_callback_event, parse_init_event,
};
pub use ipc::{Event, EventStream, Subscriber, ipc};
pub use protocol::StatusItem;
pub use widgets::{
    ActionItem, Align, Badge, BorderWidth, Button, ButtonVariant, Card, Checkbox, Color, Column,
    Container, ContentFit, Copyable, EmptyState, Expander, FontSize, FontWeight, Grid, GridChild,
    Hero, Icon, Item, LevelBar, LevelBarMode, LinkButton, Meter, MsgMapper, Orientation,
    PagerAppearance, PagerItem, PagerStrip, Picture, PopoverScaffold, PopoverSize, Progress,
    PropertyList, Radius, Row, Scroll, Select, Separator, Slider, Space, Spinner, StatusDot,
    StatusVariant, Switch, Text, TextAlign, ToggleButton, TreeNode, Variant,
};
