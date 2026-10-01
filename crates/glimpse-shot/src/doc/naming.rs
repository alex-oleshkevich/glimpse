use chrono::format::StrftimeItems;
use chrono::{DateTime, Local};
use glimpse_config::FileTemplate;

pub fn file_name(template: &FileTemplate, at: DateTime<Local>) -> String {
    at.format_with_items(StrftimeItems::new(template.as_str()))
        .to_string()
}

pub fn candidates(name: &str) -> impl Iterator<Item = String> {
    let stem = name.strip_suffix(".png").unwrap_or(name).to_owned();
    std::iter::once(name.to_owned()).chain((2..).map(move |n| format!("{stem} ({n}).png")))
}

#[cfg(test)]
mod tests;
