mod error;
mod load;
mod schema;
mod theme;
mod watch;

pub use error::ConfigError;
pub use load::{DATA_DIR, load, resolved_files, user_dir, watch_dirs};
pub use schema::*;
pub use theme::{
    DEFAULT_THEME, LOCK_STYLESHEET, PANEL_STYLESHEET, WALLPAPER_STYLESHEET, stylesheet,
    theme_dir_for, user_stylesheet, watch_theme,
};
pub use watch::{Update, watch, watch_all, watch_config};

pub fn default_document() -> String {
    let header = format!(
        "#:schema {DATA_DIR}/config.schema.json\n\
         # This is glimpse's default configuration — every setting, with the value it ships with.\n"
    );
    let body = toml::to_string_pretty(&Config::default()).expect("Config::default() serializes");
    format!("{header}\n{body}")
}

pub fn json_schema_document() -> String {
    let schema = schemars::schema_for!(Config);
    let mut body = serde_json::to_string_pretty(&schema).expect("a JsonSchema always serializes");
    body.push('\n');
    body
}
