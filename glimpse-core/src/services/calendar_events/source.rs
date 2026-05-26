use super::model::{CalendarEvent, CalendarSource};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SourceSnapshot {
    pub source: CalendarSource,
    pub events: Vec<CalendarEvent>,
}
