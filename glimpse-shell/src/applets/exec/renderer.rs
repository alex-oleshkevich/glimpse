use std::{collections::HashSet, rc::Rc};

use chrono::NaiveDate;
use glimpse_core::services::{
    calendar_events::{CalendarEvent, model::CalendarSource},
    clock::WorldClockTime,
};
use relm4::gtk::{self, prelude::*};
use serde_json::json;

use crate::{
    widgets::pager::{PagerAppearance as PagerComponentAppearance, PagerItemView},
    widgets::{
        Meter as GtkMeter, Scroll as GtkScroll, Separator as GtkSeparator, Spinner as GtkSpinner,
        badge::{Badge, BadgeKind},
        battery_hero::BatteryHero,
        boxed_list::BoxedList,
        button_row::ButtonRow,
        calendar::Calendar,
        camera_indicator::CameraIndicator,
        choice_list::ChoiceList,
        choice_tile::ChoiceTile,
        circle_box::CircleBox,
        column::Column,
        container::Container,
        date_hero::DateHero,
        empty_state::EmptyState,
        events::Events,
        expander_tile::ExpanderTile,
        header::Header,
        hero::Hero,
        key_value_grid::KeyValueGrid,
        location_indicator::LocationIndicator,
        mic_indicator::MicIndicator,
        muted_indicator::MutedIndicator,
        pager_item::PagerItem,
        pager_strip::{PagerStrip, PagerStripEntry},
        panel_indicator::PanelIndicator,
        popover_shell::PopoverShell,
        row::Row,
        screencast_indicator::ScreenCastIndicator,
        segmented_tile::SegmentedTile,
        slider_tile::SliderTile,
        status_dot::{StatusDot, StatusDotStatus},
        switch_tile::SwitchTile,
        tile::Tile,
        weather_forecast_list::{WeatherForecastItem, WeatherForecastList},
        weather_hourly_strip::{WeatherHourlyItem, WeatherHourlyStrip},
        world_clock::WorldClock,
    },
};

use super::protocol::{
    ActiveIndicatorNode, BadgeKindValue, BadgeNode, BatteryHeroNode, CalendarNode, ChildrenNode,
    ChoiceListNode, ChoiceTileNode, CircleBoxNode, CommonProps, DateHeroNode, EmptyStateNode,
    EventItemNode, EventKind, EventPayload, EventSource, EventsNode, ExpanderTileNode, HeaderNode,
    HeroNode, KeyValueGridNode, LabelNode, MeterNode, MouseButton, PagerAppearanceValue,
    PagerItemNode, PagerStripNode, PanelIndicatorNode, PopoverShellNode, ScreenCastIndicatorNode,
    ScrollNode, SegmentedTileNode, SeparatorNode, SliderTileNode, SpinnerNode, StatusDotNode,
    StatusDotStatusValue, SwitchTileNode, TileNode, TreeNode, WeatherForecastListNode,
    WeatherHourlyStripNode, WorldClockNode,
};

pub type EventSink = Rc<dyn Fn(EventPayload)>;

pub struct RenderCatalog {
    event: EventSink,
}

impl RenderCatalog {
    pub fn new(event: EventSink) -> Self {
        Self { event }
    }

