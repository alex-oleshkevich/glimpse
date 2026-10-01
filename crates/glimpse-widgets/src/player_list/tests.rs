use super::{Player, byline};

fn player(artist: &str, name: &str) -> Player {
    Player {
        artist: artist.to_owned(),
        name: name.to_owned(),
        ..Default::default()
    }
}

#[test]
fn a_byline_joins_what_it_has_and_omits_what_it_does_not() {
    assert_eq!(
        byline(&player("Boards of Canada", "Spotify")).as_deref(),
        Some("Boards of Canada · Spotify")
    );
    assert_eq!(
        byline(&player("", "Firefox")).as_deref(),
        Some("Firefox"),
        "a stream with no artist still says which application is playing it"
    );
    assert_eq!(
        byline(&player("Boards of Canada", "")).as_deref(),
        Some("Boards of Canada")
    );
    assert_eq!(
        byline(&player("", "")),
        None,
        "and a player that says nothing gets no second line rather than a bare separator"
    );
}
