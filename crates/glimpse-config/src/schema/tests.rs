use super::*;

#[test]
fn typos_and_bare_exec_remain_unknown_applets() {
    for name in ["clcok", "exec"] {
        let source = format!("[[panels]]\ncenter = [\"{name}\"]\n");
        let config: Config = toml::from_str(&source).expect("the document parses");
        let unknown = crate::load::unknown_applets(&config);
        assert!(
            unknown
                .iter()
                .any(|message| message.contains("unknown applet")),
            "the zone name must be reported: {unknown:?}"
        );
    }
}

#[test]
fn desktop_id_in_zone_resolves_and_is_placed() {
    let config: Config = toml::from_str("[[panels]]\ncenter = [\"me.example.Pomodoro\"]\n")
        .expect("the document parses");
    assert!(
        crate::load::unknown_applets(&config).is_empty(),
        "the desktop id is known"
    );
    let (name, applet) = placed_applets(&config)
        .find(|(name, _)| *name == "me.example.Pomodoro")
        .expect("the placed applet resolves");
    assert_eq!(name, "me.example.Pomodoro");
    assert_eq!(
        applet.kind,
        AppletKind::Exec(Box::new(ExecAppletConfig {
            applet: "me.example.Pomodoro".to_owned(),
            options: serde_json::Map::new(),
            env: std::collections::BTreeMap::new(),
        }))
    );
}

#[test]
fn placed_kinds_follows_zone_placement_not_table_presence() {
    let default = Config::default();
    assert!(
        !placed_applets(&default)
            .map(|(_, applet)| applet.kind)
            .any(|kind| matches!(kind, AppletKind::Heartbeat {})),
        "the default document places nothing named heartbeat"
    );

    let named_in_a_zone_with_no_table: Config =
        toml::from_str("[[panels]]\nright = [\"heartbeat\"]\n")
            .expect("a bare name resolves through Applet::from_name");
    assert!(
        placed_applets(&named_in_a_zone_with_no_table)
            .map(|(_, applet)| applet.kind)
            .any(|kind| matches!(kind, AppletKind::Heartbeat {})),
        "a name placed in a zone with no table entry must still resolve, the common case"
    );

    let table_with_no_placement: Config = toml::from_str("[applets.heartbeat]\n")
        .expect("a table with no zone naming it still loads");
    assert!(
        !placed_applets(&table_with_no_placement)
            .map(|(_, applet)| applet.kind)
            .any(|kind| matches!(kind, AppletKind::Heartbeat {})),
        "a table nobody places is not demand"
    );

    let renamed_and_placed: Config =
        toml::from_str("[applets.hb]\nextends = \"heartbeat\"\n\n[[panels]]\nright = [\"hb\"]\n")
            .expect("a renamed table placed in a zone loads");
    assert!(
        placed_applets(&renamed_and_placed)
            .map(|(_, applet)| applet.kind)
            .any(|kind| matches!(kind, AppletKind::Heartbeat {})),
        "a renamed table placed in a zone must still resolve to its kind"
    );
}

#[test]
fn the_reference_files_match_the_compiled_in_defaults() {
    assert_eq!(
        include_str!("../../../../data/config.default.toml"),
        crate::default_document(crate::Documents::Config)
    );
    assert_eq!(
        include_str!("../../../../data/panel.default.toml"),
        crate::default_document(crate::Documents::ConfigAndPanel)
    );
}

#[test]
fn the_commented_reference_files_match_the_compiled_in_renderer() {
    assert_eq!(
        include_str!("../../../../data/config.commented.toml"),
        crate::commented_document(crate::Documents::Config)
    );
    assert_eq!(
        include_str!("../../../../data/panel.commented.toml"),
        crate::commented_document(crate::Documents::ConfigAndPanel)
    );
}

#[test]
fn the_json_schemas_match_the_compiled_in_types() {
    assert_eq!(
        include_str!("../../../../data/config.schema.json"),
        crate::json_schema_document(crate::Documents::Config)
    );
    assert_eq!(
        include_str!("../../../../data/panel.schema.json"),
        crate::json_schema_document(crate::Documents::ConfigAndPanel)
    );
}