    pub fn render(&self, node: &TreeNode) -> Result<gtk::Widget, RenderError> {
        match node {
            TreeNode::Row(data) => self.render_children_box(Row::new(), data),
            TreeNode::Column(data) => self.render_children_box(Column::new(), data),
            TreeNode::Container(data) => self.render_children_box(Container::new(), data),
            TreeNode::CircleBox(data) => Ok(self.render_circle_box(data).upcast()),
            TreeNode::BoxedList(data) => self.render_children_box(BoxedList::new(), data),
            TreeNode::PopoverShell(data) => self.render_popover_shell(data),
            TreeNode::Label(data) => Ok(self.render_label(data).upcast()),
            TreeNode::Header(data) => Ok(self.render_header(data).upcast()),
            TreeNode::Hero(data) => self.render_hero(data),
            TreeNode::Badge(data) => Ok(self.render_badge(data).upcast()),
            TreeNode::StatusDot(data) => Ok(self.render_status_dot(data).upcast()),
            TreeNode::PanelIndicator(data) => self.render_panel_indicator(data),
            TreeNode::EmptyState(data) => Ok(self.render_empty_state(data).upcast()),
            TreeNode::Spinner(data) => Ok(self.render_spinner(data).upcast()),
            TreeNode::Meter(data) => Ok(self.render_meter(data).upcast()),
            TreeNode::Separator(data) => Ok(self.render_separator(data).upcast()),
            TreeNode::Scroll(data) => self.render_scroll(data),
            TreeNode::Tile(data) => self.render_tile(data),
            TreeNode::SegmentedTile(data) => self.render_segmented_tile(data),
            TreeNode::ButtonRow(data) => self.render_children_box(ButtonRow::new(), data),
            TreeNode::SwitchTile(data) => self.render_switch_tile(data),
            TreeNode::ExpanderTile(data) => self.render_expander_tile(data),
            TreeNode::SliderTile(data) => self.render_slider_tile(data),
            TreeNode::ChoiceTile(data) => self.render_choice_tile(data),
            TreeNode::ChoiceList(data) => self.render_choice_list(data),
            TreeNode::KeyValueGrid(data) => Ok(self.render_key_value_grid(data).upcast()),
            TreeNode::PagerItem(data) => self.render_pager_item(data),
            TreeNode::PagerStrip(data) => self.render_pager_strip(data),
            TreeNode::CameraIndicator(data) => {
                Ok(render_active_indicator(CameraIndicator::new(), data).upcast())
            }
            TreeNode::MicIndicator(data) => {
                Ok(render_active_indicator(MicIndicator::new(), data).upcast())
            }
            TreeNode::MutedIndicator(data) => {
                Ok(render_active_indicator(MutedIndicator::new(), data).upcast())
            }
            TreeNode::ScreenCastIndicator(data) => {
                Ok(self.render_screencast_indicator(data).upcast())
            }
            TreeNode::LocationIndicator(data) => {
                Ok(render_active_indicator(LocationIndicator::new(), data).upcast())
            }
            TreeNode::Calendar(data) => self.render_calendar(data),
            TreeNode::BatteryHero(data) => Ok(self.render_battery_hero(data).upcast()),
            TreeNode::DateHero(data) => Ok(self.render_date_hero(data).upcast()),
            TreeNode::Events(data) => self.render_events(data),
            TreeNode::WeatherForecastList(data) => {
                Ok(self.render_weather_forecast_list(data).upcast())
            }
            TreeNode::WeatherHourlyStrip(data) => {
                Ok(self.render_weather_hourly_strip(data).upcast())
            }
            TreeNode::WorldClock(data) => Ok(self.render_world_clock(data).upcast()),
        }
    }

    fn render_children_box<W>(
        &self,
        root: W,
        data: &ChildrenNode,
    ) -> Result<gtk::Widget, RenderError>
    where
        W: IsA<gtk::Box> + IsA<gtk::Widget>,
    {
        for child in &data.children {
            root.append(&self.render(child)?);
        }
        apply_common(&root, &data.common);
        Ok(root.upcast())
    }

    fn render_circle_box(&self, data: &CircleBoxNode) -> CircleBox {
        let circle = CircleBox::new();
        if !data.color.is_empty() {
            circle.set_color(&data.color);
        }
        apply_common(&circle, &data.common);
        circle
    }

    fn render_popover_shell(&self, data: &PopoverShellNode) -> Result<gtk::Widget, RenderError> {
        let shell = PopoverShell::new();
        for child in &data.children {
            shell.content().append(&self.render(child)?);
        }
        for child in &data.footer {
            shell.footer().append(&self.render(child)?);
        }
        shell.set_footer_visible(data.footer_visible || !data.footer.is_empty());
        apply_common(&shell, &data.common);
        Ok(shell.upcast())
    }

    fn render_label(&self, data: &LabelNode) -> gtk::Label {
        let label = gtk::Label::new(Some(&data.label));
        if let Some(value) = data.xalign {
            label.set_xalign(value);
        }
        if let Some(value) = data.wrap {
            label.set_wrap(value);
        }
        apply_common(&label, &data.common);
        label
    }

