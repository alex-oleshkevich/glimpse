use glimpse_dbus::night_light::NightLightSnapshot;
use glimpsectl::commands::sunset::{mode, paused_until, serving, temperature};

fn snapshot() -> NightLightSnapshot {
    NightLightSnapshot {
        schedule: "automatic".to_owned(),
        overridden: false,
        temperature: 6500,
        target: 4200,
        active: false,
        serving: true,
        reason: String::new(),
        configured: "automatic".to_owned(),
        manual: false,
        paused_until: 0,
    }
}

#[test]
fn an_overridden_mode_says_that_the_document_is_not_the_one_talking() {
    let mut overridden = snapshot();
    overridden.schedule = "off".to_owned();
    overridden.overridden = true;

    assert!(mode(&overridden).contains("off"));
    assert!(mode(&overridden).contains("override"));
    assert!(!mode(&snapshot()).contains("override"));
}

#[test]
fn a_manual_temperature_says_so_and_a_scheduled_one_does_not() {
    let mut manual = snapshot();
    manual.manual = true;

    assert!(temperature(&manual).contains("manual"));
    assert!(!temperature(&snapshot()).contains("manual"));
}

#[test]
fn a_pause_shows_its_local_deadline_and_no_pause_says_no() {
    assert_eq!(paused_until(&snapshot()), "no");
    let deadline = chrono::Local::now().with_timezone(&chrono::Utc);
    let paused = NightLightSnapshot {
        paused_until: deadline.timestamp(),
        ..snapshot()
    };
    assert_eq!(
        paused_until(&paused),
        deadline
            .with_timezone(&chrono::Local)
            .format("%H:%M")
            .to_string()
    );
}

#[test]
fn a_provider_that_is_not_serving_shows_why() {
    let mut degraded = snapshot();
    degraded.serving = false;
    degraded.reason = "another gamma client holds the outputs".to_owned();

    assert!(serving(&degraded).contains("another gamma client"));
    assert!(!serving(&snapshot()).contains('-'));
}

#[derive(Debug, zbus::DBusError)]
#[zbus(prefix = "me.aresa.Glimpse.NightLight1.Error", impl_display = true)]
enum Error {
    NotActive(String),
    #[zbus(error)]
    ZBus(zbus::Error),
}

struct Provider {
    active: bool,
    paused: bool,
}

#[zbus::interface(name = "me.aresa.Glimpse.NightLight1")]
impl Provider {
    fn pause(&mut self) -> Result<(), Error> {
        if !self.active {
            return Err(Error::NotActive("off".to_owned()));
        }
        self.paused = true;
        Ok(())
    }

    fn resume(&mut self) {
        self.paused = false;
    }
}

#[tokio::test]
async fn pause_and_resume_use_the_existing_exit_mapping_over_a_private_bus() {
    use glimpse_dbus::night_light::{
        GLIMPSE_NIGHT_LIGHT_BUS_NAME, GLIMPSE_NIGHT_LIGHT_OBJECT_PATH,
    };
    use glimpsectl::commands::sunset::{sunset_pause, sunset_resume};
    use glimpsectl::errors::{Exit, exit};
    let bus = glimpse_dbus::testing::PrivateBus::start();
    let server = bus.connection().await;
    server
        .object_server()
        .at(
            GLIMPSE_NIGHT_LIGHT_OBJECT_PATH,
            Provider {
                active: true,
                paused: false,
            },
        )
        .await
        .unwrap();
    glimpse_dbus::own_name(&server, GLIMPSE_NIGHT_LIGHT_BUS_NAME)
        .await
        .unwrap();
    let client = bus.connection().await;
    sunset_pause(&client).await.unwrap();
    let interface = server
        .object_server()
        .interface::<_, Provider>(GLIMPSE_NIGHT_LIGHT_OBJECT_PATH)
        .await
        .unwrap();
    assert!(interface.get().await.paused);
    sunset_resume(&client).await.unwrap();
    assert!(!interface.get().await.paused);
    interface.get_mut().await.active = false;
    assert_eq!(
        exit(&sunset_pause(&client).await.unwrap_err()),
        Exit::Rejected
    );
    sunset_resume(&client).await.unwrap();
    drop(interface);
    drop(server);
    assert_eq!(
        exit(&sunset_pause(&client).await.unwrap_err()),
        Exit::Unreachable
    );
    assert_eq!(
        exit(&sunset_resume(&client).await.unwrap_err()),
        Exit::Unreachable
    );
}
