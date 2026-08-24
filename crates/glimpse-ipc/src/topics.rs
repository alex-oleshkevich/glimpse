use serde::{Serialize, de::DeserializeOwned};

pub trait Topic {
    const NAME: &'static str;
    type Payload: Serialize + DeserializeOwned + PartialEq + Clone + Send + 'static;
}

// #[topic("solar.status", ())]
// pub struct SolarStatus;