#[test]
fn each_schema_offers_only_its_own_tables() {
    for (documents, offered) in [
        (crate::Documents::Config, false),
        (crate::Documents::ConfigAndPanel, true),
    ] {
        let schema: serde_json::Value =
            serde_json::from_str(&crate::json_schema_document(documents)).expect("JSON");
        let properties = schema["properties"].as_object().expect("properties");
        assert!(!properties.is_empty());
        assert_eq!(schema["additionalProperties"], false);
        for table in properties.keys() {
            assert_eq!(
                crate::PANEL_TABLES.contains(&table.as_str()),
                offered,
                "{table}"
            );
        }
    }
}

#[test]
fn each_schema_keeps_only_the_definitions_its_tables_reach() {
    let config = json_schema(crate::Documents::Config);
    let panel = json_schema(crate::Documents::ConfigAndPanel);

    for schema in [&config, &panel] {
        let definitions = schema["$defs"].as_object().expect("$defs");
        let mut refs = Vec::new();
        collect_refs(schema, &mut refs);
        for reference in refs {
            let name = reference
                .strip_prefix("#/$defs/")
                .expect("a reference into $defs");
            assert!(
                definitions.contains_key(name),
                "{reference} does not resolve"
            );
        }
    }

    assert!(config["$defs"].get("Panel").is_none());
    assert!(panel["$defs"].get("Appearance").is_none());
}

fn json_schema(documents: crate::Documents) -> serde_json::Value {
    serde_json::from_str(&crate::json_schema_document(documents)).expect("JSON")
}

fn collect_refs(value: &serde_json::Value, refs: &mut Vec<String>) {
    match value {
        serde_json::Value::Object(map) => {
            if let Some(reference) = map.get("$ref").and_then(serde_json::Value::as_str) {
                refs.push(reference.to_owned());
            }
            for child in map.values() {
                collect_refs(child, refs);
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                collect_refs(item, refs);
            }
        }
        _ => {}
    }
}

#[test]
fn each_default_document_holds_only_its_own_tables() {
    let config: toml::Table = crate::default_document(crate::Documents::Config)
        .parse()
        .expect("TOML");
    let panel: toml::Table = crate::default_document(crate::Documents::ConfigAndPanel)
        .parse()
        .expect("TOML");

    assert!(
        crate::PANEL_TABLES
            .iter()
            .all(|table| !config.contains_key(*table))
    );
    assert!(config.contains_key("lock"));
    assert_eq!(panel.keys().collect::<Vec<_>>(), ["applets", "panels"]);
    assert_eq!(
        toml::from_str::<crate::Config>(&crate::default_document(crate::Documents::Config))
            .expect("loads"),
        crate::Config::default()
    );
}

#[test]
fn every_key_and_enum_value_is_kebab_case() {
    let mut offenders = Vec::new();
    for documents in [crate::Documents::Config, crate::Documents::ConfigAndPanel] {
        let schema: serde_json::Value =
            serde_json::from_str(&crate::json_schema_document(documents))
                .expect("the schema is JSON");
        collect_underscored(&schema, &mut offenders);
    }

    assert!(
        offenders.is_empty(),
        "the document is kebab-case; add `rename_all` to the type owning {offenders:?}"
    );
}

fn collect_underscored(node: &serde_json::Value, offenders: &mut Vec<String>) {
    match node {
        serde_json::Value::Object(map) => {
            if let Some(properties) = map.get("properties").and_then(|node| node.as_object()) {
                offenders.extend(properties.keys().filter(|key| key.contains('_')).cloned());
            }
            if let Some(values) = map.get("enum").and_then(|node| node.as_array()) {
                offenders.extend(
                    values
                        .iter()
                        .filter_map(|value| value.as_str())
                        .filter(|value| value.contains('_'))
                        .map(str::to_owned),
                );
            }
            for value in map.values() {
                collect_underscored(value, offenders);
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                collect_underscored(item, offenders);
            }
        }
        _ => {}
    }
}

#[test]
fn the_table_name_selects_the_applet_when_extends_is_absent() {
    let parsed: Config = toml::from_str("[applets.clock]\n").expect("the key names the kind");

    assert_eq!(
        parsed.applets["clock"],
        Applet::from(AppletKind::Clock(applets::Clock::default()))
    );
}