    fn render_header(&self, data: &HeaderNode) -> Header {
        let header = Header::new();
        header.set_label(&data.label);
        apply_common(&header, &data.common);
        header
    }

    fn render_hero(&self, data: &HeroNode) -> Result<gtk::Widget, RenderError> {
        let hero = Hero::new();
        hero.set_title(&data.title);
        hero.set_subtitle(&data.subtitle);
        hero.set_icon(data.icon.as_deref());
        if let Some(size) = data.icon_size {
            hero.set_icon_size(size);
        }
        if let Some(active) = data.toggle {
            hero.set_toggle_visible(true);
            hero.set_toggle_active(active);
            if let Some(id) = data.id.clone() {
                let event = self.event.clone();
                hero.connect_toggled(move |_, active| {
                    event(toggle_event(id.clone(), active));
                });
            }
        }
        if let Some(sensitive) = data.toggle_sensitive {
            hero.set_toggle_sensitive(sensitive);
        }
        if let Some(separator) = data.separator {
            hero.set_separator_visible(separator);
        }
        if let Some(trailing) = &data.trailing {
            hero.append_trailing(&self.render(trailing)?);
            hero.set_trailing_visible(true);
        }
        apply_common(&hero, &data.common);
        Ok(hero.upcast())
    }

    fn render_badge(&self, data: &BadgeNode) -> Badge {
        let badge = Badge::new();
        badge.set_label(&data.label);
        badge.set_kind(to_badge_kind(data.kind));
        apply_common(&badge, &data.common);
        badge
    }

    fn render_status_dot(&self, data: &StatusDotNode) -> StatusDot {
        let dot = StatusDot::new();
        dot.set_status(to_status_dot_status(data.status));
        apply_common(&dot, &data.common);
        dot
    }

    fn render_panel_indicator(
        &self,
        data: &PanelIndicatorNode,
    ) -> Result<gtk::Widget, RenderError> {
        let indicator = PanelIndicator::new();
        indicator.set_icon(data.icon.as_deref());
        indicator.set_label(data.label.as_deref());
        indicator.set_active(data.active);
        indicator.set_checked(data.checked);
        indicator.set_needs_attention(data.needs_attention);
        if let Some(extra) = &data.extra {
            indicator.append_extra(&self.render(extra)?);
        }
        if let Some(id) = data.id.clone() {
            let event = self.event.clone();
            indicator.connect_activated(move |_| {
                event(click_event(id.clone(), None));
            });
        }
        apply_common(&indicator, &data.common);
        Ok(indicator.upcast())
    }

    fn render_empty_state(&self, data: &EmptyStateNode) -> EmptyState {
        let empty = EmptyState::new();
        empty.set_title(&data.title);
        empty.set_subtitle(data.subtitle.as_deref());
        apply_common(&empty, &data.common);
        empty
    }

    fn render_spinner(&self, data: &SpinnerNode) -> GtkSpinner {
        let spinner = GtkSpinner::new();
        spinner.set_spinning(data.spinning);
        apply_common(&spinner, &data.common);
        spinner
    }

    fn render_meter(&self, data: &MeterNode) -> GtkMeter {
        let meter = GtkMeter::new();
        let range = data.max - data.min;
        let fraction = if range > 0.0 {
            ((data.value - data.min) / range).clamp(0.0, 1.0)
        } else {
            0.0
        };
        meter.set_fraction(fraction);
        let label = data.text.as_deref().or_else(|| {
            if data.label.is_empty() {
                None
            } else {
                Some(data.label.as_str())
            }
        });
        meter.set_show_text(label.is_some());
        meter.set_text(label);
        apply_common(&meter, &data.common);
        meter
    }

    fn render_separator(&self, data: &SeparatorNode) -> GtkSeparator {
        let separator = GtkSeparator::new(gtk::Orientation::Horizontal);
        apply_common(&separator, &data.common);
        separator
    }

    fn render_scroll(&self, data: &ScrollNode) -> Result<gtk::Widget, RenderError> {
        let scroll = GtkScroll::new();
        scroll.set_policy(gtk::PolicyType::Automatic, gtk::PolicyType::Automatic);
        scroll.set_child(Some(&self.render(&data.child)?));
        apply_common(&scroll, &data.common);
        Ok(scroll.upcast())
    }

