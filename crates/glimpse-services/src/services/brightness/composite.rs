use std::sync::Arc;

use futures_util::future::BoxFuture;

use super::{Backlight, Entry, ReadOutcome};

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
    fn enumerate(&self) -> BoxFuture<'_, Vec<Entry>> {
        Box::pin(async move {
            let (mut entries, ddc) = tokio::join!(self.primary.enumerate(), self.ddc.enumerate());
            entries.extend(ddc);
            entries
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::brightness::FakeBacklight;

    fn entry(id: &str) -> Entry {
        Entry {
            id: id.to_owned(),
            device_link: None,
            type_name: String::new(),
            brightness: 1,
            max_brightness: 2,
            kind: crate::services::brightness::Kind::Display,
        }
    }

    #[tokio::test]
    async fn enumerate_merges_both_backends() {
        let primary = FakeBacklight::new(vec![entry("panel")]);
        let ddc = FakeBacklight::new(vec![entry("ddc:5")]);
        let composite = CompositeBacklight::new(Arc::new(primary), Arc::new(ddc));

        let entries = composite.enumerate().await;

        assert_eq!(entries.len(), 2);
        assert!(entries.iter().any(|entry| entry.id == "panel"));
        assert!(entries.iter().any(|entry| entry.id == "ddc:5"));
    }

    #[tokio::test]
    async fn a_write_is_routed_by_the_ddc_prefix() {
        let primary = FakeBacklight::new(vec![entry("panel")]);
        let ddc = FakeBacklight::new(vec![entry("ddc:5")]);
        let composite = CompositeBacklight::new(Arc::new(primary.clone()), Arc::new(ddc.clone()));

        composite
            .write("panel".to_owned(), 50)
            .await
            .expect("the primary backend accepts it");
        composite
            .write("ddc:5".to_owned(), 60)
            .await
            .expect("the ddc backend accepts it");

        assert_eq!(primary.writes(), vec![("panel".to_owned(), 50)]);
        assert_eq!(ddc.writes(), vec![("ddc:5".to_owned(), 60)]);
    }
}
