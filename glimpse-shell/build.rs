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

    glib_build_tools::compile_resources(
        &["resources"],                          // source dirs (relative to build.rs)
        "resources/glimpse-shell.gresource.xml", // manifest
        "glimpse-shell.gresource",               // output name (placed in OUT_DIR)
    );
}
