use std::cell::RefCell;
use std::rc::Rc;

use relm4::{
    ComponentParts, ComponentSender, SimpleComponent,
    factory::FactoryVecDeque,
    gtk::{self, gdk, gio, prelude::*},
};

use crate::{
    applets::mpris::format,
    services::mpris::{Artwork, PlaybackStatus, Player, State, model::visible_players},
    widgets::{
        animated_popover::AnimatedPopover, media_transport::PlayState,
        now_playing_card::NowPlayingCard, popover_shell::PopoverShell,
    },
};

use self::row::RowItem;

#[derive(Default)]
struct CardPlayer {
    player_id: Option<String>,
    position_micros: i64,
}

pub struct Popover {
    popover: AnimatedPopover,
    card: NowPlayingCard,
    card_player: Rc<RefCell<CardPlayer>>,
    rows: FactoryVecDeque<RowItem>,
    current: Option<Player>,
    current_texture: Option<gdk::Texture>,
    others: Vec<Player>,
    max_rows: usize,
    show_artwork: bool,
    state: State,
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
    Previous {
        player_id: String,
    },
    PlayPause {
        player_id: String,
    },
    Next {
        player_id: String,
    },
    Seek {
        player_id: String,
        offset_micros: i64,
    },
    Raise {
        player_id: String,
    },
}

#[allow(unused_assignments)]
#[relm4::component(pub)]
impl SimpleComponent for Popover {
    type Init = PopoverInit;
    type Input = PopoverInput;
    type Output = PopoverOutput;