#[test]
fn the_next_event_applet_carries_its_own_settings() {
    let parsed: Config = toml::from_str(
        "[applets.next-event]\nwithin = 15\ncountdown = 5\nhorizon = 240\nall-day = true\nupcoming = 3\n",
    )
    .expect("the table names the kind and the keys are its own");

    let AppletKind::NextEvent(settings) = &parsed.applets["next-event"].kind else {
        panic!("the table names the next-event applet");
    };
    assert_eq!(settings.within, 15);
    assert_eq!(settings.countdown, 5);
    assert_eq!(settings.horizon, 240);
    assert!(settings.all_day);
    assert_eq!(settings.upcoming, 3);

    let bare: Config = toml::from_str("[applets.next-event]\n").expect("every setting is optional");
    assert_eq!(
        bare.applets["next-event"].kind,
        AppletKind::NextEvent(applets::NextEvent::default())
    );

    assert!(
        toml::from_str::<Config>("[applets.next-event]\nwith-in = 30\n").is_err(),
        "a struct variant is what refuses a misspelled key; a unit variant would have \
             swallowed it silently"
    );
}

#[test]
fn a_place_table_reads_both_shapes_and_refuses_a_third() {
    let here: Config =
        toml::from_str("[applets.weather.place]\nat = \"here\"\n").expect("`here` is a place");
    assert_eq!(
        here.applets["weather"].kind,
        AppletKind::Weather(applets::Weather::default()),
        "following the fix is the default, so naming it changes nothing"
    );

    let fixed: Config = toml::from_str(
        "[applets.weather]\nlabel = \"Vilnius\"\ndays = 7\n\n             [applets.weather.place]\nat = \"latlon\"\nlatitude = 54.6872\n             longitude = 25.2797\n",
    )
    .expect("a fixed pair is a place");
    let AppletKind::Weather(weather) = &fixed.applets["weather"].kind else {
        panic!("the table names the weather applet");
    };
    assert_eq!(weather.label.as_deref(), Some("Vilnius"));
    assert_eq!((weather.hours, weather.days), (4, 7));
    assert_eq!(
        weather.place,
        applets::Place::Coordinates {
            latitude: 54.6872,
            longitude: 25.2797
        }
    );

    let named: Config = toml::from_str(
        "[applets.weather]\nplace = { at = \"location\", name = \"Vilnius, LT\" }\n",
    )
    .expect("a city and country code are a place");
    let AppletKind::Weather(weather) = &named.applets["weather"].kind else {
        panic!("the table names the weather applet");
    };
    assert_eq!(
        weather.place,
        applets::Place::Location {
            name: "Vilnius, LT".to_owned()
        }
    );
    let long_city = "v".repeat(101);
    for name in [
        "Vilnius".to_owned(),
        "Vilnius, lt".to_owned(),
        ", LT".to_owned(),
        "Vilnius, LTU".to_owned(),
        format!("{long_city}, LT"),
    ] {
        let document =
            format!("[applets.weather]\nplace = {{ at = \"location\", name = \"{name}\" }}\n");
        let error = toml::from_str::<Config>(&document)
            .expect_err("malformed locations are rejected")
            .to_string();
        assert!(error.contains("location must be written as"), "{error}");
    }

    assert!(
        toml::from_str::<Config>("[applets.weather.place]\nat = \"postcode\"\n").is_err(),
        "there are two shapes and a third is a mistake, not a place"
    );
    assert!(
        toml::from_str::<Config>("[applets.weather.place]\nat = \"here\"\nlatitude = 54.6\n")
            .is_err(),
        "`deny_unknown_fields` is what stops coordinates being written under the wrong shape"
    );
}

#[test]
fn a_named_place_is_canonicalized_when_loaded() {
    let document: Config = toml::from_str(
        "[applets.weather]\nplace = { at = \"location\", name = \" Washington, D.C. , US \" }\n",
    )
    .expect("a named place");
    let AppletKind::Weather(weather) = &document.applets["weather"].kind else {
        panic!("the table names the weather applet");
    };

    assert_eq!(
        weather.place,
        applets::Place::Location {
            name: "Washington, D.C., US".to_owned()
        }
    );
}

