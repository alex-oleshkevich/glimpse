use std::cell::{Cell, RefCell};
use std::rc::Rc;

use adw::prelude::*;
use gtk4::glib::subclass::prelude::*;

use super::{AgentDialog, Field, Icon, Note, Response, View, register_resources};

fn init() -> glimpse_widgets::Styles {
    gtk4::init().expect("display");
    adw::init().expect("libadwaita");
    register_resources().expect("resources");
    glimpse_widgets::Styles::install(adw::ColorScheme::Default)
}

fn view() -> View {
    View {
        icon: Icon::Lock,
        title: "Authentication needed".to_owned(),
        body: "Run a program as another user".to_owned(),
        accounts: vec!["Alex".to_owned(), "Maria".to_owned()],
        username: None,
        secret: Some(("Password".to_owned(), Field::Password)),
        autoconnect: None,
        code: None,
        details: vec![("Action".to_owned(), "org.example.run".to_owned())],
        note: None,
        busy: false,
        confirm: Some("Authorize".to_owned()),
        cancel: "Cancel".to_owned(),
    }
}

fn show_view() -> View {
    View {
        icon: Icon::Bluetooth,
        title: "Pair with “MX Keys”".to_owned(),
        body: "Type this code on the device, then press Enter".to_owned(),
        accounts: Vec::new(),
        secret: None,
        details: Vec::new(),
        code: Some("730 519".to_owned()),
        confirm: None,
        ..view()
    }
}

#[test]
fn field_rules_gate_what_can_be_confirmed() {
    assert!(Field::Password.accepts("x"));
    assert!(!Field::Password.accepts(""));
    assert!(Field::Visible.accepts("123456"));
    assert!(!Field::WifiKey.accepts("short"));
    assert!(Field::WifiKey.accepts("eightchr"));
    assert!(Field::WifiKey.accepts(&"é".repeat(63)));
    assert!(!Field::WifiKey.accepts(&"a".repeat(64).replace('a', "g")));
    assert!(Field::WifiKey.accepts(&"0a".repeat(32)));
    assert!(!Field::WifiKey.accepts(&"a".repeat(65)));
    assert!(Field::Pin.accepts("0000"));
    assert!(Field::Pin.accepts(&"a".repeat(16)));
    assert!(!Field::Pin.accepts(&"a".repeat(17)));
    assert!(!Field::Pin.accepts("00 0"));
    assert!(!Field::Pin.accepts(""));
    assert!(Field::Passkey.accepts("999999"));
    assert!(!Field::Passkey.accepts("1000000"));
    assert!(!Field::Passkey.accepts("+12"));
    assert!(!Field::Passkey.accepts(""));
}

#[test]
fn description_tints_each_quoted_span() {
    assert_eq!(
        super::description("Run `first' then `second'", "#abcdef"),
        "Run <span font_family=\"monospace\" foreground=\"#abcdef\">first</span> then <span font_family=\"monospace\" foreground=\"#abcdef\">second</span>",
    );
}

#[test]
fn description_preserves_an_unclosed_backtick() {
    assert_eq!(
        super::description("Run `first' then `unfinished", "#123456"),
        "Run <span font_family=\"monospace\" foreground=\"#123456\">first</span> then `unfinished",
    );
}

#[test]
fn description_escapes_hostile_text_inside_and_outside_quotes() {
    assert_eq!(
        super::description("<b>& `<b>&' <b>&", "#123456"),
        "&lt;b&gt;&amp; <span font_family=\"monospace\" foreground=\"#123456\">&lt;b&gt;&amp;</span> &lt;b&gt;&amp;",
    );
}

#[test]
fn description_without_backticks_is_plain_escaped_text() {
    let body = "<b>& plain 'text'";
    assert_eq!(
        super::description(body, "#123456"),
        gtk4::glib::markup_escape_text(body).as_str(),
    );
}

