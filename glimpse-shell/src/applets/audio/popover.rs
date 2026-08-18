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
    services::audio::{AudioDevice, AudioStream, Command, State, volume_icon},
    widgets::{
        animated_popover::AnimatedPopover, choice_tile::ChoiceTile, expander_tile::ExpanderTile,
        hero::Hero, popover_shell::PopoverShell, slider_tile::SliderTile, tile::Tile,
    },
};

const VOLUME_ECHO_GRACE: Duration = Duration::from_secs(2);
const VOLUME_COMMAND_INTERVAL: Duration = Duration::from_millis(50);

pub struct Popover {
    popover: AnimatedPopover,
    state: State,
    max_volume: u32,
    show_streams: bool,
    outputs_expanded: bool,
    inputs_expanded: bool,
    streams_expanded: bool,
    pending_output_volume: Rc<RefCell<Option<PendingVolume>>>,
    pending_input_volume: Rc<RefCell<Option<PendingVolume>>>,
    updating_output_scale: Rc<Cell<bool>>,
    updating_input_scale: Rc<Cell<bool>>,
    output_mute: gtk::Button,
    input_mute: gtk::Button,
    output_volume: SliderTile,
    input_volume: SliderTile,
    outputs_list: gtk::Box,
    inputs_list: gtk::Box,
    streams_list: gtk::Box,
    output_rows: HashMap<String, DeviceRow>,
    input_rows: HashMap<String, DeviceRow>,
    stream_rows: HashMap<u64, StreamRow>,
}

#[derive(Debug, Clone)]
struct PendingVolume {
    value: u32,
    changed_at: Instant,
}

impl PendingVolume {
    fn new(value: u32) -> Self {
        Self {
            value,
            changed_at: Instant::now(),
        }
    }
}

pub struct PopoverInit {
    pub parent: gtk::Box,
    pub max_volume: u32,
    pub show_streams: bool,
}

