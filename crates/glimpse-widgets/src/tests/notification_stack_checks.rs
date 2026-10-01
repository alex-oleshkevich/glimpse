use super::*;

pub(super) fn peeking(four: [Notification; 4]) {
    let peeking = NotificationStack::new();
    peeking.set_items(&four);
    let peek_height = peeking.measure(gtk4::Orientation::Vertical, 600).1;
    peeking.allocate(600, peek_height, -1, None);
    let bottom = |widget: &gtk4::Widget| {
        widget
            .compute_bounds(&peeking)
            .map(|bounds| (bounds.y() + bounds.height()).round() as i32)
            .expect("an allocated child")
    };
    let peek_front = children_of::<NotificationCard>(&peeking)[0].clone();
    let peek_strips = peeking.imp().strips.borrow().clone();
    assert_eq!(peek_strips.len(), 2);
    assert_eq!(
        peeking.imp().covered_to(1),
        bottom(peek_front.upcast_ref()),
        "the near edge paints only below the front card, so a translucent card never shows \
             an opaque band through its bottom"
    );
    assert_eq!(
        peeking.imp().covered_to(0),
        bottom(peek_strips[1].upcast_ref()),
        "the far edge paints only below the near one"
    );
}
