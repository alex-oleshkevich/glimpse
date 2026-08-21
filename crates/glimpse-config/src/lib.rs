mod error;
mod load;
mod schema;

pub use error::ConfigError;
pub use load::{Loaded, load};
pub use schema::*;

pub fn default_document() -> String {
    let header = "\
# This is glimpse's default configuration — every setting, with the value it ships with.

";
    let body = toml::to_string_pretty(&Config::default()).expect("Config::default() serializes");
    format!("{header}\n{body}")
}
