use std::{
    cell::{Cell, RefCell},
    collections::{HashMap, HashSet},
    rc::Rc,
    time::{Duration, Instant},
};

use relm4::{
    ComponentParts, ComponentSender, SimpleComponent,
    gtk::{self, glib, prelude::*},
};

use crate::{
    applets::brightness::format,
    utils::popover_scroll,
    widgets::{
        animated_popover::AnimatedPopover, empty_state::EmptyState, hero::Hero,
        popover_shell::PopoverShell, slider_tile::SliderTile,
    },
};
use glimpse_core::{
    compositors::Monitor,
    services::brightness::{BrightnessSource, BrightnessSourceKind, Command, State},
};

const ROW_COMMAND_INTERVAL: Duration = Duration::from_millis(80);
const BRIGHTNESS_ECHO_GRACE: Duration = Duration::from_secs(2);

pub struct Popover {
    popover: AnimatedPopover,
    state: State,
    sources: Vec<BrightnessSource>,
    monitors: Vec<Monitor>,
    list: gtk::Box,
    rows: HashMap<String, SourceRow>,
}

pub struct PopoverInit {
    pub parent: gtk::Box,
}

#[derive(Debug)]
pub enum PopoverInput {
    Toggle,
    UpdateState(State),
    UpdateMonitors(Vec<Monitor>),
    RowCommand(Command),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PopoverOutput {
    Command(Command),
}

#[allow(unused_assignments)]
#[relm4::component(pub)]
impl SimpleComponent for Popover {
    type Init = PopoverInit;
    type Input = PopoverInput;
    type Output = PopoverOutput;

    view! {
        root = AnimatedPopover {
            add_css_class: "popover-size-medium",

            PopoverShell {

                #[name = "hero"]
                Hero {
                    #[watch]
                    set_icon: Some(format::icon_name(&model.state)),
                    set_title: "Brightness",
                    #[watch]
                    set_subtitle: &format::hero_subtitle_with_monitors(&model.state, &model.monitors),
                },

                EmptyState {
                    set_title: "No display controls",
                    set_subtitle: Some("No writable display brightness sources available"),
                    #[watch]
                    set_visible: model.sources.is_empty(),
                },

                #[name = "scroller"]
                gtk::ScrolledWindow {
                    set_policy: (gtk::PolicyType::Never, gtk::PolicyType::Automatic),
                    set_vexpand: false,
                    set_propagate_natural_height: true,
                    #[watch]
                    set_visible: !model.sources.is_empty(),

                    #[name = "list"]
                    gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,
                        set_spacing: 2,
                    },
                },
            },
        }
    }

    fn init(
        init: Self::Init,
        _root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let state = State::default();
        let mut model = Popover {
            popover: AnimatedPopover::new(),
            sources: usable_sources(&state),
            monitors: Vec::new(),
            state,
            list: gtk::Box::new(gtk::Orientation::Vertical, 2),
            rows: HashMap::new(),
        };

        let widgets = view_output!();
        model.popover = widgets.root.clone();
        model.list = widgets.list.clone();

        widgets.root.set_parent(&init.parent);
        popover_scroll::install_half_monitor_limit(
            widgets.root.upcast_ref(),
            &widgets.scroller,
            &init.parent,
        );

        model.sync_rows(&sender);

        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>) {
        match message {
            PopoverInput::Toggle => {
                self.popover.toggle();
            }
            PopoverInput::UpdateState(state) => {
                self.sources = usable_sources(&state);
                self.state = state;
                self.sync_rows(&sender);
            }
            PopoverInput::UpdateMonitors(monitors) => {
                self.monitors = monitors;
                self.sync_rows(&sender);
            }
            PopoverInput::RowCommand(command) => {
                let _ = sender.output(PopoverOutput::Command(command));
            }
        }
    }
}

