use ipp::attribute::IppAttribute;
use ipp::client::non_blocking::AsyncIppClient;
use ipp::error::IppError;
use ipp::model::{DelimiterTag, IppVersion, Operation};
use ipp::operation::builder::IppOperationBuilder;
use ipp::prelude::Uri;
use ipp::request::IppRequestResponse;
use ipp::value::IppValue;

pub async fn cancel_job(client: AsyncIppClient, uri: Uri, id: u32) -> Result<(), IppError> {
    let job_id = i32::try_from(id).map_err(|_| IppError::MissingAttribute)?;
    let operation = IppOperationBuilder::cancel_job(uri, job_id).build()?;
    accept(client.send(operation).await?)
}

pub async fn hold_job(client: AsyncIppClient, uri: Uri, id: u32) -> Result<(), IppError> {
    let request = job_request(uri, Operation::HoldJob, id)?;
    accept(client.send(request).await?)
}

pub async fn release_job(client: AsyncIppClient, uri: Uri, id: u32) -> Result<(), IppError> {
    let request = job_request(uri, Operation::ReleaseJob, id)?;
    accept(client.send(request).await?)
}

fn job_request(uri: Uri, operation: Operation, id: u32) -> Result<IppRequestResponse, IppError> {
    let job_id = i32::try_from(id).map_err(|_| IppError::MissingAttribute)?;
    let mut request = IppRequestResponse::new(IppVersion::v1_1(), operation, Some(uri))?;
    request.attributes_mut().add(
        DelimiterTag::OperationAttributes,
        IppAttribute::with_name(IppAttribute::JOB_ID, IppValue::Integer(job_id))?,
    );
    Ok(request)
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
mod tests {
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
}
