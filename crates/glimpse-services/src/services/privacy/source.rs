use std::path::Path;
use std::pin::Pin;

use futures_util::{Stream, StreamExt, stream};
use glimpse_dbus::geoclue::GeoClueManagerProxy;
use glimpse_utils::clean;

use crate::context::Ctx;
use crate::services::audio::{App, AudioState, Direction};
use crate::services::compositor::{CastKindInfo, CastTargetInfo, CompositorPrivacy};

use super::{Event, Fact, NAME_CAP, Privacy, Resource};

const PROC_ROOT: &str = "/proc";
const BLOCKED_APP_IDS: [&str; 2] = ["org.gnome.VolumeControl", "org.PulseAudio.pavucontrol"];

type Events = Pin<Box<dyn Stream<Item = Event> + Send>>;

fn location_unavailable(reason: &'static str) -> Events {
    Box::pin(stream::once(async move {
        Event::Unavailable(Resource::Location, reason)
    }))
}

pub async fn scan_camera() -> Event {
    match tokio::task::spawn_blocking(|| scan(Path::new(PROC_ROOT))).await {
        Ok(Ok(facts)) => Event::Camera(facts),
        Ok(Err(reason)) => Event::Unavailable(Resource::Camera, reason),
        Err(_) => Event::Unavailable(Resource::Camera, "camera: the scan task did not complete"),
    }
}

fn scan(root: &Path) -> Result<Vec<Fact>, &'static str> {
    let modules = std::fs::read_to_string(root.join("modules"))
        .map_err(|_| "camera: /proc/modules is unreadable")?;
    let refcount = uvcvideo_refcount(&modules).ok_or(
        "camera: uvcvideo is not loaded, so camera activity cannot be observed on this hardware",
    )?;
    if refcount == 0 {
        return Ok(Vec::new());
    }
    Ok(holders(root))
}

fn uvcvideo_refcount(modules: &str) -> Option<u32> {
    modules
        .lines()
        .find(|line| line.split_whitespace().next() == Some("uvcvideo"))
        .and_then(|line| line.split_whitespace().nth(2))
        .and_then(|field| field.parse().ok())
}

fn holders(root: &Path) -> Vec<Fact> {
    let Ok(entries) = std::fs::read_dir(root) else {
        return Vec::new();
    };

    let mut apps: Vec<Option<String>> = Vec::new();
    for entry in entries.flatten() {
        let is_pid_dir = entry
            .file_name()
            .to_str()
            .is_some_and(|name| name.parse::<u32>().is_ok());
        if !is_pid_dir {
            continue;
        }
        if !holds_camera(&entry.path()) {
            continue;
        }
        let Ok(comm) = std::fs::read_to_string(entry.path().join("comm")) else {
            continue;
        };
        let app = app_from_comm(&comm);
        if !apps.contains(&app) {
            apps.push(app);
        }
    }

    apps.into_iter()
        .map(|app| Fact {
            app,
            icon: None,
            detail: None,
            stream_id: None,
            session: None,
        })
        .collect()
}

fn holds_camera(pid_dir: &Path) -> bool {
    let Ok(entries) = std::fs::read_dir(pid_dir.join("fd")) else {
        return false;
    };
    entries.flatten().any(|entry| {
        std::fs::read_link(entry.path())
            .map(|target| target.to_string_lossy().starts_with("/dev/video"))
            .unwrap_or(false)
    })
}

fn app_from_comm(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed == "pipewire" || trimmed == "wireplumber" {
        None
    } else {
        Some(clean(trimmed, NAME_CAP))
    }
}

