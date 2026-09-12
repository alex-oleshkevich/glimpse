use gtk4::{AccessibleRole, glib, graphene, gsk, prelude::*, subclass::prelude::*};
use std::cell::{Cell, OnceCell, RefCell};
use std::sync::OnceLock;

use crate::{Notification, NotificationCard};

pub(crate) const STEP: i32 = 4;
pub(crate) const MAX_DEPTH: usize = 2;
const INSET: i32 = 9;
const STRIP_HEIGHT: i32 = 30;
const FAN_SPACING: i32 = 10;
const CHIP_GAP: i32 = 6;
const CHIP_SPACING: i32 = 7;

#[derive(Debug, Default)]
pub struct NotificationStack {
    pub notifications: RefCell<Vec<Notification>>,
    pub rows: RefCell<Vec<(String, NotificationCard)>>,
    pub strips: RefCell<Vec<gtk4::Box>>,
    pub chip: OnceCell<gtk4::Button>,
    pub chip_label: OnceCell<gtk4::Label>,
    pub chip_arrow: OnceCell<gtk4::Image>,
    pub chip_external: Cell<bool>,
    pub collapsed: Cell<bool>,
}

#[glib::object_subclass]
impl ObjectSubclass for NotificationStack {
    const NAME: &'static str = "NotificationStack";
    type Type = super::NotificationStack;
    type ParentType = gtk4::Widget;

    fn class_init(klass: &mut Self::Class) {
        klass.set_accessible_role(AccessibleRole::Group);
    }
}

impl ObjectImpl for NotificationStack {
    fn signals() -> &'static [glib::subclass::Signal] {
        static SIGNALS: OnceLock<Vec<glib::subclass::Signal>> = OnceLock::new();
        SIGNALS.get_or_init(|| {
            vec![
                glib::subclass::Signal::builder("activated")
                    .param_types([String::static_type()])
                    .build(),
                glib::subclass::Signal::builder("dismissed")
                    .param_types([String::static_type()])
                    .build(),
                glib::subclass::Signal::builder("action-invoked")
                    .param_types([String::static_type(), String::static_type()])
                    .build(),
                glib::subclass::Signal::builder("clear-requested").build(),
            ]
        })
    }

    fn constructed(&self) {
        self.parent_constructed();
        let obj = self.obj();
        obj.add_css_class("notification-stack");
        obj.set_visible(false);
        self.collapsed.set(true);

        let label = gtk4::Label::new(None);
        let arrow: gtk4::Image = glib::Object::builder()
            .property("accessible-role", AccessibleRole::Presentation)
            .property("icon-name", "pan-down-symbolic")
            .build();
        arrow.add_css_class("notification-stack__chevron");

        let content = gtk4::Box::new(gtk4::Orientation::Horizontal, CHIP_SPACING);
        content.append(&label);
        content.append(&arrow);

        let chip = gtk4::Button::builder()
            .halign(gtk4::Align::End)
            .child(&content)
            .build();
        chip.add_css_class("notification-stack__chip");
        chip.set_visible(false);
        chip.connect_clicked(glib::clone!(
            #[weak]
            obj,
            move |_| obj.set_collapsed(!obj.is_collapsed())
        ));
        chip.insert_after(&*obj, None::<&gtk4::Widget>);

        let _ = self.chip.set(chip);
        let _ = self.chip_label.set(label);
        let _ = self.chip_arrow.set(arrow);
    }

    fn dispose(&self) {
        if let Some(chip) = self.chip.get()
            && chip.parent().is_some()
        {
            chip.unparent();
        }
        for strip in self.strips.borrow_mut().drain(..) {
            strip.unparent();
        }
        for (_, row) in self.rows.borrow_mut().drain(..) {
            row.unparent();
        }
    }
}

