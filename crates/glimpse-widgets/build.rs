fn compile_blueprints(pairs: &[(&str, &str)]) {
    for (src, out) in pairs {
        println!("cargo:rerun-if-changed={src}");
        if let Some(parent) = std::path::Path::new(out).parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        let status = std::process::Command::new("blueprint-compiler")
            .args(["compile", "--output", out, src])
            .status()
            .expect("blueprint-compiler not found");
        assert!(status.success(), "blueprint-compiler failed for {src}");
    }
}

fn main() {
    compile_blueprints(&[
        (
            "blueprints/audio_popover.blp",
            "resources/widgets/audio_popover.ui",
        ),
        (
            "blueprints/bluetooth_pairing_dialog.blp",
            "resources/widgets/bluetooth_pairing_dialog.ui",
        ),
        (
            "blueprints/bluetooth_popover.blp",
            "resources/widgets/bluetooth_popover.ui",
        ),
        (
            "blueprints/brightness_popover.blp",
            "resources/widgets/brightness_popover.ui",
        ),
        (
            "blueprints/display_popover.blp",
            "resources/widgets/display_popover.ui",
        ),
        (
            "blueprints/network_popover.blp",
            "resources/widgets/network_popover.ui",
        ),
        (
            "blueprints/network_secret_dialog.blp",
            "resources/widgets/network_secret_dialog.ui",
        ),
        ("blueprints/calendar.blp", "resources/widgets/calendar.ui"),
        (
            "blueprints/calendar_popover.blp",
            "resources/widgets/calendar_popover.ui",
        ),
        ("blueprints/clock_row.blp", "resources/widgets/clock_row.ui"),
        ("blueprints/event_row.blp", "resources/widgets/event_row.ui"),
        (
            "blueprints/forecast_day.blp",
            "resources/widgets/forecast_day.ui",
        ),
        (
            "blueprints/forecast_hour.blp",
            "resources/widgets/forecast_hour.ui",
        ),
        ("blueprints/fader.blp", "resources/widgets/fader.ui"),
        ("blueprints/hero.blp", "resources/widgets/hero.ui"),
        (
            "blueprints/tooltip_card.blp",
            "resources/widgets/tooltip_card.ui",
        ),
        (
            "blueprints/workspaces_popover.blp",
            "resources/widgets/workspaces_popover.ui",
        ),
        (
            "blueprints/workspace_section.blp",
            "resources/widgets/workspace_section.ui",
        ),
        ("blueprints/indicator.blp", "resources/widgets/indicator.ui"),
        (
            "blueprints/keyboard_popover.blp",
            "resources/widgets/keyboard_popover.ui",
        ),
        (
            "blueprints/next_event_popover.blp",
            "resources/widgets/next_event_popover.ui",
        ),
        (
            "blueprints/mpris_popover.blp",
            "resources/widgets/mpris_popover.ui",
        ),
        (
            "blueprints/notification_card.blp",
            "resources/widgets/notification_card.ui",
        ),
        (
            "blueprints/notification_header.blp",
            "resources/widgets/notification_header.ui",
        ),
        (
            "blueprints/notification_image_body.blp",
            "resources/widgets/notification_image_body.ui",
        ),
        (
            "blueprints/notification_text_body.blp",
            "resources/widgets/notification_text_body.ui",
        ),
        (
            "blueprints/notifications_popover.blp",
            "resources/widgets/notifications_popover.ui",
        ),
        ("blueprints/notice.blp", "resources/widgets/notice.ui"),
        (
            "blueprints/now_playing.blp",
            "resources/widgets/now_playing.ui",
        ),
        (
            "blueprints/pager_item.blp",
            "resources/widgets/pager_item.ui",
        ),
        ("blueprints/panel.blp", "resources/widgets/panel.ui"),
        (
            "blueprints/placeholder.blp",
            "resources/widgets/placeholder.ui",
        ),
        (
            "blueprints/player_row.blp",
            "resources/widgets/player_row.ui",
        ),
        (
            "blueprints/popover_shell.blp",
            "resources/widgets/popover_shell.ui",
        ),
        ("blueprints/readout.blp", "resources/widgets/readout.ui"),
        ("blueprints/row.blp", "resources/widgets/row.ui"),
        ("blueprints/scrubber.blp", "resources/widgets/scrubber.ui"),
        ("blueprints/section.blp", "resources/widgets/section.ui"),
        ("blueprints/split_row.blp", "resources/widgets/split_row.ui"),
        (
            "blueprints/switch_row.blp",
            "resources/widgets/switch_row.ui",
        ),
        ("blueprints/transport.blp", "resources/widgets/transport.ui"),
        (
            "blueprints/weather_popover.blp",
            "resources/widgets/weather_popover.ui",
        ),
    ]);

    glib_build_tools::compile_resources(
        &["resources"],                            // source dirs (relative to build.rs)
        "resources/glimpse-widgets.gresource.xml", // manifest
        "glimpse-widgets.gresource",               // output name (placed in OUT_DIR)
    );
}
