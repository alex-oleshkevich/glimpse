mod art;
mod select;

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::pin::Pin;

use chrono::{DateTime, Utc};
use futures_util::{Stream, StreamExt as _, stream};
use glimpse_contracts::{
    Command as _, Message, MprisControl, MprisPlayers, MprisSeek, MprisSetPosition, MprisSetRepeat,
    MprisSetShuffle, MprisSetVolume, Playback, PlayerAction, PlayerCapabilities, PlayerStatus,
    Repeat,
};
use glimpse_dbus::mpris::{MPRIS_NAME_PREFIX, MPRIS_PATH, MprisPlayerProxy, MprisRootProxy};
use glimpse_ipc::CallError;
use glimpse_utils::text::clean;
use regex::Regex;
use serde_json::Value;
use zbus::zvariant::{ObjectPath, OwnedValue};

use super::super::{AGENT, say, transport};
use crate::broker::Responder;
use crate::{
    context::Ctx,
    publisher::Publisher,
    service::{Input, Service, ServiceError, decode_args, unknown_command},
    subscription::Sub,
};

const IDENTITY: usize = 60;
const TITLE: usize = 120;
const ARTIST: usize = 120;
const ALBUM: usize = 120;

/// One player as the service holds it. `PlayerStatus` is the wire shape; this keeps the two fields
/// the wire has no use for — the bus name a command is addressed to, and the track id
/// `SetPosition` demands and the caller has no way to know.
#[derive(Debug, Clone)]
pub struct Player {
    pub bus: String,
    pub id: String,
    pub identity: String,
    pub desktop_entry: Option<String>,
    pub playback: Playback,
    pub current: bool,
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub art_url: Option<String>,
    pub track_id: Option<String>,
    pub length_us: Option<i64>,
    pub position_us: i64,
    pub position_at: DateTime<Utc>,
    pub rate: f64,
    pub volume: Option<f64>,
    pub repeat: Option<Repeat>,
    pub shuffle: Option<bool>,
    pub can: PlayerCapabilities,
    pub last_active: DateTime<Utc>,
}

pub enum Event {
    Listed(Vec<String>),
    Appeared(String),
    Vanished(String),
    Updated(Box<Player>),
    Gone(String),
    Art { url: String, path: Option<String> },
    Unavailable(String),
}

#[derive(Debug)]
pub struct Command {
    player: String,
    what: Action,
}

#[derive(Debug)]
pub enum Action {
    Control(PlayerAction),
    Seek(i64),
    Position(i64),
    Volume(f64),
    Loop(Repeat),
    Shuffle(bool),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    ignore: Vec<String>,
    fetch_art: bool,
    art_max_kib: u32,
}

impl From<&glimpse_config::Config> for Config {
    fn from(document: &glimpse_config::Config) -> Self {
        Self {
            ignore: document.mpris.ignore.clone(),
            fetch_art: document.mpris.fetch_art,
            art_max_kib: document.mpris.art_max_kib.clamp(16, 65_536),
        }
    }
}

/// `attempt` is bumped whenever the ignore list changes. A player that stops being ignored is
/// already on the bus, so nothing will announce it again — only a fresh `ListNames` finds it.
#[derive(PartialEq, Eq, Hash)]
pub enum Watch {
    Names { attempt: u64 },
    Player(String),
    Art { url: String, cap: u32 },
}

pub struct Mpris {
    players: Publisher<MprisPlayers>,
    known: BTreeMap<String, Option<Player>>,
    art: BTreeMap<String, Option<String>>,
    ignore: Vec<Regex>,
    settings: Config,
    client: Option<reqwest::Client>,
    attempt: u64,
}

