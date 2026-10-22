use std::future::Future;
use std::sync::Arc;

use futures_util::future::BoxFuture;
use futures_util::stream::BoxStream;
use futures_util::{FutureExt, StreamExt};
use glimpse_utils::clean;
use tokio::sync::{RwLock, watch};
use tokio_util::task::AbortOnDropHandle;
use zbus::proxy::PropertyStream;
use zbus::zvariant::OwnedValue;

use super::{DEADLINE, RetryDelay, retry_after_failure};
use crate::Delivery;

pub const REASON: usize = 240;
const NO_OWNER: &str = "provider has no bus owner";

pub type Changes<T> = BoxStream<'static, BoxFuture<'static, zbus::Result<T>>>;

pub trait ProviderError: Sized {
    fn unavailable(reason: String) -> Self;
    fn timed_out() -> Self;
    fn call(reason: String) -> Self;
    fn named(name: &str, reason: String) -> Option<Self>;

    fn typed(_error: &zbus::Error) -> Option<Self> {
        None
    }
}

pub async fn call<T, E: ProviderError>(
    request: impl Future<Output = zbus::Result<T>>,
) -> Result<T, E> {
    match tokio::time::timeout(DEADLINE, request).await {
        Ok(Ok(answer)) => Ok(answer),
        Ok(Err(error)) => Err(classify(&error)),
        Err(_) => Err(E::timed_out()),
    }
}

fn classify<E: ProviderError>(error: &zbus::Error) -> E {
    if let Some(typed) = E::typed(error) {
        return typed;
    }
    let zbus::Error::MethodError(name, reason, _) = error else {
        return E::call(clean(&error.to_string(), REASON));
    };
    let reason = clean(reason.as_deref().unwrap_or_default(), REASON);
    match name.as_str() {
        "org.freedesktop.DBus.Error.ServiceUnknown"
        | "org.freedesktop.DBus.Error.NameHasNoOwner" => E::unavailable(reason),
        "org.freedesktop.DBus.Error.NoReply" => E::timed_out(),
        name => E::named(name, reason.clone()).unwrap_or_else(|| E::call(reason)),
    }
}

pub trait Follow: Clone + PartialEq + Send + Sync + 'static {
    type Proxy: Clone + Send + Sync + 'static;
    type Error: ProviderError;

    const NAME: &'static str;

    fn lost(reason: String, previous: Option<&Self>) -> Self;
    fn reason(&self) -> Option<&str>;
}

pub trait Snapshots: Follow {
    type Snapshot: Send + 'static;

    const BUS_NAME: &'static str;

    fn build(
        connection: &zbus::Connection,
    ) -> impl Future<Output = zbus::Result<Self::Proxy>> + Send;
    fn read(proxy: &Self::Proxy) -> impl Future<Output = zbus::Result<Self::Snapshot>> + Send;
    fn changes(proxy: &Self::Proxy) -> impl Future<Output = Changes<Self::Snapshot>> + Send;
    fn decode(snapshot: Self::Snapshot) -> Result<Self, String>;
}

pub fn changes<T>(stream: PropertyStream<'static, T>) -> Changes<T>
where
    T: TryFrom<OwnedValue> + Unpin + Send + Sync + 'static,
    T::Error: Into<zbus::Error>,
{
    stream
        .map(|changed| async move { changed.get().await }.boxed())
        .boxed()
}

#[derive(Clone)]
pub struct ProviderHandle<S: Follow> {
    state: watch::Receiver<S>,
    proxy: Arc<RwLock<Option<S::Proxy>>>,
}

impl<S: Follow> ProviderHandle<S> {
    pub fn snapshot(&self) -> S {
        self.state.borrow().clone()
    }

    pub fn subscribe(&self) -> watch::Receiver<S> {
        self.state.clone()
    }

    pub(crate) async fn proxy(&self) -> Result<S::Proxy, S::Error> {
        self.proxy.read().await.clone().ok_or_else(|| {
            S::Error::unavailable(self.state.borrow().reason().unwrap_or(NO_OWNER).to_owned())
        })
    }
}

pub struct Provider<S: Follow> {
    handle: ProviderHandle<S>,
    pub(crate) task: Option<AbortOnDropHandle<()>>,
}

impl<S: Follow> Provider<S> {
    pub fn unavailable(reason: impl Into<String>) -> Self {
        let (_, state) = watch::channel(S::lost(clean(&reason.into(), REASON), None));
        Self {
            handle: ProviderHandle {
                state,
                proxy: Default::default(),
            },
            task: None,
        }
    }

    pub(crate) fn spawn<F: Future<Output = ()> + Send + 'static>(
        follow: impl FnOnce(watch::Sender<S>, Arc<RwLock<Option<S::Proxy>>>) -> F,
    ) -> Self {
        let (updates, state) = watch::channel(S::lost(NO_OWNER.to_owned(), None));
        let proxy = Arc::new(RwLock::new(None));
        let task = AbortOnDropHandle::new(tokio::spawn(follow(updates, proxy.clone())));
        Self {
            handle: ProviderHandle { state, proxy },
            task: Some(task),
        }
    }

    pub fn handle(&self) -> ProviderHandle<S> {
        self.handle.clone()
    }

    pub async fn shutdown(&mut self) {
        if let Some(task) = self.task.take() {
            task.abort();
            let _ = task.await;
            tracing::debug!(provider = S::NAME, "provider follower stopped");
        }
    }
}