#[test]
#[ignore = "needs a display"]
fn showing_styled_icons_emits_no_criticals() {
    let _styles = init();
    let fatal = gtk4::glib::log_set_always_fatal(
        gtk4::glib::LogLevels::LEVEL_ERROR | gtk4::glib::LogLevels::LEVEL_CRITICAL,
    );
    let dialog = AgentDialog::new();
    let window = present(&dialog);
    for icon in [Icon::Lock, Icon::Wifi, Icon::Vpn, Icon::Bluetooth] {
        dialog.show(&View {
            icon,
            username: Some("alex".to_owned()),
            ..view()
        });
        dialog.imp().cancel.grab_focus();
        let main_loop = gtk4::glib::MainLoop::new(None, false);
        gtk4::glib::timeout_add_local_once(std::time::Duration::from_millis(100), {
            let main_loop = main_loop.clone();
            move || main_loop.quit()
        });
        main_loop.run();
        assert!(natural_height(&dialog) > 0);
        let mut children = vec![dialog.clone().upcast::<gtk4::Widget>()];
        let mut found = false;
        while let Some(widget) = children.pop() {
            if widget.is::<gtk4::Image>()
                && widget.has_css_class("edit-icon")
                && widget.is_child_visible()
            {
                assert!(
                    widget.measure(gtk4::Orientation::Horizontal, -1).1 > 0,
                    "edit icon has zero size: visible={}, child-visible={}, icon={:?}",
                    widget.is_visible(),
                    widget.is_child_visible(),
                    widget.downcast_ref::<gtk4::Image>().unwrap().icon_name(),
                );
                found = true;
            }
            let mut child = widget.first_child();
            while let Some(widget) = child {
                child = widget.next_sibling();
                children.push(widget);
            }
        }
        assert!(found, "entry edit icons are missing");
    }
    window.destroy();
    gtk4::glib::log_set_always_fatal(fatal);
}

#[test]
#[ignore = "needs a display"]
fn rows_show_only_what_the_view_carries() {
    let _styles = init();
    let dialog = AgentDialog::new();
    let imp = dialog.imp();
    dialog.show(&view());
    assert!(imp.rows.get_visible());
    assert!(imp.account.get_visible());
    assert!(imp.secret.get_visible());
    assert!(imp.details.get_visible());
    assert!(!imp.username.get_visible());
    assert!(!imp.autoconnect.get_visible());
    assert!(!imp.code.get_visible());
    assert!(!imp.note.get_visible());
    assert!(imp.confirm.get_visible());
    assert_eq!(
        imp.status.icon_name().as_deref(),
        Some("system-lock-screen-symbolic")
    );

    dialog.show(&View {
        accounts: vec!["Alex".to_owned()],
        note: Some(Note::Error("Wrong password".to_owned())),
        ..view()
    });
    assert!(!imp.account.get_visible());
    assert!(imp.note.get_visible());
    assert!(imp.note.has_css_class("error"));
    assert_eq!(imp.note.ellipsize(), gtk4::pango::EllipsizeMode::End);
    assert_eq!(imp.note.lines(), 1);
    assert!(!imp.note.wraps());
    assert!(imp.note.is_single_line_mode());
    assert!(imp.secret.has_css_class("error"));

    dialog.show(&View {
        note: Some(Note::Unavailable("Can't verify passwords".to_owned())),
        ..view()
    });
    imp.secret.set_text("secret");
    assert!(imp.note.has_css_class("error"));
    assert_eq!(imp.note.text(), "Can't verify passwords");
    assert!(!imp.secret.has_css_class("error"));
    assert!(!imp.confirm.is_sensitive());

    dialog.show(&show_view());
    assert!(!imp.rows.get_visible());
    assert!(imp.code.get_visible());
    assert_eq!(imp.code.text(), "730 519");
    assert!(!imp.confirm.get_visible());
    assert_eq!(imp.cancel.label().as_deref(), Some("Cancel"));
}

