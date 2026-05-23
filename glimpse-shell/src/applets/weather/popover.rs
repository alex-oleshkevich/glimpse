use relm4::{
    ComponentParts, ComponentSender, SimpleComponent,
    gtk::{self, prelude::*},
};

use glimpse_core::services::weather::model::State;

use crate::widgets::{
    animated_popover::AnimatedPopover,
    expander_tile::ExpanderTile,
    hero::Hero,
    key_value_grid::KeyValueGrid,
    popover_shell::PopoverShell,
    weather_forecast_list::{WeatherForecastItem, WeatherForecastList},
    weather_hourly_strip::{WeatherHourlyItem, WeatherHourlyStrip},
};

use super::format::{self, build_detail_rows, forecast_items, hero_summary};

pub struct Popover {
    popover: AnimatedPopover,
    hero: Hero,
    hero_temp_label: gtk::Label,
    hourly: WeatherHourlyStrip,
    details_grid: KeyValueGrid,
    forecast_list: WeatherForecastList,
    hero_title: String,
    hero_subtitle: String,
    hourly_visible: bool,
    forecast_visible: bool,
    details_visible: bool,
}

pub struct PopoverInit {
    pub parent: gtk::Box,
}

#[derive(Debug)]
pub enum PopoverInput {
    Toggle,
    Update(State),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PopoverOutput {
    Opened,
    Closed,
}

#[relm4::component(pub)]
impl SimpleComponent for Popover {
    type Init = PopoverInit;
    type Input = PopoverInput;
    type Output = PopoverOutput;

    view! {
        root = AnimatedPopover {
            add_css_class: "popover-size-medium",
            connect_show[sender] => move |_| {
                let _ = sender.output(PopoverOutput::Opened);
            },
            connect_closed[sender] => move |_| {
                let _ = sender.output(PopoverOutput::Closed);
            },

            PopoverShell {

                #[local_ref]
                hero_widget -> Hero {
                    #[watch] set_title: &model.hero_title,
                    #[watch] set_subtitle: &model.hero_subtitle,
                },

                gtk::Separator {
                    set_orientation: gtk::Orientation::Horizontal,
                },

                #[local_ref]
                hourly_widget -> WeatherHourlyStrip {
                    #[watch] set_visible: model.hourly_visible,
                },

                gtk::Separator {
                    set_orientation: gtk::Orientation::Horizontal,
                },

                #[local_ref]
                forecast_list_widget -> WeatherForecastList {
                    #[watch] set_visible: model.forecast_visible,
                },

                gtk::Separator {
                    set_orientation: gtk::Orientation::Horizontal,
                },

                #[name = "details_expander"]
                ExpanderTile {
                    set_primary: "Details",
                    #[watch] set_visible: model.details_visible,
                },
            },
        }
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let hero = Hero::new();
        hero.set_icon_size(32);
        hero.set_trailing_visible(true);
        hero.set_toggle_visible(false);
        hero.set_separator_visible(false);

        let hero_temp_label = gtk::Label::new(Some("—"));
        hero_temp_label.add_css_class("weather-hero-temp");
        hero.append_trailing(&hero_temp_label);

        let hourly = WeatherHourlyStrip::new();
        let details_grid = KeyValueGrid::new();
        let forecast_list = WeatherForecastList::new();

        let hero_widget = &hero;
        let hourly_widget = &hourly;
        let forecast_list_widget = &forecast_list;

        let model = Popover {
            popover: root.clone(),
            hero: hero.clone(),
            hero_temp_label,
            hourly: hourly.clone(),
            details_grid: details_grid.clone(),
            forecast_list: forecast_list.clone(),
            hero_title: String::new(),
            hero_subtitle: String::new(),
            hourly_visible: false,
            forecast_visible: false,
            details_visible: false,
        };

        #[allow(unused_assignments)]
        let widgets = view_output!();
        widgets.root.set_parent(&init.parent);
        widgets.details_expander.set_child(Some(details_grid));

        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, _sender: ComponentSender<Self>) {
        match message {
            PopoverInput::Toggle => self.popover.toggle(),
            PopoverInput::Update(state) => self.apply_state(&state),
        }
    }
}

impl Popover {
    fn apply_state(&mut self, state: &State) {
        match state {
            State::Ready(snapshot) => {
                self.hero.set_icon(Some(&snapshot.current.icon));
                self.hero_temp_label
                    .set_text(&format::temperature(snapshot.current.temperature));
                self.hero_title = hero_summary(&snapshot.current);
                self.hero_subtitle = snapshot.location.city.clone();

                let hourly: Vec<WeatherHourlyItem> = snapshot
                    .hourly
                    .iter()
                    .map(|h| WeatherHourlyItem {
                        time: h.time.clone(),
                        icon: h.icon.clone(),
                        temperature: format::temperature(h.temperature),
                    })
                    .collect();
                self.hourly_visible = !hourly.is_empty();
                self.hourly.set_items(&hourly);

                let forecast_data: Vec<WeatherForecastItem> = forecast_items(&snapshot.forecast)
                    .into_iter()
                    .map(|d| WeatherForecastItem {
                        day_name: d.day_name,
                        icon: d.icon,
                        condition: d.condition,
                        temperatures: format!(
                            "{} / {}",
                            format::temperature(d.temperature_max),
                            format::temperature(d.temperature_min),
                        ),
                        is_today: d.is_today,
                    })
                    .collect();
                self.forecast_visible = !forecast_data.is_empty();
                self.forecast_list.set_items(&forecast_data);

                let today = snapshot
                    .forecast
                    .iter()
                    .find(|d| d.is_today)
                    .or_else(|| snapshot.forecast.first());
                let detail_rows = build_detail_rows(&snapshot.current, today);
                self.details_grid.clear();
                for (key, value) in &detail_rows {
                    self.details_grid.add_row(key, value);
                }
                self.details_visible = !detail_rows.is_empty();
            }
            State::Loading => self.set_unavailable("Loading weather"),
            State::Unknown => self.set_unavailable("Weather unavailable"),
            State::Unavailable(message) => self.set_unavailable(if message.is_empty() {
                "Weather unavailable"
            } else {
                message
            }),
        }
    }

    fn set_unavailable(&mut self, message: &str) {
        self.hero.set_icon(Some("weather-overcast-symbolic"));
        self.hero_temp_label.set_text("—");
        self.hero_title = message.into();
        self.hero_subtitle = String::new();
        self.hourly_visible = false;
        self.forecast_visible = false;
        self.details_visible = false;
    }
}
