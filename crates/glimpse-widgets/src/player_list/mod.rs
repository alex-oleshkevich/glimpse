mod imp;
mod row;

pub use row::PlayerRow;

use gtk4::{glib, prelude::*, subclass::prelude::*};

use crate::{Row, none_if_empty};

const JOIN: &str = " · ";

/// `key` is what a row reports when it is activated. Rows are reused in place as the list changes,
/// so a position says nothing durable about which player it stands for, and a caller resolving one
/// against its own copy of the list has to keep two orderings in step.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Player {
    pub key: String,
    pub name: String,
    pub icon_name: String,
    pub title: String,
    pub artist: String,
    pub playing: bool,
}

glib::wrapper! {
    pub struct PlayerList(ObjectSubclass<imp::PlayerList>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Default for PlayerList {
    fn default() -> Self {
        Self::new()
    }
}

impl PlayerList {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn set_players(&self, players: &[Player]) {
        let imp = self.imp();
        if imp.players.borrow().as_slice() == players {
            return;
        }
        imp.players.replace(players.to_vec());
        self.render();
    }

    pub fn connect_activated<F: Fn(&Self, String) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            "activated",
            false,
            glib::closure_local!(move |list: Self, key: String| f(&list, key)),
        )
    }

    pub fn connect_toggled<F: Fn(&Self, String) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            "toggled",
            false,
            glib::closure_local!(move |list: Self, key: String| f(&list, key)),
        )
    }

    fn render(&self) {
        let imp = self.imp();
        let players = imp.players.borrow();
        let mut rows = imp.rows.borrow_mut();

        for (index, player) in players.iter().enumerate() {
            if rows.len() == index {
                let row = self.build_row(index as u32);
                row.insert_after(self, rows.last());
                rows.push(row);
            }
            let row = &rows[index];
            let item: &Row = row.upcast_ref();
            item.set_title(none_if_empty(&player.title));
            item.set_subtitle(byline(player).as_deref());
            item.set_lead_icon(none_if_empty(&player.icon_name));
            row.set_playing(player.playing);
        }

        for row in rows.split_off(players.len()) {
            row.unparent();
        }
    }

    /// The row's key is read back at the moment it fires, not captured when it was built: `render`
    /// reuses a row in place, so a key captured here would name whichever player happened to hold
    /// that position first.
    fn build_row(&self, index: u32) -> PlayerRow {
        let row = PlayerRow::new();
        row.connect_clicked(glib::clone!(
            #[weak(rename_to = list)]
            self,
            move |_| list.report("activated", index)
        ));
        row.connect_toggled(glib::clone!(
            #[weak(rename_to = list)]
            self,
            move |_| list.report("toggled", index)
        ));
        row
    }

    fn report(&self, signal: &str, index: u32) {
        let key = self
            .imp()
            .players
            .borrow()
            .get(index as usize)
            .map(|player| player.key.clone());

        if let Some(key) = key {
            self.emit_by_name::<()>(signal, &[&key]);
        }
    }
}

fn byline(player: &Player) -> Option<String> {
    let parts: Vec<&str> = [player.artist.as_str(), player.name.as_str()]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect();
    (!parts.is_empty()).then(|| parts.join(JOIN))
}

#[cfg(test)]
mod tests {
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
}