impl<S: Snapshots> Provider<S> {
    pub fn start(connection: zbus::Connection) -> Self {
        Self::spawn(|updates, current| follow_snapshots(connection, updates, current))
    }
}

pub fn replace_unavailable<S: Follow>(updates: &watch::Sender<S>, reason: impl AsRef<str>) {
    let reason = clean(reason.as_ref(), REASON);
    updates.send_if_modified(|previous| {
        let next = S::lost(reason, Some(previous));
        if *previous == next {
            return false;
        }
        *previous = next;
        true
    });
}

pub async fn follow_snapshots<S: Snapshots>(
    connection: zbus::Connection,
    updates: watch::Sender<S>,
    current: Arc<RwLock<Option<S::Proxy>>>,
) {
    let name = S::NAME;
    let dbus = match zbus::fdo::DBusProxy::new(&connection).await {
        Ok(dbus) => dbus,
        Err(error) => {
            replace_unavailable(&updates, error.to_string());
            tracing::warn!(%error, provider = name, "cannot observe the provider owner");
            return;
        }
    };
    let mut owners = match super::owners(&dbus, S::BUS_NAME).await {
        Ok(owners) => owners,
        Err(error) => {
            replace_unavailable(&updates, error.to_string());
            tracing::warn!(%error, provider = name, "cannot observe provider owner changes");
            return;
        }
    };

    let mut retry_delay = RetryDelay::default();

    loop {
        let proxy = match S::build(&connection).await {
            Ok(proxy) => {
                retry_delay.reset();
                proxy
            }
            Err(error) => {
                *current.write().await = None;
                replace_unavailable(&updates, error.to_string());
                tracing::debug!(
                    provider = name,
                    reason = clean(&error.to_string(), REASON),
                    "provider property cache is unavailable"
                );
                if !retry_after_failure(&dbus, &mut owners, S::BUS_NAME, &mut retry_delay).await {
                    return;
                }
                continue;
            }
        };
        let mut changes = S::changes(&proxy).await;
        let mut serving = match S::read(&proxy)
            .await
            .map_err(|error| error.to_string())
            .and_then(S::decode)
        {
            Ok(state) => {
                *current.write().await = Some(proxy.clone());
                tracing::info!(provider = name, "provider connected");
                updates.send_replace(state);
                true
            }
            Err(reason) => {
                *current.write().await = None;
                let reason = clean(&reason, REASON);
                replace_unavailable(&updates, &reason);
                tracing::warn!(provider = name, reason, "provider snapshot is unavailable");
                false
            }
        };

        let reason = loop {
            tokio::select! {
                changed = changes.next() => {
                    let Some(changed) = changed else {
                        break format!("{name} snapshot stream ended");
                    };
                    match changed
                        .await
                        .map_err(|error| error.to_string())
                        .and_then(S::decode)
                    {
                        Ok(state) => {
                            if !serving {
                                *current.write().await = Some(proxy.clone());
                                serving = true;
                                tracing::info!(provider = name, "provider recovered without changing owner");
                            }
                            tracing::debug!(provider = name, "provider snapshot changed");
                            updates.send_replace(state);
                        }
                        Err(reason) => {
                            let reason = clean(&reason, REASON);
                            tracing::warn!(provider = name, reason, "provider snapshot change failed");
                            if serving {
                                *current.write().await = None;
                                serving = false;
                            }
                            replace_unavailable(&updates, &reason);
                        }
                    }
                }
                owner = owners.next() => {
                    let Some(owner) = owner else {
                        break format!("{name} provider owner stream ended");
                    };
                    let (old_owner, new_owner) = match owner {
                        Delivery::Item(change) => change,
                        Delivery::Missed => break "provider owner changed".to_owned(),
                    };
                    if old_owner {
                        break match new_owner {
                            true => "provider owner changed",
                            false => NO_OWNER,
                        }.to_owned();
                    }
                    if !serving && new_owner {
                        break "provider took the name".to_owned();
                    }
                }
            }
        };
        if serving {
            *current.write().await = None;
            replace_unavailable(&updates, &reason);
            tracing::warn!(provider = name, reason, "provider disconnected");
        } else {
            tracing::debug!(provider = name, reason, "provider rebuilding its proxy");
        }
    }
}
