use anyhow::Result;
use glimpse_contracts::SystemMethods;

use super::{Session, absent, empty_reason, narrow};
use crate::render::Table;

const METHODS: &str = "system.methods";

pub async fn methods(
    session: &Session,
    pattern: Option<String>,
    owner: Option<String>,
) -> Result<()> {
    let Some(event) = session.client.get(METHODS).await? else {
        return absent(METHODS, false);
    };

    let pattern_given = pattern.is_some();
    let data = match pattern {
        None => event.data,
        Some(pattern) => narrow(event.data, METHODS, "methods", &pattern)?,
    };

    let mut report: SystemMethods = serde_json::from_value(data)?;
    if let Some(owner) = &owner {
        report.methods.retain(|_, entry| entry.service == *owner);
    }

    Table::new()
        .with_headers(["METHOD", "OWNER"])
        .with_empty(&empty_reason("method", pattern_given, owner.as_deref()))
        .with_rows(
            report
                .methods
                .iter()
                .map(|(method, entry)| [method.clone(), entry.service.clone()]),
        )
        .print()
}
