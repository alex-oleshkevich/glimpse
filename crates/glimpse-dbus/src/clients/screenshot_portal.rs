use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use futures_util::StreamExt as _;
use zbus::Connection;
use zbus::proxy::CacheProperties;
use zbus::zvariant::{OwnedObjectPath, OwnedValue, Value};

#[zbus::proxy(
    interface = "org.freedesktop.portal.Screenshot",
    default_service = "org.freedesktop.portal.Desktop",
    default_path = "/org/freedesktop/portal/desktop"
)]
pub trait ScreenshotPortal {
    fn pick_color(
        &self,
        parent_window: &str,
        options: HashMap<&str, Value<'_>>,
    ) -> zbus::Result<OwnedObjectPath>;
}

#[zbus::proxy(
    interface = "org.freedesktop.portal.Request",
    default_service = "org.freedesktop.portal.Desktop"
)]
pub trait PortalRequest {
    fn close(&self) -> zbus::Result<()>;

    #[zbus(signal)]
    fn response(&self, response: u32, results: HashMap<String, OwnedValue>) -> zbus::Result<()>;
}

#[derive(Debug, thiserror::Error)]
pub enum PickColorError {
    #[error("no screenshot portal is running")]
    Unavailable,
    #[error("the screenshot portal could not pick a color")]
    Failed,
    #[error("the screenshot portal returned an unexpected request path")]
    UnexpectedHandle,
}

pub async fn pick_color(connection: &Connection) -> Result<Option<[u8; 3]>, PickColorError> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let token = format!("glimpse_pick_{}", NEXT.fetch_add(1, Ordering::Relaxed));
    let sender = connection
        .unique_name()
        .ok_or(PickColorError::Failed)?
        .trim_start_matches(':')
        .replace('.', "_");
    let predicted = OwnedObjectPath::try_from(format!(
        "/org/freedesktop/portal/desktop/request/{sender}/{token}"
    ))
    .map_err(|_| PickColorError::Failed)?;

    let (request, mut responses) = subscribe(connection, predicted.clone()).await?;
    let mut pending = Pending(Some(request));
    let portal = ScreenshotPortalProxy::builder(connection)
        .cache_properties(CacheProperties::No)
        .build()
        .await
        .map_err(|_| PickColorError::Failed)?;
    let handle = portal
        .pick_color(
            "",
            HashMap::from([("handle_token", Value::from(token.as_str()))]),
        )
        .await
        .map_err(|error| {
            if unavailable(&error) {
                PickColorError::Unavailable
            } else {
                PickColorError::Failed
            }
        })?;
    if handle != predicted {
        let request = PortalRequestProxy::builder(connection)
            .path(handle)
            .map_err(|_| PickColorError::Failed)?
            .cache_properties(CacheProperties::No)
            .build()
            .await
            .map_err(|_| PickColorError::Failed)?;
        pending.0 = Some(request);
        return Err(PickColorError::UnexpectedHandle);
    }

    let response = responses.next().await.ok_or(PickColorError::Failed)?;
    pending.0 = None;
    let mut args = response.args().map_err(|_| PickColorError::Failed)?;
    match args.response {
        0 => {
            let color = args.results.remove("color").ok_or(PickColorError::Failed)?;
            let (red, green, blue) =
                <(f64, f64, f64)>::try_from(color).map_err(|_| PickColorError::Failed)?;
            Ok(Some([channel(red)?, channel(green)?, channel(blue)?]))
        }
        1 => Ok(None),
        _ => Err(PickColorError::Failed),
    }
}

async fn subscribe(
    connection: &Connection,
    path: OwnedObjectPath,
) -> Result<(PortalRequestProxy<'static>, ResponseStream), PickColorError> {
    let request = PortalRequestProxy::builder(connection)
        .path(path)
        .map_err(|_| PickColorError::Failed)?
        .cache_properties(CacheProperties::No)
        .build()
        .await
        .map_err(|_| PickColorError::Failed)?;
    let responses = request
        .receive_response()
        .await
        .map_err(|_| PickColorError::Failed)?;
    Ok((request, responses))
}

fn unavailable(error: &zbus::Error) -> bool {
    let zbus::Error::MethodError(name, _, _) = error else {
        return false;
    };
    matches!(
        name.rsplit('.').next(),
        Some(
            "ServiceUnknown" | "UnknownMethod" | "UnknownInterface" | "UnknownObject" | "NotFound"
        )
    )
}

fn channel(value: f64) -> Result<u8, PickColorError> {
    if !value.is_finite() || !(0.0..=1.0).contains(&value) {
        return Err(PickColorError::Failed);
    }
    Ok((value * 255.0).round() as u8)
}

struct Pending(Option<PortalRequestProxy<'static>>);

impl Drop for Pending {
    fn drop(&mut self) {
        let Some(request) = self.0.take() else {
            return;
        };
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            runtime.spawn(async move {
                let _ = request.close().await;
            });
        }
    }
}

#[cfg(test)]
mod tests;
