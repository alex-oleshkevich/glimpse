use super::service::*;
use super::*;

fn profile(ssid: &str, timestamp: u64) -> nm::Profile {
    nm::Profile {
        id: Some(ssid.to_owned()),
        uuid: Some(format!("uuid-{ssid}-{timestamp}")),
        kind: Some("802-11-wireless".to_owned()),
        ssid: Some(ssid.to_owned()),
        timestamp,
        autoconnect: true,
        ..nm::Profile::default()
    }
}

#[tokio::test]
async fn the_most_recently_used_profile_wins_for_a_shared_ssid() {
    let mut harness = harness().await;
    enumerate(&mut harness, measured_objects()).await;

    harness.service.profiles.insert(
        "/org/freedesktop/NetworkManager/Settings/1".to_owned(),
        profile("Skylink", 100),
    );
    harness.service.profiles.insert(
        "/org/freedesktop/NetworkManager/Settings/9".to_owned(),
        profile("Skylink", 900),
    );

    assert_eq!(
        harness
            .service
            .profile_matching(Some("Skylink"), None)
            .map(|id| id.as_str().to_owned()),
        Some("/org/freedesktop/NetworkManager/Settings/9".to_owned()),
        "the newer timestamp is the one the user last actually used"
    );
}

fn beacon(ssid: &str, rsn: u32, bssid: &str) -> nm::AccessPointProperties {
    nm::AccessPointProperties {
        ssid: Some(ssid.to_owned()),
        raw_ssid: ssid.as_bytes().to_vec(),
        bssid: Some(bssid.to_owned()),
        flags: 1,
        rsn_flags: rsn,
        ..Default::default()
    }
}

#[tokio::test]
async fn a_profile_that_cannot_join_the_beacon_is_never_offered_for_it() {
    let mut harness = harness().await;
    enumerate(&mut harness, measured_objects()).await;

    let mut open = profile("CorpNet", 900);
    open.key_mgmt = None;
    let mut secured = profile("CorpNet", 100);
    secured.key_mgmt = Some("wpa-psk".to_owned());
    harness.service.profiles.insert("/s/open".to_owned(), open);
    harness
        .service
        .profiles
        .insert("/s/secured".to_owned(), secured);

    let point = beacon("CorpNet", 392, "00:11:22:33:44:55");
    assert_eq!(
        harness
            .service
            .profile_matching(Some("CorpNet"), Some(&point))
            .map(|id| id.as_str().to_owned()),
        Some("/s/secured".to_owned()),
        "the newer open profile cannot join a WPA2 beacon, so choosing it by timestamp alone \
         fails the join without ever asking for a password"
    );
}

#[tokio::test]
async fn a_profile_that_has_seen_this_beacon_outranks_a_newer_one_that_has_not() {
    let mut harness = harness().await;
    enumerate(&mut harness, measured_objects()).await;

    let mut seen = profile("Skylink", 100);
    seen.key_mgmt = Some("wpa-psk".to_owned());
    seen.seen_bssids = vec!["00:11:22:33:44:55".to_owned()];
    let mut newer = profile("Skylink", 900);
    newer.key_mgmt = Some("wpa-psk".to_owned());
    harness.service.profiles.insert("/s/seen".to_owned(), seen);
    harness
        .service
        .profiles
        .insert("/s/newer".to_owned(), newer);

    let point = beacon("Skylink", 392, "00:11:22:33:44:55");
    assert_eq!(
        harness
            .service
            .profile_matching(Some("Skylink"), Some(&point))
            .map(|id| id.as_str().to_owned()),
        Some("/s/seen".to_owned())
    );
}

#[tokio::test]
async fn a_tie_on_timestamp_is_broken_by_path_so_the_answer_never_moves() {
    let mut harness = harness().await;
    enumerate(&mut harness, measured_objects()).await;

    for path in ["/s/2", "/s/1", "/s/3"] {
        harness
            .service
            .profiles
            .insert(path.to_owned(), profile("Skylink", 0));
    }

    let first = harness.service.profile_matching(Some("Skylink"), None);
    for _ in 0..5 {
        assert_eq!(
            harness.service.profile_matching(Some("Skylink"), None),
            first,
            "an unchanged set must answer the same way every time"
        );
    }
    assert_eq!(
        first.map(|id| id.as_str().to_owned()),
        Some("/s/1".to_owned()),
        "the lowest path breaks the tie"
    );
}

async fn autoconnect(harness: &mut Harness, id: &str) -> Result<(), NetworkError> {
    let (reply, result) = tokio::sync::oneshot::channel();
    harness
        .service
        .handle(
            &harness.ctx,
            Input::Command(Command::SetAutoconnect {
                id: NetworkId::new(id),
                autoconnect: false,
                reply,
            }),
        )
        .await;
    tokio::time::timeout(std::time::Duration::from_secs(1), result)
        .await
        .expect("the reply arrives rather than hanging")
        .expect("the sender was not dropped")
}

#[tokio::test]
async fn a_profile_setting_reached_from_an_access_point_row_resolves_to_the_profile() {
    let mut harness = harness().await;
    enumerate(&mut harness, measured_objects()).await;
    harness
        .service
        .profiles
        .insert("/s/1".to_owned(), profile("Skylink", 10));
    harness.service.access_points.insert(
        "/org/freedesktop/NetworkManager/AccessPoint/1".to_owned(),
        nm::AccessPointProperties {
            ssid: Some("Skylink".to_owned()),
            ..nm::AccessPointProperties::default()
        },
    );
    harness.service.access_points.insert(
        "/org/freedesktop/NetworkManager/AccessPoint/2".to_owned(),
        nm::AccessPointProperties {
            ssid: Some("PLAY internet".to_owned()),
            ..nm::AccessPointProperties::default()
        },
    );

    assert_eq!(
        harness.service.profile_of(&NetworkId::new(
            "/org/freedesktop/NetworkManager/AccessPoint/1"
        )),
        Some("/s/1".to_owned()),
        "the access point resolves to the profile saved for its ssid"
    );
    assert!(
        matches!(
            autoconnect(
                &mut harness,
                "/org/freedesktop/NetworkManager/AccessPoint/1"
            )
            .await,
            Err(NetworkError::Unavailable(_))
        ),
        "the detail drawer of a networks row carries the access point path, and \
         Settings.Connection does not exist on one - the saved profile behind it is \
         what the call has to name"
    );
    assert!(
        matches!(
            autoconnect(&mut harness, "/s/1").await,
            Err(NetworkError::Unavailable(_))
        ),
        "a known-networks row already carries the profile path"
    );
    assert!(
        matches!(
            autoconnect(
                &mut harness,
                "/org/freedesktop/NetworkManager/AccessPoint/2"
            )
            .await,
            Err(NetworkError::Failed(Failure::NotFound))
        ),
        "an access point nothing is saved for has no profile to write"
    );
}

#[tokio::test]
async fn a_network_with_no_saved_profile_has_none() {
    let mut harness = harness().await;
    enumerate(&mut harness, measured_objects()).await;
    harness
        .service
        .profiles
        .insert("/s/1".to_owned(), profile("Skylink", 10));

    assert_eq!(
        harness
            .service
            .profile_matching(Some("PLAY internet"), None),
        None
    );
    assert_eq!(
        harness.service.profile_matching(None, None),
        None,
        "an unnamed network cannot match a profile by name"
    );
}
