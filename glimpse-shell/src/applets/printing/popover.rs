use std::collections::HashMap;

use relm4::{
    ComponentParts, ComponentSender, SimpleComponent,
    gtk::{self, prelude::*},
};

use glimpse_core::services::printing::{Command, InkMarker, PrintJob, State};

use crate::{
    utils::popover_scroll,
    widgets::{
        animated_popover::AnimatedPopover, empty_state::EmptyState, expander_tile::ExpanderTile,
        hero::Hero, popover_shell::PopoverShell, segmented_tile::SegmentedTile, tile::Tile,
    },
};

use super::format;

// ── Public interface ──────────────────────────────────────────────────────────

pub struct Popover {
    popover: AnimatedPopover,
    hero_subtitle: String,
    error_box: gtk::Box,
    job_list: gtk::Box,
    job_rows: HashMap<u32, JobRow>,
    empty_state: EmptyState,
    ink_box: gtk::Box,
    printers_expander: ExpanderTile,
}

#[derive(Debug)]
pub struct PopoverInit {
    pub parent: gtk::Box,
}

#[derive(Debug)]
pub enum PopoverInput {
    Toggle,
    UpdateState(State),
    JobCommand(Command),
    OpenQueue(String),
}

#[derive(Debug, Clone)]
pub enum PopoverOutput {
    Command(Command),
    OpenQueue(String),
}

// ── Component ─────────────────────────────────────────────────────────────────

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
                Hero {
                    set_icon: Some("printer-symbolic"),
                    set_title: "Printers",
                    #[watch]
                    set_subtitle: &model.hero_subtitle,
                },

                #[local_ref]
                error_box -> gtk::Box {},

                #[name = "scroller"]
                gtk::ScrolledWindow {
                    set_policy: (gtk::PolicyType::Never, gtk::PolicyType::Automatic),
                    set_vexpand: false,
                    set_propagate_natural_height: true,

                    gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,
                        set_spacing: 2,

                        #[local_ref]
                        job_list -> gtk::Box {},

                        #[local_ref]
                        empty_state -> EmptyState {},

                        #[local_ref]
                        ink_box -> gtk::Box {},
                    }
                },

                gtk::Separator {
                    set_orientation: gtk::Orientation::Horizontal,
                    add_css_class: "spacer",
                },

                #[local_ref]
                printers_expander -> ExpanderTile {},
            },
        }
    }

    fn init(
        init: Self::Init,
        _root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let error_box = gtk::Box::new(gtk::Orientation::Vertical, 2);
        let job_list = gtk::Box::new(gtk::Orientation::Vertical, 2);
        let ink_box = gtk::Box::new(gtk::Orientation::Vertical, 4);

        let empty_state = EmptyState::new();
        empty_state.set_title("No print jobs");
        empty_state.set_subtitle(Some("Nothing is printing right now"));

        let printers_expander = ExpanderTile::new();
        printers_expander.set_primary("Printers");

        let mut model = Popover {
            popover: AnimatedPopover::new(),
            hero_subtitle: "No print jobs".into(),
            error_box: error_box.clone(),
            job_list: job_list.clone(),
            job_rows: HashMap::new(),
            empty_state: empty_state.clone(),
            ink_box: ink_box.clone(),
            printers_expander: printers_expander.clone(),
        };

        let widgets = view_output!();
        model.popover = widgets.root.clone();
        widgets.root.set_parent(&init.parent);
        popover_scroll::install_half_monitor_limit(
            widgets.root.upcast_ref(),
            &widgets.scroller,
            &init.parent,
        );

        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>) {
        match message {
            PopoverInput::Toggle => self.popover.toggle(),
            PopoverInput::UpdateState(state) => self.apply_state(state, &sender),
            PopoverInput::JobCommand(cmd) => {
                let _ = sender.output(PopoverOutput::Command(cmd));
            }
            PopoverInput::OpenQueue(name) => {
                let _ = sender.output(PopoverOutput::OpenQueue(name));
            }
        }
    }
}

impl Popover {
    fn apply_state(&mut self, state: State, sender: &ComponentSender<Self>) {
        self.hero_subtitle = match state.jobs.len() {
            0 => "No print jobs".into(),
            1 => "1 job active".into(),
            n => format!("{n} jobs active"),
        };

        self.sync_error_banners(&state);
        self.sync_job_rows(&state, sender);
        self.sync_ink_section(&state);
        self.sync_printers_expander(&state, sender);
    }

