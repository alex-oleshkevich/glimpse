use super::{FromApplet, Outgoing};

const HELLO: &str = include_str!("../../../../../../sdk/sdk-ts/fixtures/hello.ndjson");
const OPS: &str = include_str!("../../../../../../sdk/sdk-ts/fixtures/ops.ndjson");
const TOOLTIP: &str = include_str!("../../../../../../sdk/sdk-ts/fixtures/tooltip.ndjson");
const HOST: &str = include_str!("../../../../../../sdk/sdk-ts/fixtures/host.ndjson");
const REQUESTS: &str = include_str!("../../../../../../sdk/sdk-ts/fixtures/requests.ndjson");
const GARBAGE: &str = include_str!("../../../../../../sdk/sdk-ts/fixtures/invalid/garbage.ndjson");

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
fn host_fixture_serializes_with_a_nullable_entry() {
    for line in lines(HOST) {
        let message: Outgoing =
            serde_json::from_str(line).unwrap_or_else(|error| panic!("{line}: {error}"));
        let encoded = serde_json::to_string(&message).expect("outgoing encodes");
        let expected = if matches!(message, Outgoing::Hello { entry: None, .. })
            && !line.contains("\"entry\"")
        {
            format!(
                "{},\"entry\":null}}",
                line.strip_suffix('}').expect("object")
            )
        } else {
            line.to_owned()
        };
        assert_eq!(encoded, expected);
    }
}

#[test]
fn a_hello_without_an_entry_serializes_it_as_null() {
    let mut hello: serde_json::Value =
        serde_json::from_str(lines(HOST).next().expect("hello")).expect("hello");
    hello.as_object_mut().expect("object").remove("entry");
    let message: Outgoing = serde_json::from_value(hello).expect("hello without entry");
    assert!(matches!(message, Outgoing::Hello { entry: None, .. }));
    let encoded = serde_json::to_value(message).expect("hello encodes");
    assert_eq!(encoded.get("entry"), Some(&serde_json::Value::Null));
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
