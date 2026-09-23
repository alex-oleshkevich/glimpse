use std::time::SystemTime;

use gettextrs::gettext;
use glimpse_services::{PrivacyResource, PrivacyState, PrivacyUsage as ServiceUsage};
use glimpse_widgets::PrivacyUsage;

pub const CAMERA: &str = "camera-web-symbolic";
pub const MICROPHONE: &str = "audio-input-microphone-symbolic";
pub const SCREEN: &str = "video-display-symbolic";
pub const LOCATION: &str = "find-location-symbolic";
pub const RECORDING: &str = "media-record-symbolic";
pub const RECORDING_CLASS: &str = "indicator--recording";

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

pub fn usages(state: &PrivacyState, filters: Filters) -> Vec<PrivacyUsage> {
    visible(state, filters)
        .map(|usage| PrivacyUsage {
            icon: icon(usage.kind).to_owned(),
            title: title(usage.kind),
            detail: detail(usage),
            id: usage_id(usage),
            stoppable: usage.session.is_some(),
        })
        .collect()
}

/// The banner is raised by the presence of any screen usage, whether or not it can be stopped: a
/// wlr-screencopy or Hyprland cast still records, and the applet's job is to tell the truth about
/// capture rather than only about what it can stop. The subtitle names every cast sharing the
/// screen right now, joined the same way `tooltip` joins several usages of one resource, and is
/// empty when none of them gave a detail.
pub fn screen_shared(state: &PrivacyState, filters: Filters) -> Option<String> {
    let screens: Vec<&ServiceUsage> = visible(state, filters)
        .filter(|usage| usage.kind == PrivacyResource::Screen)
        .collect();
    if screens.is_empty() {
        return None;
    }
    let details: Vec<String> = screens.iter().filter_map(|usage| detail(usage)).collect();
    Some(details.join(", "))
}

