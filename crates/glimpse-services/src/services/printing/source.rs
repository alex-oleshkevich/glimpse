use std::pin::Pin;
use std::time::Duration;

use futures_util::{Stream, StreamExt, stream};
use ipp::attribute::{IppAttribute, IppAttributeGroup, IppAttributes};
use ipp::client::non_blocking::AsyncIppClient;
use ipp::error::IppError;
use ipp::model::{DelimiterTag, IppVersion, Operation, StatusCode};
use ipp::operation::cups::CupsGetPrinters;
use ipp::prelude::Uri;
use ipp::request::IppRequestResponse;
use ipp::value::IppValue;
use tokio::time;
use zbus::message::Type;
use zbus::{Connection, MatchRule, MessageStream};

use crate::context::Ctx;

use super::{Config, Event, JobState, PrintJob, Printer, PrinterState, Printing, PrintingState};

const NAME_CAP: usize = 128;
const NOTIFIER_INTERFACE: &str = "org.cups.cupsd.Notifier";
const NOTIFIER_PATH: &str = "/org/cups/cupsd/Notifier";
const REQUESTED_JOB_ATTRIBUTES: [&str; 6] = [
    "job-id",
    "job-name",
    "job-state",
    "job-printer-uri",
    "job-impressions",
    "job-impressions-completed",
];

type Events = Pin<Box<dyn Stream<Item = Event> + Send>>;

fn nothing() -> Events {
    Box::pin(stream::empty())
}

pub async fn fetch(config: &Config) -> Event {
    let uri = match config.uri() {
        Ok(uri) => uri,
        Err(reason) => return Event::Unavailable(reason),
    };
    match fetch_state(uri).await {
        Ok(state) => Event::Polled(state),
        Err(error) => Event::Unavailable(describe(error)),
    }
}

pub async fn notifier(ctx: Ctx<Printing>, config: Config, floor: Duration) -> Events {
    let Ok(connection) = ctx.system_bus() else {
        return nothing();
    };
    let Some(stream) = signal_stream(connection).await else {
        return nothing();
    };

    Box::pin(stream::unfold(
        (stream, config, floor, time::Instant::now()),
        |(mut stream, config, floor, mut next_allowed)| async move {
            loop {
                let message = stream.next().await?;
                if message.is_err() {
                    continue;
                }
                let now = time::Instant::now();
                if now < next_allowed {
                    continue;
                }
                next_allowed = now + floor;
                let event = fetch(&config).await;
                return Some((event, (stream, config, floor, next_allowed)));
            }
        },
    ))
}

async fn signal_stream(connection: &Connection) -> Option<MessageStream> {
    let rule = MatchRule::builder()
        .msg_type(Type::Signal)
        .interface(NOTIFIER_INTERFACE)
        .ok()?
        .path(NOTIFIER_PATH)
        .ok()?
        .build();

    MessageStream::for_match_rule(rule, connection, None)
        .await
        .ok()
}

async fn fetch_state(uri: Uri) -> Result<PrintingState, IppError> {
    let (printers, jobs) = tokio::try_join!(
        fetch_printers(AsyncIppClient::new(uri.clone())),
        fetch_jobs(AsyncIppClient::new(uri.clone()), uri),
    )?;
    Ok(PrintingState { printers, jobs })
}

async fn fetch_printers(client: AsyncIppClient) -> Result<Vec<Printer>, IppError> {
    let response = client.send(CupsGetPrinters::new()).await?;
    Ok(parse_printers(accept_list(&response)?))
}

async fn fetch_jobs(client: AsyncIppClient, uri: Uri) -> Result<Vec<PrintJob>, IppError> {
    let request = get_jobs_request(uri)?;
    let response = client.send(request).await?;
    Ok(parse_jobs(accept_list(&response)?))
}

fn get_jobs_request(uri: Uri) -> Result<IppRequestResponse, IppError> {
    let mut request = IppRequestResponse::new(IppVersion::v1_1(), Operation::GetJobs, Some(uri))?;
    let requested = REQUESTED_JOB_ATTRIBUTES
        .into_iter()
        .map(|name| Ok(IppValue::Keyword(name.try_into()?)))
        .collect::<Result<Vec<IppValue>, IppError>>()?;
    request.attributes_mut().add(
        DelimiterTag::OperationAttributes,
        IppAttribute::with_name(
            IppAttribute::REQUESTED_ATTRIBUTES,
            IppValue::Array(requested),
        )?,
    );
    Ok(request)
}

