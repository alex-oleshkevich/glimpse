use super::*;

pub(super) fn check() {
    let subclassed = ForecastDay::new();
    let inherited: &Row = subclassed.upcast_ref();
    inherited.set_lead_icon(Some("weather-clear-symbolic"));
    assert_eq!(
        child_named::<gtk4::Image>(&subclassed, "row__icon")
            .icon_name()
            .as_deref(),
        Some("weather-clear-symbolic"),
        "a Row subclass fills the lead through `lead-icon`; `icon-name` on a Gtk.Button is \
             the parent's own property and would replace the row's child instead"
    );
    assert!(
        child_named::<crate::RangeBar>(&subclassed, "range-bar")
            .parent()
            .is_some(),
        "and its template children land in the trail the parent declared"
    );

    let strip = ForecastStrip::new();
    strip.set_hours(&[
        Hour {
            label: "Now".to_owned(),
            icon_name: "weather-showers-symbolic".to_owned(),
            temperature: 18.4,
            now: true,
        },
        Hour {
            label: "16:00".to_owned(),
            icon_name: "weather-clear-symbolic".to_owned(),
            temperature: 16.6,
            now: false,
        },
    ]);
    let times = all_named(&strip, "forecast__time");
    assert_eq!(times.len(), 2);
    assert!(
        times[0].has_css_class("forecast__now") && !times[1].has_css_class("forecast__now"),
        "exactly one column is now"
    );
    let temperatures = all_named(&strip, "forecast__temperature");
    assert_eq!(
        temperatures[0]
            .clone()
            .downcast::<gtk4::Label>()
            .expect("label")
            .text(),
        "18°",
        "the strip owns the rounding and the unit, so two columns cannot disagree"
    );
    let reading = |index: usize| {
        temperatures[index]
            .clone()
            .downcast::<gtk4::Label>()
            .expect("label")
            .text()
            .to_string()
    };
    strip.set_unit("°F");
    assert_eq!(
        (reading(0), reading(1)),
        ("18°F".to_owned(), "17°F".to_owned()),
        "the unit is set from the system the payload declares, and reaches every column"
    );

    let forecast = ForecastList::new();
    let day = |label: &str, precipitation, low: f64, high: f64| Day {
        label: label.to_owned(),
        icon_name: "weather-clear-symbolic".to_owned(),
        precipitation,
        low,
        high,
        now: None,
    };
    forecast.set_days(&[
        Day {
            now: Some(16.0),
            ..day("Today", Some(60), 12.0, 18.0)
        },
        day("Tomorrow", Some(0), 11.0, 20.0),
        day("Sunday", None, 7.0, 26.0),
    ]);
    assert_eq!(
        forecast.scale(),
        (7.0, 26.0),
        "the list owns the scale, so every bar is measured against the same span"
    );
    let bars: Vec<gtk4::Widget> = all_named(&forecast, "range-bar");
    assert_eq!(bars.len(), 3);
    for bar in &bars {
        assert_eq!(
            bar.clone().downcast::<RangeBar>().expect("bar").scale(),
            (7.0, 26.0)
        );
    }
    let now_of = |bar: &gtk4::Widget| bar.clone().downcast::<RangeBar>().expect("bar").now();
    assert_eq!(now_of(&bars[0]), Some(16.0));
    assert_eq!(
        now_of(&bars[1]),
        None,
        "only the day given a reading marks one"
    );
    let chances = all_named(&forecast, "forecast__precipitation");
    let chance = |index: usize| {
        chances[index]
            .clone()
            .downcast::<gtk4::Label>()
            .expect("label")
            .get_visible()
    };
    assert!(chance(0));
    assert!(
        !chance(1) && !chance(2),
        "a zero chance of rain says nothing, and neither does an unknown one"
    );

    assert!(
        children_of::<Row>(&forecast).is_empty(),
        "a day is parented with the panel that unfolds under it, not directly"
    );
    let second = forecast.imp().holders.borrow()[1].clone();
    let second_day = second.head::<ForecastDay>().expect("a day row");
    second_day.emit_clicked();
    assert!(!second.expanded(), "a day with no detail does not open");
    forecast.set_details(&[None, Some(gtk4::Label::new(None).upcast())]);
    second_day.emit_clicked();
    assert!(
        second.expanded(),
        "a day with a detail opens under its own row"
    );

    let ends = |class: &str| {
        all_named(&forecast, class)
            .first()
            .cloned()
            .and_downcast::<gtk4::Label>()
            .expect("label")
            .text()
            .to_string()
    };
    assert_eq!(
        (ends("forecast__low"), ends("forecast__high")),
        ("12°".to_owned(), "18°".to_owned())
    );
    forecast.set_unit("°C");
    assert_eq!(
        (ends("forecast__low"), ends("forecast__high")),
        ("12°C".to_owned(), "18°C".to_owned()),
        "the list takes the same unit the strip does, on both ends of the range"
    );
}
