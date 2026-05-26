mod aggregate;
mod dedupe;
mod ical;
mod local;
pub mod model;
mod provider;
mod service;
mod source;

pub use model::{
    CalendarDate, CalendarDaySnapshot, CalendarEvent, CalendarMonthSnapshot, Command, MonthKey,
    State,
};
pub use service::{CalendarEventsHandle, CalendarEventsService};
