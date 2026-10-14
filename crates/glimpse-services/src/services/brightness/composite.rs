use std::sync::Arc;

use futures_util::future::BoxFuture;

use super::{Backlight, Entry, Enumeration, ReadOutcome};

const DDC_PREFIX: &str = "ddc:";

/// Merges two `Backlight` backends into one, routing `read`/`write` by a static id prefix rather
/// than remembering which backend served a given id from the last `enumerate`. This only works
/// because each backend owns a disjoint id namespace by construction: sysfs ids are bare device
/// names, `DdcBacklight` ids are always `ddc:<bus>`.
pub struct CompositeBacklight {
    primary: Arc<dyn Backlight>,
    ddc: Arc<dyn Backlight>,
}

impl CompositeBacklight {
    pub fn new(primary: Arc<dyn Backlight>, ddc: Arc<dyn Backlight>) -> Self {
        Self { primary, ddc }
    }

    fn route(&self, id: &str) -> &Arc<dyn Backlight> {
        if id.starts_with(DDC_PREFIX) {
            &self.ddc
        } else {
            &self.primary
        }
    }
}

impl Backlight for CompositeBacklight {
    fn enumerate(&self) -> BoxFuture<'_, Enumeration> {
        Box::pin(async move {
            let (mut enumeration, ddc) =
                tokio::join!(self.primary.enumerate(), self.ddc.enumerate());
            enumeration.entries.extend(ddc.entries);
            enumeration.unanswered.extend(ddc.unanswered);
            enumeration
        })
    }

    fn read(&self, id: String) -> BoxFuture<'_, ReadOutcome> {
        let backend = Arc::clone(self.route(&id));
        Box::pin(async move { backend.read(id).await })
    }

    fn write(&self, id: String, value: u32) -> BoxFuture<'_, Result<(), String>> {
        let backend = Arc::clone(self.route(&id));
        Box::pin(async move { backend.write(id, value).await })
    }

    fn reprobe(&self, connector: String) -> BoxFuture<'_, Option<Entry>> {
        self.ddc.reprobe(connector)
    }
}

#[cfg(test)]
mod tests;