fn accept_list(response: &IppRequestResponse) -> Result<&IppAttributes, IppError> {
    let status = response.header().status_code();
    if status.is_success() || status == StatusCode::ClientErrorNotFound {
        Ok(response.attributes())
    } else {
        Err(IppError::StatusError(status))
    }
}

fn describe(error: IppError) -> String {
    match error {
        IppError::AsyncClientError(inner) => crate::services::transport(inner),
        other => crate::services::say(other),
    }
}

fn parse_printers(attributes: &IppAttributes) -> Vec<Printer> {
    attributes
        .groups_of(DelimiterTag::PrinterAttributes)
        .map(|group| {
            let name = cap(&str_attr(group, "printer-name").unwrap_or_default());
            let make_model = cap(&str_attr(group, "printer-make-and-model").unwrap_or_default());
            let state = match u32_attr(group, "printer-state") {
                Some(3) => PrinterState::Idle,
                Some(4) => PrinterState::Processing,
                Some(5) => PrinterState::Stopped,
                _ => PrinterState::Idle,
            };
            let state_reasons = str_list_attr(group, "printer-state-reasons")
                .into_iter()
                .filter(|reason| reason != "none")
                .map(|reason| cap(&reason))
                .collect();
            let job_count = u32_attr(group, "queued-job-count").unwrap_or(0);

            Printer {
                name,
                make_model,
                state,
                state_reasons,
                state_message: cap(&str_attr(group, "printer-state-message").unwrap_or_default()),
                location: cap(&str_attr(group, "printer-location").unwrap_or_default()),
                accepting_jobs: bool_attr(group, "printer-is-accepting-jobs").unwrap_or(true),
                color: bool_attr(group, "color-supported").unwrap_or(false),
                duplex: str_list_attr(group, "sides-supported")
                    .iter()
                    .any(|side| side.starts_with("two-sided")),
                media_ready: str_list_attr(group, "media-ready")
                    .into_iter()
                    .map(|media| cap(&media))
                    .collect(),
                resolution: resolution_attr(group).unwrap_or_default(),
                job_count,
            }
        })
        .collect()
}

fn parse_jobs(attributes: &IppAttributes) -> Vec<PrintJob> {
    attributes
        .groups_of(DelimiterTag::JobAttributes)
        .filter_map(|group| {
            let id = u32_attr(group, "job-id")?;
            let name = str_attr(group, "job-name")
                .map(|name| cap(&name))
                .unwrap_or_else(|| format!("Job {id}"));
            let printer_uri = str_attr(group, "job-printer-uri").unwrap_or_default();
            let printer_name = cap(&printer_name_from_uri(&printer_uri));
            let state = match u32_attr(group, "job-state") {
                Some(3) => JobState::Pending,
                Some(4) => JobState::Held,
                Some(5) => JobState::Processing,
                Some(6) => JobState::Stopped,
                Some(7) => JobState::Cancelled,
                Some(8) => JobState::Aborted,
                Some(9) => JobState::Completed,
                _ => JobState::Pending,
            };
            let pages_completed = u32_attr(group, "job-impressions-completed");
            let pages_total = u32_attr(group, "job-impressions");

            Some(PrintJob {
                id,
                name,
                printer_name,
                state,
                pages_completed,
                pages_total,
            })
        })
        .collect()
}

fn cap(text: &str) -> String {
    text.chars().take(NAME_CAP).collect()
}

fn printer_name_from_uri(uri: &str) -> String {
    uri.rsplit('/').next().unwrap_or(uri).to_owned()
}

fn str_attr(group: &IppAttributeGroup, name: &str) -> Option<String> {
    ipp_str_value(group.get(name)?.value())
}

fn u32_attr(group: &IppAttributeGroup, name: &str) -> Option<u32> {
    match group.get(name)?.value() {
        IppValue::Enum(value) | IppValue::Integer(value) => u32::try_from(*value).ok(),
        _ => None,
    }
}

