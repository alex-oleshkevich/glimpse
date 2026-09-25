pub fn shot_shortcuts() -> adw::ShortcutsDialog {
    let builder = gtk4::Builder::from_resource("/me/aresa/GlimpseShell/widgets/shot_shortcuts.ui");
    match builder.object("shortcuts") {
        Some(dialog) => dialog,
        None => {
            tracing::error!("shot shortcuts resource has no dialog");
            adw::ShortcutsDialog::new()
        }
    }
}
