use super::*;

pub(super) fn check() {
    let network = NetworkPopover::new();
    network.set_radio(
        "Wi-Fi",
        "Skylink",
        "network-wireless-signal-good-symbolic",
        true,
        true,
    );

    let entries = vec![
        NetworkEntry {
            id: "/ap/1".to_owned(),
            title: "Skylink".to_owned(),
            icon: "network-wireless-signal-good-symbolic".to_owned(),
            place: NetworkPlace::Other,
            secured: true,
            selected: false,
            busy: false,
        },
        NetworkEntry {
            id: "/s/1".to_owned(),
            title: "Skylink 2G".to_owned(),
            place: NetworkPlace::Wifi,
            ..Default::default()
        },
    ];
    network.set_entries(&entries);
    assert_eq!(
        all_named(&network, "network-popover__row").len(),
        2,
        "one row per entry, split across their sections"
    );

    let held = all_named(&network, "network-popover__row");
    let mut renamed = entries.clone();
    renamed[1].title = "Skylink 5G".to_owned();
    network.set_entries(&renamed);
    let again = all_named(&network, "network-popover__row");
    assert!(
        held[0] == again[0],
        "a row whose content changed is updated in place, not replaced"
    );
    assert_eq!(
        again[0]
            .clone()
            .downcast::<SplitRow>()
            .expect("a split row")
            .row()
            .title()
            .map(|title| title.to_string()),
        Some("Skylink 5G".to_owned()),
        "and it actually took the new title"
    );

    let net_panel = |id: &str| -> gtk4::Revealer {
        let imp = network.imp();
        for held in [
            &imp.other_held,
            &imp.wifi_held,
            &imp.wired_held,
            &imp.vpn_held,
        ] {
            if let Some((_, holder)) = held.borrow().iter().find(|(key, _)| key == id) {
                return holder
                    .last_child()
                    .and_downcast::<gtk4::Revealer>()
                    .expect("a network row carries its own panel");
            }
        }
        panic!("no row is holding {id}");
    };
    let net_head = |id: &str| -> gtk4::Widget {
        let imp = network.imp();
        for held in [
            &imp.other_held,
            &imp.wifi_held,
            &imp.wired_held,
            &imp.vpn_held,
        ] {
            if let Some((_, holder)) = held.borrow().iter().find(|(key, _)| key == id) {
                return holder.first_child().expect("a holder leads with its row");
            }
        }
        panic!("no row is holding {id}");
    };
    let net_rows = |id: &str| -> Vec<Row> {
        let page = net_panel(id)
            .child()
            .and_then(|card| card.last_child())
            .and_downcast::<gtk4::Box>()
            .expect("an opened panel has a page");
        children_of::<Row>(&page)
    };

    assert!(
        net_head("/ap/1")
            .downcast::<SplitRow>()
            .expect("a split row")
            .row()
            .trail()
            .is_some_and(|lock| lock.get_visible()),
        "a secured network says so with a padlock"
    );
    assert!(
        net_head("/s/1")
            .downcast::<SplitRow>()
            .expect("a split row")
            .row()
            .trail()
            .is_some_and(|lock| !lock.get_visible()),
        "and an unsecured one hides the same padlock rather than growing a second widget"
    );

    assert!(
        !net_panel("/ap/1").reveals_child(),
        "a network reveals nothing until its chevron opens it"
    );

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
            ..Default::default()
        },
    ];
    let card = |id: &str| NetworkDetails {
        id: id.to_owned(),
        facts: vec![Fact::new("Security", "WPA2")],
        lines: lines.clone(),
    };
    let chevron = |id: &str| {
        net_head(id)
            .downcast::<SplitRow>()
            .expect("a split row")
            .emit_by_name::<()>("details", &[])
    };
    let dimmed = |widget: &gtk4::Widget| {
        std::iter::successors(Some(widget.clone()), |widget| widget.parent())
            .any(|widget| widget.has_css_class("receded"))
    };
    network.set_details(&[card("/ap/1"), card("/s/1")]);
    assert!(
        !net_panel("/ap/1").reveals_child(),
        "handing the details over opens nothing; the chevron does"
    );
    chevron("/ap/1");
    assert!(
        net_panel("/ap/1").reveals_child(),
        "the detail belongs under the network it describes, not beside the list"
    );
    assert!(
        !dimmed(&net_head("/ap/1"))
            && dimmed(&net_head("/s/1"))
            && dimmed(network.imp().hero.upcast_ref()),
        "everything the panel is read against recedes while it is open"
    );
    let opened = net_rows("/ap/1");
    assert_eq!(opened.len(), 3);
    assert!(
        net_panel("/ap/1")
            .child()
            .and_then(|card| card.first_child())
            .and_downcast::<FactList>()
            .is_some(),
        "the facts lead the card, before any action"
    );
    assert!(
        opened[1].clone().downcast::<SwitchRow>().is_ok(),
        "a line carrying a toggle is a SwitchRow, not a row with a switch dropped in it"
    );

    let acted = Rc::new(RefCell::new(Vec::new()));
    network.connect_acted({
        let acted = Rc::clone(&acted);
        move |_, id, action| acted.borrow_mut().push(format!("{id}/{action}"))
    });
    chevron("/s/1");
    assert!(
        !net_panel("/ap/1").reveals_child() && net_panel("/s/1").reveals_child(),
        "opening one network closes the one that was open"
    );
    net_rows("/s/1")[2].emit_by_name::<()>("clicked", &[]);
    assert_eq!(
        *acted.borrow(),
        ["/s/1/forget"],
        "a row whose action key repeats across networks must not forget the one no longer shown"
    );

    network.set_details(&[card("/ap/1")]);
    assert!(
        !net_panel("/s/1").reveals_child()
            && !dimmed(&net_head("/ap/1"))
            && !dimmed(network.imp().hero.upcast_ref()),
        "a network whose detail is gone closes, and takes the dimming with it"
    );

    let joined = Rc::new(RefCell::new(Vec::new()));
    network.connect_activated({
        let joined = Rc::clone(&joined);
        move |_, id| joined.borrow_mut().push(id.to_owned())
    });
    let mut connected = entries.clone();
    connected[0].selected = true;
    network.set_entries(&connected);
    let head = net_head("/ap/1");
    assert!(
        !head.is::<SplitRow>(),
        "a network in use is one row with no join target, so a click cannot disconnect it"
    );
    head.downcast::<Row>()
        .expect("a plain row")
        .emit_by_name::<()>("clicked", &[]);
    assert!(
        net_panel("/ap/1").reveals_child() && joined.borrow().is_empty(),
        "its whole row opens its card, where Disconnect lives, and reports nothing"
    );
    network.set_entries(&entries);
    assert!(
        net_head("/ap/1").is::<SplitRow>(),
        "once it is no longer in use its row joins again"
    );

    network.set_entries(&[]);
    assert!(
        !network.imp().other.get_visible(),
        "with no networks and no scan there is nothing to head"
    );
    network.set_scanning(true);
    assert!(
        network.imp().other.get_visible() && network.imp().other.empty(),
        "a scan that has found nothing yet is the whole reason the placeholder exists; a \
             hidden section shows it to nobody"
    );
    network.set_scanning(false);
    network.set_entries(&entries);

    let answers = Rc::new(RefCell::new(Vec::new()));
    network.connect_answered({
        let answers = Rc::clone(&answers);
        move |_, accepted, entered| answers.borrow_mut().push((accepted, entered.to_owned()))
    });

    assert!(
        !network.prompting() && network.imp().hero.is_sensitive(),
        "nothing is being asked, so the list is the page"
    );
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
        !network.imp().hero.is_sensitive(),
        "the Wi-Fi switch is not a way out of a question that is being asked"
    );
    assert!(
        !network.can_submit(),
        "an empty box cannot be submitted, here as in the dialog"
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
            && network.imp().hero.is_sensitive()
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
            && !header.has_css_class("open")
            && !network.imp().all.get_visible(),
        "closed, Other networks is one header line, with no count beside it"
    );
    network.set_others(12, true, Some("4 more networks"));
    assert!(
        header.has_css_class("open") && network.imp().all.get_visible(),
        "open and capped, the header turns and the list ends in a row that shows the rest"
    );
    network.set_others(0, false, None);
    assert!(
        !header.get_visible(),
        "no stranger in range, no header to open"
    );
}
