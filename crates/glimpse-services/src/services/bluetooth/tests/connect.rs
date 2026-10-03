use futures_util::FutureExt;

use super::*;

#[test]
fn a_transport_below_the_device_proves_an_audio_profile() {
    assert!(call::proves_audio_profile(
        "/org/bluez/hci0/dev_X",
        "/org/bluez/hci0/dev_X/sep6/fd0",
        [bluez::MEDIA_TRANSPORT1],
    ));
}

#[test]
fn the_device_itself_is_not_audio_profile_proof() {
    assert!(!call::proves_audio_profile(
        "/org/bluez/hci0/dev_X",
        "/org/bluez/hci0/dev_X",
        [bluez::MEDIA_TRANSPORT1],
    ));
}

#[test]
fn a_device_sharing_the_prefix_is_not_audio_profile_proof() {
    assert!(!call::proves_audio_profile(
        "/org/bluez/hci0/dev_X",
        "/org/bluez/hci0/dev_XY/sep1/fd0",
        [bluez::MEDIA_TRANSPORT1],
    ));
}

#[test]
fn a_child_without_media_transport_is_not_audio_profile_proof() {
    assert!(!call::proves_audio_profile(
        "/org/bluez/hci0/dev_X",
        "/org/bluez/hci0/dev_X/sep6/fd0",
        [bluez::DEVICE1, "org.bluez.MediaControl1"],
    ));
}

fn failed(token: &str) -> zbus::Error {
    zbus::Error::MethodError(
        zbus::names::OwnedErrorName::try_from("org.bluez.Error.Failed")
            .expect("a well-formed error name"),
        Some(token.to_owned()),
        zbus::message::Message::method_call("/org/bluez", "Connect")
            .expect("a call")
            .build(&())
            .expect("a message"),
    )
}

#[tokio::test]
async fn a_non_busy_failure_is_returned_untouched_without_waiting() {
    let error = failed("br-connection-page-timeout");
    let original = error.clone();
    let outcome = call::finish_connect(Err(error), std::future::pending())
        .now_or_never()
        .expect("a non-Busy failure returns immediately");
    assert_eq!(outcome, Err(original));
}

#[tokio::test]
async fn proof_turns_a_busy_connect_into_success() {
    let outcome = call::finish_connect(
        Err(failed("br-connection-create-socket")),
        std::future::ready(Ok(true)),
    )
    .await;
    assert_eq!(outcome, Ok(()));
}

#[tokio::test]
async fn absent_proof_keeps_the_original_busy_failure() {
    let error = failed("br-connection-create-socket");
    let original = error.clone();
    let outcome = call::finish_connect(Err(error), std::future::ready(Ok(false))).await;
    assert_eq!(outcome, Err(original));
}

#[tokio::test]
async fn an_observation_error_keeps_the_original_busy_failure() {
    let error = failed("br-connection-create-socket");
    let original = error.clone();
    let outcome = call::finish_connect(
        Err(error),
        std::future::ready(Err(zbus::Error::InvalidField)),
    )
    .await;
    assert_eq!(outcome, Err(original));
}

#[tokio::test]
async fn a_busy_connect_without_proof_expires_after_five_seconds() {
    let error = failed("br-connection-create-socket");
    let original = error.clone();
    let start = tokio::time::Instant::now();
    let outcome = tokio::time::timeout(
        std::time::Duration::from_secs(6),
        call::finish_connect(Err(error), std::future::pending()),
    )
    .await
    .expect("the proof wait has a five-second deadline");
    assert_eq!(outcome, Err(original));
    assert!(start.elapsed() >= std::time::Duration::from_secs(5));
}
