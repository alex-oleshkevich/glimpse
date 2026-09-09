mod popover;
mod render;

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Duration;

use chrono::Utc;
use gettextrs::gettext;
use glimpse_config::{Applet as AppletConfig, AppletKind, MprisAppletConfig};
use glimpse_contracts::{
    Message as _, MprisControl, MprisPlayers, MprisSetPosition, MprisSetRepeat, MprisSetShuffle,
    Playback, PlayerAction, PlayerStatus, Repeat,
};
use glimpse_widgets::{IndicatorSpec, MprisPopover, Repeat as TransportRepeat, TransportAction};
use gtk4::{gdk, gio, glib};

use crate::applet::popover::{PopoverHandle, Seat};
use crate::applet::{Applet, Ctx, Input, payload};

const SECOND: Duration = Duration::from_secs(1);
const MINUTE: Duration = Duration::from_secs(60);
const ART: i32 = 192;

/// A player's icon, resolved against the theme once rather than once per render: `indicators` is
/// pulled after every input, and resolving walks a candidate list calling `has_icon` on each. The
/// desktop entry is held because it is an input to that resolution and a player may answer
/// `DesktopEntry` late — keyed on the id alone, a fallback would stick for the player's whole life.
pub struct Themed {
    pub entry: Option<String>,
    pub name: String,
    pub icon: gio::Icon,
}

#[derive(Default)]
pub struct Mpris {
    settings: MprisAppletConfig,
    tooltip_format: Option<String>,
    players: Vec<PlayerStatus>,
    ticking: Option<bool>,
    art: Option<(String, gdk::Texture)>,
    icons: HashMap<String, Themed>,
    /// The player the popover's own controls act on. A signal closure cannot reach `&mut self`,
    /// and which player is current changes under it.
    aimed: Rc<RefCell<String>>,
    /// Transport presses waiting to be acted on. `Transport` reports only that a button was
    /// pressed, so the next shuffle or repeat value has to be computed against the model — which
    /// only `handle` holds, and which is also what `dress` renders, so applying it there is what
    /// puts the optimistic value on the button instead of on a copy nothing draws.
    pressed: Rc<RefCell<Vec<TransportAction>>>,
    spec: Vec<IndicatorSpec>,
    shown: glib::WeakRef<MprisPopover>,
}

impl Applet for Mpris {
    fn topics(&self) -> &'static [&'static str] {
        &[MprisPlayers::NAME]
    }

    fn start() -> Self {
        Self::default()
    }

    fn configure(&mut self, ctx: &Ctx, config: &AppletConfig) {
        let AppletKind::Mpris(settings) = &config.kind else {
            return;
        };
        self.settings = settings.clone();
        self.tooltip_format = config.common.tooltip_format.clone();
        self.pace(ctx);
        self.refresh();
    }

    fn handle(&mut self, ctx: &Ctx, input: &Input) {
        match input {
            Input::Topic(event) => {
                let Some(status) = payload::<MprisPlayers>(event) else {
                    return;
                };
                self.players = status.players;
                self.pace(ctx);
            }
            Input::Woken => self.press(ctx),
            Input::Tick => {}
            _ => return,
        }
        self.refresh();
    }

    fn indicators(&self) -> Vec<IndicatorSpec> {
        self.spec.clone()
    }

    fn popover(&mut self, seat: &Seat) -> Option<Box<dyn PopoverHandle>> {
        let shown = MprisPopover::new();
        let player = shown.player();

        player.transport().connect_action({
            let (pressed, opener) = (self.pressed.clone(), seat.opener());
            move |_, action| {
                pressed.borrow_mut().push(action);
                opener.wake();
            }
        });

        player.scrubber().connect_seek({
            let (caller, held) = (seat.caller(), self.aimed.clone());
            move |_, seconds| {
                aimed(&held, |player| {
                    caller.call::<MprisSetPosition>(MprisSetPosition {
                        player,
                        position_us: (seconds * 1_000_000.0) as i64,
                    })
                })
            }
        });

        shown.connect_raise_requested({
            let caller = seat.caller();
            move |_, player| {
                caller.call::<MprisControl>(MprisControl {
                    player,
                    action: PlayerAction::Raise,
                })
            }
        });

        shown.connect_toggle_requested({
            let caller = seat.caller();
            move |_, player| {
                caller.call::<MprisControl>(MprisControl {
                    player,
                    action: PlayerAction::PlayPause,
                })
            }
        });

        shown.connect_footer_activated({
            let (caller, held) = (seat.caller(), self.aimed.clone());
            move |_| {
                aimed(&held, |player| {
                    caller.call::<MprisControl>(MprisControl {
                        player,
                        action: PlayerAction::Raise,
                    })
                })
            }
        });

        self.shown.set(Some(&shown));
        self.refresh();
        Some(Box::new(shown))
    }
}

