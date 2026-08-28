use adw::gdk;
use glimpse_config::Position;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use relm4::{
    ComponentParts, ComponentSender, SimpleComponent,
    gtk::{self, prelude::*},
};

pub struct Panel {
    window: gtk::Window,
    bar: glimpse_widgets::Panel,
}

#[derive(Debug)]
pub struct Config {
    pub position: Position,
    pub size: u32,
    pub monitor: gdk::Monitor,
}

#[derive(Debug)]
pub enum Input {
    Configure(Config),
}

#[relm4::component(pub)]
impl SimpleComponent for Panel {
    type Init = Config;
    type Input = Input;
    type Output = ();

    view! {
        root = gtk::Window {
            #[name = "bar"]
            glimpse_widgets::Panel {}
        }
    }

    fn init(
        config: Self::Init,
        root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        root.init_layer_shell();
        root.set_namespace(Some("glimpse-panel"));
        root.set_layer(Layer::Top);
        root.set_keyboard_mode(KeyboardMode::None);
        root.auto_exclusive_zone_enable();

        let window = root.clone();
        let widgets = view_output!();
        let model = Panel {
            window: window.clone(),
            bar: widgets.bar.clone(),
        };

        model.apply(&config);
        window.present();

        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, _sender: ComponentSender<Self>) {
        match message {
            Input::Configure(config) => self.apply(&config),
        }
    }
}

impl Panel {
    fn apply(&self, config: &Config) {
        let (anchors, orientation) = match config.position {
            Position::Top => (
                [Edge::Top, Edge::Left, Edge::Right],
                gtk::Orientation::Horizontal,
            ),
            Position::Bottom => (
                [Edge::Bottom, Edge::Left, Edge::Right],
                gtk::Orientation::Horizontal,
            ),
            Position::Left => (
                [Edge::Left, Edge::Top, Edge::Bottom],
                gtk::Orientation::Vertical,
            ),
            Position::Right => (
                [Edge::Right, Edge::Top, Edge::Bottom],
                gtk::Orientation::Vertical,
            ),
        };

        if self.window.monitor().as_ref() != Some(&config.monitor) {
            self.window.set_monitor(Some(&config.monitor));
        }
        for edge in [Edge::Top, Edge::Bottom, Edge::Left, Edge::Right] {
            self.window.set_anchor(edge, anchors.contains(&edge));
        }
        self.bar.set_orientation(orientation);
        self.bar.set_thickness(config.size);

        tracing::debug!(
            position = ?config.position,
            size = config.size,
            monitor = ?config.monitor.connector(),
            "panel configured"
        );
    }
}
