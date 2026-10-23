use super::*;

use crate::services::audio::{App, AppId, Device, DeviceId, Role, StreamRef};

use crate::services::compositor::{CastInfo, CastKindInfo};

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

mod camera;
mod casts;
mod location;
mod pipewire;
