#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SubscriptionId(pub u64);

pub trait BrokerHandle: Send + Sync + 'static {
    fn unsubscribe(&self, id: SubscriptionId);
}