impl Service for Mpris {
    const NAME: &'static str = "mpris";
    const TOPICS: &'static [&'static str] = &[MprisPlayers::NAME];
    const METHODS: &'static [&'static str] = &[
        MprisControl::NAME,
        MprisSeek::NAME,
        MprisSetPosition::NAME,
        MprisSetVolume::NAME,
        MprisSetRepeat::NAME,
        MprisSetShuffle::NAME,
    ];

    type Config = Config;
    type Command = Command;
    type Event = Event;
    type SubKey = Watch;

    fn decode(method: &str, args: Value) -> Result<Self::Command, CallError> {
        Ok(match method {
            MprisControl::NAME => {
                let asked: MprisControl = decode_args(args)?;
                Command {
                    player: asked.player,
                    what: Action::Control(asked.action),
                }
            }
            MprisSeek::NAME => {
                let asked: MprisSeek = decode_args(args)?;
                Command {
                    player: asked.player,
                    what: Action::Seek(asked.offset_us),
                }
            }
            MprisSetPosition::NAME => {
                let asked: MprisSetPosition = decode_args(args)?;
                Command {
                    player: asked.player,
                    what: Action::Position(asked.position_us),
                }
            }
            MprisSetVolume::NAME => {
                let asked: MprisSetVolume = decode_args(args)?;
                Command {
                    player: asked.player,
                    what: Action::Volume(asked.volume),
                }
            }
            MprisSetRepeat::NAME => {
                let asked: MprisSetRepeat = decode_args(args)?;
                Command {
                    player: asked.player,
                    what: Action::Loop(asked.repeat),
                }
            }
            MprisSetShuffle::NAME => {
                let asked: MprisSetShuffle = decode_args(args)?;
                Command {
                    player: asked.player,
                    what: Action::Shuffle(asked.shuffle),
                }
            }
            _ => return Err(unknown_command(Self::NAME, method)),
        })
    }

    fn subscriptions(&self) -> Vec<Sub<Self>> {
        let mut subs = vec![Sub::stream(
            Watch::Names {
                attempt: self.attempt,
            },
            names,
        )];

        subs.extend(self.known.keys().map(|bus| {
            let bus = bus.clone();
            Sub::stream(Watch::Player(bus.clone()), move |ctx| follow(ctx, bus))
        }));

        let Some(client) = self.client.clone() else {
            return subs;
        };
        let cap = self.settings.art_max_kib;
        subs.extend(self.wanted_art().into_iter().map(|url| {
            let (client, wanted) = (client.clone(), url.clone());
            Sub::stream(Watch::Art { url, cap }, move |_ctx| async move {
                stream::once(art::fetch(client, wanted, cap))
            })
        }));

        subs
    }

    async fn start(ctx: &Ctx<Self>, config: Self::Config) -> Result<Self, ServiceError> {
        if let Err(reason) = ctx.session_bus() {
            ctx.degraded(format!("no session bus: {reason}"));
        }

        Ok(Self {
            players: ctx.publisher::<MprisPlayers>(),
            known: BTreeMap::new(),
            art: BTreeMap::new(),
            ignore: select::compile(&config.ignore),
            client: client(ctx, &config),
            settings: config,
            attempt: 0,
        })
    }

    async fn handle(&mut self, ctx: &Ctx<Self>, input: Input<Self>) {
        match input {
            Input::Event(Event::Listed(names)) => {
                self.known
                    .retain(|bus, _| names.iter().any(|listed| listed == bus));
                for bus in names {
                    self.admit(bus);
                }
                self.publish();
            }
            Input::Event(Event::Appeared(bus)) => {
                self.admit(bus);
                self.publish();
            }
            Input::Event(Event::Vanished(bus)) | Input::Event(Event::Gone(bus)) => {
                if self.known.remove(&bus).is_some() {
                    self.publish();
                }
            }
            Input::Event(Event::Updated(player)) if !self.known.contains_key(&player.bus) => {}
            Input::Event(Event::Updated(mut player)) => {
                if select::ignored(&self.ignore, &player.id, &player.identity) {
                    self.known.remove(&player.bus);
                } else {
                    ctx.running();
                    let held = self.known.get(&player.bus).and_then(Option::as_ref);
                    player.last_active = stirred(held, &player);
                    self.known.insert(player.bus.clone(), Some(*player));
                }
                self.publish();
            }
            Input::Event(Event::Art { url, path }) => {
                self.art.insert(url, path);
                self.publish();
            }
            Input::Event(Event::Unavailable(reason)) => {
                ctx.degraded(reason);
                self.known.clear();
                self.publish();
            }
            Input::Config(config) => {
                if config.ignore != self.settings.ignore {
                    self.ignore = select::compile(&config.ignore);
                    self.known.retain(|_, held| {
                        !held.as_ref().is_some_and(|player| {
                            select::ignored(&self.ignore, &player.id, &player.identity)
                        })
                    });
                    self.attempt = self.attempt.wrapping_add(1);
                }
                if config.fetch_art != self.settings.fetch_art
                    || config.art_max_kib != self.settings.art_max_kib
                {
                    self.art.clear();
                }
                self.client = client(ctx, &config);
                self.settings = config;
                self.publish();
            }
            Input::Command(command, responder) => self.act(ctx, command, responder),
        }
    }
}

