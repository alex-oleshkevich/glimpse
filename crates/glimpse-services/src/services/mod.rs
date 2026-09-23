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

mod audio;
mod battery;
mod bluetooth;
mod brightness;
mod calendar;
mod clipboard;
mod color_picker;
mod compositor;
mod geolocation;
mod heartbeat;
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
mod system_monitor;
mod tray;
mod weather;

pub use audio::{
    App as AudioApp, AppId as AudioAppId, Audio, AudioError, AudioHandle, AudioState,
    Device as AudioDevice, DeviceId as AudioDeviceId, Direction as AudioDirection,
    NAME_CAP as AUDIO_NAME_CAP, Role as AudioRole, StreamRef as AudioStreamRef,
};
pub use battery::{
    Battery, BatteryHandle, BatteryState, Charge, Peripheral as BatteryPeripheral, Profiles,
    Supply as BatterySupply,
};
pub use bluetooth::BluetoothError;
pub use bluetooth::{
    Adapter, Answer, Bluetooth, BluetoothHandle, BluetoothState, Busy, Confirmation, Device,
    DeviceId, Failure, Hold, Prompt,
};
pub use brightness::{
    Backlight, Brightness, BrightnessHandle, BrightnessState, CompositeBacklight,
    Config as BrightnessConfig, DdcBacklight, Dependencies as BrightnessDependencies,
    Entry as BacklightEntry, Kind as BrightnessKind, Source as BrightnessSource, SysfsBacklight,
    UnavailableBacklight,
};
pub use calendar::{Calendar, CalendarHandle};
pub use clipboard::{
    Clipboard, ClipboardEntry, ClipboardEntryId, ClipboardHandle, ClipboardKind, ClipboardState,
    Config as ClipboardConfig, Dependencies as ClipboardDependencies,
};
pub use color_picker::{
    ColorPicker, ColorPickerHandle, ColorPickerState, Config as ColorPickerConfig,
    Dependencies as ColorPickerDependencies, PickRequest, Picked, PickedColor, Picker,
    ProcessPicker,
};
pub use compositor::{Compositor, CompositorHandle, CompositorState};
pub use geolocation::{Geolocation, GeolocationHandle};
pub use heartbeat::{Heartbeat, HeartbeatHandle};
pub use keyboard::{Dependencies as KeyboardDependencies, Keyboard, KeyboardHandle};
pub use mpris::{Mpris, MprisHandle};
pub use network::{
    Access, Answer as SecretAnswer, Busy as NetworkBusy, Failure as NetworkFailure, Network,
    NetworkError, NetworkHandle, NetworkId, NetworkState, Radio, Request as SecretRequest, Saved,
    Secret as NetworkSecret, Vpn, Wired,
};
pub use night_light::{
    Config as NightLightConfig, DAY as NEUTRAL_KELVIN, Dependencies as NightLightDependencies,
    NightLight, NightLightHandle, NightLightState,
};
pub use notifications::{Incoming, Notifications, NotificationsHandle, NotificationsState};
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
    SessionActionsHandle, SessionActionsState, SessionEntry, Updates as SessionUpdates,
};
pub use solar::{Solar, SolarDependencies, SolarHandle};
pub use system_monitor::{
    Cpu as SystemMonitorCpu, DiskUsage as SystemMonitorDiskUsage, Gpu as SystemMonitorGpu,
    GpuMemoryKind as SystemMonitorGpuMemoryKind, LoadAverage as SystemMonitorLoadAverage,
    NetworkRate as SystemMonitorNetworkRate, SystemMonitor, SystemMonitorHandle,
    SystemMonitorState, Usage as SystemMonitorUsage,
};
pub use tray::{Tray, TrayHandle, TrayItems};
pub use weather::{Config as WeatherConfig, Weather, WeatherDependencies, WeatherHandle};

pub use calendar::{CalendarEvent, CalendarEvents, GuestCounts, Meeting, MeetingProvider, meeting};
pub use compositor::{
    CastInfo, CastKindInfo, CastTargetInfo, CompositorCapabilities, CompositorOutputs,
    CompositorPrivacy, CompositorStatus, CompositorWindows, CompositorWorkspaces, OutputInfo,
    OutputLogical, OutputMode, WindowInfo, WindowRef, WorkspaceInfo, WorkspaceRef,
};
pub use geolocation::GeolocationStatus;
pub use heartbeat::{HeartbeatInterval, HeartbeatTick};
pub use keyboard::{KeyboardLayout, KeyboardLayouts, LayoutRef};
pub use mpris::{MprisPlayers, Playback, PlayerAction, PlayerCapabilities, PlayerStatus, Repeat};
pub use session::SessionStatus;
pub use solar::{SolarPhase, SolarStatus};