#[test]
fn coordinates_outside_their_ranges_are_refused_at_load() {
    let place = |latitude: &str, longitude: &str| {
        format!(
            "[applets.weather.place]\nat = \"latlon\"\nlatitude = {latitude}\n                 longitude = {longitude}\n"
        )
    };

    assert!(toml::from_str::<Config>(&place("54.6872", "25.2797")).is_ok());

    for (latitude, longitude, named) in [
        ("91.0", "25.2797", "latitude"),
        ("-90.5", "25.2797", "latitude"),
        ("54.6872", "180.5", "longitude"),
        ("54.6872", "-181.0", "longitude"),
    ] {
        let error = toml::from_str::<Config>(&place(latitude, longitude))
            .expect_err("that pair is not on Earth")
            .to_string();
        assert!(
            error.contains(named) && error.contains("[applets.weather]"),
            "the message names the key and the table it is in: {error}"
        );
    }
}

#[test]
fn a_common_setting_sits_beside_the_kinds_own() {
    let parsed: Config =
        toml::from_str("[applets.clock]\nlabel-format = \"%H\"\ntooltip-format = \"%A\"\n")
            .expect("common and kind settings share one flat table");

    assert_eq!(parsed.applets["clock"].kind.tooltip_format(), Some("%A"));
    let AppletKind::Clock(clock) = &parsed.applets["clock"].kind else {
        panic!("the table names the clock");
    };
    assert_eq!(clock.label_format, "%H");
}

#[test]
fn a_misspelled_common_setting_is_still_a_load_error() {
    assert!(
        toml::from_str::<Config>("[applets.clock]\ntooltipformat = \"%A\"\n").is_err(),
        "the splitter leaves what it does not recognise in the table, so the kind refuses it"
    );
}

#[test]
fn an_unknown_setting_names_the_common_ones_too() {
    let error = toml::from_str::<Config>("[applets.clock]\ntooltipformat = \"%A\"\n")
        .expect_err("a misspelled key is refused")
        .to_string();

    assert!(
        error.contains("`tooltip-format`"),
        "the kind cannot list a setting it does not own, so the message has to name the one \
             the user meant: {error}"
    );
}

#[test]
fn a_clock_reads_its_world_clock_zones() {
    let parsed: Config = toml::from_str(
        "[applets.clock]\n[[applets.clock.timezones]]\nlabel = \"Tokyo\"\ntimezone = \"Asia/Tokyo\"\n",
    )
    .expect("a full table");

    let AppletKind::Clock(clock) = &parsed.applets["clock"].kind else {
        panic!("the table names the clock");
    };
    assert_eq!(clock.timezones.len(), 1);
    assert_eq!(clock.timezones[0].label, "Tokyo");
    assert_eq!(clock.timezones[0].note, None);
}

#[test]
fn a_configured_applet_survives_being_written_back_out() {
    let parsed: Config =
        toml::from_str("[applets.clock]\nlabel-format = \"%H\"\ntooltip-format = \"%A\"\n")
            .expect("a table carrying both halves");

    let written = toml::to_string(&parsed).expect("the document serializes");
    let again: Config = toml::from_str(&written).expect("what was written parses again");

    assert_eq!(
        parsed.applets, again.applets,
        "`glimpsectl config show` writes this document out, and both halves are flattened \
             into one table on the way"
    );
}

#[test]
fn a_setting_a_nested_table_lacks_is_not_blamed_on_a_common_one() {
    let error = toml::from_str::<Config>(
        "[applets.clock]\n[[applets.clock.timezones]]\nlabel = \"T\"\ntimezone = \"Asia/Tokyo\"\nnonesuch = 1\n",
    )
    .expect_err("a key the timezone entry does not have")
    .to_string();
    assert!(
        !error.contains("tooltip-format"),
        "a common setting is not valid inside a timezone entry: {error}"
    );
}

