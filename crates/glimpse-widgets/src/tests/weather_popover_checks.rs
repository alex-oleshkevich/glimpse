use super::*;

pub(super) fn check() {
    let weather = WeatherPopover::new();

    let day_panel = |index: usize| -> Option<gtk4::Revealer> {
        weather
            .imp()
            .days
            .imp()
            .holders
            .borrow()
            .get(index)
            .map(|(row, _)| row.details())
            .and_then(|details| details.ancestor(gtk4::Revealer::static_type()))
            .and_downcast::<gtk4::Revealer>()
    };
    let alert_rows = |weather: &WeatherPopover| -> Vec<ExpanderRow> {
        children_of::<ExpanderRow>(&weather.imp().alerts.get())
    };
    let opened = |weather: &WeatherPopover| -> Option<String> {
        let imp = weather.imp();
        if let Some(day) = imp
            .days
            .imp()
            .holders
            .borrow()
            .iter()
            .position(|(row, _)| row.open())
        {
            return Some(day_page(day as u32));
        }
        imp.notices
            .borrow()
            .iter()
            .position(ExpanderRow::open)
            .map(alert_page)
    };
    let press = |weather: &WeatherPopover, key: &str| {
        let imp = weather.imp();
        if let Some(day) = key
            .strip_prefix("day")
            .and_then(|at| at.parse::<usize>().ok())
        {
            imp.days.imp().holders.borrow()[day]
                .0
                .imp()
                .head
                .emit_by_name::<()>("activated", &[]);
        }
        if let Some(alert) = key
            .strip_prefix("alert")
            .and_then(|at| at.parse::<usize>().ok())
        {
            imp.notices.borrow()[alert]
                .imp()
                .head
                .emit_by_name::<()>("activated", &[]);
        }
    };
    let dimmed = |widget: &gtk4::Widget| {
        std::iter::successors(Some(widget.clone()), |widget| widget.parent())
            .any(|widget| widget.has_css_class("receded"))
    };

    let page = |key: &str, title: &str| WeatherPage {
        key: key.to_owned(),
        title: title.to_owned(),
        description: None,
        facts: vec![Fact::new("High", "18 °C")],
    };

    weather.set_heading("weather-showers-symbolic", "Vilnius", Some("Light rain"));
    weather.set_reading(Some(("18", "°")));
    assert!(child_named::<Readout>(&weather, "readout").get_visible());
    weather.set_reading(None);
    assert!(
        !child_named::<Readout>(&weather, "readout").get_visible(),
        "no reading reserves no space in the header"
    );

    weather.set_hours(&[]);
    weather.set_days(&[]);
    assert!(
        !weather.imp().hourly.get_visible() && !weather.imp().hourly_rule.get_visible(),
        "an empty strip takes its hairline with it"
    );
    weather.set_days(&[Day {
        label: "Today".to_owned(),
        icon_name: "weather-clear-symbolic".to_owned(),
        precipitation: None,
        low: 11.0,
        high: 18.0,
        now: None,
    }]);
    assert!(weather.imp().daily.get_visible() && weather.imp().daily_rule.get_visible());

    assert_eq!(opened(&weather), None);

    weather.set_days(&[
        Day {
            label: "Tomorrow".to_owned(),
            icon_name: "weather-clear-symbolic".to_owned(),
            precipitation: None,
            low: 11.0,
            high: 18.0,
            now: None,
        },
        Day {
            label: "Wednesday".to_owned(),
            icon_name: "weather-clear-symbolic".to_owned(),
            precipitation: None,
            low: 12.0,
            high: 19.0,
            now: None,
        },
    ]);
    weather.set_pages(&[page("day0", "Tomorrow"), page("day1", "Wednesday")]);
    press(&weather, "day0");
    assert_eq!(opened(&weather).as_deref(), Some("day0"));
    assert!(
        day_panel(0).is_some_and(|panel| panel.is_ancestor(&weather.imp().days.get())),
        "a day's detail unfolds inside the list it belongs to, not beside the column"
    );
    assert!(
        children_of::<ForecastDay>(&weather.imp().days.get()).is_empty(),
        "every day travels with its own panel, so the list's children are holders"
    );
    assert!(
        dimmed(weather.imp().shell.imp().header.upcast_ref())
            && dimmed(weather.imp().hours.upcast_ref()),
        "an open detail is read against a quiet card, so everything else recedes"
    );
    press(&weather, "day1");
    assert_eq!(
        opened(&weather).as_deref(),
        Some("day1"),
        "another day switches rather than closing"
    );
    assert!(
        day_panel(0).is_some_and(|panel| !panel.reveals_child()),
        "opening one day closes the one that was open"
    );
    press(&weather, "day1");
    assert_eq!(
        opened(&weather),
        None,
        "the control that opened the detail is the one that closes it"
    );

    press(&weather, "day1");
    weather.set_pages(&[page("day0", "Tomorrow")]);
    assert_eq!(
        opened(&weather),
        None,
        "a detail must not stand open on a page that has gone away"
    );
    assert!(
        !dimmed(weather.imp().shell.imp().header.upcast_ref()),
        "and the card it was read against comes back up with it"
    );

    let advisory = |title: &str, page: Option<&str>, severity| Advisory {
        severity,
        icon_name: "dialog-warning-symbolic".to_owned(),
        title: title.to_owned(),
        subtitle: Some("LHMT".to_owned()),
        page: page.map(str::to_owned),
    };

    assert!(alert_rows(&weather).is_empty());
    assert!(
        !weather.imp().alerts.get_visible(),
        "an empty alert box would still cost the space between it and the nowcast"
    );
    weather.set_pages(&[page("day0", "Tomorrow"), page("alert0", "Storm")]);
    weather.set_alerts(&[
        advisory("Thunderstorm warning", Some("alert0"), Severity::Error),
        advisory("Wind advisory", None, Severity::Warning),
    ]);

    let raised = alert_rows(&weather);
    assert_eq!(raised.len(), 2);
    assert_eq!(
        (raised[0].tone(), raised[1].tone()),
        (RowTone::Error, RowTone::Warning)
    );
    assert!(
        raised[0].tint() && raised[0].title_lines() == 2 && raised[0].subtitle().is_none(),
        "an alert is a tinted headline; its source is a fact in the drawer"
    );

    press(&weather, "alert1");
    assert_eq!(
        opened(&weather),
        None,
        "an alert with nothing to read has no drawer"
    );
    press(&weather, "alert0");
    assert_eq!(opened(&weather).as_deref(), Some("alert0"));

    press(&weather, "alert0");
    assert_eq!(
        opened(&weather),
        None,
        "one handler, not one per reconcile: a second click closes rather than reopening"
    );

    let ordered = WeatherPopover::new();
    ordered.set_alerts(&[advisory(
        "Thunderstorm warning",
        Some("alert0"),
        Severity::Error,
    )]);
    ordered.set_pages(&[page("alert0", "Storm")]);
    press(&ordered, "alert0");
    assert_eq!(
        opened(&ordered).as_deref(),
        Some("alert0"),
        "a notice and its page arrive through different setters, and neither order may lose it"
    );

    weather.set_alerts(&[advisory("Wind advisory", None, Severity::Warning)]);
    assert_eq!(
        alert_rows(&weather).len(),
        1,
        "a cleared alert is unparented rather than left behind empty"
    );
    weather.set_alerts(&[]);
    assert!(alert_rows(&weather).is_empty());
    assert!(!weather.imp().alerts.get_visible());

    weather.set_nowcast(None);
    assert!(!weather.imp().nowcast.get_visible());
    weather.set_nowcast(Some(&advisory(
        "Rain starting in 25 minutes",
        None,
        Severity::Info,
    )));
    assert!(weather.imp().nowcast.get_visible());

}
