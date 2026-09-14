use std::sync::OnceLock;

use glimpse_contracts::{GeoCoordinates, WatchedPlace};
use reverse_geocoder::ReverseGeocoder;
use serde::Deserialize;

use super::fetch_json;

const SEARCH: &str = "https://geocoding-api.open-meteo.com/v1/search";
const CITY: usize = 100;

#[derive(Debug, Clone, PartialEq)]
pub(super) struct ResolvedPlace {
    pub coordinates: GeoCoordinates,
    pub city: String,
    pub country_code: String,
}

pub(super) async fn resolve(
    client: &reqwest::Client,
    place: &WatchedPlace,
    coordinates: Option<GeoCoordinates>,
) -> Result<ResolvedPlace, String> {
    match place {
        WatchedPlace::Location { name } => forward(client, name).await,
        WatchedPlace::Here | WatchedPlace::Coordinates { .. } => {
            let coordinates = coordinates.ok_or("there is no location fix yet")?;
            tokio::task::spawn_blocking(move || reverse(coordinates))
                .await
                .map_err(|_| "the place resolver stopped".to_owned())
        }
    }
}

pub(super) fn normalize(name: &str) -> Result<String, String> {
    let (city, country_code) = named(name)?;
    Ok(format!("{city}, {country_code}"))
}

async fn forward(client: &reqwest::Client, name: &str) -> Result<ResolvedPlace, String> {
    let (city, country_code) = named(name)?;
    let url = reqwest::Url::parse_with_params(
        SEARCH,
        &[
            ("name", city),
            ("countryCode", country_code.clone()),
            ("count", "1".to_owned()),
            ("language", "en".to_owned()),
            ("format", "json".to_owned()),
        ],
    )
    .map_err(|_| "the location request could not be built".to_owned())?;
    let result: Search = fetch_json(client, url).await?;
    let found = result
        .results
        .into_iter()
        .next()
        .ok_or_else(|| format!("no location matches {name}"))?;

    Ok(ResolvedPlace {
        coordinates: GeoCoordinates {
            latitude: found.latitude,
            longitude: found.longitude,
        },
        city: glimpse_utils::clean(&found.name, CITY),
        country_code: found.country_code.to_ascii_uppercase(),
    })
}

fn named(name: &str) -> Result<(String, String), String> {
    let (city, country_code) = name
        .rsplit_once(',')
        .ok_or("a location is written as City, CC")?;
    let city = city.trim();
    let country_code = country_code.trim();
    if city.is_empty() || city.chars().count() > CITY {
        return Err("a location needs a city of at most 100 characters".to_owned());
    }
    if country_code.len() != 2 || !country_code.bytes().all(|byte| byte.is_ascii_alphabetic()) {
        return Err("a location needs a two-letter country code".to_owned());
    }

    Ok((city.to_owned(), country_code.to_ascii_uppercase()))
}

fn reverse(coordinates: GeoCoordinates) -> ResolvedPlace {
    static GEOCODER: OnceLock<ReverseGeocoder> = OnceLock::new();

    let result = GEOCODER
        .get_or_init(ReverseGeocoder::new)
        .search((coordinates.latitude, coordinates.longitude));
    ResolvedPlace {
        coordinates,
        city: glimpse_utils::clean(&result.record.name, CITY),
        country_code: result.record.cc.to_ascii_uppercase(),
    }
}

#[derive(Deserialize)]
struct Search {
    #[serde(default)]
    results: Vec<SearchResult>,
}

#[derive(Deserialize)]
struct SearchResult {
    name: String,
    latitude: f64,
    longitude: f64,
    country_code: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_named_location_is_split_and_normalized() {
        assert_eq!(
            named("Vilnius, lt"),
            Ok(("Vilnius".to_owned(), "LT".to_owned()))
        );
        assert!(named("Vilnius").is_err());
        assert!(named("Vilnius, Lithuania").is_err());
    }

    #[tokio::test]
    async fn coordinates_resolve_without_a_network_service() {
        let place = tokio::task::spawn_blocking(|| {
            reverse(GeoCoordinates {
                latitude: 52.2297,
                longitude: 21.0122,
            })
        })
        .await
        .unwrap();

        assert_eq!(place.city, "Warsaw");
        assert_eq!(place.country_code, "PL");
    }
}
