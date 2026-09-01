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
        ("blueprints/hero.blp", "resources/widgets/hero.ui"),
        ("blueprints/indicator.blp", "resources/widgets/indicator.ui"),
        ("blueprints/panel.blp", "resources/widgets/panel.ui"),
        (
            "blueprints/placeholder.blp",
            "resources/widgets/placeholder.ui",
        ),
        (
            "blueprints/popover_shell.blp",
            "resources/widgets/popover_shell.ui",
        ),
        ("blueprints/row.blp", "resources/widgets/row.ui"),
    ]);

    glib_build_tools::compile_resources(
        &["resources"],                          // source dirs (relative to build.rs)
        "resources/glimpse-panel.gresource.xml", // manifest
        "glimpse-panel.gresource",               // output name (placed in OUT_DIR)
    );
}
