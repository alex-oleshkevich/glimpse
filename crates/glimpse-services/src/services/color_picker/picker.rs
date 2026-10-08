use std::future::Future;
use std::pin::Pin;

use glimpse_dbus::Buses;

pub type Picked = Pin<Box<dyn Future<Output = Result<Option<[u8; 3]>, String>> + Send>>;

pub trait Picker: Send + Sync + 'static {
    fn pick(&self) -> Picked;
}

pub struct PortalPicker {
    buses: Buses,
}

impl PortalPicker {
    pub fn new(buses: Buses) -> Self {
        Self { buses }
    }
}

impl Picker for PortalPicker {
    fn pick(&self) -> Picked {
        let connection = self.buses.session_bus().cloned().map_err(str::to_owned);
        Box::pin(async move {
            let connection = connection?;
            glimpse_dbus::screenshot_portal::pick_color(&connection)
                .await
                .map_err(|error| error.to_string())
        })
    }
}

#[cfg(test)]
mod tests;
