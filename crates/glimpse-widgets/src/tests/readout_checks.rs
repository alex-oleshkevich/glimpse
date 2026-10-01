use super::*;

pub(super) fn check() {
    let readout = Readout::new();
    let readout_value = child_named::<gtk4::Label>(&readout, "readout__value");
    let readout_unit = child_named::<gtk4::Label>(&readout, "readout__unit");
    assert!(!readout_value.get_visible() && !readout_unit.get_visible());
    readout.set_value(Some("18"));
    readout.set_unit(Some("°"));
    assert!(readout_value.get_visible() && readout_unit.get_visible());
    assert_eq!(
        *readout.imp().value.text(),
        *"18",
        "the value and the unit are separate labels so each can carry its own size"
    );
    readout.set_unit(None::<&str>);
    assert!(
        !readout_unit.get_visible(),
        "a value with no unit reserves no width for one"
    );
}
