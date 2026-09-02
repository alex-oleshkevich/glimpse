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
        ("blueprints/calendar.blp", "resources/widgets/calendar.ui"),
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
        ("blueprints/hero.blp", "resources/widgets/hero.ui"),
        ("blueprints/indicator.blp", "resources/widgets/indicator.ui"),
        ("blueprints/notice.blp", "resources/widgets/notice.ui"),
        (
            "blueprints/now_playing.blp",
            "resources/widgets/now_playing.ui",
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
        ("blueprints/transport.blp", "resources/widgets/transport.ui"),
    ]);

    glib_build_tools::compile_resources(
        &["resources"],                          // source dirs (relative to build.rs)
        "resources/glimpse-panel.gresource.xml", // manifest
        "glimpse-panel.gresource",               // output name (placed in OUT_DIR)
    );
}
