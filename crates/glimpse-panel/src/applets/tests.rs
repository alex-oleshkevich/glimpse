use super::*;

fn custom(name: &str, extends: AppletConfig) -> BTreeMap<String, AppletConfig> {
    BTreeMap::from([(name.to_owned(), extends)])
}

mod configured;
