mod popover;
mod render;

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::{Duration, Instant};

use chrono::Utc;
use gettextrs::gettext;
use glimpse_config::{Applet as AppletConfig, AppletKind, MprisAppletConfig};
use glimpse_services::MprisHandle;
use glimpse_services::{Playback, PlayerAction, PlayerStatus, Repeat};
use glimpse_widgets::{
    IndicatorSpec, MprisPopover, Repeat as TransportRepeat, Row, TransportAction,
};
use gtk4::{gdk, gio, glib, prelude::*};

use crate::applet::popover::Seat;
use crate::applet::{Applet, Ctx, Input, Opener, spawn_command};

const SECOND: Duration = Duration::from_secs(1);
const MINUTE: Duration = Duration::from_secs(60);
const ART: i32 = 192;
const PENDING_TTL: Duration = Duration::from_secs(2);
const PENDING_MATCH: f64 = 1.0;

struct Pending {
    player: String,
    position: f64,
    at: Instant,
}

pub struct ResolvedIcon {
    pub entry: Option<String>,
    pub name: String,
}

pub struct Mpris {
    service: MprisHandle,
    settings: MprisAppletConfig,
    tooltip_format: Option<String>,
    players: Vec<PlayerStatus>,
    ticking: Option<bool>,
    pending: Rc<RefCell<Option<Pending>>>,
    art: Rc<RefCell<Art<gdk::Texture>>>,
    opener: Option<Opener>,
    icons: HashMap<String, ResolvedIcon>,
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
    fn configure(&mut self, ctx: &Ctx, config: &AppletConfig) {
        let AppletKind::Mpris(settings) = &config.kind else {
            return;
        };
        self.settings = settings.clone();
        self.opener = Some(ctx.opener());
        self.tooltip_format = config.kind.tooltip_format().map(str::to_owned);
        self.pace(ctx);
        self.refresh();
    }

    fn handle(&mut self, ctx: &Ctx, input: &Input) {
        match input {
            Input::Woken => {
                self.players = self.service.snapshot().players;
                self.pace(ctx);
                self.press();
            }
            Input::Tick => {}
            Input::Pointer(_) => return,
        }
        self.refresh();
    }

    fn indicators(&self) -> Vec<IndicatorSpec> {
        self.spec.clone()
    }

    fn popover(&mut self, seat: &Seat) -> Option<gtk4::Widget> {
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
            let (service, held) = (self.service.clone(), self.aimed.clone());
            let pending = Rc::clone(&self.pending);
            move |_, seconds| {
                let service = service.clone();
                aimed(&held, |player| {
                    *pending.borrow_mut() = Some(Pending {
                        player: player.clone(),
                        position: seconds,
                        at: Instant::now(),
                    });
                    spawn_command("mpris.set_position", async move {
                        service
                            .set_position(player, (seconds * 1_000_000.0) as i64)
                            .await
                    });
                })
            }
        });

        shown.connect_raise_requested({
            let service = self.service.clone();
            move |_, player| {
                let service = service.clone();
                spawn_command("mpris.raise", async move {
                    service.control(player, PlayerAction::Raise).await
                });
            }
        });

        shown.connect_toggle_requested({
            let service = self.service.clone();
            move |_, player| {
                let service = service.clone();
                spawn_command("mpris.play_pause", async move {
                    service.control(player, PlayerAction::PlayPause).await
                });
            }
        });

        shown.open_player().connect_activated({
            let (service, held) = (self.service.clone(), self.aimed.clone());
            move |_| {
                let service = service.clone();
                aimed(&held, |player| {
                    spawn_command("mpris.raise", async move {
                        service.control(player, PlayerAction::Raise).await
                    });
                })
            }
        });

        self.shown.set(Some(&shown));
        self.refresh();
        Some(shown.upcast())
    }
}