impl Popover {
    fn sync_rows(&mut self, sender: &ComponentSender<Self>) {
        let sources = self
            .sources
            .iter()
            .map(|source| {
                let mut source = source.clone();
                source.name = format::source_display_name(&source, &self.monitors);
                source
            })
            .collect();
        sync_source_rows(&mut self.rows, &self.list, sources, sender);
    }
}

#[derive(Debug, Clone)]
struct PendingBrightness {
    percent: u8,
    changed_at: Instant,
}

impl PendingBrightness {
    fn new(percent: u8) -> Self {
        Self {
            percent,
            changed_at: Instant::now(),
        }
    }
}

struct SourceRow {
    root: SliderTile,
    icon: gtk::Image,
    source: Rc<RefCell<BrightnessSource>>,
    updating: Rc<Cell<bool>>,
    pending_service_percent: Rc<RefCell<Option<PendingBrightness>>>,
}

impl SourceRow {
    fn new(source: &BrightnessSource, sender: &ComponentSender<Popover>) -> Self {
        let root = SliderTile::new();
        root.add_css_class("display-control");

        let icon = gtk::Image::from_icon_name(&source.icon);
        icon.add_css_class("display-control__icon");
        icon.add_css_class("display-control__icon-slot");
        icon.set_pixel_size(16);
        icon.set_halign(gtk::Align::Center);
        icon.set_valign(gtk::Align::Center);
        root.set_left(Some(icon.clone()));

        let source_state = Rc::new(RefCell::new(source.clone()));
        let updating = Rc::new(Cell::new(false));
        let last_sent = Rc::new(Cell::new(Instant::now() - ROW_COMMAND_INTERVAL));
        let pending = Rc::new(Cell::new(false));
        let pending_percent = Rc::new(Cell::new(0));
        let pending_service_percent = Rc::new(RefCell::new(None));

        root.connect_changed({
            let source_state = source_state.clone();
            let updating = updating.clone();
            let last_sent = last_sent.clone();
            let pending = pending.clone();
            let pending_percent = pending_percent.clone();
            let pending_service_percent = pending_service_percent.clone();
            let sender = sender.clone();
            move |_, value| {
                if updating.get() {
                    return;
                }

                let percent = percent_from_scale_value(&source_state.borrow(), value);
                let id = {
                    let mut source = source_state.borrow_mut();
                    if percent == source.percent {
                        return;
                    }
                    source.current = current_from_percent(&source, percent);
                    source.percent = percent;
                    source.id.clone()
                };

                pending_service_percent
                    .borrow_mut()
                    .replace(PendingBrightness::new(percent));
                emit_throttled_row_command(
                    id,
                    percent,
                    last_sent.clone(),
                    pending.clone(),
                    pending_percent.clone(),
                    sender.clone(),
                );
            }
        });

        let row = Self {
            root,
            icon,
            source: source_state,
            updating,
            pending_service_percent,
        };
        row
    }

    fn update(&self, mut source: BrightnessSource) {
        let now = Instant::now();
        let should_apply_value = {
            let mut pending = self.pending_service_percent.borrow_mut();
            should_apply_service_percent(&mut pending, source.percent, now)
        };

        if !should_apply_value {
            let current = self.source.borrow();
            source.percent = current.percent;
            source.current = current.current;
        }

        self.source.replace(source.clone());
        self.icon.set_icon_name(Some(&source.icon));
        self.root.set_label(Some(&source.name));
        self.root.set_sensitive(source.writable);
        self.root
            .set_tooltip_text(Some(&format!("{} - {}%", source.name, source.percent)));
        if source.primary {
            self.root.add_css_class("is-primary");
        } else {
            self.root.remove_css_class("is-primary");
        }
        self.sync_scale(&source);
    }

