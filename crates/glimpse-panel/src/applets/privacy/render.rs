use std::time::SystemTime;

use gettextrs::gettext;
use glimpse_services::{OutputInfo, PrivacyResource, PrivacyState, PrivacyUsage as ServiceUsage};
use glimpse_widgets::PrivacyUsage;

pub const CAMERA: &str = "camera-web-symbolic";
pub const MICROPHONE: &str = "audio-input-microphone-symbolic";
pub const SCREEN: &str = "video-display-symbolic";
pub const LOCATION: &str = "find-location-symbolic";
pub const RECORDING: &str = "media-record-symbolic";
pub const RECORDING_CLASS: &str = "indicator--recording";
const NAME_CAP: usize = 48;

const KINDS: [PrivacyResource; 4] = [
    PrivacyResource::Camera,
    PrivacyResource::Microphone,
    PrivacyResource::Screen,
    PrivacyResource::Location,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Filters {
    pub camera: bool,
    pub microphone: bool,
    pub screencast: bool,
    pub location: bool,
}

impl Default for Filters {
    fn default() -> Self {
        Self {
            camera: true,
            microphone: true,
            screencast: true,
            location: true,
        }
    }
}

impl Filters {
    fn allows(&self, kind: PrivacyResource) -> bool {
        match kind {
            PrivacyResource::Camera => self.camera,
            PrivacyResource::Microphone => self.microphone,
            PrivacyResource::Screen => self.screencast,
            PrivacyResource::Location => self.location,
        }
    }
}

fn visible(state: &PrivacyState, filters: Filters) -> impl Iterator<Item = &ServiceUsage> {
    state
        .usages
        .iter()
        .filter(move |usage| filters.allows(usage.kind))
}

pub fn icon(kind: PrivacyResource) -> &'static str {
    match kind {
        PrivacyResource::Camera => CAMERA,
        PrivacyResource::Microphone => MICROPHONE,
        PrivacyResource::Screen => SCREEN,
        PrivacyResource::Location => LOCATION,
    }
}

pub fn title(kind: PrivacyResource) -> String {
    match kind {
        PrivacyResource::Camera => gettext("Camera"),
        PrivacyResource::Microphone => gettext("Microphone"),
        PrivacyResource::Screen => gettext("Screen"),
        PrivacyResource::Location => gettext("Location"),
    }
}

/// The app name and the detail joined when both are known, whichever is known alone, or nothing
/// when neither is. A location usage carries neither: GeoClue exposes no per-client attribution.
pub fn detail(usage: &ServiceUsage) -> Option<String> {
    match (usage.app.as_deref(), usage.detail.as_deref()) {
        (Some(app), Some(detail)) => Some(format!("{app} · {detail}")),
        (Some(app), None) => Some(app.to_owned()),
        (None, Some(detail)) => Some(detail.to_owned()),
        (None, None) => None,
    }
}

/// A key stable across renders: the resource plus the app for camera and microphone, the resource
/// plus the stream id for a screen cast, and the bare resource for location, which is always
/// exactly one usage.
pub fn usage_id(usage: &ServiceUsage) -> String {
    match usage.kind {
        PrivacyResource::Screen => format!("screen:{}", usage.stream_id.unwrap_or_default()),
        PrivacyResource::Camera => format!("camera:{}", usage.app.as_deref().unwrap_or("")),
        PrivacyResource::Microphone => format!("mic:{}", usage.app.as_deref().unwrap_or("")),
        PrivacyResource::Location => "location".to_owned(),
    }
}

fn row_key(usage: &ServiceUsage) -> String {
    match &usage.app {
        Some(app) => format!("app:{app}"),
        None => usage_id(usage),
    }
}

pub fn usages(
    state: &PrivacyState,
    filters: Filters,
    outputs: &[OutputInfo],
    muted: bool,
) -> Vec<PrivacyUsage> {
    let mut groups: Vec<(String, Vec<&ServiceUsage>)> = Vec::new();
    for usage in visible(state, filters) {
        let key = row_key(usage);
        match groups.iter_mut().find(|(held, _)| *held == key) {
            Some((_, group)) => group.push(usage),
            None => groups.push((key, vec![usage])),
        }
    }
    groups
        .into_iter()
        .map(|(id, group)| row(id, &group, outputs, muted))
        .collect()
}