#[test]
#[ignore = "needs a display"]
fn showing_the_same_view_again_writes_nothing() {
    let _styles = init();
    let dialog = AgentDialog::new();
    let imp = dialog.imp();
    let typed = View {
        body: "Run `/usr/bin/ls' as another user".to_owned(),
        username: Some("alex".to_owned()),
        autoconnect: Some(true),
        ..view()
    };
    dialog.show(&typed);
    let changes = Rc::new(Cell::new(0));
    imp.accounts.connect_items_changed({
        let changes = changes.clone();
        move |_, _, _, _| changes.set(changes.get() + 1)
    });
    imp.status.connect_description_notify({
        let changes = changes.clone();
        move |_| changes.set(changes.get() + 1)
    });
    let rows = imp.detail_rows();
    imp.username.set_text("maria");
    imp.secret.set_text("hunter22");
    imp.autoconnect.set_active(false);

    dialog.show(&typed);
    assert_eq!(changes.get(), 0);
    assert_eq!(imp.detail_rows(), rows);
    assert_eq!(imp.username.text(), "maria");
    assert_eq!(imp.secret.text(), "hunter22");
    assert!(!imp.autoconnect.is_active());
}

#[test]
#[ignore = "needs a display"]
fn confirmation_waits_for_arming_and_note_updates_do_not_rearm() {
    let _styles = init();
    let dialog = AgentDialog::new();
    dialog.show(&view());
    let imp = dialog.imp();
    let responses = Rc::new(Cell::new(0));
    dialog.connect_response({
        let responses = responses.clone();
        move |_, response| {
            if matches!(response, Response::Confirm { .. }) {
                responses.set(responses.get() + 1);
            }
        }
    });
    imp.secret.set_text("hunter22");
    assert!(!imp.confirm.is_sensitive());
    imp.secret.emit_by_name::<()>("entry-activated", &[]);
    imp.username.emit_by_name::<()>("entry-activated", &[]);
    imp.confirm.emit_clicked();
    assert_eq!(responses.get(), 0);

    wait(std::time::Duration::from_millis(1050));
    assert!(imp.confirm.is_sensitive());
    imp.secret.emit_by_name::<()>("entry-activated", &[]);
    assert_eq!(responses.get(), 1);

    dialog.show(&View {
        note: Some(Note::Info("Try again".to_owned())),
        ..view()
    });
    assert!(imp.confirm.is_sensitive());
}

#[test]
#[ignore = "needs a display"]
fn capture_drops_early_keys_but_escape_still_cancels() {
    let _styles = init();
    let dialog = AgentDialog::new();
    dialog.show(&view());
    let controllers = dialog.observe_controllers();
    let keys = (0..controllers.n_items())
        .find_map(|index| {
            controllers
                .item(index)
                .and_downcast::<gtk4::EventControllerKey>()
        })
        .expect("capture key controller");
    assert_eq!(keys.propagation_phase(), gtk4::PropagationPhase::Capture);
    let cancels = Rc::new(Cell::new(0));
    dialog.connect_response({
        let cancels = cancels.clone();
        move |_, response| {
            if matches!(response, Response::Cancel) {
                cancels.set(cancels.get() + 1);
            }
        }
    });
    let key_press = |key: gtk4::gdk::Key| {
        keys.emit_by_name::<bool>(
            "key-pressed",
            &[&key, &0_u32, &gtk4::gdk::ModifierType::empty()],
        )
    };
    assert!(key_press(gtk4::gdk::Key::a));
    assert_eq!(dialog.imp().secret.text(), "");
    assert!(key_press(gtk4::gdk::Key::Escape));
    assert_eq!(cancels.get(), 1);

    wait(std::time::Duration::from_millis(450));
    assert!(!key_press(gtk4::gdk::Key::a));
    assert!(!dialog.imp().confirm.is_sensitive());
}