fn str_list_attr(group: &IppAttributeGroup, name: &str) -> Vec<String> {
    let Some(attribute) = group.get(name) else {
        return Vec::new();
    };
    match attribute.value() {
        IppValue::Array(values) => values.iter().filter_map(ipp_str_value).collect(),
        single => ipp_str_value(single).into_iter().collect(),
    }
}

fn bool_attr(group: &IppAttributeGroup, name: &str) -> Option<bool> {
    match group.get(name)?.value() {
        IppValue::Boolean(value) => Some(*value),
        _ => None,
    }
}

/// `printer-resolution-default` is a `resolution` value, not a string, so it has no `ipp_str_value`
/// arm. Only the cross-feed figure is rendered: every printer this has been seen on reports a
/// square resolution, and "600 dpi" reads better than "600x600".
fn resolution_attr(group: &IppAttributeGroup) -> Option<String> {
    match group.get("printer-resolution-default")?.value() {
        IppValue::Resolution {
            cross_feed,
            feed,
            units,
        } => {
            let unit = match units {
                4 => "dpcm",
                _ => "dpi",
            };
            Some(match cross_feed == feed {
                true => format!("{cross_feed} {unit}"),
                false => format!("{cross_feed}x{feed} {unit}"),
            })
        }
        _ => None,
    }
}

