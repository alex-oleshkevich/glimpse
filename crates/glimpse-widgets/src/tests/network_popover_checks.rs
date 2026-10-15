use super::*;

pub(super) fn check() {
    let network = NetworkPopover::new();
    let radio_changes = Rc::new(Cell::new(0));
    network.connect_wifi_toggled({
        let changes = Rc::clone(&radio_changes);
        move |_, _| changes.set(changes.get() + 1)
    });
    network.set_radio("Wi-Fi", true, true);
    network.set_radio("Wi-Fi", true, true);
    assert_eq!(
        radio_changes.get(),
        0,
        "rendering radio state sends no command"
    );
    assert!(network.imp().shell.subtitle().is_none());

    let hidden_requests = Rc::new(Cell::new(0));
    network.connect_hidden_network({
        let requests = Rc::clone(&hidden_requests);
        move |_| requests.set(requests.get() + 1)
    });
    network.set_hidden_entry(true);
    network.imp().hidden.emit_by_name::<()>("activated", &[]);
    assert_eq!(
        hidden_requests.get(),
        1,
        "the hidden-network action still emits its request"
    );

    let entries = vec![
        NetworkEntry {
            id: "/ap/1".to_owned(),
            title: "Skylink".to_owned(),
            icon: "network-wireless-signal-good-symbolic".to_owned(),
            place: NetworkPlace::Other,
            secured: true,
            ..Default::default()
        },
        NetworkEntry {
            id: "/s/1".to_owned(),
            title: "Skylink 2G".to_owned(),
            place: NetworkPlace::Wifi,
            ..Default::default()
        },
    ];
    network.set_entries(&entries);
    network.set_others(1, false, None);
    let held = all_named(&network, "network-popover__row");
    assert_eq!(held.len(), 2);

    let mut renamed = entries.clone();
    renamed[1].title = "Skylink 5G".to_owned();
    network.set_entries(&renamed);
    let again = all_named(&network, "network-popover__row");
    assert_eq!(held[0], again[0], "changing a title keeps its drawer");
    assert_eq!(
        again[0]
            .clone()
            .downcast::<ExpanderRow>()
            .expect("an expander")
            .title()
            .as_deref(),
        Some("Skylink 5G")
    );

    let net_row = |id: &str| {
        let imp = network.imp();
        for held in [
            &imp.other_held,
            &imp.wifi_held,
            &imp.wired_held,
            &imp.vpn_held,
        ] {
            if let Some((_, row)) = held.borrow().iter().find(|(key, _)| key == id) {
                return row.clone();
            }
        }
        panic!("no row is holding {id}");
    };
    let net_head = |id: &str| net_row(id).imp().expander.get();
    let net_rows = |id: &str| {
        let row = net_row(id);
        let imp = row.imp();
        let mut rows = Vec::new();
        for row in [
            imp.autoconnect.upcast_ref::<Row>(),
            imp.connection.upcast_ref::<Row>(),
            imp.forget.upcast_ref::<Row>(),
        ] {
            if row.get_visible() {
                rows.push(row.clone());
            }
        }
        rows
    };
    let toggle = |id: &str| {
        net_head(id).imp().head.emit_by_name::<()>("activated", &[]);
    };
    let dimmed = |widget: &gtk4::Widget| {
        std::iter::successors(Some(widget.clone()), |widget| widget.parent())
            .any(|widget| widget.has_css_class("receded"))
    };
    assert!(net_row("/ap/1").imp().status.get_visible());
    assert!(!net_row("/s/1").imp().status.get_visible());

    let lines = vec![
        NetworkLine {
            action: "connect".to_owned(),
            title: "Connect".to_owned(),
            activates: true,
            ..Default::default()
        },
        NetworkLine {
            action: "autoconnect".to_owned(),
            title: "Connect automatically".to_owned(),
            toggle: Some(false),
            ..Default::default()
        },
        NetworkLine {
            action: "forget".to_owned(),
            title: "Forget this network".to_owned(),
            activates: true,
            destructive: true,
            ..Default::default()
        },
    ];
    let card = |id: &str| NetworkDetails {
        id: id.to_owned(),
        facts: vec![Fact::new("Security", "WPA2")],
        lines: lines.clone(),
    };
    let action_card = |id: &str, action: &str, title: &str| NetworkDetails {
        id: id.to_owned(),
        facts: Vec::new(),
        lines: vec![NetworkLine {
            action: action.to_owned(),
            title: title.to_owned(),
            activates: true,
            ..Default::default()
        }],
    };
    network.set_details(&[card("/ap/1"), card("/s/1")]);
    assert!(!net_head("/ap/1").open());
    network
        .imp()
        .more
        .imp()
        .head
        .emit_by_name::<()>("activated", &[]);
    assert!(network.imp().more.open());
    toggle("/ap/1");
    assert!(
        net_head("/ap/1").open() && network.imp().more.open(),
        "opening a nested network preserves its disclosure"
    );
    assert!(
        !dimmed(net_head("/ap/1").upcast_ref())
            && dimmed(net_head("/s/1").upcast_ref())
            && dimmed(network.imp().wifi.upcast_ref()),
        "unrelated controls recede around the open network"
    );
    let details = net_head("/ap/1").details();
    assert!(
        details
            .first_child()
            .is_some_and(|child| child.is::<gtk4::Box>()),
        "the facts lead the drawer"
    );
    let opened = net_rows("/ap/1");
    assert_eq!(opened.len(), 3);
    assert!(opened[0].is::<ToggleRow>());
    assert_eq!(opened[1].title().as_deref(), Some("Connect"));
    assert_eq!(opened[2].tone(), RowTone::Destructive);
    toggle("/ap/1");
    assert!(
        !net_head("/ap/1").open(),
        "the same head also closes its drawer"
    );
    assert!(
        network.imp().more.open(),
        "closing its child preserves the disclosure"
    );

    let joined = Rc::new(RefCell::new(Vec::new()));
    network.connect_activated({
        let joined = Rc::clone(&joined);
        let entries = entries.clone();
        move |popover, id| {
            joined.borrow_mut().push(id.to_owned());
            popover.set_entries(&entries);
        }
    });
    toggle("/ap/1");
    assert!(
        joined.borrow().is_empty(),
        "opening a drawer sends no join command"
    );
    opened[1]
        .clone()
        .downcast::<ActionRow>()
        .expect("an action")
        .emit_by_name::<()>("activated", &[]);
    assert_eq!(*joined.borrow(), ["/ap/1"]);
    assert!(net_head("/ap/1").open(), "dispatch alone does not close");
    network.collapse("/ap/1");
    assert!(
        !net_head("/ap/1").open(),
        "a successful action closes its card"
    );

    let toggled = Rc::new(RefCell::new(Vec::new()));
    network.connect_toggled({
        let toggled = Rc::clone(&toggled);
        move |_, id, action, on| {
            toggled
                .borrow_mut()
                .push((id.to_owned(), action.to_owned(), on))
        }
    });
    let setting = opened[0].clone().downcast::<ToggleRow>().expect("a toggle");
    setting.emit_by_name::<()>("activate", &[]);
    setting.emit_by_name::<()>("activate", &[]);
    assert_eq!(
        *toggled.borrow(),
        [
            ("/ap/1".to_owned(), "autoconnect".to_owned(), true),
            ("/ap/1".to_owned(), "autoconnect".to_owned(), false),
        ]
    );
    setting.set_active(true);
    assert_eq!(toggled.borrow().len(), 2, "reconciliation stays quiet");
    let mut updated = card("/ap/1");
    updated.lines[1].toggle = Some(true);
    updated.lines[2].busy = true;
    network.set_details(&[updated, card("/s/1")]);
    assert_eq!(
        net_rows("/ap/1"),
        opened,
        "updates reuse the template actions"
    );
    assert!(setting.active() && net_rows("/ap/1")[2].busy());
    assert_eq!(
        toggled.borrow().len(),
        2,
        "backend state does not echo a command"
    );
    network.set_details(&[card("/ap/1"), card("/s/1")]);
    assert!(!setting.active());
    assert_eq!(toggled.borrow().len(), 2);
    setting.emit_by_name::<()>("activate", &[]);
    assert!(setting.active());
    network.set_details(&[card("/ap/1"), card("/s/1")]);
    assert!(
        !setting.active(),
        "unchanged details restore a toggle the command did not move"
    );

    let acted = Rc::new(RefCell::new(Vec::new()));
    network.connect_acted({
        let acted = Rc::clone(&acted);
        move |_, id, action| acted.borrow_mut().push(format!("{id}/{action}"))
    });
    toggle("/s/1");
    assert!(!net_head("/ap/1").open() && net_head("/s/1").open());
    assert!(
        !network.imp().more.open(),
        "a separate drawer closes the disclosure"
    );
    net_rows("/s/1")[2]
        .clone()
        .downcast::<ActionRow>()
        .expect("an action")
        .emit_by_name::<()>("activated", &[]);
    assert_eq!(*acted.borrow(), ["/s/1/forget"]);

    network.set_details(&[card("/ap/1")]);
    assert!(
        net_rows("/s/1").is_empty(),
        "missing actions hide their fixed rows, Connect included"
    );
    assert!(
        net_head("/s/1").open(),
        "losing optional detail keeps the drawer open"
    );

    let mut connected = entries.clone();
    connected[0].selected = true;
    network.set_entries(&connected);
    network.set_details(&[action_card("/ap/1", "disconnect", "Disconnect")]);
    assert_eq!(
        net_row("/ap/1").imp().status.icon_name().as_deref(),
        Some("object-select-symbolic")
    );
    let status = net_row("/ap/1").imp().status.get();
    assert!(status.has_css_class("accent"));
    assert!(
        gtk4::IconTheme::for_display(&status.display())
            .has_icon(status.icon_name().as_deref().expect("connected marker")),
        "the connected marker exists in the icon theme"
    );
    assert_eq!(net_rows("/ap/1")[0].title().as_deref(), Some("Disconnect"));
    network.imp().more.set_open(true);
    toggle("/ap/1");
    let before = joined.borrow().len();
    net_rows("/ap/1")[0]
        .clone()
        .downcast::<ActionRow>()
        .expect("an action")
        .emit_by_name::<()>("activated", &[]);
    assert_eq!(
        acted.borrow().last().map(String::as_str),
        Some("/ap/1/disconnect")
    );
    assert_eq!(
        joined.borrow().len(),
        before,
        "Disconnect cannot emit a join"
    );
    network.set_entries(&entries);
    network.set_details(&[card("/ap/1")]);
    assert!(!status.has_css_class("accent"));
    assert_eq!(net_rows("/ap/1")[1].title().as_deref(), Some("Connect"));

    let devices = [
        NetworkEntry {
            id: "/wired/1".to_owned(),
            place: NetworkPlace::Wired,
            ..Default::default()
        },
        NetworkEntry {
            id: "/vpn/1".to_owned(),
            place: NetworkPlace::Vpn,
            selected: true,
            ..Default::default()
        },
    ];
    network.set_entries(&devices);
    network.set_details(&[
        action_card("/wired/1", "connect", "Connect"),
        action_card("/vpn/1", "disconnect-vpn", "Disconnect"),
    ]);
    toggle("/wired/1");
    assert!(
        net_head("/wired/1").open(),
        "a connect-only drawer still opens"
    );
    let mut refreshed = devices.to_vec();
    refreshed.push(NetworkEntry {
        id: "/ap/refreshed".to_owned(),
        place: NetworkPlace::Wifi,
        ..Default::default()
    });
    network.set_entries(&refreshed);
    assert!(
        net_head("/wired/1").open(),
        "a Wi-Fi refresh keeps an open connect-only Ethernet drawer"
    );
    net_rows("/wired/1")[0]
        .clone()
        .downcast::<ActionRow>()
        .expect("an action")
        .emit_by_name::<()>("activated", &[]);
    assert_eq!(joined.borrow().last().map(String::as_str), Some("/wired/1"));
    network.set_entries(&devices);
    net_rows("/vpn/1")[0]
        .clone()
        .downcast::<ActionRow>()
        .expect("an action")
        .emit_by_name::<()>("activated", &[]);
    assert_eq!(
        acted.borrow().last().map(String::as_str),
        Some("/vpn/1/disconnect-vpn")
    );

    network.set_others(0, false, None);
    network.set_entries(&[]);
    assert!(!network.imp().other.get_visible());
    network.set_scanning(true);
    assert!(network.imp().other.get_visible() && network.imp().other.empty());
    network.set_scanning(false);
    network.set_entries(&entries);

    let answers = Rc::new(RefCell::new(Vec::new()));
    network.connect_answered({
        let answers = Rc::clone(&answers);
        move |_, accepted, entered| answers.borrow_mut().push((accepted, entered.to_owned()))
    });

    assert!(
        !network.prompting() && network.imp().wifi.is_sensitive(),
        "nothing is being asked, so the list is the page"
    );
    network.set_others(1, true, None);
    network.set_prompt(Some(&NetworkAsk {
        key: "hidden".to_owned(),
        network: "Hidden network".to_owned(),
        question: "Type the name the network broadcasts nothing about.".to_owned(),
        entered: NetworkEntered::Name,
        accept: "Continue".to_owned(),
        choices: Vec::new(),
        open_choice: None,
    }));
    assert!(
        network.prompting()
            && network.imp().prompt_name.get_visible()
            && !network.imp().prompt_secret.get_visible(),
        "a network with no name to show asks for one, not for a password"
    );
    assert!(
        !network.imp().wifi.is_sensitive(),
        "the Wi-Fi switch is not a way out of a question that is being asked"
    );
    assert!(!network.imp().more.open());
    network.set_others(1, true, None);
    assert!(
        !network.imp().more.open() && !dimmed(network.imp().prompt_accept.upcast_ref()),
        "network refreshes cannot reopen a drawer over the password page"
    );
    assert!(
        !network.can_submit(),
        "an empty box cannot be submitted, here as in the dialog"
    );
    network
        .imp()
        .prompt_name
        .emit_by_name::<()>("activate", &[]);
    assert!(
        answers.borrow().is_empty(),
        "Enter on an empty box sends nothing"
    );

    network.type_in("Skylink Guest");
    assert!(network.can_submit());
    network
        .imp()
        .prompt_accept
        .emit_by_name::<()>("clicked", &[]);
    assert_eq!(
        *answers.borrow(),
        [(true, "Skylink Guest".to_owned())],
        "the name reaches the applet, which asks for the password next"
    );

    network.set_prompt(Some(&NetworkAsk {
        key: "secret:Skylink Guest".to_owned(),
        network: "Skylink Guest".to_owned(),
        question: "The network needs a password before this computer can join it.".to_owned(),
        entered: NetworkEntered::Secret,
        accept: "Connect".to_owned(),
        choices: Vec::new(),
        open_choice: None,
    }));
    assert!(
        network.imp().prompt_secret.get_visible() && !network.imp().prompt_name.get_visible(),
        "the second ask is a password box, with the peek icon a name box must not have"
    );
    assert!(
        !network.can_submit(),
        "a new question starts empty; carrying the name over would submit it as a password"
    );

    network.type_in("wrongpassword");
    assert!(network.can_submit());
    network
        .imp()
        .prompt_secret
        .emit_by_name::<()>("activate", &[]);
    assert_eq!(
        answers.borrow().last(),
        Some(&(true, "wrongpassword".to_owned())),
        "Enter in the password box submits it"
    );
    network.set_prompt(Some(&NetworkAsk {
        key: "802-11-wireless-security:Skylink Guest".to_owned(),
        network: "Skylink Guest".to_owned(),
        question: "Skylink Guest refused that password. Check it and try again.".to_owned(),
        entered: NetworkEntered::Secret,
        accept: "Try again".to_owned(),
        choices: Vec::new(),
        open_choice: None,
    }));
    assert!(
        !network.can_submit(),
        "a retry clears the password that was refused; leaving it invites sending it again"
    );

    network.set_prompt(Some(&NetworkAsk {
        key: "hidden-secret:Skylink Guest".to_owned(),
        network: "Skylink Guest".to_owned(),
        question: "Choose how the network is secured, then type its password.".to_owned(),
        entered: NetworkEntered::Secret,
        accept: "Connect".to_owned(),
        choices: vec![
            "None".to_owned(),
            "WEP".to_owned(),
            "WPA & WPA2 Personal".to_owned(),
            "WPA3 Personal".to_owned(),
        ],
        open_choice: Some(0),
    }));
    assert!(
        network.imp().prompt_security.get_visible(),
        "a hidden network has no beacon to read its security off, so the user picks it"
    );
    assert!(
        !network.imp().prompt_secret.get_visible() && network.can_submit(),
        "an open network takes no password, and demanding one makes it unjoinable"
    );
    network.imp().prompt_security.set_selected(2);
    assert!(
        network.imp().prompt_secret.get_visible() && !network.can_submit(),
        "choosing WPA brings the password box back, empty and unsubmittable"
    );
    assert_eq!(network.chosen(), 2);

    network.type_in("hunter2hunter2");
    answers.borrow_mut().clear();
    network
        .imp()
        .prompt_cancel
        .emit_by_name::<()>("clicked", &[]);
    assert_eq!(
        *answers.borrow(),
        [(false, String::new())],
        "cancelling discards what was typed rather than sending it"
    );

    network.set_prompt(None);
    assert!(
        !network.prompting()
            && network.imp().wifi.is_sensitive()
            && network.imp().pages.visible_child_name().as_deref() == Some("networks"),
        "the question answered, the list comes back"
    );

    let secret = SecretDialog::new();
    secret.ask("psk:Skylink", "Skylink", false, NetworkEntered::Passphrase);
    let first = secret.heading().map(|one| one.to_string());
    assert!(
        !secret.body().is_empty(),
        "the first ask explains why it is asking"
    );

    secret.type_in("the-wrong-one");
    secret.ask("psk:Skylink", "Skylink", true, NetworkEntered::Passphrase);
    assert!(
        secret.is_blank(),
        "a retry clears the rejected password; leaving it invites retyping the same thing"
    );
    assert_ne!(
        secret.heading().map(|one| one.to_string()),
        first,
        "a retry must not look like the first ask, or the same password is typed again"
    );
    assert!(
        secret.heading().is_some_and(|one| !one.is_empty()),
        "the retry says the last password was refused"
    );

    network.set_others(3, false, None);
    let header = &network.imp().more;
    assert!(
        header.get_visible()
            && header.value().is_none()
            && !header.open()
            && !network.imp().all.get_visible(),
        "collapsed, Other networks has no count or extra action"
    );
    network.set_others(12, true, Some("4 more networks"));
    assert!(header.open() && network.imp().all.get_visible());
    assert!(network.imp().all.is::<ActionRow>());
    network.set_others(0, false, None);
    assert!(!header.get_visible() && !header.open());
}
