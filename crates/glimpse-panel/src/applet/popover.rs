use glimpse_ipc::Client;

use super::Opener;

#[derive(Clone)]
pub struct Seat {
    name: String,
    client: Client,
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

pub fn run(command: &[String]) {
    let Some(program) = command.first() else {
        return;
    };
    let argv: Vec<&std::ffi::OsStr> = command.iter().map(|argument| argument.as_ref()).collect();

    if let Err(error) = gtk4::gio::Subprocess::newv(&argv, gtk4::gio::SubprocessFlags::NONE) {
        tracing::warn!(program, %error, "settings-command did not start");
    }
}

pub trait PopoverHandle {
    fn widget(&self) -> gtk4::Widget;
}

impl<W: gtk4::prelude::IsA<gtk4::Widget> + Clone> PopoverHandle for W {
    fn widget(&self) -> gtk4::Widget {
        use gtk4::prelude::Cast as _;
        self.clone().upcast()
    }
}
