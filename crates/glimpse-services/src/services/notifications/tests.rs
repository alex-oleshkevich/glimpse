use glimpse_dbus::Buses;

use tokio_util::sync::CancellationToken;

use super::*;

use crate::service::ServiceRuntime;

fn incoming(app: &str, summary: &str) -> Incoming {
    Incoming {
        app_name: app.to_owned(),
        app_id: app.to_owned(),
        app_pid: None,
        icon: None,
        image: None,
        summary: summary.to_owned(),
        body: String::new(),
        actions: Vec::new(),
        urgency: NotificationUrgency::Normal,
        progress: None,
        resident: false,
        expire_timeout: -1,
        transient: false,
    }
}

fn store(keep: usize, posted: &[(u32, &str, &str)]) -> Store {
    let mut store = Store::new(keep);
    for (id, app, summary) in posted {
        store.post(*id, incoming(app, summary));
    }
    store
}

async fn dnd_service() -> (
    NotificationsHandle,
    crate::service::ServiceSender<Notifications>,
    tokio::sync::watch::Receiver<NotificationsState>,
    CancellationToken,
    tokio::task::JoinHandle<Result<(), ServiceError>>,
) {
    let cancel = CancellationToken::new();
    let (mut runtime, handle) = ServiceRuntime::<Notifications>::new(
        Config {
            keep: 10,
            suppress: Vec::new(),
        },
        Buses::unavailable("no bus in tests"),
        cancel.clone(),
    );
    let state = handle.subscribe();
    let sender = runtime.sender();
    let running = tokio::spawn(async move { runtime.run(()).await });
    (handle, sender, state, cancel, running)
}

fn enabled(state: &NotificationsState) -> bool {
    state.dnd.as_ref().is_some_and(|dnd| dnd.dnd.enabled)
}

fn image_data(
    (width, height, rowstride): (i32, i32, i32),
    alpha: bool,
    bits: i32,
    channels: i32,
    data: Vec<u8>,
) -> HashMap<String, OwnedValue> {
    let value = zbus::zvariant::Value::from(zbus::zvariant::Structure::from((
        width, height, rowstride, alpha, bits, channels, data,
    )));
    HashMap::from([(
        "image-data".to_owned(),
        OwnedValue::try_from(value).expect("an owned value"),
    )])
}

mod dnd;
mod history;
mod identity;
mod images;
mod limits;
