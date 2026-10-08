mod palette;
mod picker;

use std::sync::Arc;

use glimpse_config::ColorFormat;
use tokio::sync::{oneshot, watch};

use crate::{
    ServiceState,
    context::{Ctx, SourceGuard},
    publisher::Publisher,
    selection::{Offer, Selection},
    service::{CommandError, Input, Service, ServiceEndpoint, ServiceError},
};

use palette::Palette;
pub use palette::PickedColor;
pub use picker::{Picked, Picker, PortalPicker};

const TEXT: &str = "text/plain;charset=utf-8";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    format: ColorFormat,
    limit: usize,
}

impl From<&glimpse_config::Config> for Config {
    fn from(document: &glimpse_config::Config) -> Self {
        let picker = &document.color_picker;
        Self {
            format: picker.format,
            limit: picker.limit,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ColorPickerState {
    pub colors: Vec<PickedColor>,
    pub picking: bool,
    pub format: ColorFormat,
}

pub enum Command {
    Pick {
        reply: oneshot::Sender<Result<(), CommandError>>,
    },
    Copy {
        id: u32,
        format: ColorFormat,
        reply: oneshot::Sender<Result<(), CommandError>>,
    },
}

pub enum Event {
    Picked(Result<Option<[u8; 3]>, String>),
}

pub struct Dependencies {
    pub selection: Arc<dyn Selection>,
    pub picker: Arc<dyn Picker>,
}

#[derive(Clone)]
pub struct ColorPickerHandle(ServiceEndpoint<ColorPicker>);

impl ColorPickerHandle {
    pub fn snapshot(&self) -> ColorPickerState {
        self.0.snapshot()
    }

    pub fn subscribe(&self) -> watch::Receiver<ColorPickerState> {
        self.0.subscribe()
    }

    pub fn health(&self) -> watch::Receiver<ServiceState> {
        self.0.health()
    }

    pub async fn pick(&self) -> Result<(), CommandError> {
        self.ask(|reply| Command::Pick { reply }, "picking").await
    }

    pub async fn copy(&self, id: u32, format: ColorFormat) -> Result<(), CommandError> {
        self.ask(|reply| Command::Copy { id, format, reply }, "copying")
            .await
    }

    async fn ask(
        &self,
        build: impl FnOnce(oneshot::Sender<Result<(), CommandError>>) -> Command,
        doing: &str,
    ) -> Result<(), CommandError> {
        let (reply, result) = oneshot::channel();
        self.0.command(build(reply))?;
        result.await.map_err(|_| {
            CommandError::Unavailable(format!("the color picker stopped while {doing}"))
        })?
    }
}

pub struct ColorPicker {
    state: Publisher<ColorPickerState>,
    selection: Arc<dyn Selection>,
    picker: Arc<dyn Picker>,
    palette: Palette,
    config: Config,
    pending: Option<(SourceGuard, oneshot::Sender<Result<(), CommandError>>)>,
}

impl Service for ColorPicker {
    const NAME: &'static str = "color_picker";

    type Config = Config;
    type State = ColorPickerState;
    type Handle = ColorPickerHandle;
    type Command = Command;
    type Event = Event;
    type Dependencies = Dependencies;
    type SubKey = ();

    fn from_endpoint(endpoint: ServiceEndpoint<Self>) -> Self::Handle {
        ColorPickerHandle(endpoint)
    }

    fn initial_state(config: &Self::Config) -> Self::State {
        ColorPickerState {
            colors: Vec::new(),
            picking: false,
            format: config.format,
        }
    }

    async fn start(
        ctx: &Ctx<Self>,
        config: Self::Config,
        dependencies: Self::Dependencies,
    ) -> Result<Self, ServiceError> {
        Ok(Self {
            state: ctx.publisher(),
            selection: dependencies.selection,
            picker: dependencies.picker,
            palette: Palette::new(config.limit),
            config,
            pending: None,
        })
    }

    async fn handle(&mut self, ctx: &Ctx<Self>, input: Input<Self>) {
        match input {
            Input::Command(Command::Pick { reply }) => {
                if self.pending.is_some() {
                    let _ = reply.send(Err(CommandError::LimitExceeded(
                        "a color is already being picked".to_owned(),
                    )));
                    return;
                }
                let picker = Arc::clone(&self.picker);
                let guard =
                    ctx.spawn(move |_ctx| async move { Event::Picked(picker.pick().await) });
                self.pending = Some((guard, reply));
            }
            Input::Command(Command::Copy { id, format, reply }) => {
                let _ = reply.send(self.copy(id, format));
            }
            Input::Event(Event::Picked(outcome)) => {
                let Some((_guard, reply)) = self.pending.take() else {
                    return;
                };
                let _ = reply.send(self.picked(outcome));
            }
            Input::Config(_) => return,
        }
        self.publish();
    }
}

impl ColorPicker {
    fn picked(&mut self, outcome: Result<Option<[u8; 3]>, String>) -> Result<(), CommandError> {
        let Some(rgb) = outcome.map_err(CommandError::Unavailable)? else {
            return Ok(());
        };
        let color = self.palette.push(rgb);
        self.offer(self.config.format.render(color.rgb))
    }

    fn copy(&self, id: u32, format: ColorFormat) -> Result<(), CommandError> {
        let color = self.palette.get(id).ok_or_else(|| {
            CommandError::InvalidArgument("that color is no longer in the palette".to_owned())
        })?;
        self.offer(format.render(color.rgb))
    }

    fn offer(&self, text: String) -> Result<(), CommandError> {
        self.selection
            .offer(Offer {
                mime: TEXT.to_owned(),
                data: Arc::from(text.into_bytes()),
            })
            .map_err(CommandError::Unavailable)
    }

    fn publish(&self) {
        self.state.set(ColorPickerState {
            colors: self.palette.colors(),
            picking: self.pending.is_some(),
            format: self.config.format,
        });
    }
}

#[cfg(test)]
mod tests;
