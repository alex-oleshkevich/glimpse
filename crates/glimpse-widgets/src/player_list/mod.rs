mod imp;
mod row;

pub use row::PlayerRow;

use gtk4::{glib, prelude::*, subclass::prelude::*};

use crate::Row;

const JOIN: &str = " · ";

#[derive(Debug, Default, Clone, PartialEq)]
pub struct Player {
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

    pub fn connect_activated<F: Fn(&Self, u32) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            "activated",
            false,
            glib::closure_local!(move |list: Self, index: u32| f(&list, index)),
        )
    }

    pub fn connect_toggled<F: Fn(&Self, u32) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            "toggled",
            false,
            glib::closure_local!(move |list: Self, index: u32| f(&list, index)),
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

    fn build_row(&self, index: u32) -> PlayerRow {
        let row = PlayerRow::new();
        row.connect_clicked(glib::clone!(
            #[weak(rename_to = list)]
            self,
            move |_| list.emit_by_name::<()>("activated", &[&index])
        ));
        row.connect_toggled(glib::clone!(
            #[weak(rename_to = list)]
            self,
            move |_| list.emit_by_name::<()>("toggled", &[&index])
        ));
        row
    }
}

fn byline(player: &Player) -> Option<String> {
    let parts: Vec<&str> = [player.artist.as_str(), player.name.as_str()]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect();
    (!parts.is_empty()).then(|| parts.join(JOIN))
}

fn none_if_empty(text: &str) -> Option<&str> {
    (!text.is_empty()).then_some(text)
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
