use chrono::{DateTime, NaiveDate, Utc};
use glimpse_dbus::weather::GeoCoordinates;
use sunrise::{Coordinates, SolarDay, SolarEvent};

/// Sunrise and sunset, each absent on a day the sun does not cross the horizon.
pub type Events = (Option<DateTime<Utc>>, Option<DateTime<Utc>>);

/// Sunrise and sunset for a place on a day.
///
/// `None` means the coordinates are not on Earth, which is a caller error; `Some((None, None))`
/// means the sun did not cross the horizon there that day, which is an answer. Collapsing the two
/// would leave a polar day indistinguishable from a bad fix.
pub fn events(coordinates: &GeoCoordinates, date: NaiveDate) -> Option<Events> {
    let place = Coordinates::new(coordinates.latitude, coordinates.longitude)?;
    let day = SolarDay::new(place, date);

    Some((
        day.event_time(SolarEvent::Sunrise),
        day.event_time(SolarEvent::Sunset),
    ))
}
