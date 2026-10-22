use super::*;

fn load(text: &str) -> Result<crate::Config, crate::ConfigError> {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let path = dir.path().join("config.toml");
    std::fs::write(&path, text).expect("writes");
    crate::load_files(std::slice::from_ref(&path))
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

#[test]
fn a_latitude_outside_the_globe_is_refused() {
    let rendered = load("[geolocation]\nprovider = \"manual\"\nlatitude = 151.5\nlongitude = 10\n")
        .expect_err("a latitude past the pole is not a location")
        .to_string();

    assert!(
        rendered.contains("latitude is degrees north of the equator, between -90 and 90"),
        "the error must give the latitude range, got {rendered}"
    );
}

#[test]
fn a_longitude_outside_the_globe_is_refused() {
    let rendered = load("[geolocation]\nprovider = \"manual\"\nlatitude = 10\nlongitude = 200\n")
        .expect_err("a longitude past the antimeridian is not a location")
        .to_string();

    assert!(
        rendered.contains("longitude is degrees east of Greenwich, between -180 and 180"),
        "the error must give the longitude range, got {rendered}"
    );
}

#[test]
fn the_edges_of_both_ranges_are_accepted() {
    assert_eq!(
        load("[geolocation]\nprovider = \"manual\"\nlatitude = 90\nlongitude = -180\n")
            .expect("both edges are on the globe")
            .geolocation,
        Geolocation::Manual {
            latitude: 90.0,
            longitude: -180.0
        }
    );
}
