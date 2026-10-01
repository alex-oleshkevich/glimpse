use super::{FromApplet, Outgoing, Urgency};

const HELLO: &str = include_str!("../../../../../../sdk/applet/fixtures/hello.ndjson");
const OPS: &str = include_str!("../../../../../../sdk/applet/fixtures/ops.ndjson");
const TOOLTIP: &str = include_str!("../../../../../../sdk/applet/fixtures/tooltip.ndjson");
const HOST: &str = include_str!("../../../../../../sdk/applet/fixtures/host.ndjson");
const REQUESTS: &str = include_str!("../../../../../../sdk/applet/fixtures/requests.ndjson");
const GARBAGE: &str = include_str!("../../../../../../sdk/applet/fixtures/invalid/garbage.ndjson");

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
    let message =
        serde_json::from_str::<FromApplet>(r#"{"t":"notify","summary":"Saved"}"#).expect("notify");
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
fn an_unknown_tag_decodes_as_unknown_and_an_unknown_op_does_not_decode() {
    assert_eq!(
        serde_json::from_str::<FromApplet>(r#"{"t":"dance","x":1}"#).expect("unknown"),
        FromApplet::Unknown
    );
    assert!(
        serde_json::from_str::<FromApplet>(r#"{"t":"commit","ops":[{"op":"explode","id":1}]}"#)
            .is_err()
    );
    assert!(serde_json::from_str::<Outgoing>(r#"{"t":"dance"}"#).is_err());
}