impl Mpris {
    fn admit(&mut self, bus: String) {
        let id = bus.trim_start_matches(MPRIS_NAME_PREFIX);
        if id.is_empty() || select::ignored(&self.ignore, id, "") {
            return;
        }
        self.known.entry(bus).or_default();
    }

    /// Every remote artwork URL a known player names, in the form `self.art` is keyed by. The raw
    /// `art_url` is not that key — `Url` serializes a trimmed, lower-cased, percent-encoded form —
    /// so anything comparing against the raw string misses.
    fn remote_art(&self) -> BTreeSet<String> {
        self.known
            .values()
            .flatten()
            .filter_map(|player| match art::classify(player.art_url.as_deref())? {
                art::Art::Remote(url) => Some(url),
                art::Art::Local(_) => None,
            })
            .collect()
    }

    fn wanted_art(&self) -> BTreeSet<String> {
        let mut wanted = self.remote_art();
        wanted.retain(|url| !self.art.contains_key(url));
        wanted
    }

    fn publish(&mut self) {
        let referenced = self.remote_art();
        self.art.retain(|url, _| referenced.contains(url));

        let held: Vec<Player> = self.known.values().flatten().cloned().collect();
        let arranged = select::arrange(held);

        let players = arranged
            .iter()
            .map(|player| self.status(player))
            .collect::<Vec<_>>();
        self.players.set(MprisPlayers { players });
    }

    fn status(&self, player: &Player) -> PlayerStatus {
        let art = match art::classify(player.art_url.as_deref()) {
            Some(art::Art::Local(path)) => Some(path),
            Some(art::Art::Remote(url)) => self.art.get(&url).cloned().flatten(),
            None => None,
        };

        PlayerStatus {
            id: player.id.clone(),
            identity: player.identity.clone(),
            desktop_entry: player.desktop_entry.clone(),
            playback: player.playback,
            current: player.current,
            title: player.title.clone(),
            artist: player.artist.clone(),
            album: player.album.clone(),
            art,
            length_us: player.length_us,
            position_us: player.position_us,
            position_at: player.position_at,
            rate: player.rate,
            volume: player.volume,
            repeat: player.repeat,
            shuffle: player.shuffle,
            can: player.can,
        }
    }

    fn act(&mut self, ctx: &Ctx<Self>, command: Command, responder: Responder) {
        let Some(held) = self.player(&command.player) else {
            responder.fail(CallError::new(
                glimpse_ipc::ErrorCode::InvalidArgs,
                format!("no player `{}`", command.player),
            ));
            return;
        };

        let (bus, track) = (held.bus.clone(), held.track_id.clone());

        if let Some(Some(player)) = self.known.get_mut(&bus) {
            player.last_active = Utc::now();
        }

        let Ok(connection) = ctx.session_bus().cloned() else {
            responder.fail(CallError::new(
                glimpse_ipc::ErrorCode::Unavailable,
                "no session bus",
            ));
            return;
        };

        ctx.spawn_detached(move |_ctx| async move {
            match apply(&connection, &bus, track, command.what).await {
                Ok(()) => responder.ok(()),
                Err(reason) => {
                    responder.fail(CallError::new(glimpse_ipc::ErrorCode::Unavailable, reason))
                }
            }
        });
    }

    fn player(&self, id: &str) -> Option<&Player> {
        self.known.values().flatten().find(|player| player.id == id)
    }
}

