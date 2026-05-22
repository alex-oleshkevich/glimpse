use std::cell::Cell;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use relm4::{
    ComponentParts, ComponentSender, SimpleComponent,
    gtk::{self, gdk, gio, prelude::*},
};

use crate::{
    applets::mpris::format,
    components::{popover_scroll, popover_shell::PopoverShell, section_header::SectionHeader},
    services::mpris::{Artwork, PlaybackStatus, Player, State, model::visible_players},
    widgets::{
        animated_popover::AnimatedPopover, media_transport::PlayState,
        now_playing_card::NowPlayingCard, secondary_player_row::SecondaryPlayerRow,
    },
};

#[derive(Default)]
struct CardPlayer {
    player_id: Option<String>,
    position_micros: i64,
}

pub struct Popover {
    popover: AnimatedPopover,
    card: NowPlayingCard,
    card_player: Rc<RefCell<CardPlayer>>,
    rows_box: gtk::Box,
    rows: HashMap<String, SecondaryPlayerRow>,
    max_rows: usize,
    show_artwork: bool,
    state: State,
    other_visible: bool,
}

pub struct PopoverInit {
    pub parent: gtk::Box,
    pub max_rows: usize,
    pub show_artwork: bool,
}

#[derive(Debug)]
pub enum PopoverInput {
    Toggle,
    Update(State),
    Reconfigure { max_rows: usize, show_artwork: bool },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PopoverOutput {
    Previous { player_id: String },
    PlayPause { player_id: String },
    Next { player_id: String },
    Seek { player_id: String, offset_micros: i64 },
    Raise { player_id: String },
}

#[allow(unused_assignments)]
#[relm4::component(pub)]
impl SimpleComponent for Popover {
    type Init = PopoverInit;
    type Input = PopoverInput;
    type Output = PopoverOutput;

    view! {
        root = AnimatedPopover {
            add_css_class: "mpris-popover",
            set_hexpand: false,
            set_autohide: true,

            #[template]
            PopoverShell {
                #[template_child]
                footer {
                    set_visible: false,
                },

                #[template_child]
                content {
                    #[local_ref]
                    card_widget -> NowPlayingCard {},

                    #[name = "other_section"]
                    gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,
                        set_spacing: 6,
                        add_css_class: "mpris-other",

                        #[name = "other_header"]
                        #[template]
                        SectionHeader {},

                        #[name = "scroller"]
                        gtk::ScrolledWindow {
                            set_policy: (gtk::PolicyType::Never, gtk::PolicyType::Automatic),
                            set_vexpand: false,
                            set_propagate_natural_height: true,

                            #[local_ref]
                            rows_widget -> gtk::Box {
                                set_orientation: gtk::Orientation::Vertical,
                                set_spacing: 6,
                            },
                        },
                    },

                    #[name = "empty_state"]
                    gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,
                        add_css_class: "empty-state",

                        gtk::Label {
                            add_css_class: "empty-state__title",
                            set_label: "No media playing",
                        },

                        gtk::Label {
                            add_css_class: "empty-state__subtitle",
                            set_label: "Start playback in any MPRIS-compatible player",
                        },
                    },
                },
            },
        }
    }

    fn init(
        init: Self::Init,
        _root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let card = NowPlayingCard::new();
        let card_widget = card.clone();
        let rows_box = gtk::Box::new(gtk::Orientation::Vertical, 6);
        let rows_widget = rows_box.clone();
        let card_player: Rc<RefCell<CardPlayer>> = Rc::new(RefCell::new(CardPlayer::default()));

        let mut model = Popover {
            popover: AnimatedPopover::new(),
            card,
            card_player,
            rows_box,
            rows: HashMap::new(),
            max_rows: init.max_rows,
            show_artwork: init.show_artwork,
            state: State::default(),
            other_visible: false,
        };

        let widgets = view_output!();
        model.popover = widgets.root.clone();
        widgets.root.set_parent(&init.parent);
        widgets.other_header.title.set_label("Other players");
        popover_scroll::install_half_monitor_limit(
            widgets.root.upcast_ref::<gtk::Popover>(),
            &widgets.scroller,
            &init.parent,
        );

        wire_card(&model.card, model.card_player.clone(), sender);

        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>) {
        match message {
            PopoverInput::Toggle => self.popover.toggle(),
            PopoverInput::Update(state) => self.sync_state(state, &sender),
            PopoverInput::Reconfigure {
                max_rows,
                show_artwork,
            } => {
                self.max_rows = max_rows;
                self.show_artwork = show_artwork;
                let state = self.state.clone();
                self.sync_state(state, &sender);
            }
        }
    }

    fn post_view() {
        let has_card = model.card_player.borrow().player_id.is_some();
        model.card.set_visible(has_card);
        other_section.set_visible(model.other_visible);
        empty_state.set_visible(!has_card && !model.other_visible);
    }
}

