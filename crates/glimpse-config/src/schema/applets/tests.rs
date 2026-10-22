use super::{
    BatteryIndicatorStyle, Kind, NotificationIndicatorStyle, Printing, PrintingShow, Privacy,
};

#[test]
fn exec_refuses_unknown_keys() {
    assert!(toml::from_str::<super::Exec>("applet = \"me.x.A\"\nunknown = true").is_err());
}

#[test]
fn exec_sandbox_table_is_refused_by_name() {
    let error = toml::from_str::<crate::Config>(
        "[applets.x]\nextends = \"exec\"\napplet = \"me.x.A\"\n[applets.x.sandbox]\nnetwork = true\n",
    )
    .expect_err("sandbox")
    .to_string();
    assert!(error.contains("sandbox"), "{error}");
}

#[test]
fn desktop_id_segments_require_ascii_identifier_starts() {
    for name in ["me.example.Pomodoro", "_me.example-2", "a.b.c"] {
        assert!(super::is_desktop_id(name), "{name}");
    }
    for name in [
        "exec",
        "me.",
        ".example",
        "me..example",
        "1me.example",
        "me.2example",
        "me.ex ample",
        "mé.example",
    ] {
        assert!(!super::is_desktop_id(name), "{name}");
    }
}

#[test]
fn desktop_id_table_without_extends_carries_options() {
    let config: crate::Config =
        toml::from_str("[applets.\"me.example.Pomodoro\".options]\nwork-minutes = 25\n")
            .expect("a desktop-id table loads as exec");
    let Kind::Exec(exec) = &config.applets["me.example.Pomodoro"].kind else {
        panic!("the desktop-id table resolves to exec");
    };
    assert_eq!(exec.applet, "me.example.Pomodoro");
    assert_eq!(exec.options["work-minutes"], 25);
}

#[test]
fn exec_env_parses_and_an_invalid_name_is_rejected() {
    let config: crate::Config = toml::from_str(
        "[applets.ruler]\nextends = \"exec\"\napplet = \"me.aresa.Ruler\"\nenv = { GSK_RENDERER = \"cairo\" }\n",
    )
    .expect("env parses");
    let Kind::Exec(exec) = &config.applets["ruler"].kind else {
        panic!("ruler is an exec applet");
    };
    assert_eq!(
        exec.env.get("GSK_RENDERER").map(String::as_str),
        Some("cairo")
    );

    let absent: crate::Config =
        toml::from_str("[applets.ruler]\nextends = \"exec\"\napplet = \"me.aresa.Ruler\"\n")
            .expect("env defaults to empty");
    let Kind::Exec(exec) = &absent.applets["ruler"].kind else {
        panic!("ruler is an exec applet");
    };
    assert!(exec.env.is_empty());

    for (table, key) in [
        ("env = { \"\" = \"x\" }\n", "\"\""),
        ("env = { \"A=B\" = \"x\" }\n", "A=B"),
    ] {
        let error = toml::from_str::<crate::Config>(&format!(
            "[applets.ruler]\nextends = \"exec\"\napplet = \"me.aresa.Ruler\"\n{table}"
        ))
        .expect_err(table)
        .to_string();
        assert!(error.contains("ruler"), "{table}{error}");
        assert!(error.contains(key), "{table}{error}");
    }

    let mut nul_name = std::collections::BTreeMap::new();
    nul_name.insert("A\0B".to_owned(), "x".to_owned());
    let error = super::validate_env(&nul_name).expect_err("a name containing NUL");
    assert!(error.contains("A\\0B"), "{error}");

    let mut nul_value = std::collections::BTreeMap::new();
    nul_value.insert("GSK_RENDERER".to_owned(), "cai\0ro".to_owned());
    let error = super::validate_env(&nul_value).expect_err("a value containing NUL");
    assert!(error.contains("GSK_RENDERER"), "{error}");
}

