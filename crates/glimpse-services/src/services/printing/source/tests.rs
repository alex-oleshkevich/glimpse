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
    let response =
        IppRequestResponse::new_response(IppVersion::v1_1(), StatusCode::ClientErrorNotFound, 1)
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
