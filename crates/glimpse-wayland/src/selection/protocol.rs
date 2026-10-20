use wayland_client::{Connection, Dispatch, QueueHandle, protocol::wl_seat};
use wayland_protocols::ext::data_control::v1::client as ext;
use wayland_protocols_wlr::data_control::v1::client as wlr;

use super::backend::Backend;

pub enum Manager {
    Ext(ext::ext_data_control_manager_v1::ExtDataControlManagerV1),
    Wlr(wlr::zwlr_data_control_manager_v1::ZwlrDataControlManagerV1),
}

pub enum Device {
    Ext(ext::ext_data_control_device_v1::ExtDataControlDeviceV1),
    Wlr(wlr::zwlr_data_control_device_v1::ZwlrDataControlDeviceV1),
}

pub enum Source {
    Ext(ext::ext_data_control_source_v1::ExtDataControlSourceV1),
    Wlr(wlr::zwlr_data_control_source_v1::ZwlrDataControlSourceV1),
}

pub enum DataOffer {
    Ext(ext::ext_data_control_offer_v1::ExtDataControlOfferV1),
    Wlr(wlr::zwlr_data_control_offer_v1::ZwlrDataControlOfferV1),
}

impl Manager {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Ext(_) => "ext_data_control_manager_v1",
            Self::Wlr(_) => "zwlr_data_control_manager_v1",
        }
    }

    pub fn device(&self, seat: &wl_seat::WlSeat, qh: &QueueHandle<Backend>) -> Device {
        match self {
            Self::Ext(manager) => Device::Ext(manager.get_data_device(seat, qh, ())),
            Self::Wlr(manager) => Device::Wlr(manager.get_data_device(seat, qh, ())),
        }
    }

    pub fn source(&self, qh: &QueueHandle<Backend>) -> Source {
        match self {
            Self::Ext(manager) => Source::Ext(manager.create_data_source(qh, ())),
            Self::Wlr(manager) => Source::Wlr(manager.create_data_source(qh, ())),
        }
    }
}

impl Device {
    pub fn set_selection(&self, source: &Source) {
        match (self, source) {
            (Self::Ext(device), Source::Ext(source)) => device.set_selection(Some(source)),
            (Self::Wlr(device), Source::Wlr(source)) => device.set_selection(Some(source)),
            _ => tracing::error!("a data-control source built under the other protocol"),
        }
    }
}

impl Source {
    pub fn offer(&self, mime: String) {
        match self {
            Self::Ext(source) => source.offer(mime),
            Self::Wlr(source) => source.offer(mime),
        }
    }

    pub fn destroy(&self) {
        match self {
            Self::Ext(source) => source.destroy(),
            Self::Wlr(source) => source.destroy(),
        }
    }
}

impl DataOffer {
    pub fn key(&self) -> u32 {
        use wayland_client::Proxy as _;
        match self {
            Self::Ext(offer) => offer.id().protocol_id(),
            Self::Wlr(offer) => offer.id().protocol_id(),
        }
    }

    pub fn receive(&self, mime: String, fd: std::os::fd::BorrowedFd<'_>) {
        match self {
            Self::Ext(offer) => offer.receive(mime, fd),
            Self::Wlr(offer) => offer.receive(mime, fd),
        }
    }

    pub fn destroy(&self) {
        match self {
            Self::Ext(offer) => offer.destroy(),
            Self::Wlr(offer) => offer.destroy(),
        }
    }
}

macro_rules! ignore {
    ($($interface:path),* $(,)?) => {
        $(impl Dispatch<$interface, ()> for Backend {
            fn event(
                _state: &mut Self,
                _proxy: &$interface,
                _event: <$interface as wayland_client::Proxy>::Event,
                _data: &(),
                _conn: &Connection,
                _qh: &QueueHandle<Self>,
            ) {
            }
        })*
    };
}

ignore!(
    ext::ext_data_control_manager_v1::ExtDataControlManagerV1,
    wlr::zwlr_data_control_manager_v1::ZwlrDataControlManagerV1,
);