    fn sync_scale(&self, source: &BrightnessSource) {
        let config = scale_config(source);
        let value = scale_value(source);
        if source.kind == BrightnessSourceKind::Keyboard {
            tracing::debug!(
                id = %source.id,
                name = %source.name,
                current = source.current,
                max = source.max,
                percent = source.percent,
                slider_upper = config.upper,
                slider_value = value,
                step_increment = config.step_increment,
                page_increment = config.page_increment,
                "keyboard brightness slider range"
            );
        }
        self.updating.set(true);
        self.root.set_range(0.0, config.upper);
        self.root
            .set_increments(config.step_increment, config.page_increment);
        self.root
            .set_snap_step(uses_discrete_scale(source).then_some(config.step_increment));
        self.root.set_digits(0);
        self.root.set_value(value);
        self.updating.set(false);
    }

    fn widget(&self) -> &gtk::Widget {
        self.root.upcast_ref()
    }
}

fn sync_source_rows(
    rows: &mut HashMap<String, SourceRow>,
    container: &gtk::Box,
    sources: Vec<BrightnessSource>,
    sender: &ComponentSender<Popover>,
) {
    let mut seen = HashSet::new();
    let mut previous: Option<gtk::Widget> = None;

    for source in sources {
        seen.insert(source.id.clone());
        let row = rows
            .entry(source.id.clone())
            .or_insert_with(|| SourceRow::new(&source, sender));
        row.update(source);
        place_row(row.widget(), container, previous.as_ref());
        previous = Some(row.widget().clone());
    }

    rows.retain(|id, row| {
        let keep = seen.contains(id);
        if !keep {
            remove_row(row.widget());
        }
        keep
    });
}

fn place_row(row_widget: &gtk::Widget, container: &gtk::Box, previous: Option<&gtk::Widget>) {
    let target = container.clone().upcast::<gtk::Widget>();
    let already_in_container = row_widget.parent().is_some_and(|parent| parent == target);

    if !already_in_container {
        remove_row(row_widget);
        container.append(row_widget);
    }
    container.reorder_child_after(row_widget, previous);
}

fn remove_row(row_widget: &gtk::Widget) {
    if let Some(parent) = row_widget.parent()
        && let Ok(parent) = parent.downcast::<gtk::Box>()
    {
        parent.remove(row_widget);
    }
}

fn emit_throttled_row_command(
    id: String,
    percent: u8,
    last_sent: Rc<Cell<Instant>>,
    pending: Rc<Cell<bool>>,
    pending_percent: Rc<Cell<u8>>,
    sender: ComponentSender<Popover>,
) {
    pending_percent.set(percent);

    let now = Instant::now();
    if now.duration_since(last_sent.get()) >= ROW_COMMAND_INTERVAL {
        pending.set(false);
        last_sent.set(now);
        sender.input(PopoverInput::RowCommand(Command::SetPercent {
            id,
            percent,
        }));
        return;
    }

    if pending.get() {
        return;
    }

    pending.set(true);
    let delay = ROW_COMMAND_INTERVAL.saturating_sub(now.duration_since(last_sent.get()));
    glib::timeout_add_local_once(delay, move || {
        if !pending.get() {
            return;
        }

        pending.set(false);
        last_sent.set(Instant::now());
        sender.input(PopoverInput::RowCommand(Command::SetPercent {
            id,
            percent: pending_percent.get(),
        }));
    });
}

fn usable_sources(state: &State) -> Vec<BrightnessSource> {
    state
        .sources
        .iter()
        .filter(|source| source.is_usable())
        .cloned()
        .collect()
}

