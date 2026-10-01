use super::*;

pub(super) fn check() {
    let audio = AudioPopover::new();
    assert_eq!(audio.imp().hero.title().as_deref(), Some("Sound"));
    assert!(
        !audio.imp().output.has_css_class("accent") && !audio.imp().input.has_css_class("accent"),
        "neither fader is accented; both take the default white track the brightness fader uses"
    );

    audio.set_heading("audio-volume-muted-symbolic", Some("Speakers"), true);
    assert!(
        audio.imp().hero.subtitle().as_deref() == Some("Speakers")
            && audio.imp().hero.has_css_class("audio-popover__hero--muted"),
        "the header follows the output: its device, and the warning colour while muted"
    );
    audio.set_heading("audio-volume-high-symbolic", Some("Speakers"), false);
    assert!(!audio.imp().hero.has_css_class("audio-popover__hero--muted"));

    audio.set_footer(Some("Sound settings"));
    assert!(audio.imp().footer.get_visible());

    let out_entries = vec![
        AudioEntry {
            id: "sink-1".to_owned(),
            title: "Speakers".to_owned(),
            icon: Some("audio-speakers-symbolic".to_owned()),
            selected: true,
            ..Default::default()
        },
        AudioEntry {
            id: "sink-2".to_owned(),
            title: "Headphones".to_owned(),
            ..Default::default()
        },
    ];
    audio.set_outputs(&out_entries);
    assert!(audio.imp().outputs.get_visible());
    assert_eq!(
        audio.imp().output_current.title().as_deref(),
        Some("Speakers"),
        "one row names the device in use"
    );
    let listed = || children_of::<Row>(&audio.imp().output_rows.get());
    assert_eq!(listed().len(), 2, "its card lists every output device");
    assert!(
        !audio.imp().output_device.expanded(),
        "the list stays closed until the row opens it"
    );

    let held = listed();
    let mut renamed = out_entries.clone();
    renamed[0].title = "Studio monitors".to_owned();
    audio.set_outputs(&renamed);
    assert!(
        held[0] == listed()[0],
        "a row whose content changed is updated in place, not replaced"
    );
    assert_eq!(
        audio.imp().output_current.title().as_deref(),
        Some("Studio monitors")
    );

    audio.imp().output_current.emit_clicked();
    assert!(audio.imp().output_device.expanded());
    let selected = Rc::new(RefCell::new(Vec::new()));
    audio.connect_device_selected({
        let selected = Rc::clone(&selected);
        move |_, dir, id| selected.borrow_mut().push(format!("{dir}/{id}"))
    });
    listed()[1].emit_clicked();
    assert_eq!(*selected.borrow(), ["output/sink-2"]);
    audio.collapse("output");
    assert!(
        !audio.imp().output_device.expanded(),
        "a switch that succeeded closes the card it was chosen from"
    );

    audio.set_outputs(&[]);
    assert!(
        !audio.imp().outputs.get_visible(),
        "with no output devices there is nothing to head"
    );
    audio.set_outputs(&out_entries);

    audio.set_inputs(&[AudioEntry {
        id: "source-1".to_owned(),
        title: "Microphone".to_owned(),
        ..Default::default()
    }]);
    assert!(audio.imp().inputs.get_visible());
    assert_eq!(
        audio.imp().input_current.title().as_deref(),
        Some("Microphone"),
        "with no default reported the first device names the row"
    );

    let levels = Rc::new(RefCell::new(Vec::new()));
    audio.connect_level_changed({
        let levels = Rc::clone(&levels);
        move |_, dir, value| levels.borrow_mut().push(format!("{dir}/{value}"))
    });
    let toggles = Rc::new(RefCell::new(Vec::new()));
    audio.connect_level_toggled({
        let toggles = Rc::clone(&toggles);
        move |_, dir, muted| toggles.borrow_mut().push(format!("{dir}/{muted}"))
    });

    audio.set_output_level(64.0, false, Some("audio-volume-high-symbolic"));
    assert_eq!(audio.imp().output.value(), 64.0);
    audio
        .imp()
        .output
        .emit_by_name::<()>("changed", &[&64.0f64]);
    audio.imp().input.emit_by_name::<()>("changed", &[&30.0f64]);
    assert_eq!(*levels.borrow(), ["output/64", "input/30"]);
    audio.imp().output.emit_by_name::<()>("toggled", &[&true]);
    assert_eq!(*toggles.borrow(), ["output/true"]);

    let apps = vec![
        AudioEntry {
            id: "app-a".to_owned(),
            title: "Firefox".to_owned(),
            muted_icon: Some("audio-volume-muted-symbolic".to_owned()),
            ..Default::default()
        },
        AudioEntry {
            id: "app-b".to_owned(),
            title: "OBS Studio".to_owned(),
            ..Default::default()
        },
    ];
    audio.set_apps(&apps);
    assert_eq!(all_named(&audio, "audio-popover__app").len(), 2);
    assert!(audio.imp().apps.get_visible());

    let app = |id: &str| -> Expandable {
        audio
            .imp()
            .app_held
            .borrow()
            .iter()
            .find(|(key, _)| key == id)
            .map(|(_, holder)| holder.clone())
            .expect("a held app")
    };
    let app_head = |id: &str| -> Row { app(id).head::<Row>().expect("an app row") };
    let muted_glyph = |id: &str| -> gtk4::Image {
        app_head(id)
            .trail()
            .and_then(|trail| trail.first_child())
            .and_downcast::<gtk4::Image>()
            .expect("an app row's trail leads with its muted glyph")
    };
    assert!(
        muted_glyph("app-a").get_visible()
            && muted_glyph("app-a").has_css_class("audio-popover__muted")
            && !muted_glyph("app-b").get_visible(),
        "a muted app shows the muted glyph, in the warning colour, instead of a word"
    );

    let dimmed = |widget: &gtk4::Widget| {
        std::iter::successors(Some(widget.clone()), |widget| widget.parent())
            .any(|widget| widget.has_css_class("receded"))
    };
    let single_role = AudioDetails {
        id: "app-a".to_owned(),
        blocks: vec![AudioBlock {
            dir: "output".to_owned(),
            volume: 64.0,
            adjustable: true,
            devices: vec![AudioEntry {
                id: "sink-1".to_owned(),
                title: "Speakers".to_owned(),
                selected: true,
                ..Default::default()
            }],
            ..Default::default()
        }],
    };
    let two_roles = AudioDetails {
        id: "app-b".to_owned(),
        blocks: vec![
            AudioBlock {
                dir: "output".to_owned(),
                heading: Some("Output".to_owned()),
                volume: 40.0,
                adjustable: true,
                ..Default::default()
            },
            AudioBlock {
                dir: "input".to_owned(),
                heading: Some("Input".to_owned()),
                muted: true,
                adjustable: false,
                devices: vec![AudioEntry {
                    id: "source-1".to_owned(),
                    title: "Microphone".to_owned(),
                    selected: true,
                    ..Default::default()
                }],
                ..Default::default()
            },
        ],
    };
    audio.set_details(&[single_role, two_roles.clone()]);
    assert!(
        !app("app-a").expanded(),
        "handing the details over opens nothing; the row does"
    );

    app_head("app-a").emit_clicked();
    assert!(
        app("app-a").expanded() && app("app-a").has_css_class("open"),
        "the card belongs under the application it describes, not beside the list"
    );
    assert!(
        dimmed(app_head("app-b").upcast_ref())
            && dimmed(audio.imp().output.upcast_ref())
            && dimmed(audio.imp().hero.upcast_ref())
            && !dimmed(app_head("app-a").upcast_ref()),
        "everything the card is read against recedes while it is open"
    );

    let card_blocks = |id: &str| -> Vec<gtk4::Box> {
        children_of::<gtk4::Box>(
            &app(id)
                .details::<gtk4::Box>()
                .expect("an opened app has a card"),
        )
    };
    let heading_label = |block: &gtk4::Box| -> gtk4::Label {
        children_of::<gtk4::Label>(block)
            .into_iter()
            .next()
            .expect("a block has a heading label")
    };
    let block_fader = |block: &gtk4::Box| -> Fader {
        children_of::<Fader>(block)
            .into_iter()
            .next()
            .expect("a block has a fader")
    };
    let block_devices_box = |block: &gtk4::Box| -> gtk4::Box {
        children_of::<gtk4::Box>(block)
            .into_iter()
            .next()
            .expect("a block has a device rows box")
    };

    let single = card_blocks("app-a");
    assert_eq!(
        single.len(),
        1,
        "an application with one role renders one block"
    );
    assert!(
        !heading_label(&single[0]).get_visible(),
        "a single-role application draws no heading"
    );
    assert_eq!(block_fader(&single[0]).value(), 64.0);
    assert!(block_fader(&single[0]).is_sensitive());

    app_head("app-b").emit_clicked();
    assert!(
        !app("app-a").expanded() && app("app-b").expanded(),
        "opening one application closes the one that was open"
    );

    let pair = card_blocks("app-b");
    assert_eq!(
        pair.len(),
        2,
        "an application with two roles renders two blocks"
    );
    assert_eq!(heading_label(&pair[0]).text().as_str(), "Output");
    assert_eq!(
        heading_label(&pair[1]).text().as_str(),
        "Input",
        "blocks render in output-then-input order"
    );
    assert!(block_fader(&pair[0]).is_sensitive());
    assert!(
        !block_fader(&pair[1]).is_sensitive(),
        "a block with adjustable: false yields an insensitive fader"
    );

    let device_rows = children_of::<Row>(&block_devices_box(&pair[1]));
    assert_eq!(device_rows.len(), 1);
    assert!(device_rows[0].selected());
    assert_eq!(
        children_of::<Row>(&block_devices_box(&card_blocks("app-a")[0])).len(),
        1,
        "refreshing one app's card leaves another app's rows where they are"
    );

    let moved = Rc::new(RefCell::new(Vec::new()));
    audio.connect_app_moved({
        let moved = Rc::clone(&moved);
        move |_, app, dir, device| moved.borrow_mut().push(format!("{app}/{dir}/{device}"))
    });
    device_rows[0].emit_clicked();
    assert_eq!(*moved.borrow(), ["app-b/input/source-1"]);

    let app_levels = Rc::new(RefCell::new(Vec::new()));
    audio.connect_app_level_changed({
        let app_levels = Rc::clone(&app_levels);
        move |_, app, dir, value| app_levels.borrow_mut().push(format!("{app}/{dir}/{value}"))
    });
    block_fader(&pair[0]).emit_by_name::<()>("changed", &[&77.0f64]);
    assert_eq!(*app_levels.borrow(), ["app-b/output/77"]);

    let app_toggles = Rc::new(RefCell::new(Vec::new()));
    audio.connect_app_level_toggled({
        let app_toggles = Rc::clone(&app_toggles);
        move |_, app, dir, muted| {
            app_toggles
                .borrow_mut()
                .push(format!("{app}/{dir}/{muted}"))
        }
    });
    block_fader(&pair[1]).emit_by_name::<()>("toggled", &[&true]);
    assert_eq!(*app_toggles.borrow(), ["app-b/input/true"]);

    audio.collapse("app-b");
    assert!(
        !app("app-b").expanded() && !dimmed(app_head("app-a").upcast_ref()),
        "a move that succeeded closes the card, and takes the dimming with it"
    );

    audio.set_details(&[two_roles]);
    app_head("app-a").emit_clicked();
    assert!(
        !app("app-a").expanded(),
        "an app whose detail is gone has no card to open"
    );

    audio.set_apps(&[]);
    assert!(
        !audio.imp().apps.get_visible(),
        "no applications is nothing to head, the same as an empty output or input list"
    );
    audio.set_apps(&apps);

    audio.set_more_apps(Some("3 more apps"));
    assert!(audio.imp().more_apps.get_visible());
    let expanded = Rc::new(RefCell::new(Vec::new()));
    audio.connect_expanded({
        let expanded = Rc::clone(&expanded);
        move |_, place| expanded.borrow_mut().push(place.to_owned())
    });
    audio.imp().more_apps.emit_clicked();
    assert_eq!(*expanded.borrow(), ["apps"]);
    audio.set_more_apps(None);
    assert!(!audio.imp().more_apps.get_visible());

    let footer_activated = Rc::new(Cell::new(0u32));
    audio.connect_footer_activated({
        let footer_activated = Rc::clone(&footer_activated);
        move |_| footer_activated.set(footer_activated.get() + 1)
    });
    audio.imp().footer.emit_clicked();
    assert_eq!(footer_activated.get(), 1);
}
