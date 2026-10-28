/// What every glimpse process calls itself when it asks a server for something.
pub(crate) const AGENT: &str = concat!("glimpse/", env!("CARGO_PKG_VERSION"));

/// Any error, as the one line a degraded health state or command failure carries.
pub(crate) fn say(error: impl std::fmt::Display) -> String {
    error.to_string()
}

/// A request that failed, without the URL in it. `reqwest` puts the whole URL in its `Display`,
/// and a feed address is the user's business rather than the journal's.
pub(crate) fn transport(error: reqwest::Error) -> String {
    match error.is_timeout() {
        true => "the request timed out".to_owned(),
        false => error.without_url().to_string(),
    }
}

pub(crate) fn cap(text: &str, limit: usize) -> String {
    text.chars().take(limit).collect()
}

pub(crate) fn compile_patterns(
    patterns: &[String],
    warn: impl Fn(&str, regex::Error),
) -> Vec<regex::Regex> {
    patterns
        .iter()
        .filter_map(|pattern| match regex::Regex::new(pattern) {
            Ok(compiled) => Some(compiled),
            Err(error) => {
                warn(pattern, error);
                None
            }
        })
        .collect()
}

mod audio;
mod battery;
mod bluetooth;
mod brightness;
mod calendar;
mod clipboard;
mod compositor;
mod desktop_mode;
mod exec;
mod geolocation;
mod heartbeat;
mod kdeconnect;
mod keyboard;
mod mpris;
mod network;
mod night_light;
mod notifications;
mod places;
mod printing;
mod privacy;
mod removable;
mod session;
mod session_actions;
mod solar;
mod tray;
mod weather;