fn uses_discrete_scale(source: &BrightnessSource) -> bool {
    source.kind == BrightnessSourceKind::Keyboard || source.max <= 20
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct ScaleConfig {
    upper: f64,
    step_increment: f64,
    page_increment: f64,
}

fn scale_config(source: &BrightnessSource) -> ScaleConfig {
    if uses_discrete_scale(source) {
        ScaleConfig {
            upper: source.max.max(1) as f64,
            step_increment: 1.0,
            page_increment: 1.0,
        }
    } else {
        ScaleConfig {
            upper: 100.0,
            step_increment: 1.0,
            page_increment: 10.0,
        }
    }
}

fn scale_value(source: &BrightnessSource) -> f64 {
    if uses_discrete_scale(source) {
        source.current.min(source.max) as f64
    } else {
        source.percent as f64
    }
}

fn percent_from_scale_value(source: &BrightnessSource, value: f64) -> u8 {
    if uses_discrete_scale(source) {
        percent_from_raw_value(
            value.round().clamp(0.0, source.max.max(1) as f64) as u32,
            source.max,
        )
    } else {
        let min = match source.kind {
            BrightnessSourceKind::BuiltInDisplay | BrightnessSourceKind::ExternalDisplay => 1.0,
            BrightnessSourceKind::Keyboard | BrightnessSourceKind::Other => 0.0,
        };
        value.round().clamp(min, 100.0) as u8
    }
}

fn current_from_percent(source: &BrightnessSource, percent: u8) -> u32 {
    ((source.max as f64 * percent as f64) / 100.0)
        .round()
        .clamp(0.0, source.max as f64) as u32
}

fn should_apply_service_percent(
    pending: &mut Option<PendingBrightness>,
    service_percent: u8,
    now: Instant,
) -> bool {
    let Some(value) = pending else {
        return true;
    };

    if value.percent == service_percent {
        *pending = None;
        return true;
    }

    if now.duration_since(value.changed_at) < BRIGHTNESS_ECHO_GRACE {
        return false;
    }

    *pending = None;
    true
}

fn percent_from_raw_value(current: u32, max: u32) -> u8 {
    if max == 0 {
        return 0;
    }

    ((current as f64 / max as f64) * 100.0)
        .round()
        .clamp(0.0, 100.0) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usable_sources_keeps_all_usable_sources_in_order() {
        let state = State {
            available: true,
            sources: vec![
                source("primary", BrightnessSourceKind::BuiltInDisplay, 50, 100, 50),
                unavailable_source("unavailable"),
                source("keyboard", BrightnessSourceKind::Keyboard, 1, 3, 33),
                source(
                    "external",
                    BrightnessSourceKind::ExternalDisplay,
                    80,
                    100,
                    80,
                ),
            ],
            active: None,
        };

        let sources = usable_sources(&state);

        assert_eq!(sources.len(), 3);
        assert_eq!(sources[0].id, "primary");
        assert_eq!(sources[1].id, "keyboard");
        assert_eq!(sources[2].id, "external");
    }

    #[test]
    fn empty_state_is_used_when_no_sources_are_usable() {
        let state = State {
            available: true,
            sources: vec![unavailable_source("unavailable")],
            active: None,
        };

        assert!(usable_sources(&state).is_empty());
    }

    #[test]
    fn keyboard_source_uses_native_step_count() {
        let source = source("kbd", BrightnessSourceKind::Keyboard, 1, 3, 33);

        assert_eq!(
            scale_config(&source),
            ScaleConfig {
                upper: 3.0,
                step_increment: 1.0,
                page_increment: 1.0,
            }
        );
        assert_eq!(scale_value(&source), 1.0);
        assert_eq!(percent_from_scale_value(&source, 2.0), 67);
    }

    #[test]
    fn display_source_uses_percent_scale() {
        let source = source(
            "display",
            BrightnessSourceKind::BuiltInDisplay,
            128,
            255,
            50,
        );

        assert_eq!(
            scale_config(&source),
            ScaleConfig {
                upper: 100.0,
                step_increment: 1.0,
                page_increment: 10.0,
            }
        );
        assert_eq!(scale_value(&source), 50.0);
        assert_eq!(percent_from_scale_value(&source, 42.2), 42);
        assert_eq!(percent_from_scale_value(&source, 0.0), 1);
    }

    #[test]
    fn external_display_source_uses_one_percent_floor() {
        let source = source(
            "external",
            BrightnessSourceKind::ExternalDisplay,
            50,
            100,
            50,
        );

        assert_eq!(percent_from_scale_value(&source, 0.0), 1);
    }

    #[test]
    fn pending_brightness_ignores_recent_stale_service_values() {
        let mut pending = Some(PendingBrightness::new(80));
        let now = Instant::now();

        assert!(!should_apply_service_percent(&mut pending, 40, now));
        assert!(pending.is_some());
    }

    #[test]
    fn pending_brightness_clears_when_service_catches_up() {
        let mut pending = Some(PendingBrightness::new(80));
        let now = Instant::now();

        assert!(should_apply_service_percent(&mut pending, 80, now));
        assert!(pending.is_none());
    }

    #[test]
    fn monitor_display_name_prefers_builtin_then_make_model_then_connector() {
        let mut external = monitor("DP-2", true, false);
        external.make = Some("Dell Inc.".into());
        external.model = Some("AW2725Q".into());

        assert_eq!(
            format::monitor_display_name(&monitor("eDP-1", true, true)),
            "Built-in display"
        );
        assert_eq!(format::monitor_display_name(&external), "Dell Inc. AW2725Q");
        assert_eq!(
            format::monitor_display_name(&monitor("HDMI-A-1", true, false)),
            "HDMI-A-1"
        );
    }

    #[test]
    fn monitor_display_name_deduplicates_make_when_model_already_contains_it() {
        let mut external = monitor("DP-2", true, false);
        external.make = Some("Dell Inc.".into());
        external.model = Some("Dell Inc. U2723QE".into());

        assert_eq!(format::monitor_display_name(&external), "Dell Inc. U2723QE");
    }

    #[test]
    fn display_source_name_uses_matching_monitor_name() {
        let mut external = monitor("DP-2", true, false);
        external.make = Some("Dell Inc.".into());
        external.model = Some("AW2725Q".into());
        let source = source_on_connector(
            "ddcutil:1",
            BrightnessSourceKind::ExternalDisplay,
            50,
            100,
            50,
            "DP-2",
        );

        assert_eq!(
            format::source_display_name(&source, &[external]),
            "Dell Inc. AW2725Q"
        );
    }

    #[test]
    fn hero_subtitle_uses_matching_monitor_name() {
        let mut external = monitor("DP-2", true, false);
        external.make = Some("Dell Inc.".into());
        external.model = Some("AW2725Q".into());
        let mut source = source_on_connector(
            "ddcutil:1",
            BrightnessSourceKind::ExternalDisplay,
            50,
            100,
            50,
            "DP-2",
        );
        source.primary = true;
        let state = State {
            available: true,
            sources: vec![source],
            active: None,
        };

        assert_eq!(
            format::hero_subtitle_with_monitors(&state, &[external]),
            "Dell Inc. AW2725Q"
        );
    }

    fn source(
        id: &str,
        kind: BrightnessSourceKind,
        current: u32,
        max: u32,
        percent: u8,
    ) -> BrightnessSource {
        BrightnessSource {
            id: id.into(),
            name: id.into(),
            connector: None,
            kind,
            icon: "display-brightness-symbolic".into(),
            current,
            max,
            percent,
            writable: true,
            primary: false,
            available: true,
        }
    }

    fn source_on_connector(
        id: &str,
        kind: BrightnessSourceKind,
        current: u32,
        max: u32,
        percent: u8,
        connector: &str,
    ) -> BrightnessSource {
        let mut source = source(id, kind, current, max, percent);
        source.connector = Some(connector.into());
        source
    }

    fn unavailable_source(id: &str) -> BrightnessSource {
        BrightnessSource {
            writable: false,
            available: false,
            ..source(id, BrightnessSourceKind::BuiltInDisplay, 0, 100, 0)
        }
    }

    fn monitor(name: &str, enabled: bool, built_in: bool) -> Monitor {
        Monitor {
            id: None,
            name: name.into(),
            description: None,
            active_workspace: None,
            focused: false,
            make: None,
            model: None,
            enabled,
            built_in,
            current_mode: None,
        }
    }
}
