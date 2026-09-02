mod clock;
mod heartbeat;
mod pager;

use glimpse_config::Applet as AppletConfig;
use std::collections::BTreeMap;

use crate::applet::Applet;
use crate::applet::runtime::Builder;

pub fn resolve(
    name: &str,
    configured: &BTreeMap<String, AppletConfig>,
) -> Option<(AppletConfig, Builder)> {
    let config = configured
        .get(name)
        .cloned()
        .or_else(|| AppletConfig::from_name(name));
    let Some(config) = config else {
        tracing::warn!(applet = name, "unknown applet, skipping");
        return None;
    };
    let Some(builder) = build(&config) else {
        tracing::debug!(applet = name, "applet is not implemented yet, skipping");
        return None;
    };
    Some((config, builder))
}

fn build(config: &AppletConfig) -> Option<Builder> {
    match config {
        AppletConfig::Clock(_) => Some(|| Box::new(clock::Clock::start())),
        AppletConfig::Heartbeat {} => Some(|| Box::new(heartbeat::Heartbeat::start())),
        AppletConfig::Pager(_) => Some(|| Box::new(pager::Pager::start())),
        AppletConfig::Audio {}
        | AppletConfig::Battery {}
        | AppletConfig::Brightness {}
        | AppletConfig::Bluetooth {}
        | AppletConfig::Display {}
        | AppletConfig::Clipboard {}
        | AppletConfig::Command {}
        | AppletConfig::Exec {}
        | AppletConfig::Idle {}
        | AppletConfig::Keyboard {}
        | AppletConfig::Mpris {}
        | AppletConfig::Network {}
        | AppletConfig::NextEvent {}
        | AppletConfig::Notifications {}
        | AppletConfig::Privacy {}
        | AppletConfig::Printing {}
        | AppletConfig::Removable {}
        | AppletConfig::Session {}
        | AppletConfig::Tray {}
        | AppletConfig::Weather {} => None,
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
        assert!(resolve("heartbeat", &BTreeMap::new()).is_some());
    }

    #[test]
    fn extends_names_the_kind_so_one_kind_can_have_several_instances() {
        let configured = configured("pulse", AppletConfig::Heartbeat {});
        assert!(
            resolve("pulse", &configured).is_some(),
            "`pulse` is not a kind; `extends` is what says which one it is"
        );
        assert!(
            resolve("pulse", &BTreeMap::new()).is_none(),
            "without the entry the same name is just unknown"
        );
    }

    #[test]
    fn a_kind_without_an_implementation_is_not_the_same_as_a_typo() {
        assert!(
            AppletConfig::from_name("audio").is_some(),
            "`audio` is a real applet, so skipping it is expected rather than a bad document"
        );
        assert!(build(&AppletConfig::Audio {}).is_none());
        assert!(AppletConfig::from_name("nonesuch").is_none());
        assert!(resolve("nonesuch", &BTreeMap::new()).is_none());
    }
}
