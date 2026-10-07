use super::*;

macro_rules! last_in_body {
    ($($popover:ident),+ $(,)?) => {$({
        let popover = $popover::new();
        let imp = popover.imp();
        assert_eq!(
            imp.shell.body().last_child().as_ref(),
            Some(imp.footer.upcast_ref::<gtk4::Widget>()),
            "{} keeps its footer row last in the shell's body",
            stringify!($popover),
        );
    })+};
}

pub fn check() {
    last_in_body!(
        AudioPopover,
        KdeconnectPopover,
        NextEventPopover,
        ColorPickerPopover,
        ClipboardPopover,
        WeatherPopover,
        CalendarPopover,
        BluetoothPopover,
        DisplayPopover,
        IdlePopover,
        KeyboardPopover,
        NetworkPopover,
        NotificationsPopover,
        PlacesPopover,
        PowerPopover,
        PrintingPopover,
        RemovablePopover,
        RulerPopover,
        SessionPopover,
        SystemMonitorPopover,
        WorkspaceNamePopover,
    );
}
