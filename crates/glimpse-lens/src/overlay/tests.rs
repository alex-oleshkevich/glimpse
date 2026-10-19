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
