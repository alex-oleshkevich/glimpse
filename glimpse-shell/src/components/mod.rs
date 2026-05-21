pub mod action_row;
pub mod animated_popover;
pub mod badge;
pub mod card_surface;
pub mod collapsible_section;
pub mod copyable;
pub mod device_list;
pub mod device_status;
pub mod empty_state;
pub mod hero;
pub mod item;
pub mod key_value_grid;
pub mod list;
pub mod menu_button;
pub mod menu_item;
pub mod meter;
pub mod pager;
pub mod popover_scroll;
pub mod popover_shell;
pub mod section_header;
pub mod status_dot;
pub mod toast;

#[cfg(test)]
pub(crate) mod test_support {
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
}
