use std::collections::HashSet;

use gettextrs::gettext;
use glimpse_services::{PrivacyResource, PrivacyState, PrivacyUsage as ServiceUsage};
use glimpse_widgets::{PrivacyAction, PrivacyUsage};

pub const HERO: &str = "security-medium-symbolic";
pub const CAMERA: &str = "camera-web-symbolic";
pub const MICROPHONE: &str = "audio-input-microphone-symbolic";
pub const SCREEN: &str = "video-display-symbolic";
pub const LOCATION: &str = "find-location-symbolic";

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

/// A screen cast is only offered a stop control when it carries a session id: niri cannot stop a
/// wlr-screencopy cast through IPC, and Hyprland's synthetic casts carry no session id at all.
/// Offering the control otherwise would report success while the recording continues. The
/// microphone's mute is offered until it is already muted, so the row never shows a control that
/// changes nothing. A row whose command is already in flight offers no control at all, and neither
/// the camera nor the location resource has one.
pub fn action(usage: &ServiceUsage, mic_muted: bool, busy: bool) -> Option<PrivacyAction> {
    if busy {
        return None;
    }
    match usage.kind {
        PrivacyResource::Microphone => (!mic_muted).then_some(PrivacyAction::Mute),
        PrivacyResource::Screen => usage
            .session
            .is_some()
            .then_some(PrivacyAction::StopSharing),
        PrivacyResource::Camera | PrivacyResource::Location => None,
    }
}

pub fn usages(
    state: &PrivacyState,
    filters: Filters,
    mic_muted: bool,
    pending: &HashSet<String>,
) -> Vec<PrivacyUsage> {
    visible(state, filters)
        .map(|usage| {
            let id = usage_id(usage);
            let busy = pending.contains(&id);
            PrivacyUsage {
                icon: icon(usage.kind).to_owned(),
                title: title(usage.kind),
                detail: detail(usage),
                action: action(usage, mic_muted, busy),
                busy,
                id,
            }
        })
        .collect()
}

/// The banner is raised by the presence of any screen usage, whether or not it can be stopped: a
/// wlr-screencopy or Hyprland cast still records, and the applet's job is to tell the truth about
/// capture rather than only about what it can stop. The subtitle is the usage's own detail, empty
/// when the compositor gave none.
pub fn screen_shared(state: &PrivacyState, filters: Filters) -> Option<String> {
    visible(state, filters)
        .find(|usage| usage.kind == PrivacyResource::Screen)
        .map(|usage| detail(usage).unwrap_or_default())
}

pub fn session_for(state: &PrivacyState, id: &str) -> Option<u64> {
    state
        .usages
        .iter()
        .find(|usage| usage.kind == PrivacyResource::Screen && usage_id(usage) == id)
        .and_then(|usage| usage.session)
}

pub fn kinds_in_use(state: &PrivacyState, filters: Filters) -> Vec<PrivacyResource> {
    KINDS
        .into_iter()
        .filter(|&kind| filters.allows(kind) && state.usages.iter().any(|usage| usage.kind == kind))
        .collect()
}

pub fn tooltip(kind: PrivacyResource, usages: &[&ServiceUsage]) -> String {
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
    fn a_screen_usage_with_no_session_offers_no_stop_control() {
        let stoppable = ServiceUsage {
            session: Some(11),
            ..usage(PrivacyResource::Screen)
        };
        let unstoppable = ServiceUsage {
            session: None,
            ..usage(PrivacyResource::Screen)
        };
        assert_eq!(
            action(&stoppable, false, false),
            Some(PrivacyAction::StopSharing)
        );
        assert_eq!(action(&unstoppable, false, false), None);
    }

    #[test]
    fn an_already_muted_microphone_offers_no_mute_control() {
        assert_eq!(
            action(&usage(PrivacyResource::Microphone), false, false),
            Some(PrivacyAction::Mute)
        );
        assert_eq!(
            action(&usage(PrivacyResource::Microphone), true, false),
            None
        );
    }

    #[test]
    fn a_busy_row_offers_no_control_of_any_kind() {
        let stoppable = ServiceUsage {
            session: Some(11),
            ..usage(PrivacyResource::Screen)
        };
        assert_eq!(action(&stoppable, false, true), None);
        assert_eq!(
            action(&usage(PrivacyResource::Microphone), false, true),
            None
        );
    }

    #[test]
    fn camera_and_location_never_offer_a_control() {
        assert_eq!(action(&usage(PrivacyResource::Camera), false, false), None);
        assert_eq!(
            action(&usage(PrivacyResource::Location), false, false),
            None
        );
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
        let usages = usages(
            &state(vec![usage(PrivacyResource::Camera)]),
            filters,
            false,
            &HashSet::new(),
        );
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
    fn session_for_looks_up_a_screen_row_by_its_widget_id() {
        let usage = ServiceUsage {
            stream_id: Some(3),
            session: Some(42),
            ..usage(PrivacyResource::Screen)
        };
        let id = usage_id(&usage);
        assert_eq!(session_for(&state(vec![usage]), &id), Some(42));
        assert_eq!(session_for(&state(Vec::new()), &id), None);
    }

    #[test]
    fn the_tooltip_falls_back_to_the_title_with_no_detail_to_add() {
        let bare = usage(PrivacyResource::Location);
        assert_eq!(tooltip(PrivacyResource::Location, &[&bare]), "Location");
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
            tooltip(PrivacyResource::Microphone, &[&chrome, &discord]),
            "Microphone — chrome, discord"
        );
    }
}
