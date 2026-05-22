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
    // gtk4-layer-shell MUST appear before libwayland-client in the link line,
    // otherwise the layer-shell hook into libwayland's display init runs too
    // late and you get the runtime warning "Failed to initialize layer surface,
    // GTK4 Layer Shell may have been linked after libwayland." Emitting the
    // link directive from build.rs places it ahead of cargo's `-lwayland-client`
    // (which is dragged in by our direct wayland-client + wayland-protocols deps
    // for the idle inhibit binding).
    //
    // See: https://github.com/wmww/gtk4-layer-shell/blob/main/linking.md
    println!("cargo:rustc-link-lib=gtk4-layer-shell");

    compile_blueprints(&[
        ("src/widgets/tile/template.blp", "resources/widgets/tile.ui"),
        (
            "src/widgets/switch_tile/template.blp",
            "resources/widgets/switch_tile.ui",
        ),
        (
            "src/widgets/expander_tile/template.blp",
            "resources/widgets/expander_tile.ui",
        ),
        (
            "src/widgets/choice_tile/template.blp",
            "resources/widgets/choice_tile.ui",
        ),
        (
            "src/widgets/battery_hero/template.blp",
            "resources/widgets/battery_hero.ui",
        ),
        (
            "src/widgets/calendar/controls/template.blp",
            "resources/widgets/calendar_controls.ui",
        ),
        (
            "src/widgets/calendar/month_view/template.blp",
            "resources/widgets/calendar_month_view.ui",
        ),
        (
            "src/widgets/calendar/year_view/template.blp",
            "resources/widgets/calendar_year_view.ui",
        ),
        (
            "src/widgets/date_hero/template.blp",
            "resources/widgets/date_hero.ui",
        ),
        ("src/widgets/hero/template.blp", "resources/widgets/hero.ui"),
        (
            "src/widgets/message/template.blp",
            "resources/widgets/message.ui",
        ),
        (
            "src/widgets/message_group/template.blp",
            "resources/widgets/message_group.ui",
        ),
        (
            "src/widgets/mic_indicator/template.blp",
            "resources/widgets/mic_indicator.ui",
        ),
        (
            "src/widgets/muted_indicator/template.blp",
            "resources/widgets/muted_indicator.ui",
        ),
        (
            "src/widgets/camera_indicator/template.blp",
            "resources/widgets/camera_indicator.ui",
        ),
        (
            "src/widgets/location_indicator/template.blp",
            "resources/widgets/location_indicator.ui",
        ),
        (
            "src/widgets/screencast_indicator/template.blp",
            "resources/widgets/screencast_indicator.ui",
        ),
        (
            "src/widgets/popover_shell/template.blp",
            "resources/widgets/popover_shell.ui",
        ),
        (
            "src/widgets/segmented_tile/template.blp",
            "resources/widgets/segmented_tile.ui",
        ),
        (
            "src/widgets/slider_tile/template.blp",
            "resources/widgets/slider_tile.ui",
        ),
        (
            "src/widgets/world_clock/template.blp",
            "resources/widgets/world_clock.ui",
        ),
        (
            "src/widgets/world_clock/row/template.blp",
            "resources/widgets/world_clock_row.ui",
        ),
    ]);

    glib_build_tools::compile_resources(
        &["resources"],                          // source dirs (relative to build.rs)
        "resources/glimpse-shell.gresource.xml", // manifest
        "glimpse-shell.gresource",               // output name (placed in OUT_DIR)
    );
}
