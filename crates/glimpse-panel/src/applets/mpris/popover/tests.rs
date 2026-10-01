use super::*;

fn player(id: &str, entry: Option<&str>) -> PlayerStatus {
    PlayerStatus {
        id: id.to_owned(),
        identity: id.to_owned(),
        desktop_entry: entry.map(str::to_owned),
        title: None,
        artist: None,
        album: None,
        length_us: None,
        ..super::super::render::tests::player()
    }
}

#[test]
fn a_desktop_entry_is_tried_before_anything_guessed_from_the_bus_name() {
    assert_eq!(
        candidates(&player("chromium.instance4181", Some("chromium-browser"))),
        ["chromium-browser", "chromium.instance4181", "chromium"]
    );
}

#[test]
fn an_instance_number_names_no_icon_and_is_not_offered() {
    assert_eq!(
        candidates(&player("chromium.instance4181", None)),
        ["chromium.instance4181", "chromium"]
    );
}

#[test]
fn a_reverse_dns_bus_name_offers_its_last_segment_too() {
    assert_eq!(
        candidates(&player("org.gnome.Rhythmbox3", None)),
        ["org.gnome.Rhythmbox3", "org", "gnome", "Rhythmbox3"]
    );
}

#[test]
fn a_plain_id_offers_itself_once() {
    assert_eq!(candidates(&player("spotify", None)), ["spotify"]);
}

#[test]
fn the_current_player_is_never_repeated_in_the_others_list() {
    let mut current = player("spotify", None);
    current.current = true;

    let rows = rows(&[current, player("mpv", None)], &HashMap::new());

    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].name, "mpv");
    assert_eq!(
        rows[0].key, "mpv",
        "a row carries the id a command is addressed to, not its position"
    );
}