fn client(ctx: &Ctx<Mpris>, config: &Config) -> Option<reqwest::Client> {
    if !config.fetch_art {
        return None;
    }
    match reqwest::Client::builder()
        .user_agent(AGENT)
        .redirect(reqwest::redirect::Policy::limited(3))
        .build()
    {
        Ok(client) => Some(client),
        Err(error) => {
            ctx.degraded(format!("artwork cannot be fetched: {error}"));
            None
        }
    }
}

async fn apply(
    connection: &zbus::Connection,
    bus: &str,
    track: Option<String>,
    action: Action,
) -> Result<(), String> {
    if let Action::Control(PlayerAction::Raise) = action {
        let root = MprisRootProxy::builder(connection)
            .destination(bus.to_owned())
            .map_err(say)?
            .build()
            .await
            .map_err(say)?;
        return root.raise().await.map_err(say);
    }

    let player = MprisPlayerProxy::builder(connection)
        .destination(bus.to_owned())
        .map_err(say)?
        .build()
        .await
        .map_err(say)?;

    match action {
        Action::Control(action) => match action {
            PlayerAction::Play => player.play().await.map_err(say),
            PlayerAction::Pause => player.pause().await.map_err(say),
            PlayerAction::PlayPause => player.play_pause().await.map_err(say),
            PlayerAction::Stop => player.stop().await.map_err(say),
            PlayerAction::Previous => player.previous().await.map_err(say),
            PlayerAction::Next => player.next().await.map_err(say),
            PlayerAction::Raise => Ok(()),
        },
        Action::Seek(offset_us) => player.seek(offset_us).await.map_err(say),
        Action::Position(position_us) => {
            let track = track.ok_or("the player named no track to seek within")?;
            let path = ObjectPath::try_from(track.as_str())
                .map_err(|_| "the player's track id is not an object path".to_owned())?;
            player
                .set_position(&path, position_us.max(0))
                .await
                .map_err(say)
        }
        Action::Volume(volume) => player.set_volume(volume.clamp(0.0, 1.0)).await.map_err(say),
        Action::Loop(repeat) => player.set_loop_status(status(repeat)).await.map_err(say),
        Action::Shuffle(shuffle) => player.set_shuffle(shuffle).await.map_err(say),
    }
}

fn status(repeat: Repeat) -> &'static str {
    match repeat {
        Repeat::Track => "Track",
        Repeat::Playlist => "Playlist",
        Repeat::Off | Repeat::Unknown => "None",
    }
}

async fn names(ctx: Ctx<Mpris>) -> Pin<Box<dyn Stream<Item = Event> + Send>> {
    match owners(&ctx).await {
        Ok(owners) => Box::pin(owners),
        Err(reason) => Box::pin(stream::once(async move { Event::Unavailable(reason) })),
    }
}

async fn owners(ctx: &Ctx<Mpris>) -> Result<impl Stream<Item = Event> + Send + 'static, String> {
    let connection = ctx.session_bus().map_err(str::to_owned)?.clone();
    let dbus = zbus::fdo::DBusProxy::new(&connection).await.map_err(say)?;

    let changes = dbus.receive_name_owner_changed().await.map_err(say)?;

    let listed: Vec<String> = dbus
        .list_names()
        .await
        .map_err(say)?
        .into_iter()
        .map(|name| name.to_string())
        .filter(|name| name.starts_with(MPRIS_NAME_PREFIX))
        .collect();

    let following = changes.filter_map(|signal| async move {
        let args = signal.args().ok()?;
        let name = args.name().to_string();
        if !name.starts_with(MPRIS_NAME_PREFIX) {
            return None;
        }
        Some(match args.new_owner().is_some() {
            true => Event::Appeared(name),
            false => Event::Vanished(name),
        })
    });

    Ok(stream::once(async move { Event::Listed(listed) }).chain(following))
}

async fn follow(ctx: Ctx<Mpris>, bus: String) -> Pin<Box<dyn Stream<Item = Event> + Send>> {
    match watch(&ctx, bus.clone()).await {
        Ok(updates) => Box::pin(updates),
        Err(reason) => {
            tracing::debug!(bus, reason, "a player could not be read");
            Box::pin(stream::once(async move { Event::Gone(bus) }))
        }
    }
}

