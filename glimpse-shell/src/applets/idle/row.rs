use relm4::{
    WidgetTemplate,
    gtk::{self, pango, prelude::*},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdleRowInit {
    pub icon: String,
    pub label: String,
    pub tooltip: String,
}

#[relm4::widget_template(pub)]
impl WidgetTemplate for IdleRow {
    type Init = IdleRowInit;

    view! {
        gtk::Box {
            add_css_class: "idle-row",
            add_css_class: "action-row",
            set_orientation: gtk::Orientation::Horizontal,
            set_spacing: 8,
            set_tooltip_text: Some(&init.tooltip),

            #[name = "icon"]
            gtk::Image {
                add_css_class: "idle-row__icon",
                add_css_class: "action-row__leading",
                set_pixel_size: 16,
                set_icon_name: Some(&init.icon),
            },

            #[name = "label"]
            gtk::Label {
                add_css_class: "idle-row__label",
                add_css_class: "action-row__title",
                set_halign: gtk::Align::Start,
                set_xalign: 0.0,
                set_hexpand: true,
                set_ellipsize: pango::EllipsizeMode::End,
                set_label: &init.label,
            },
        }
    }
}