fn ipp_str_value(value: &IppValue) -> Option<String> {
    match value {
        IppValue::TextWithoutLanguage(text) => Some(text.as_ref().to_owned()),
        IppValue::NameWithoutLanguage(name) => Some(name.as_str().to_owned()),
        IppValue::Uri(uri) => Some(uri.as_str().to_owned()),
        IppValue::Keyword(keyword) => Some(keyword.as_str().to_owned()),
        IppValue::OctetString(bytes) => std::str::from_utf8(bytes).ok().map(str::to_owned),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use ipp::attribute::IppAttribute;
    use ipp::model::IppVersion;
    use tokio_util::sync::CancellationToken;

    use super::*;
    use crate::service::Input;

    fn group(tag: DelimiterTag, pairs: Vec<(&str, IppValue)>) -> IppAttributes {
        let mut attributes = IppAttributes::new();
        for (name, value) in pairs {
            attributes.add(
                tag,
                IppAttribute::with_name(name, value).expect("a valid attribute name"),
            );
        }
        attributes
    }

    #[test]
    fn printer_state_follows_rfc8011_and_falls_back_to_idle() {
        for (code, expected) in [
            (3, PrinterState::Idle),
            (4, PrinterState::Processing),
            (5, PrinterState::Stopped),
            (99, PrinterState::Idle),
        ] {
            let attributes = group(
                DelimiterTag::PrinterAttributes,
                vec![
                    (
                        "printer-name",
                        IppValue::NameWithoutLanguage("office".try_into().expect("a name")),
                    ),
                    ("printer-state", IppValue::Enum(code)),
                ],
            );
            let printers = parse_printers(&attributes);
            assert_eq!(printers[0].state, expected, "state code {code}");
        }
    }

    #[test]
    fn job_state_follows_rfc8011_and_falls_back_to_pending() {
        for (code, expected) in [
            (3, JobState::Pending),
            (4, JobState::Held),
            (5, JobState::Processing),
            (6, JobState::Stopped),
            (7, JobState::Cancelled),
            (8, JobState::Aborted),
            (9, JobState::Completed),
            (42, JobState::Pending),
        ] {
            let attributes = group(
                DelimiterTag::JobAttributes,
                vec![
                    ("job-id", IppValue::Integer(1)),
                    ("job-state", IppValue::Enum(code)),
                ],
            );
            let jobs = parse_jobs(&attributes);
            assert_eq!(jobs[0].state, expected, "state code {code}");
        }
    }

    #[test]
    fn a_job_missing_a_name_gets_a_generated_one() {
        let attributes = group(
            DelimiterTag::JobAttributes,
            vec![("job-id", IppValue::Integer(7))],
        );
        assert_eq!(parse_jobs(&attributes)[0].name, "Job 7");
    }

    #[test]
    fn a_job_with_no_id_is_dropped_rather_than_guessed() {
        let attributes = group(
            DelimiterTag::JobAttributes,
            vec![(
                "job-name",
                IppValue::NameWithoutLanguage("x".try_into().expect("a name")),
            )],
        );
        assert!(parse_jobs(&attributes).is_empty());
    }

    #[test]
    fn printer_name_extracted_from_ipp_uri() {
        assert_eq!(
            printer_name_from_uri("ipp://localhost:631/printers/Office"),
            "Office"
        );
        assert_eq!(printer_name_from_uri("Office"), "Office");
    }

    #[test]
    fn a_none_state_reason_is_dropped_and_a_real_one_kept() {
        let attributes = group(
            DelimiterTag::PrinterAttributes,
            vec![
                (
                    "printer-name",
                    IppValue::NameWithoutLanguage("office".try_into().expect("a name")),
                ),
                (
                    "printer-state-reasons",
                    IppValue::Array(vec![
                        IppValue::Keyword("none".try_into().expect("a keyword")),
                        IppValue::Keyword("media-low".try_into().expect("a keyword")),
                    ]),
                ),
            ],
        );
        let printers = parse_printers(&attributes);
        assert_eq!(printers[0].state_reasons, vec!["media-low".to_owned()]);
    }

    #[test]
    fn get_jobs_requests_exactly_the_attributes_this_parser_reads() {
        let uri: Uri = "http://localhost:631/".parse().expect("a uri");
        let request = get_jobs_request(uri).expect("a request");

        let operation_attributes = request
            .attributes()
            .first_of(DelimiterTag::OperationAttributes)
            .expect("operation attributes group");
        let requested = operation_attributes
            .get("requested-attributes")
            .expect("requested-attributes must be present, or a conformant server answers id-only");

        let names: Vec<String> = match requested.value() {
            IppValue::Array(values) => values.iter().filter_map(ipp_str_value).collect(),
            _ => Vec::new(),
        };
        assert_eq!(
            names,
            REQUESTED_JOB_ATTRIBUTES
                .iter()
                .map(|name| (*name).to_owned())
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn a_successful_list_response_is_accepted() {
        let response =
            IppRequestResponse::new_response(IppVersion::v1_1(), StatusCode::SuccessfulOk, 1)
                .expect("a response");
        assert!(accept_list(&response).is_ok());
    }

    #[test]
    fn a_not_found_list_response_is_treated_as_empty_rather_than_a_failure() {
        let response = IppRequestResponse::new_response(
            IppVersion::v1_1(),
            StatusCode::ClientErrorNotFound,
            1,
        )
        .expect("a response");
        assert!(
            accept_list(&response).is_ok(),
            "no printers or jobs configured is not a backend failure"
        );
    }

    #[test]
    fn a_server_error_list_response_is_a_failure() {
        let response = IppRequestResponse::new_response(
            IppVersion::v1_1(),
            StatusCode::ServerErrorInternalError,
            1,
        )
        .expect("a response");
        assert!(accept_list(&response).is_err());
    }

    fn test_config(server_url: Option<&str>) -> Config {
        let mut document = glimpse_config::Config::default();
        document.printing.server_url = server_url.map(str::to_owned);
        Config::from(&document)
    }

    #[tokio::test]
    async fn the_notifier_source_is_inert_without_a_system_bus() {
        let cancel = CancellationToken::new();
        let (events, _inbox) = tokio::sync::mpsc::channel::<Input<Printing>>(8);
        let (state, _state_rx) = tokio::sync::watch::channel(PrintingState::default());
        let (health, _health_rx) = tokio::sync::watch::channel(crate::ServiceState::Starting);
        let ctx = Ctx::<Printing>::new(
            events,
            &cancel,
            state,
            health,
            glimpse_dbus::Buses::unavailable("no bus in tests"),
        );

        let mut stream = notifier(ctx, test_config(None), Duration::from_millis(10)).await;

        assert!(
            matches!(
                futures_util::FutureExt::now_or_never(stream.next()),
                Some(None)
            ),
            "with no system bus the notifier must degrade to an empty stream, never panic"
        );
    }
}
