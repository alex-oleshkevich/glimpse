use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use zbus::Connection;
use zbus::object_server::SignalEmitter;
use zbus::zvariant::OwnedObjectPath;

pub const MANAGER_PATH: &str = "/org/freedesktop/GeoClue2/Manager";
pub const SERVICE_NAME: &str = "org.freedesktop.GeoClue2";
/// The literal path a real GeoClue client's `Location` property carries before any fix arrives.
pub const NO_FIX: &str = "/";

/// A recorded call, in the shape of `testing::tray::Call`: enough for a test to assert which
/// calls landed and in what order, which is the whole point of a fake over a stub.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Call {
    GetClient,
    CreateClient,
    Start(OwnedObjectPath),
    Stop(OwnedObjectPath),
    DeleteClient(OwnedObjectPath),
}

type Calls = Arc<Mutex<Vec<Call>>>;

struct ClientState {
    desktop_id: String,
    accuracy: u32,
    location: OwnedObjectPath,
}

impl ClientState {
    fn new() -> Self {
        Self {
            desktop_id: String::new(),
            accuracy: 0,
            location: OwnedObjectPath::try_from(NO_FIX).expect("a literal path"),
        }
    }
}

#[derive(Default)]
struct Registry {
    next_client: u32,
    next_location: u32,
    /// The client handed to each peer so far, keyed by the caller's unique name: `GetClient`
    /// hands back the same path until that client is deleted, which is the behaviour `.1`
    /// reasons about.
    peers: HashMap<String, OwnedObjectPath>,
    /// Every client currently started, which is what makes `InUse` honest rather than a stub
    /// that always answers `false`.
    started: std::collections::HashSet<OwnedObjectPath>,
    states: HashMap<OwnedObjectPath, Arc<Mutex<ClientState>>>,
}

/// Flips a test can throw to make the next `Location` property read fail, for the
/// transient-error case.
#[derive(Default)]
struct Control {
    fail_location_reads: bool,
}

#[derive(Clone)]
struct Manager {
    connection: Connection,
    registry: Arc<Mutex<Registry>>,
    calls: Calls,
}

impl Manager {
    async fn spawn_client(&self) -> zbus::fdo::Result<OwnedObjectPath> {
        let ordinal = {
            let mut registry = self.registry.lock().expect("lock poisoned");
            registry.next_client += 1;
            registry.next_client
        };
        let path = OwnedObjectPath::try_from(format!("/org/freedesktop/GeoClue2/Client/{ordinal}"))
            .expect("a literal path");
        let state = Arc::new(Mutex::new(ClientState::new()));
        let client = Client {
            path: path.clone(),
            registry: self.registry.clone(),
            calls: self.calls.clone(),
            state: state.clone(),
        };
        self.connection
            .object_server()
            .at(path.as_ref(), client)
            .await
            .map_err(|error| zbus::fdo::Error::Failed(error.to_string()))?;
        self.registry
            .lock()
            .expect("lock poisoned")
            .states
            .insert(path.clone(), state);
        Ok(path)
    }
}

#[zbus::interface(name = "org.freedesktop.GeoClue2.Manager")]
impl Manager {
    async fn get_client(
        &self,
        #[zbus(header)] header: zbus::message::Header<'_>,
    ) -> zbus::fdo::Result<OwnedObjectPath> {
        self.calls
            .lock()
            .expect("lock poisoned")
            .push(Call::GetClient);
        let sender = header.sender().map(ToString::to_string).unwrap_or_default();
        let existing = self
            .registry
            .lock()
            .expect("lock poisoned")
            .peers
            .get(&sender)
            .cloned();
        if let Some(path) = existing {
            return Ok(path);
        }
        let path = self.spawn_client().await?;
        self.registry
            .lock()
            .expect("lock poisoned")
            .peers
            .insert(sender, path.clone());
        Ok(path)
    }

    async fn create_client(&self) -> zbus::fdo::Result<OwnedObjectPath> {
        self.calls
            .lock()
            .expect("lock poisoned")
            .push(Call::CreateClient);
        self.spawn_client().await
    }

    async fn delete_client(&self, client: OwnedObjectPath) -> zbus::fdo::Result<()> {
        self.calls
            .lock()
            .expect("lock poisoned")
            .push(Call::DeleteClient(client.clone()));
        {
            let mut registry = self.registry.lock().expect("lock poisoned");
            registry.started.remove(&client);
            registry.states.remove(&client);
            registry.peers.retain(|_, path| path != &client);
        }
        let _ = self
            .connection
            .object_server()
            .remove::<Client, _>(client.as_ref())
            .await;
        Ok(())
    }

    #[zbus(property, name = "InUse")]
    async fn in_use(&self) -> bool {
        !self
            .registry
            .lock()
            .expect("lock poisoned")
            .started
            .is_empty()
    }
}

struct Client {
    path: OwnedObjectPath,
    registry: Arc<Mutex<Registry>>,
    calls: Calls,
    state: Arc<Mutex<ClientState>>,
}

#[zbus::interface(name = "org.freedesktop.GeoClue2.Client")]
impl Client {
    async fn start(&self) -> zbus::fdo::Result<()> {
        self.calls
            .lock()
            .expect("lock poisoned")
            .push(Call::Start(self.path.clone()));
        self.registry
            .lock()
            .expect("lock poisoned")
            .started
            .insert(self.path.clone());
        Ok(())
    }

    async fn stop(&self) -> zbus::fdo::Result<()> {
        self.calls
            .lock()
            .expect("lock poisoned")
            .push(Call::Stop(self.path.clone()));
        self.registry
            .lock()
            .expect("lock poisoned")
            .started
            .remove(&self.path);
        Ok(())
    }