#[test]
fn desktop_id_schema_accepts_options_without_an_explicit_kind() {
    let schema: serde_json::Value = serde_json::from_str(&crate::json_schema_document(
        crate::Documents::ConfigAndPanel,
    ))
    .expect("the schema is JSON");
    let patterns = schema["properties"]["applets"]["patternProperties"]
        .as_object()
        .expect("desktop ids have their own schema");
    let (pattern, entry) = patterns.iter().next().expect("a desktop id pattern");
    assert!(pattern.starts_with("^[_A-Za-z]"));
    assert_eq!(entry["required"], serde_json::json!([]));
    assert_eq!(entry["properties"]["extends"]["const"], "exec");
    assert!(entry["properties"]["options"].is_object());
}

#[test]
fn explicit_exec_table_creates_a_named_instance() {
    let config: crate::Config =
        toml::from_str("[applets.pomo]\nextends = \"exec\"\napplet = \"me.example.Pomodoro\"\n")
            .expect("an explicit exec table loads");
    let Kind::Exec(exec) = &config.applets["pomo"].kind else {
        panic!("pomo resolves to exec");
    };
    assert_eq!(exec.applet, "me.example.Pomodoro");
    assert!(exec.options.is_empty());
}

#[test]
fn invalid_exec_applet_names_the_table_and_key() {
    let error = toml::from_str::<crate::Config>(
        "[applets.pomo]\nextends = \"exec\"\napplet = \"not an id\"\n",
    )
    .expect_err("an invalid desktop id must fail");
    let message = error.to_string();
    assert!(message.contains("[applets.pomo]"), "{message}");
    assert!(message.contains("applet"), "{message}");
}

#[test]
fn built_in_table_name_remains_its_kind() {
    let config: crate::Config =
        toml::from_str("[applets.clock]\n").expect("the built-in table loads");
    assert!(matches!(config.applets["clock"].kind, Kind::Clock(_)));
}

#[test]
fn places_still_resolves_from_a_bare_name_and_reads_its_own_settings() {
    let bare = super::Applet::from_name("places").expect("a known applet name");
    assert_eq!(bare.kind, Kind::Places(super::Places::default()));

    let configured: crate::Config =
        toml::from_str("[applets.pl]\nextends = \"places\"\nbookmarks = 4\n")
            .expect("the table loads");
    assert_eq!(
        configured.applets["pl"].kind,
        Kind::Places(super::Places {
            bookmarks: 4,
            tooltip_format: None
        })
    );
}

#[test]
fn removable_still_resolves_from_a_bare_name_and_reads_its_own_settings() {
    let bare = super::Applet::from_name("removable").expect("a known applet name");
    assert_eq!(bare.kind, Kind::Removable(super::Removable::default()));

    let configured: crate::Config =
        toml::from_str("[applets.rm]\nextends = \"removable\"\nvolumes = 4\n")
            .expect("the table loads");
    assert_eq!(
        configured.applets["rm"].kind,
        Kind::Removable(super::Removable {
            volumes: 4,
            tooltip_format: None
        })
    );
}

#[test]
fn bluetooth_still_resolves_from_a_bare_name_and_reads_its_own_settings() {
    let bare = super::Applet::from_name("bluetooth").expect("a known applet name");
    assert_eq!(bare.kind, Kind::Bluetooth(super::Bluetooth::default()));

    let configured: crate::Config =
        toml::from_str("[applets.bt]\nextends = \"bluetooth\"\ndevices = 4\n")
            .expect("the table loads");
    assert_eq!(
        configured.applets["bt"].kind,
        Kind::Bluetooth(super::Bluetooth {
            tooltip_format: None,
            devices: 4,
            nearby: 8,
        })
    );
}

