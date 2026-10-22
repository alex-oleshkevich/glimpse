mod commented;
mod environment;
mod error;
mod load;
mod schema;
mod theme;
mod watch;

pub use commented::commented_document;
pub use environment::{TWENTY_FOUR, clock};
pub use error::ConfigError;
pub use load::{
    DATA_DIR, Documents, applets_dir, config_dir, config_files, find, init, load, load_files,
    load_or_default, resolve_image, runtime_dir, seed_user_config, state_dir, unknown_applets,
    writable_file, write_table,
};
pub use schema::*;
pub use theme::{
    DARK_STYLESHEET, DEFAULT_THEME, LOCK_STYLESHEET, PANEL_STYLESHEET, Theme, WALLPAPER_STYLESHEET,
    load_theme, watch_theme,
};
pub use watch::{Reload, Update, watch_config, watch_dirs};

pub fn default_document(documents: Documents) -> String {
    let name = documents.name();
    let header = format!(
        "#:schema {DATA_DIR}/{name}.schema.json\n\
         # This is glimpse's default {name}.toml — every setting, with the value it ships with.\n"
    );
    format!("{header}\n{}", document_body(documents))
}

pub fn json_schema_document(documents: Documents) -> String {
    let mut schema = schemars::schema_for!(Config);
    if let Some(properties) = schema
        .get_mut("properties")
        .and_then(serde_json::Value::as_object_mut)
    {
        properties.retain(|table, _| documents.renders(table));
    }
    let mut body = serde_json::to_string_pretty(&schema).expect("a JsonSchema always serializes");
    body.push('\n');
    body
}

pub(crate) fn document_body(documents: Documents) -> String {
    let body = toml::to_string_pretty(&Config::default()).expect("Config::default() serializes");
    let mut document = body
        .parse::<toml_edit::DocumentMut>()
        .expect("serialized TOML parses");
    document
        .as_table_mut()
        .retain(|table, _| documents.renders(table));
    document.to_string().trim_start().to_owned()
}