    fn render_tile(&self, data: &TileNode) -> Result<gtk::Widget, RenderError> {
        let tile = Tile::new();
        tile.set_primary(&data.primary);
        tile.set_secondary(data.secondary.as_deref());
        set_left_slot(&tile, data.left_icon.as_deref(), data.left.as_deref(), self)?;
        set_optional_slot(data.right.as_deref(), self, |child| tile.set_right(child))?;
        tile.set_activatable(data.activatable || data.id.is_some());
        if let Some(id) = data.id.clone() {
            let event = self.event.clone();
            tile.connect_activated(move |_| {
                event(click_event(id.clone(), None));
            });
        }
        apply_common(&tile, &data.common);
        Ok(tile.upcast())
    }

    fn render_segmented_tile(&self, data: &SegmentedTileNode) -> Result<gtk::Widget, RenderError> {
        let tile = SegmentedTile::new();
        tile.set_primary(&data.primary);
        tile.set_secondary(data.secondary.as_deref());
        set_left_slot(&tile, data.left_icon.as_deref(), data.left.as_deref(), self)?;
        set_optional_slot(data.right.as_deref(), self, |child| tile.set_right(child))?;
        set_optional_slot(data.child.as_deref(), self, |child| tile.set_child(child))?;
        tile.set_expanded(data.expanded);
        tile.set_activatable(data.activatable);
        if let Some(id) = data.id.clone() {
            if data.activatable {
                let click_id = id.clone();
                let event = self.event.clone();
                tile.connect_activated(move |_| {
                    event(click_event(click_id.clone(), None));
                });
            }
            let event = self.event.clone();
            tile.connect_expanded(move |_, expanded| {
                event(toggle_event(id.clone(), expanded));
            });
        }
        apply_common(&tile, &data.common);
        Ok(tile.upcast())
    }

    fn render_switch_tile(&self, data: &SwitchTileNode) -> Result<gtk::Widget, RenderError> {
        let tile = SwitchTile::new();
        tile.set_primary(&data.primary);
        tile.set_secondary(data.secondary.as_deref());
        set_left_slot(&tile, data.left_icon.as_deref(), data.left.as_deref(), self)?;
        tile.set_active(data.active);
        let id = data.id.clone();
        let event = self.event.clone();
        tile.connect_toggled(move |_, active| {
            event(toggle_event(id.clone(), active));
        });
        apply_common(&tile, &data.common);
        Ok(tile.upcast())
    }

    fn render_expander_tile(&self, data: &ExpanderTileNode) -> Result<gtk::Widget, RenderError> {
        let tile = ExpanderTile::new();
        tile.set_primary(&data.primary);
        tile.set_secondary(data.secondary.as_deref());
        set_left_slot(&tile, data.left_icon.as_deref(), data.left.as_deref(), self)?;
        set_optional_slot(data.child.as_deref(), self, |child| tile.set_child(child))?;
        tile.set_expanded(data.expanded);
        if let Some(id) = data.id.clone() {
            let event = self.event.clone();
            tile.connect_expanded(move |_, expanded| {
                event(toggle_event(id.clone(), expanded));
            });
        }
        apply_common(&tile, &data.common);
        Ok(tile.upcast())
    }

    fn render_slider_tile(&self, data: &SliderTileNode) -> Result<gtk::Widget, RenderError> {
        let tile = SliderTile::new();
        tile.set_label(data.label.as_deref());
        set_left_slot(&tile, data.left_icon.as_deref(), data.left.as_deref(), self)?;
        tile.set_range(data.min, data.max);
        tile.set_increments(data.step, data.page);
        tile.set_digits(data.digits);
        tile.set_snap_step(data.snap_step);
        tile.set_value(data.value);
        let id = data.id.clone();
        let event = self.event.clone();
        tile.connect_changed(move |_, value| {
            event(change_event(id.clone(), json!(value)));
        });
        apply_common(&tile, &data.common);
        Ok(tile.upcast())
    }

