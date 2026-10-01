use super::*;

fn test_placement() -> glimpse_services::Placement {
    glimpse_services::Placement {
        output: None,
        position: glimpse_services::Edge::Top,
        orientation: glimpse_services::Orientation::Horizontal,
        zone: glimpse_services::Zone::Center,
        size: 32,
    }
}

fn custom(name: &str, extends: AppletConfig) -> BTreeMap<String, AppletConfig> {
    BTreeMap::from([(name.to_owned(), extends)])
}

mod builders;
mod configured;