#[derive(Debug)]
pub enum PopoverInput {
    Toggle,
    UpdateState(State),
    Reconfigure { max_volume: u32, show_streams: bool },
    SetOutputsExpanded(bool),
    SetInputsExpanded(bool),
    SetStreamsExpanded(bool),
    Command(Command),
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
                    set_icon: Some(output_icon_name(&model.state)),
                    set_title: "Audio",
                    #[watch]
                    set_subtitle: &hero_subtitle(&model.state),
                },

                #[name = "output_volume"]
                SliderTile {
                    set_increments: (1.0, 5.0),
                    #[watch]
                    set_sensitive: model.state.default_output().is_some(),
                },

                #[name = "input_volume"]
                SliderTile {
                    set_increments: (1.0, 5.0),
                    #[watch]
                    set_sensitive: model.state.default_input().is_some(),
                },

                gtk::Separator {
                    set_orientation: gtk::Orientation::Horizontal,
                },

                #[name = "outputs_section"]
                ExpanderTile {
                    add_css_class: "audio-device-section",
                    #[watch]
                    set_primary: "Output devices",
                    #[watch]
                    set_visible: !model.output_rows.is_empty(),
                    #[watch]
                    set_expanded: model.outputs_expanded,

                    connect_expanded[sender] => move |_, expanded| {
                        sender.input(PopoverInput::SetOutputsExpanded(expanded));
                    },
                },

                #[name = "inputs_section"]
                ExpanderTile {
                    add_css_class: "audio-device-section",
                    #[watch]
                    set_primary: "Input devices",
                    #[watch]
                    set_visible: !model.input_rows.is_empty(),
                    #[watch]
                    set_expanded: model.inputs_expanded,

                    connect_expanded[sender] => move |_, expanded| {
                        sender.input(PopoverInput::SetInputsExpanded(expanded));
                    },
                },

                #[name = "streams_section"]
                ExpanderTile {
                    add_css_class: "audio-device-section",
                    set_primary: "Apps",
                    #[watch]
                    set_visible: model.show_streams && !model.stream_rows.is_empty(),
                    #[watch]
                    set_expanded: model.streams_expanded,

                    connect_expanded[sender] => move |_, expanded| {
                        sender.input(PopoverInput::SetStreamsExpanded(expanded));
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
        let updating_output_scale = Rc::new(Cell::new(false));
        let updating_input_scale = Rc::new(Cell::new(false));
        let mut model = Popover {
            popover: AnimatedPopover::new(),
            state: State::default(),
            max_volume: init.max_volume,
            show_streams: init.show_streams,
            outputs_expanded: false,
            inputs_expanded: false,
            streams_expanded: false,
            pending_output_volume: Rc::new(RefCell::new(None)),
            pending_input_volume: Rc::new(RefCell::new(None)),
            updating_output_scale,
            updating_input_scale,
            output_mute: gtk::Button::new(),
            input_mute: gtk::Button::new(),
            output_volume: SliderTile::new(),
            input_volume: SliderTile::new(),
            outputs_list: gtk::Box::new(gtk::Orientation::Vertical, 2),
            inputs_list: gtk::Box::new(gtk::Orientation::Vertical, 2),
            streams_list: gtk::Box::new(gtk::Orientation::Vertical, 2),
            output_rows: HashMap::new(),
            input_rows: HashMap::new(),
            stream_rows: HashMap::new(),
        };

        let widgets = view_output!();
        model.popover = widgets.root.clone();
        model.output_volume = widgets.output_volume.clone();
        model.input_volume = widgets.input_volume.clone();

        widgets.root.set_parent(&init.parent);
        model.output_mute.add_css_class("flat");
        model.output_mute.set_focusable(false);
        model.output_mute.connect_clicked({
            let sender = sender.clone();
            move |_| sender.input(PopoverInput::Command(Command::ToggleOutputMute))
        });
        widgets
            .output_volume
            .set_left(Some(model.output_mute.clone()));

        model.input_mute.add_css_class("flat");
        model.input_mute.set_focusable(false);
        model.input_mute.connect_clicked({
            let sender = sender.clone();
            move |_| sender.input(PopoverInput::Command(Command::ToggleInputMute))
        });
        widgets
            .input_volume
            .set_left(Some(model.input_mute.clone()));

        widgets
            .outputs_section
            .set_child(Some(model.outputs_list.clone()));
        widgets
            .inputs_section
            .set_child(Some(model.inputs_list.clone()));
        widgets
            .streams_section
            .set_child(Some(model.streams_list.clone()));

        connect_throttled_slider(
            &widgets.output_volume,
            model.updating_output_scale.clone(),
            model.pending_output_volume.clone(),
            sender.clone(),
            Command::SetOutputVolume,
        );
        connect_throttled_slider(
            &widgets.input_volume,
            model.updating_input_scale.clone(),
            model.pending_input_volume.clone(),
            sender.clone(),
            Command::SetInputVolume,
        );

        model.sync_volume_ranges();
        model.sync_volume_values();
        model.sync_rows(&sender);

        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>) {
        match message {
            PopoverInput::Toggle => {
                self.popover.toggle();
            }
            PopoverInput::UpdateState(state) => {
                self.state = state;
                self.sync_volume_values();
                self.sync_rows(&sender);
            }
            PopoverInput::Reconfigure {
                max_volume,
                show_streams,
            } => {
                self.max_volume = max_volume;
                self.show_streams = show_streams;
                self.sync_volume_ranges();
            }
            PopoverInput::SetOutputsExpanded(expanded) => {
                self.outputs_expanded = expanded;
            }
            PopoverInput::SetInputsExpanded(expanded) => {
                self.inputs_expanded = expanded;
            }
            PopoverInput::SetStreamsExpanded(expanded) => {
                self.streams_expanded = expanded;
            }
            PopoverInput::Command(command) => {
                match &command {
                    Command::SetOutputVolume(volume) => {
                        self.pending_output_volume
                            .replace(Some(PendingVolume::new(*volume)));
                    }
                    Command::SetInputVolume(volume) => {
                        self.pending_input_volume
                            .replace(Some(PendingVolume::new(*volume)));
                    }
                    _ => {}
                }
                let _ = sender.output(PopoverOutput::Command(command));
            }
        }
    }
}

impl Popover {
    /// Applies max_volume to both sliders under the same updating_*_scale
    /// guard set_value uses. Shrinking the range below the slider's current
    /// value makes GTK clamp the adjustment and fire value-changed; without
    /// the guard, the throttled-slider handler would mistake that clamp for
    /// a user edit and write it back to PipeWire.
    fn sync_volume_ranges(&self) {
        self.updating_output_scale.set(true);
        self.output_volume.set_range(0.0, self.max_volume as f64);
        self.updating_output_scale.set(false);

        self.updating_input_scale.set(true);
        self.input_volume.set_range(0.0, self.max_volume as f64);
        self.updating_input_scale.set(false);
    }

