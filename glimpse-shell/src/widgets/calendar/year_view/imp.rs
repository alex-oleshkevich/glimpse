use std::cell::{Cell, RefCell};
use std::sync::OnceLock;

use chrono::{Datelike, Local};
use glib::subclass::Signal;
use gtk4::{CompositeTemplate, TemplateChild, glib, prelude::*, subclass::prelude::*};

const MONTH_LABELS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

#[derive(CompositeTemplate)]
#[template(resource = "/me/aresa/GlimpseShell/widgets/calendar_year_view.ui")]
pub struct YearView {
    #[template_child]
    grid: TemplateChild<gtk4::Grid>,
    month_buttons: RefCell<Vec<gtk4::Button>>,
    pub(super) visible_year: Cell<i32>,
    current_month: Cell<u32>,
}

impl Default for YearView {
    fn default() -> Self {
        let today = Local::now().date_naive();
        Self {
            grid: TemplateChild::default(),
            month_buttons: RefCell::new(Vec::new()),
            visible_year: Cell::new(today.year()),
            current_month: Cell::new(today.month()),
        }
    }
}

#[glib::object_subclass]
impl ObjectSubclass for YearView {
    const NAME: &'static str = "YearView";
    type Type = super::YearView;
    type ParentType = gtk4::Box;

    fn class_init(klass: &mut Self::Class) {
        klass.bind_template();
    }

    fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
        obj.init_template();
    }
}

impl ObjectImpl for YearView {
    fn constructed(&self) {
        self.parent_constructed();

        let mut buttons = self.month_buttons.borrow_mut();
        for (i, label) in MONTH_LABELS.iter().enumerate() {
            let button = gtk4::Button::with_label(label);
            button.add_css_class("flat");
            button.add_css_class("calendar__month");
            button.set_hexpand(true);
            button.set_vexpand(true);

            let col = (i % 4) as i32;
            let row = (i / 4) as i32;
            self.grid.attach(&button, col, row, 1, 1);

            let obj = self.obj().downgrade();
            let month_index = (i + 1) as u32;
            button.connect_clicked(move |_| {
                if let Some(o) = obj.upgrade() {
                    o.emit_by_name::<()>("month-picked", &[&month_index]);
                }
            });

            buttons.push(button);
        }
        drop(buttons);

        self.refresh();
    }

    fn signals() -> &'static [Signal] {
        static SIGNALS: OnceLock<Vec<Signal>> = OnceLock::new();
        SIGNALS.get_or_init(|| {
            vec![
                Signal::builder("month-picked")
                    .param_types([u32::static_type()])
                    .build(),
            ]
        })
    }
}

impl WidgetImpl for YearView {}
impl BoxImpl for YearView {}

impl YearView {
    fn refresh(&self) {
        let visible_year = self.visible_year.get();
        let current_month = self.current_month.get();
        let today = Local::now().date_naive();
        let buttons = self.month_buttons.borrow();

        for (i, button) in buttons.iter().enumerate() {
            let month_index = (i + 1) as u32;
            let is_today_month = visible_year == today.year() && month_index == today.month();

            set_class(button, "current", month_index == current_month);
            set_class(button, "today-month", is_today_month);
        }
    }

    pub(super) fn set_current_month(&self, year: i32, month: u32) {
        self.visible_year.set(year);
        self.current_month.set(month);
        self.refresh();
    }
}

fn set_class(button: &gtk4::Button, class: &str, on: bool) {
    if on {
        button.add_css_class(class);
    } else {
        button.remove_css_class(class);
    }
}
