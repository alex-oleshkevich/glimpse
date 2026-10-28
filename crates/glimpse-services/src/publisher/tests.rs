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
