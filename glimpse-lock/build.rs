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
    println!("cargo:rerun-if-changed=resources/lock.css");
    println!("cargo:rerun-if-changed=resources/glimpse-lock.gresource.xml");

    compile_blueprints(&[(
        "src/widgets/avatar/template.blp",
        "resources/widgets/avatar.ui",
    )]);

    glib_build_tools::compile_resources(
        &["resources"],
        "resources/glimpse-lock.gresource.xml",
        "glimpse-lock.gresource",
    );
}