#[test]
fn notification_indicator_style_defaults_to_a_dot_and_reads_every_variant() {
    let default: crate::Config = toml::from_str("[applets.notifications]\n")
        .expect("the built-in notification applet loads");
    let Kind::Notifications(default) = &default.applets["notifications"].kind else {
        panic!("notifications resolves to its own kind");
    };
    assert_eq!(default.indicator_style, NotificationIndicatorStyle::IconDot);

    for (value, expected) in [
        ("icon-only", NotificationIndicatorStyle::IconOnly),
        ("icon-dot", NotificationIndicatorStyle::IconDot),
        ("icon-counter", NotificationIndicatorStyle::IconCounter),
    ] {
        let text = format!("[applets.notifications]\nindicator-style = \"{value}\"\n");
        let document: crate::Config = toml::from_str(&text).expect("the style is valid");
        let Kind::Notifications(settings) = &document.applets["notifications"].kind else {
            panic!("notifications resolves to its own kind");
        };
        assert_eq!(settings.indicator_style, expected);
    }

    toml::from_str::<crate::Config>("[applets.notifications]\nindicator-style = \"counter\"\n")
        .expect_err("an undocumented spelling is refused");
}

#[test]
fn power_defaults_to_icon_only_and_reads_every_style() {
    let default: crate::Config =
        toml::from_str("[applets.power]\n").expect("the built-in power applet loads");
    let Kind::Power(default) = &default.applets["power"].kind else {
        panic!("power resolves to its own kind");
    };
    assert_eq!(default.indicator_style, BatteryIndicatorStyle::IconOnly);
    assert_eq!(default.label_format, "{percentage}");

    for (value, expected) in [
        ("icon-only", BatteryIndicatorStyle::IconOnly),
        ("icon-text", BatteryIndicatorStyle::IconText),
        ("text", BatteryIndicatorStyle::Text),
    ] {
        let text = format!("[applets.power]\nindicator-style = \"{value}\"\n");
        let document: crate::Config = toml::from_str(&text).expect("the style is valid");
        let Kind::Power(settings) = &document.applets["power"].kind else {
            panic!("power resolves to its own kind");
        };
        assert_eq!(settings.indicator_style, expected);
    }

    toml::from_str::<crate::Config>("[applets.power]\nindicator-style = \"percent\"\n")
        .expect_err("an undocumented spelling is refused");
    toml::from_str::<crate::Config>("[applets.power]\nunknown = 1\n")
        .expect_err("a struct variant refuses keys that are not its own");
}

#[test]
fn power_instances_load_with_defaults() {
    let document: crate::Config =
        toml::from_str("[applets.power]\n[applets.laptop]\nextends = \"power\"\n")
            .expect("power and a custom instance load");
    for name in ["power", "laptop"] {
        assert_eq!(
            document.applets[name].kind,
            Kind::Power(super::Power::default())
        );
    }
    assert_eq!(
        super::Applet::from_name("power").map(|applet| applet.kind),
        Some(Kind::Power(super::Power::default()))
    );
    assert!(
        crate::Panel::default()
            .right
            .iter()
            .any(|name| name == "power")
    );
    assert!(
        crate::Panel::default()
            .right
            .iter()
            .any(|name| name == "idle")
    );
}

#[test]
fn schema_only_accepts_canonical_applet_kinds() {
    let document: serde_json::Value = serde_json::from_str(&crate::json_schema_document(
        crate::Documents::ConfigAndPanel,
    ))
    .unwrap();
    let applets = &document["properties"]["applets"];
    let branches = applets["additionalProperties"]["oneOf"].as_array().unwrap();
    for (old, new) in [("battery", "power"), ("brightness", "display")] {
        assert!(
            branches
                .iter()
                .all(|branch| branch["properties"]["extends"]["const"] != old)
        );
        assert!(
            branches
                .iter()
                .any(|branch| branch["properties"]["extends"]["const"] == new)
        );
        assert!(applets["properties"].get(old).is_none());
    }
    let generated: serde_json::Value =
        serde_json::from_str(include_str!("../../../../../data/panel.schema.json")).unwrap();
    assert_eq!(generated, document);
}