#[test]
#[ignore = "needs a display"]
fn focus_regain_restarts_the_full_arm_delay() {
    let _styles = init();
    let dialog = AgentDialog::new();
    dialog.show(&view());
    let imp = dialog.imp();
    imp.secret.set_text("hunter22");
    wait(std::time::Duration::from_millis(1050));
    assert!(imp.confirm.is_sensitive());

    imp.focus_changed(false);
    assert!(!imp.confirm.is_sensitive());
    wait(std::time::Duration::from_millis(1050));
    assert!(!imp.confirm.is_sensitive());
    imp.focus_changed(true);
    wait(std::time::Duration::from_millis(600));
    assert!(!imp.confirm.is_sensitive());
    imp.focus_changed(false);
    imp.focus_changed(true);
    wait(std::time::Duration::from_millis(500));
    assert!(!imp.confirm.is_sensitive());
    wait(std::time::Duration::from_millis(550));
    assert!(imp.confirm.is_sensitive());
}

#[test]
#[ignore = "needs a display"]
fn root_window_activation_restarts_the_full_arm_delay() {
    let _styles = init();
    let dialog = AgentDialog::new();
    dialog.show(&view());
    let window = present(&dialog);
    let imp = dialog.imp();
    imp.secret.set_text("hunter22");
    wait(std::time::Duration::from_millis(1050));
    assert!(window.is_active());
    assert!(imp.confirm.is_sensitive());

    let other = gtk4::Window::new();
    other.present();
    wait(std::time::Duration::from_millis(100));
    assert!(other.is_active());
    assert!(!window.is_active());
    assert!(!imp.confirm.is_sensitive());
    wait(std::time::Duration::from_millis(1050));
    assert!(!imp.confirm.is_sensitive());

    window.present();
    wait(std::time::Duration::from_millis(100));
    assert!(window.is_active());
    assert!(!other.is_active());
    assert!(!imp.confirm.is_sensitive());
    wait(std::time::Duration::from_millis(500));
    assert!(!imp.confirm.is_sensitive());
    wait(std::time::Duration::from_millis(550));
    assert!(imp.confirm.is_sensitive());
    other.destroy();
    window.destroy();
}

#[test]
#[ignore = "needs a display"]
fn replacing_and_closing_asks_cancel_their_timers() {
    let _styles = init();
    let first = AgentDialog::new();
    first.show(&view());
    first.imp().secret.set_text("first secret");
    let window = present(&first);
    wait(std::time::Duration::from_millis(500));

    let second = AgentDialog::new();
    second.show(&view());
    second.imp().secret.set_text("second secret");
    window.set_child(Some(&second));
    wait(std::time::Duration::from_millis(600));
    assert!(!first.imp().confirm.is_sensitive());
    assert!(!second.imp().confirm.is_sensitive());
    wait(std::time::Duration::from_millis(450));
    assert!(second.imp().confirm.is_sensitive());
    second.imp().focus_changed(true);
    second.set_sensitive(false);
    wait(std::time::Duration::from_millis(1050));
    second.set_sensitive(true);
    assert!(!second.imp().confirm.is_sensitive());
    window.destroy();
}

#[test]
#[ignore = "needs a display"]
fn confirm_waits_for_a_valid_secret_and_not_busy() {
    let _styles = init();
    let dialog = AgentDialog::new();
    let imp = dialog.imp();
    let wifi = View {
        icon: Icon::Wifi,
        secret: Some(("Password".to_owned(), Field::WifiKey)),
        confirm: Some("Join".to_owned()),
        ..view()
    };
    dialog.show(&wifi);
    wait(std::time::Duration::from_millis(1050));
    assert!(!imp.confirm.is_sensitive());
    imp.secret.set_text("short");
    assert!(!imp.confirm.is_sensitive());
    imp.secret.set_text("long enough");
    assert!(imp.confirm.is_sensitive());

    dialog.show(&View {
        busy: true,
        ..wifi.clone()
    });
    assert!(!imp.confirm.is_sensitive());
    assert!(imp.spinner.get_visible());
    assert!(!imp.secret.is_sensitive());
    assert_eq!(imp.secret.text(), "long enough");

    dialog.show(&wifi);
    assert!(!imp.spinner.get_visible());
    assert_eq!(imp.secret.text(), "");
    assert!(!imp.confirm.is_sensitive());
}

