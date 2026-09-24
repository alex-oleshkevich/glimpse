use glimpse_dbus::upower::WarningLevel;
use glimpse_dbus::weather::{WatchedPlace, WeatherProviderState, reading};
use glimpse_services::{BatteryState, BluetoothState, KeyboardLayouts, NetworkState};
use glimpse_utils::clean;
use glimpse_widgets::{IndicatorSpec, Severity};
use gtk4::gio;
use gtk4::prelude::{Cast, IconExt};

const LAYOUT_CAP: usize = 8;

#[derive(Debug, Clone, Default)]
pub struct Status {
    pub weather: Option<IndicatorSpec>,
    pub battery: Option<IndicatorSpec>,
    pub layout: Option<IndicatorSpec>,
    pub bluetooth: Option<IndicatorSpec>,
    pub network: Option<IndicatorSpec>,
}

pub fn shown(status: &Status, enabled: bool) -> Status {
    match enabled {
        true => status.clone(),
        false => Status::default(),
    }
}

pub fn battery(state: &BatteryState) -> Option<IndicatorSpec> {
    let charge = state.display.as_ref()?;
    Some(IndicatorSpec {
        icon: Some(themed(&charge.icon_name())),
        label: Some(format!("{}%", charge.percentage)),
        severity: severity(charge.warning),
        ..Default::default()
    })
}

pub fn network(state: &NetworkState) -> Option<IndicatorSpec> {
    state.icon_name().map(icon_only)
}

pub fn bluetooth(state: &BluetoothState) -> Option<IndicatorSpec> {
    state.icon_name().map(icon_only)
}

pub fn layout(layouts: &KeyboardLayouts) -> Option<IndicatorSpec> {
    if layouts.layouts.len() < 2 {
        return None;
    }
    let current = layouts
        .current
        .and_then(|index| layouts.layouts.get(usize::from(index)))
        .or_else(|| layouts.layouts.first())?;
    Some(IndicatorSpec {
        label: Some(clean(&current.code, LAYOUT_CAP)),
        ..Default::default()
    })
}

pub fn weather(state: &WeatherProviderState) -> Option<IndicatorSpec> {
    if !state.owner {
        return None;
    }
    let places = &state.status.as_ref()?.places;
    let current = places
        .iter()
        .find(|place| place.place == WatchedPlace::Here && place.current.is_some())
        .or_else(|| places.iter().find(|place| place.current.is_some()))?
        .current
        .as_ref()?;
    Some(IndicatorSpec {
        icon: Some(themed(current.condition.icon_name(current.is_day))),
        label: Some(reading(current.temperature)),
        ..Default::default()
    })
}

pub fn same(a: Option<&IndicatorSpec>, b: Option<&IndicatorSpec>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => {
            let icons = match (&a.icon, &b.icon) {
                (None, None) => true,
                (Some(a), Some(b)) => a.equal(Some(b)),
                _ => false,
            };
            icons && a.label == b.label && a.severity == b.severity
        }
        _ => false,
    }
}

pub fn replace(slot: &mut Option<IndicatorSpec>, next: Option<IndicatorSpec>) -> bool {
    if same(slot.as_ref(), next.as_ref()) {
        return false;
    }
    *slot = next;
    true
}

fn severity(level: WarningLevel) -> Option<Severity> {
    match level {
        WarningLevel::Low => Some(Severity::Warning),
        WarningLevel::Critical | WarningLevel::Action => Some(Severity::Error),
        WarningLevel::Unknown | WarningLevel::None | WarningLevel::Discharging => None,
    }
}

fn icon_only(name: &str) -> IndicatorSpec {
    IndicatorSpec {
        icon: Some(themed(name)),
        ..Default::default()
    }
}

