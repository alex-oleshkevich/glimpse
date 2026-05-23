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
pub use events::{
    CallbackEvent, ChangeEvent, ClickEvent, InitEvent, InputEvent, PopoverEvent, ScrollEvent,
    ToggleEvent, parse_callback_event, parse_init_event,
};
pub use ipc::{Event, EventStream, Subscriber, ipc};
pub use protocol::StatusItem;
pub use tokio::sync::mpsc;
pub use widgets::{
    ActiveIndicator, Badge, BadgeKind, BatteryHero, BoxedList, ButtonRow, Calendar,
    CameraIndicator, Choice, ChoiceList, ChoiceTile, CircleBox, Column, CommonProps, Container,
    DateHero, EmptyState, EventItem, Events, ExpanderTile, FontSize, FontWeight, Header, Hero,
    KeyValueGrid, KeyValueRow, LocationIndicator, Meter, MicIndicator, MsgMapper, MutedIndicator,
    PagerAppearance, PagerItem, PagerStrip, PanelIndicator, PopoverShell, PopoverSize, Row,
    ScreenCastIndicator, Scroll, SegmentedTile, Separator, SliderTile, Spinner, StatusDot,
    StatusDotStatus, SwitchTile, Text, TextColor, Tile, TreeNode, WeatherForecastItem,
    WeatherForecastList, WeatherHourlyItem, WeatherHourlyStrip, WorldClock, WorldClockRow,
};
