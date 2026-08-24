use serde::de::DeserializeOwned;

use crate::context::Ctx;

#[derive(Debug, thiserror::Error)]
pub enum StartError {}

pub enum Input<S: Service> {
    Event(S::Event),
    Command(S::Command),
    Config(S::Config),
}

pub trait Service: Sized + Send + 'static {
    type Config: DeserializeOwned + PartialEq + Send + 'static;
    type Command: DeserializeOwned + Send + 'static;
    type Event: Send + 'static;

    fn start(
        ctx: &Ctx<Self>,
        config: Self::Config,
    ) -> impl Future<Output = Result<Self, StartError>> + Send;

    fn handle(&mut self, ctx: &Ctx<Self>, input: Input<Self>) -> impl Future<Output = ()> + Send;
    fn stop(self, ctx: &Ctx<Self>) -> impl Future<Output = ()> + Send {
        let _ = ctx;
        async {}
    }
}