pub fn microphone(state: &AudioState) -> Vec<Fact> {
    state
        .apps
        .iter()
        .filter_map(|app| {
            let role = app.capture.as_ref()?;
            if role.corked {
                return None;
            }
            let captures_a_real_source = role
                .streams
                .iter()
                .any(|stream| state.device(Direction::Input, &stream.device).is_some());
            if !captures_a_real_source {
                return None;
            }
            if app
                .app_id
                .as_deref()
                .is_some_and(|id| BLOCKED_APP_IDS.contains(&id))
            {
                return None;
            }

            Some(Fact {
                app: display_name(app),
                icon: app.icon_name.as_deref().map(|icon| clean(icon, NAME_CAP)),
                detail: None,
                stream_id: None,
                session: None,
            })
        })
        .collect()
}

fn display_name(app: &App) -> Option<String> {
    let stripped = app.name.strip_suffix(" input").unwrap_or(&app.name);
    let raw = if stripped.is_empty() {
        app.binary.as_deref().unwrap_or_default()
    } else {
        stripped
    };
    Some(clean(raw, NAME_CAP)).filter(|name| !name.is_empty())
}

pub fn screen(privacy: &CompositorPrivacy) -> Vec<Fact> {
    privacy
        .casts
        .iter()
        .filter(|cast| cast.active)
        .map(|cast| Fact {
            app: None,
            icon: None,
            detail: match &cast.target {
                CastTargetInfo::Output(name) => Some(clean(name, NAME_CAP)),
                CastTargetInfo::Window(_) | CastTargetInfo::Unknown => None,
            },
            stream_id: Some(cast.stream_id),
            session: match cast.kind {
                CastKindInfo::PipeWire => cast.session_id,
                CastKindInfo::Screencopy | CastKindInfo::Unknown => None,
            },
        })
        .collect()
}

