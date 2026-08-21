use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Power {
    pub lock_before_sleep: bool,
    pub lock_on_request: bool,
}

impl Default for Power {
    fn default() -> Self {
        Self {
            lock_before_sleep: true,
            lock_on_request: true,
        }
    }
}