impl Popover {
    fn sync_state(&mut self, state: State, sender: &ComponentSender<Self>) {
        let visible = visible_players(&state.snapshot.players);
        let current = state
            .snapshot
            .current_player
            .clone()
            .filter(|player| player.playback_status != PlaybackStatus::Stopped)
            .or_else(|| visible.first().cloned());

        let others: Vec<Player> = visible
            .into_iter()
            .filter(|player| {
                current
                    .as_ref()
                    .is_none_or(|c| c.player_id != player.player_id)
            })
            .take(self.max_rows)
            .collect();

        match current.as_ref() {
            Some(player) => {
                self.card_player.replace(CardPlayer {
                    player_id: Some(player.player_id.clone()),
                    position_micros: player.position.unwrap_or(0) as i64,
                });
                apply_player_to_card(&self.card, player, self.show_artwork);
            }
            None => {
                self.card_player.replace(CardPlayer::default());
            }
        }

        self.sync_rows(&others, sender);
        self.state = state;
    }

    fn sync_rows(&mut self, players: &[Player], sender: &ComponentSender<Self>) {
        while let Some(child) = self.rows_box.first_child() {
            self.rows_box.remove(&child);
        }
        let mut next: HashMap<String, SecondaryPlayerRow> = HashMap::new();
        for player in players {
            let row = self
                .rows
                .remove(&player.player_id)
                .unwrap_or_else(|| build_row(player.player_id.clone(), sender.clone()));
            apply_player_to_row(&row, player, self.show_artwork);
            self.rows_box.append(&row);
            next.insert(player.player_id.clone(), row);
        }
        self.rows = next;
        self.other_visible = !players.is_empty();
    }
}

fn wire_card(
    card: &NowPlayingCard,
    card_player: Rc<RefCell<CardPlayer>>,
    sender: ComponentSender<Popover>,
) {
    let id_emitter = |make: fn(String) -> PopoverOutput| {
        let card_player = card_player.clone();
        let sender = sender.clone();
        move || {
            if let Some(id) = card_player.borrow().player_id.clone() {
                let _ = sender.output(make(id));
            }
        }
    };

    let transport = card.transport();
    let prev = id_emitter(|player_id| PopoverOutput::Previous { player_id });
    transport.connect_previous(move |_| prev());
    let play_pause = id_emitter(|player_id| PopoverOutput::PlayPause { player_id });
    transport.connect_play_pause(move |_| play_pause());
    let next = id_emitter(|player_id| PopoverOutput::Next { player_id });
    transport.connect_next(move |_| next());
    let artwork = id_emitter(|player_id| PopoverOutput::Raise { player_id });
    card.artwork().connect_activated(move |_| artwork());
    let meta = id_emitter(|player_id| PopoverOutput::Raise { player_id });
    card.meta().connect_activated(move |_| meta());

    let pending_seek: Rc<Cell<Option<i64>>> = Rc::new(Cell::new(None));
    card.scrubber().connect_seek_requested(move |_, seconds| {
        let player = card_player.borrow();
        let Some(id) = player.player_id.clone() else {
            return;
        };
        let target_micros = (seconds * 1_000_000.0) as i64;
        let offset_micros = target_micros - player.position_micros;
        // Coalesce rapid drags by buffering the most-recent offset; the timeout
        // 120 ms later picks it up. Later ticks overwrite the pending value, so
        // we only ever emit one seek per gesture.
        pending_seek.set(Some(offset_micros));
        let sender = sender.clone();
        let pending = pending_seek.clone();
        glib::timeout_add_local_once(std::time::Duration::from_millis(120), move || {
            let Some(offset) = pending.take() else {
                return;
            };
            let _ = sender.output(PopoverOutput::Seek {
                player_id: id,
                offset_micros: offset,
            });
        });
    });
}

