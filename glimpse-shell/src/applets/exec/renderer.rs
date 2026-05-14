use std::rc::Rc;

use relm4::{
    WidgetTemplate,
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
    ActionItemNode, AlignValue, BadgeNode, BoxNode, ButtonNode, ButtonVariant, CardNode,
    CheckboxNode, CommonProps, ContentFitValue, CopyableNode, EmptyStateNode, EventKind,
    EventPayload, EventSource, ExpanderNode, GridNode, HeroNode, Icon, IconNode, ImageNode,
    ItemNode, LabelNode, LayoutNode, LevelBarModeValue, LevelBarNode, LinkButtonNode, ListBoxNode,
    MenuButtonNode, MeterNode, OrientationValue, OverlayNode, PagerAppearanceValue, PagerItemNode,
    PagerStripNode, PictureNode, ProgressNode, PropertyListNode, ScrollNode, SectionNode,
    SelectNode, SeparatorNode, SliderNode, SpinnerNode, StatusNode, SwitchNode, ToggleButtonNode,
    TreeExpanderNode, TreeNode,
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
            TreeNode::Box(data) => self.render_box(data),
            TreeNode::Grid(data) => self.render_grid(data),
            TreeNode::Scroll(data) => self.render_scroll(data),
            TreeNode::Overlay(data) => self.render_overlay(data),
            TreeNode::ListBox(data) => self.render_list_box(data),
            TreeNode::LevelBar(data) => Ok(self.render_level_bar(data).upcast()),
            TreeNode::Progress(data) => Ok(self.render_progress(data).upcast()),
            TreeNode::Separator(data) => Ok(self.render_separator(data).upcast()),
            TreeNode::Label(data) => Ok(self.render_label(data).upcast()),
            TreeNode::Icon(data) => Ok(self.render_icon(data).upcast()),
            TreeNode::Image(data) => Ok(self.render_image(data).upcast()),
            TreeNode::Picture(data) => Ok(self.render_picture(data).upcast()),
            TreeNode::Button(data) => self.render_button(data),
            TreeNode::LinkButton(data) => Ok(self.render_link_button(data).upcast()),
            TreeNode::Expander(data) => self.render_expander(data),
            TreeNode::TreeExpander(data) => self.render_tree_expander(data),
            TreeNode::MenuButton(data) => self.render_menu_button(data),
            TreeNode::Switch(data) => self.render_switch(data),
            TreeNode::ToggleButton(data) => self.render_toggle_button(data),
            TreeNode::Checkbox(data) => self.render_checkbox(data),
            TreeNode::Slider(data) => self.render_slider(data),
            TreeNode::Select(data) => self.render_select(data),
        }
    }

    fn render_hero(&self, data: &HeroNode) -> Result<gtk::Widget, RenderError> {
        let hero = HeroView::init(());
        hero.title.set_label(&data.title);
        hero.subtitle.set_label(&data.subtitle);
        hero.subtitle.set_visible(!data.subtitle.is_empty());
        hero.icon.set_visible(data.icon.is_some());
        if let Some(icon) = &data.icon {
            apply_icon_to_image(&hero.icon, icon);
        }
        apply_common_props(hero.as_ref(), &data.common);
        Ok(hero.as_ref().clone().upcast())
    }

    fn render_card(&self, data: &CardNode) -> Result<gtk::Widget, RenderError> {
        let card = CardSurface::init(());
        apply_common_props(card.as_ref(), &data.common);
        for child in &data.children {
            card.body.append(&self.render(child)?);
        }
        Ok(card.as_ref().clone().upcast())
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
        for child in &data.children {
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
            let id = require_id("meter", &data.common)?;
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
            &data.icon,
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
        let id = require_id("action_item", &data.common)?;
        let root = gtk::Button::new();
        root.add_css_class("flat");
        root.add_css_class("list-item");
        root.add_css_class("list-item__button");
        root.set_hexpand(false);
        root.set_sensitive(data.enabled);

        let inert_renderer = RenderCatalog::new(Rc::new(|_| {}));
        let content = build_item_content(
            &data.icon,
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
        badge.as_ref().clone()
    }

    fn render_status(&self, data: &StatusNode) -> gtk::Box {
        let dot = StatusDotView::init(());
        dot.add_css_class("status");
        apply_common_props(dot.as_ref(), &data.common);
        dot.as_ref().clone()
    }

    fn render_pager_item(&self, data: &PagerItemNode) -> gtk::Box {
        let item = static_pager_item(&pager_item_view(data));
        if let Some(id) = data.common.id.clone().filter(|id| !id.is_empty()) {
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
                if let Some(id) = item.common.id.clone().filter(|id| !id.is_empty()) {
                    connect_widget_click(&item_box, self.event.clone(), id);
                }
                apply_common_props(&item_box, &item.common);
            }
            child = widget.next_sibling();
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

    fn render_box(&self, data: &BoxNode) -> Result<gtk::Widget, RenderError> {
        let root = gtk::Box::new(to_orientation(data.orientation), data.spacing);
        root.add_css_class(match data.orientation {
            OrientationValue::Horizontal => "row",
            OrientationValue::Vertical => "column",
        });
        apply_common_props(&root, &data.common);
        for child in &data.children {
            root.append(&self.render(child)?);
        }
        Ok(root.upcast())
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

    fn render_overlay(&self, data: &OverlayNode) -> Result<gtk::Widget, RenderError> {
        let overlay = gtk::Overlay::new();
        overlay.add_css_class("overlay");
        let child = self.render(&data.child)?;
        overlay.set_child(Some(&child));
        for node in &data.overlays {
            let overlaid = self.render(node)?;
            overlay.add_overlay(&overlaid);
        }
        apply_common_props(&overlay, &data.common);
        Ok(overlay.upcast())
    }

    fn render_list_box(&self, data: &ListBoxNode) -> Result<gtk::Widget, RenderError> {
        let list_box = gtk::ListBox::new();
        list_box.add_css_class("list-box");
        for child in &data.children {
            let row = gtk::ListBoxRow::new();
            row.set_selectable(false);
            row.set_activatable(false);
            row.set_child(Some(&self.render(child)?));
            list_box.append(&row);
        }
        apply_common_props(&list_box, &data.common);
        Ok(list_box.upcast())
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

    fn render_image(&self, data: &ImageNode) -> gtk::Image {
        let image = gtk::Image::new();
        image.add_css_class("image");
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
        let id = require_id("button", &data.common)?;
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

    fn render_tree_expander(&self, data: &TreeExpanderNode) -> Result<gtk::Widget, RenderError> {
        let tree_expander = gtk::TreeExpander::new();
        tree_expander.add_css_class("tree-expander");
        tree_expander.set_hide_expander(data.hide_expander);
        tree_expander.set_indent_for_depth(data.indent_for_depth);
        tree_expander.set_indent_for_icon(data.indent_for_icon);
        let child = self.render(&data.child)?;
        tree_expander.set_child(Some(&child));
        apply_common_props(&tree_expander, &data.common);
        Ok(tree_expander.upcast())
    }

    fn render_menu_button(&self, data: &MenuButtonNode) -> Result<gtk::Widget, RenderError> {
        let menu_button = gtk::MenuButton::new();
        menu_button.add_css_class("menu-button");

        if data.label.is_some() || data.icon.is_some() {
            let content = gtk::Box::new(gtk::Orientation::Horizontal, 6);
            content.add_css_class("menu-button__content");
            content.set_valign(gtk::Align::Center);
            if let Some(icon) = &data.icon {
                content.append(&gtk::Image::from_icon_name(icon));
            }
            if let Some(label) = &data.label {
                content.append(&gtk::Label::new(Some(label)));
            }
            menu_button.set_child(Some(&content));
        }

        let popover = gtk::Popover::new();
        popover.add_css_class("menu-button__popover");
        popover.set_child(Some(&self.render(&data.popover)?));
        menu_button.set_popover(Some(&popover));
        apply_common_props(&menu_button, &data.common);
        Ok(menu_button.upcast())
    }

    fn render_switch(&self, data: &SwitchNode) -> Result<gtk::Widget, RenderError> {
        let id = require_id("switch", &data.common)?;
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
        let id = require_id("toggle_button", &data.common)?;
        let toggle = if let Some(label) = &data.label {
            gtk::ToggleButton::with_label(label)
        } else {
            gtk::ToggleButton::new()
        };
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
        let id = require_id("checkbox", &data.common)?;
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
        let id = require_id("slider", &data.common)?;
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
        let id = require_id("select", &data.common)?;
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

pub fn apply_icon_to_image(image: &gtk::Image, icon: &Icon) {
    match icon {
        Icon::Name { name } => image.set_icon_name(Some(name)),
        Icon::Path { path } => image.set_from_file(Some(path)),
    }
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
    icon: &str,
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
    left.set_visible(!icon.is_empty());
    if !icon.is_empty() {
        let image = gtk::Image::from_icon_name(icon);
        image.set_pixel_size(16);
        left.append(&image);
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

fn require_id(widget_type: &'static str, props: &CommonProps) -> Result<String, RenderError> {
    props
        .id
        .clone()
        .filter(|id| !id.is_empty())
        .ok_or(RenderError::MissingId { widget_type })
}

fn apply_common_props(widget: &impl IsA<gtk::Widget>, props: &CommonProps) {
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
    if let Some(class_name) = props.variant.and_then(|variant| variant.class_name()) {
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

    #[test]
    fn buttons_require_ids_for_events() {
        if !gtk_available_on_this_thread() {
            return;
        }

        let renderer = RenderCatalog::new(Rc::new(|_| {}));
        let result = renderer.render(&TreeNode::Button(ButtonNode {
            common: CommonProps::default(),
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
                icon: "network-wireless-symbolic".into(),
                label: "Wi-Fi".into(),
                sublabel: "Connected".into(),
                right: Some(Box::new(TreeNode::Badge(BadgeNode {
                    common: CommonProps::default(),
                    label: "home-5G".into(),
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
                common: CommonProps {
                    id: Some("wifi".into()),
                    ..CommonProps::default()
                },
                icon: "network-wireless-symbolic".into(),
                label: "Wi-Fi".into(),
                sublabel: "Connected".into(),
                enabled: true,
                right: Some(Box::new(TreeNode::Button(ButtonNode {
                    common: CommonProps {
                        id: Some("nested".into()),
                        ..CommonProps::default()
                    },
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
    fn overlay_renders_base_child_and_overlays() {
        if !gtk_available_on_this_thread() {
            return;
        }

        let renderer = RenderCatalog::new(Rc::new(|_| {}));
        let overlay = renderer
            .render(&TreeNode::Overlay(OverlayNode {
                common: CommonProps::default(),
                child: Box::new(TreeNode::Label(LabelNode {
                    common: CommonProps::default(),
                    text: "Base".into(),
                    wrap: false,
                    xalign: None,
                    selectable: false,
                })),
                overlays: vec![TreeNode::Badge(BadgeNode {
                    common: CommonProps::default(),
                    label: "Top".into(),
                })],
            }))
            .expect("overlay should render")
            .downcast::<gtk::Overlay>()
            .expect("overlay root should be gtk::Overlay");

        assert!(overlay.has_css_class("overlay"));
        let base = overlay
            .child()
            .and_downcast::<gtk::Label>()
            .expect("overlay child should be a label");
        assert_eq!(base.label().as_str(), "Base");

        let overlaid = overlay
            .last_child()
            .and_downcast::<gtk::Label>()
            .expect("overlay should expose the overlaid badge as a label child");
        assert!(overlaid.has_css_class("badge"));
        assert_eq!(overlaid.label().as_str(), "Top");
    }

    #[test]
    fn list_box_renders_each_child_in_a_row() {
        if !gtk_available_on_this_thread() {
            return;
        }

        let renderer = RenderCatalog::new(Rc::new(|_| {}));
        let list_box = renderer
            .render(&TreeNode::ListBox(ListBoxNode {
                common: CommonProps::default(),
                children: vec![
                    TreeNode::Label(LabelNode {
                        common: CommonProps::default(),
                        text: "First".into(),
                        wrap: false,
                        xalign: None,
                        selectable: false,
                    }),
                    TreeNode::Badge(BadgeNode {
                        common: CommonProps::default(),
                        label: "Second".into(),
                    }),
                ],
            }))
            .expect("list box should render")
            .downcast::<gtk::ListBox>()
            .expect("list box root should be gtk::ListBox");

        assert!(list_box.has_css_class("list-box"));
        let first_row = list_box
            .first_child()
            .and_downcast::<gtk::ListBoxRow>()
            .expect("first child should be a list box row");
        let first_label = first_row
            .child()
            .and_downcast::<gtk::Label>()
            .expect("first row should contain the rendered label");
        assert_eq!(first_label.label().as_str(), "First");

        let last_row = list_box
            .last_child()
            .and_downcast::<gtk::ListBoxRow>()
            .expect("last child should be a list box row");
        let badge = last_row
            .child()
            .and_downcast::<gtk::Label>()
            .expect("last row should contain the rendered badge");
        assert!(badge.has_css_class("badge"));
        assert_eq!(badge.label().as_str(), "Second");
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
    fn tree_expander_renders_child_and_flags() {
        if !gtk_available_on_this_thread() {
            return;
        }

        let renderer = RenderCatalog::new(Rc::new(|_| {}));
        let tree_expander = renderer
            .render(&TreeNode::TreeExpander(TreeExpanderNode {
                common: CommonProps::default(),
                child: Box::new(TreeNode::Label(LabelNode {
                    common: CommonProps::default(),
                    text: "Nested".into(),
                    wrap: false,
                    xalign: None,
                    selectable: false,
                })),
                hide_expander: true,
                indent_for_depth: true,
                indent_for_icon: true,
            }))
            .expect("tree expander should render")
            .downcast::<gtk::TreeExpander>()
            .expect("tree expander root should be gtk::TreeExpander");

        assert!(tree_expander.has_css_class("tree-expander"));
        assert!(tree_expander.hides_expander());
        assert!(tree_expander.is_indent_for_depth());
        assert!(tree_expander.is_indent_for_icon());
        let child = tree_expander
            .child()
            .and_downcast::<gtk::Label>()
            .expect("tree expander child should render nested label");
        assert_eq!(child.label().as_str(), "Nested");
    }

    #[test]
    fn menu_button_renders_content_and_popover() {
        if !gtk_available_on_this_thread() {
            return;
        }

        let renderer = RenderCatalog::new(Rc::new(|_| {}));
        let menu_button = renderer
            .render(&TreeNode::MenuButton(MenuButtonNode {
                common: CommonProps::default(),
                label: Some("More".into()),
                icon: Some("open-menu-symbolic".into()),
                popover: Box::new(TreeNode::Label(LabelNode {
                    common: CommonProps::default(),
                    text: "Menu content".into(),
                    wrap: false,
                    xalign: None,
                    selectable: false,
                })),
            }))
            .expect("menu button should render")
            .downcast::<gtk::MenuButton>()
            .expect("menu button root should be gtk::MenuButton");

        assert!(menu_button.has_css_class("menu-button"));
        assert!(menu_button.child().is_some());
        let popover = menu_button
            .popover()
            .expect("menu button should have popover");
        assert!(popover.has_css_class("menu-button__popover"));
        let child = popover
            .child()
            .and_downcast::<gtk::Label>()
            .expect("popover child should render nested label");
        assert_eq!(child.label().as_str(), "Menu content");
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
                common: CommonProps {
                    id: Some("wifi".into()),
                    ..CommonProps::default()
                },
                label: Some("Wi-Fi".into()),
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
