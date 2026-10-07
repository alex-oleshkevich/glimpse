use super::*;
#[test]
fn placement_carries_panel_zone_output_and_orientation() {
    assert_eq!(
        placement(Position::Left, 32, Some("DP-2".into()), panel::Zone::End),
        Placement {
            output: Some("DP-2".into()),
            position: Edge::Left,
            orientation: Orientation::Vertical,
            zone: Zone::Right,
            size: 32
        }
    );
}

#[test]
#[ignore = "needs a display"]
fn the_footer_row_follows_the_footer_while_it_is_open() {
    gtk4::init().expect("GTK display");
    glimpse_widgets::register_resources().expect("resources");
    let footer = Footer::default();
    let row = glimpse_widgets::ActionRow::new();
    dress_footer(&row, &footer);
    assert!(!row.is_visible(), "an unconfigured footer shows no row");
    footer.set(Some(("Settings".to_owned(), vec!["tool".to_owned()])));
    dress_footer(&row, &footer);
    assert!(row.is_visible());
    assert_eq!(
        row.property::<Option<String>>("title").as_deref(),
        Some("Settings")
    );
    footer.set(None);
    dress_footer(&row, &footer);
    assert!(!row.is_visible(), "a removed footer hides the row");
}