    fn sync_volume_values(&self) {
        let output = self.state.default_output();
        let input = self.state.default_input();

        self.output_mute
            .set_icon_name(output_icon_name(&self.state));
        self.output_mute.set_sensitive(output.is_some());
        self.input_mute.set_icon_name(input_icon_name(input));
        self.input_mute.set_sensitive(input.is_some());

        let now = Instant::now();
        if let Some(device) = output {
            let should_apply = {
                let mut pending = self.pending_output_volume.borrow_mut();
                should_apply_service_volume(&mut pending, device.volume, now)
            };
            if should_apply {
                self.updating_output_scale.set(true);
                self.output_volume.set_value(device.volume as f64);
                self.updating_output_scale.set(false);
            }
        }

        if let Some(device) = input {
            let should_apply = {
                let mut pending = self.pending_input_volume.borrow_mut();
                should_apply_service_volume(&mut pending, device.volume, now)
            };
            if should_apply {
                self.updating_input_scale.set(true);
                self.input_volume.set_value(device.volume as f64);
                self.updating_input_scale.set(false);
            }
        }
    }

    fn sync_rows(&mut self, sender: &ComponentSender<Self>) {
        sync_device_rows(
            &mut self.output_rows,
            &self.outputs_list,
            output_row_models(&self.state.outputs),
            sender,
        );
        sync_device_rows(
            &mut self.input_rows,
            &self.inputs_list,
            input_row_models(&self.state.inputs),
            sender,
        );
        sync_stream_rows(
            &mut self.stream_rows,
            &self.streams_list,
            stream_row_models(&self.state.streams),
            sender,
        );
    }
}

fn should_apply_service_volume(
    pending: &mut Option<PendingVolume>,
    service_volume: u32,
    now: Instant,
) -> bool {
    let Some(value) = pending else {
        return true;
    };

    if value.value == service_volume {
        *pending = None;
        return true;
    }

    if now.duration_since(value.changed_at) < VOLUME_ECHO_GRACE {
        return false;
    }

    *pending = None;
    true
}

fn connect_throttled_slider(
    slider: &SliderTile,
    updating: Rc<Cell<bool>>,
    pending_volume: Rc<RefCell<Option<PendingVolume>>>,
    sender: ComponentSender<Popover>,
    make_command: fn(u32) -> Command,
) {
    let last_sent = Rc::new(Cell::new(Instant::now() - VOLUME_COMMAND_INTERVAL));
    let pending = Rc::new(Cell::new(false));
    let pending_value = Rc::new(Cell::new(0));

    slider.connect_changed(move |_, value| {
        if updating.get() {
            return;
        }

        let volume = volume_from_scale_value(value);
        pending_value.set(volume);
        pending_volume
            .borrow_mut()
            .replace(PendingVolume::new(volume));

        let now = Instant::now();
        if now.duration_since(last_sent.get()) >= VOLUME_COMMAND_INTERVAL {
            last_sent.set(now);
            pending.set(false);
            sender.input(PopoverInput::Command(make_command(volume)));
        } else if !pending.get() {
            pending.set(true);
            let last_sent = last_sent.clone();
            let pending = pending.clone();
            let pending_value = pending_value.clone();
            let sender = sender.input_sender().clone();
            let delay = VOLUME_COMMAND_INTERVAL.saturating_sub(now.duration_since(last_sent.get()));
            glib::timeout_add_local_once(delay, move || {
                if pending.get() {
                    pending.set(false);
                    last_sent.set(Instant::now());
                    let _ = sender.send(PopoverInput::Command(make_command(pending_value.get())));
                }
            });
        }
    });
}

