use glimpse_ipc::Client;

use super::Opener;

#[derive(Clone)]
pub struct Seat {
    name: String,
    client: Client,
    #[allow(dead_code, reason = "read by opener()")]
    host: relm4::Sender<super::runtime::HostInput>,
}

impl Seat {
    pub(crate) fn new(
        name: String,
        client: Client,
        host: relm4::Sender<super::runtime::HostInput>,
    ) -> Self {
        Self { name, client, host }
    }

    #[allow(
        dead_code,
        reason = "an applet's half of dismissal; no applet dismisses its own yet"
    )]
    pub fn opener(&self) -> Opener {
        Opener(self.host.clone())
    }

    pub fn caller(&self) -> super::Caller {
        super::Caller {
            name: format!("{}.popover", self.name),
            client: self.client.clone(),
        }
    }
}

pub trait PopoverHandle {
    fn root(&self) -> gtk4::Widget;
}