#[test]
fn printing_defaults_and_refuses_an_unknown_key() {
    let default: crate::Config =
        toml::from_str("[applets.printing]\n").expect("the built-in printing applet loads");
    assert_eq!(
        default.applets["printing"].kind,
        Kind::Printing(Printing::default())
    );

    let configured: crate::Config = toml::from_str("[applets.printing]\njobs = 3\n")
        .expect("jobs is the printing applet's own setting");
    let Kind::Printing(settings) = &configured.applets["printing"].kind else {
        panic!("the table names the printing applet");
    };
    assert_eq!(settings.jobs, 3);
    assert_eq!(settings.show, PrintingShow::Jobs);

    for (value, show) in [
        ("always", PrintingShow::Always),
        ("jobs", PrintingShow::Jobs),
    ] {
        let document = format!("[applets.printing]\nshow = \"{value}\"\n");
        let configured: crate::Config = toml::from_str(&document).expect("show parses");
        let Kind::Printing(settings) = &configured.applets["printing"].kind else {
            panic!("the table names the printing applet");
        };
        assert_eq!(settings.show, show);
    }
    toml::from_str::<crate::Config>("[applets.printing]\nshow = \"never\"\n")
        .expect_err("show names one of two values");

    toml::from_str::<crate::Config>("[applets.printing]\nbogus = 1\n")
        .expect_err("a struct variant refuses keys that are not its own");
}

#[test]
fn privacy_defaults_and_refuses_an_unknown_key() {
    let default: crate::Config =
        toml::from_str("[applets.privacy]\n").expect("the built-in privacy applet loads");
    assert_eq!(
        default.applets["privacy"].kind,
        Kind::Privacy(Privacy::default())
    );

    let configured: crate::Config = toml::from_str("[applets.privacy]\nshow-location = false\n")
        .expect("show-location is the privacy applet's own setting");
    let Kind::Privacy(settings) = &configured.applets["privacy"].kind else {
        panic!("the table names the privacy applet");
    };
    assert!(!settings.show_location);

    toml::from_str::<crate::Config>("[applets.privacy]\nbogus = 1\n")
        .expect_err("a struct variant refuses keys that are not its own");
}

#[test]
fn display_instances_load_the_same_defaults() {
    let document: crate::Config =
        toml::from_str("[applets.display]\n[applets.screen]\nextends = \"display\"\n")
            .expect("display and a custom instance load");
    let expected = Kind::Display(super::Display {
        scroll_step: 5,
        tooltip_format: None,
    });
    for name in ["display", "screen"] {
        assert_eq!(document.applets[name].kind, expected);
    }
    assert_eq!(
        super::resolve_applet("display", &std::collections::BTreeMap::new())
            .expect("a bare zone name resolves")
            .kind,
        expected
    );
}

#[test]
fn display_settings_parse_and_validate() {
    let document: crate::Config =
        toml::from_str("[applets.display]\nscroll-step = 12\n").expect("display settings load");
    assert_eq!(
        document.applets["display"].kind,
        Kind::Display(super::Display {
            scroll_step: 12,
            tooltip_format: None
        })
    );
    for value in [1, 100] {
        let document: crate::Config =
            toml::from_str(&format!("[applets.display]\nscroll-step = {value}\n"))
                .expect("range endpoints load");
        assert_eq!(
            document.applets["display"].kind,
            Kind::Display(super::Display {
                scroll_step: value,
                tooltip_format: None
            })
        );
    }
    for setting in ["scroll-step = 0", "scroll-step = 101", "unknown = true"] {
        let error = toml::from_str::<crate::Config>(&format!("[applets.display]\n{setting}\n"))
            .expect_err("invalid display settings are refused")
            .to_string();
        assert!(error.contains("[applets.display]"), "{error}");
    }
}

#[test]
fn the_default_zones_place_display_once_without_brightness() {
    let panel = crate::Panel::default();
    let names: Vec<_> = panel
        .left
        .iter()
        .chain(&panel.center)
        .chain(&panel.right)
        .collect();
    assert_eq!(
        names
            .iter()
            .filter(|name| name.as_str() == "display")
            .count(),
        1
    );
    assert!(!names.iter().any(|name| name.as_str() == "brightness"));
}