#[test]
#[ignore = "needs a display"]
fn a_dialog_confirms_again_and_cancel_still_emits() {
    let _styles = init();
    let dialog = AgentDialog::new();
    let imp = dialog.imp();
    dialog.show(&view());
    let seen = Rc::new(RefCell::new(Vec::new()));
    dialog.connect_response({
        let seen = seen.clone();
        move |_, response| {
            seen.borrow_mut().push(match response {
                Response::Confirm {
                    account, secret, ..
                } => format!("confirm {account} {}", secret.as_str()),
                Response::Cancel => "cancel".to_owned(),
                Response::Account(account) => format!("account {account}"),
            })
        }
    });
    imp.confirm.emit_clicked();
    imp.account.set_selected(1);
    imp.secret.set_text("hunter22");
    wait(std::time::Duration::from_millis(1050));
    imp.confirm.emit_clicked();
    imp.confirm.emit_clicked();
    imp.cancel.emit_clicked();
    assert_eq!(
        *seen.borrow(),
        [
            "account 1",
            "confirm 1 hunter22",
            "confirm 1 hunter22",
            "cancel"
        ]
    );
}

#[test]
#[ignore = "needs a display"]
fn changing_the_account_row_emits_the_selected_account() {
    let _styles = init();
    let dialog = AgentDialog::new();
    let imp = dialog.imp();
    let seen = Rc::new(RefCell::new(Vec::new()));
    dialog.connect_response({
        let seen = seen.clone();
        move |_, response| {
            if let Response::Account(account) = response {
                seen.borrow_mut().push(account);
            }
        }
    });
    dialog.show(&view());
    assert!(
        seen.borrow().is_empty(),
        "showing a view emitted an account"
    );
    imp.account.set_selected(1);
    assert_eq!(*seen.borrow(), [1]);
    imp.account.set_selected(0);
    assert_eq!(*seen.borrow(), [1, 0]);
    dialog.show(&View {
        accounts: vec!["Alex".to_owned(), "Maria".to_owned(), "Sam".to_owned()],
        ..view()
    });
    assert_eq!(*seen.borrow(), [1, 0], "filling the row emitted an account");
}

#[test]
#[ignore = "needs a display"]
fn a_long_command_in_the_body_is_shown_complete_and_wraps() {
    let _styles = init();
    let dialog = AgentDialog::new();
    let _window = present(&dialog);
    let command = format!("/usr/bin/env ARG={}", "p".repeat(600));
    let body = format!("Authentication is needed to run `{command}' as the super user");
    dialog.show(&View {
        body: body.clone(),
        ..view()
    });
    pump();
    let label = dialog.imp().body_label().expect("the body label");
    assert!(label.text().contains(&command));
    assert_eq!(label.text(), body.replace(['`', '\''], ""));
    assert!(label.wraps());
    assert_eq!(label.wrap_mode(), gtk4::pango::WrapMode::WordChar);
    assert_eq!(label.ellipsize(), gtk4::pango::EllipsizeMode::None);
    assert!(label.lines() <= 0);
}

