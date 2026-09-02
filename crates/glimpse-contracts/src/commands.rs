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
}
