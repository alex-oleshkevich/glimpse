use std::path::PathBuf;

use gettextrs::{gettext, ngettext};
use glimpse_services::{Place, PlacesState};
use glimpse_widgets::{PlacesEntry, PlacesTrash};

pub const ICON: &str = "folder-symbolic";
const TRASH_ID: &str = "trash";
const NAME_CAP: usize = 60;

fn cap(name: &str) -> String {
    glimpse_utils::clean(name, NAME_CAP)
}

pub fn chip(places: &PlacesState) -> bool {
    !places.places.is_empty()
        || !places.bookmarks.is_empty()
        || !places.network.is_empty()
        || places.trash.is_some()
}

pub fn tooltip(places: &PlacesState, format: Option<&str>) -> String {
    let Some(format) = format else {
        return gettext("Places");
    };
    let bookmarks = places.bookmarks.len().to_string();
    let places_count = places.places.len().to_string();
    crate::applets::tokens::render(format, |token| match token {
        "bookmarks" => Some(bookmarks.as_str()),
        "places" => Some(places_count.as_str()),
        _ => None,
    })
}

pub struct Listing {
    pub entries: Vec<PlacesEntry>,
    pub more: Option<String>,
}

fn shown(cap: usize, expanded: bool) -> usize {
    match expanded {
        true => usize::MAX,
        false => cap,
    }
}

fn more(total: usize, cap: usize, expanded: bool) -> Option<String> {
    let hidden = total.saturating_sub(cap);
    if hidden == 0 {
        return None;
    }
    Some(match expanded {
        true => gettext("Show fewer"),
        false => ngettext("{count} more", "{count} more", hidden as u32)
            .replace("{count}", &hidden.to_string()),
    })
}

fn place_entry(place: &Place) -> PlacesEntry {
    PlacesEntry {
        id: place.id.clone(),
        title: cap(&place.name),
        subtitle: String::new(),
        icon: place.kind.icon_name().to_owned(),
        busy: false,
    }
}

pub fn places(state: &PlacesState) -> Vec<PlacesEntry> {
    state.places.iter().map(place_entry).collect()
}

pub fn bookmarks(state: &PlacesState, cap: usize, expanded: bool) -> Listing {
    let entries = state
        .bookmarks
        .iter()
        .take(shown(cap, expanded))
        .map(place_entry)
        .collect();
    Listing {
        entries,
        more: more(state.bookmarks.len(), cap, expanded),
    }
}

pub fn network(state: &PlacesState) -> Vec<PlacesEntry> {
    state
        .network
        .iter()
        .map(|place| PlacesEntry {
            id: place.id.clone(),
            title: cap(&place.name),
            subtitle: place.path.display().to_string(),
            icon: place.kind.icon_name().to_owned(),
            busy: false,
        })
        .collect()
}

pub fn trash(state: &PlacesState) -> Option<PlacesTrash> {
    state.trash.map(|items| PlacesTrash {
        items: items as u32,
    })
}

fn place_path(state: &PlacesState, id: &str) -> Option<PathBuf> {
    state
        .places
        .iter()
        .chain(&state.bookmarks)
        .chain(&state.network)
        .find(|place| place.id == id)
        .map(|place| place.path.clone())
}

pub enum Activation {
    Open(PathBuf),
    OpenUri(&'static str),
}

pub fn activation(places: &PlacesState, id: &str) -> Option<Activation> {
    if id == TRASH_ID {
        return Some(Activation::OpenUri("trash:///"));
    }
    place_path(places, id).map(Activation::Open)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn place(id: &str, name: &str, kind: glimpse_services::PlacesKind, path: &str) -> Place {
        Place {
            id: id.to_owned(),
            name: name.to_owned(),
            path: PathBuf::from(path),
            kind,
        }
    }

    #[test]
    fn nothing_read_yet_shows_no_chip_and_a_known_empty_state_does() {
        assert!(!chip(&PlacesState::default()));

        let known = PlacesState {
            trash: Some(0),
            ..Default::default()
        };
        assert!(chip(&known));
    }

    #[test]
    fn a_hostile_bookmark_label_is_capped_before_it_reaches_a_dto() {
        let long: String = std::iter::repeat_n('\u{e9}', 400).collect();
        let mut state = PlacesState::default();
        state.bookmarks.push(place(
            "bookmark",
            &long,
            glimpse_services::PlacesKind::Bookmark,
            "/home/alex/projects",
        ));

        let listing = bookmarks(&state, 8, false);

        assert_eq!(listing.entries.len(), 1);
        assert!(
            listing.entries[0].title.chars().count() <= NAME_CAP + 1,
            "a multibyte label must be capped by character count"
        );
    }

    #[test]
    fn bookmarks_past_the_cap_go_behind_an_overflow_row() {
        let mut state = PlacesState::default();
        for index in 0..10 {
            state.bookmarks.push(place(
                &format!("b{index}"),
                &format!("bookmark{index}"),
                glimpse_services::PlacesKind::Bookmark,
                "/home/alex",
            ));
        }

        let capped = bookmarks(&state, 8, false);
        assert_eq!(capped.entries.len(), 8);
        assert!(capped.more.is_some());

        let opened = bookmarks(&state, 8, true);
        assert_eq!(opened.entries.len(), 10);
        assert_eq!(opened.more.as_deref(), Some("Show fewer"));
    }

    #[test]
    fn activating_a_place_opens_its_path() {
        let mut places = PlacesState::default();
        places.places.push(place(
            "home",
            "Home",
            glimpse_services::PlacesKind::Home,
            "/home/alex",
        ));

        match activation(&places, "home") {
            Some(Activation::Open(path)) => assert_eq!(path, PathBuf::from("/home/alex")),
            _ => panic!("expected an open request"),
        }
    }

    #[test]
    fn activating_trash_opens_the_trash_uri() {
        match activation(&PlacesState::default(), TRASH_ID) {
            Some(Activation::OpenUri(uri)) => assert_eq!(uri, "trash:///"),
            _ => panic!("expected an open request"),
        }
    }
}