#[test]
#[ignore = "needs a display"]
fn hostile_names_stay_one_plain_line() {
    let _styles = init();
    let dialog = AgentDialog::new();
    let imp = dialog.imp();
    let hostile = format!("<b>é\u{202e}\n{}", "ж".repeat(4096));
    dialog.show(&View {
        title: hostile.clone(),
        body: hostile.clone(),
        accounts: vec![hostile.clone(), hostile.clone()],
        details: vec![(hostile.clone(), hostile.clone())],
        note: Some(Note::Info(hostile)),
        ..view()
    });
    let title = imp.status.title();
    assert!(title.chars().count() <= super::TITLE_MAX + 1);
    assert!(title.starts_with("<b>"));
    assert!(!title.contains('\n') && !title.contains('\u{202e}'));
    let body = imp.status.description().unwrap_or_default();
    assert!(body.starts_with("&lt;b&gt;"));
    let account = imp.accounts.string(0).unwrap_or_default();
    assert!(account.chars().count() <= super::ACCOUNT_MAX + 1);
    assert!(imp.note.text().chars().count() <= super::BODY_MAX + 1);
    assert!(!imp.note.uses_markup());
    let rows = imp.detail_rows();
    let subtitle = rows[0].subtitle().unwrap_or_default();
    assert!(subtitle.chars().count() <= super::DETAIL_MAX + 1);
    assert!(!rows[0].uses_markup());
    assert!(!imp.secret.uses_markup() && !imp.account.uses_markup());
}

#[test]
#[ignore = "needs a display"]
fn the_dialog_grows_to_fit_the_note_and_the_details() {
    let _styles = init();
    if let Some(settings) = gtk4::Settings::default() {
        settings.set_gtk_enable_animations(false);
    }
    let dialog = AgentDialog::new();
    let _window = present(&dialog);
    dialog.show(&view());
    pump();
    let collapsed = natural_height(&dialog);

    dialog.show(&View {
        note: Some(Note::Error("Wrong password".to_owned())),
        ..view()
    });
    pump();
    let noted = natural_height(&dialog);
    assert!(
        noted > collapsed,
        "note did not grow the dialog: {collapsed} -> {noted}"
    );
    let noted_minimum = minimum_height(&dialog);

    dialog.imp().details.set_expanded(true);
    pump();
    let opened = natural_height(&dialog);
    assert!(
        opened > noted,
        "details did not grow the dialog: {noted} -> {opened}"
    );
    let opened_minimum = minimum_height(&dialog);
    assert!(
        opened_minimum > noted_minimum,
        "details did not raise the minimum height: {noted_minimum} -> {opened_minimum}"
    );
    let command = format!("<b>{}", "x".repeat(1497));
    dialog.show(&View {
        details: vec![("Command".to_owned(), command.clone())],
        note: Some(Note::Error("Wrong password".to_owned())),
        ..view()
    });
    pump();
    let row = &dialog.imp().detail_rows()[0];
    assert_eq!(row.subtitle().as_deref(), Some(command.as_str()));
    assert_eq!(row.subtitle_lines(), 0);
    assert!(row.is_subtitle_selectable());
    assert!(!row.uses_markup());
    let mut children = vec![row.clone().upcast::<gtk4::Widget>()];
    let mut found = false;
    while let Some(widget) = children.pop() {
        if let Some(label) = widget.downcast_ref::<gtk4::Label>()
            && label.text() == command
        {
            assert!(label.wraps());
            assert!(label.is_selectable());
            assert!(!label.uses_markup());
            assert_eq!(label.ellipsize(), gtk4::pango::EllipsizeMode::None);
            assert!(label.lines() <= 0);
            found = true;
        }
        let mut child = widget.first_child();
        while let Some(widget) = child {
            child = widget.next_sibling();
            children.push(widget);
        }
    }
    assert!(found, "the full command is missing from the row label");
    let long_minimum = minimum_height(&dialog);
    assert!(
        long_minimum > opened_minimum,
        "the command did not raise the minimum height: {opened_minimum} -> {long_minimum}"
    );
    let imp = dialog.imp();
    assert!(
        inside(&dialog, &*imp.confirm),
        "confirm is outside the dialog"
    );
    assert!(
        inside(&dialog, &*imp.cancel),
        "cancel is outside the dialog"
    );
}

