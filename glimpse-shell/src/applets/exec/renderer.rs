use std::rc::Rc;

use relm4::{
    RelmWidgetExt, WidgetTemplate,
    gtk::{self, prelude::*},
};

use crate::components::{
    badge::BadgeView,
    card_surface::CardSurface,
    copyable::CopyableView,
    empty_state::EmptyStateView,
    hero::HeroView,
    key_value_grid::{KeyValueItem, static_key_value_grid},
    meter::MeterView,
    pager::{
        PagerAppearance as PagerComponentAppearance, PagerItemView, static_pager_item,
        static_pager_strip,
    },
    section_header::SectionHeader,
    status_dot::StatusDotView,
};

use super::protocol::{
    ActionItemNode, AlignValue, BadgeNode, BorderWidthValue, ButtonNode, ButtonVariant, CardNode,
    CheckboxNode, ColorValue, CommonProps, ContainerNode, ContentFitValue, CopyableNode,
    EmptyStateNode, EventKind, EventPayload, EventSource, ExpanderNode, FontSizeValue,
    FontWeightValue, GridNode, HeroNode, IconNode, ItemNode, LabelNode, LayoutNode,
    LevelBarModeValue, LevelBarNode, LinkButtonNode, MeterNode, OrientationValue,
    PagerAppearanceValue, PagerItemNode, PagerStripNode, PictureNode, ProgressNode,
    PropertyListNode, RadiusValue, ScrollNode, SectionNode, SelectNode, SeparatorNode, SliderNode,
    SpaceValue, SpinnerNode, StatusNode, SwitchNode, ToggleButtonNode, TreeNode, Variant,
};

pub type EventSink = Rc<dyn Fn(EventPayload)>;

#[derive(Clone)]
pub struct RenderCatalog {
    event: EventSink,
}

impl RenderCatalog {
    pub fn new(event: EventSink) -> Self {
        Self { event }
    }

    pub fn render(&self, node: &TreeNode) -> Result<gtk::Widget, RenderError> {
        match node {
            TreeNode::Hero(data) => self.render_hero(data),
            TreeNode::Card(data) => self.render_card(data),
            TreeNode::Container(data) => self.render_container(data),
            TreeNode::Section(data) => self.render_section(data),
            TreeNode::Meter(data) => self.render_meter(data),
            TreeNode::Copyable(data) => Ok(self.render_copyable(data).upcast()),
            TreeNode::Column(data) => {
                self.render_layout(data, gtk::Orientation::Vertical, "column")
            }
            TreeNode::Row(data) => self.render_layout(data, gtk::Orientation::Horizontal, "row"),
            TreeNode::PropertyList(data) => Ok(self.render_property_list(data).upcast()),
            TreeNode::Item(data) => self.render_item(data),
            TreeNode::ActionItem(data) => self.render_action_item(data),
            TreeNode::EmptyState(data) => Ok(self.render_empty_state(data).upcast()),
            TreeNode::Badge(data) => Ok(self.render_badge(data).upcast()),
            TreeNode::Status(data) => Ok(self.render_status(data).upcast()),
            TreeNode::PagerItem(data) => Ok(self.render_pager_item(data).upcast()),
            TreeNode::PagerStrip(data) => Ok(self.render_pager_strip(data).upcast()),
            TreeNode::Spinner(data) => Ok(self.render_spinner(data).upcast()),
            TreeNode::Grid(data) => self.render_grid(data),
            TreeNode::Scroll(data) => self.render_scroll(data),
            TreeNode::LevelBar(data) => Ok(self.render_level_bar(data).upcast()),
            TreeNode::Progress(data) => Ok(self.render_progress(data).upcast()),
            TreeNode::Separator(data) => Ok(self.render_separator(data).upcast()),
            TreeNode::Label(data) => Ok(self.render_label(data).upcast()),
            TreeNode::Icon(data) => Ok(self.render_icon(data).upcast()),
            TreeNode::Picture(data) => Ok(self.render_picture(data).upcast()),
            TreeNode::Button(data) => self.render_button(data),
            TreeNode::LinkButton(data) => Ok(self.render_link_button(data).upcast()),
            TreeNode::Expander(data) => self.render_expander(data),
            TreeNode::Switch(data) => self.render_switch(data),
            TreeNode::ToggleButton(data) => self.render_toggle_button(data),
            TreeNode::Checkbox(data) => self.render_checkbox(data),
            TreeNode::Slider(data) => self.render_slider(data),
            TreeNode::Select(data) => self.render_select(data),
            // PopoverScaffold is a root-only node handled in Popover::rebuild();
            // render just the body if encountered nested.
            TreeNode::PopoverScaffold(data) => self.render(&data.body),
        }
    }

    fn render_hero(&self, data: &HeroNode) -> Result<gtk::Widget, RenderError> {
        let hero = HeroView::init(());
        hero.title.set_label(&data.title);
        hero.subtitle.set_label(&data.subtitle);
        hero.subtitle.set_visible(!data.subtitle.is_empty());
        hero.icon.set_visible(data.icon.is_some());
        if let Some(icon) = &data.icon {
            hero.icon.set_icon_name(Some(icon.as_str()));
        }
        if let Some(active) = data.switch {
            hero.trailing.set_visible(true);
            hero.toggle.set_active(active);
            if let Some(id) = data.id.clone().filter(|id| !id.is_empty()) {
                let event = self.event.clone();
                hero.toggle.connect_state_set(move |_, active| {
                    event(EventPayload {
                        id: id.clone(),
                        kind: EventKind::Toggle,
                        source: EventSource::Popover,
                        button: None,
                        active: Some(active),
                        value: None,
                        delta_y: None,
                    });
                    gtk::glib::Propagation::Proceed
                });
            }
        }
        apply_common_props(hero.as_ref(), &data.common);
        Ok(hero.as_ref().clone().upcast())
    }

    fn render_card(&self, data: &CardNode) -> Result<gtk::Widget, RenderError> {
        let card = CardSurface::init(());
        apply_common_props(card.as_ref(), &data.common);
        if let Some(child) = &data.child {
            card.body.append(&self.render(child)?);
        }
        Ok(card.as_ref().clone().upcast())
    }

    fn render_container(&self, data: &ContainerNode) -> Result<gtk::Widget, RenderError> {
        let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
        root.add_css_class("container");
        apply_common_props_without_inline(&root, &data.common);
        apply_container_size(&root, data);
        apply_container_margin(&root, data);
        apply_container_styles(&root, data);
        apply_inline_styles(&root, &data.common);
        if let Some(child) = &data.child {
            root.append(&self.render(child)?);
        }
        Ok(root.upcast())
    }