async fn watch(
    ctx: &Ctx<Mpris>,
    bus: String,
) -> Result<impl Stream<Item = Event> + Send + 'static, String> {
    let connection = ctx.session_bus().map_err(str::to_owned)?.clone();

    let root = MprisRootProxy::builder(&connection)
        .destination(bus.clone())
        .map_err(say)?
        .build()
        .await
        .map_err(say)?;
    let player = MprisPlayerProxy::builder(&connection)
        .destination(bus.clone())
        .map_err(say)?
        .build()
        .await
        .map_err(say)?;
    let properties = zbus::fdo::PropertiesProxy::builder(&connection)
        .destination(bus.clone())
        .map_err(say)?
        .path(MPRIS_PATH)
        .map_err(say)?
        .build()
        .await
        .map_err(say)?;

    let changes = properties.receive_properties_changed().await.map_err(say)?;
    let seeks = player.receive_seeked().await.map_err(say)?;

    let first = read(bus.clone(), &root, &player).await;

    let again = {
        let (bus, root, player) = (bus.clone(), root.clone(), player.clone());
        move || {
            let (bus, root, player) = (bus.clone(), root.clone(), player.clone());
            async move { read(bus, &root, &player).await }
        }
    };

    let updated = changes.then({
        let again = again.clone();
        move |_| again()
    });
    let seeked = seeks.then(move |_| again());

    Ok(stream::once(async move { first }).chain(futures_util::stream::select(updated, seeked)))
}

/// Everything is re-read on every change, because a `Player` is rebuilt whole either way and only
/// `Position` costs a round trip — every other property is answered from the proxy's cache, kept
/// fresh by the same signal that woke this up.
async fn read(bus: String, root: &MprisRootProxy<'_>, player: &MprisPlayerProxy<'_>) -> Event {
    let identity = root
        .identity()
        .await
        .map(|text| clean(&text, IDENTITY))
        .unwrap_or_default();
    let metadata = player.metadata().await.unwrap_or_default();
    let id = bus.trim_start_matches(MPRIS_NAME_PREFIX).to_owned();

    Event::Updated(Box::new(Player {
        identity: match identity.is_empty() {
            true => id.clone(),
            false => identity,
        },
        desktop_entry: root.desktop_entry().await.ok().filter(|it| !it.is_empty()),
        playback: playback(player.playback_status().await.ok().as_deref()),
        current: false,
        title: text(&metadata, "xesam:title").map(|it| clean(&it, TITLE)),
        artist: artists(&metadata).map(|it| clean(&it, ARTIST)),
        album: text(&metadata, "xesam:album").map(|it| clean(&it, ALBUM)),
        art_url: text(&metadata, "mpris:artUrl"),
        track_id: text(&metadata, "mpris:trackid"),
        length_us: number(&metadata, "mpris:length"),
        position_us: player.position().await.unwrap_or_default().max(0),
        position_at: Utc::now(),
        rate: player.rate().await.unwrap_or(1.0),
        volume: player.volume().await.ok(),
        repeat: player.loop_status().await.ok().map(|it| repeat(&it)),
        shuffle: player.shuffle().await.ok(),
        can: PlayerCapabilities {
            play: player.can_play().await.unwrap_or(false),
            pause: player.can_pause().await.unwrap_or(false),
            previous: player.can_go_previous().await.unwrap_or(false),
            next: player.can_go_next().await.unwrap_or(false),
            seek: player.can_seek().await.unwrap_or(false),
            control: player.can_control().await.unwrap_or(false),
            raise: root.can_raise().await.unwrap_or(false),
        },
        last_active: Utc::now(),
        bus,
        id,
    }))
}

/// When this player last started doing something, which is what breaks a tie between two that are
/// both playing. A re-read is not activity: every property change would otherwise refresh it, and
/// the chattiest player would win rather than the one most recently started.
fn stirred(held: Option<&Player>, player: &Player) -> DateTime<Utc> {
    match held {
        Some(held) if held.playback == player.playback => held.last_active,
        _ => player.last_active,
    }
}

