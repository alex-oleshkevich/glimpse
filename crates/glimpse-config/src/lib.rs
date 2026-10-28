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
    DATA_DIR, Documents, applets_dir, cache_dir, config_dir, config_files, find, init, load,
    load_files, load_or_default, resolve_image, runtime_dir, seed_user_config, state_dir,
    unknown_applets, writable_file, write_table,
};
pub use schema::*;
pub use theme::{
    LOCK_STYLESHEET, PANEL_STYLESHEET, Theme, WALLPAPER_STYLESHEET, load_theme, watch_theme,
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
    retain_reachable_defs(&mut schema);
    let mut body = serde_json::to_string_pretty(&schema).expect("a JsonSchema always serializes");
    body.push('\n');
    body
}

fn retain_reachable_defs(schema: &mut schemars::Schema) {
    let Some(root) = schema.as_object_mut() else {
        return;
    };
    let Some(definitions) = root.remove("$defs") else {
        return;
    };
    let serde_json::Value::Object(mut definitions) = definitions else {
        root.insert("$defs".to_owned(), definitions);
        return;
    };

    let mut reached = std::collections::BTreeSet::new();
    let mut pending = Vec::new();
    for value in root.values() {
        collect_definition_refs(value, &mut reached, &mut pending);
    }
    while let Some(name) = pending.pop() {
        if let Some(definition) = definitions.get(&name) {
            collect_definition_refs(definition, &mut reached, &mut pending);
        }
    }
    definitions.retain(|name, _| reached.contains(name));
    if !definitions.is_empty() {
        root.insert("$defs".to_owned(), serde_json::Value::Object(definitions));
    }
}

fn collect_definition_refs(
    value: &serde_json::Value,
    reached: &mut std::collections::BTreeSet<String>,
    pending: &mut Vec<String>,
) {
    match value {
        serde_json::Value::Array(items) => {
            for item in items {
                collect_definition_refs(item, reached, pending);
            }
        }
        serde_json::Value::Object(map) => {
            if let Some(name) = map
                .get("$ref")
                .and_then(serde_json::Value::as_str)
                .and_then(definition_name)
                && reached.insert(name.to_owned())
            {
                pending.push(name.to_owned());
            }
            for child in map.values() {
                collect_definition_refs(child, reached, pending);
            }
        }
        _ => {}
    }
}

fn definition_name(reference: &str) -> Option<&str> {
    let name = reference.strip_prefix("#/$defs/")?;
    if name.is_empty() || name.contains('/') {
        None
    } else {
        Some(name)
    }
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
