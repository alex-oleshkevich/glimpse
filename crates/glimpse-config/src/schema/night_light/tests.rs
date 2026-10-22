use super::*;

#[test]
fn every_spelling_is_one_the_document_reads_back_as_the_same_mode() {
    for mode in [Schedule::Off, Schedule::Automatic, Schedule::Schedule] {
        let parsed: NightLight = toml::from_str(&format!("schedule = \"{}\"\n", mode.as_str()))
            .expect("the spelling is one serde accepts");
        assert_eq!(parsed.schedule, mode);
        assert_eq!(Schedule::parse(mode.as_str()), Some(mode));
    }
}

#[test]
fn a_time_the_runtime_cannot_read_is_refused_at_load() {
    for spelling in ["10pm", "22:00:00", "24:00", "noon", ""] {
        let error = toml::from_str::<NightLight>(&format!("start-time = \"{spelling}\"\n"))
            .expect_err("a time the schedule could never use");
        assert!(error.to_string().contains("HH:MM"), "{spelling}: {error}");
    }
}

#[test]
fn both_keys_are_checked_not_only_the_first() {
    let error = toml::from_str::<NightLight>("start-time = \"20:00\"\nend-time = \"7pm\"\n")
        .expect_err("end-time is checked too");
    assert!(error.to_string().contains("HH:MM"), "{error}");
}

#[test]
fn the_emitted_schema_constrains_both_times_rather_than_leaving_it_to_the_loader() {
    let document: serde_json::Value =
        serde_json::from_str(&crate::json_schema_document(crate::Documents::Config))
            .expect("the emitted schema is JSON");
    let night_light = &document["$defs"]["NightLight"]["properties"];

    for key in ["start-time", "end-time"] {
        assert_eq!(
            night_light[key]["pattern"], CLOCK_PATTERN,
            "{key} is an unconstrained string in the schema, so an editor accepts a value the \
                 loader then refuses"
        );
    }
}

#[test]
fn a_time_written_as_hh_mm_loads_unchanged() {
    let parsed: NightLight =
        toml::from_str("start-time = \"20:00\"\nend-time = \"07:00\"\n").expect("both are HH:MM");

    assert_eq!(parsed.start_time.as_deref(), Some("20:00"));
    assert_eq!(parsed.end_time.as_deref(), Some("07:00"));
}

#[test]
fn absent_times_are_absent_rather_than_refused() {
    let parsed: NightLight = toml::from_str("schedule = \"schedule\"\n").expect("no times");

    assert_eq!(parsed.start_time, None);
    assert_eq!(parsed.end_time, None);
}

#[test]
fn temperature_bounds_match_the_emitted_schema() {
    for value in [MIN_TEMPERATURE, MAX_TEMPERATURE] {
        let parsed: NightLight =
            toml::from_str(&format!("temperature = {value}")).expect("boundary loads");
        assert_eq!(parsed.temperature, value);
    }
    for (value, clamped) in [
        (0, MIN_TEMPERATURE),
        (MIN_TEMPERATURE - 1, MIN_TEMPERATURE),
        (MAX_TEMPERATURE + 1, MAX_TEMPERATURE),
        (u32::MAX, MAX_TEMPERATURE),
    ] {
        let parsed: NightLight =
            toml::from_str(&format!("temperature = {value}\nschedule = \"off\""))
                .expect("an out-of-range temperature still loads");
        assert_eq!(parsed.temperature, clamped);
        assert_eq!(parsed.schedule, Schedule::Off);
    }
    let document: serde_json::Value =
        serde_json::from_str(&crate::json_schema_document(crate::Documents::Config)).expect("JSON");
    let temperature = &document["$defs"]["NightLight"]["properties"]["temperature"];
    assert_eq!(temperature["minimum"], MIN_TEMPERATURE);
    assert_eq!(temperature["maximum"], MAX_TEMPERATURE);
}

#[test]
fn a_temperature_the_range_cannot_hold_loads_clamped_and_is_reported_once() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let path = directory.path().join("config.toml");
    for (written, used) in [
        ("9000", MAX_TEMPERATURE),
        ("999", MIN_TEMPERATURE),
        ("-1", MIN_TEMPERATURE),
        ("4500.0", 4500),
        ("nan", MIN_TEMPERATURE),
    ] {
        std::fs::write(
            &path,
            format!(
                "[night-light]\ntemperature = {written}\nschedule = \"off\"\ntransition-minutes = 3\n"
            ),
        )
        .expect("writes");
        let config = crate::load_files(std::slice::from_ref(&path))
            .expect("the rest of the document still loads");

        assert_eq!(config.night_light.temperature, used, "{written}");
        assert_eq!(config.night_light.schedule, Schedule::Off, "{written}");
        assert_eq!(config.night_light.transition_minutes, 3, "{written}");
        let problems: Vec<String> = config.validate().iter().map(ToString::to_string).collect();
        assert_eq!(problems.len(), 1, "{written}: {problems:?}");
        assert!(
            problems[0]
                .to_lowercase()
                .contains(&format!("night-light.temperature: {written}"))
                && problems[0].contains(&format!("using {used}")),
            "{problems:?}"
        );
    }
}

#[test]
fn a_whole_temperature_in_range_is_not_reported_and_a_string_is_refused() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let path = directory.path().join("config.toml");
    std::fs::write(&path, "[night-light]\ntemperature = 4500\n").expect("writes");
    let config = crate::load_files(std::slice::from_ref(&path)).expect("loads");
    assert_eq!(config.night_light.temperature, 4500);
    assert!(config.validate().is_empty());

    std::fs::write(&path, "[night-light]\ntemperature = \"4500\"\n").expect("writes");
    let error =
        crate::load_files(std::slice::from_ref(&path)).expect_err("a string is a type error");
    assert!(error.to_string().contains("kelvin"), "{error}");
}
