use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

pub const MAX_LINE: usize = 1 << 20;

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "t", rename_all = "kebab-case")]
pub enum FromApplet {
    Hello {
        v: u32,
    },
    Commit {
        ops: Vec<Op>,
    },
    Notify {
        summary: String,
        #[serde(default)]
        body: String,
        #[serde(default)]
        icon: Option<String>,
        #[serde(default)]
        urgency: Urgency,
    },
    Copy {
        text: String,
    },
    OpenUri {
        uri: String,
    },
    Launch {
        id: String,
    },
    Mpris {
        player: String,
        #[serde(flatten)]
        call: MprisCall,
    },
    Compositor {
        #[serde(flatten)]
        call: CompositorCall,
    },
    ClosePopover,
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "op", rename_all = "kebab-case")]
pub enum MprisCall {
    Control { action: MprisAction },
    Seek { offset: i64 },
    SetPosition { position: i64 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MprisAction {
    Play,
    Pause,
    PlayPause,
    Stop,
    Previous,
    Next,
    Raise,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "op", rename_all = "kebab-case")]
pub enum CompositorCall {
    FocusWorkspace {
        target: WorkspaceTarget,
    },
    FocusWindow {
        target: WindowTarget,
    },
    MoveWindow {
        window: u64,
        target: WorkspaceTarget,
    },
    RenameWorkspace {
        id: u64,
        name: Option<String>,
    },
    CloseWindow {
        id: u64,
    },
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(untagged)]
pub enum WorkspaceTarget {
    Id { id: u64 },
    Index { index: u8 },
    Name { name: String },
    Step(Step),
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(untagged)]
pub enum WindowTarget {
    Id { id: u64 },
    Step(Step),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Step {
    Next,
    Prev,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "op", rename_all = "lowercase")]
pub enum Op {
    Insert {
        parent: u32,
        node: WireNode,
        before: Option<u32>,
    },
    Move {
        parent: u32,
        id: u32,
        before: Option<u32>,
    },
    Remove {
        parent: u32,
        id: u32,
    },
    Set {
        id: u32,
        props: Map<String, Value>,
        #[serde(default)]
        seq: Option<u64>,
    },
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct WireNode {
    pub id: u32,
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub props: Map<String, Value>,
    #[serde(default)]
    pub children: Vec<WireNode>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Urgency {
    Low,
    #[default]
    Normal,
    Critical,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "lowercase")]
pub enum Outgoing {
    Hello {
        v: u32,
        name: String,
        state: String,
        options: Map<String, Value>,
        placement: Placement,
    },
    Options {
        options: Map<String, Value>,
    },
    Placement {
        placement: Placement,
    },
    Event {
        id: u32,
        name: String,
        args: Vec<Value>,
        seq: Option<u64>,
    },
    Popover {
        open: bool,
    },
    Service {
        name: String,
        value: Value,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Placement {
    pub output: Option<String>,
    pub position: Edge,
    pub orientation: Orientation,
    pub zone: Zone,
    pub size: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Edge {
    Top,
    Bottom,
    Left,
    Right,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Orientation {
    #[default]
    Horizontal,
    Vertical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Zone {
    Left,
    Center,
    Right,
}

#[cfg(test)]
mod tests;