    fn render_section(&self, data: &SectionNode) -> Result<gtk::Widget, RenderError> {
        let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
        root.add_css_class("section-block");
        root.add_css_class("section");
        apply_common_props(&root, &data.common);

        if !data.title.is_empty() {
            let header = SectionHeader::init(());
            header.title.set_label(&data.title);
            header.subtitle.set_label(&data.subtitle);
            header.subtitle.set_visible(!data.subtitle.is_empty());
            root.append(header.as_ref());
        }

        let body = gtk::Box::new(gtk::Orientation::Vertical, 0);
        body.add_css_class("section-block__body");
        body.add_css_class("section__body");
        if let Some(child) = &data.child {
            body.append(&self.render(child)?);
        }
        root.append(&body);
        Ok(root.upcast())
    }

    fn render_layout(
        &self,
        data: &LayoutNode,
        orientation: gtk::Orientation,
        class_name: &'static str,
    ) -> Result<gtk::Widget, RenderError> {
        let root = gtk::Box::new(orientation, data.spacing);
        root.add_css_class(class_name);
        apply_common_props(&root, &data.common);
        for child in &data.children {
            root.append(&self.render(child)?);
        }
        Ok(root.upcast())
    }

    fn render_meter(&self, data: &MeterNode) -> Result<gtk::Widget, RenderError> {
        let meter = MeterView::init(());
        meter.label.set_label(&data.label);
        if let Some(icon) = &data.icon {
            apply_icon_to_image(&meter.icon, icon);
            meter.icon.set_visible(true);
        }
        if let Some(text) = &data.text {
            meter.value.set_label(text);
            meter.value.set_visible(true);
        }

        if data.interactive {
            let id = require_id("meter", data.id.as_deref().unwrap_or(""))?;
            let (min, max) = meter_bounds(data.min, data.max, data.step);
            let scale = gtk::Scale::with_range(
                gtk::Orientation::Horizontal,
                min,
                max,
                data.step.max(f64::EPSILON),
            );
            scale.add_css_class("meter__scale");
            scale.add_css_class("scale");
            scale.set_draw_value(false);
            scale.set_value(data.value.clamp(min, max));
            let event = self.event.clone();
            scale.connect_value_changed(move |scale| {
                event(EventPayload {
                    id: id.clone(),
                    kind: EventKind::Change,
                    source: EventSource::Popover,
                    button: None,
                    active: None,
                    value: Some(serde_json::Value::from(scale.value())),
                    delta_y: None,
                });
            });
            meter.control.append(&scale);
        } else {
            let progress = gtk::ProgressBar::new();
            progress.add_css_class("meter__progress");
            progress.add_css_class("progress");
            progress.set_fraction(meter_fraction(data.value, data.min, data.max));
            meter.control.append(&progress);
        }

        apply_common_props(meter.as_ref(), &data.common);
        Ok(meter.as_ref().clone().upcast())
    }

    fn render_copyable(&self, data: &CopyableNode) -> gtk::Box {
        let copyable = CopyableView::init(());
        if !data.label.is_empty() {
            copyable.label.set_label(&data.label);
            copyable.label.set_visible(true);
        }
        copyable.value.set_label(&data.value);
        let copy_value = data.value.clone();
        copyable.button.connect_clicked(move |_| {
            if let Some(display) = gtk::gdk::Display::default() {
                display.clipboard().set_text(&copy_value);
            }
        });
        apply_common_props(copyable.as_ref(), &data.common);
        copyable.as_ref().clone()
    }

    fn render_property_list(&self, data: &PropertyListNode) -> gtk::Box {
        let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
        root.add_css_class("property-list");

        if !data.title.is_empty() {
            let title = gtk::Label::new(Some(&data.title));
            title.add_css_class("property-list__title");
            title.set_halign(gtk::Align::Start);
            title.set_xalign(0.0);
            root.append(&title);
        }

        let rows = static_key_value_grid(
            data.rows
                .iter()
                .map(|item| KeyValueItem {
                    label: item.key.clone(),
                    value: item.value.clone(),
                    visible: true,
                })
                .collect(),
        );
        rows.add_css_class("property-list__rows");
        root.append(&rows);
        apply_common_props(&root, &data.common);
        root
    }

    fn render_item(&self, data: &ItemNode) -> Result<gtk::Widget, RenderError> {
        let root = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        root.add_css_class("flat");
        root.add_css_class("list-item");
        root.add_css_class("list-item__button");
        root.set_hexpand(false);

        let content = build_item_content(
            data.left.as_deref(),
            self,
            &data.label,
            &data.sublabel,
            data.right.as_deref(),
            self,
        )?;
        root.append(&content);
        apply_common_props(&root, &data.common);
        Ok(root.upcast())
    }

    fn render_action_item(&self, data: &ActionItemNode) -> Result<gtk::Widget, RenderError> {
        let id = require_id("action_item", &data.id)?;
        let root = gtk::Button::new();
        root.add_css_class("flat");
        root.add_css_class("list-item");
        root.add_css_class("list-item__button");
        root.set_hexpand(false);
        root.set_sensitive(data.enabled);

        let inert_renderer = RenderCatalog::new(Rc::new(|_| {}));
        let content = build_item_content(
            data.left.as_deref(),
            &inert_renderer,
            &data.label,
            &data.sublabel,
            data.right.as_deref(),
            &inert_renderer,
        )?;
        root.set_child(Some(&content));
        apply_common_props(&root, &data.common);
        connect_click(&root, self.event.clone(), id);
        Ok(root.upcast())
    }

    fn render_empty_state(&self, data: &EmptyStateNode) -> gtk::Box {
        let empty = EmptyStateView::init(());
        empty.title.set_label(&data.title);
        empty.subtitle.set_label(&data.subtitle);
        empty.subtitle.set_visible(!data.subtitle.is_empty());
        apply_common_props(empty.as_ref(), &data.common);
        empty.as_ref().clone()
    }

    fn render_badge(&self, data: &BadgeNode) -> gtk::Label {
        let badge = BadgeView::init(());
        badge.set_label(&data.label);
        apply_common_props(badge.as_ref(), &data.common);
        apply_variant(badge.as_ref(), data.variant);
        badge.as_ref().clone()
    }

    fn render_status(&self, data: &StatusNode) -> gtk::Box {
        let dot = StatusDotView::init(());
        dot.add_css_class("status");
        apply_common_props(dot.as_ref(), &data.common);
        if let Some(variant) = data.variant {
            dot.add_css_class(variant.class_name());
        }
        dot.as_ref().clone()
    }