pub async fn location(ctx: Ctx<Privacy>) -> Events {
    let Ok(bus) = ctx.system_bus().cloned() else {
        return location_unavailable("location: no system bus");
    };
    let manager = match GeoClueManagerProxy::new(&bus).await {
        Ok(manager) => manager,
        Err(_) => return location_unavailable("location: GeoClue is unavailable"),
    };

    let changes = manager.receive_in_use_changed().await;
    let known = manager.in_use().await.unwrap_or(false);

    let first = stream::once(async move { Event::Location(known) });
    let following =
        changes.then(|change| async move { Event::Location(change.get().await.unwrap_or(false)) });

    Box::pin(first.chain(following))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::audio::{App, AppId, Device, DeviceId, Role, StreamRef};
    use crate::services::compositor::{CastInfo, CastKindInfo};

    #[test]
    fn a_zero_refcount_is_read_from_the_third_field() {
        let modules = "uvcvideo 106496 0 - Live 0x0000000000000000\n";
        assert_eq!(uvcvideo_refcount(modules), Some(0));
    }

    #[test]
    fn a_nonzero_refcount_is_read_from_the_third_field() {
        let modules = "other_mod 16384 1 - Live 0x0\nuvcvideo 106496 2 - Live 0x0\n";
        assert_eq!(uvcvideo_refcount(modules), Some(2));
    }

    #[test]
    fn a_missing_uvcvideo_line_is_none() {
        assert_eq!(
            uvcvideo_refcount("other_mod 16384 1 - Live 0x0\n"),
            None,
            "absent is not the same fact as present-and-idle: only the former means this \
             hardware cannot be observed at all"
        );
    }

    fn fake_proc(root: &Path, pid: u32, comm: &str, video_fd: Option<&str>) {
        let pid_dir = root.join(pid.to_string());
        let fd_dir = pid_dir.join("fd");
        std::fs::create_dir_all(&fd_dir).expect("create fake pid/fd dir");
        std::fs::write(pid_dir.join("comm"), format!("{comm}\n")).expect("write comm");
        if let Some(target) = video_fd {
            #[cfg(unix)]
            std::os::unix::fs::symlink(target, fd_dir.join("3")).expect("symlink fake fd");
        }
    }

    fn fake_modules(root: &Path, refcount: u32) {
        std::fs::write(
            root.join("modules"),
            format!("uvcvideo 106496 {refcount} - Live 0x0\n"),
        )
        .expect("write fake /proc/modules");
    }

    #[test]
    fn a_zero_refcount_skips_the_walk_entirely() {
        let dir = tempfile::tempdir().expect("a temporary directory");
        fake_modules(dir.path(), 0);
        fake_proc(dir.path(), 4242, "ffmpeg", Some("/dev/video0"));

        assert!(
            scan(dir.path())
                .expect("uvcvideo is loaded in this fixture")
                .is_empty(),
            "a zero uvcvideo refcount must skip the /proc walk entirely, not merely find nothing \
             in it: this fabricated process would otherwise be found and named"
        );
    }

    #[test]
    fn a_process_holding_the_camera_is_named() {
        let dir = tempfile::tempdir().expect("a temporary directory");
        fake_modules(dir.path(), 1);
        fake_proc(dir.path(), 4242, "ffmpeg", Some("/dev/video0"));

        let facts = scan(dir.path()).expect("uvcvideo is loaded in this fixture");
        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].app.as_deref(), Some("ffmpeg"));
    }

    #[test]
    fn pipewire_and_wireplumber_are_the_daemon_not_the_app() {
        assert_eq!(app_from_comm("pipewire"), None);
        assert_eq!(app_from_comm("wireplumber"), None);
        assert_eq!(app_from_comm("chrome"), Some("chrome".to_owned()));
    }

    #[test]
    fn a_process_with_no_camera_fd_is_ignored() {
        let dir = tempfile::tempdir().expect("a temporary directory");
        fake_modules(dir.path(), 1);
        fake_proc(dir.path(), 4242, "sshd", None);

        assert!(
            scan(dir.path())
                .expect("uvcvideo is loaded in this fixture")
                .is_empty()
        );
    }

    #[test]
    fn an_unreadable_modules_file_is_reported_unavailable() {
        let dir = tempfile::tempdir().expect("a temporary directory");

        assert_eq!(scan(dir.path()), Err("camera: /proc/modules is unreadable"));
    }

    #[test]
    fn an_absent_uvcvideo_module_is_reported_unavailable_rather_than_idle() {
        let dir = tempfile::tempdir().expect("a temporary directory");
        std::fs::write(dir.path().join("modules"), "other_mod 16384 1 - Live 0x0\n")
            .expect("write fake /proc/modules");

        assert_eq!(
            scan(dir.path()),
            Err(
                "camera: uvcvideo is not loaded, so camera activity cannot be observed on this \
                 hardware"
            )
        );
    }

    fn device(id: &str) -> Device {
        Device {
            id: DeviceId::new(id),
            index: 0,
            name: id.to_owned(),
            icon_name: None,
            form_factor: None,
            volume: 100,
            muted: false,
            default: true,
        }
    }

    fn stream_ref(device_id: &str) -> StreamRef {
        StreamRef {
            index: 1,
            volume: 80,
            device: DeviceId::new(device_id),
        }
    }

    fn role(device_id: &str, corked: bool) -> Role {
        Role {
            volume: 80,
            muted: false,
            adjustable: true,
            device: DeviceId::new(device_id),
            corked,
            streams: vec![stream_ref(device_id)],
        }
    }

    fn app(id: &str, name: &str, binary: Option<&str>, capture: Option<Role>) -> App {
        App {
            id: AppId::new(id),
            name: name.to_owned(),
            icon_name: Some("google-chrome".to_owned()),
            app_id: None,
            binary: binary.map(str::to_owned),
            playback: None,
            capture,
        }
    }

    #[test]
    fn a_corked_capture_is_excluded() {
        let state = AudioState {
            inputs: vec![device("mic")],
            apps: vec![app("chrome", "chrome", None, Some(role("mic", true)))],
            ..Default::default()
        };

        assert!(microphone(&state).is_empty());
    }

    #[test]
    fn a_capture_on_a_monitor_source_is_excluded() {
        let state = AudioState {
            inputs: vec![device("mic")],
            apps: vec![app(
                "recorder",
                "recorder",
                None,
                Some(role("bluez_output.monitor", false)),
            )],
            ..Default::default()
        };

        assert!(
            microphone(&state).is_empty(),
            "a monitor device is not present in AudioState.inputs, and that absence is the signal"
        );
    }

    #[test]
    fn a_mixed_monitor_and_mic_app_still_reports_the_mic() {
        let mut role = role("bluez_output.monitor", false);
        role.streams.push(stream_ref("mic"));
        let state = AudioState {
            inputs: vec![device("mic")],
            apps: vec![app("chrome", "chrome", None, Some(role))],
            ..Default::default()
        };

        assert_eq!(
            microphone(&state).len(),
            1,
            "Role.device names only one arbitrary stream's device; a second stream that IS a \
             real microphone must not be hidden by the first being a monitor"
        );
    }

    #[test]
    fn volume_control_is_blocked_by_app_id() {
        let mut by_id = app("vc", "irrelevant", None, Some(role("mic", false)));
        by_id.app_id = Some("org.gnome.VolumeControl".to_owned());

        let state = AudioState {
            inputs: vec![device("mic")],
            apps: vec![by_id],
            ..Default::default()
        };

        assert!(microphone(&state).is_empty());
    }

    #[test]
    fn a_hostile_name_claiming_to_be_the_volume_control_is_not_excluded() {
        let spoofed = app(
            "totally-not-spyware",
            "PulseAudio Volume Control",
            None,
            Some(role("mic", false)),
        );
        let state = AudioState {
            inputs: vec![device("mic")],
            apps: vec![spoofed],
            ..Default::default()
        };

        assert_eq!(
            microphone(&state).len(),
            1,
            "application.name is attacker-controlled; only the app id may gate the indicator"
        );
    }

    #[test]
    fn the_application_name_is_preferred_and_its_input_suffix_is_stripped() {
        let state = AudioState {
            inputs: vec![device("mic")],
            apps: vec![app(
                "chrome",
                "Google Chrome input",
                Some("chrome"),
                Some(role("mic", false)),
            )],
            ..Default::default()
        };

        assert_eq!(
            microphone(&state)[0].app.as_deref(),
            Some("Google Chrome"),
            "the raw name is not a display name, but it is still the field to prefer once cleaned"
        );
    }

    #[test]
    fn a_useless_binary_never_overrides_a_good_name() {
        let state = AudioState {
            inputs: vec![device("mic")],
            apps: vec![app(
                "walz",
                "walz",
                Some("WebKitWebProcess"),
                Some(role("mic", false)),
            )],
            ..Default::default()
        };

        assert_eq!(
            microphone(&state)[0].app.as_deref(),
            Some("walz"),
            "a WebKit-hosted app's own name is the useful one; its binary is not"
        );
    }

    #[test]
    fn an_empty_name_and_no_binary_is_a_first_class_unnamed_row() {
        let state = AudioState {
            inputs: vec![device("mic")],
            apps: vec![app("7", "", None, Some(role("mic", false)))],
            ..Default::default()
        };

        assert_eq!(
            microphone(&state)[0].app,
            None,
            "a blank label is worse than an explicitly unnamed row"
        );
    }

    #[test]
    fn a_four_kilobyte_app_name_is_capped_and_sanitized() {
        let long = "a".repeat(4096);
        let state = AudioState {
            inputs: vec![device("mic")],
            apps: vec![app("app", &long, None, Some(role("mic", false)))],
            ..Default::default()
        };

        let facts = microphone(&state);
        assert!(facts[0].app.as_ref().expect("a name").chars().count() <= NAME_CAP + 1);
    }

    fn cast(
        stream_id: u64,
        session_id: Option<u64>,
        kind: CastKindInfo,
        target: CastTargetInfo,
        active: bool,
    ) -> CastInfo {
        CastInfo {
            stream_id,
            session_id,
            kind,
            target,
            pw_node_id: None,
            active,
        }
    }

    #[test]
    fn an_inactive_cast_is_excluded() {
        let privacy = CompositorPrivacy {
            active: false,
            casts: vec![cast(
                1,
                Some(1),
                CastKindInfo::PipeWire,
                CastTargetInfo::Unknown,
                false,
            )],
        };

        assert!(screen(&privacy).is_empty());
    }

    #[test]
    fn both_cast_kinds_light_the_indicator_identically() {
        let privacy = CompositorPrivacy {
            active: true,
            casts: vec![
                cast(
                    1,
                    Some(11),
                    CastKindInfo::PipeWire,
                    CastTargetInfo::Output("DP-2".to_owned()),
                    true,
                ),
                cast(
                    2,
                    None,
                    CastKindInfo::Screencopy,
                    CastTargetInfo::Unknown,
                    true,
                ),
            ],
        };

        let facts = screen(&privacy);
        assert_eq!(facts.len(), 2, "neither cast kind is treated as invisible");
        assert_eq!(facts[0].detail.as_deref(), Some("DP-2"));
        assert_eq!(facts[0].app, None);
        assert_eq!(facts[0].stream_id, Some(1));
        assert_eq!(facts[1].detail, None);
        assert_eq!(facts[1].app, None, "screen attribution is a later bead");
        assert_eq!(facts[1].stream_id, Some(2));
    }

    #[test]
    fn only_a_pipewire_cast_with_a_session_id_carries_a_stoppable_session() {
        let privacy = CompositorPrivacy {
            active: true,
            casts: vec![
                cast(
                    1,
                    Some(11),
                    CastKindInfo::PipeWire,
                    CastTargetInfo::Unknown,
                    true,
                ),
                cast(
                    2,
                    Some(22),
                    CastKindInfo::Screencopy,
                    CastTargetInfo::Unknown,
                    true,
                ),
                cast(
                    3,
                    None,
                    CastKindInfo::PipeWire,
                    CastTargetInfo::Unknown,
                    true,
                ),
            ],
        };

        let facts = screen(&privacy);
        assert_eq!(
            facts[0].session,
            Some(11),
            "a PipeWire cast with a session id can be stopped"
        );
        assert_eq!(
            facts[1].session, None,
            "niri cannot stop a wlr-screencopy cast; offering the control would lie"
        );
        assert_eq!(
            facts[2].session, None,
            "Hyprland's synthetic casts carry no session id at all"
        );
    }

    #[test]
    fn a_window_target_carries_no_detail() {
        let privacy = CompositorPrivacy {
            active: true,
            casts: vec![cast(
                1,
                Some(1),
                CastKindInfo::PipeWire,
                CastTargetInfo::Window(9),
                true,
            )],
        };

        assert_eq!(screen(&privacy)[0].detail, None);
    }

    #[tokio::test]
    async fn the_location_source_reports_unavailable_without_a_system_bus() {
        let cancel = tokio_util::sync::CancellationToken::new();
        let (events, _inbox) = tokio::sync::mpsc::channel::<crate::service::Input<Privacy>>(8);
        let (state, _state_rx) = tokio::sync::watch::channel(super::super::PrivacyState::default());
        let (health, _health_rx) = tokio::sync::watch::channel(crate::ServiceState::Starting);
        let ctx = Ctx::<Privacy>::new(
            events,
            &cancel,
            state,
            health,
            glimpse_dbus::Buses::unavailable("no bus in tests"),
        );

        let mut stream = location(ctx).await;

        let event = futures_util::FutureExt::now_or_never(stream.next())
            .expect("a stream::once with no real await point resolves synchronously")
            .expect("one item");
        assert!(
            matches!(event, Event::Unavailable(Resource::Location, _)),
            "with no system bus the source must say it cannot see, never silently report nothing \
             in use"
        );
    }
}