    fn render_choice_tile(&self, data: &ChoiceTileNode) -> Result<gtk::Widget, RenderError> {
        let tile = ChoiceTile::new();
        tile.set_primary(&data.primary);
        tile.set_secondary(data.secondary.as_deref());
        set_left_slot(&tile, data.left_icon.as_deref(), data.left.as_deref(), self)?;
        tile.set_selected(data.selected);
        if let Some(id) = data.id.clone() {
            let event = self.event.clone();
            tile.connect_activated(move |_| {
                event(click_event(id.clone(), None));
            });
        }
        apply_common(&tile, &data.common);
        Ok(tile.upcast())
    }

    fn render_choice_list(&self, data: &ChoiceListNode) -> Result<gtk::Widget, RenderError> {
        let list = ChoiceList::new();
        for choice in &data.choices {
            list.add_choice(
                &choice.id,
                &choice.primary,
                choice.secondary.as_deref(),
                choice.icon.as_deref(),
            );
        }
        if let Some(active) = &data.active {
            list.set_active(active);
        }
        let id = data.id.clone();
        let event = self.event.clone();
        list.connect_changed(move |_, value| {
            event(change_event(id.clone(), json!(value)));
        });
        apply_common(&list, &data.common);
        Ok(list.upcast())
    }

    fn render_key_value_grid(&self, data: &KeyValueGridNode) -> KeyValueGrid {
        let grid = KeyValueGrid::new();
        for row in &data.rows {
            grid.add_row(&row.key, &row.value);
        }
        apply_common(&grid, &data.common);
        grid
    }

    fn render_pager_item(&self, data: &PagerItemNode) -> Result<gtk::Widget, RenderError> {
        let item = PagerItem::new();
        item.set_view(&pager_item_view(data));
        let id = data.id;
        let event = self.event.clone();
        item.connect_activated(move |_| {
            event(click_event(id.to_string(), None));
        });
        apply_common(&item, &data.common);
        Ok(item.upcast())
    }

    fn render_pager_strip(&self, data: &PagerStripNode) -> Result<gtk::Widget, RenderError> {
        let strip = PagerStrip::new();
        strip.set_placeholder(data.placeholder);
        let entries: Vec<PagerStripEntry> = data
            .items
            .iter()
            .map(|item| PagerStripEntry {
                id: item.id as usize,
                view: pager_item_view(item),
            })
            .collect();
        strip.set_items(&entries);
        if let Some(id) = data.id.clone() {
            let event = self.event.clone();
            strip.connect_activated(move |_, item_id| {
                event(change_event(id.clone(), json!(item_id)));
            });
        }
        apply_common(&strip, &data.common);
        Ok(strip.upcast())
    }

    fn render_screencast_indicator(&self, data: &ScreenCastIndicatorNode) -> ScreenCastIndicator {
        let indicator = ScreenCastIndicator::new();
        indicator.set_active(data.active);
        if let Some(text) = &data.timer_text {
            indicator.set_timer_text(text);
        }
        apply_common(&indicator, &data.common);
        indicator
    }

    fn render_calendar(&self, data: &CalendarNode) -> Result<gtk::Widget, RenderError> {
        let calendar = Calendar::new();
        let selected = parse_date(&data.selected_date)?;
        calendar.set_selected_date(selected);
        let days = data
            .event_days
            .iter()
            .map(|date| parse_date(date))
            .collect::<Result<HashSet<_>, _>>()?;
        calendar.set_event_days(&days);
        if let Some(id) = data.id.clone() {
            let event = self.event.clone();
            calendar.connect_day_selected(move |calendar| {
                event(change_event(
                    id.clone(),
                    json!(calendar.selected_date().to_string()),
                ));
            });
        }
        apply_common(&calendar, &data.common);
        Ok(calendar.upcast())
    }

    fn render_battery_hero(&self, data: &BatteryHeroNode) -> BatteryHero {
        let hero = BatteryHero::new();
        hero.set_icon_name(&data.icon);
        hero.set_percentage(&data.percentage);
        hero.set_fraction(data.fraction);
        hero.set_state(&data.state);
        apply_common(&hero, &data.common);
        hero
    }

    fn render_date_hero(&self, data: &DateHeroNode) -> DateHero {
        let hero = DateHero::new();
        hero.set_weekday(&data.weekday);
        hero.set_date(&data.date);
        apply_common(&hero, &data.common);
        hero
    }

