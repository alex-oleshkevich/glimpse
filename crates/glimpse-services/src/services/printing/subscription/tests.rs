use super::*;

const CREATE: &str = "020000160000000101470012617474726962757465732d6368617273657400057574662d3848001b617474726962757465732d6e61747572616c2d6c616e67756167650002656e45000b7072696e7465722d75726900106970703a2f2f6c6f63616c686f73742f42001472657175657374696e672d757365722d6e616d650004616c6578064500146e6f746966792d726563697069656e742d7572690007646275733a2f2f44000d6e6f746966792d6576656e7473000b6a6f622d6372656174656444000000116a6f622d73746174652d6368616e6765642100156e6f746966792d6c656173652d6475726174696f6e00040000003c03";
const CREATED: &str = "020000000000000101470012617474726962757465732d6368617273657400057574662d3848001b617474726962757465732d6e61747572616c2d6c616e67756167650002656e0644000d6e6f746966792d6576656e747300116a6f622d73746174652d6368616e6765642100156e6f746966792d6c656173652d6475726174696f6e00040000003c4500146e6f746966792d726563697069656e742d7572690007646275733a2f2f42001b6e6f746966792d737562736372696265722d757365722d6e616d650004616c65782100146e6f746966792d74696d652d696e74657276616c0004000000002100166e6f746966792d737562736372697074696f6e2d696400040000006903";
const GONE: &str = "020004060000000201470012617474726962757465732d6368617273657400057574662d3848001b617474726962757465732d6e61747572616c2d6c616e67756167650002656e41000e7374617475732d6d6573736167650024537562736372697074696f6e202339393939393920646f6573206e6f742065786973742e03";

fn unhex(text: &str) -> Vec<u8> {
    text.as_bytes()
        .chunks_exact(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).expect("hex"), 16).expect("byte"))
        .collect()
}

#[test]
fn create_matches_the_captured_wire_request() {
    let uri = "http://localhost/".parse().expect("uri");
    assert_eq!(
        request(
            &uri,
            0x0016,
            None,
            "alex",
            &["job-created", "job-state-changed"],
            60
        )
        .expect("request"),
        unhex(CREATE)
    );
}

#[test]
fn captured_create_and_missing_subscription_replies_are_parsed() {
    assert_eq!(parse(&unhex(CREATED)), Ok((0, Some(105))));
    assert_eq!(parse(&unhex(GONE)), Ok((0x0406, None)));
    assert!(accept(0x0406).is_err());
}

#[test]
fn every_truncation_of_a_captured_reply_is_rejected() {
    let reply = unhex(CREATED);
    for end in 0..reply.len() {
        assert!(parse(&reply[..end]).is_err(), "truncation at {end}");
    }
}

#[test]
fn subscription_ids_require_the_correct_group_type_length_and_positive_value() {
    for (group, tag, value, valid) in [
        (6, 0x21, vec![0, 0, 0, 105], true),
        (1, 0x21, vec![0, 0, 0, 105], false),
        (6, 0x23, vec![0, 0, 0, 105], false),
        (6, 0x21, vec![105], false),
        (6, 0x21, vec![0, 0, 0, 0], false),
        (6, 0x21, vec![255; 4], false),
    ] {
        let mut reply = vec![2, 0, 0, 0, 0, 0, 0, 1, group];
        attribute(&mut reply, tag, "notify-subscription-id", &value).expect("attribute");
        reply.push(3);
        assert_eq!(parse(&reply).ok().and_then(|(_, id)| id).is_some(), valid);
    }
    assert_eq!(parse(&[2, 0, 0, 0, 0, 0, 0, 1, 3]), Ok((0, None)));
}
