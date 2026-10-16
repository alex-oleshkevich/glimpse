use glimpse_dbus::{settings::*, testing::PrivateBus};
use glimpsectl::{
    cli::DesktopMode,
    commands::{settings_mode, settings_resume, settings_status},
};

struct Settings(ModeSnapshot);

#[zbus::interface(name = "me.aresa.Glimpse.Settings1")]
impl Settings {
    #[zbus(property)]
    fn solar(&self) -> SolarSnapshot {
        (false, 0, 0, "no location".into())
    }
    #[zbus(property)]
    fn geolocation(&self) -> GeolocationSnapshot {
        (false, 0.0, 0.0, "no location".into())
    }
    #[zbus(property)]
    fn mode(&self) -> ModeSnapshot {
        self.0.clone()
    }
    fn set_temporary_mode(&mut self, mode: u32) -> zbus::fdo::Result<()> {
        if mode != 1 && mode != 2 {
            return Err(zbus::fdo::Error::InvalidArgs("invalid mode".into()));
        }
        self.0.2 = mode;
        self.0.3 = mode;
        self.0.4 = mode;
        Ok(())
    }
    fn resume(&mut self) {
        self.0.4 = 0;
    }
}

#[tokio::test]
async fn settings_commands_read_status_send_modes_and_report_a_missing_provider() {
    let bus = PrivateBus::start();
    let server = bus.connection().await;
    let client = bus.connection().await;
    server
        .object_server()
        .at(
            GLIMPSE_SETTINGS_OBJECT_PATH,
            Settings((true, "external".into(), 0, 0, 0, String::new())),
        )
        .await
        .unwrap();
    glimpse_dbus::own_name(&server, GLIMPSE_SETTINGS_BUS_NAME)
        .await
        .unwrap();
    settings_status(&client, true).await.unwrap();
    settings_mode(&client, DesktopMode::Dark).await.unwrap();
    let proxy = Settings1Proxy::builder(&client)
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await
        .unwrap();
    assert_eq!(proxy.mode().await.unwrap().4, 2);
    settings_mode(&client, DesktopMode::Light).await.unwrap();
    assert_eq!(proxy.mode().await.unwrap().4, 1);
    settings_resume(&client).await.unwrap();
    assert_eq!(proxy.mode().await.unwrap().4, 0);
    assert!(proxy.set_temporary_mode(0).await.is_err());
    let interface = server
        .object_server()
        .interface::<_, Settings>(GLIMPSE_SETTINGS_OBJECT_PATH)
        .await
        .unwrap();
    interface.get_mut().await.0 = (false, "sun".into(), 0, 0, 0, "no solar phase".into());
    settings_status(&client, false).await.unwrap();
    interface.get_mut().await.0.3 = 99;
    assert!(settings_status(&client, true).await.is_err());
    assert!(
        glimpsectl::commands::settings::appearance_status(&client, true)
            .await
            .is_err()
    );
    glimpsectl::commands::settings::geolocation_status(&client, true)
        .await
        .unwrap();
    server
        .release_name(GLIMPSE_SETTINGS_BUS_NAME)
        .await
        .unwrap();
    assert!(settings_status(&client, true).await.is_err());
}
