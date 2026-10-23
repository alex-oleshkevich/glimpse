use std::time::Duration;

use ipp::prelude::Uri;

use super::call;

pub const LEASE_SECONDS: i32 = 3600;
const RESPONSE_CAP: usize = 65536;
const EVENTS: [&str; 10] = [
    "job-created",
    "job-state-changed",
    "job-completed",
    "job-stopped",
    "printer-added",
    "printer-deleted",
    "printer-modified",
    "printer-state-changed",
    "server-started",
    "server-restarted",
];

#[derive(Debug, PartialEq)]
pub enum Renewal {
    Kept,
    Gone,
    Failed(String),
}

pub async fn create(uri: &Uri) -> Result<i32, String> {
    let user = call::user_name().ok_or("printing subscription requires a valid USER or LOGNAME")?;
    let body = request(uri, 0x0016, None, &user, &EVENTS, LEASE_SECONDS)?;
    let response = exchange(uri, body).await?;
    let (status, id) = parse(&response)?;
    accept(status)?;
    id.ok_or_else(|| "CUPS subscription response has no subscription id".to_owned())
}

pub async fn renew(uri: &Uri, id: i32) -> Renewal {
    match change(uri, 0x001a, id).await {
        Ok(0x0406) => Renewal::Gone,
        Ok(status) => match accept(status) {
            Ok(()) => Renewal::Kept,
            Err(reason) => Renewal::Failed(reason),
        },
        Err(reason) => Renewal::Failed(reason),
    }
}

pub async fn cancel(uri: &Uri, id: i32) -> Result<(), String> {
    accept(change(uri, 0x001b, id).await?)
}

async fn change(uri: &Uri, operation: u16, id: i32) -> Result<u16, String> {
    let user = call::user_name().ok_or("printing subscription requires a valid USER or LOGNAME")?;
    let body = request(uri, operation, Some(id), &user, &[], LEASE_SECONDS)?;
    parse(&exchange(uri, body).await?).map(|(status, _)| status)
}

async fn exchange(uri: &Uri, body: Vec<u8>) -> Result<Vec<u8>, String> {
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|error| error.to_string())?;
    let mut response = client
        .post(uri.to_string())
        .header(reqwest::header::CONTENT_TYPE, "application/ipp")
        .body(body)
        .send()
        .await
        .map_err(|error| error.to_string())?
        .error_for_status()
        .map_err(|error| error.to_string())?;
    if !response.status().is_success() {
        return Err(format!(
            "CUPS subscription HTTP status {}",
            response.status()
        ));
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|error| error.to_string())? {
        if chunk.len() > RESPONSE_CAP.saturating_sub(bytes.len()) {
            return Err("CUPS subscription response is too large".to_owned());
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

fn request(
    uri: &Uri,
    operation: u16,
    id: Option<i32>,
    user: &str,
    events: &[&str],
    lease: i32,
) -> Result<Vec<u8>, String> {
    let mut bytes = vec![2, 0];
    bytes.extend_from_slice(&operation.to_be_bytes());
    bytes.extend_from_slice(&1_i32.to_be_bytes());
    bytes.push(1);
    attribute(&mut bytes, 0x47, "attributes-charset", b"utf-8")?;
    attribute(&mut bytes, 0x48, "attributes-natural-language", b"en")?;
    let printer = uri
        .to_string()
        .replacen("https://", "ipps://", 1)
        .replacen("http://", "ipp://", 1);
    attribute(&mut bytes, 0x45, "printer-uri", printer.as_bytes())?;
    attribute(&mut bytes, 0x42, "requesting-user-name", user.as_bytes())?;
    if let Some(id) = id {
        attribute(
            &mut bytes,
            0x21,
            "notify-subscription-id",
            &id.to_be_bytes(),
        )?;
    } else {
        bytes.push(6);
        attribute(&mut bytes, 0x45, "notify-recipient-uri", b"dbus://")?;
        for (index, event) in events.iter().enumerate() {
            attribute(
                &mut bytes,
                0x44,
                if index == 0 { "notify-events" } else { "" },
                event.as_bytes(),
            )?;
        }
    }
    if operation != 0x001b {
        attribute(
            &mut bytes,
            0x21,
            "notify-lease-duration",
            &lease.to_be_bytes(),
        )?;
    }
    bytes.push(3);
    Ok(bytes)
}

fn attribute(bytes: &mut Vec<u8>, tag: u8, name: &str, value: &[u8]) -> Result<(), String> {
    let name_len = u16::try_from(name.len()).map_err(|error| error.to_string())?;
    let value_len = u16::try_from(value.len()).map_err(|error| error.to_string())?;
    bytes.push(tag);
    bytes.extend_from_slice(&name_len.to_be_bytes());
    bytes.extend_from_slice(name.as_bytes());
    bytes.extend_from_slice(&value_len.to_be_bytes());
    bytes.extend_from_slice(value);
    Ok(())
}

fn accept(status: u16) -> Result<(), String> {
    if status < 0x0100 {
        Ok(())
    } else {
        Err(format!("CUPS subscription IPP status 0x{status:04x}"))
    }
}

fn parse(bytes: &[u8]) -> Result<(u16, Option<i32>), String> {
    let malformed = || "malformed CUPS subscription response".to_owned();
    let header = bytes.get(..8).ok_or_else(malformed)?;
    if !matches!(header[0], 1 | 2) {
        return Err(malformed());
    }
    let status = u16::from_be_bytes([header[2], header[3]]);
    let mut cursor = 8;
    let mut group = 0;
    let mut id = None;
    loop {
        let tag = *bytes.get(cursor).ok_or_else(malformed)?;
        cursor += 1;
        if tag == 3 {
            return Ok((status, id));
        }
        if tag < 0x10 {
            if !matches!(tag, 1 | 2 | 4 | 5 | 6 | 7 | 9) {
                return Err(malformed());
            }
            group = tag;
            continue;
        }
        let name = field(bytes, &mut cursor)?;
        let value = field(bytes, &mut cursor)?;
        if group == 6 && name == b"notify-subscription-id" {
            if tag != 0x21 || value.len() != 4 || id.is_some() {
                return Err(malformed());
            }
            let found = i32::from_be_bytes(value.try_into().map_err(|_| malformed())?);
            if found <= 0 {
                return Err(malformed());
            }
            id = Some(found);
        }
    }
}

fn field<'a>(bytes: &'a [u8], cursor: &mut usize) -> Result<&'a [u8], String> {
    let malformed = || "truncated CUPS subscription attribute".to_owned();
    let length = bytes.get(*cursor..*cursor + 2).ok_or_else(malformed)?;
    let length = usize::from(u16::from_be_bytes([length[0], length[1]]));
    *cursor += 2;
    let value = bytes.get(*cursor..*cursor + length).ok_or_else(malformed)?;
    *cursor += length;
    Ok(value)
}

#[cfg(test)]
mod tests;