    fn sync_error_banners(&self, state: &State) {
        while let Some(child) = self.error_box.first_child() {
            self.error_box.remove(&child);
        }
        for printer in &state.printers {
            for reason in &printer.state_reasons {
                let tile = Tile::new();
                tile.add_css_class("error-banner");
                tile.set_primary(&printer.name);
                tile.set_secondary(Some(format::state_reason_text(reason).as_str()));
                tile.set_activatable(false);
                self.error_box.append(&tile);
            }
        }
        self.error_box
            .set_visible(self.error_box.first_child().is_some());
    }

    fn sync_job_rows(&mut self, state: &State, sender: &ComponentSender<Self>) {
        let mut seen = std::collections::HashSet::new();
        let mut previous: Option<gtk::Widget> = None;

        for job in &state.jobs {
            seen.insert(job.id);
            let row = self
                .job_rows
                .entry(job.id)
                .or_insert_with(|| JobRow::new(job, sender));
            row.update(job, sender);
            place_widget(row.widget(), &self.job_list, previous.as_ref());
            previous = Some(row.widget().clone());
        }

        self.job_rows.retain(|id, row| {
            let keep = seen.contains(id);
            if !keep {
                remove_widget(row.widget(), &self.job_list);
            }
            keep
        });

        self.job_list.set_visible(!state.jobs.is_empty());
        self.empty_state.set_visible(state.jobs.is_empty());
    }

    fn sync_printers_expander(&self, state: &State, sender: &ComponentSender<Self>) {
        let children = gtk::Box::new(gtk::Orientation::Vertical, 2);

        for printer in &state.printers {
            let tile = Tile::new();
            tile.set_primary(&printer.name);
            tile.set_secondary(Some(format::printer_state_text(&printer.state)));
            let name = printer.name.clone();
            tile.connect_activated({
                let sender = sender.clone();
                move |_| sender.input(PopoverInput::OpenQueue(name.clone()))
            });
            children.append(&tile);
        }

        self.printers_expander
            .set_visible(!state.printers.is_empty());
        if state.printers.is_empty() {
            self.printers_expander.set_child(None::<gtk::Widget>);
        } else {
            self.printers_expander.set_child(Some(children));
        }
    }

    fn sync_ink_section(&self, state: &State) {
        while let Some(child) = self.ink_box.first_child() {
            self.ink_box.remove(&child);
        }

        let printers_with_markers: Vec<_> = state
            .printers
            .iter()
            .filter(|p| p.markers.iter().any(|m| m.level >= 0))
            .collect();

        if printers_with_markers.is_empty() {
            self.ink_box.set_visible(false);
            return;
        }

        let header = gtk::Label::new(Some("Ink & Toner"));
        header.add_css_class("caption");
        header.add_css_class("dim-label");
        header.set_xalign(0.0);
        self.ink_box.append(&header);

        for printer in &printers_with_markers {
            if state.printers.len() > 1 {
                let printer_label = gtk::Label::new(Some(&printer.name));
                printer_label.add_css_class("caption");
                printer_label.set_xalign(0.0);
                self.ink_box.append(&printer_label);
            }
            for marker in printer.markers.iter().filter(|m| m.level >= 0) {
                self.ink_box.append(&build_ink_row(marker));
            }
        }

        self.ink_box.set_visible(true);
    }
}

// ── Job rows ──────────────────────────────────────────────────────────────────

struct JobRow {
    root: SegmentedTile,
    status_box: gtk::Box,
    status_spinner: gtk::Spinner,
    status_label: gtk::Label,
    actions: gtk::Box,
}

impl JobRow {
    fn new(job: &PrintJob, sender: &ComponentSender<Popover>) -> Self {
        let root = SegmentedTile::new();
        root.add_css_class("print-job-row");
        root.set_activatable(false);

        let icon = gtk::Image::from_icon_name("document-print-symbolic");
        icon.set_pixel_size(16);
        icon.set_valign(gtk::Align::Center);
        root.set_left(Some(icon));

        let status_box = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        status_box.set_valign(gtk::Align::Center);

        let status_spinner = gtk::Spinner::new();
        status_spinner.set_visible(false);
        status_box.append(&status_spinner);

        let status_label = gtk::Label::new(None);
        status_label.add_css_class("dim-label");
        status_label.add_css_class("caption");
        status_label.set_visible(false);
        status_box.append(&status_label);

        let actions = gtk::Box::new(gtk::Orientation::Vertical, 2);

        let row = Self {
            root,
            status_box,
            status_spinner,
            status_label,
            actions,
        };
        row.update(job, sender);
        row
    }

