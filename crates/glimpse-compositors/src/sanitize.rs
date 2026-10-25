pub const TITLE_CAP: usize = 512;
pub const APP_ID_CAP: usize = 128;
pub const NAME_CAP: usize = 128;
pub const EDID_CAP: usize = 128;

pub fn sanitize(text: &str, cap: usize) -> String {
    text.chars()
        .filter(|character| !glimpse_utils::text::hostile(*character))
        .take(cap)
        .collect()
}

pub fn capped_title_str(text: &str) -> Option<String> {
    non_empty(sanitize(text, TITLE_CAP))
}

pub fn capped_app_id_str(text: &str) -> Option<String> {
    non_empty(sanitize(text, APP_ID_CAP))
}

pub fn capped_name_str(text: &str) -> Option<String> {
    non_empty(sanitize(text, NAME_CAP))
}

pub fn capped_edid_str(text: &str) -> Option<String> {
    non_empty(sanitize(text, EDID_CAP).trim().to_owned())
}

fn non_empty(text: String) -> Option<String> {
    (!text.is_empty()).then_some(text)
}