impl WidgetImpl for NotificationStack {
    fn measure(&self, orientation: gtk4::Orientation, for_size: i32) -> (i32, i32, i32, i32) {
        let shown = self.shown();
        if shown.is_empty() {
            return (0, 0, -1, -1);
        }
        let chip = self
            .chip
            .get()
            .filter(|chip| !self.chip_external.get() && chip.get_visible());

        if orientation == gtk4::Orientation::Horizontal {
            let mut min = 0;
            let mut nat = 0;
            for row in &shown {
                let (row_min, row_nat, _, _) = row.measure(orientation, -1);
                min = min.max(row_min);
                nat = nat.max(row_nat);
            }
            if let Some(chip) = chip {
                let (chip_min, chip_nat, _, _) = chip.measure(orientation, -1);
                min = min.max(chip_min);
                nat = nat.max(chip_nat);
            }
            return (min, nat, -1, -1);
        }

        let width = if for_size >= 0 {
            for_size
        } else {
            self.measure(gtk4::Orientation::Horizontal, -1).1
        };
        let mut height = 0;
        if let Some(chip) = chip {
            height += chip.measure(gtk4::Orientation::Vertical, -1).1 + CHIP_GAP;
        }
        height += self.body_height(&shown, width);
        (height, height, -1, -1)
    }

    fn size_allocate(&self, width: i32, _height: i32, _baseline: i32) {
        let shown = self.shown();
        if shown.is_empty() {
            return;
        }

        let mut y = 0;
        if let Some(chip) = self
            .chip
            .get()
            .filter(|chip| !self.chip_external.get() && chip.get_visible())
        {
            let chip_width = chip.measure(gtk4::Orientation::Horizontal, -1).1.min(width);
            let chip_height = chip.measure(gtk4::Orientation::Vertical, chip_width).1;
            chip.allocate(chip_width, chip_height, -1, at(width - chip_width, y));
            y += chip_height + CHIP_GAP;
        }

        if !self.collapsed.get() {
            for row in &shown {
                let row_height = row.measure(gtk4::Orientation::Vertical, width).1;
                row.allocate(width, row_height, -1, at(0, y));
                y += row_height + FAN_SPACING;
            }
            return;
        }

        let Some(front) = shown.first() else {
            return;
        };
        let front_height = front.measure(gtk4::Orientation::Vertical, width).1;

        let strips = self.strips.borrow();
        let count = strips.len();
        for (index, strip) in strips.iter().enumerate() {
            let depth = (count - index) as i32;
            let strip_width = (width - INSET * 2 * depth).max(0);
            strip.allocate(
                strip_width,
                STRIP_HEIGHT,
                -1,
                at(
                    INSET * depth,
                    y + front_height - STRIP_HEIGHT + STEP * depth,
                ),
            );
        }
        drop(strips);

        front.allocate(width, front_height, -1, at(0, y));
    }
}

impl NotificationStack {
    pub(crate) fn depth(&self) -> usize {
        if !self.collapsed.get() || self.rows.borrow().len() < super::STACK_MIN_ITEMS {
            return 0;
        }
        self.rows.borrow().len().saturating_sub(1).min(MAX_DEPTH)
    }

    fn shown(&self) -> Vec<NotificationCard> {
        let rows = self.rows.borrow();
        if self.collapsed.get() {
            return rows
                .first()
                .map(|(_, row)| row.clone())
                .into_iter()
                .collect();
        }
        rows.iter().map(|(_, row)| row.clone()).collect()
    }

    fn body_height(&self, shown: &[NotificationCard], width: i32) -> i32 {
        if self.collapsed.get() {
            let front = shown
                .first()
                .map(|row| row.measure(gtk4::Orientation::Vertical, width).1)
                .unwrap_or(0);
            return front + STEP * self.depth() as i32;
        }

        let mut height = 0;
        for (index, row) in shown.iter().enumerate() {
            if index > 0 {
                height += FAN_SPACING;
            }
            height += row.measure(gtk4::Orientation::Vertical, width).1;
        }
        height
    }
}

fn at(x: i32, y: i32) -> Option<gsk::Transform> {
    Some(gsk::Transform::new().translate(&graphene::Point::new(x as f32, y as f32)))
}
