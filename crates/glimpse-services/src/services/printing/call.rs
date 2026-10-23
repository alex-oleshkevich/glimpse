use std::io::Cursor;
use std::path::Path;
use std::time::Duration;

use ipp::attribute::IppAttribute;
use ipp::client::non_blocking::AsyncIppClient;
use ipp::error::IppError;
use ipp::model::{DelimiterTag, IppVersion, Operation};
use ipp::parser::IppParser;
use ipp::prelude::Uri;
use ipp::reader::IppReader;
use ipp::request::IppRequestResponse;
use ipp::value::IppValue;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;
use tokio::time::timeout;

const CUPS_SOCKET: &str = "/run/cups/cups.sock";
const SOCKET_TIMEOUT: Duration = Duration::from_secs(10);
const REPLY_CAP: u64 = 1 << 20;

pub async fn perform(local: bool, uri: Uri, operation: Operation, id: u32) -> Result<(), IppError> {
    let user = local.then(user_name).flatten();
    let request = job_request(uri.clone(), operation, id, user.as_deref())?;
    let response = match user {
        Some(user) if tokio::fs::try_exists(CUPS_SOCKET).await.unwrap_or(false) => timeout(
            SOCKET_TIMEOUT,
            over_socket(Path::new(CUPS_SOCKET), &user, request),
        )
        .await
        .map_err(|elapsed| IppError::IoError(elapsed.into()))??,
        _ => AsyncIppClient::new(uri).send(request).await?,
    };
    accept(response)
}

fn job_request(
    uri: Uri,
    operation: Operation,
    id: u32,
    user: Option<&str>,
) -> Result<IppRequestResponse, IppError> {
    let job_id = i32::try_from(id).map_err(|_| IppError::MissingAttribute)?;
    let mut request = IppRequestResponse::new(IppVersion::v1_1(), operation, Some(uri))?;
    let attributes = request.attributes_mut();
    attributes.add(
        DelimiterTag::OperationAttributes,
        IppAttribute::with_name(IppAttribute::JOB_ID, IppValue::Integer(job_id))?,
    );
    if let Some(user) = user {
        attributes.add(
            DelimiterTag::OperationAttributes,
            IppAttribute::with_name(
                IppAttribute::REQUESTING_USER_NAME,
                IppValue::NameWithoutLanguage(user.try_into()?),
            )?,
        );
    }
    Ok(request)
}

pub(super) fn user_name() -> Option<String> {
    ["USER", "LOGNAME"]
        .into_iter()
        .filter_map(|key| std::env::var(key).ok())
        .find(|name| valid_user(name))
}

fn valid_user(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '@' | '$'))
}

async fn over_socket(
    path: &Path,
    user: &str,
    request: IppRequestResponse,
) -> Result<IppRequestResponse, IppError> {
    let body = request.to_bytes();
    let head = format!(
        "POST / HTTP/1.0\r\nHost: localhost\r\nContent-Type: application/ipp\r\n\
         Content-Length: {}\r\nAuthorization: PeerCred {user}\r\n\r\n",
        body.len()
    );
    let mut stream = UnixStream::connect(path).await?;
    stream.write_all(head.as_bytes()).await?;
    stream.write_all(&body).await?;
    let mut reply = Vec::new();
    stream.take(REPLY_CAP).read_to_end(&mut reply).await?;
    parse_reply(reply)
}

fn parse_reply(reply: Vec<u8>) -> Result<IppRequestResponse, IppError> {
    let split = reply
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .ok_or(IppError::RequestError(0))?;
    let status = std::str::from_utf8(&reply[..split])
        .ok()
        .and_then(|head| head.split_whitespace().nth(1))
        .and_then(|code| code.parse::<u16>().ok())
        .ok_or(IppError::RequestError(0))?;
    if status != 200 {
        return Err(IppError::RequestError(status));
    }
    let body = reply[split + 4..].to_vec();
    Ok(IppParser::new(IppReader::new(Cursor::new(body))).parse()?)
}

fn accept(response: IppRequestResponse) -> Result<(), IppError> {
    let status = response.header().status_code();
    if status.is_success() {
        Ok(())
    } else {
        Err(IppError::StatusError(status))
    }
}

#[cfg(test)]
mod tests;