    fn render_events(&self, data: &EventsNode) -> Result<gtk::Widget, RenderError> {
        let events = Events::new();
        let date = parse_date(&data.date)?;
        let items = data.events.iter().map(event_item).collect::<Vec<_>>();
        events.set_data(date, &items, data.loading);
        apply_common(&events, &data.common);
        Ok(events.upcast())
    }

    fn render_weather_forecast_list(&self, data: &WeatherForecastListNode) -> WeatherForecastList {
        let list = WeatherForecastList::new();
        let items = data
            .items
            .iter()
            .map(|item| WeatherForecastItem {
                day_name: item.day_name.clone(),
                icon: item.icon.clone(),
                condition: item.condition.clone(),
                temperatures: item.temperatures.clone(),
                is_today: item.is_today,
            })
            .collect::<Vec<_>>();
        list.set_items(&items);
        apply_common(&list, &data.common);
        list
    }

    fn render_weather_hourly_strip(&self, data: &WeatherHourlyStripNode) -> WeatherHourlyStrip {
        let strip = WeatherHourlyStrip::new();
        let items = data
            .items
            .iter()
            .map(|item| WeatherHourlyItem {
                time: item.time.clone(),
                icon: item.icon.clone(),
                temperature: item.temperature.clone(),
            })
            .collect::<Vec<_>>();
        strip.set_items(&items);
        apply_common(&strip, &data.common);
        strip
    }

    fn render_world_clock(&self, data: &WorldClockNode) -> WorldClock {
        let world = WorldClock::new();
        let rows = data
            .rows
            .iter()
            .map(|row| WorldClockTime {
                name: row.name.clone(),
                timezone: row.timezone.clone(),
                time: row.time.clone(),
                offset: row.offset.clone(),
                day_label: static_day_label(row.day_label.as_deref()),
            })
            .collect::<Vec<_>>();
        world.set_rows(&rows);
        apply_common(&world, &data.common);
        world
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RenderError {
    InvalidValue { field: &'static str, value: String },
}

impl std::fmt::Display for RenderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidValue { field, value } => {
                write!(f, "invalid {field} value: {value}")
            }
        }
    }
}

impl std::error::Error for RenderError {}

fn apply_common(widget: &impl IsA<gtk::Widget>, props: &CommonProps) {
    if let Some(visible) = props.visible {
        widget.set_visible(visible);
    }
    if let Some(tooltip) = &props.tooltip {
        widget.set_tooltip_text(Some(tooltip));
    }
    for class in &props.css_classes {
        widget.add_css_class(class);
    }
}

fn set_left_slot<T>(
    widget: &T,
    icon: Option<&str>,
    child: Option<&TreeNode>,
    renderer: &RenderCatalog,
) -> Result<(), RenderError>
where
    T: LeftSlot,
{
    if let Some(child) = child {
        widget.set_left_widget(Some(renderer.render(child)?));
    } else if let Some(icon) = icon {
        widget.set_left_widget(Some(gtk::Image::from_icon_name(icon).upcast()));
    }
    Ok(())
}

fn set_optional_slot(
    child: Option<&TreeNode>,
    renderer: &RenderCatalog,
    setter: impl FnOnce(Option<gtk::Widget>),
) -> Result<(), RenderError> {
    if let Some(child) = child {
        setter(Some(renderer.render(child)?));
    }
    Ok(())
}

trait LeftSlot {
    fn set_left_widget(&self, child: Option<gtk::Widget>);
}

impl LeftSlot for Tile {
    fn set_left_widget(&self, child: Option<gtk::Widget>) {
        self.set_left(child);
    }
}

impl LeftSlot for SegmentedTile {
    fn set_left_widget(&self, child: Option<gtk::Widget>) {
        self.set_left(child);
    }
}

impl LeftSlot for SwitchTile {
    fn set_left_widget(&self, child: Option<gtk::Widget>) {
        self.set_left(child);
    }
}

impl LeftSlot for ExpanderTile {
    fn set_left_widget(&self, child: Option<gtk::Widget>) {
        self.set_left(child);
    }
}

impl LeftSlot for SliderTile {
    fn set_left_widget(&self, child: Option<gtk::Widget>) {
        self.set_left(child);
    }
}

