use glimpse_services::OutputInfo;

pub const ICON: &str = "video-display-symbolic";
pub const NAME_CAP: usize = 32;

pub fn chip(count: usize) -> Option<&'static str> {
    (count > 0).then_some(ICON)
}

pub fn heading(output: &OutputInfo) -> String {
    let heading = match output.label.as_deref() {
        Some(label) if !label.is_empty() => [output.connector.as_str(), label].join(" · "),
        _ => output.connector.clone(),
    };
    glimpse_utils::clean(&heading, NAME_CAP)
}

pub fn tooltip(outputs: &[OutputInfo]) -> Option<String> {
    let named = outputs
        .iter()
        .find(|output| output.focused)
        .or_else(|| outputs.first())?;
    Some(heading(named))
}

#[cfg(test)]
mod tests;