#[test]
fn a_command_applet_carries_a_program_per_gesture() {
    let parsed = toml::from_str::<Config>(
        "[applets.shot]\nextends = \"command\"\nicon = \"camera-photo-symbolic\"\non-click = [\"grim\"]\non-scroll-up = [\"pamixer\", \"-i\", \"5\"]\n",
    )
    .expect("a command applet with programs parses");
    let AppletKind::Command(command) = &parsed.applets["shot"].kind else {
        panic!("`extends = \"command\"` names the kind");
    };
    assert_eq!(command.on_click, ["grim"]);
    assert_eq!(command.on_scroll_up, ["pamixer", "-i", "5"]);
}

#[test]
fn a_command_gesture_that_names_no_program_is_refused_by_key() {
    let error =
        toml::from_str::<Config>("[applets.shot]\nextends = \"command\"\non-scroll-up = [\" \"]\n")
            .expect_err("an empty program name was accepted")
            .to_string();
    assert!(
        error.contains("[applets.shot]") && error.contains("on-scroll-up"),
        "the error names the table and the key: {error}"
    );
}

#[test]
fn a_command_applet_inherits_the_panel_environment_by_default() {
    let parsed = toml::from_str::<Config>("[applets.volume-up]\nextends = \"command\"\n")
        .expect("a command applet parses");
    let AppletKind::Command(command) = &parsed.applets["volume-up"].kind else {
        panic!("a command applet");
    };
    assert!(command.inherit_env);
    assert!(CommandAppletConfig::default().inherit_env);
    let serialized = toml::to_string(&CommandAppletConfig::default()).expect("serializes");
    assert!(!serialized.contains("inherit-env"), "{serialized}");
    let off = toml::from_str::<CommandAppletConfig>("inherit-env = false\n").expect("parses");
    assert!(!off.inherit_env);
    assert!(
        toml::to_string(&off)
            .expect("serializes")
            .contains("inherit-env = false")
    );
}

#[test]
fn a_command_applet_refuses_an_invalid_env_name() {
    let error = toml::from_str::<Config>(
        "[applets.volume-up]\nextends = \"command\"\nenv = { \"BAD-NAME\" = \"x\" }\n",
    )
    .expect_err("an invalid environment variable name")
    .to_string();
    assert!(
        error.contains("env name is invalid: \"BAD-NAME\""),
        "{error}"
    );
}

#[test]
fn a_command_applet_env_round_trips() {
    let parsed = toml::from_str::<Config>(
        "[applets.volume-up]\nextends = \"command\"\nenv = { PULSE_SINK = \"@DEFAULT_SINK@\" }\non-click = [\"pactl\", \"info\"]\n",
    )
    .expect("a command applet with environment variables parses");
    let AppletKind::Command(command) = &parsed.applets["volume-up"].kind else {
        panic!("a command applet");
    };
    assert_eq!(command.env["PULSE_SINK"], "@DEFAULT_SINK@");
    let serialized = toml::to_string(&parsed).expect("the configuration serializes");
    let round_trip: Config = toml::from_str(&serialized).expect("the configuration parses again");
    assert_eq!(round_trip.applets["volume-up"], parsed.applets["volume-up"]);
}

#[test]
fn a_command_icon_path_must_be_absolute() {
    let relative = "[applets.shot]\nextends = \"command\"\nicon = \"icons/shot.png\"\n";
    let absolute = "[applets.shot]\nextends = \"command\"\nicon = \"/usr/share/shot.png\"\n";
    assert!(toml::from_str::<Config>(relative).is_err());
    assert!(toml::from_str::<Config>(absolute).is_ok());
}

#[test]
fn a_command_applet_refuses_a_key_it_does_not_have() {
    assert!(
        toml::from_str::<Config>("[applets.shot]\nextends = \"command\"\non-clik = [\"x\"]\n")
            .is_err(),
        "a unit variant would swallow the misspelling"
    );
}

#[test]
fn the_schema_offers_every_applet_by_its_table_name() {
    let schema: serde_json::Value = serde_json::from_str(&crate::json_schema_document(
        crate::Documents::ConfigAndPanel,
    ))
    .expect("the schema is JSON");
    let applets = &schema["properties"]["applets"];
    let by_name = applets["properties"]
        .as_object()
        .expect("one entry per applet, so an editor resolves [applets.clock] on its own");

    assert_eq!(
        by_name["clock"]["properties"]["label-format"]["default"],
        serde_json::json!("%a, %-d %b, %H:%M"),
        "a by-name entry carries the applet's own settings"
    );
    assert_eq!(
        by_name["clock"]["required"],
        serde_json::json!([]),
        "`extends` is not required when the table name already names the applet"
    );
    assert_eq!(
        applets["additionalProperties"]["oneOf"]
            .as_array()
            .map(Vec::len),
        Some(by_name.len()),
        "an aliased table falls through to the tagged form"
    );
}

