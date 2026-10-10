use super::{
    BatteryIndicatorStyle, COMMON, Chip, Common, Kind, NotificationIndicatorStyle, Printing,
    PrintingShow, Privacy,
};

#[test]
fn exec_sandbox_resolves_paths_and_refuses_unknown_keys() {
    let config: super::Exec =
        toml::from_str("applet = \"me.x.A\"\noptions = {}\n[sandbox]\nread = [\".\"]\n")
            .expect("exec settings parse");
    let resolved = config
        .sandbox
        .resolve(std::path::Path::new("/a/b"))
        .expect("paths resolve");
    assert_eq!(resolved.read, ["/a/b"]);
    assert!(toml::from_str::<super::Exec>("applet = \"me.x.A\"\nunknown = true").is_err());
    assert!(toml::from_str::<super::Exec>("applet = \"me.x.A\"\n[sandbox]\nextra = true").is_err());
    assert!(
        toml::from_str::<super::Exec>("applet = \"me.x.A\"\n[sandbox]\nread = [\"relative\"]")
            .expect("relative dev grants parse")
            .sandbox
            .resolve(std::path::Path::new("relative-dir"))
            .is_err()
    );
}

#[test]
fn every_common_setting_is_taken_off_the_table() {
    let schema = schemars::schema_for!(Common);
    let declared: Vec<String> = schema
        .get("properties")
        .and_then(serde_json::Value::as_object)
        .expect("Common is an object")
        .keys()
        .cloned()
        .collect();

    for key in &declared {
        assert!(
            COMMON.contains(&key.as_str()),
            "`{key}` is a common setting the splitter never removes, so it reaches the kind \
                 and is refused as one of its own"
        );
    }
    assert_eq!(
        COMMON.len(),
        declared.len(),
        "the splitter removes a key no common setting declares"
    );
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
    assert_eq!(exec.sandbox, super::Sandbox::default());
}

#[test]
fn exec_sandbox_grants_parse_and_relative_paths_are_refused() {
    let config: crate::Config = toml::from_str(
        "[applets.\"me.example.Weather\"]\nsandbox = { read = [\"~/code\"], write = [\"/srv/x\"], network = true }\n",
    )
    .expect("absolute and home-relative grants load");
    let Kind::Exec(exec) = &config.applets["me.example.Weather"].kind else {
        panic!("the desktop-id table resolves to exec");
    };
    assert_eq!(exec.sandbox.read, ["~/code"]);
    assert_eq!(exec.sandbox.write, ["/srv/x"]);
    assert!(exec.sandbox.network);
    assert!(!exec.sandbox.unconfined);

    for bad in [
        "sandbox = { read = [\"code\"] }",
        "sandbox = { write = [\"~code\"] }",
        "sandbox = { exec = true }",
    ] {
        let error =
            toml::from_str::<crate::Config>(&format!("[applets.\"me.example.Weather\"]\n{bad}\n"))
                .expect_err(bad)
                .to_string();
        assert!(error.contains("me.example.Weather"), "{bad}: {error}");
    }
}

#[test]
fn desktop_id_schema_accepts_options_without_an_explicit_kind() {
    let schema: serde_json::Value =
        serde_json::from_str(&crate::json_schema_document()).expect("the schema is JSON");
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
        Kind::Places(super::Places { bookmarks: 4 })
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
        Kind::Removable(super::Removable { volumes: 4 })
    );
}

#[test]
fn kdeconnect_resolves_from_a_bare_name_and_reads_its_own_settings() {
    let bare = super::Applet::from_name("kdeconnect").expect("a known applet name");
    assert_eq!(bare.kind, Kind::Kdeconnect(super::Kdeconnect::default()));

    let configured: crate::Config = toml::from_str(
        "[applets.phone]\nextends = \"kdeconnect\"\nindicator-style = \"icon-only\"\n\
             label-format = \"{name}\"\ndevice = \"Pixel 8\"\nhide-when-disconnected = true\n",
    )
    .expect("the table loads");
    assert_eq!(
        configured.applets["phone"].kind,
        Kind::Kdeconnect(super::Kdeconnect {
            indicator_style: BatteryIndicatorStyle::IconOnly,
            label_format: "{name}".to_owned(),
            device: Some("Pixel 8".to_owned()),
            hide_when_disconnected: true,
        })
    );
    assert!(
        toml::from_str::<crate::Config>("[applets.p]\nextends = \"kdeconnect\"\nbattery = 1\n")
            .is_err(),
        "an unknown key under kdeconnect is refused"
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
fn legacy_applet_names_are_unknown() {
    for name in ["battery", "brightness"] {
        assert!(super::Applet::from_name(name).is_none(), "{name}");
        assert!(super::resolve_applet(name, &std::collections::BTreeMap::new()).is_none());
        for text in [
            format!("[applets.{name}]\n"),
            format!("[applets.custom]\nextends = \"{name}\"\n"),
        ] {
            toml::from_str::<crate::Config>(&text).expect_err("unknown applet kinds are refused");
        }
        let document: crate::Config =
            toml::from_str(&format!("[[panels]]\nleft = [\"{name}\"]\n")).unwrap();
        let unknown = crate::unknown_applets(&document);
        assert!(
            unknown
                .iter()
                .any(|message| message.contains(&format!("unknown applet `{name}`"))),
            "{unknown:?}"
        );
    }
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
    let document: serde_json::Value = serde_json::from_str(&crate::json_schema_document()).unwrap();
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
        serde_json::from_str(include_str!("../../../../../data/config.schema.json")).unwrap();
    assert_eq!(generated, document);
}

#[test]
fn a_repeated_chip_collapses_to_its_first_occurrence() {
    let document: crate::Config =
        toml::from_str("[applets.system-monitor]\nchips = [\"cpu\", \"cpu\", \"ram\"]\n")
            .expect("chips is a key of this table");
    let Kind::SystemMonitor(settings) = &document.applets["system-monitor"].kind else {
        panic!("system-monitor resolves to its own kind");
    };
    assert_eq!(settings.chips, vec![Chip::Cpu, Chip::Ram]);
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
    assert_eq!(settings.show, PrintingShow::Always);

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
        show_keyboard: true,
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
        toml::from_str("[applets.display]\nscroll-step = 12\nshow-keyboard = false\n")
            .expect("display settings load");
    assert_eq!(
        document.applets["display"].kind,
        Kind::Display(super::Display {
            scroll_step: 12,
            show_keyboard: false,
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
                show_keyboard: true,
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
