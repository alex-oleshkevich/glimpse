use std::{
    collections::HashSet,
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    io::Read,
    os::fd::AsFd,
    sync::Arc,
    time::Duration,
};

use serde::{Deserialize, Serialize};
use tokio::{
    io::{Interest, unix::AsyncFd},
    sync::{mpsc, watch},
};
use tokio_util::sync::CancellationToken;
use wayland_client::{
    Connection, Dispatch, QueueHandle,
    globals::{GlobalListContents, registry_queue_init},
    protocol::{wl_registry, wl_seat},
    event_created_child,
};
use wayland_protocols_wlr::data_control::v1::client::{
    zwlr_data_control_device_v1,
    zwlr_data_control_manager_v1,
    zwlr_data_control_offer_v1,
};
use wl_clipboard_rs::{
    copy::{
        ClipboardType as CopyClipboardType, MimeType as CopyMimeType, Options as CopyOptions,
        Seat as CopySeat, Source as CopySource, clear as clear_clipboard,
    },
    paste::{
        ClipboardType as PasteClipboardType, Error as PasteError, MimeType as PasteMimeType,
        Seat as PasteSeat, get_contents, get_mime_types_ordered,
    },
};

use crate::services::framework::{Control, ServiceCommand, ServiceHandle};

const COMMAND_QUEUE_SIZE: usize = 32;
const WATCHER_EVENT_QUEUE_SIZE: usize = 8;
const WATCHER_RETRY_DELAY: Duration = Duration::from_secs(2);
const WATCHER_SETUP_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_READ_BYTES: u64 = 2 * 1024 * 1024;
const MAX_PREVIEW_CHARS: usize = 240;
const MAX_HISTORY_BYTES: usize = 10 * 1024 * 1024;
const PASSWORD_HINT_MIME: &str = "x-kde-passwordManagerHint";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "kebab-case")]
pub enum ClipboardEntryKind {
    Text,
    Html,
    Image,
    Files,
    Other,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ClipboardEntry {
    pub id: u64,
    pub kind: ClipboardEntryKind,
    pub mime_type: String,
    pub mime_types: Vec<String>,
    pub preview: String,
    pub size: u64,
    pub timestamp: u64,
    #[serde(skip)]
    data: Arc<[u8]>,
    fingerprint: u64,
}

impl ClipboardEntry {
    pub fn data(&self) -> &[u8] {
        &self.data
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct State {
    pub available: bool,
    pub history: Vec<ClipboardEntry>,
    pub current_id: Option<u64>,
    pub health: Health,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum Health {
    #[default]
    Starting,
    Ready,
    Degraded(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Refresh,
    Select(u64),
    Remove(u64),
    ClearHistory,
    ClearClipboard,
}

pub type ClipboardHandle = ServiceHandle<State, Command>;

pub struct ClipboardService {
    state_tx: watch::Sender<State>,
    command_rx: mpsc::Receiver<ServiceCommand<Command>>,
    backend: WlClipboardBackend,
    state: State,
    next_id: u64,
    suppressed_current_fingerprints: HashSet<u64>,
}

impl ClipboardService {
    pub fn new() -> (Self, ClipboardHandle) {
        let (state_tx, state_rx) = watch::channel(State::default());
        let (command_tx, command_rx) = mpsc::channel(COMMAND_QUEUE_SIZE);
        (
            Self {
                state_tx,
                command_rx,
                backend: WlClipboardBackend,
                state: State::default(),
                next_id: 1,
                suppressed_current_fingerprints: HashSet::new(),
            },
            ServiceHandle::new(state_rx, command_tx),
        )
    }

    pub async fn run(mut self, cancel: CancellationToken) {
        self.publish(State {
            health: Health::Starting,
            ..self.state.clone()
        });
        self.refresh().await;

        let (sel_tx, mut sel_rx) = mpsc::channel::<SelectionEvent>(WATCHER_EVENT_QUEUE_SIZE);
        let watcher_cancel = cancel.clone();
        tokio::spawn(async move {
            loop {
                match run_watcher(sel_tx.clone(), watcher_cancel.clone()).await {
                    Ok(()) => break,
                    Err(error) => {
                        tracing::warn!(%error, "clipboard watcher failed");
                        tokio::select! {
                            _ = watcher_cancel.cancelled() => break,
                            _ = tokio::time::sleep(WATCHER_RETRY_DELAY) => {}
                        }
                    }
                }
            }
        });

        loop {
            tokio::select! {
                _ = cancel.cancelled() => break,
                event = sel_rx.recv() => match event {
                    Some(SelectionEvent::Changed) => self.refresh().await,
                    Some(SelectionEvent::Cleared) => self.handle_clipboard_cleared(),
                    None => break, // watcher task exited (cancel or error)
                },
                command = self.command_rx.recv() => match command {
                    Some(ServiceCommand::Control(Control::Shutdown)) | None => {
                        break;
                    }
                    Some(ServiceCommand::Control(Control::Start(_) | Control::Reconfigure(_)))
                    | Some(ServiceCommand::Command(Command::Refresh)) => self.refresh().await,
                    Some(ServiceCommand::Command(command)) => self.handle_command(command).await,
                }
            }
        }
    }

    fn handle_clipboard_cleared(&mut self) {
        let changed = !self.state.available
            || self.state.current_id.is_some()
            || self.state.health != Health::Ready;
        self.state.available = true;
        self.state.current_id = None;
        self.state.health = Health::Ready;
        if changed {
            self.publish_current();
        }
    }

    async fn handle_command(&mut self, command: Command) {
        match command {
            Command::Refresh => self.refresh().await,
            Command::Select(id) => {
                let Some(entry) = self
                    .state
                    .history
                    .iter()
                    .find(|entry| entry.id == id)
                    .cloned()
                else {
                    return;
                };

                if let Err(error) = self.backend.copy_entry(entry).await {
                    self.degrade(format!("failed to copy clipboard entry: {error}"));
                } else {
                    self.refresh().await;
                }
            }
            Command::Remove(id) => {
                let removed = remove_history_entry(&mut self.state, id);
                if let Some(fingerprint) = removed {
                    self.suppressed_current_fingerprints.insert(fingerprint);
                    self.publish_current();
                }
            }
            Command::ClearHistory => {
                self.state.history.clear();
                self.state.current_id = None;
                self.publish_current();
            }
            Command::ClearClipboard => {
                if let Err(error) = self.backend.clear().await {
                    self.degrade(format!("failed to clear clipboard: {error}"));
                } else {
                    self.suppressed_current_fingerprints.clear();
                    self.state.current_id = None;
                    self.publish_current();
                }
            }
        }
    }

    async fn refresh(&mut self) {
        match self.backend.read_current().await {
            Ok(Some(snapshot)) => {
                let entry = entry_from_snapshot(self.next_id, snapshot, now_ms());
                let suppressed = self
                    .suppressed_current_fingerprints
                    .contains(&entry.fingerprint);
                let status_changed =
                    !self.state.available || self.state.health != Health::Ready;
                let history_changed = if suppressed {
                    self.state.current_id = None;
                    false
                } else {
                    self.suppressed_current_fingerprints.clear();
                    apply_clipboard_entry(&mut self.state, entry)
                };
                if history_changed {
                    self.next_id += 1;
                }
                self.state.available = true;
                self.state.health = Health::Ready;
                if status_changed || history_changed {
                    self.publish_current();
                }
            }
            Ok(None) => {
                // empty clipboard or filtered entry (password hint) — no history change
                let status_changed =
                    !self.state.available || self.state.health != Health::Ready;
                self.state.available = true;
                self.state.health = Health::Ready;
                if status_changed {
                    self.publish_current();
                }
            }
            Err(error) => {
                self.degrade(error.to_string());
            }
        }
    }

    fn publish_current(&self) {
        self.publish(self.state.clone());
    }

    fn publish(&self, state: State) {
        if let Err(error) = self.state_tx.send(state) {
            tracing::warn!(%error, "failed to publish clipboard state");
        }
    }

    fn degrade(&mut self, message: String) {
        if !self.state.available && self.state.health == Health::Degraded(message.clone()) {
            return;
        }

        self.state.available = false;
        self.state.health = Health::Degraded(message.clone());
        tracing::warn!(%message, "clipboard service degraded");
        self.publish_current();
    }
}

struct WlClipboardBackend;

impl WlClipboardBackend {
    async fn read_current(&self) -> anyhow::Result<Option<ClipboardSnapshot>> {
        tokio::task::spawn_blocking(read_current_clipboard)
            .await
            .map_err(|error| anyhow::anyhow!("clipboard read task failed: {error}"))?
    }

    async fn copy_entry(&self, entry: ClipboardEntry) -> anyhow::Result<()> {
        tokio::task::spawn_blocking(move || copy_clipboard_entry(&entry))
            .await
            .map_err(|error| anyhow::anyhow!("clipboard copy task failed: {error}"))?
    }

    async fn clear(&self) -> anyhow::Result<()> {
        tokio::task::spawn_blocking(|| {
            clear_clipboard(CopyClipboardType::Regular, CopySeat::All)?;
            Ok::<_, anyhow::Error>(())
        })
        .await
        .map_err(|error| anyhow::anyhow!("clipboard clear task failed: {error}"))?
    }
}

#[derive(Debug, Clone)]
struct ClipboardSnapshot {
    mime_type: String,
    mime_types: Vec<String>,
    data: Vec<u8>,
}

fn read_current_clipboard() -> anyhow::Result<Option<ClipboardSnapshot>> {
    let mime_types =
        match get_mime_types_ordered(PasteClipboardType::Regular, PasteSeat::Unspecified) {
            Ok(mime_types) => mime_types,
            Err(PasteError::ClipboardEmpty | PasteError::NoMimeType | PasteError::NoSeats) => {
                return Ok(None);
            }
            Err(error) => return Err(error.into()),
        };

    if mime_types.is_empty() {
        return Ok(None);
    }

    if mime_types
        .iter()
        .any(|m| m.eq_ignore_ascii_case(PASSWORD_HINT_MIME))
    {
        return Ok(None);
    }

    let preferred = preferred_mime_type(&mime_types);
    let paste_mime = preferred
        .as_deref()
        .map(PasteMimeType::Specific)
        .unwrap_or(PasteMimeType::Any);
    let (mut pipe, mime_type) = match get_contents(
        PasteClipboardType::Regular,
        PasteSeat::Unspecified,
        paste_mime,
    ) {
        Ok(result) => result,
        Err(PasteError::ClipboardEmpty | PasteError::NoMimeType | PasteError::NoSeats) => {
            return Ok(None);
        }
        Err(error) => return Err(error.into()),
    };

    let mut data = Vec::new();
    let limit = MAX_READ_BYTES + 1;
    pipe.by_ref().take(limit).read_to_end(&mut data)?;
    if data.len() as u64 > MAX_READ_BYTES {
        tracing::debug!(
            mime_type,
            max_bytes = MAX_READ_BYTES,
            "clipboard entry exceeds read limit, ignoring"
        );
        return Ok(None);
    }

    Ok(Some(ClipboardSnapshot {
        mime_type,
        mime_types,
        data,
    }))
}

fn copy_clipboard_entry(entry: &ClipboardEntry) -> anyhow::Result<()> {
    let mut options = CopyOptions::new();
    options.clipboard(CopyClipboardType::Regular);
    options.seat(CopySeat::All);

    let mime_type = if entry.kind == ClipboardEntryKind::Text {
        CopyMimeType::Text
    } else {
        CopyMimeType::Specific(entry.mime_type.clone())
    };
    options.copy(
        CopySource::Bytes(Box::from(entry.data.as_ref())),
        mime_type,
    )?;
    Ok(())
}

fn preferred_mime_type(mime_types: &[String]) -> Option<String> {
    const PREFERRED: &[&str] = &[
        "text/plain;charset=utf-8",
        "text/plain",
        "text/html",
        "image/png",
        "image/jpeg",
    ];

    PREFERRED
        .iter()
        .find_map(|preferred| {
            mime_types
                .iter()
                .find(|mime| mime.eq_ignore_ascii_case(preferred))
        })
        .cloned()
        .or_else(|| mime_types.first().cloned())
}

fn entry_from_snapshot(id: u64, snapshot: ClipboardSnapshot, timestamp: u64) -> ClipboardEntry {
    let kind = classify_mime(&snapshot.mime_type, &snapshot.mime_types);
    let size = snapshot.data.len() as u64;
    let preview = preview_for(kind, &snapshot.mime_type, &snapshot.data);
    let fingerprint = fingerprint(&snapshot.mime_type, &snapshot.data);
    let data: Arc<[u8]> = snapshot.data.into();

    ClipboardEntry {
        id,
        kind,
        mime_type: snapshot.mime_type,
        mime_types: snapshot.mime_types,
        preview,
        size,
        timestamp,
        data,
        fingerprint,
    }
}

fn remove_history_entry(state: &mut State, id: u64) -> Option<u64> {
    let index = state.history.iter().position(|entry| entry.id == id)?;
    let entry = state.history.remove(index);
    if state.current_id == Some(id) {
        state.current_id = None;
    }
    Some(entry.fingerprint)
}

fn apply_clipboard_entry(state: &mut State, entry: ClipboardEntry) -> bool {
    if entry.data.is_empty() {
        return false;
    }

    if state
        .history
        .first()
        .is_some_and(|current| current.fingerprint == entry.fingerprint)
    {
        state.current_id = state.history.first().map(|entry| entry.id);
        return false;
    }

    state
        .history
        .retain(|existing| existing.fingerprint != entry.fingerprint);
    state.current_id = Some(entry.id);
    state.history.insert(0, entry);
    apply_history_byte_limit(&mut state.history);
    true
}

fn apply_history_byte_limit(history: &mut Vec<ClipboardEntry>) {
    let mut total: usize = 0;
    history.retain(|entry| {
        total += entry.data.len();
        total <= MAX_HISTORY_BYTES
    });
}

fn classify_mime(primary: &str, all: &[String]) -> ClipboardEntryKind {
    let primary = primary.to_ascii_lowercase();
    if primary == "text/html" {
        ClipboardEntryKind::Html
    } else if primary.starts_with("image/") {
        ClipboardEntryKind::Image
    } else if primary == "text/uri-list"
        || all
            .iter()
            .any(|mime| mime.eq_ignore_ascii_case("x-special/gnome-copied-files"))
    {
        ClipboardEntryKind::Files
    } else if primary.starts_with("text/")
        || matches!(primary.as_str(), "utf8_string" | "text" | "string")
    {
        ClipboardEntryKind::Text
    } else {
        ClipboardEntryKind::Other
    }
}

fn preview_for(kind: ClipboardEntryKind, mime_type: &str, data: &[u8]) -> String {
    match kind {
        ClipboardEntryKind::Text | ClipboardEntryKind::Html | ClipboardEntryKind::Files => {
            String::from_utf8_lossy(data)
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
                .chars()
                .take(MAX_PREVIEW_CHARS)
                .collect()
        }
        ClipboardEntryKind::Image => format!("Image ({mime_type})"),
        ClipboardEntryKind::Other => format!("{} bytes ({mime_type})", data.len()),
    }
}

fn fingerprint(mime_type: &str, data: &[u8]) -> u64 {
    let mut hasher = DefaultHasher::new();
    mime_type.hash(&mut hasher);
    data.hash(&mut hasher);
    hasher.finish()
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ── Wayland data-control watcher ────────────────────────────────────────────

enum SelectionEvent {
    Changed,
    Cleared,
}

struct WatcherState {
    event_tx: mpsc::Sender<SelectionEvent>,
}

struct WatcherSetup {
    conn: Connection,
    event_queue: wayland_client::EventQueue<WatcherState>,
    state: WatcherState,
    // Kept alive for the duration of the watcher: dropping the device destroys the Wayland object
    // and stops selection events from being delivered.
    _device: zwlr_data_control_device_v1::ZwlrDataControlDeviceV1,
}

async fn run_watcher(
    event_tx: mpsc::Sender<SelectionEvent>,
    cancel: CancellationToken,
) -> anyhow::Result<()> {
    let WatcherSetup {
        conn,
        mut event_queue,
        mut state,
        _device,
    } = tokio::select! {
        _ = cancel.cancelled() => return Ok(()),
        setup = tokio::time::timeout(WATCHER_SETUP_TIMEOUT, setup_watcher(event_tx)) => {
            setup.map_err(|_| anyhow::anyhow!("clipboard watcher setup timed out"))??
        }
    };

    let owned_fd = conn.as_fd().try_clone_to_owned()?;
    let async_fd = AsyncFd::with_interest(owned_fd, Interest::READABLE)?;

    loop {
        event_queue.dispatch_pending(&mut state)?;
        conn.flush()?;

        tokio::select! {
            _ = cancel.cancelled() => return Ok(()),
            readable = async_fd.readable() => {
                let mut guard = readable?;
                if let Some(read_guard) = conn.prepare_read() {
                    read_guard.read()?;
                }
                guard.clear_ready();
            }
        }
    }
}

async fn setup_watcher(event_tx: mpsc::Sender<SelectionEvent>) -> anyhow::Result<WatcherSetup> {
    tokio::task::spawn_blocking(move || setup_watcher_blocking(event_tx))
        .await
        .map_err(|e| anyhow::anyhow!("clipboard watcher setup task failed: {e}"))?
}

fn setup_watcher_blocking(event_tx: mpsc::Sender<SelectionEvent>) -> anyhow::Result<WatcherSetup> {
    let conn = Connection::connect_to_env()
        .map_err(|e| anyhow::anyhow!("clipboard watcher: failed to connect to Wayland: {e}"))?;
    let (globals, mut event_queue) = registry_queue_init::<WatcherState>(&conn)
        .map_err(|e| anyhow::anyhow!("clipboard watcher: registry init failed: {e}"))?;
    let qh = event_queue.handle();
    let manager = globals
        .bind::<zwlr_data_control_manager_v1::ZwlrDataControlManagerV1, _, _>(&qh, 1..=2, ())
        .map_err(|e| {
            anyhow::anyhow!(
                "clipboard watcher: zwlr_data_control_manager_v1 not available: {e}"
            )
        })?;
    let seat = globals
        .bind::<wl_seat::WlSeat, _, _>(&qh, 1..=9, ())
        .map_err(|e| anyhow::anyhow!("clipboard watcher: wl_seat not available: {e}"))?;
    let mut state = WatcherState { event_tx };
    let device = manager.get_data_device(&seat, &qh, ());
    event_queue
        .roundtrip(&mut state)
        .map_err(|e| anyhow::anyhow!("clipboard watcher: roundtrip failed: {e}"))?;
    Ok(WatcherSetup {
        conn,
        event_queue,
        state,
        _device: device,
    })
}

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for WatcherState {
    fn event(
        _state: &mut Self,
        _proxy: &wl_registry::WlRegistry,
        _event: wl_registry::Event,
        _data: &GlobalListContents,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<zwlr_data_control_manager_v1::ZwlrDataControlManagerV1, ()> for WatcherState {
    fn event(
        _state: &mut Self,
        _proxy: &zwlr_data_control_manager_v1::ZwlrDataControlManagerV1,
        _event: zwlr_data_control_manager_v1::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<wl_seat::WlSeat, ()> for WatcherState {
    fn event(
        _state: &mut Self,
        _proxy: &wl_seat::WlSeat,
        _event: wl_seat::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<zwlr_data_control_device_v1::ZwlrDataControlDeviceV1, ()> for WatcherState {
    fn event(
        state: &mut Self,
        _proxy: &zwlr_data_control_device_v1::ZwlrDataControlDeviceV1,
        event: zwlr_data_control_device_v1::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        match event {
            zwlr_data_control_device_v1::Event::Selection { id } => match id {
                Some(offer) => {
                    offer.destroy();
                    let _ = state.event_tx.try_send(SelectionEvent::Changed);
                }
                None => {
                    let _ = state.event_tx.try_send(SelectionEvent::Cleared);
                }
            },
            zwlr_data_control_device_v1::Event::PrimarySelection { id: Some(offer) } => {
                offer.destroy();
            }
            _ => {}
        }
    }

    event_created_child!(WatcherState, zwlr_data_control_device_v1::ZwlrDataControlDeviceV1, [
        zwlr_data_control_device_v1::EVT_DATA_OFFER_OPCODE => (zwlr_data_control_offer_v1::ZwlrDataControlOfferV1, ())
    ]);
}

impl Dispatch<zwlr_data_control_offer_v1::ZwlrDataControlOfferV1, ()> for WatcherState {
    fn event(
        _state: &mut Self,
        _proxy: &zwlr_data_control_offer_v1::ZwlrDataControlOfferV1,
        _event: zwlr_data_control_offer_v1::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_common_mime_types() {
        assert_eq!(
            classify_mime("text/plain;charset=utf-8", &[]),
            ClipboardEntryKind::Text
        );
        assert_eq!(classify_mime("text/html", &[]), ClipboardEntryKind::Html);
        assert_eq!(classify_mime("image/png", &[]), ClipboardEntryKind::Image);
        assert_eq!(
            classify_mime("text/uri-list", &[]),
            ClipboardEntryKind::Files
        );
    }

    #[test]
    fn preview_collapses_text_whitespace() {
        assert_eq!(
            preview_for(ClipboardEntryKind::Text, "text/plain", b"hello\n  world"),
            "hello world"
        );
    }

    #[test]
    fn apply_entry_deduplicates_and_keeps_recent_first() {
        let mut state = State::default();
        let first = entry(1, "one");
        let duplicate = entry(2, "one");
        let second = entry(3, "two");

        assert!(apply_clipboard_entry(&mut state, first));
        assert!(!apply_clipboard_entry(&mut state, duplicate));
        assert!(apply_clipboard_entry(&mut state, second));

        assert_eq!(state.history.len(), 2);
        assert_eq!(state.history[0].id, 3);
        assert_eq!(state.history[1].id, 1);
    }

    #[test]
    fn apply_history_byte_limit_drops_oldest_entries_over_budget() {
        // Build entries whose total payload exceeds MAX_HISTORY_BYTES.
        // Each entry gets a 4 MB payload so that 3 entries = 12 MB > 10 MB limit.
        let make_big = |id: u64| ClipboardEntry {
            id,
            kind: ClipboardEntryKind::Other,
            mime_type: "application/octet-stream".into(),
            mime_types: vec!["application/octet-stream".into()],
            preview: String::new(),
            size: (4 * 1024 * 1024) as u64,
            timestamp: id,
            data: Arc::from(vec![0u8; 4 * 1024 * 1024].as_slice()),
            fingerprint: id,
        };
        let mut history = vec![make_big(1), make_big(2), make_big(3)];
        apply_history_byte_limit(&mut history);
        // history[0] + history[1] = 8 MB ≤ 10 MB; history[2] would push to 12 MB → dropped
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].id, 1);
        assert_eq!(history[1].id, 2);
    }

    #[test]
    fn remove_history_entry_removes_one_item() {
        let mut state = State {
            history: vec![entry(1, "one"), entry(2, "two"), entry(3, "three")],
            current_id: Some(1),
            ..State::default()
        };
        let removed = state.history[1].fingerprint;

        assert_eq!(remove_history_entry(&mut state, 2), Some(removed));
        assert_eq!(
            state
                .history
                .iter()
                .map(|entry| entry.id)
                .collect::<Vec<_>>(),
            vec![1, 3]
        );
        assert_eq!(state.current_id, Some(1));
    }

    #[test]
    fn remove_history_entry_clears_current_id_when_current_is_removed() {
        let mut state = State {
            history: vec![entry(1, "one")],
            current_id: Some(1),
            ..State::default()
        };

        assert!(remove_history_entry(&mut state, 1).is_some());
        assert!(state.history.is_empty());
        assert_eq!(state.current_id, None);
    }

    fn entry(id: u64, text: &str) -> ClipboardEntry {
        entry_from_snapshot(
            id,
            ClipboardSnapshot {
                mime_type: "text/plain".into(),
                mime_types: vec!["text/plain".into()],
                data: text.as_bytes().to_vec(),
            },
            id,
        )
    }
}