fn row(id: String, group: &[&ServiceUsage], outputs: &[OutputInfo], muted: bool) -> PrivacyUsage {
    let first = group[0];
    let named = first.app.is_some();
    let mut parts = Vec::new();
    for kind in KINDS {
        let of_kind: Vec<&ServiceUsage> = group
            .iter()
            .copied()
            .filter(|usage| usage.kind == kind)
            .collect();
        if of_kind.is_empty() {
            continue;
        }
        match kind {
            PrivacyResource::Screen => {
                let shared = shared(&of_kind, outputs);
                parts.push(match named {
                    true => gettext("Sharing {what}").replace("{what}", &shared),
                    false => shared,
                });
            }
            _ if named => parts.push(title(kind)),
            _ => {}
        }
        if kind == PrivacyResource::Microphone && muted {
            parts.push(gettext("muted"));
        }
    }
    PrivacyUsage {
        id,
        icon: first
            .icon
            .clone()
            .unwrap_or_else(|| icon(first.kind).to_owned()),
        title: match &first.app {
            Some(app) => glimpse_utils::clean(app, NAME_CAP),
            None => title(first.kind),
        },
        detail: (!parts.is_empty()).then(|| parts.join(" · ")),
        stoppable: group.iter().any(|usage| usage.session.is_some()),
    }
}

fn shared(casts: &[&ServiceUsage], outputs: &[OutputInfo]) -> String {
    let names: Vec<String> = casts
        .iter()
        .map(|cast| match &cast.detail {
            Some(connector) => crate::applets::output_name(connector, outputs),
            None => gettext("a window"),
        })
        .collect();
    names.join(", ")
}

pub fn sessions_for(state: &PrivacyState, filters: Filters, id: &str) -> Vec<u64> {
    visible(state, filters)
        .filter(|usage| row_key(usage) == id)
        .filter_map(|usage| usage.session)
        .collect()
}

pub fn uses_microphone(state: &PrivacyState, filters: Filters) -> bool {
    visible(state, filters).any(|usage| usage.kind == PrivacyResource::Microphone)
}

pub fn screencast_since(state: &PrivacyState, filters: Filters) -> Option<SystemTime> {
    visible(state, filters)
        .filter(|usage| usage.kind == PrivacyResource::Screen)
        .map(|usage| usage.since)
        .min()
}

pub fn elapsed(since: SystemTime, now: SystemTime) -> String {
    let seconds = now.duration_since(since).unwrap_or_default().as_secs();
    let (hours, minutes, seconds) = (seconds / 3600, (seconds % 3600) / 60, seconds % 60);
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes:02}:{seconds:02}")
    }
}

pub fn kinds_in_use(state: &PrivacyState, filters: Filters) -> Vec<PrivacyResource> {
    KINDS
        .into_iter()
        .filter(|&kind| filters.allows(kind) && state.usages.iter().any(|usage| usage.kind == kind))
        .collect()
}

/// `format` is `[applets.privacy] tooltip-format`, honoured as a plain override: unlike every other
/// applet's tooltip, a privacy chip's composed text has no natural per-resource token to
/// substitute into it — camera, microphone, screen and location each carry a different shape of
/// detail — so a configured format simply replaces the computed tooltip rather than filling a
/// placeholder inside it.
pub fn tooltip(kind: PrivacyResource, usages: &[&ServiceUsage], format: Option<&str>) -> String {
    if let Some(format) = format {
        return format.to_owned();
    }
    let base = title(kind);
    let details: Vec<String> = usages.iter().filter_map(|usage| detail(usage)).collect();
    if details.is_empty() {
        base
    } else {
        format!("{base} — {}", details.join(", "))
    }
}

#[cfg(test)]
mod tests;