    fn render_pager_item(&self, data: &PagerItemNode) -> gtk::Box {
        let item = static_pager_item(&pager_item_view(data));
        if let Some(id) = data.id.clone().filter(|id| !id.is_empty()) {
            connect_widget_click(&item, self.event.clone(), id);
        }
        apply_common_props(&item, &data.common);
        item
    }

    fn render_pager_strip(&self, data: &PagerStripNode) -> gtk::Box {
        let views = data.items.iter().map(pager_item_view).collect::<Vec<_>>();
        let strip = static_pager_strip(&views);
        let mut child = strip.first_child();
        for item in &data.items {
            let Some(widget) = child else {
                break;
            };
            if let Ok(item_box) = widget.clone().downcast::<gtk::Box>() {
                if let Some(id) = item.id.clone().filter(|id| !id.is_empty()) {
                    connect_widget_click(&item_box, self.event.clone(), id);
                }
                apply_common_props(&item_box, &item.common);
            }
            child = widget.next_sibling();
        }
        if let Some(id) = data.id.clone().filter(|id| !id.is_empty()) {
            connect_widget_scroll(&strip, self.event.clone(), id);
        }
        apply_common_props(&strip, &data.common);
        strip
    }

    fn render_spinner(&self, data: &SpinnerNode) -> gtk::Spinner {
        let spinner = gtk::Spinner::new();
        spinner.add_css_class("spinner");
        spinner.set_spinning(data.spinning);
        apply_common_props(&spinner, &data.common);
        spinner
    }

    fn render_grid(&self, data: &GridNode) -> Result<gtk::Widget, RenderError> {
        let grid = gtk::Grid::new();
        grid.add_css_class("grid");
        grid.set_row_spacing(data.row_spacing as u32);
        grid.set_column_spacing(data.column_spacing as u32);
        apply_common_props(&grid, &data.common);
        for child in &data.children {
            grid.attach(
                &self.render(&child.child)?,
                child.column,
                child.row,
                child.width,
                child.height,
            );
        }
        Ok(grid.upcast())
    }

    fn render_scroll(&self, data: &ScrollNode) -> Result<gtk::Widget, RenderError> {
        let scroll = gtk::ScrolledWindow::new();
        scroll.add_css_class("scroll");
        scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
        scroll.set_propagate_natural_height(true);
        apply_common_props(&scroll, &data.common);
        scroll.set_child(Some(&self.render(&data.child)?));
        Ok(scroll.upcast())
    }

    fn render_progress(&self, data: &ProgressNode) -> gtk::ProgressBar {
        let progress = gtk::ProgressBar::new();
        progress.add_css_class("progress");
        apply_common_props(&progress, &data.common);
        progress.set_fraction(progress_fraction(data.value, data.max));
        if data.show_text || data.text.is_some() {
            progress.set_show_text(true);
            progress.set_text(data.text.as_deref());
        }
        progress
    }

    fn render_level_bar(&self, data: &LevelBarNode) -> gtk::LevelBar {
        let level_bar = gtk::LevelBar::new();
        level_bar.add_css_class("level-bar");
        level_bar.set_min_value(data.min);
        level_bar.set_max_value(data.max);
        level_bar.set_value(data.value);
        level_bar.set_mode(to_level_bar_mode(data.mode));
        apply_common_props(&level_bar, &data.common);
        level_bar
    }

    fn render_separator(&self, data: &SeparatorNode) -> gtk::Separator {
        let separator = gtk::Separator::new(to_orientation(
            data.orientation.unwrap_or(OrientationValue::Horizontal),
        ));
        separator.add_css_class("separator");
        apply_common_props(&separator, &data.common);
        separator
    }

    fn render_label(&self, data: &LabelNode) -> gtk::Label {
        let label = gtk::Label::new(Some(&data.text));
        label.add_css_class("label");
        label.set_wrap(data.wrap);
        label.set_selectable(data.selectable);
        if let Some(xalign) = data.xalign {
            label.set_xalign(xalign);
        }
        apply_common_props(&label, &data.common);
        apply_variant(&label, data.variant);
        label
    }

    fn render_icon(&self, data: &IconNode) -> gtk::Image {
        let image = gtk::Image::new();
        image.add_css_class("icon");
        if let Some(pixel_size) = data.pixel_size {
            image.set_pixel_size(pixel_size);
        }
        apply_icon_to_image(&image, &data.icon);
        apply_common_props(&image, &data.common);
        image
    }

    fn render_picture(&self, data: &PictureNode) -> gtk::Picture {
        let picture = gtk::Picture::for_filename(&data.path);
        picture.add_css_class("picture");
        picture.set_content_fit(to_content_fit(data.content_fit));
        apply_common_props(&picture, &data.common);
        picture
    }

    fn render_button(&self, data: &ButtonNode) -> Result<gtk::Widget, RenderError> {
        let id = require_id("button", &data.id)?;
        let button = gtk::Button::new();
        button.add_css_class("button");
        apply_button_variant(&button, data.variant);
        button.set_sensitive(data.enabled);
        apply_common_props(&button, &data.common);
        let content = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        content.set_valign(gtk::Align::Center);
        if let Some(icon) = &data.icon {
            let image = gtk::Image::from_icon_name(icon);
            content.append(&image);
        }
        if let Some(label) = &data.label {
            content.append(&gtk::Label::new(Some(label)));
        }
        button.set_child(Some(&content));
        connect_click(&button, self.event.clone(), id);
        Ok(button.upcast())
    }

    fn render_link_button(&self, data: &LinkButtonNode) -> gtk::LinkButton {
        let link = if let Some(label) = &data.label {
            gtk::LinkButton::with_label(&data.uri, label)
        } else {
            gtk::LinkButton::new(&data.uri)
        };
        link.add_css_class("link-button");
        apply_common_props(&link, &data.common);
        link
    }

    fn render_expander(&self, data: &ExpanderNode) -> Result<gtk::Widget, RenderError> {
        let expander = gtk::Expander::new(Some(&data.label));
        expander.add_css_class("expander");
        expander.set_expanded(data.expanded);
        let child = self.render(&data.child)?;
        expander.set_child(Some(&child));
        apply_common_props(&expander, &data.common);
        Ok(expander.upcast())
    }

