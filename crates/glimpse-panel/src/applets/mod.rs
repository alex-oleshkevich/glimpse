mod agenda;
mod clock;
mod heartbeat;
mod keyboard;
mod mpris;
mod next_event;
mod notifications;
mod pager;
mod weather;

use glimpse_config::{Applet as AppletConfig, AppletKind, Regional};
use std::collections::BTreeMap;

use crate::applet::Applet;
use crate::applet::runtime::Builder;

pub fn resolve(
    name: &str,
    configured: &BTreeMap<String, AppletConfig>,
    regional: &Regional,
) -> Option<(AppletConfig, Builder)> {
    let config = configured
        .get(name)
        .cloned()
        .or_else(|| AppletConfig::from_name(name));
    let Some(mut config) = config else {
        tracing::warn!(applet = name, "unknown applet, skipping");
        return None;
    };
    config.regional = regional.clone();
    let Some(builder) = build(&config) else {
        tracing::debug!(applet = name, "applet is not implemented yet, skipping");
        return None;
    };
    Some((config, builder))
}

fn build(config: &AppletConfig) -> Option<Builder> {
    match &config.kind {
        AppletKind::Clock(_) => Some(|| Box::new(clock::Clock::start())),
        AppletKind::Heartbeat {} => Some(|| Box::new(heartbeat::Heartbeat::start())),
        AppletKind::Mpris(_) => Some(|| Box::new(mpris::Mpris::start())),
        AppletKind::NextEvent(_) => Some(|| Box::new(next_event::NextEvent::start())),
        AppletKind::Pager(_) => Some(|| Box::new(pager::Pager::start())),
        AppletKind::Weather(_) => Some(|| Box::new(weather::Weather::start())),
        AppletKind::Keyboard {} => Some(|| Box::new(keyboard::Keyboard::start())),
        AppletKind::Notifications(_) => Some(|| Box::new(notifications::Notifications::start())),
        AppletKind::Audio {}
        | AppletKind::Battery {}
        | AppletKind::Brightness {}
        | AppletKind::Bluetooth {}
        | AppletKind::Display {}
        | AppletKind::Clipboard {}
        | AppletKind::Command {}
        | AppletKind::Exec {}
        | AppletKind::Idle {}
        | AppletKind::Network {}
        | AppletKind::Privacy {}
        | AppletKind::Printing {}
        | AppletKind::Removable {}
        | AppletKind::Session {}
        | AppletKind::Tray {} => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn configured(name: &str, extends: AppletConfig) -> BTreeMap<String, AppletConfig> {
        BTreeMap::from([(name.to_owned(), extends)])
    }

    #[test]
    fn a_name_the_panel_implements_resolves_to_a_builder() {
        assert!(resolve("heartbeat", &BTreeMap::new(), &Regional::default()).is_some());
    }

    #[test]
    fn extends_names_the_kind_so_one_kind_can_have_several_instances() {
        let configured = configured("pulse", AppletKind::Heartbeat {}.into());
        assert!(
            resolve("pulse", &configured, &Regional::default()).is_some(),
            "`pulse` is not a kind; `extends` is what says which one it is"
        );
        assert!(
            resolve("pulse", &BTreeMap::new(), &Regional::default()).is_none(),
            "without the entry the same name is just unknown"
        );
    }

    #[test]
    fn the_next_event_applet_is_built_rather_than_skipped() {
        assert!(
            resolve("next-event", &BTreeMap::new(), &Regional::default()).is_some(),
            "the kind has an implementation, so it must not fall through to the skipped arm"
        );
    }

    #[test]
    fn the_weather_applet_is_built_rather_than_skipped() {
        assert!(
            resolve("weather", &BTreeMap::new(), &Regional::default()).is_some(),
            "the kind has an implementation, so it must not fall through to the skipped arm"
        );
    }

    #[test]
    fn the_keyboard_applet_is_built_rather_than_skipped() {
        assert!(
            resolve("keyboard", &BTreeMap::new(), &Regional::default()).is_some(),
            "the kind has an implementation, so it must not fall through to the skipped arm"
        );
    }

    #[test]
    fn the_notifications_applet_is_built_rather_than_skipped() {
        assert!(
            resolve("notifications", &BTreeMap::new(), &Regional::default()).is_some(),
            "the kind has an implementation, so it must not fall through to the skipped arm"
        );
    }

    #[test]
    fn a_kind_without_an_implementation_is_not_the_same_as_a_typo() {
        assert!(
            AppletConfig::from_name("audio").is_some(),
            "`audio` is a real applet, so skipping it is expected rather than a bad document"
        );
        assert!(build(&AppletKind::Audio {}.into()).is_none());
        assert!(AppletConfig::from_name("nonesuch").is_none());
        assert!(resolve("nonesuch", &BTreeMap::new(), &Regional::default()).is_none());
    }

    /// The shipped defaults name applets in a panel zone and carry no `[applets.<name>]` table for
    /// them, so every applet on a default bar is built by `from_name` rather than found in the
    /// map. That path invents a config, and an invented config that kept `Regional::default()`
    /// would silently ignore an explicit `hour-format`.
    #[test]
    fn an_applet_with_no_table_of_its_own_still_carries_the_document_s_regional() {
        let twelve = Regional {
            hour_format: glimpse_config::HourFormat::Twelve,
            ..Regional::default()
        };

        let (built, _) = resolve("clock", &BTreeMap::new(), &twelve).expect("the clock builds");
        assert!(
            built.regional.twelve_hour(),
            "an applet with no table of its own must still read the document's `[regional]`"
        );

        let (found, _) = resolve(
            "clock",
            &configured("clock", AppletKind::Clock(<_>::default()).into()),
            &twelve,
        )
        .expect("the clock builds");
        assert!(
            found.regional.twelve_hour(),
            "and so must one that has a table"
        );
    }
}
