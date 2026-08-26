#[zbus::proxy(
    interface = "org.freedesktop.timedate1",
    default_service = "org.freedesktop.timedate1",
    default_path = "/org/freedesktop/timedate1"
)]
pub trait Timedate1 {
    #[zbus(property)]
    fn timezone(&self) -> zbus::Result<String>;

    #[zbus(property, name = "NTP")]
    fn ntp(&self) -> zbus::Result<bool>;

    #[zbus(property(emits_changed_signal = "false"), name = "NTPSynchronized")]
    fn ntp_synchronized(&self) -> zbus::Result<bool>;

    #[zbus(property(emits_changed_signal = "false"), name = "CanNTP")]
    fn can_ntp(&self) -> zbus::Result<bool>;

    #[zbus(property, name = "LocalRTC")]
    fn local_rtc(&self) -> zbus::Result<bool>;
}
