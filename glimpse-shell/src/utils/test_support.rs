use std::{
    sync::{Mutex, OnceLock},
    thread::ThreadId,
};

use relm4::gtk::{self, gio};

static GTK_INIT_LOCK: Mutex<()> = Mutex::new(());
static GTK_TEST_THREAD: OnceLock<ThreadId> = OnceLock::new();
static GTK_TEST_RESOURCES: OnceLock<bool> = OnceLock::new();

pub fn gtk_available_on_this_thread() -> bool {
    let Ok(_guard) = GTK_INIT_LOCK.lock() else {
        return false;
    };

    if gtk::is_initialized() {
        register_resources();
        return GTK_TEST_THREAD
            .get()
            .is_some_and(|thread| *thread == std::thread::current().id());
    }

    if gtk::init().is_err() {
        return false;
    }

    register_resources();

    let _ = GTK_TEST_THREAD.set(std::thread::current().id());
    true
}

fn register_resources() {
    GTK_TEST_RESOURCES.get_or_init(|| {
        gio::resources_register_include!("glimpse-shell.gresource")
            .expect("failed to register embedded resources for GTK tests");
        true
    });
}