impl LeftSlot for ChoiceTile {
    fn set_left_widget(&self, child: Option<gtk::Widget>) {
        self.set_left(child);
    }
}

fn render_active_indicator<T>(indicator: T, data: &ActiveIndicatorNode) -> T
where
    T: ActiveIndicator,
{
    indicator.set_active_state(data.active);
    apply_common(&indicator, &data.common);
    indicator
}

trait ActiveIndicator: IsA<gtk::Widget> {
    fn set_active_state(&self, active: bool);
}

impl ActiveIndicator for CameraIndicator {
    fn set_active_state(&self, active: bool) {
        self.set_active(active);
    }
}

impl ActiveIndicator for MicIndicator {
    fn set_active_state(&self, active: bool) {
        self.set_active(active);
    }
}

impl ActiveIndicator for MutedIndicator {
    fn set_active_state(&self, active: bool) {
        self.set_active(active);
    }
}

impl ActiveIndicator for LocationIndicator {
    fn set_active_state(&self, active: bool) {
        self.set_active(active);
    }
}

fn click_event(id: String, button: Option<MouseButton>) -> EventPayload {
    EventPayload {
        id,
        kind: EventKind::Click,
        source: EventSource::Popover,
        button,
        active: None,
        value: None,
        delta_y: None,
    }
}

fn toggle_event(id: String, active: bool) -> EventPayload {
    EventPayload {
        id,
        kind: EventKind::Toggle,
        source: EventSource::Popover,
        button: None,
        active: Some(active),
        value: None,
        delta_y: None,
    }
}

fn change_event(id: String, value: serde_json::Value) -> EventPayload {
    EventPayload {
        id,
        kind: EventKind::Change,
        source: EventSource::Popover,
        button: None,
        active: None,
        value: Some(value),
        delta_y: None,
    }
}

fn to_badge_kind(value: BadgeKindValue) -> BadgeKind {
    match value {
        BadgeKindValue::Default => BadgeKind::Default,
        BadgeKindValue::Success => BadgeKind::Success,
        BadgeKindValue::Warning => BadgeKind::Warning,
        BadgeKindValue::Error => BadgeKind::Error,
        BadgeKindValue::Accent => BadgeKind::Accent,
    }
}

fn to_status_dot_status(value: StatusDotStatusValue) -> StatusDotStatus {
    match value {
        StatusDotStatusValue::Neutral => StatusDotStatus::Neutral,
        StatusDotStatusValue::Success => StatusDotStatus::Success,
        StatusDotStatusValue::Warning => StatusDotStatus::Warning,
        StatusDotStatusValue::Error => StatusDotStatus::Error,
        StatusDotStatusValue::Accent => StatusDotStatus::Accent,
    }
}

fn to_pager_appearance(value: PagerAppearanceValue) -> PagerComponentAppearance {
    match value {
        PagerAppearanceValue::Dots => PagerComponentAppearance::Dots,
        PagerAppearanceValue::Numbers => PagerComponentAppearance::Numbers,
    }
}

fn pager_item_view(data: &PagerItemNode) -> PagerItemView {
    PagerItemView {
        label: data.label.clone(),
        active: data.active,
        inactive: data.inactive,
        occupied: data.occupied,
        urgent: data.urgent,
        appearance: to_pager_appearance(data.appearance),
    }
}

fn parse_date(value: &str) -> Result<NaiveDate, RenderError> {
    NaiveDate::parse_from_str(value, "%Y-%m-%d").map_err(|_| RenderError::InvalidValue {
        field: "date",
        value: value.to_string(),
    })
}

fn event_item(item: &EventItemNode) -> CalendarEvent {
    CalendarEvent {
        event_id: item.id.clone(),
        title: item.title.clone(),
        start: item.start.clone(),
        end: item.end.clone(),
        location: item.location.clone(),
        all_day: item.all_day,
        source: CalendarSource::default(),
    }
}