    fn render_switch(&self, data: &SwitchNode) -> Result<gtk::Widget, RenderError> {
        let id = require_id("switch", &data.id)?;
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        row.add_css_class("switch");
        apply_common_props(&row, &data.common);
        if let Some(label) = &data.label {
            let text = gtk::Label::new(Some(label));
            text.set_xalign(0.0);
            text.set_hexpand(true);
            row.append(&text);
        }
        let switch = gtk::Switch::new();
        switch.set_active(data.active);
        let event = self.event.clone();
        switch.connect_state_set(move |_, active| {
            event(EventPayload {
                id: id.clone(),
                kind: EventKind::Toggle,
                source: EventSource::Popover,
                button: None,
                active: Some(active),
                value: None,
                delta_y: None,
            });
            gtk::glib::Propagation::Proceed
        });
        row.append(&switch);
        Ok(row.upcast())
    }

    fn render_toggle_button(&self, data: &ToggleButtonNode) -> Result<gtk::Widget, RenderError> {
        let id = require_id("toggle_button", &data.id)?;
        let toggle = gtk::ToggleButton::new();
        if data.icon.is_some() || data.label.is_some() {
            let content = gtk::Box::new(gtk::Orientation::Horizontal, 6);
            content.set_valign(gtk::Align::Center);
            if let Some(icon) = &data.icon {
                content.append(&gtk::Image::from_icon_name(icon));
            }
            if let Some(label) = &data.label {
                content.append(&gtk::Label::new(Some(label)));
            }
            toggle.set_child(Some(&content));
        }
        toggle.add_css_class("toggle-button");
        toggle.set_active(data.active);
        apply_common_props(&toggle, &data.common);
        let event = self.event.clone();
        toggle.connect_toggled(move |button| {
            event(EventPayload {
                id: id.clone(),
                kind: EventKind::Toggle,
                source: EventSource::Popover,
                button: None,
                active: Some(button.is_active()),
                value: None,
                delta_y: None,
            });
        });
        Ok(toggle.upcast())
    }

    fn render_checkbox(&self, data: &CheckboxNode) -> Result<gtk::Widget, RenderError> {
        let id = require_id("checkbox", &data.id)?;
        let checkbox = if let Some(label) = &data.label {
            gtk::CheckButton::with_label(label)
        } else {
            gtk::CheckButton::new()
        };
        checkbox.add_css_class("checkbox");
        checkbox.set_active(data.active);
        apply_common_props(&checkbox, &data.common);
        let event = self.event.clone();
        checkbox.connect_toggled(move |button| {
            event(EventPayload {
                id: id.clone(),
                kind: EventKind::Toggle,
                source: EventSource::Popover,
                button: None,
                active: Some(button.is_active()),
                value: None,
                delta_y: None,
            });
        });
        Ok(checkbox.upcast())
    }

    fn render_slider(&self, data: &SliderNode) -> Result<gtk::Widget, RenderError> {
        let id = require_id("slider", &data.id)?;
        let slider = gtk::Scale::with_range(
            to_orientation(data.orientation.unwrap_or(OrientationValue::Horizontal)),
            data.min,
            data.max,
            data.step,
        );
        slider.add_css_class("slider");
        slider.set_value(data.value);
        slider.set_draw_value(data.draw_value);
        apply_common_props(&slider, &data.common);
        let event = self.event.clone();
        slider.connect_value_changed(move |slider| {
            event(EventPayload {
                id: id.clone(),
                kind: EventKind::Change,
                source: EventSource::Popover,
                button: None,
                active: None,
                value: Some(serde_json::Value::from(slider.value())),
                delta_y: None,
            });
        });
        Ok(slider.upcast())
    }

    fn render_select(&self, data: &SelectNode) -> Result<gtk::Widget, RenderError> {
        let id = require_id("select", &data.id)?;
        let labels: Vec<&str> = data.items.iter().map(|item| item.label.as_str()).collect();
        let select = gtk::DropDown::from_strings(&labels);
        select.add_css_class("select");
        if let Some(selected) = data.selected {
            select.set_selected(selected);
        }
        apply_common_props(&select, &data.common);
        let items = data.items.clone();
        let event = self.event.clone();
        select.connect_selected_notify(move |select| {
            let index = select.selected();
            let value = items
                .get(index as usize)
                .map(|item| serde_json::json!({"id": item.id, "label": item.label, "index": index}))
                .unwrap_or_else(|| serde_json::json!({"index": index}));
            event(EventPayload {
                id: id.clone(),
                kind: EventKind::Change,
                source: EventSource::Popover,
                button: None,
                active: None,
                value: Some(value),
                delta_y: None,
            });
        });
        Ok(select.upcast())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RenderError {
    MissingId { widget_type: &'static str },
}

impl std::fmt::Display for RenderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingId { widget_type } => write!(f, "{widget_type} requires a string id"),
        }
    }
}

impl std::error::Error for RenderError {}

pub fn apply_icon_to_image(image: &gtk::Image, icon: &str) {
    image.set_icon_name(Some(icon));
}

fn progress_fraction(value: f64, max: f64) -> f64 {
    if max <= 0.0 {
        0.0
    } else {
        (value / max).clamp(0.0, 1.0)
    }
}

fn meter_bounds(min: f64, max: f64, step: f64) -> (f64, f64) {
    if max > min {
        (min, max)
    } else {
        (min, min + step.max(f64::EPSILON))
    }
}

fn meter_fraction(value: f64, min: f64, max: f64) -> f64 {
    if max <= min {
        0.0
    } else {
        ((value - min) / (max - min)).clamp(0.0, 1.0)
    }
}

fn build_item_content(
    left_node: Option<&TreeNode>,
    left_renderer: &RenderCatalog,
    label_text: &str,
    sublabel_text: &str,
    right_node: Option<&TreeNode>,
    right_renderer: &RenderCatalog,
) -> Result<gtk::Box, RenderError> {
    let content = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    content.add_css_class("list-item__content");
    content.set_valign(gtk::Align::Center);

    let left = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    left.add_css_class("list-item__left");
    left.set_halign(gtk::Align::Start);
    left.set_valign(gtk::Align::Center);
    left.set_hexpand(false);
    left.set_visible(left_node.is_some());
    if let Some(child) = left_node {
        left.append(&left_renderer.render(child)?);
    }
    content.append(&left);

    let text = gtk::Box::new(gtk::Orientation::Vertical, 0);
    text.add_css_class("list-item__text");
    text.set_hexpand(true);
    text.set_valign(gtk::Align::Center);

    let label = gtk::Label::new(Some(label_text));
    label.add_css_class("list-item__label");
    label.set_halign(gtk::Align::Start);
    label.set_xalign(0.0);
    text.append(&label);

    let sublabel = gtk::Label::new(Some(sublabel_text));
    sublabel.add_css_class("list-item__secondary_label");
    sublabel.set_halign(gtk::Align::Start);
    sublabel.set_xalign(0.0);
    sublabel.set_visible(!sublabel_text.is_empty());
    text.append(&sublabel);
    content.append(&text);

    let right = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    right.add_css_class("list-item__right");
    right.set_halign(gtk::Align::End);
    right.set_valign(gtk::Align::Center);
    right.set_hexpand(false);
    right.set_visible(right_node.is_some());
    if let Some(child) = right_node {
        right.append(&right_renderer.render(child)?);
    }
    content.append(&right);

    Ok(content)
}

