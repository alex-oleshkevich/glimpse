use chrono::{DateTime, Utc};
use serde::{Serialize, de::DeserializeOwned};

use crate::types::*;

/// One invocable command. `Args` is the type itself, the way a topic's `Payload` is, so a command
/// is one named struct rather than a marker plus an argument type that can drift from it.
pub trait Command {
    const NAME: &'static str;
    type Args: Serialize + DeserializeOwned + Send + 'static;
    type Output: Serialize + DeserializeOwned + Send + 'static;
}

#[macro_export]
macro_rules! commands {
    ($(
        #[name = $name:literal]
        $(#[$meta:meta])*
        $vis:vis struct $ty:ident {
            $( $(#[$field_meta:meta])* $field:ident : $fty:ty ),* $(,)?
        } -> $output:ty;
    )*) => {
        $(
            $(#[$meta])*
            #[derive(Debug, Clone, PartialEq, ::serde::Serialize, ::serde::Deserialize)]
            $vis struct $ty {
                $( $(#[$field_meta])* pub $field: $fty, )*
            }

            impl $crate::Command for $ty {
                const NAME: &'static str = $name;
                type Args = Self;
                type Output = $output;
            }
        )*

        /// Every command name the tree knows. See `ALL_TOPICS`.
        pub const ALL_COMMANDS: &[&str] = &[$($name),*];
    };
}

commands! {
    #[name = "heartbeat.reset"]
    pub struct HeartbeatReset {} -> ();

    #[name = "heartbeat.set_interval"]
    pub struct HeartbeatSetInterval { period_ms: u64 } -> HeartbeatInterval;

    #[name = "geolocation.refresh"]
    pub struct GeolocationRefresh {} -> ();

    #[name = "solar.refresh"]
    pub struct SolarRefresh {} -> ();

    #[name = "compositor.focus_workspace"]
    pub struct FocusWorkspace { target: WorkspaceRef } -> ();

    #[name = "compositor.focus_window"]
    pub struct FocusWindow { target: WindowRef } -> ();

    #[name = "compositor.focus_output"]
    pub struct FocusOutput { connector: String } -> ();

    #[name = "compositor.rename_workspace"]
    pub struct RenameWorkspace { id: u64, name: Option<String> } -> ();

    #[name = "compositor.move_workspace_to_output"]
    pub struct MoveWorkspaceToOutput { id: u64, connector: String } -> ();

    #[name = "compositor.reorder_workspace"]
    pub struct ReorderWorkspace { id: u64, index: u8 } -> ();

    #[name = "compositor.move_window_to_workspace"]
    pub struct MoveWindowToWorkspace { window: u64, workspace: WorkspaceRef } -> ();

    #[name = "compositor.close_window"]
    pub struct CloseWindow { id: u64 } -> ();

    #[name = "calendar.refresh"]
    pub struct CalendarRefresh {} -> ();

    #[name = "calendar.set_range"]
    pub struct CalendarSetRange { from: DateTime<Utc>, to: DateTime<Utc> } -> ();

    #[name = "weather.watch"]
    pub struct WeatherWatch { place: WatchedPlace } -> ();

    #[name = "weather.refresh"]
    pub struct WeatherRefresh {} -> ();

    #[name = "mpris.control"]
    pub struct MprisControl { player: String, action: PlayerAction } -> ();

    #[name = "mpris.seek"]
    pub struct MprisSeek { player: String, offset_us: i64 } -> ();

    #[name = "mpris.set_position"]
    pub struct MprisSetPosition { player: String, position_us: i64 } -> ();

    #[name = "mpris.set_volume"]
    pub struct MprisSetVolume { player: String, volume: f64 } -> ();

    #[name = "mpris.set_repeat"]
    pub struct MprisSetRepeat { player: String, repeat: Repeat } -> ();

    #[name = "mpris.set_shuffle"]
    pub struct MprisSetShuffle { player: String, shuffle: bool } -> ();

    #[name = "notifications.dismiss"]
    pub struct NotificationsDismiss { id: u32 } -> ();

    #[name = "notifications.invoke_action"]
    /// `activation_token` is an xdg-activation token the caller minted from the click that
    /// invoked the action. The server emits `ActivationToken` immediately before `ActionInvoked`
    /// when one is present and nothing when it is absent, which the specification permits.
    pub struct NotificationsInvokeAction {
        id: u32,
        action: String,
        activation_token: Option<String>,
    } -> ();

    #[name = "notifications.clear_app"]
    pub struct NotificationsClearApp { app_id: String } -> ();

    #[name = "notifications.clear_all"]
    pub struct NotificationsClearAll {} -> ();

    #[name = "notifications.set_dnd"]
    pub struct NotificationsSetDnd { dnd: DoNotDisturb } -> ();

    #[name = "keyboard.switch_layout"]
    pub struct SwitchLayout { target: LayoutRef } -> ();
}
