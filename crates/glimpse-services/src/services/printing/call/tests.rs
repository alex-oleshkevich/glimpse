use ipp::model::StatusCode;

use super::*;

fn response(status: StatusCode) -> IppRequestResponse {
    IppRequestResponse::new_response(IppVersion::v1_1(), status, 1).expect("a response")
}

#[test]
fn a_successful_status_is_accepted() {
    assert!(accept(response(StatusCode::SuccessfulOk)).is_ok());
}

#[test]
fn a_client_error_status_is_a_typed_failure_not_silently_accepted() {
    let outcome = accept(response(StatusCode::ClientErrorNotFound));
    assert!(matches!(
        outcome,
        Err(IppError::StatusError(StatusCode::ClientErrorNotFound))
    ));
}

fn reply(head: &str, status: StatusCode) -> Vec<u8> {
    let mut bytes = head.as_bytes().to_vec();
    bytes.extend_from_slice(&response(status).to_bytes());
    bytes
}

#[test]
fn a_socket_reply_carries_the_ipp_status_through() {
    let parsed = parse_reply(reply(
        "HTTP/1.1 200 OK\r\nContent-Type: application/ipp\r\n\r\n",
        StatusCode::ClientErrorNotPossible,
    ))
    .expect("a parsed response");
    assert_eq!(
        parsed.header().status_code(),
        StatusCode::ClientErrorNotPossible
    );
}

#[test]
fn an_http_refusal_on_the_socket_is_a_request_error() {
    let outcome = parse_reply(b"HTTP/1.1 401 Unauthorized\r\nContent-Length: 0\r\n\r\n".to_vec());
    assert!(matches!(outcome, Err(IppError::RequestError(401))));
}

#[test]
fn a_truncated_socket_reply_is_an_error_not_a_panic() {
    assert!(matches!(
        parse_reply(b"HTTP/1.1 200".to_vec()),
        Err(IppError::RequestError(0))
    ));
}

#[test]
fn a_user_name_that_could_break_the_header_is_refused() {
    assert!(valid_user("alex"));
    assert!(valid_user("first.last@corp"));
    assert!(!valid_user(""));
    assert!(!valid_user("alex\r\nX-Evil: 1"));
    assert!(!valid_user("alex bob"));
}

#[test]
fn a_job_request_names_the_job_and_the_user() {
    let uri: Uri = "http://localhost:631/".parse().expect("a uri");
    let request = job_request(uri, Operation::CancelJob, 28, Some("alex")).expect("a request");
    let operation = request
        .attributes()
        .first_of(DelimiterTag::OperationAttributes)
        .expect("operation attributes");
    assert_eq!(
        operation
            .get(IppAttribute::JOB_ID)
            .map(|a| a.value().clone()),
        Some(IppValue::Integer(28))
    );
    assert!(operation.get(IppAttribute::REQUESTING_USER_NAME).is_some());
}

#[tokio::test]
async fn a_socket_request_carries_peer_credentials_and_reads_the_reply_to_eof() {
    let dir = tempfile::tempdir().expect("a temp dir");
    let path = dir.path().join("cups.sock");
    let listener = tokio::net::UnixListener::bind(&path).expect("a listener");
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.expect("a client");
        let mut received = Vec::new();
        let mut chunk = [0u8; 1024];
        let (head, body_len) = loop {
            let read = stream.read(&mut chunk).await.expect("a read");
            received.extend_from_slice(&chunk[..read]);
            if let Some(split) = received.windows(4).position(|w| w == b"\r\n\r\n") {
                break (split, received.len() - split - 4);
            }
        };
        let text = String::from_utf8_lossy(&received[..head]).into_owned();
        let mut reply = b"HTTP/1.0 200 OK\r\nContent-Type: application/ipp\r\n\r\n".to_vec();
        reply.extend_from_slice(&response(StatusCode::SuccessfulOk).to_bytes());
        stream.write_all(&reply).await.expect("a write");
        (text, body_len)
    });

    let uri: Uri = "http://localhost:631/".parse().expect("a uri");
    let request = job_request(uri, Operation::CancelJob, 28, Some("alex")).expect("a request");
    let expected_len = request.to_bytes().len();
    let parsed = timeout(Duration::from_secs(5), over_socket(&path, "alex", request))
        .await
        .expect("the reply ends at EOF")
        .expect("a parsed reply");

    let (head, body_len) = server.await.expect("the server");
    assert!(head.contains("Authorization: PeerCred alex"));
    assert!(head.contains(&format!("Content-Length: {expected_len}")));
    assert!(body_len <= expected_len);
    assert!(accept(parsed).is_ok());
}
