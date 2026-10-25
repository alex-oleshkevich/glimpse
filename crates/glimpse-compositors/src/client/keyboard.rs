use std::collections::VecDeque;
use std::time::Duration;

use glimpse_dbus::Delivery;
use tokio::sync::mpsc;
use zbus::fdo::PropertiesProxy;
use zbus::message::{Sequence, Type};

use super::*;
use crate::contract::OBJECT_PATH;

const INTERFACE: &str = "me.aresa.Glimpse.Compositor1";

pub enum KeyboardUpdate {
    State(Box<CompositorState>),
    Barrier(u64),
}

#[derive(Clone)]
pub struct KeyboardBarrier(mpsc::Sender<u64>);

impl KeyboardBarrier {
    pub async fn request(&self, id: u64) -> Result<(), Error> {
        self.0
            .send(id)
            .await
            .map_err(|_| Error::Unavailable("the keyboard event stream ended".to_owned()))
    }
}

struct Watching {
    properties: PropertiesProxy<'static>,
    changes: BoxStream<'static, Delivery<Result<zbus::Message, String>>>,
    owners: BoxStream<'static, ()>,
    after: Sequence,
    state: CompositorState,
    pending: VecDeque<KeyboardUpdate>,
}

pub(super) fn updates(handle: Handle) -> (KeyboardBarrier, BoxStream<'static, KeyboardUpdate>) {
    let (barrier, barriers) = mpsc::channel(8);
    let updates = stream::unfold(
        (handle, None::<Watching>, false, barriers),
        |(handle, watching, retry, mut barriers)| async move {
            if let Some(mut watching) = watching {
                match watching.next(&mut barriers).await {
                    Ok(update) => return Some((update, (handle, Some(watching), false, barriers))),
                    Err(error) => {
                        return Some((
                            KeyboardUpdate::State(Box::new(CompositorState::lost(
                                error.to_string(),
                                None,
                            ))),
                            (handle, None, true, barriers),
                        ));
                    }
                }
            }
            if retry {
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
            let mut state = handle.subscribe();
            loop {
                if state.borrow().unavailable.is_none() {
                    break;
                }
                if state.changed().await.is_err() {
                    return None;
                }
            }
            let result = match handle.0.proxy().await {
                Ok(proxy) => Watching::start(&proxy).await,
                Err(error) => Err(zbus::Error::Failure(error.to_string())),
            };
            match result {
                Ok(watching) => Some((
                    KeyboardUpdate::State(Box::new(watching.state.clone())),
                    (handle, Some(watching), false, barriers),
                )),
                Err(error) => Some((
                    KeyboardUpdate::State(Box::new(CompositorState::lost(error.to_string(), None))),
                    (handle, None, true, barriers),
                )),
            }
        },
    )
    .boxed();
    (KeyboardBarrier(barrier), updates)
}

impl Watching {
    async fn start(proxy: &Compositor1Proxy<'static>) -> zbus::Result<Self> {
        let owners = glimpse_dbus::wake(proxy.inner().receive_owner_changed().await?);
        let bus = zbus::fdo::DBusProxy::new(proxy.inner().connection()).await?;
        let owner = bus.get_name_owner(BUS_NAME.try_into()?).await?;
        let properties = PropertiesProxy::builder(proxy.inner().connection())
            .destination(owner.clone())?
            .path(OBJECT_PATH)?
            .build()
            .await?;
        let rule = zbus::MatchRule::builder().sender(owner)?.build();
        let changes = glimpse_dbus::events(
            zbus::MessageStream::for_match_rule(rule, proxy.inner().connection(), None)
                .await?
                .filter_map(|message| async move {
                    match message {
                        Ok(message) if relevant(&message) => Some(Ok(message)),
                        Ok(_) => None,
                        Err(error) => Some(Err(error.to_string())),
                    }
                }),
        );
        let reply = properties
            .inner()
            .call_method("GetAll", &(INTERFACE,))
            .await?;
        let mut state = CompositorState::default();
        apply(&mut state, reply.body().deserialize()?)?;
        Ok(Self {
            properties,
            changes,
            owners,
            after: reply.recv_position(),
            state,
            pending: VecDeque::new(),
        })
    }

    async fn next(&mut self, barriers: &mut mpsc::Receiver<u64>) -> zbus::Result<KeyboardUpdate> {
        loop {
            if let Some(update) = self.pending.pop_front() {
                return Ok(update);
            }
            tokio::select! {
                _ = self.owners.next() => {
                    return Err(zbus::Error::Failure("the compositor provider owner changed".to_owned()));
                }
                message = self.changes.next() => {
                    let message = delivered(message)?;
                    if message.recv_position() <= self.after {
                        continue;
                    }
                    if self.apply_message(&message)? {
                        return Ok(KeyboardUpdate::State(Box::new(self.state.clone())));
                    }
                }
                Some(id) = barriers.recv() => self.readback(id).await?,
            }
        }
    }

    fn apply_message(&mut self, message: &zbus::Message) -> zbus::Result<bool> {
        if message.message_type() != Type::Signal {
            return Ok(false);
        }
        let (interface, changed, invalidated): (String, HashMap<String, OwnedValue>, Vec<String>) =
            message.body().deserialize()?;
        if interface != INTERFACE {
            return Ok(false);
        }
        if invalidated
            .iter()
            .any(|name| matches!(name.as_str(), "Windows" | "Focus" | "Keyboard"))
        {
            return Err(zbus::Error::Failure(
                "the compositor keyboard properties were invalidated".to_owned(),
            ));
        }
        apply(&mut self.state, changed)
    }

    async fn readback(&mut self, id: u64) -> zbus::Result<()> {
        let reply = self
            .properties
            .inner()
            .call_method("GetAll", &(INTERFACE,))
            .await?;
        let boundary = reply.recv_position();
        loop {
            let message = delivered(self.changes.next().await)?;
            if message.recv_position() <= self.after {
                continue;
            }
            if message.recv_position() == boundary {
                break;
            }
            if message.recv_position() > boundary {
                return Err(zbus::Error::Failure(
                    "the keyboard completion marker was missed".to_owned(),
                ));
            }
            if self.apply_message(&message)? {
                self.pending
                    .push_back(KeyboardUpdate::State(Box::new(self.state.clone())));
            }
        }
        apply(&mut self.state, reply.body().deserialize()?)?;
        self.pending
            .push_back(KeyboardUpdate::State(Box::new(self.state.clone())));
        self.pending.push_back(KeyboardUpdate::Barrier(id));
        self.after = boundary;
        Ok(())
    }
}

fn relevant(message: &zbus::Message) -> bool {
    if message.message_type() == Type::MethodReturn {
        return true;
    }
    let header = message.header();
    message.message_type() == Type::Signal
        && header
            .interface()
            .is_some_and(|name| name.as_str() == "org.freedesktop.DBus.Properties")
        && header
            .member()
            .is_some_and(|name| name.as_str() == "PropertiesChanged")
        && header
            .path()
            .is_some_and(|path| path.as_str() == OBJECT_PATH)
}

fn delivered(
    message: Option<Delivery<Result<zbus::Message, String>>>,
) -> zbus::Result<zbus::Message> {
    match message {
        Some(Delivery::Item(message)) => message.map_err(zbus::Error::Failure),
        Some(Delivery::Missed) => Err(zbus::Error::Failure(
            "the keyboard event stream missed changes".to_owned(),
        )),
        None => Err(zbus::Error::Failure(
            "the keyboard event stream ended".to_owned(),
        )),
    }
}

fn apply(state: &mut CompositorState, changed: HashMap<String, OwnedValue>) -> zbus::Result<bool> {
    let mut relevant = false;
    for (name, value) in changed {
        match name.as_str() {
            "Windows" => {
                let items = Vec::<Item>::try_from(value)?;
                state.windows = items.iter().filter_map(window).collect();
            }
            "Focus" => {
                let item = Item::try_from(value)?;
                state.focus = Focus {
                    output: text(&item, key::OUTPUT, capped_name_str),
                    window: get(&item, key::WINDOW).map(WindowId),
                };
            }
            "Keyboard" => {
                let item = Item::try_from(value)?;
                state.keyboard = KeyboardLayouts {
                    names: names(&item, key::NAMES),
                    codes: names(&item, key::CODES),
                    current: get(&item, key::CURRENT),
                };
            }
            _ => continue,
        }
        relevant = true;
    }
    Ok(relevant)
}
