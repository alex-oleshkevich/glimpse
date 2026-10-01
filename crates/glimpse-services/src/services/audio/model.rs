pub const NAME_CAP: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Direction {
    Output,
    Input,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DeviceId(String);

impl DeviceId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct AppId(String);

impl AppId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Device {
    pub id: DeviceId,
    pub index: u32,
    pub name: String,
    pub icon_name: Option<String>,
    pub form_factor: Option<String>,
    pub volume: u32,
    pub muted: bool,
    pub default: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreamRef {
    pub index: u32,
    pub volume: u32,
    pub device: DeviceId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Role {
    pub volume: u32,
    pub muted: bool,
    pub adjustable: bool,
    pub device: DeviceId,
    pub corked: bool,
    pub streams: Vec<StreamRef>,
}

impl Role {
    pub fn scaled(&self, target: u32) -> Vec<(u32, u32)> {
        self.streams
            .iter()
            .map(|stream| {
                let volume = match self.volume {
                    0 => target,
                    max => ((stream.volume as u64 * target as u64) / max as u64) as u32,
                };
                (stream.index, volume)
            })
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct App {
    pub id: AppId,
    pub name: String,
    pub icon_name: Option<String>,
    pub app_id: Option<String>,
    pub binary: Option<String>,
    pub playback: Option<Role>,
    pub capture: Option<Role>,
}

impl App {
    pub fn role(&self, dir: Direction) -> Option<&Role> {
        match dir {
            Direction::Output => self.playback.as_ref(),
            Direction::Input => self.capture.as_ref(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AudioState {
    pub outputs: Vec<Device>,
    pub inputs: Vec<Device>,
    pub apps: Vec<App>,
}

impl AudioState {
    pub fn default_output(&self) -> Option<&Device> {
        self.outputs.iter().find(|device| device.default)
    }

    pub fn default_input(&self) -> Option<&Device> {
        self.inputs.iter().find(|device| device.default)
    }

    pub fn devices(&self, dir: Direction) -> &[Device] {
        match dir {
            Direction::Output => &self.outputs,
            Direction::Input => &self.inputs,
        }
    }

    pub fn device(&self, dir: Direction, id: &DeviceId) -> Option<&Device> {
        self.devices(dir).iter().find(|device| &device.id == id)
    }

    pub fn app(&self, id: &AppId) -> Option<&App> {
        self.apps.iter().find(|app| &app.id == id)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AudioError {
    #[error("the audio server refused the command: {0}")]
    Refused(String),
    #[error("the audio server is unavailable")]
    Unavailable,
    #[error(transparent)]
    Service(#[from] crate::service::CommandError),
}

#[cfg(test)]
mod tests;