    view! {
        root = AnimatedPopover {
            add_css_class: "popover-size-xlarge",
            PopoverShell {
                #[local_ref]
                card_widget -> NowPlayingCard {
                        #[watch]
                        set_visible: model.current.is_some(),
                        #[watch]
                        set_tooltip_text: model.current.as_ref()
                            .map(format::row_tooltip).as_deref(),
                        #[watch]
                        set_title: &model.current.as_ref().map(format::title).unwrap_or_default(),
                        #[watch]
                        set_subtitle: &model.current.as_ref().map(format::subtitle).unwrap_or_default(),
                        #[watch]
                        set_artwork: model.current_texture.as_ref(),
                        #[watch]
                        set_progress: (
                            model.current.as_ref()
                                .and_then(|p| p.position).map(micros_to_seconds).unwrap_or(0.0),
                            model.current.as_ref()
                                .and_then(|p| p.length).map(micros_to_seconds).unwrap_or(0.0),
                        ),
                        #[watch]
                        set_seekable: model.current.as_ref()
                            .is_some_and(|p| p.can_seek && p.length.unwrap_or(0) > 0),
                        #[watch]
                        set_position_text: &model.current.as_ref()
                            .and_then(|p| p.position).map(format::duration).unwrap_or_default(),
                        #[watch]
                        set_length_text: &model.current.as_ref()
                            .and_then(|p| p.length).map(format::duration).unwrap_or_default(),
                        #[watch]
                        set_play_state: model.current.as_ref()
                            .map(|p| play_state(p.playback_status)).unwrap_or(PlayState::Paused),
                        #[watch]
                        set_can_previous: model.current.as_ref().is_some_and(|p| p.can_go_previous),
                        #[watch]
                        set_can_play_pause: model.current.as_ref().is_some_and(|p| p.can_play_pause),
                        #[watch]
                        set_can_next: model.current.as_ref().is_some_and(|p| p.can_go_next),
                    },

                gtk::Separator {
                    set_orientation: gtk::Orientation::Horizontal,
                    add_css_class: "mpris-divider",
                    #[watch]
                    set_visible: model.current.is_some() && !model.rows.is_empty(),
                },

                gtk::ScrolledWindow {
                    add_css_class: "mpris-other",
                    set_policy: (gtk::PolicyType::Never, gtk::PolicyType::Automatic),
                    set_vexpand: false,
                    set_propagate_natural_height: true,
                    #[watch]
                    set_visible: !model.rows.is_empty(),

                    #[local_ref]
                    rows_widget -> gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,
                    },
                },

                gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                    add_css_class: "empty-state",
                    #[watch]
                    set_visible: model.current.is_none() && model.rows.is_empty(),

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
        let rows = FactoryVecDeque::builder()
            .launch(rows_box.clone())
            .forward(sender.output_sender(), |output| output);
        let rows_widget = rows_box.clone();

        let card_player: Rc<RefCell<CardPlayer>> = Rc::new(RefCell::new(CardPlayer::default()));

        let mut model = Popover {
            popover: AnimatedPopover::new(),
            card,
            card_player,
            rows,
            current: None,
            current_texture: None,
            others: Vec::new(),
            max_rows: init.max_rows,
            show_artwork: init.show_artwork,
            state: State::default(),
        };

        let widgets = view_output!();
        model.popover = widgets.root.clone();
        widgets.root.set_parent(&init.parent);

        wire_card(&model.card, model.card_player.clone(), sender);

        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, _sender: ComponentSender<Self>) {
        match message {
            PopoverInput::Toggle => self.popover.toggle(),
            PopoverInput::Update(state) => self.sync_state(state),
            PopoverInput::Reconfigure {
                max_rows,
                show_artwork,
            } => {
                self.max_rows = max_rows;
                self.show_artwork = show_artwork;
                self.others.clear();
                // sync_state only reloads current_texture when the artwork
                // URI itself changes, so toggling show_artwork alone (same
                // track, same URI) would otherwise leave a stale texture
                // (or none) in place. Recompute it directly here.
                self.current_texture = self
                    .current
                    .as_ref()
                    .and_then(|p| load_texture(p, self.show_artwork));
                let state = self.state.clone();
                self.sync_state(state);
            }
        }
    }
}

impl Popover {
    fn sync_state(&mut self, state: State) {
        let visible = visible_players(&state.snapshot.players);
        let current = state
            .snapshot
            .current_player
            .clone()
            .filter(|player| visible.iter().any(|v| v.player_id == player.player_id))
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

        // Mirror the current player into the Rc cell the transport/seek
        // signal handlers read at click-time.
        self.card_player.replace(CardPlayer {
            player_id: current.as_ref().map(|p| p.player_id.clone()),
            position_micros: current.as_ref().and_then(|p| p.position).unwrap_or(0) as i64,
        });

        // Reload the texture only when the artwork URI actually changes,
        // not on every position tick.
        let next_artwork = current.as_ref().map(|p| &p.artwork);
        let prev_artwork = self.current.as_ref().map(|p| &p.artwork);
        if next_artwork != prev_artwork {
            self.current_texture = current
                .as_ref()
                .and_then(|p| load_texture(p, self.show_artwork));
        }

        self.current = current;

        // Diff instead of clear()+rebuild: guard.clear() destroys and
        // recreates every row, which re-reads artwork from disk (see
        // RowItem::init_model) — a full rebuild on every position tick of
        // any visible player, since `others` includes `position`.
        let same_membership_and_order = others.len() == self.others.len()
            && others
                .iter()
                .zip(&self.others)
                .all(|(a, b)| a.player_id == b.player_id);
        if same_membership_and_order {
            let mut guard = self.rows.guard();
            for (index, player) in others.iter().enumerate() {
                if *player != self.others[index]
                    && let Some(row) = guard.get_mut(index)
                {
                    row.update_player(player.clone(), self.show_artwork);
                }
            }
        } else {
            let mut guard = self.rows.guard();
            guard.clear();
            for player in &others {
                guard.push_back((player.clone(), self.show_artwork));
            }
        }
        self.others = others;

        self.state = state;
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

    // MediaScrubber emits `seek-requested` once when the user releases the
    // slider (drag) or after each keyboard step. No need to debounce here.
    card.scrubber().connect_seek_requested(move |_, seconds| {
        let player = card_player.borrow();
        let Some(id) = player.player_id.clone() else {
            return;
        };
        let target_micros = (seconds * 1_000_000.0) as i64;
        let offset_micros = target_micros - player.position_micros;
        let _ = sender.output(PopoverOutput::Seek {
            player_id: id,
            offset_micros,
        });
    });
}

pub(crate) fn play_state(status: PlaybackStatus) -> PlayState {
    match status {
        PlaybackStatus::Playing => PlayState::Playing,
        PlaybackStatus::Paused | PlaybackStatus::Stopped => PlayState::Paused,
    }
}

fn micros_to_seconds(value: u64) -> f64 {
    value as f64 / 1_000_000.0
}

pub(crate) fn load_texture(player: &Player, show_artwork: bool) -> Option<gdk::Texture> {
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

mod row {
    use relm4::{
        factory::{DynamicIndex, FactoryComponent, FactorySender},
        gtk::{self, gdk, prelude::*},
    };

    use crate::{
        applets::mpris::{
            format,
            popover::{PopoverOutput, load_texture, play_state},
        },
        services::mpris::Player,
        widgets::secondary_player_row::SecondaryPlayerRow,
    };

    pub struct RowItem {
        player: Player,
        artwork: Option<gdk::Texture>,
    }

    #[derive(Debug)]
    pub enum RowInput {
        PlayPausePressed,
        NextPressed,
        Activated,
    }

    #[relm4::factory(pub)]
    impl FactoryComponent for RowItem {
        type Init = (Player, bool);
        type Input = RowInput;
        type Output = PopoverOutput;
        type CommandOutput = ();
        type ParentWidget = gtk::Box;

        view! {
            #[root]
            SecondaryPlayerRow {
                #[watch]
                set_title: &format::title(&self.player),
                #[watch]
                set_subtitle: &format::row_subtitle(&self.player),
                #[watch]
                set_tooltip_text: Some(&format::row_tooltip(&self.player)),
                #[watch]
                set_artwork: self.artwork.as_ref(),
                #[watch]
                set_play_state: play_state(self.player.playback_status),
                #[watch]
                set_can_play_pause: self.player.can_play_pause,
                #[watch]
                set_can_next: self.player.can_go_next,

                connect_play_pause => RowInput::PlayPausePressed,
                connect_next => RowInput::NextPressed,
                connect_activated => RowInput::Activated,
            }
        }

        fn init_model(
            init: Self::Init,
            _index: &DynamicIndex,
            _sender: FactorySender<Self>,
        ) -> Self {
            let (player, show_artwork) = init;
            let artwork = load_texture(&player, show_artwork);
            Self { player, artwork }
        }

        fn update(&mut self, message: Self::Input, sender: FactorySender<Self>) {
            let player_id = self.player.player_id.clone();
            match message {
                RowInput::PlayPausePressed => {
                    let _ = sender.output(PopoverOutput::PlayPause { player_id });
                }
                RowInput::NextPressed => {
                    let _ = sender.output(PopoverOutput::Next { player_id });
                }
                RowInput::Activated => {
                    let _ = sender.output(PopoverOutput::Raise { player_id });
                }
            }
        }
    }

    impl RowItem {
        /// Updates an existing row in place instead of destroying and
        /// recreating it, so a field tick (e.g. position) on an unrelated
        /// row doesn't force every row to reload its artwork from disk.
        /// Only reloads the texture when the artwork identity itself changed.
        pub fn update_player(&mut self, player: Player, show_artwork: bool) {
            if player.artwork != self.player.artwork {
                self.artwork = load_texture(&player, show_artwork);
            }
            self.player = player;
        }
    }
}
