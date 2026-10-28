use super::*;
use crate::testing::PrivateBus;

#[tokio::test]
async fn a_missing_agents_name_is_an_error() {
    let bus = PrivateBus::start();
    let client = bus.connection().await;
    let proxy = SessionProxy::builder(&client).build().await.expect("proxy");

    assert!(proxy.request_action("reboot").await.is_err());
}
