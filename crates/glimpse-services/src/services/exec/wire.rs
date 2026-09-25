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
    Session {
        action: SessionVerb,
    },
    ClosePopover,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SessionVerb {
    Lock,
    Suspend,
    Hibernate,
    LogOut,
    Reboot,
    PowerOff,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "lowercase")]
pub enum Outgoing {
    Hello {
        v: u32,
        name: String,
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
mod tests {
    use super::{FromApplet, Outgoing, Urgency};

    const HELLO: &str = include_str!("../../../../../sdk/applet/fixtures/hello.ndjson");
    const OPS: &str = include_str!("../../../../../sdk/applet/fixtures/ops.ndjson");
    const TOOLTIP: &str = include_str!("../../../../../sdk/applet/fixtures/tooltip.ndjson");
    const HOST: &str = include_str!("../../../../../sdk/applet/fixtures/host.ndjson");
    const REQUESTS: &str = include_str!("../../../../../sdk/applet/fixtures/requests.ndjson");
    const GARBAGE: &str = include_str!("../../../../../sdk/applet/fixtures/invalid/garbage.ndjson");

    fn lines(source: &str) -> impl Iterator<Item = &str> {
        source
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
    }

    #[test]
    fn every_applet_fixture_line_decodes() {
        for source in [HELLO, OPS, REQUESTS, TOOLTIP] {
            for line in lines(source) {
                serde_json::from_str::<FromApplet>(line)
                    .unwrap_or_else(|error| panic!("{line}: {error}"));
            }
        }
    }

    #[test]
    fn host_fixture_serializes_byte_identical() {
        for line in lines(HOST) {
            let message: Outgoing =
                serde_json::from_str(line).unwrap_or_else(|error| panic!("{line}: {error}"));
            let encoded = serde_json::to_string(&message).expect("outgoing encodes");
            assert_eq!(encoded, line);
        }
    }

    #[test]
    fn a_notify_without_optional_fields_uses_the_defaults() {
        let message = serde_json::from_str::<FromApplet>(r#"{"t":"notify","summary":"Saved"}"#)
            .expect("notify");
        assert_eq!(
            message,
            FromApplet::Notify {
                summary: "Saved".to_owned(),
                body: String::new(),
                icon: None,
                urgency: Urgency::Normal,
            }
        );
    }

    #[test]
    fn a_garbage_line_does_not_decode() {
        for line in lines(GARBAGE) {
            assert!(serde_json::from_str::<FromApplet>(line).is_err());
            assert!(serde_json::from_str::<Outgoing>(line).is_err());
        }
    }

    #[test]
    fn an_unknown_tag_or_op_is_a_decode_error() {
        assert!(serde_json::from_str::<FromApplet>(r#"{"t":"dance"}"#).is_err());
        assert!(
            serde_json::from_str::<FromApplet>(r#"{"t":"commit","ops":[{"op":"explode","id":1}]}"#)
                .is_err()
        );
        assert!(serde_json::from_str::<Outgoing>(r#"{"t":"dance"}"#).is_err());
    }
}