fn build_row(player_id: String, sender: ComponentSender<Popover>) -> SecondaryPlayerRow {
    let row = SecondaryPlayerRow::new();
    let emit = |make: fn(String) -> PopoverOutput| {
        let player_id = player_id.clone();
        let sender = sender.clone();
        move || {
            let _ = sender.output(make(player_id.clone()));
        }
    };

    let play_pause = emit(|player_id| PopoverOutput::PlayPause { player_id });
    row.connect_play_pause(move |_| play_pause());
    let next = emit(|player_id| PopoverOutput::Next { player_id });
    row.connect_next(move |_| next());
    let activated = emit(|player_id| PopoverOutput::Raise { player_id });
    row.connect_activated(move |_| activated());
    row
}

fn apply_player_to_card(card: &NowPlayingCard, player: &Player, show_artwork: bool) {
    card.set_title(&format::title(player));
    card.set_subtitle(&format::subtitle(player));
    card.set_artwork(load_texture(player, show_artwork).as_ref());

    let position_s = player.position.map(micros_to_seconds).unwrap_or(0.0);
    let length_s = player.length.map(micros_to_seconds).unwrap_or(0.0);
    card.scrubber().set_progress(position_s, length_s);
    card.scrubber().set_seekable(player.can_seek);

    card.times()
        .set_position_text(&player.position.map(format::duration).unwrap_or_default());
    card.times()
        .set_length_text(&player.length.map(format::duration).unwrap_or_default());

    card.transport()
        .set_play_state(play_state(player.playback_status));
    card.transport().set_can_previous(player.can_go_previous);
    card.transport().set_can_play_pause(player.can_play_pause);
    card.transport().set_can_next(player.can_go_next);
}

fn apply_player_to_row(row: &SecondaryPlayerRow, player: &Player, show_artwork: bool) {
    row.set_title(&format::title(player));
    row.set_subtitle(&format::subtitle(player));
    row.set_artwork(load_texture(player, show_artwork).as_ref());
    row.set_play_state(play_state(player.playback_status));
    row.set_can_play_pause(player.can_play_pause);
    row.set_can_next(player.can_go_next);
}

fn play_state(status: PlaybackStatus) -> PlayState {
    match status {
        PlaybackStatus::Playing => PlayState::Playing,
        PlaybackStatus::Paused | PlaybackStatus::Stopped => PlayState::Paused,
    }
}

fn micros_to_seconds(value: u64) -> f64 {
    value as f64 / 1_000_000.0
}

fn load_texture(player: &Player, show_artwork: bool) -> Option<gdk::Texture> {
    if !show_artwork {
        return None;
    }
    match &player.artwork {
        Artwork::FilePath(path) => gdk::Texture::from_filename(path).ok(),
        Artwork::FileUri(uri) => gio::File::for_uri(uri)
            .path()
            .and_then(|path| gdk::Texture::from_filename(path).ok()),
        Artwork::RemoteUrl(_) | Artwork::None => None,
    }
}
