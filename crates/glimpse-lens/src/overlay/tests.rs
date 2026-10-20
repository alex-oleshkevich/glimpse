use super::*;
use std::cell::Cell;
use std::rc::Rc;

fn item() -> glib::Object {
    glib::Object::new::<glib::Object>()
}

#[test]
fn an_added_output_ends_the_session_and_a_removed_one_does_not() {
    let store = gio::ListStore::new::<glib::Object>();
    store.append(&item());
    let ended = Rc::new(Cell::new(0));
    let (model, handler) = on_output_added(store.upcast_ref(), {
        let ended = ended.clone();
        move || ended.set(ended.get() + 1)
    });

    store.remove(0);
    assert_eq!(ended.get(), 0);

    store.append(&item());
    assert_eq!(ended.get(), 1);

    model.disconnect(handler);
    store.append(&item());
    assert_eq!(ended.get(), 1);
}

fn frame(connector: &str) -> Frame {
    Frame {
        connector: connector.to_owned(),
        width: 1,
        height: 1,
        pixels: glib::Bytes::from_owned([0u8; 4]),
        alpha: false,
        layout: Layout::Bgrx,
    }
}

#[test]
fn a_monitor_without_a_frame_names_its_output() {
    let monitors = vec![(1, "eDP-1".to_owned()), (2, "DP-1".to_owned())];
    assert_eq!(
        matched(monitors.clone(), vec![frame("eDP-1")])
            .err()
            .as_deref(),
        Some("output \"DP-1\"")
    );
    let paired = matched(monitors, vec![frame("DP-1"), frame("eDP-1")]).unwrap();
    assert_eq!(paired[0].1.connector, "eDP-1");
    assert_eq!(paired[1].1.connector, "DP-1");
    assert_eq!(
        matched(Vec::<(u8, String)>::new(), vec![]).err().as_deref(),
        Some("any output")
    );
}

#[test]
fn the_value_sits_under_the_lens_and_flips_above_it_near_the_bottom_edge() {
    let area = (1000.0, 800.0);
    let pill = (120.0, 34.0);

    assert_eq!(
        pill_origin((500.0, 300.0), 88.0, pill, area),
        (440.0, 398.0)
    );
    assert_eq!(
        pill_origin((500.0, 780.0), 88.0, pill, area),
        (440.0, 648.0)
    );
    assert_eq!(pill_origin((10.0, 300.0), 88.0, pill, area), (0.0, 398.0));
    assert_eq!(
        pill_origin((995.0, 300.0), 88.0, pill, area),
        (880.0, 398.0)
    );
    assert_eq!(
        pill_origin((500.0, 300.0), 106.0, pill, area),
        (440.0, 416.0)
    );
}
