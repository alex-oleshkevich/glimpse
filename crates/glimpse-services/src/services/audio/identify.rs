use crate::services::cap;
use std::collections::HashMap;

use super::model::{App, AppId, DeviceId, NAME_CAP, Role, StreamRef};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Identity {
    pub id: AppId,
    pub name: String,
    pub icon_name: Option<String>,
    pub app_id: Option<String>,
    pub binary: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stream {
    pub index: u32,
    pub device: DeviceId,
    pub volume: u32,
    pub muted: bool,
    pub adjustable: bool,
    pub corked: bool,
    pub client: Option<u32>,
    pub client_props: Option<HashMap<String, String>>,
    pub props: HashMap<String, String>,
}

pub fn identify(
    props: &HashMap<String, String>,
    client_props: Option<&HashMap<String, String>>,
    client: Option<u32>,
    index: u32,
) -> Identity {
    let name = cap(&resolve_name(props, client_props), NAME_CAP);
    let icon_name =
        field(props, client_props, "application.icon_name").map(|value| cap(&value, NAME_CAP));
    let app_id = field(props, client_props, "application.id");
    let binary =
        field(props, client_props, "application.process.binary").map(|value| cap(&value, NAME_CAP));

    let id = match &app_id {
        Some(app_id) => AppId::new(app_id.clone()),
        None => match client {
            Some(client) => AppId::new(client.to_string()),
            None => AppId::new(index.to_string()),
        },
    };

    Identity {
        id,
        name,
        icon_name,
        app_id,
        binary,
    }
}

pub fn is_event_sound(props: &HashMap<String, String>) -> bool {
    props.get("media.role").map(String::as_str) == Some("event")
}

pub fn group(playback: Vec<Stream>, capture: Vec<Stream>) -> Vec<App> {
    let mut groups: HashMap<AppId, Group> = HashMap::new();

    for stream in playback
        .into_iter()
        .filter(|stream| !is_event_sound(&stream.props))
    {
        insert_stream(&mut groups, stream, true);
    }
    for stream in capture
        .into_iter()
        .filter(|stream| !is_event_sound(&stream.props))
    {
        insert_stream(&mut groups, stream, false);
    }

    let mut apps: Vec<App> = groups.into_values().map(Group::into_app).collect();
    apps.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.id.cmp(&b.id)));
    apps
}

struct Group {
    identity: Identity,
    playback: Vec<Stream>,
    capture: Vec<Stream>,
}

impl Group {
    fn merge_identity(&mut self, identity: &Identity) {
        if self.identity.icon_name.is_none() {
            self.identity.icon_name = identity.icon_name.clone();
        }
        if self.identity.app_id.is_none() {
            self.identity.app_id = identity.app_id.clone();
        }
        if self.identity.binary.is_none() {
            self.identity.binary = identity.binary.clone();
        }
    }

    fn into_app(self) -> App {
        App {
            id: self.identity.id,
            name: self.identity.name,
            icon_name: self.identity.icon_name,
            app_id: self.identity.app_id,
            binary: self.identity.binary,
            playback: build_role(self.playback),
            capture: build_role(self.capture),
        }
    }
}

fn insert_stream(groups: &mut HashMap<AppId, Group>, stream: Stream, is_playback: bool) {
    let identity = identify(
        &stream.props,
        stream.client_props.as_ref(),
        stream.client,
        stream.index,
    );
    let entry = groups.entry(identity.id.clone()).or_insert_with(|| Group {
        identity: identity.clone(),
        playback: Vec::new(),
        capture: Vec::new(),
    });
    entry.merge_identity(&identity);

    if is_playback {
        entry.playback.push(stream);
    } else {
        entry.capture.push(stream);
    }
}

fn build_role(streams: Vec<Stream>) -> Option<Role> {
    if streams.is_empty() {
        return None;
    }

    let volume = streams
        .iter()
        .map(|stream| stream.volume)
        .max()
        .unwrap_or(0);
    let muted = streams.iter().all(|stream| stream.muted);
    let adjustable = streams.iter().all(|stream| stream.adjustable);
    let corked = streams.iter().all(|stream| stream.corked);
    let device = streams[0].device.clone();
    let refs = streams
        .iter()
        .map(|stream| StreamRef {
            index: stream.index,
            volume: stream.volume,
            device: stream.device.clone(),
        })
        .collect();

    Some(Role {
        volume,
        muted,
        adjustable,
        corked,
        device,
        streams: refs,
    })
}

fn resolve_name(
    props: &HashMap<String, String>,
    client_props: Option<&HashMap<String, String>>,
) -> String {
    field(props, client_props, "application.name")
        .map(|name| unwrap_alsa_name(&name))
        .filter(|name| !name.is_empty())
        .or_else(|| field(props, client_props, "media.name"))
        .or_else(|| field(props, client_props, "node.name"))
        .unwrap_or_default()
}

fn unwrap_alsa_name(name: &str) -> String {
    const PREFIXES: [&str; 2] = ["PipeWire ALSA [", "ALSA plug-in ["];

    for prefix in PREFIXES {
        if let Some(rest) = name.strip_prefix(prefix)
            && let Some(inner) = rest.strip_suffix(']')
        {
            return inner.to_owned();
        }
    }

    name.to_owned()
}

fn field(
    props: &HashMap<String, String>,
    client_props: Option<&HashMap<String, String>>,
    key: &str,
) -> Option<String> {
    props
        .get(key)
        .and_then(|value| sanitize(value))
        .or_else(|| {
            client_props
                .and_then(|client| client.get(key))
                .and_then(|value| sanitize(value))
        })
}

fn sanitize(value: &str) -> Option<String> {
    let stripped: String = value.chars().filter(|c| !c.is_control()).collect();
    let trimmed = stripped.trim();

    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_owned())
    }
}

#[cfg(test)]
mod tests;