impl Mpris {
    /// A second while something is playing, a minute otherwise. `ctx.interval` replaces the timer
    /// rather than adding one, so asking again is how it changes pace.
    fn pace(&mut self, ctx: &Ctx) {
        let playing = self
            .players
            .iter()
            .any(|player| player.playback == Playback::Playing);
        if self.ticking != Some(playing) {
            ctx.interval(match playing {
                true => SECOND,
                false => MINUTE,
            });
            self.ticking = Some(playing);
        }
    }

    fn current(&self) -> Option<&PlayerStatus> {
        self.players.iter().find(|player| player.current)
    }

    fn refresh(&mut self) {
        self.resolve();
        self.spec = self.indicator().into_iter().collect();

        let Some(shown) = self.shown.upgrade() else {
            return;
        };
        self.load();
        self.dress(&shown);
    }

    fn resolve(&mut self) {
        let wanted: HashMap<&str, Option<&str>> = self
            .players
            .iter()
            .map(|player| (player.id.as_str(), player.desktop_entry.as_deref()))
            .collect();
        self.icons
            .retain(|id, held| wanted.get(id.as_str()) == Some(&held.entry.as_deref()));

        for player in &self.players {
            if self.icons.contains_key(&player.id) {
                continue;
            }
            let name = popover::icon_name(player);
            self.icons.insert(
                player.id.clone(),
                Themed {
                    entry: player.desktop_entry.clone(),
                    icon: popover::themed(&name),
                    name,
                },
            );
        }
    }

    /// Every press recorded while the popover was live, applied against the model `dress` renders
    /// so the control moves now and the next payload reconciles it.
    fn press(&mut self, ctx: &Ctx) {
        for action in self.pressed.take() {
            let Some(player) = self.players.iter_mut().find(|player| player.current) else {
                continue;
            };
            let id = player.id.clone();

            match action {
                TransportAction::Shuffle => {
                    let shuffle = !player.shuffle.unwrap_or_default();
                    player.shuffle = Some(shuffle);
                    ctx.call::<MprisSetShuffle>(MprisSetShuffle {
                        player: id,
                        shuffle,
                    });
                }
                TransportAction::Repeat => {
                    let repeat = cycled(player.repeat.unwrap_or_default());
                    player.repeat = Some(repeat);
                    ctx.call::<MprisSetRepeat>(MprisSetRepeat { player: id, repeat });
                }
                TransportAction::PlayPause => {
                    player.playback = match player.playback {
                        Playback::Playing => Playback::Paused,
                        _ => Playback::Playing,
                    };
                    ctx.call::<MprisControl>(MprisControl {
                        player: id,
                        action: PlayerAction::PlayPause,
                    });
                }
                TransportAction::Previous => ctx.call::<MprisControl>(MprisControl {
                    player: id,
                    action: PlayerAction::Previous,
                }),
                TransportAction::Next => ctx.call::<MprisControl>(MprisControl {
                    player: id,
                    action: PlayerAction::Next,
                }),
            }
        }
    }

    /// Decoded once per path rather than once per tick, and only while the popover is the thing
    /// that would show it.
    fn load(&mut self) {
        let wanted = self
            .settings
            .show_art
            .then(|| self.current().and_then(|player| player.art.clone()))
            .flatten();

        let Some(path) = wanted else {
            self.art = None;
            return;
        };
        if self.art.as_ref().is_some_and(|(held, _)| *held == path) {
            return;
        }
        self.art = glimpse_widgets::artwork(std::path::Path::new(&path), ART)
            .map(|texture| (path, texture));
    }