impl Mpris {
    pub fn start(service: MprisHandle) -> Self {
        let players = service.snapshot().players;
        Self {
            service,
            settings: MprisAppletConfig::default(),
            tooltip_format: None,
            players,
            ticking: None,
            pending: Rc::new(RefCell::new(None)),
            art: Rc::default(),
            opener: None,
            icons: HashMap::new(),
            aimed: Rc::new(RefCell::new(String::new())),
            pressed: Rc::new(RefCell::new(Vec::new())),
            spec: Vec::new(),
            shown: glib::WeakRef::new(),
        }
    }

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
                ResolvedIcon {
                    entry: player.desktop_entry.clone(),
                    name,
                },
            );
        }
    }

    fn press(&mut self) {
        let service = self.service.clone();
        for action in self.pressed.take() {
            let Some(player) = self.players.iter_mut().find(|player| player.current) else {
                continue;
            };
            let id = player.id.clone();

            match action {
                TransportAction::Shuffle => {
                    let shuffle = !player.shuffle.unwrap_or_default();
                    player.shuffle = Some(shuffle);
                    let service = service.clone();
                    spawn_command("mpris.set_shuffle", async move {
                        service.set_shuffle(id, shuffle).await
                    });
                }
                TransportAction::Repeat => {
                    let repeat = cycled(player.repeat.unwrap_or_default());
                    player.repeat = Some(repeat);
                    let service = service.clone();
                    spawn_command("mpris.set_repeat", async move {
                        service.set_repeat(id, repeat).await
                    });
                }
                TransportAction::PlayPause => {
                    player.playback = match player.playback {
                        Playback::Playing => Playback::Paused,
                        _ => Playback::Playing,
                    };
                    let service = service.clone();
                    spawn_command("mpris.play_pause", async move {
                        service.control(id, PlayerAction::PlayPause).await
                    });
                }
                TransportAction::Previous => {
                    let service = service.clone();
                    spawn_command("mpris.previous", async move {
                        service.control(id, PlayerAction::Previous).await
                    });
                }
                TransportAction::Next => {
                    let service = service.clone();
                    spawn_command("mpris.next", async move {
                        service.control(id, PlayerAction::Next).await
                    });
                }
            }
        }
    }

    fn load(&mut self) {
        let Some(opener) = self.opener.clone() else {
            return;
        };
        let wanted = self.current().and_then(|player| player.art.clone());
        let Some((generation, path)) = self.art.borrow_mut().want(wanted.as_deref()) else {
            return;
        };
        let art = self.art.clone();
        glib::spawn_future_local(async move {
            let texture = gio::spawn_blocking(move || {
                glimpse_widgets::artwork(
                    std::path::Path::new(&path),
                    ART,
                    glimpse_widgets::Formats::Any,
                )
            })
            .await
            .ok()
            .flatten();
            if art.borrow_mut().land(generation, texture) {
                opener.wake();
            }
        });
    }

    fn indicator(&self) -> Option<IndicatorSpec> {
        let player = self.current()?;
        let now = Utc::now();
        let cap = usize::from(self.settings.max_length.max(1));
        let label = render::trimmed(
            &render::label(&self.settings.label_format, player, now),
            cap,
        )
        .or_else(|| render::trimmed(&player.identity, cap));

        Some(IndicatorSpec {
            label,
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
            shown.player().set_visible(false);
            shown.set_others(None);
            shown.open_player().set_visible(false);
            return;
        };

        let shuffle = player.shuffle.unwrap_or_default();
        let repeat = player.repeat.unwrap_or_default();
        self.aimed.replace(player.id.clone());

        let playing = shown.player();
        playing.set_visible(true);
        playing.set_source(Some(player.identity.clone()));
        playing.set_title(player.title.clone());
        playing.set_artist(player.artist.clone());
        playing.set_album(player.album.clone());
        playing.set_icon_name(self.icons.get(&player.id).map(|held| held.name.clone()));
        playing.set_art(self.art.borrow().texture());

        let scrubber = playing.scrubber();
        scrubber.set_duration(render::seconds(player.length_us.unwrap_or_default()));
        let published = render::seconds(render::position(player, Utc::now()));
        scrubber.set_position(aimed_position(
            &self.pending,
            &player.id,
            published,
            Instant::now(),
        ));
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

        let others =
            Some(popover::rows(&self.players, &self.icons)).filter(|rows| !rows.is_empty());
        shown.set_others(others.as_deref());
        let open_player = shown.open_player();
        open_player.upcast_ref::<Row>().set_title(Some(
            gettext("Open {player}")
                .replace("{player}", &player.identity)
                .as_str(),
        ));
        open_player.set_visible(player.can.raise);
    }
}

struct Art<T> {
    path: Option<String>,
    texture: Option<T>,
    loading: bool,
    generation: u64,
}

impl<T> Default for Art<T> {
    fn default() -> Self {
        Self {
            path: None,
            texture: None,
            loading: false,
            generation: 0,
        }
    }
}

impl<T> Art<T> {
    fn want(&mut self, wanted: Option<&str>) -> Option<(u64, String)> {
        if self.path.as_deref() != wanted {
            self.path = wanted.map(str::to_owned);
            self.texture = None;
            self.loading = false;
            self.generation = self.generation.wrapping_add(1);
        }
        if self.loading || self.texture.is_some() {
            return None;
        }
        let path = self.path.clone()?;
        self.loading = true;
        Some((self.generation, path))
    }

    fn land(&mut self, generation: u64, texture: Option<T>) -> bool {
        if !self.loading || self.generation != generation {
            return false;
        }
        self.loading = false;
        self.texture = texture;
        self.texture.is_some()
    }

    fn texture(&self) -> Option<&T> {
        self.texture.as_ref()
    }
}

fn aimed(held: &RefCell<String>, act: impl FnOnce(String)) {
    let player = held.borrow().clone();
    if !player.is_empty() {
        act(player);
    }
}

fn aimed_position(
    pending: &RefCell<Option<Pending>>,
    player: &str,
    published: f64,
    now: Instant,
) -> f64 {
    let mut slot = pending.borrow_mut();
    let Some(held) = slot.as_ref() else {
        return published;
    };
    let target = held.position;
    let stale = now.saturating_duration_since(held.at) >= PENDING_TTL;
    let reached = (published - target).abs() <= PENDING_MATCH;
    let aimed = held.player == player;
    if !aimed || reached || stale {
        *slot = None;
        return published;
    }
    target
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
mod tests;
