use super::*;

macro_rules! pinned {
    ($($popover:ident),+ $(,)?) => {$({
        let popover = $popover::new();
        let imp = popover.imp();
        assert_eq!(
            imp.footer.parent().as_ref(),
            Some(imp.shell.imp().footer_box.upcast_ref::<gtk4::Widget>()),
            "{} keeps its footer row in the shell's pinned footer, not the scrolling body",
            stringify!($popover),
        );
    })+};
}

pub fn check() {
    macro_rules! body {
        ($($popover:ident),+ $(,)?) => {$({
            let popover = $popover::new();
            let imp = popover.imp();
            assert_eq!(
                imp.shell.body().last_child().as_ref(),
                Some(imp.footer.upcast_ref::<gtk4::Widget>()),
                "{} keeps its footer row last in the body",
                stringify!($popover),
            );
        })+};
    }

    body!(
        AudioPopover,
        KdeconnectPopover,
        NextEventPopover,
        ColorPickerPopover,
        ClipboardPopover,
        WeatherPopover,
        CalendarPopover,
    );
    pinned!(
        BluetoothPopover,
        DisplayPopover,
        IdlePopover,
        KeyboardPopover,
        NetworkPopover,
        NotificationsPopover,
        PlacesPopover,
        PowerPopover,
        PrintingPopover,
        RulerPopover,
        SessionPopover,
        SystemMonitorPopover,
        WorkspaceNamePopover,
    );
}