/// The compositor session id behind a usage id from the widget's `stop-activated` signal — the
/// widget reports the id it was given, not a session, so the applet resolves it back against the
/// state it dressed the popover from.
pub fn session_for(state: &PrivacyState, filters: Filters, id: &str) -> Option<u64> {
    visible(state, filters)
        .find(|usage| usage_id(usage) == id)
        .and_then(|usage| usage.session)
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
mod tests {
    use super::*;

    fn usage(kind: PrivacyResource) -> ServiceUsage {
        ServiceUsage {
            kind,
            app: None,
            icon: None,
            detail: None,
            since: std::time::SystemTime::now(),
            stream_id: None,
            session: None,
        }
    }

    fn state(usages: Vec<ServiceUsage>) -> PrivacyState {
        PrivacyState { usages }
    }

    #[test]
    fn an_elapsed_cast_reads_mm_ss_until_it_passes_an_hour() {
        let start = SystemTime::UNIX_EPOCH;
        let at = |secs| elapsed(start, start + std::time::Duration::from_secs(secs));
        assert_eq!(at(0), "00:00");
        assert_eq!(at(12), "00:12");
        assert_eq!(at(59), "00:59");
        assert_eq!(at(60), "01:00");
        assert_eq!(at(3599), "59:59");
        assert_eq!(at(3600), "1:00:00");
        assert_eq!(at(3672), "1:01:12");
    }

    #[test]
    fn a_clock_that_jumped_backwards_reads_zero_rather_than_panicking() {
        let now = SystemTime::UNIX_EPOCH;
        let future = now + std::time::Duration::from_secs(90);
        assert_eq!(elapsed(future, now), "00:00");
    }

    #[test]
    fn several_casts_count_from_the_oldest_because_they_collapse_onto_one_chip() {
        let old = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(100);
        let new = old + std::time::Duration::from_secs(50);
        let casts = state(vec![
            ServiceUsage {
                since: new,
                stream_id: Some(2),
                ..usage(PrivacyResource::Screen)
            },
            ServiceUsage {
                since: old,
                stream_id: Some(1),
                ..usage(PrivacyResource::Screen)
            },
        ]);

        assert_eq!(
            screencast_since(&casts, Filters::default()),
            Some(old),
            "the screen has been shared continuously since the first cast began"
        );
    }

    #[test]
    fn a_hidden_screencast_starts_no_clock() {
        let casts = state(vec![usage(PrivacyResource::Screen)]);
        let filters = Filters {
            screencast: false,
            ..Filters::default()
        };

        assert_eq!(screencast_since(&casts, filters), None);
    }

    #[test]
    fn detail_joins_app_and_detail_when_both_are_known() {
        let usage = ServiceUsage {
            app: Some("chrome".to_owned()),
            detail: Some("DP-2".to_owned()),
            ..usage(PrivacyResource::Screen)
        };
        assert_eq!(detail(&usage).as_deref(), Some("chrome · DP-2"));
    }

    #[test]
    fn detail_falls_back_to_whichever_half_is_known() {
        let app_only = ServiceUsage {
            app: Some("chrome".to_owned()),
            ..usage(PrivacyResource::Microphone)
        };
        assert_eq!(detail(&app_only).as_deref(), Some("chrome"));

        let detail_only = ServiceUsage {
            detail: Some("DP-2".to_owned()),
            ..usage(PrivacyResource::Screen)
        };
        assert_eq!(detail(&detail_only).as_deref(), Some("DP-2"));
    }

    #[test]
    fn a_location_usage_carries_neither_half_and_has_no_detail() {
        assert_eq!(detail(&usage(PrivacyResource::Location)), None);
    }

    #[test]
    fn the_id_keys_a_screen_row_by_stream_id_not_by_session() {
        let a = ServiceUsage {
            stream_id: Some(3),
            session: Some(99),
            ..usage(PrivacyResource::Screen)
        };
        let b = ServiceUsage {
            stream_id: Some(3),
            session: None,
            ..usage(PrivacyResource::Screen)
        };
        assert_eq!(usage_id(&a), usage_id(&b));
    }

    #[test]
    fn the_id_keys_camera_and_microphone_by_app() {
        let chrome = ServiceUsage {
            app: Some("chrome".to_owned()),
            ..usage(PrivacyResource::Camera)
        };
        let ffmpeg = ServiceUsage {
            app: Some("ffmpeg".to_owned()),
            ..usage(PrivacyResource::Camera)
        };
        assert_ne!(usage_id(&chrome), usage_id(&ffmpeg));
    }

    #[test]
    fn location_always_has_the_same_id() {
        assert_eq!(
            usage_id(&usage(PrivacyResource::Location)),
            usage_id(&usage(PrivacyResource::Location))
        );
    }

    #[test]
    fn a_filtered_out_resource_never_reaches_the_popover() {
        let filters = Filters {
            camera: false,
            ..Filters::default()
        };
        let usages = usages(&state(vec![usage(PrivacyResource::Camera)]), filters);
        assert!(usages.is_empty());
    }

    #[test]
    fn a_filtered_out_resource_never_lights_a_chip() {
        let filters = Filters {
            location: false,
            ..Filters::default()
        };
        let kinds = kinds_in_use(&state(vec![usage(PrivacyResource::Location)]), filters);
        assert!(kinds.is_empty());
    }

    #[test]
    fn no_usages_lights_no_chips() {
        assert!(kinds_in_use(&state(Vec::new()), Filters::default()).is_empty());
    }

    #[test]
    fn every_kind_in_use_lights_its_own_chip() {
        let kinds = kinds_in_use(
            &state(vec![
                usage(PrivacyResource::Camera),
                usage(PrivacyResource::Screen),
            ]),
            Filters::default(),
        );
        assert_eq!(
            kinds,
            vec![PrivacyResource::Camera, PrivacyResource::Screen]
        );
    }

    #[test]
    fn the_banner_is_raised_by_a_screen_usage_with_no_stoppable_session() {
        let unstoppable = ServiceUsage {
            session: None,
            detail: Some("DP-2".to_owned()),
            ..usage(PrivacyResource::Screen)
        };
        assert_eq!(
            screen_shared(&state(vec![unstoppable]), Filters::default()).as_deref(),
            Some("DP-2"),
            "a capture that cannot be stopped must still raise the banner"
        );
    }

    #[test]
    fn the_banner_shows_with_no_subtitle_when_the_cast_has_no_detail() {
        let bare = usage(PrivacyResource::Screen);
        assert_eq!(
            screen_shared(&state(vec![bare]), Filters::default()).as_deref(),
            Some("")
        );
    }

    #[test]
    fn the_banner_names_every_cast_sharing_the_screen_at_once() {
        let one = ServiceUsage {
            stream_id: Some(1),
            detail: Some("DP-1".to_owned()),
            ..usage(PrivacyResource::Screen)
        };
        let two = ServiceUsage {
            stream_id: Some(2),
            detail: Some("DP-2".to_owned()),
            ..usage(PrivacyResource::Screen)
        };
        assert_eq!(
            screen_shared(&state(vec![one, two]), Filters::default()).as_deref(),
            Some("DP-1, DP-2"),
            "the subtitle is an incomplete truth if it names only the first of two active casts"
        );
    }

    #[test]
    fn no_screen_usage_means_no_banner_at_all() {
        assert_eq!(
            screen_shared(
                &state(vec![usage(PrivacyResource::Camera)]),
                Filters::default()
            ),
            None
        );
    }

    #[test]
    fn a_disabled_screencast_filter_hides_the_banner_too() {
        let filters = Filters {
            screencast: false,
            ..Filters::default()
        };
        assert_eq!(
            screen_shared(&state(vec![usage(PrivacyResource::Screen)]), filters),
            None
        );
    }

    #[test]
    fn the_tooltip_falls_back_to_the_title_with_no_detail_to_add() {
        let bare = usage(PrivacyResource::Location);
        assert_eq!(
            tooltip(PrivacyResource::Location, &[&bare], None),
            "Location"
        );
    }

    #[test]
    fn the_tooltip_lists_every_known_detail() {
        let chrome = ServiceUsage {
            app: Some("chrome".to_owned()),
            ..usage(PrivacyResource::Microphone)
        };
        let discord = ServiceUsage {
            app: Some("discord".to_owned()),
            ..usage(PrivacyResource::Microphone)
        };
        assert_eq!(
            tooltip(PrivacyResource::Microphone, &[&chrome, &discord], None),
            "Microphone — chrome, discord"
        );
    }

    #[test]
    fn a_configured_tooltip_format_replaces_the_composed_tooltip_outright() {
        let chrome = ServiceUsage {
            app: Some("chrome".to_owned()),
            ..usage(PrivacyResource::Microphone)
        };
        assert_eq!(
            tooltip(PrivacyResource::Microphone, &[&chrome], Some("Mic in use")),
            "Mic in use",
            "privacy has no per-resource token to fill, so a configured format is a plain override"
        );
    }
}
