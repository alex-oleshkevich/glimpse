use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};

use zbus::zvariant::OwnedObjectPath;

const OPEN: u8 = 0;
const CLOSED: u8 = 1;
const REPLIED: u8 = 2;

pub struct Flight {
    state: AtomicU8,
}

impl Flight {
    pub fn new() -> Self {
        Self {
            state: AtomicU8::new(OPEN),
        }
    }

    pub fn mark_closed(&self) {
        let _ = self
            .state
            .compare_exchange(OPEN, CLOSED, Ordering::SeqCst, Ordering::SeqCst);
    }

    pub fn is_closed(&self) -> bool {
        self.state.load(Ordering::SeqCst) == CLOSED
    }

    fn claim_reply(&self) -> bool {
        self.state
            .compare_exchange(OPEN, REPLIED, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
    }
}

pub async fn finish(flight: &Flight, reply: u32) -> zbus::fdo::Result<u32> {
    if flight.claim_reply() {
        Ok(reply)
    } else {
        std::future::pending().await
    }
}

pub struct PortalRequest {
    pub flight: Arc<Flight>,
    pub handle: OwnedObjectPath,
}

#[zbus::interface(name = "org.freedesktop.impl.portal.Request")]
impl PortalRequest {
    #[zbus(name = "Close")]
    async fn close(
        &self,
        #[zbus(object_server)] object_server: &zbus::ObjectServer,
    ) -> zbus::fdo::Result<()> {
        self.flight.mark_closed();
        let _ = object_server
            .remove::<PortalRequest, _>(self.handle.clone())
            .await;
        Ok(())
    }
}