#[test]
fn extends_names_the_applet_so_one_kind_can_have_several_instances() {
    let parsed: Config = toml::from_str("[applets.clock-utc]\nextends = \"clock\"\n")
        .expect("extends wins over the key");

    assert_eq!(
        parsed.applets["clock-utc"],
        Applet::from(AppletKind::Clock(applets::Clock::default()))
    );
}

#[test]
fn the_pager_defaults_to_dots_of_windows_on_this_output() {
    let parsed: Config = toml::from_str("[applets.pager]\n").expect("settings are optional");

    assert_eq!(
        parsed.applets["pager"],
        Applet::from(AppletKind::Pager(applets::Pager {
            mode: PagerMode::Windows,
            shape: PagerShape::Dots,
            scope: PagerScope::Output,
            label: "{index}".to_owned(),
            focused_label: None,
            unfocused_label: None,
            urgent_label: None,
        }))
    );
}

#[test]
fn the_pager_reads_its_settings_in_kebab_case() {
    let parsed: Config = toml::from_str(
        "[applets.pager]\nmode = \"workspaces\"\nshape = \"labels\"\nscope = \"session\"\nfocused-label = \"{name}\"\n",
    )
    .expect("a full table");

    let AppletKind::Pager(pager) = &parsed.applets["pager"].kind else {
        panic!("the table names the pager");
    };
    assert_eq!(pager.mode, PagerMode::Workspaces);
    assert_eq!(pager.shape, PagerShape::Labels);
    assert_eq!(pager.scope, PagerScope::Session);
    assert_eq!(pager.focused_label.as_deref(), Some("{name}"));
}

#[test]
fn the_pager_refuses_a_setting_it_does_not_have() {
    assert!(
        toml::from_str::<Config>("[applets.pager]\nappearance = \"dots\"\n").is_err(),
        "a silently ignored key is a setting that never took effect"
    );
}

#[test]
fn an_unresolvable_table_name_is_refused_and_lists_the_applets() {
    let error = toml::from_str::<Config>("[applets.nonesuch]\n")
        .expect_err("an applet nobody implements is a bad document, not a silent skip")
        .to_string();

    assert!(error.contains("nonesuch"), "{error}");
    assert!(
        error.contains("clock"),
        "the message lists what is valid: {error}"
    );
}

#[test]
fn a_setting_no_applet_declares_is_refused() {
    let error = toml::from_str::<Config>("[applets.clock]\nfrmat = \"%H\"\n")
        .expect_err("a typo is loud rather than ignored")
        .to_string();

    assert!(error.contains("frmat"), "{error}");
    assert!(
        error.contains("applets.clock"),
        "it names the table: {error}"
    );
}

#[test]
fn disabled_idle_listeners_are_not_validated() {
    let config: crate::Config = toml::from_str(
        "[idle.profiles.ac]\nlisteners = [\n  { timeout = 0, on-idle = \"\" },\n  { timeout = 0, on-idle = \"lock\" },\n  { timeout = 600, on-idle = \"lock\" },\n]\n",
    )
    .expect("the document parses");
    assert!(config.validate().is_empty(), "{:?}", config.validate());
}

#[test]
fn duplicate_enabled_idle_timeouts_are_still_refused() {
    let config: crate::Config = toml::from_str(
        "[idle.profiles.battery]\nlisteners = [\n  { timeout = 600, on-idle = \"lock\" },\n  { timeout = 600, on-idle = \"suspend\" },\n]\n",
    )
    .expect("the document parses");
    let problems: Vec<String> = config.validate().iter().map(ToString::to_string).collect();
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert!(
        problems[0].contains("duplicate timeout 600"),
        "{problems:?}"
    );
}
