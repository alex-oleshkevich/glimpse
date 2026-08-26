mod error;
mod load;
mod schema;
mod watch;

pub use error::ConfigError;
pub use load::{load, resolved_files, watch_dirs};
pub use schema::*;
pub use watch::{Update, reread, watch, watch_config};

pub fn default_document() -> String {
    let header = "\
#:schema /usr/share/glimpse/config.schema.json
# This is glimpse's default configuration — every setting, with the value it ships with.
";
    let body = toml::to_string_pretty(&Config::default()).expect("Config::default() serializes");
    format!("{header}\n{body}")
}

pub fn json_schema_document() -> String {
    let schema = schemars::schema_for!(Config);
    let mut body = serde_json::to_string_pretty(&schema).expect("a JsonSchema always serializes");
    body.push('\n');
    body
}
