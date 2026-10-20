use std::path::PathBuf;

use gettextrs::{gettext, ngettext};
use glimpse_services::{Place, PlacesState};
use glimpse_widgets::{PlacesEntry, PlacesTrash};

pub const ICON: &str = "folder-symbolic";
const TRASH_ID: &str = "trash";
const NAME_CAP: usize = 60;

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
    let title = match place.kind {
        glimpse_services::PlacesKind::Home => gettext("Home"),
        _ => glimpse_utils::clean(&place.name, NAME_CAP),
    };
    PlacesEntry {
        id: place.id.clone(),
        title,
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
        .map(|place| {
            let (title, subtitle) = share(&place.id);
            PlacesEntry {
                id: place.id.clone(),
                title: glimpse_utils::clean(&title, NAME_CAP),
                subtitle: glimpse_utils::clean(&subtitle, NAME_CAP),
                icon: place.kind.icon_name().to_owned(),
                busy: false,
            }
        })
        .collect()
}

fn share(mount: &str) -> (String, String) {
    let Some((scheme, params)) = mount.split_once(':') else {
        return (mount.to_owned(), String::new());
    };
    let param = |key: &str| {
        params
            .split(',')
            .filter_map(|pair| pair.split_once('='))
            .find(|(name, _)| *name == key)
            .map(|(_, value)| {
                gtk4::glib::uri_unescape_string(value, None::<&str>)
                    .map_or_else(|| value.to_owned(), |text| text.to_string())
            })
            .filter(|value| !value.is_empty())
    };
    let host = param("server").or_else(|| param("host"));
    let folder = param("share")
        .or_else(|| param("prefix").map(|prefix| prefix.trim_matches('/').to_owned()));
    let protocol = scheme.trim_end_matches("-share").to_uppercase();
    match (folder.filter(|folder| !folder.is_empty()), host) {
        (Some(folder), Some(host)) => (folder, format!("{host} · {protocol}")),
        (None, Some(host)) => (host, protocol),
        (_, None) => (mount.to_owned(), String::new()),
    }
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
mod tests;