pub use audio::{
    App as AudioApp, AppId as AudioAppId, Audio, AudioError, AudioHandle, AudioState,
    Device as AudioDevice, DeviceId as AudioDeviceId, Direction as AudioDirection,
    NAME_CAP as AUDIO_NAME_CAP, Role as AudioRole, StreamRef as AudioStreamRef,
};
pub use battery::{
    Battery, BatteryHandle, BatteryState, Charge, Dependencies as BatteryDependencies,
    HistoryPoint as BatteryHistoryPoint, Peripheral as BatteryPeripheral, Profiles,
    Source as BatterySource, Supply as BatterySupply,
};
pub use bluetooth::BluetoothError;
pub use bluetooth::{
    Adapter, Bluetooth, BluetoothHandle, BluetoothState, Busy, Confirmation, Device, DeviceId,
    Failure, Hold, icon_for,
};
pub use brightness::{
    Backlight, Brightness, BrightnessHandle, BrightnessState, CompositeBacklight,
    Config as BrightnessConfig, DdcBacklight, Dependencies as BrightnessDependencies,
    Entry as BacklightEntry, Enumeration as BacklightEnumeration, Kind as BrightnessKind,
    Source as BrightnessSource, SysfsBacklight,
};
pub use calendar::{Calendar, CalendarHandle, CalendarState, FailingSource};
pub use clipboard::{
    Clipboard, ClipboardEntry, ClipboardEntryId, ClipboardHandle, ClipboardKind, ClipboardState,
    Config as ClipboardConfig, Dependencies as ClipboardDependencies,
};
pub use compositor::{
    Compositor, CompositorHandle, CompositorState, Dependencies as CompositorDependencies,
};
pub use desktop_mode::{DesktopMode, DesktopModeHandle, ModeDependencies, ModeWriter};
pub use exec::{
    ActionRowProps, AddonProps, Align, BarsProps, BoxProps, ButtonProps, CalendarMarker,
    CalendarProps, Catalog as ExecCatalog, CheckButtonProps, ChoiceItem, ChoiceListProps,
    ClassName, ContainerProps, DesktopCatalog, DotsProps, DropDownProps, Edge, Element,
    ElementKind, Ellipsize, EndProps, Entry as ExecEntry, EntryProps, EntryRowProps,
    ExpanderRowProps, FactItem, FactListProps, ImageProps, IndicatorProps, LabelProps,
    MeterRowProps, Node, NoticeProps, Op, Orientation, Paint, PlaceholderProps, Placement,
    PopoverProps, PopoverSize, ROOT, RangeBarProps, ReadoutProps, RevealerProps, RowProps, RowTone,
    RowTypography, ScaleProps, SectionProps, SeparatorProps, Severity, SliderRowProps,
    SparklineProps, SpinnerProps, StackProps, SwatchProps, SwatchSize, SwitchProps,
    ToggleButtonProps, ToggleControl, ToggleRowProps, Tone, Transition, Tree, WireNode, Zone,
    expand as exec_expand,
};
pub use exec::{
    BarRequest as ExecBarRequest, Config as ExecConfig, Dependencies as ExecDependencies,
    DevRequest as ExecDevRequest, Exec, ExecHandle, ExecState, SlotState as ExecSlotState,
    Status as ExecStatus, SystemdLauncher, UserEvent as ExecUserEvent,
};
pub use geolocation::{Geolocation, GeolocationDependencies, GeolocationHandle};
pub use glimpse_compositors::{
    client::{CompositorProvider, Handle as CompositorProviderHandle},
    contract::Capability,
};
pub use heartbeat::{Heartbeat, HeartbeatHandle};
pub use kdeconnect::{
    Action as KdeconnectAction, Actions as KdeconnectActions, Battery as KdeconnectBattery,
    Device as KdeconnectDevice, DeviceId as KdeconnectDeviceId, DeviceType as KdeconnectDeviceType,
    Kdeconnect, KdeconnectHandle, KdeconnectState, PairState as KdeconnectPairState,
    batteries as kdeconnect_batteries,
};
pub use keyboard::{Dependencies as KeyboardDependencies, Keyboard, KeyboardHandle};
pub use mpris::{Mpris, MprisHandle};
pub use network::{
    Access, Busy as NetworkBusy, Failure as NetworkFailure, Network, NetworkError, NetworkHandle,
    NetworkId, NetworkState, Radio, Saved, Vpn, Wired,
};
pub use night_light::{
    Config as NightLightConfig, DAY as NEUTRAL_KELVIN, Dependencies as NightLightDependencies,
    NightLight, NightLightHandle, NightLightState,
};
pub use notifications::{
    Incoming, Notifications, NotificationsHandle, NotificationsState, UNREAD_MAX,
};
pub use places::{
    Config as PlacesConfig, Kind as PlacesKind, Place, Places, PlacesHandle, PlacesState,
};
pub use printing::{
    Config as PrintingConfig, Failure as PrintingFailure, JobState, PrintJob, Printer,
    PrinterState, Printing, PrintingError, PrintingHandle, PrintingState,
};
pub use privacy::{
    Dependencies as PrivacyDependencies, Privacy, PrivacyHandle, PrivacyState,
    Resource as PrivacyResource, Usage as PrivacyUsage,
};
pub use removable::{
    Busy as RemovableBusy, Capacity as RemovableCapacity, Config as RemovableConfig, Drive,
    DriveId, Encryption as RemovableEncryption, Failure as RemovableFailure,
    Mount as RemovableMount, Removable, RemovableError, RemovableHandle, RemovableState, Volume,
    VolumeId,
};
pub use session::{Dependencies as SessionDependencies, Session, SessionHandle};
pub use session_actions::{
    Action as SessionAction, Capability as SessionCapability,
    Dependencies as SessionActionsDependencies, Inhibitor as SessionInhibitor, SessionActions,
    SessionActionsHandle, SessionActionsState,
};
pub use solar::{Solar, SolarDependencies, SolarHandle};
pub use tray::{Tray, TrayHandle, TrayItems};
pub use weather::{Config as WeatherConfig, Weather, WeatherDependencies, WeatherHandle};

pub use calendar::{Meeting, MeetingProvider, meeting};
pub use compositor::{
    CastInfo, CompositorOutputs, CompositorPrivacy, CompositorStatus, CompositorWindows,
    CompositorWorkspaces, OutputInfo, OutputLogical, OutputMode, WindowInfo, WindowRef,
    WorkspaceInfo, WorkspaceRef,
};
pub use geolocation::GeolocationStatus;
pub use glimpse_dbus::calendar::{CalendarEvent, CalendarEvents, GuestCounts};
pub use heartbeat::{HeartbeatInterval, HeartbeatTick};
pub use keyboard::{KeyboardLayout, KeyboardLayouts, LayoutRef};
pub use mpris::{MprisPlayers, Playback, PlayerAction, PlayerCapabilities, PlayerStatus, Repeat};
pub use session::SessionStatus;
pub use solar::{SolarPhase, SolarStatus};