fn volume_from_scale_value(value: f64) -> u32 {
    value.round().clamp(0.0, u32::MAX as f64) as u32
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DeviceRowModel {
    id: String,
    icon: String,
    label: String,
    tooltip: String,
    selected: bool,
    command: Command,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct StreamRowModel {
    id: u64,
    icon: String,
    label: String,
    status: String,
    tooltip: String,
    command: Command,
}

fn output_row_models(devices: &[AudioDevice]) -> Vec<DeviceRowModel> {
    device_row_models(devices, |device| {
        Command::SetDefaultOutput(device.name.clone())
    })
}

fn input_row_models(devices: &[AudioDevice]) -> Vec<DeviceRowModel> {
    device_row_models(devices, |device| {
        Command::SetDefaultInput(device.name.clone())
    })
}

fn device_row_models(
    devices: &[AudioDevice],
    make_command: impl Fn(&AudioDevice) -> Command,
) -> Vec<DeviceRowModel> {
    devices
        .iter()
        .map(|device| DeviceRowModel {
            id: device.name.clone(),
            icon: device.icon_name.clone(),
            label: device_label(&device.description),
            tooltip: device_tooltip(device),
            selected: device.is_default,
            command: make_command(device),
        })
        .collect()
}

fn stream_row_models(streams: &[AudioStream]) -> Vec<StreamRowModel> {
    streams
        .iter()
        .map(|stream| StreamRowModel {
            id: stream.index,
            icon: stream.app_icon.clone(),
            label: stream.app_name.clone(),
            status: stream_status(stream),
            tooltip: format!("{}%", stream.volume),
            command: Command::ToggleStreamMute(stream.index),
        })
        .collect()
}

struct DeviceRow {
    root: ChoiceTile,
    icon: gtk::Image,
}

impl DeviceRow {
    fn new(model: &DeviceRowModel, sender: &ComponentSender<Popover>) -> Self {
        let root = ChoiceTile::new();
        root.add_css_class("audio-device-row");

        let icon = gtk::Image::from_icon_name(&model.icon);
        icon.add_css_class("audio-device-row__icon");
        icon.set_pixel_size(16);
        root.set_left(Some(icon.clone()));
        root.set_secondary(None);

        root.connect_activated({
            let sender = sender.clone();
            let command = model.command.clone();
            move |_| sender.input(PopoverInput::Command(command.clone()))
        });

        let row = Self { root, icon };
        row.update(model);
        row
    }

    fn update(&self, model: &DeviceRowModel) {
        self.icon.set_icon_name(Some(&model.icon));
        self.root.set_primary(&model.label);
        self.root.set_selected(model.selected);
        self.root.set_tooltip_text(Some(&model.tooltip));
    }

    fn widget(&self) -> &gtk::Widget {
        self.root.upcast_ref()
    }
}

struct StreamRow {
    root: Tile,
    icon: gtk::Image,
    status: gtk::Label,
}

impl StreamRow {
    fn new(model: &StreamRowModel, sender: &ComponentSender<Popover>) -> Self {
        let root = Tile::new();
        root.add_css_class("audio-stream-row");

        let icon = gtk::Image::from_icon_name(&model.icon);
        icon.add_css_class("audio-stream-row__icon");
        icon.set_pixel_size(16);
        root.set_left(Some(icon.clone()));
        root.set_secondary(None);

        let status = gtk::Label::new(None);
        status.add_css_class("dim-label");
        status.add_css_class("caption");
        status.set_valign(gtk::Align::Center);
        root.set_right(Some(status.clone()));

        root.connect_activated({
            let sender = sender.clone();
            let command = model.command.clone();
            move |_| sender.input(PopoverInput::Command(command.clone()))
        });

        let row = Self { root, icon, status };
        row.update(model);
        row
    }

    fn update(&self, model: &StreamRowModel) {
        self.icon.set_icon_name(Some(&model.icon));
        self.root.set_primary(&model.label);
        self.status.set_label(&model.status);
        self.root.set_tooltip_text(Some(&model.tooltip));
    }

    fn widget(&self) -> &gtk::Widget {
        self.root.upcast_ref()
    }
}

fn sync_device_rows(
    rows: &mut HashMap<String, DeviceRow>,
    container: &gtk::Box,
    models: Vec<DeviceRowModel>,
    sender: &ComponentSender<Popover>,
) {
    let mut seen = HashSet::new();
    let mut previous: Option<gtk::Widget> = None;

    for model in models {
        seen.insert(model.id.clone());
        let row = rows
            .entry(model.id.clone())
            .or_insert_with(|| DeviceRow::new(&model, sender));
        row.update(&model);
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

fn sync_stream_rows(
    rows: &mut HashMap<u64, StreamRow>,
    container: &gtk::Box,
    models: Vec<StreamRowModel>,
    sender: &ComponentSender<Popover>,
) {
    let mut seen = HashSet::new();
    let mut previous: Option<gtk::Widget> = None;

    for model in models {
        seen.insert(model.id);
        let row = rows
            .entry(model.id)
            .or_insert_with(|| StreamRow::new(&model, sender));
        row.update(&model);
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

fn hero_subtitle(state: &State) -> String {
    if !state.available {
        return "Unavailable".into();
    }

    state
        .default_output()
        .map(|device| device.description.clone())
        .unwrap_or_else(|| "No output device".into())
}

fn output_icon_name(state: &State) -> &'static str {
    state
        .default_output()
        .map(|device| volume_icon(device.volume, device.muted))
        .unwrap_or("audio-volume-muted-symbolic")
}

fn device_label(description: &str) -> String {
    description.to_owned()
}

fn input_icon_name(device: Option<&AudioDevice>) -> &'static str {
    match device {
        Some(device) if device.muted => "microphone-sensitivity-muted-symbolic",
        _ => "audio-input-microphone-symbolic",
    }
}

fn device_tooltip(device: &AudioDevice) -> String {
    if device.muted {
        format!("{} muted", device.description)
    } else {
        format!("{} {}%", device.description, device.volume)
    }
}

fn stream_status(stream: &AudioStream) -> String {
    if stream.muted {
        "Muted".into()
    } else {
        format!("{}%", stream.volume)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_rows_map_devices_to_default_output_commands() {
        let rows = output_row_models(&[AudioDevice {
            index: 1,
            name: "sink".into(),
            description: "Speakers".into(),
            volume: 70,
            muted: false,
            is_default: true,
            icon_name: "audio-speakers-symbolic".into(),
        }]);

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].label, "Speakers");
        assert!(rows[0].selected);
        assert_eq!(rows[0].command, Command::SetDefaultOutput("sink".into()));
    }

    #[test]
    fn output_and_input_device_labels_are_not_truncated() {
        let long_description = "123456789012345678901234567890EXTRA";
        let output_rows = output_row_models(&[AudioDevice {
            index: 1,
            name: "sink".into(),
            description: long_description.into(),
            volume: 70,
            muted: false,
            is_default: true,
            icon_name: "audio-speakers-symbolic".into(),
        }]);
        let input_rows = input_row_models(&[AudioDevice {
            index: 2,
            name: "source".into(),
            description: long_description.into(),
            volume: 55,
            muted: false,
            is_default: true,
            icon_name: "audio-input-microphone-symbolic".into(),
        }]);

        assert_eq!(output_rows[0].label, long_description);
        assert_eq!(input_rows[0].label, long_description);
    }

    #[test]
    fn stream_rows_toggle_stream_mute_on_click() {
        let rows = stream_row_models(&[AudioStream {
            index: 7,
            app_name: "Firefox".into(),
            app_icon: "firefox-symbolic".into(),
            volume: 43,
            muted: false,
        }]);

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].label, "Firefox");
        assert_eq!(rows[0].status, "43%");
        assert_eq!(rows[0].command, Command::ToggleStreamMute(7));
    }

    #[test]
    fn pending_volume_ignores_recent_stale_service_values() {
        let now = Instant::now();
        let mut pending = Some(PendingVolume {
            value: 80,
            changed_at: now,
        });

        assert!(!should_apply_service_volume(&mut pending, 40, now));
        assert!(pending.is_some());
    }

    #[test]
    fn pending_volume_clears_when_service_catches_up() {
        let now = Instant::now();
        let mut pending = Some(PendingVolume {
            value: 80,
            changed_at: now,
        });

        assert!(should_apply_service_volume(&mut pending, 80, now));
        assert!(pending.is_none());
    }

    #[test]
    fn pending_volume_expires_if_service_never_catches_up() {
        let now = Instant::now();
        let mut pending = Some(PendingVolume {
            value: 80,
            changed_at: now - VOLUME_ECHO_GRACE - Duration::from_millis(1),
        });

        assert!(should_apply_service_volume(&mut pending, 40, now));
        assert!(pending.is_none());
    }

    #[test]
    fn scale_values_are_normalized_before_commands() {
        assert_eq!(volume_from_scale_value(-1.0), 0);
        assert_eq!(volume_from_scale_value(40.4), 40);
        assert_eq!(volume_from_scale_value(40.6), 41);
    }
}