#[test]
#[ignore = "needs a display"]
fn the_secret_row_is_focused_for_a_new_view_and_after_a_clear() {
    let _styles = init();
    let window = gtk4::Window::new();
    let first = AgentDialog::new();
    window.set_child(Some(&first));
    first.show(&view());
    window.present();
    pump();
    assert!(
        focus_inside(first.imp().secret.upcast_ref()),
        "the first secret row is not focused"
    );

    let second = AgentDialog::new();
    second.show(&view());
    window.set_child(Some(&second));
    pump();
    assert!(
        focus_inside(second.imp().secret.upcast_ref()),
        "a new head in a mapped surface did not focus the secret"
    );

    second.imp().secret.set_text("nope");
    second.show(&View {
        busy: true,
        ..view()
    });
    second.show(&View {
        note: Some(Note::Error("Wrong password".to_owned())),
        ..view()
    });
    pump();
    assert_eq!(second.imp().secret.text(), "");
    assert!(
        focus_inside(second.imp().secret.upcast_ref()),
        "the cleared secret row did not take focus"
    );
}

#[test]
#[ignore = "needs a display"]
fn a_view_without_an_entry_focuses_confirm() {
    let _styles = init();
    let dialog = AgentDialog::new();
    let _window = present(&dialog);
    dialog.show(&confirm_view("Pair", "Cancel", Some("123 456".to_owned())));
    wait(std::time::Duration::from_millis(1050));
    assert!(focus_inside(dialog.imp().confirm.upcast_ref()));
    assert!(!focus_inside(dialog.imp().cancel.upcast_ref()));

    dialog.show(&confirm_view("Allow", "Deny", None));
    pump();
    assert!(focus_inside(dialog.imp().confirm.upcast_ref()));
    assert!(dialog.imp().confirm.is_sensitive());
}

fn confirm_view(confirm: &str, cancel: &str, code: Option<String>) -> View {
    View {
        icon: Icon::Bluetooth,
        title: "Pair with “WH-1000XM4”?".to_owned(),
        body: "Check that the device shows the same code".to_owned(),
        accounts: Vec::new(),
        username: None,
        secret: None,
        autoconnect: None,
        code,
        details: Vec::new(),
        note: None,
        busy: false,
        confirm: Some(confirm.to_owned()),
        cancel: cancel.to_owned(),
    }
}

fn present(dialog: &AgentDialog) -> gtk4::Window {
    let window = gtk4::Window::new();
    window.set_child(Some(dialog));
    window.present();
    pump();
    window
}

fn pump() {
    let context = gtk4::glib::MainContext::default();
    for _ in 0..30 {
        let _ = context.iteration(false);
    }
}

fn wait(duration: std::time::Duration) {
    let main_loop = gtk4::glib::MainLoop::new(None, false);
    gtk4::glib::timeout_add_local_once(duration, {
        let main_loop = main_loop.clone();
        move || main_loop.quit()
    });
    main_loop.run();
}

fn natural_height(dialog: &AgentDialog) -> i32 {
    let width = dialog.measure(gtk4::Orientation::Horizontal, -1).1;
    dialog.measure(gtk4::Orientation::Vertical, width).1
}

fn minimum_height(dialog: &AgentDialog) -> i32 {
    let width = dialog.measure(gtk4::Orientation::Horizontal, -1).1;
    dialog.measure(gtk4::Orientation::Vertical, width).0
}

fn inside(host: &impl IsA<gtk4::Widget>, child: &impl IsA<gtk4::Widget>) -> bool {
    let Some(bounds) = child.compute_bounds(host) else {
        return false;
    };
    let height = host.height() as f32;
    bounds.y() >= -1.0 && bounds.y() + bounds.height() <= height + 1.0
}

fn focus_inside(widget: &gtk4::Widget) -> bool {
    if widget.has_focus() {
        return true;
    }
    let mut child = widget.first_child();
    while let Some(current) = child {
        if focus_inside(&current) {
            return true;
        }
        child = current.next_sibling();
    }
    false
}