fn themed(name: &str) -> gio::Icon {
    gio::ThemedIcon::new(name).upcast()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use glimpse_dbus::bluez::Power;
    use glimpse_dbus::network_manager as nm;
    use glimpse_dbus::upower::ChargeState;
    use glimpse_dbus::weather::{
        Condition, CurrentWeather, GeoCoordinates, PlaceWeather, UnitSystem, WeatherStatus,
    };
    use glimpse_services::{Access, Adapter, Charge, KeyboardLayout, NetworkId, Radio};

    fn icon_name(spec: &IndicatorSpec) -> Option<String> {
        let icon = spec.icon.as_ref()?.downcast_ref::<gio::ThemedIcon>()?;
        icon.names().first().map(ToString::to_string)
    }

    fn charge(percentage: u8, warning: WarningLevel) -> Charge {
        Charge {
            percentage,
            state: ChargeState::Discharging,
            icon_name: String::new(),
            time_to_empty: None,
            time_to_full: None,
            energy_rate_mw: None,
            warning,
        }
    }

    fn on_battery(charge: Charge) -> BatteryState {
        BatteryState {
            display: Some(charge),
            ..Default::default()
        }
    }

    fn layouts(codes: &[&str], current: Option<u8>) -> KeyboardLayouts {
        KeyboardLayouts {
            layouts: codes
                .iter()
                .map(|code| KeyboardLayout {
                    code: (*code).to_owned(),
                    name: format!("{code} layout"),
                })
                .collect(),
            current,
        }
    }

    fn place(place: WatchedPlace, current: Option<CurrentWeather>) -> PlaceWeather {
        PlaceWeather {
            place,
            coordinates: GeoCoordinates {
                latitude: 50.0,
                longitude: 14.0,
            },
            city: Some("Prague".to_owned()),
            country_code: Some("CZ".to_owned()),
            utc_offset_seconds: 0,
            current,
            hours: Vec::new(),
            days: Vec::new(),
            alerts: Vec::new(),
        }
    }

    fn reading_now(condition: Condition, is_day: bool, temperature: f64) -> CurrentWeather {
        CurrentWeather {
            observed_at: Utc::now(),
            condition,
            is_day,
            temperature,
            apparent_temperature: None,
            humidity: None,
            wind_speed: None,
            wind_direction: None,
            precipitation: None,
        }
    }

    fn provider(places: Vec<PlaceWeather>) -> WeatherProviderState {
        WeatherProviderState {
            status: Some(WeatherStatus {
                units: UnitSystem::Metric,
                places,
                updated_at: None,
            }),
            available: true,
            stale: false,
            reason: None,
            owner: true,
        }
    }

    #[test]
    fn the_battery_shows_its_charge_icon_and_percentage() {
        let charge = charge(84, WarningLevel::Discharging);
        let spec = battery(&on_battery(charge.clone())).expect("a battery slot");
        assert_eq!(icon_name(&spec), Some(charge.icon_name()));
        assert_eq!(spec.label.as_deref(), Some("84%"));
        assert_eq!(spec.severity, None);
        assert!(!spec.attention);
    }

    #[test]
    fn a_battery_icon_named_by_upower_wins() {
        let mut charge = charge(40, WarningLevel::None);
        charge.icon_name = "battery-caution-symbolic".to_owned();
        let spec = battery(&on_battery(charge)).expect("a battery slot");
        assert_eq!(
            icon_name(&spec).as_deref(),
            Some("battery-caution-symbolic")
        );
    }

    #[test]
    fn no_display_device_hides_the_battery() {
        assert!(battery(&BatteryState::default()).is_none());
    }

    #[test]
    fn a_battery_warning_maps_to_severity_and_never_to_attention() {
        let cases = [
            (WarningLevel::Unknown, None),
            (WarningLevel::None, None),
            (WarningLevel::Discharging, None),
            (WarningLevel::Low, Some(Severity::Warning)),
            (WarningLevel::Critical, Some(Severity::Error)),
            (WarningLevel::Action, Some(Severity::Error)),
        ];
        for (warning, expected) in cases {
            let spec = battery(&on_battery(charge(5, warning))).expect("a battery slot");
            assert_eq!(spec.severity, expected, "{warning:?}");
            assert!(
                !spec.attention,
                "{warning:?} never pulses on the lock screen"
            );
        }
    }

    #[test]
    fn the_network_shows_its_icon_and_never_an_ssid() {
        let state = NetworkState {
            networking: true,
            wifi: Some(Radio {
                enabled: true,
                hardware_enabled: true,
            }),
            networks: vec![Access {
                id: NetworkId::new("/ap/1"),
                ssid: Some("Skylink".to_owned()),
                bssid: None,
                strength: 70,
                band: nm::Band::Five,
                security: nm::Security::Wpa2,
                active: true,
                saved: None,
                address: None,
                busy: None,
                failure: None,
            }],
            ..Default::default()
        };
        let spec = network(&state).expect("a network slot");
        assert_eq!(icon_name(&spec).as_deref(), state.icon_name());
        assert_eq!(spec.label, None);
        assert_eq!(spec.tooltip, None);
        assert_eq!(spec.badge, None);
    }

    #[test]
    fn networking_off_still_shows_the_offline_icon() {
        let state = NetworkState::default();
        let spec = network(&state).expect("a network slot");
        assert_eq!(icon_name(&spec).as_deref(), state.icon_name());
    }

    #[test]
    fn no_adapter_hides_bluetooth() {
        assert!(bluetooth(&BluetoothState::default()).is_none());
    }

    #[test]
    fn an_adapter_shows_the_bluetooth_icon() {
        let state = BluetoothState {
            adapter: Some(Adapter {
                alias: "laptop".to_owned(),
                power: Power::On,
                discoverable: false,
            }),
            ..Default::default()
        };
        let spec = bluetooth(&state).expect("a bluetooth slot");
        assert_eq!(icon_name(&spec).as_deref(), state.icon_name());
        assert_eq!(spec.label, None);
    }

    #[test]
    fn fewer_than_two_layouts_hide_the_slot() {
        assert!(layout(&layouts(&[], None)).is_none());
        assert!(layout(&layouts(&["us"], Some(0))).is_none());
    }

    #[test]
    fn two_layouts_show_the_current_code() {
        let spec = layout(&layouts(&["us", "ru"], Some(1))).expect("a layout slot");
        assert_eq!(spec.label.as_deref(), Some("ru"));
        assert!(spec.icon.is_none());
    }

    #[test]
    fn an_unknown_current_layout_falls_back_to_the_first() {
        let unset = layout(&layouts(&["us", "ru"], None)).expect("a layout slot");
        assert_eq!(unset.label.as_deref(), Some("us"));
        let out_of_range = layout(&layouts(&["us", "ru"], Some(9))).expect("a layout slot");
        assert_eq!(out_of_range.label.as_deref(), Some("us"));
    }

    #[test]
    fn a_layout_code_is_cleaned_and_capped() {
        let spec = layout(&layouts(&["us", "a\u{202e}very-long-layout-code"], Some(1)))
            .expect("a layout slot");
        let label = spec.label.expect("a label");
        assert_eq!(label.chars().count(), LAYOUT_CAP + 1, "{label}");
        assert!(!label.contains('\u{202e}'));
        assert!(label.ends_with('…'));
    }

    #[test]
    fn weather_here_shows_the_condition_and_reading() {
        let state = provider(vec![
            place(
                WatchedPlace::Location {
                    name: "Oslo".to_owned(),
                },
                Some(reading_now(Condition::Snow, true, -3.0)),
            ),
            place(
                WatchedPlace::Here,
                Some(reading_now(Condition::ClearSky, false, 13.6)),
            ),
        ]);
        let spec = weather(&state).expect("a weather slot");
        assert_eq!(
            icon_name(&spec).as_deref(),
            Some(Condition::ClearSky.icon_name(false))
        );
        assert_eq!(spec.label, Some(reading(13.6)));
        assert_eq!(spec.severity, None);
    }

    #[test]
    fn a_fixed_place_alone_reaches_the_island() {
        let elsewhere = provider(vec![place(
            WatchedPlace::Location {
                name: "Oslo".to_owned(),
            },
            Some(reading_now(Condition::Snow, true, -3.0)),
        )]);
        let spec = weather(&elsewhere).expect("the panel's fixed place");
        assert_eq!(
            icon_name(&spec).as_deref(),
            Some(Condition::Snow.icon_name(true))
        );
        assert_eq!(spec.label, Some(reading(-3.0)));
    }

    #[test]
    fn here_without_a_reading_gives_way_to_the_first_place_with_one() {
        let state = provider(vec![
            place(WatchedPlace::Here, None),
            place(
                WatchedPlace::Coordinates {
                    latitude: 1.0,
                    longitude: 2.0,
                },
                None,
            ),
            place(
                WatchedPlace::Location {
                    name: "Oslo".to_owned(),
                },
                Some(reading_now(Condition::Fog, true, 4.0)),
            ),
        ]);
        let spec = weather(&state).expect("the first place with a reading");
        assert_eq!(spec.label, Some(reading(4.0)));
    }

    #[test]
    fn weather_without_any_reading_is_hidden() {
        let no_reading = provider(vec![place(WatchedPlace::Here, None)]);
        assert!(weather(&no_reading).is_none());

        let no_status = WeatherProviderState {
            status: None,
            ..provider(Vec::new())
        };
        assert!(weather(&no_status).is_none());
    }

    #[test]
    fn weather_from_an_absent_provider_is_hidden_even_when_retained() {
        let mut state = provider(vec![place(
            WatchedPlace::Here,
            Some(reading_now(Condition::Rain, true, 8.0)),
        )]);
        state.owner = false;
        state.available = false;
        state.stale = true;
        state.reason = Some("provider has no bus owner".to_owned());
        assert!(
            weather(&state).is_none(),
            "no trouble chip on the lock screen"
        );
    }

    #[test]
    fn weather_alerts_never_colour_the_island() {
        let mut here = place(
            WatchedPlace::Here,
            Some(reading_now(Condition::Thunderstorm, true, 20.0)),
        );
        here.alerts = vec![glimpse_dbus::weather::WeatherAlert {
            severity: glimpse_dbus::weather::AlertSeverity::Extreme,
            headline: "Storm".to_owned(),
            description: None,
            source: None,
            starts_at: None,
            expires_at: None,
        }];
        let mut state = provider(vec![here]);
        state.stale = true;
        let spec = weather(&state).expect("a weather slot");
        assert_eq!(spec.severity, None);
        assert_eq!(
            icon_name(&spec).as_deref(),
            Some(Condition::Thunderstorm.icon_name(true))
        );
    }

    #[test]
    fn same_compares_icon_label_and_severity() {
        let low = battery(&on_battery(charge(9, WarningLevel::Low)));
        assert!(same(low.as_ref(), low.clone().as_ref()));
        assert!(
            same(
                low.as_ref(),
                battery(&on_battery(charge(9, WarningLevel::Low))).as_ref()
            ),
            "a freshly built icon of the same name is the same"
        );
        assert!(same(None, None));
        assert!(!same(low.as_ref(), None));
        assert!(!same(
            low.as_ref(),
            battery(&on_battery(charge(8, WarningLevel::Low))).as_ref()
        ));
        assert!(!same(
            low.as_ref(),
            battery(&on_battery(charge(9, WarningLevel::Critical))).as_ref()
        ));
        let mut other_icon = charge(9, WarningLevel::Low);
        other_icon.icon_name = "battery-caution-symbolic".to_owned();
        assert!(!same(
            low.as_ref(),
            battery(&on_battery(other_icon)).as_ref()
        ));

        let mut slot = low.clone();
        assert!(
            !replace(&mut slot, low.clone()),
            "an equal spec is no change"
        );
        assert!(replace(&mut slot, None));
        assert!(slot.is_none());
    }

    fn full() -> Status {
        Status {
            weather: Some(IndicatorSpec::default()),
            battery: Some(IndicatorSpec::default()),
            layout: Some(IndicatorSpec::default()),
            bluetooth: Some(IndicatorSpec::default()),
            network: Some(IndicatorSpec::default()),
        }
    }

    #[test]
    fn disabled_status_hides_all_five_slots() {
        let hidden = shown(&full(), false);
        assert!(hidden.weather.is_none());
        assert!(hidden.battery.is_none());
        assert!(hidden.layout.is_none());
        assert!(hidden.bluetooth.is_none());
        assert!(hidden.network.is_none());
    }

    #[test]
    fn enabled_status_passes_every_slot_through() {
        let status = full();
        let visible = shown(&status, true);
        assert!(visible.weather.is_some());
        assert!(visible.battery.is_some());
        assert!(visible.layout.is_some());
        assert!(visible.bluetooth.is_some());
        assert!(visible.network.is_some());
    }
}