fn connect_click(button: &gtk::Button, event: EventSink, id: String) {
    button.connect_clicked(move |_| {
        event(EventPayload {
            id: id.clone(),
            kind: EventKind::Click,
            source: EventSource::Popover,
            button: Some(super::protocol::MouseButton::Left),
            active: None,
            value: None,
            delta_y: None,
        });
    });
}

fn connect_widget_click(widget: &impl IsA<gtk::Widget>, event: EventSink, id: String) {
    let click = gtk::GestureClick::new();
    click.set_button(1);
    click.connect_pressed(move |_, _, _, _| {
        event(EventPayload {
            id: id.clone(),
            kind: EventKind::Click,
            source: EventSource::Popover,
            button: Some(super::protocol::MouseButton::Left),
            active: None,
            value: None,
            delta_y: None,
        });
    });
    widget.add_controller(click);
}

fn connect_widget_scroll(widget: &impl IsA<gtk::Widget>, event: EventSink, id: String) {
    let scroll = gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::VERTICAL);
    scroll.connect_scroll(move |_, _, dy| {
        event(EventPayload {
            id: id.clone(),
            kind: EventKind::Scroll,
            source: EventSource::Popover,
            button: None,
            active: None,
            value: None,
            delta_y: Some(dy),
        });
        gtk::glib::Propagation::Stop
    });
    widget.add_controller(scroll);
}

fn require_id(widget_type: &'static str, id: &str) -> Result<String, RenderError> {
    if id.is_empty() {
        Err(RenderError::MissingId { widget_type })
    } else {
        Ok(id.to_owned())
    }
}

fn apply_common_props(widget: &impl IsA<gtk::Widget>, props: &CommonProps) {
    apply_common_props_without_inline(widget, props);
    apply_inline_styles(widget, props);
}

fn apply_common_props_without_inline(widget: &impl IsA<gtk::Widget>, props: &CommonProps) {
    if let Some(visible) = props.visible {
        widget.set_visible(visible);
    }
    if let Some(hexpand) = props.hexpand {
        widget.set_hexpand(hexpand);
    }
    if let Some(vexpand) = props.vexpand {
        widget.set_vexpand(vexpand);
    }
    if let Some(halign) = props.halign {
        widget.set_halign(to_align(halign));
    }
    if let Some(valign) = props.valign {
        widget.set_valign(to_align(valign));
    }
    if let Some(tooltip) = &props.tooltip {
        widget.set_tooltip_text(Some(tooltip));
    }
    for class in &props.css_classes {
        widget.add_css_class(class);
    }
}

fn apply_inline_styles(widget: &impl IsA<gtk::Widget>, props: &CommonProps) {
    if props.styles.is_empty() {
        return;
    }

    let declarations = props
        .styles
        .iter()
        .filter_map(|(property, value)| inline_style_declaration(property, value))
        .collect::<Vec<_>>();
    if declarations.is_empty() {
        return;
    }

    widget.inline_css(&declarations.join(" "));
}

fn inline_style_declaration(property: &str, value: &str) -> Option<String> {
    if !is_safe_css_property(property) || !is_safe_css_value(value) {
        tracing::warn!(property, "exec widget: ignored invalid inline style");
        return None;
    }

    Some(format!("{property}: {value};"))
}

fn is_safe_css_property(property: &str) -> bool {
    !property.is_empty()
        && property
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
}

fn is_safe_css_value(value: &str) -> bool {
    !value.is_empty() && !value.bytes().any(|byte| matches!(byte, b'{' | b'}' | b';'))
}

fn apply_container_size(widget: &impl IsA<gtk::Widget>, data: &ContainerNode) {
    let width = size_request_value(data.width.or(data.min_width));
    let height = size_request_value(data.height.or(data.min_height));
    if width >= 0 || height >= 0 {
        widget.set_size_request(width, height);
    }
}

fn size_request_value(value: Option<i32>) -> i32 {
    value.unwrap_or(-1).max(-1)
}

fn apply_container_margin(widget: &impl IsA<gtk::Widget>, data: &ContainerNode) {
    let margin = data.margin.map(space_px);
    if let Some(value) = data.margin_top.map(space_px).or(margin) {
        widget.set_margin_top(value);
    }
    if let Some(value) = data.margin_right.map(space_px).or(margin) {
        widget.set_margin_end(value);
    }
    if let Some(value) = data.margin_bottom.map(space_px).or(margin) {
        widget.set_margin_bottom(value);
    }
    if let Some(value) = data.margin_left.map(space_px).or(margin) {
        widget.set_margin_start(value);
    }
}

fn apply_container_styles(widget: &impl IsA<gtk::Widget>, data: &ContainerNode) {
    let mut declarations = Vec::new();
    if let Some(background) = data.background {
        declarations.push(format!("background: {};", color_css(background)));
    }
    if let Some(color) = data.color {
        declarations.push(format!("color: {};", color_css(color)));
    }
    if let Some(radius) = data.border_radius {
        declarations.push(format!("border-radius: {};", radius_css(radius)));
    }
    if let Some(width) = data.border_width {
        declarations.push(format!("border-width: {};", border_width_css(width)));
        declarations.push("border-style: solid;".into());
    }
    if let Some(color) = data.border_color {
        declarations.push(format!("border-color: {};", color_css(color)));
    }
    if let Some(size) = data.font_size {
        declarations.push(format!("font-size: {};", font_size_css(size)));
    }
    if let Some(weight) = data.font_weight {
        declarations.push(format!("font-weight: {};", font_weight_css(weight)));
    }
    append_spacing_declarations(
        &mut declarations,
        "padding",
        data.padding,
        data.padding_top,
        data.padding_right,
        data.padding_bottom,
        data.padding_left,
    );
    if !declarations.is_empty() {
        widget.inline_css(&declarations.join(" "));
    }
}

