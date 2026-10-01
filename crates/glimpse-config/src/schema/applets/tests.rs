use super::{
    BatteryIndicatorStyle, COMMON, Chip, Common, Kind, NotificationIndicatorStyle, Printing,
    Privacy,
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
fn battery_defaults_to_icon_only_and_reads_every_style() {
    let default: crate::Config =
        toml::from_str("[applets.battery]\n").expect("the built-in battery applet loads");
    let Kind::Battery(default) = &default.applets["battery"].kind else {
        panic!("battery resolves to its own kind");
    };
    assert_eq!(default.indicator_style, BatteryIndicatorStyle::IconOnly);
    assert_eq!(default.label_format, "{percentage}");

    for (value, expected) in [
        ("icon-only", BatteryIndicatorStyle::IconOnly),
        ("icon-text", BatteryIndicatorStyle::IconText),
        ("text", BatteryIndicatorStyle::Text),
    ] {
        let text = format!("[applets.battery]\nindicator-style = \"{value}\"\n");
        let document: crate::Config = toml::from_str(&text).expect("the style is valid");
        let Kind::Battery(settings) = &document.applets["battery"].kind else {
            panic!("battery resolves to its own kind");
        };
        assert_eq!(settings.indicator_style, expected);
    }

    toml::from_str::<crate::Config>("[applets.battery]\nindicator-style = \"percent\"\n")
        .expect_err("an undocumented spelling is refused");
    toml::from_str::<crate::Config>("[applets.battery]\nunknown = 1\n")
        .expect_err("a struct variant refuses keys that are not its own");
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