    #[zbus(property, name = "DesktopId")]
    async fn desktop_id(&self) -> String {
        self.state.lock().expect("lock poisoned").desktop_id.clone()
    }

    #[zbus(property, name = "DesktopId")]
    async fn set_desktop_id(&self, value: String) {
        self.state.lock().expect("lock poisoned").desktop_id = value;
    }

    #[zbus(property, name = "RequestedAccuracyLevel")]
    async fn requested_accuracy_level(&self) -> u32 {
        self.state.lock().expect("lock poisoned").accuracy
    }

    #[zbus(property, name = "RequestedAccuracyLevel")]
    async fn set_requested_accuracy_level(&self, value: u32) {
        self.state.lock().expect("lock poisoned").accuracy = value;
    }

    #[zbus(property, name = "Location")]
    async fn location(&self) -> OwnedObjectPath {
        self.state.lock().expect("lock poisoned").location.clone()
    }

    #[zbus(signal, name = "LocationUpdated")]
    async fn location_updated(
        emitter: &SignalEmitter<'_>,
        old: OwnedObjectPath,
        new: OwnedObjectPath,
    ) -> zbus::Result<()>;
}

#[derive(Clone)]
struct Location {
    latitude: f64,
    longitude: f64,
    control: Arc<Mutex<Control>>,
}

#[zbus::interface(name = "org.freedesktop.GeoClue2.Location")]
impl Location {
    #[zbus(property, name = "Latitude")]
    async fn latitude(&self) -> zbus::fdo::Result<f64> {
        if self
            .control
            .lock()
            .expect("lock poisoned")
            .fail_location_reads
        {
            return Err(zbus::fdo::Error::Failed(
                "transient location read failure".to_owned(),
            ));
        }
        Ok(self.latitude)
    }

    #[zbus(property, name = "Longitude")]
    async fn longitude(&self) -> zbus::fdo::Result<f64> {
        if self
            .control
            .lock()
            .expect("lock poisoned")
            .fail_location_reads
        {
            return Err(zbus::fdo::Error::Failed(
                "transient location read failure".to_owned(),
            ));
        }
        Ok(self.longitude)
    }
}

/// A GeoClue2 `Manager` on a bus of the test's own, with a recorded call log and `InUse`
/// bookkeeping the fake maintains honestly rather than a stub that always answers `false`.
/// Dropping it leaves the objects up: the connection owns them, so drop that to take the peer
/// away.
pub struct FakeGeoClue {
    connection: Connection,
    registry: Arc<Mutex<Registry>>,
    calls: Calls,
    control: Arc<Mutex<Control>>,
}

impl FakeGeoClue {
    pub async fn start(connection: Connection) -> zbus::Result<Self> {
        let registry = Arc::new(Mutex::new(Registry::default()));
        let calls = Arc::new(Mutex::new(Vec::new()));
        let control = Arc::new(Mutex::new(Control::default()));
        connection
            .object_server()
            .at(
                MANAGER_PATH,
                Manager {
                    connection: connection.clone(),
                    registry: registry.clone(),
                    calls: calls.clone(),
                },
            )
            .await?;
        crate::own_name(&connection, SERVICE_NAME).await?;
        Ok(Self {
            connection,
            registry,
            calls,
            control,
        })
    }

    pub fn calls(&self) -> Vec<Call> {
        self.calls.lock().expect("lock poisoned").clone()
    }

    pub fn in_use(&self) -> bool {
        !self
            .registry
            .lock()
            .expect("lock poisoned")
            .started
            .is_empty()
    }

    /// Fail every subsequent `Location.Latitude`/`Longitude` read, for the transient-error case.
    /// Call again with `false` to let reads succeed again.
    pub fn fail_location_reads(&self, fail: bool) {
        self.control
            .lock()
            .expect("lock poisoned")
            .fail_location_reads = fail;
    }

    /// Push a fix to `client`: exports a fresh `Location` object, points the client's `Location`
    /// property at it, and both emits `LocationUpdated` and republishes the `Location` property
    /// itself — a real fix does both, and it is the property's own `PropertiesChanged` that
    /// `GeoClueClientProxy::receive_location_changed` actually follows.
    pub async fn push_fix(
        &self,
        client: &OwnedObjectPath,
        latitude: f64,
        longitude: f64,
    ) -> zbus::Result<()> {
        let state = self
            .registry
            .lock()
            .expect("lock poisoned")
            .states
            .get(client)
            .cloned()
            .ok_or(zbus::Error::InterfaceNotFound)?;

        let ordinal = {
            let mut registry = self.registry.lock().expect("lock poisoned");
            registry.next_location += 1;
            registry.next_location
        };
        let location_path =
            OwnedObjectPath::try_from(format!("/org/freedesktop/GeoClue2/Location/{ordinal}"))
                .expect("a literal path");
        self.connection
            .object_server()
            .at(
                location_path.as_ref(),
                Location {
                    latitude,
                    longitude,
                    control: self.control.clone(),
                },
            )
            .await?;

        let old = {
            let mut state = state.lock().expect("lock poisoned");
            let old = state.location.clone();
            state.location = location_path.clone();
            old
        };

        Client::location_updated(
            &SignalEmitter::new(&self.connection, client.as_ref())?,
            old,
            location_path,
        )
        .await?;

        let reference = self
            .connection
            .object_server()
            .interface::<_, Client>(client.as_ref())
            .await?;
        reference
            .get()
            .await
            .location_changed(reference.signal_emitter())
            .await
    }
}