fn playback(status: Option<&str>) -> Playback {
    match status {
        Some("Playing") => Playback::Playing,
        Some("Paused") => Playback::Paused,
        Some("Stopped") => Playback::Stopped,
        _ => Playback::Unknown,
    }
}

fn repeat(status: &str) -> Repeat {
    match status {
        "Track" => Repeat::Track,
        "Playlist" => Repeat::Playlist,
        "None" => Repeat::Off,
        _ => Repeat::Unknown,
    }
}

/// `mpris:trackid` is an object path by specification and a plain string in several players, and
/// an unknown vendor key is ordinary rather than an error — so every key is read for itself and
/// what does not parse is left absent.
fn text(metadata: &HashMap<String, OwnedValue>, key: &str) -> Option<String> {
    let text = match &**metadata.get(key)? {
        zbus::zvariant::Value::Str(text) => text.as_str().to_owned(),
        zbus::zvariant::Value::ObjectPath(path) => path.as_str().to_owned(),
        _ => return None,
    };
    Some(text).filter(|it| !it.is_empty())
}

/// `xesam:artist` is an array of strings, and a player sending a bare one must not cost the rest
/// of the metadata.
fn artists(metadata: &HashMap<String, OwnedValue>) -> Option<String> {
    let value = metadata.get("xesam:artist")?;
    if let Ok(many) = Vec::<String>::try_from(value.try_clone().ok()?) {
        let joined = many.join(", ");
        return Some(joined).filter(|it| !it.is_empty());
    }
    text(metadata, "xesam:artist")
}

fn number(metadata: &HashMap<String, OwnedValue>, key: &str) -> Option<i64> {
    let value = metadata.get(key)?;
    i64::try_from(value)
        .ok()
        .or_else(|| {
            u64::try_from(value)
                .ok()
                .and_then(|it| i64::try_from(it).ok())
        })
        .filter(|length| *length > 0)
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone as _;

    use super::*;

    #[test]
    fn declared_topics_and_methods_exist() {
        crate::service::assert_declarations::<Mpris>();
    }

    #[test]
    fn a_loop_status_survives_a_round_trip_through_the_wire_name() {
        for value in [Repeat::Off, Repeat::Playlist, Repeat::Track] {
            assert_eq!(repeat(status(value)), value);
        }
        assert_eq!(
            status(Repeat::Unknown),
            "None",
            "a status this daemon does not know must not be sent back as itself"
        );
    }

    #[test]
    fn a_playback_status_the_spec_does_not_name_is_unknown_rather_than_stopped() {
        assert_eq!(playback(Some("Playing")), Playback::Playing);
        assert_eq!(playback(Some("Paused")), Playback::Paused);
        assert_eq!(playback(Some("Stopped")), Playback::Stopped);
        assert_eq!(playback(Some("Buffering")), Playback::Unknown);
        assert_eq!(playback(None), Playback::Unknown);
    }

    /// The chattiest player would otherwise win every tie: a property change is not the viewer
    /// starting something, and `Position` alone moves on every read.
    #[test]
    fn a_re_read_that_changes_nothing_does_not_count_as_activity() {
        let earlier = Utc.with_ymd_and_hms(2026, 9, 9, 12, 0, 0).unwrap();
        let later = Utc.with_ymd_and_hms(2026, 9, 9, 12, 30, 0).unwrap();

        let mut held = select::tests::sample();
        held.playback = Playback::Playing;
        held.last_active = earlier;

        let mut fresh = held.clone();
        fresh.last_active = later;
        assert_eq!(
            stirred(Some(&held), &fresh),
            earlier,
            "an unchanged playback state is the same activity, however many times it is re-read"
        );

        fresh.playback = Playback::Paused;
        assert_eq!(stirred(Some(&held), &fresh), later);
        assert_eq!(
            stirred(None, &fresh),
            later,
            "a player just seen is active now"
        );
    }

    #[test]
    fn art_max_kib_is_clamped_where_the_document_meets_the_service() {
        let mut document = glimpse_config::Config::default();
        document.mpris.art_max_kib = 0;
        assert_eq!(Config::from(&document).art_max_kib, 16);

        document.mpris.art_max_kib = u32::MAX;
        assert_eq!(Config::from(&document).art_max_kib, 65_536);
    }
}