fn static_day_label(value: Option<&str>) -> Option<&'static str> {
    match value {
        Some("Yesterday") => Some("Yesterday"),
        Some("Today") => Some("Today"),
        Some("Tomorrow") => Some("Tomorrow"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use std::{cell::RefCell, fs, path::PathBuf};

    use super::*;
    use crate::utils::test_support::gtk_available_on_this_thread;
    use gtk::subclass::prelude::ObjectSubclassIsExt;

    fn fixtures_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../sdk/fixtures")
    }

    #[test]
    fn golden_widget_fixtures_render_without_errors() {
        if !gtk_available_on_this_thread() {
            return;
        }

        let dir = fixtures_root().join("widgets");
        let renderer = RenderCatalog::new(Rc::new(|_| {}));
        let mut failures = Vec::new();

        for entry in fs::read_dir(&dir).expect("fixtures/widgets should be readable") {
            let entry = entry.expect("fixture directory entry should be readable");
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
                continue;
            }

            let data = fs::read_to_string(&path).expect("fixture should be readable");
            let node = match serde_json::from_str::<TreeNode>(&data) {
                Ok(node) => node,
                Err(error) => {
                    failures.push(format!("{}: deserialize: {error}", path.display()));
                    continue;
                }
            };

            if let Err(error) = renderer.render(&node) {
                failures.push(format!("{}: render: {error}", path.display()));
            }
        }

        assert!(
            failures.is_empty(),
            "golden widget fixtures should render without errors:\n{}",
            failures.join("\n")
        );
    }

    #[test]
    fn switch_tile_emits_toggle_event() {
        if !gtk_available_on_this_thread() {
            return;
        }

        let events = Rc::new(RefCell::new(Vec::new()));
        let sink_events = events.clone();
        let renderer = RenderCatalog::new(Rc::new(move |event| {
            sink_events.borrow_mut().push(event);
        }));

        let widget = renderer
            .render(&TreeNode::SwitchTile(SwitchTileNode {
                common: CommonProps::default(),
                id: "vpn".into(),
                primary: "VPN".into(),
                secondary: None,
                left_icon: None,
                left: None,
                active: false,
            }))
            .expect("switch tile should render")
            .downcast::<SwitchTile>()
            .expect("root should be SwitchTile");

        widget.set_active(true);

        let events = events.borrow();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].id, "vpn");
        assert_eq!(events[0].kind, EventKind::Toggle);
        assert_eq!(events[0].active, Some(true));
    }

    #[test]
    fn segmented_tile_with_id_is_not_activatable_without_primary_action() {
        if !gtk_available_on_this_thread() {
            return;
        }

        let events = Rc::new(RefCell::new(Vec::new()));
        let sink_events = events.clone();
        let renderer = RenderCatalog::new(Rc::new(move |event| {
            sink_events.borrow_mut().push(event);
        }));

        let widget = renderer
            .render(&TreeNode::SegmentedTile(SegmentedTileNode {
                common: CommonProps::default(),
                id: Some("device".into()),
                primary: "Pixel".into(),
                secondary: None,
                left_icon: None,
                left: None,
                right: None,
                child: None,
                expanded: false,
                activatable: false,
            }))
            .expect("segmented tile should render")
            .downcast::<SegmentedTile>()
            .expect("root should be SegmentedTile");

        assert!(!widget.imp().main.has_css_class("activatable"));

        widget.emit_by_name::<()>("activated", &[]);
        assert!(events.borrow().is_empty());
    }

    #[test]
    fn segmented_tile_with_primary_action_is_activatable() {
        if !gtk_available_on_this_thread() {
            return;
        }

        let events = Rc::new(RefCell::new(Vec::new()));
        let sink_events = events.clone();
        let renderer = RenderCatalog::new(Rc::new(move |event| {
            sink_events.borrow_mut().push(event);
        }));

        let widget = renderer
            .render(&TreeNode::SegmentedTile(SegmentedTileNode {
                common: CommonProps::default(),
                id: Some("device".into()),
                primary: "Pixel".into(),
                secondary: None,
                left_icon: None,
                left: None,
                right: None,
                child: None,
                expanded: false,
                activatable: true,
            }))
            .expect("segmented tile should render")
            .downcast::<SegmentedTile>()
            .expect("root should be SegmentedTile");

        assert!(widget.imp().main.has_css_class("activatable"));

        widget.emit_by_name::<()>("activated", &[]);
        let events = events.borrow();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].id, "device");
        assert_eq!(events[0].kind, EventKind::Click);
    }
}