fn append_spacing_declarations(
    declarations: &mut Vec<String>,
    property: &str,
    all: Option<SpaceValue>,
    top: Option<SpaceValue>,
    right: Option<SpaceValue>,
    bottom: Option<SpaceValue>,
    left: Option<SpaceValue>,
) {
    if let Some(value) = all {
        declarations.push(format!("{property}: {};", space_css(value)));
    }
    if let Some(value) = top {
        declarations.push(format!("{property}-top: {};", space_css(value)));
    }
    if let Some(value) = right {
        declarations.push(format!("{property}-right: {};", space_css(value)));
    }
    if let Some(value) = bottom {
        declarations.push(format!("{property}-bottom: {};", space_css(value)));
    }
    if let Some(value) = left {
        declarations.push(format!("{property}-left: {};", space_css(value)));
    }
}

fn space_px(value: SpaceValue) -> i32 {
    match value {
        SpaceValue::None => 0,
        SpaceValue::Xxs => 2,
        SpaceValue::Xs => 4,
        SpaceValue::Sm => 6,
        SpaceValue::Md => 8,
        SpaceValue::Lg => 16,
    }
}

fn space_css(value: SpaceValue) -> &'static str {
    match value {
        SpaceValue::None => "0",
        SpaceValue::Xxs => "var(--space-1)",
        SpaceValue::Xs => "var(--space-2)",
        SpaceValue::Sm => "var(--space-3)",
        SpaceValue::Md => "var(--space-4)",
        SpaceValue::Lg => "var(--space-6)",
    }
}

fn color_css(value: ColorValue) -> &'static str {
    match value {
        ColorValue::Bg => "var(--color-bg)",
        ColorValue::Fg => "var(--color-fg)",
        ColorValue::Surface => "var(--color-surface)",
        ColorValue::SurfaceRaised => "var(--color-surface-raised)",
        ColorValue::Border => "var(--color-border)",
        ColorValue::MutedFg => "var(--color-muted-fg)",
        ColorValue::Accent => "var(--color-accent)",
        ColorValue::AccentFg => "var(--color-accent-fg)",
        ColorValue::Success => "var(--color-success)",
        ColorValue::SuccessFg => "var(--color-success-fg)",
        ColorValue::Warning => "var(--color-warning)",
        ColorValue::WarningFg => "var(--color-warning-fg)",
        ColorValue::Danger => "var(--color-danger)",
        ColorValue::DangerFg => "var(--color-danger-fg)",
    }
}

fn radius_css(value: RadiusValue) -> &'static str {
    match value {
        RadiusValue::None => "0",
        RadiusValue::Sm => "var(--radius-sm)",
        RadiusValue::Md => "var(--radius-md)",
        RadiusValue::Lg => "var(--radius-lg)",
        RadiusValue::Pill => "var(--radius-pill)",
    }
}

fn border_width_css(value: BorderWidthValue) -> &'static str {
    match value {
        BorderWidthValue::None => "var(--border-width-none)",
        BorderWidthValue::Thin => "var(--border-width-thin)",
        BorderWidthValue::Medium => "var(--border-width-medium)",
        BorderWidthValue::Thick => "var(--border-width-thick)",
    }
}

fn font_size_css(value: FontSizeValue) -> &'static str {
    match value {
        FontSizeValue::Xxs => "var(--font-size-xxs)",
        FontSizeValue::Xs => "var(--font-size-xs)",
        FontSizeValue::Sm => "var(--font-size-sm)",
        FontSizeValue::Md | FontSizeValue::Base => "var(--font-size-md)",
        FontSizeValue::Lg => "var(--font-size-lg)",
        FontSizeValue::Xl => "var(--font-size-xl)",
    }
}

fn font_weight_css(value: FontWeightValue) -> &'static str {
    match value {
        FontWeightValue::Normal => "var(--font-weight-normal)",
        FontWeightValue::Medium => "var(--font-weight-medium)",
        FontWeightValue::Semibold => "var(--font-weight-semibold)",
        FontWeightValue::Bold => "var(--font-weight-bold)",
    }
}

fn apply_variant(widget: &impl IsA<gtk::Widget>, variant: Option<Variant>) {
    if let Some(class_name) = variant.and_then(|v| v.class_name()) {
        widget.add_css_class(class_name);
    }
}

fn apply_button_variant(button: &gtk::Button, variant: ButtonVariant) {
    match variant {
        ButtonVariant::Primary => button.add_css_class("suggested-action"),
        ButtonVariant::Secondary => {}
        ButtonVariant::Compact => button.add_css_class("compact"),
        ButtonVariant::Flat => button.add_css_class("flat"),
        ButtonVariant::Danger => button.add_css_class("destructive-action"),
    }
}

fn pager_item_view(data: &PagerItemNode) -> PagerItemView {
    PagerItemView {
        appearance: match data.appearance {
            PagerAppearanceValue::Dots => PagerComponentAppearance::Dots,
            PagerAppearanceValue::Numbers => PagerComponentAppearance::Numbers,
        },
        label: data.label.clone(),
        active: data.active,
        inactive: data.inactive,
        occupied: data.occupied,
        urgent: data.urgent,
    }
}

fn to_orientation(value: OrientationValue) -> gtk::Orientation {
    match value {
        OrientationValue::Horizontal => gtk::Orientation::Horizontal,
        OrientationValue::Vertical => gtk::Orientation::Vertical,
    }
}

fn to_align(value: AlignValue) -> gtk::Align {
    match value {
        AlignValue::Fill => gtk::Align::Fill,
        AlignValue::Start => gtk::Align::Start,
        AlignValue::End => gtk::Align::End,
        AlignValue::Center => gtk::Align::Center,
        AlignValue::Baseline => gtk::Align::Baseline,
    }
}

fn to_content_fit(value: ContentFitValue) -> gtk::ContentFit {
    match value {
        ContentFitValue::Fill => gtk::ContentFit::Fill,
        ContentFitValue::Contain => gtk::ContentFit::Contain,
        ContentFitValue::Cover => gtk::ContentFit::Cover,
        ContentFitValue::ScaleDown => gtk::ContentFit::ScaleDown,
    }
}