    /// A label that renders to nothing leaves an icon-only chip rather than removing the applet:
    /// a stream with no metadata would otherwise take the popover off the bar with it, while it is
    /// still the thing making the noise.
    fn indicator(&self) -> Option<IndicatorSpec> {
        let player = self.current()?;
        let now = Utc::now();

        Some(IndicatorSpec {
            icon: self.icons.get(&player.id).map(|held| held.icon.clone()),
            label: render::trimmed(
                &render::label(&self.settings.label_format, player, now),
                usize::from(self.settings.max_length.max(1)),
            ),
            tooltip: self
                .tooltip_format
                .as_deref()
                .map(|format| render::label(format, player, now)),
            ..Default::default()
        })
    }

    fn dress(&self, shown: &MprisPopover) {
        let Some(player) = self.current() else {
            self.aimed.replace(String::new());
            shown.set_others(self.settings.show_others.then_some(&[][..]));
            shown.set_footer(None);
            return;
        };

        let shuffle = player.shuffle.unwrap_or_default();
        let repeat = player.repeat.unwrap_or_default();
        self.aimed.replace(player.id.clone());

        let playing = shown.player();
        playing.set_source(Some(player.identity.clone()));
        playing.set_title(player.title.clone());
        playing.set_artist(player.artist.clone());
        playing.set_album(player.album.clone());
        playing.set_icon_name(self.icons.get(&player.id).map(|held| held.name.clone()));
        playing.set_art(self.art.as_ref().map(|(_, texture)| texture));

        let scrubber = playing.scrubber();
        scrubber.set_duration(render::seconds(player.length_us.unwrap_or_default()));
        scrubber.set_position(render::seconds(render::position(player, Utc::now())));
        scrubber.set_seekable(player.can.seek);

        let transport = playing.transport();
        transport.set_playing(player.playback == Playback::Playing);
        transport.set_can_play(player.can.play || player.can.pause);
        transport.set_can_previous(player.can.previous);
        transport.set_can_next(player.can.next);
        transport.set_can_shuffle(player.shuffle.is_some());
        transport.set_shuffle(shuffle);
        transport.set_can_repeat(player.repeat.is_some());
        transport.set_repeat(transport_repeat(repeat));

        let others = self
            .settings
            .show_others
            .then(|| popover::rows(&self.players, &self.icons));
        shown.set_others(others.as_deref());
        shown.set_footer(
            player
                .can
                .raise
                .then(|| gettext("Open {player}").replace("{player}", &player.identity))
                .as_deref(),
        );
    }
}

/// The current player's id, or nothing at all: a control that fired with no player current would
/// otherwise ask the daemon about one named "".
fn aimed(held: &RefCell<String>, act: impl FnOnce(String)) {
    let player = held.borrow().clone();
    if !player.is_empty() {
        act(player);
    }
}

fn cycled(repeat: Repeat) -> Repeat {
    match repeat {
        Repeat::Off => Repeat::Playlist,
        Repeat::Playlist => Repeat::Track,
        Repeat::Track | Repeat::Unknown => Repeat::Off,
    }
}

fn transport_repeat(repeat: Repeat) -> TransportRepeat {
    match repeat {
        Repeat::Track => TransportRepeat::Track,
        Repeat::Playlist => TransportRepeat::Playlist,
        Repeat::Off | Repeat::Unknown => TransportRepeat::Off,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_transport_repeat_carries_the_same_three_states_as_the_wire() {
        assert_eq!(transport_repeat(Repeat::Off), TransportRepeat::Off);
        assert_eq!(
            transport_repeat(Repeat::Playlist),
            TransportRepeat::Playlist
        );
        assert_eq!(transport_repeat(Repeat::Track), TransportRepeat::Track);
        assert_eq!(
            transport_repeat(Repeat::Unknown),
            TransportRepeat::Off,
            "a state this panel does not know renders as off rather than as a missing icon"
        );
    }

    /// The button says only that it was pressed, so pressing it three times has to walk the whole
    /// cycle and come back.
    #[test]
    fn repeat_cycles_off_playlist_track_and_round_again() {
        assert_eq!(cycled(Repeat::Off), Repeat::Playlist);
        assert_eq!(cycled(Repeat::Playlist), Repeat::Track);
        assert_eq!(cycled(Repeat::Track), Repeat::Off);
        assert_eq!(cycled(Repeat::Unknown), Repeat::Off);
    }
}
