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
pub use picker::{Picked, Picker, ProcessPicker, Request as PickRequest};

const TEXT: &str = "text/plain;charset=utf-8";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    request: PickRequest,
    limit: usize,
}

impl From<&glimpse_config::Config> for Config {
    fn from(document: &glimpse_config::Config) -> Self {
        let picker = &document.color_picker;
        Self {
            request: PickRequest {
                format: picker.format,
                lens_radius: picker.lens_radius.clamp(40, 400),
                max_zoom: picker.max_zoom.clamp(2, 64),
            },
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
            format: config.request.format,
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
                let request = self.config.request;
                let guard =
                    ctx.spawn(move |_ctx| async move { Event::Picked(picker.pick(request).await) });
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
        self.offer(self.config.request.format.render(color.rgb))
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
            format: self.config.request.format,
        });
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use glimpse_dbus::Buses;
    use tokio::sync::mpsc;
    use tokio_util::sync::CancellationToken;

    use super::*;
    use crate::selection::FakeSelection;

    struct FakePicker {
        answers: Mutex<Vec<Result<Option<[u8; 3]>, String>>>,
        requests: Mutex<Vec<PickRequest>>,
    }

    impl Picker for FakePicker {
        fn pick(&self, request: PickRequest) -> Picked {
            self.requests.lock().unwrap().push(request);
            let answer = self.answers.lock().unwrap().remove(0);
            Box::pin(async move { answer })
        }
    }

    struct Harness {
        service: ColorPicker,
        selection: FakeSelection,
        picker: Arc<FakePicker>,
        ctx: Ctx<ColorPicker>,
        inbox: mpsc::Receiver<Input<ColorPicker>>,
        state: watch::Receiver<ColorPickerState>,
        _cancel: CancellationToken,
    }

    fn document(format: glimpse_config::ColorFormat, limit: usize) -> glimpse_config::Config {
        let mut document = glimpse_config::Config::default();
        document.color_picker.format = format;
        document.color_picker.limit = limit;
        document
    }

    async fn harness(answers: Vec<Result<Option<[u8; 3]>, String>>) -> Harness {
        let config = Config::from(&document(ColorFormat::Hex, 8));
        let (events, inbox) = mpsc::channel(32);
        let cancel = CancellationToken::new();
        let (health, _health) = watch::channel(ServiceState::Starting);
        let (published, state) = watch::channel(ColorPicker::initial_state(&config));
        let ctx = Ctx::<ColorPicker>::new(
            events,
            &cancel,
            published,
            health,
            Buses::unavailable("no bus in tests"),
        );
        let selection = FakeSelection::default();
        let picker = Arc::new(FakePicker {
            answers: Mutex::new(answers),
            requests: Mutex::new(Vec::new()),
        });
        let service = ColorPicker::start(
            &ctx,
            config,
            Dependencies {
                selection: Arc::new(selection.clone()),
                picker: picker.clone(),
            },
        )
        .await
        .unwrap();
        Harness {
            service,
            selection,
            picker,
            ctx,
            inbox,
            state,
            _cancel: cancel,
        }
    }

    impl Harness {
        async fn feed(&mut self, input: Input<ColorPicker>) {
            self.service.handle(&self.ctx, input).await;
        }

        async fn pick(&mut self) -> oneshot::Receiver<Result<(), CommandError>> {
            let (reply, result) = oneshot::channel();
            self.feed(Input::Command(Command::Pick { reply })).await;
            result
        }

        async fn settle(&mut self) {
            let input = self.inbox.recv().await.unwrap();
            self.feed(input).await;
        }

        fn offered(&self) -> Vec<String> {
            self.selection
                .offered()
                .into_iter()
                .map(|offer| String::from_utf8(offer.data.to_vec()).unwrap())
                .collect()
        }
    }

    #[tokio::test]
    async fn a_pick_lands_in_the_palette_and_on_the_clipboard_in_the_configured_format() {
        let mut harness = harness(vec![Ok(Some([224, 86, 63]))]).await;

        let result = harness.pick().await;
        assert!(harness.state.borrow().picking);
        harness.settle().await;

        assert_eq!(result.await.unwrap(), Ok(()));
        let state = harness.state.borrow().clone();
        assert!(!state.picking);
        assert_eq!(state.colors.len(), 1);
        assert_eq!(state.colors[0].rgb, [224, 86, 63]);
        assert_eq!(harness.offered(), ["#E0563F"]);
        assert_eq!(
            harness.picker.requests.lock().unwrap()[0],
            PickRequest {
                format: ColorFormat::Hex,
                lens_radius: 106,
                max_zoom: 30
            }
        );
    }

    #[tokio::test]
    async fn a_cancelled_pick_changes_nothing_and_is_not_an_error() {
        let mut harness = harness(vec![Ok(None)]).await;

        let result = harness.pick().await;
        harness.settle().await;

        assert_eq!(result.await.unwrap(), Ok(()));
        assert!(harness.state.borrow().colors.is_empty());
        assert!(harness.offered().is_empty());
    }

    #[tokio::test]
    async fn a_failed_pick_reports_what_the_picker_said() {
        let mut harness = harness(vec![Err("no screencopy".to_owned())]).await;

        let result = harness.pick().await;
        harness.settle().await;

        assert_eq!(
            result.await.unwrap(),
            Err(CommandError::Unavailable("no screencopy".to_owned()))
        );
        assert!(!harness.state.borrow().picking);
    }

    #[tokio::test]
    async fn a_second_pick_while_one_is_open_is_refused() {
        let mut harness = harness(vec![Ok(Some([1, 2, 3]))]).await;

        let _first = harness.pick().await;
        tokio::task::yield_now().await;
        let second = harness.pick().await;

        assert!(matches!(
            second.await.unwrap(),
            Err(CommandError::LimitExceeded(_))
        ));
        assert_eq!(harness.picker.requests.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn a_palette_color_copies_in_any_format_and_a_missing_one_is_refused() {
        let mut harness = harness(vec![Ok(Some([224, 86, 63]))]).await;
        let _picked = harness.pick().await;
        harness.settle().await;
        let id = harness.state.borrow().colors[0].id;

        let (reply, result) = oneshot::channel();
        harness
            .feed(Input::Command(Command::Copy {
                id,
                format: ColorFormat::Rgb,
                reply,
            }))
            .await;
        assert_eq!(result.await.unwrap(), Ok(()));
        assert_eq!(harness.offered(), ["#E0563F", "rgb(224 86 63)"]);

        let (reply, result) = oneshot::channel();
        harness
            .feed(Input::Command(Command::Copy {
                id: id + 100,
                format: ColorFormat::Hex,
                reply,
            }))
            .await;
        assert!(matches!(
            result.await.unwrap(),
            Err(CommandError::InvalidArgument(_))
        ));
    }
}
