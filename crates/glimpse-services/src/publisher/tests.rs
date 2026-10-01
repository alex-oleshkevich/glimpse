use super::*;

#[test]
fn unchanged_values_are_suppressed() {
    let (sender, receiver) = watch::channel(2);
    let publisher = Publisher::new(sender);

    assert!(!publisher.set(2));
    assert!(!receiver.has_changed().expect("sender is alive"));
    assert!(publisher.set(3));
    assert!(receiver.has_changed().expect("sender is alive"));
}

#[test]
fn a_new_receiver_has_the_current_snapshot() {
    let (sender, receiver) = watch::channel(2);
    let publisher = Publisher::new(sender);
    publisher.set(3);

    let current = receiver.clone();
    assert_eq!(*current.borrow(), 3);
}
