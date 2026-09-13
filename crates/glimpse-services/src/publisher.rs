use tokio::sync::watch;

#[derive(Clone)]
pub struct Publisher<P> {
    state: watch::Sender<P>,
}

impl<P> Publisher<P> {
    pub(crate) fn new(state: watch::Sender<P>) -> Self {
        Self { state }
    }
}

impl<P: PartialEq> Publisher<P> {
    pub fn set(&self, value: P) -> bool {
        self.state.send_if_modified(|held| {
            if *held == value {
                return false;
            }
            *held = value;
            true
        })
    }

    pub fn update(&self, change: impl FnOnce(&mut P)) -> bool
    where
        P: Clone,
    {
        self.state.send_if_modified(|held| {
            let mut next = held.clone();
            change(&mut next);
            if *held == next {
                false
            } else {
                *held = next;
                true
            }
        })
    }
}

#[cfg(test)]
mod tests {
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
}
