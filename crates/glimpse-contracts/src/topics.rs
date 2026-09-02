use std::collections::BTreeMap;

use serde::{Serialize, de::DeserializeOwned};

use crate::types::*;

pub trait Message {
    const NAME: &'static str;
    type Payload: Clone + Serialize + DeserializeOwned + PartialEq + Send + Sync + 'static;
}

#[macro_export]
macro_rules! topic {
    ($ty:ty, $name:literal) => {
        impl $crate::Message for $ty {
            const NAME: &'static str = $name;
            type Payload = Self;
        }
    };
}

#[macro_export]
macro_rules! topics {
    ($(
        #[name = $name:literal]
        $(#[$meta:meta])*
        $vis:vis struct $ty:ident {
            $( $(#[$field_meta:meta])* $field:ident : $fty:ty ),* $(,)?
        }
    )*) => {
        $(
            $(#[$meta])*
            #[derive(Debug, Clone, PartialEq, ::serde::Serialize, ::serde::Deserialize)]
            $vis struct $ty {
                $( $(#[$field_meta])* pub $field: $fty, )*
            }

            $crate::topic!($ty, $name);
        )*

        /// Every topic name the tree knows. A second `topics!` invocation is a duplicate
        /// definition of this, which is the intended way to keep them in one block.
        pub const ALL_TOPICS: &[&str] = &[$($name),*];
    };
}

topics! {
    #[name = "system.topics"]
    pub struct SystemTopics { topics: BTreeMap<String, TopicReport> }

    #[name = "system.methods"]
    pub struct SystemMethods { methods: BTreeMap<String, MethodReport> }

    #[name = "system.services"]
    pub struct SystemServices { services: BTreeMap<String, ServiceState> }

    #[name = "solar.status"]
    pub struct SolarStatus { phase: SolarPhase }

    #[name = "geolocation.status"]
    pub struct GeolocationStatus { coordinates: Option<GeoCoordinates> }

    #[name = "heartbeat.tick"]
    pub struct HeartbeatTick { count: u64 }

    #[name = "compositor.status"]
    pub struct CompositorStatus { name: String, capabilities: CompositorCapabilities }

    #[name = "compositor.workspaces"]
    pub struct CompositorWorkspaces { workspaces: Vec<WorkspaceInfo> }

    #[name = "compositor.windows"]
    pub struct CompositorWindows { windows: Vec<WindowInfo> }

    #[name = "compositor.outputs"]
    pub struct CompositorOutputs { outputs: Vec<OutputInfo> }
}
