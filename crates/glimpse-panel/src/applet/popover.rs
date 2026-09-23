use super::Opener;

#[derive(Clone)]
pub struct Seat {
    host: relm4::Sender<super::runtime::HostInput>,
}

impl Seat {
    pub(crate) fn new(host: relm4::Sender<super::runtime::HostInput>) -> Self {
        Self { host }
    }

    pub fn opener(&self) -> Opener {
        Opener(self.host.clone())
    }
}

pub fn run(command: &[String]) {
    if let Err(error) = launch(command) {
        tracing::warn!(program = command.first(), %error, "settings-command did not start");
    }
}

pub fn launch(command: &[String]) -> Result<(), gtk4::glib::Error> {
    if command.is_empty() {
        return Ok(());
    }
    let launcher = gtk4::gio::SubprocessLauncher::new(gtk4::gio::SubprocessFlags::NONE);
    if !glimpse_utils::language_was_inherited() {
        launcher.unsetenv("LANGUAGE");
    }
    if let Some(token) = activation_token() {
        launcher.setenv("XDG_ACTIVATION_TOKEN", &token, true);
    }
    let argv: Vec<&std::ffi::OsStr> = command.iter().map(|argument| argument.as_ref()).collect();
    launcher.spawn(&argv).map(drop)
}

fn activation_token() -> Option<gtk4::glib::GString> {
    use gtk4::prelude::{AppLaunchContextExt as _, DisplayExt as _};
    if !gtk4::is_initialized_main_thread() {
        return None;
    }
    let token = gtk4::gdk::Display::default()?
        .app_launch_context()
        .startup_notify_id(gtk4::gio::AppInfo::NONE, &[])?;
    (!token.is_empty()).then_some(token)
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
