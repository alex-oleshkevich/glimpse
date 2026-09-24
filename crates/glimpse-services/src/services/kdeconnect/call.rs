use zbus::proxy::CacheProperties;
use zbus::{Connection, Result};

use glimpse_dbus::kdeconnect::{
    self, ClipboardProxy, DeviceProxy, FindMyPhoneProxy, PingProxy, SftpProxy, ShareProxy, SmsProxy,
};

use super::{Action, DeviceId, source};

macro_rules! proxy {
    ($proxy:ty, $connection:expr, $owner:expr, $path:expr) => {
        <$proxy>::builder($connection)
            .destination($owner.to_owned())?
            .path($path)?
            .cache_properties(CacheProperties::No)
            .build()
            .await?
    };
}

pub async fn act(
    connection: &Connection,
    owner: &str,
    id: &DeviceId,
    action: Action,
) -> Result<()> {
    let id = id.as_str();
    let plugin = |name: &str| kdeconnect::plugin_path(id, name);
    match action {
        Action::Ring => {
            proxy!(FindMyPhoneProxy, connection, owner, plugin("findmyphone"))
                .ring()
                .await
        }
        Action::Ping => {
            proxy!(PingProxy, connection, owner, plugin("ping"))
                .send_ping()
                .await
        }
        Action::SendClipboard => {
            proxy!(ClipboardProxy, connection, owner, plugin("clipboard"))
                .send_clipboard()
                .await
        }
        Action::Browse => {
            let started = proxy!(SftpProxy, connection, owner, plugin("sftp"))
                .start_browsing()
                .await?;
            match started {
                true => Ok(()),
                false => Err(zbus::Error::Failure(
                    "the phone's storage could not be mounted".to_owned(),
                )),
            }
        }
        Action::OpenMessages => {
            proxy!(SmsProxy, connection, owner, plugin("sms"))
                .launch_app()
                .await
        }
        Action::Pair => {
            proxy!(DeviceProxy, connection, owner, kdeconnect::device_path(id))
                .request_pairing()
                .await
        }
        Action::Unpair => {
            proxy!(DeviceProxy, connection, owner, kdeconnect::device_path(id))
                .unpair()
                .await
        }
    }
}

pub async fn share(
    connection: &Connection,
    owner: &str,
    id: &DeviceId,
    urls: &[String],
) -> Result<()> {
    let urls: Vec<&str> = urls.iter().map(String::as_str).collect();
    proxy!(
        ShareProxy,
        connection,
        owner,
        kdeconnect::plugin_path(id.as_str(), "share")
    )
    .share_urls(&urls)
    .await
}

pub async fn discover(connection: &Connection, owner: &str) -> Result<()> {
    source::daemon(connection, owner)
        .await?
        .force_on_network_change()
        .await
}