fn to_level_bar_mode(value: LevelBarModeValue) -> gtk::LevelBarMode {
    match value {
        LevelBarModeValue::Continuous => gtk::LevelBarMode::Continuous,
        LevelBarModeValue::Discrete => gtk::LevelBarMode::Discrete,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::test_support::gtk_available_on_this_thread;
    use std::cell::RefCell;
    use std::collections::BTreeMap;
    use std::{fs, path::PathBuf};

    #[test]
    fn golden_widget_fixtures_render_without_errors() {
        if !gtk_available_on_this_thread() {
            return;
        }

        let fixtures_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("sdk")
            .join("fixtures")
            .join("widgets");
        let mut fixtures = fs::read_dir(&fixtures_dir)
            .unwrap_or_else(|error| {
                panic!(
                    "read widget fixtures directory {}: {error}",
                    fixtures_dir.display()
                )
            })
            .collect::<Result<Vec<_>, _>>()
            .unwrap_or_else(|error| panic!("read widget fixture entry: {error}"));
        fixtures.sort_by_key(|entry| entry.file_name());

        let renderer = RenderCatalog::new(Rc::new(|_| {}));
        let mut failures = Vec::new();

        for entry in fixtures {
            let path = entry.path();
            if path.extension().and_then(|extension| extension.to_str()) != Some("json") {
                continue;
            }

            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("<unknown>")
                .to_string();
            let contents = match fs::read_to_string(&path) {
                Ok(contents) => contents,
                Err(error) => {
                    failures.push(format!("{name}: read failed: {error}"));
                    continue;
                }
            };
            let node = match serde_json::from_str::<TreeNode>(&contents) {
                Ok(node) => node,
                Err(error) => {
                    failures.push(format!("{name}: decode failed: {error}"));
                    continue;
                }
            };

            if let Err(error) = renderer.render(&node) {
                failures.push(format!("{name}: render failed: {error:?}"));
            }
        }

        assert!(
            failures.is_empty(),
            "golden widget fixtures should render without errors:\n{}",
            failures.join("\n")
        );
    }

    #[test]
    fn common_props_inline_styles_do_not_add_generated_css_class() {
        if !gtk_available_on_this_thread() {
            return;
        }

        let mut styles = BTreeMap::new();
        styles.insert("font-weight".into(), "600".into());
        styles.insert("margin-top".into(), "2px".into());

        let renderer = RenderCatalog::new(Rc::new(|_| {}));
        let label = renderer
            .render(&TreeNode::Label(LabelNode {
                common: CommonProps {
                    styles,
                    ..Default::default()
                },
                text: "Styled".into(),
                wrap: false,
                xalign: None,
                selectable: false,
                variant: None,
            }))
            .expect("label should render")
            .downcast::<gtk::Label>()
            .expect("label root should be gtk::Label");

        assert!(
            label
                .css_classes()
                .iter()
                .all(|class| !class.as_str().starts_with("glimpse-exec-inline-style-")),
            "did not expect generated inline style class"
        );
    }

    #[test]
    fn container_size_request_clamps_external_negative_values() {
        assert_eq!(size_request_value(None), -1);
        assert_eq!(size_request_value(Some(-20)), -1);
        assert_eq!(size_request_value(Some(-1)), -1);
        assert_eq!(size_request_value(Some(0)), 0);
        assert_eq!(size_request_value(Some(24)), 24);
    }

    #[test]
    fn container_tokens_map_to_css_variables() {
        assert_eq!(color_css(ColorValue::Border), "var(--color-border)");
        assert_eq!(
            border_width_css(BorderWidthValue::Thin),
            "var(--border-width-thin)"
        );
    }

    #[test]
    fn buttons_require_ids_for_events() {
        if !gtk_available_on_this_thread() {
            return;
        }

        let renderer = RenderCatalog::new(Rc::new(|_| {}));
        let result = renderer.render(&TreeNode::Button(ButtonNode {
            common: CommonProps::default(),
            id: String::new(),
            label: Some("Run".into()),
            icon: None,
            enabled: true,
            variant: ButtonVariant::Flat,
        }));

        assert_eq!(
            result.err(),
            Some(RenderError::MissingId {
                widget_type: "button"
            })
        );
    }

    #[test]
    fn interactive_meters_require_ids_for_events() {
        if !gtk_available_on_this_thread() {
            return;
        }

        let renderer = RenderCatalog::new(Rc::new(|_| {}));
        let result = renderer.render(&TreeNode::Meter(MeterNode {
            common: CommonProps::default(),
            id: None,
            icon: None,
            label: "Volume".into(),
            value: 0.5,
            min: 0.0,
            max: 1.0,
            step: 0.01,
            text: None,
            interactive: true,
        }));

        assert_eq!(
            result.err(),
            Some(RenderError::MissingId {
                widget_type: "meter"
            })
        );
    }

    #[test]
    fn item_renders_list_item_class_contract() {
        if !gtk_available_on_this_thread() {
            return;
        }

        let renderer = RenderCatalog::new(Rc::new(|_| {}));
        let item = renderer
            .render(&TreeNode::Item(ItemNode {
                common: CommonProps::default(),
                left: Some(Box::new(TreeNode::Icon(IconNode {
                    common: CommonProps::default(),
                    icon: "network-wireless-symbolic".into(),
                    pixel_size: Some(16),
                }))),
                label: "Wi-Fi".into(),
                sublabel: "Connected".into(),
                right: Some(Box::new(TreeNode::Badge(BadgeNode {
                    common: CommonProps::default(),
                    label: "home-5G".into(),
                    variant: None,
                }))),
            }))
            .expect("item should render")
            .downcast::<gtk::Box>()
            .expect("item root should be a box");

        assert!(item.has_css_class("list-item"));
        assert!(item.has_css_class("list-item__button"));

        let content = item
            .first_child()
            .and_downcast::<gtk::Box>()
            .expect("item should contain content box");
        assert!(content.has_css_class("list-item__content"));

        let left = content
            .first_child()
            .and_downcast::<gtk::Box>()
            .expect("item should contain left slot");
        assert!(left.has_css_class("list-item__left"));

        let text = left
            .next_sibling()
            .and_downcast::<gtk::Box>()
            .expect("item should contain text box");
        assert!(text.has_css_class("list-item__text"));

        let label = text
            .first_child()
            .and_downcast::<gtk::Label>()
            .expect("item should contain label");
        assert!(label.has_css_class("list-item__label"));

        let sublabel = label
            .next_sibling()
            .and_downcast::<gtk::Label>()
            .expect("item should contain sublabel");
        assert!(sublabel.has_css_class("list-item__secondary_label"));

        let right = text
            .next_sibling()
            .and_downcast::<gtk::Box>()
            .expect("item should contain right slot");
        assert!(right.has_css_class("list-item__right"));
        assert!(right.first_child().is_some());
    }

    #[test]
    fn action_item_renders_clickable_list_item_with_render_only_right_slot() {
        if !gtk_available_on_this_thread() {
            return;
        }

        let events = Rc::new(RefCell::new(Vec::new()));
        let event_sink = {
            let events = events.clone();
            Rc::new(move |event| events.borrow_mut().push(event))
        };
        let renderer = RenderCatalog::new(event_sink);
        let action_item = renderer
            .render(&TreeNode::ActionItem(ActionItemNode {
                common: CommonProps::default(),
                id: "wifi".into(),
                left: Some(Box::new(TreeNode::Icon(IconNode {
                    common: CommonProps::default(),
                    icon: "network-wireless-symbolic".into(),
                    pixel_size: Some(16),
                }))),
                label: "Wi-Fi".into(),
                sublabel: "Connected".into(),
                enabled: true,
                right: Some(Box::new(TreeNode::Button(ButtonNode {
                    common: CommonProps::default(),
                    id: "nested".into(),
                    label: None,
                    icon: Some("go-next-symbolic".into()),
                    enabled: true,
                    variant: ButtonVariant::Flat,
                }))),
            }))
            .expect("action item should render")
            .downcast::<gtk::Button>()
            .expect("action item root should be a button");

        assert!(action_item.has_css_class("list-item"));
        assert!(action_item.has_css_class("list-item__button"));

        let content = action_item
            .child()
            .and_downcast::<gtk::Box>()
            .expect("action item should contain row content");
        assert!(content.has_css_class("list-item__content"));

        let right = content
            .last_child()
            .and_downcast::<gtk::Box>()
            .expect("action item should contain a right slot");
        assert!(right.has_css_class("list-item__right"));

        let nested = right
            .first_child()
            .and_downcast::<gtk::Button>()
            .expect("right slot should render nested button visually");
        nested.emit_clicked();
        assert!(events.borrow().is_empty());

        action_item.emit_clicked();
        let events = events.borrow();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].id, "wifi");
        assert_eq!(events[0].kind, EventKind::Click);
    }

    #[test]
    fn picture_renders_gtk_picture_with_content_fit() {
        if !gtk_available_on_this_thread() {
            return;
        }

        let renderer = RenderCatalog::new(Rc::new(|_| {}));
        let picture = renderer
            .render(&TreeNode::Picture(PictureNode {
                common: CommonProps::default(),
                path: "/home/me/photo.png".into(),
                content_fit: ContentFitValue::Cover,
            }))
            .expect("picture should render")
            .downcast::<gtk::Picture>()
            .expect("picture root should be gtk::Picture");

        assert!(picture.has_css_class("picture"));
        assert_eq!(picture.content_fit(), gtk::ContentFit::Cover);
    }

    #[test]
    fn link_button_renders_uri_and_label() {
        if !gtk_available_on_this_thread() {
            return;
        }

        let renderer = RenderCatalog::new(Rc::new(|_| {}));
        let link = renderer
            .render(&TreeNode::LinkButton(LinkButtonNode {
                common: CommonProps::default(),
                uri: "https://example.com/docs".into(),
                label: Some("Docs".into()),
            }))
            .expect("link button should render")
            .downcast::<gtk::LinkButton>()
            .expect("link button root should be gtk::LinkButton");

        assert!(link.has_css_class("link-button"));
        assert_eq!(link.uri().as_str(), "https://example.com/docs");
        assert_eq!(link.label().as_deref(), Some("Docs"));
    }

    #[test]
    fn expander_renders_label_expanded_state_and_child() {
        if !gtk_available_on_this_thread() {
            return;
        }

        let renderer = RenderCatalog::new(Rc::new(|_| {}));
        let expander = renderer
            .render(&TreeNode::Expander(ExpanderNode {
                common: CommonProps::default(),
                label: "Details".into(),
                expanded: true,
                child: Box::new(TreeNode::Label(LabelNode {
                    common: CommonProps::default(),
                    text: "More".into(),
                    wrap: false,
                    xalign: None,
                    selectable: false,
                    variant: None,
                })),
            }))
            .expect("expander should render")
            .downcast::<gtk::Expander>()
            .expect("expander root should be gtk::Expander");

        assert!(expander.has_css_class("expander"));
        assert_eq!(expander.label().as_deref(), Some("Details"));
        assert!(expander.is_expanded());
        let child = expander
            .child()
            .and_downcast::<gtk::Label>()
            .expect("expander child should render nested label");
        assert_eq!(child.label().as_str(), "More");
    }

    #[test]
    fn level_bar_renders_value_range_and_mode() {
        if !gtk_available_on_this_thread() {
            return;
        }

        let renderer = RenderCatalog::new(Rc::new(|_| {}));
        let level_bar = renderer
            .render(&TreeNode::LevelBar(LevelBarNode {
                common: CommonProps::default(),
                value: 0.7,
                min: 0.0,
                max: 1.0,
                mode: LevelBarModeValue::Continuous,
            }))
            .expect("level bar should render")
            .downcast::<gtk::LevelBar>()
            .expect("level bar root should be gtk::LevelBar");

        assert!(level_bar.has_css_class("level-bar"));
        assert_eq!(level_bar.value(), 0.7);
        assert_eq!(level_bar.min_value(), 0.0);
        assert_eq!(level_bar.max_value(), 1.0);
        assert_eq!(level_bar.mode(), gtk::LevelBarMode::Continuous);
    }

    #[test]
    fn toggle_button_renders_and_emits_toggle_event() {
        if !gtk_available_on_this_thread() {
            return;
        }

        let events = Rc::new(RefCell::new(Vec::new()));
        let event_sink = {
            let events = events.clone();
            Rc::new(move |event| events.borrow_mut().push(event))
        };
        let renderer = RenderCatalog::new(event_sink);
        let toggle = renderer
            .render(&TreeNode::ToggleButton(ToggleButtonNode {
                common: CommonProps::default(),
                id: "wifi".into(),
                label: Some("Wi-Fi".into()),
                icon: None,
                active: false,
            }))
            .expect("toggle button should render")
            .downcast::<gtk::ToggleButton>()
            .expect("toggle button root should be gtk::ToggleButton");

        assert!(toggle.has_css_class("toggle-button"));
        assert_eq!(toggle.label().as_deref(), Some("Wi-Fi"));
        assert!(!toggle.is_active());

        toggle.set_active(true);
        let events = events.borrow();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].id, "wifi");
        assert_eq!(events[0].kind, EventKind::Toggle);
        assert_eq!(events[0].active, Some(true));
    }
}