    fn update(&self, job: &PrintJob, sender: &ComponentSender<Popover>) {
        use glimpse_core::services::printing::JobState;

        self.root.set_primary(&job.name);
        self.root.set_secondary(Some(&format!(
            "{} · {}",
            job.printer_name,
            format::job_state_text(&job.state)
        )));

        let progress = format::page_progress(job);
        let is_printing = job.state == JobState::Processing;

        if is_printing && progress.is_none() {
            self.status_spinner.set_visible(true);
            self.status_spinner.set_spinning(true);
            self.status_label.set_visible(false);
            self.root.set_right(Some(self.status_box.clone()));
        } else if let Some(text) = progress {
            self.status_spinner.set_visible(false);
            self.status_spinner.set_spinning(false);
            self.status_label.set_text(&text);
            self.status_label.set_visible(true);
            self.root.set_right(Some(self.status_box.clone()));
        } else {
            self.root.set_right(None::<gtk::Widget>);
        }

        while let Some(child) = self.actions.first_child() {
            self.actions.remove(&child);
        }

        if job.state == JobState::Processing || job.state == JobState::Pending {
            let pause = Tile::new();
            pause.set_primary("Pause");
            let id = job.id;
            pause.connect_activated({
                let sender = sender.clone();
                move |_| sender.input(PopoverInput::JobCommand(Command::PauseJob { id }))
            });
            self.actions.append(&pause);
        }

        if job.state == JobState::Held {
            let resume = Tile::new();
            resume.set_primary("Resume");
            let id = job.id;
            resume.connect_activated({
                let sender = sender.clone();
                move |_| sender.input(PopoverInput::JobCommand(Command::ResumeJob { id }))
            });
            self.actions.append(&resume);
        }

        use glimpse_core::services::printing::JobState::*;
        if matches!(job.state, Pending | Processing | Held) {
            let cancel = Tile::new();
            cancel.set_primary("Cancel");
            cancel.add_css_class("destructive-action");
            let id = job.id;
            cancel.connect_activated({
                let sender = sender.clone();
                move |_| sender.input(PopoverInput::JobCommand(Command::CancelJob { id }))
            });
            self.actions.append(&cancel);
        }

        if self.actions.first_child().is_some() {
            self.root.set_child(Some(self.actions.clone()));
        } else {
            self.root.set_child(None::<gtk::Widget>);
        }
    }

    fn widget(&self) -> &gtk::Widget {
        self.root.upcast_ref()
    }
}

// ── Ink bar ───────────────────────────────────────────────────────────────────

fn build_ink_row(marker: &InkMarker) -> gtk::Box {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    row.set_valign(gtk::Align::Center);

    let name_label = gtk::Label::new(Some(&marker.name));
    name_label.set_xalign(0.0);
    name_label.set_hexpand(true);
    name_label.add_css_class("caption");
    row.append(&name_label);

    let level = marker.level.clamp(0, 100) as f64 / 100.0;

    let bar_container = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    bar_container.set_size_request(80, 6);
    bar_container.add_css_class("ink-level-track");

    let fill = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    fill.set_hexpand(false);
    fill.set_size_request((80.0 * level) as i32, 6);
    let css_class = if level > 0.25 {
        "ink-level-fill--ok"
    } else if level > 0.10 {
        "ink-level-fill--low"
    } else {
        "ink-level-fill--critical"
    };
    fill.add_css_class("ink-level-fill");
    fill.add_css_class(css_class);
    bar_container.append(&fill);
    row.append(&bar_container);

    let pct_label = gtk::Label::new(Some(&format!("{}%", marker.level)));
    pct_label.add_css_class("caption");
    pct_label.add_css_class("dim-label");
    row.append(&pct_label);

    row
}

// ── Layout helpers ────────────────────────────────────────────────────────────

fn place_widget(widget: &gtk::Widget, container: &gtk::Box, previous: Option<&gtk::Widget>) {
    let target = container.clone().upcast::<gtk::Widget>();
    if widget.parent().is_some_and(|p| p == target) {
        container.reorder_child_after(widget, previous);
    } else {
        remove_widget(widget, container);
        container.insert_child_after(widget, previous);
    }
}

fn remove_widget(widget: &gtk::Widget, container: &gtk::Box) {
    if let Some(parent) = widget.parent()
        && let Ok(b) = parent.downcast::<gtk::Box>()
        && b == *container
    {
        container.remove(widget);
    }
}
