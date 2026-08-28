use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// `manual` carries its coordinates in the table that selects it, so a provider without them cannot
/// be written down: serde refuses the document and names the key that is missing. The alternative —
/// two loose `Option` fields — makes a half-filled table a runtime problem for every reader.
#[derive(Debug, Clone, Copy, Default, PartialEq, Deserialize, Serialize, JsonSchema)]
#[serde(tag = "provider", rename_all = "kebab-case")]
pub enum Geolocation {
    #[default]
    Geoclue,
    Manual {
        latitude: f64,
        longitude: f64,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn load(text: &str) -> Result<crate::Config, crate::ConfigError> {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let path = dir.path().join("config.toml");
        std::fs::write(&path, text).expect("writes");
        crate::load(Some(&path))
    }

    #[test]
    fn a_manual_table_carries_the_pair_it_names() {
        let parsed =
            load("[geolocation]\nprovider = \"manual\"\nlatitude = 51.5074\nlongitude = -0.1278\n")
                .expect("both keys are present");

        assert_eq!(
            parsed.geolocation,
            Geolocation::Manual {
                latitude: 51.5074,
                longitude: -0.1278
            }
        );
    }

    /// The whole point of the tagged enum: a half-filled table is refused at load, naming the key
    /// that is missing, rather than reaching a service as a location it has to degrade over.
    #[test]
    fn a_manual_table_is_refused_unless_both_coordinates_are_given() {
        for (text, missing) in [
            ("[geolocation]\nprovider = \"manual\"\n", "latitude"),
            (
                "[geolocation]\nprovider = \"manual\"\nlatitude = 51.5074\n",
                "longitude",
            ),
            (
                "[geolocation]\nprovider = \"manual\"\nlongitude = -0.1278\n",
                "latitude",
            ),
        ] {
            let rendered = load(text)
                .expect_err("half a pair is not a location")
                .to_string();

            assert!(
                rendered.contains(missing),
                "the error must name `{missing}`, got {rendered}"
            );
        }
    }

    #[test]
    fn geoclue_needs_no_coordinates_and_is_the_default() {
        assert_eq!(
            load("[geolocation]\nprovider = \"geoclue\"\n")
                .expect("geoclue stands alone")
                .geolocation,
            Geolocation::Geoclue
        );
        assert_eq!(
            load("").expect("an absent table is fine").geolocation,
            Geolocation::Geoclue
        );
    }

    #[test]
    fn an_unknown_provider_is_refused() {
        load("[geolocation]\nprovider = \"gps\"\n").expect_err("`gps` is not a provider");
    }
}
