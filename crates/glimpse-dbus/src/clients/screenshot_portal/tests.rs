use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use zbus::ObjectServer;
use zbus::message::Header;
use zbus::names::BusName;

use crate::testing::PrivateBus;

use super::*;

#[derive(Clone, Copy)]
enum Answer {
    Color(f64, f64, f64),
    Cancelled,
    Failed,
    NoColor,
    TextColor,
    Never,
    UnexpectedHandle,
}

struct FakePortal {
    answer: Answer,
    closed: Arc<AtomicBool>,
}

#[zbus::interface(name = "org.freedesktop.portal.Screenshot")]
impl FakePortal {
    async fn pick_color(
        &self,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] connection: &Connection,
        #[zbus(object_server)] server: &ObjectServer,
        _parent_window: &str,
        options: HashMap<String, OwnedValue>,
    ) -> zbus::fdo::Result<OwnedObjectPath> {
        let sender = header
            .sender()
            .unwrap()
            .trim_start_matches(':')
            .replace('.', "_");
        let token = <&str>::try_from(options.get("handle_token").unwrap()).unwrap();
        let mut path = OwnedObjectPath::try_from(format!(
            "/org/freedesktop/portal/desktop/request/{sender}/{token}"
        ))
        .unwrap();
        if matches!(self.answer, Answer::UnexpectedHandle) {
            path = OwnedObjectPath::try_from(format!("{path}_unexpected")).unwrap();
        }
        let (code, results): (u32, HashMap<&str, Value<'_>>) = match self.answer {
            Answer::Color(red, green, blue) => (
                0,
                HashMap::from([("color", Value::from((red, green, blue)))]),
            ),
            Answer::Cancelled => (1, HashMap::new()),
            Answer::Failed => (2, HashMap::new()),
            Answer::NoColor => (0, HashMap::new()),
            Answer::TextColor => (0, HashMap::from([("color", Value::from("#E0563F"))])),
            Answer::Never | Answer::UnexpectedHandle => {
                server
                    .at(
                        path.clone(),
                        FakeRequest {
                            closed: self.closed.clone(),
                        },
                    )
                    .await
                    .unwrap();
                return Ok(path);
            }
        };
        connection
            .emit_signal(
                None::<BusName<'_>>,
                &path,
                "org.freedesktop.portal.Request",
                "Response",
                &(code, results),
            )
            .await
            .unwrap();
        Ok(path)
    }
}

struct FakeRequest {
    closed: Arc<AtomicBool>,
}

#[zbus::interface(name = "org.freedesktop.portal.Request")]
impl FakeRequest {
    fn close(&self) {
        self.closed.store(true, Ordering::SeqCst);
    }
}

async fn serve(bus: &PrivateBus, answer: Answer, closed: Arc<AtomicBool>) -> Connection {
    let connection = bus.connection().await;
    connection
        .object_server()
        .at(
            "/org/freedesktop/portal/desktop",
            FakePortal { answer, closed },
        )
        .await
        .unwrap();
    crate::own_name(&connection, "org.freedesktop.portal.Desktop")
        .await
        .unwrap();
    connection
}

async fn pick(answer: Answer) -> Result<Option<[u8; 3]>, PickColorError> {
    let bus = PrivateBus::start();
    let _portal = serve(&bus, answer, Arc::default()).await;
    let client = bus.connection().await;
    tokio::time::timeout(Duration::from_secs(2), pick_color(&client))
        .await
        .unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_picked_color_is_scaled_to_bytes() {
    assert_eq!(
        pick(Answer::Color(1.0, 0.337, 0.247)).await.unwrap(),
        Some([255, 86, 63])
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_response_sent_before_the_method_reply_is_received() {
    assert!(pick(Answer::Color(0.0, 0.0, 0.0)).await.is_ok());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn out_of_range_and_non_finite_channels_are_rejected() {
    for value in [f64::NAN, f64::INFINITY, -0.1, 1.1] {
        assert!(matches!(
            pick(Answer::Color(value, 0.0, 0.0)).await,
            Err(PickColorError::Failed)
        ));
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_cancelled_pick_is_none() {
    assert_eq!(pick(Answer::Cancelled).await.unwrap(), None);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_failed_or_malformed_answer_is_failed() {
    for answer in [Answer::Failed, Answer::NoColor, Answer::TextColor] {
        assert!(matches!(pick(answer).await, Err(PickColorError::Failed)));
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn no_portal_is_unavailable() {
    let bus = PrivateBus::start();
    let client = bus.connection().await;
    let result = tokio::time::timeout(Duration::from_secs(2), pick_color(&client))
        .await
        .unwrap();
    assert!(matches!(result, Err(PickColorError::Unavailable)));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn dropping_the_pick_closes_the_request() {
    let bus = PrivateBus::start();
    let closed = Arc::new(AtomicBool::new(false));
    let _portal = serve(&bus, Answer::Never, closed.clone()).await;
    let client = bus.connection().await;
    assert!(
        tokio::time::timeout(Duration::from_millis(300), pick_color(&client))
            .await
            .is_err()
    );
    tokio::time::timeout(Duration::from_secs(2), async {
        while !closed.load(Ordering::SeqCst) {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_unexpected_request_path_fails_and_is_closed() {
    let bus = PrivateBus::start();
    let closed = Arc::new(AtomicBool::new(false));
    let _portal = serve(&bus, Answer::UnexpectedHandle, closed.clone()).await;
    let client = bus.connection().await;
    let result = tokio::time::timeout(Duration::from_secs(2), pick_color(&client))
        .await
        .unwrap();
    assert!(matches!(result, Err(PickColorError::UnexpectedHandle)));
    tokio::time::timeout(Duration::from_secs(2), async {
        while !closed.load(Ordering::SeqCst) {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
}
