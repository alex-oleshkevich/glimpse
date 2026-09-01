use gtk4::glib;

pub const COLUMNS: usize = 7;
pub const WEEKS: usize = 6;
pub const CELLS: usize = COLUMNS * WEEKS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Ymd {
    pub year: i32,
    pub month: u32,
    pub day: u32,
}

impl Ymd {
    pub fn new(year: i32, month: u32, day: u32) -> Self {
        Self { year, month, day }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cell {
    pub date: Ymd,
    pub in_month: bool,
}

pub fn days_in_month(year: i32, month: u32) -> u32 {
    let first = glib::DateTime::from_utc(year, month as i32, 1, 0, 0, 0.0);
    let last = first
        .and_then(|first| first.add_months(1))
        .and_then(|next| next.add_days(-1));
    last.map(|last| last.day_of_month() as u32).unwrap_or(30)
}

pub fn weekday(year: i32, month: u32, day: u32) -> u32 {
    glib::DateTime::from_utc(year, month as i32, day as i32, 0, 0, 0.0)
        .map(|date| date.day_of_week() as u32)
        .unwrap_or(1)
}

pub fn step_month(year: i32, month: u32, by: i32) -> (i32, u32) {
    let months = year as i64 * 12 + (month as i64 - 1) + by as i64;
    (
        months.div_euclid(12) as i32,
        months.rem_euclid(12) as u32 + 1,
    )
}

pub fn month_grid(year: i32, month: u32, first_weekday: u32) -> [Cell; CELLS] {
    let lead = (weekday(year, month, 1) + COLUMNS as u32 - first_weekday) % COLUMNS as u32;
    let (prev_year, prev_month) = step_month(year, month, -1);
    let (next_year, next_month) = step_month(year, month, 1);
    let this_month = days_in_month(year, month);
    let previous = days_in_month(prev_year, prev_month);

    let mut cells = [Cell {
        date: Ymd::new(year, month, 1),
        in_month: true,
    }; CELLS];

    for (index, cell) in cells.iter_mut().enumerate() {
        let offset = index as i64 - lead as i64;
        *cell = if offset < 0 {
            Cell {
                date: Ymd::new(prev_year, prev_month, (previous as i64 + offset + 1) as u32),
                in_month: false,
            }
        } else if (offset as u32) < this_month {
            Cell {
                date: Ymd::new(year, month, offset as u32 + 1),
                in_month: true,
            }
        } else {
            Cell {
                date: Ymd::new(next_year, next_month, offset as u32 - this_month + 1),
                in_month: false,
            }
        };
    }
    cells
}

#[derive(Debug, Default)]
pub struct Scroll {
    carried: f64,
}

impl Scroll {
    pub fn accumulate(&mut self, delta: f64) -> i32 {
        self.carried += delta;
        let steps = self.carried.trunc();
        self.carried -= steps;
        steps as i32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_month_is_always_six_weeks() {
        for (year, month) in [(2026, 2), (2026, 9), (2024, 2), (2027, 8)] {
            assert_eq!(month_grid(year, month, 1).len(), CELLS);
        }
    }

    #[test]
    fn the_first_of_the_month_lands_under_its_own_weekday() {
        let cells = month_grid(2026, 9, 1);
        let first = cells
            .iter()
            .position(|cell| cell.in_month)
            .expect("the month appears");
        assert_eq!(
            first as u32,
            weekday(2026, 9, 1) - 1,
            "September 2026 starts on a Tuesday, so one leading day is shown"
        );
        assert_eq!(cells[first].date, Ymd::new(2026, 9, 1));
    }

    #[test]
    fn a_week_starting_on_sunday_shifts_the_whole_month() {
        let monday = month_grid(2026, 9, 1);
        let sunday = month_grid(2026, 9, 7);
        let at = |cells: &[Cell], date: Ymd| {
            cells
                .iter()
                .position(|cell| cell.date == date)
                .expect("the date appears")
        };
        assert_eq!(
            at(&sunday, Ymd::new(2026, 9, 1)) - at(&monday, Ymd::new(2026, 9, 1)),
            1,
            "starting the week a day earlier moves every date one cell later"
        );
    }

    #[test]
    fn the_days_around_the_month_come_from_its_neighbours() {
        let cells = month_grid(2026, 9, 1);
        assert_eq!(cells[0].date, Ymd::new(2026, 8, 31));
        assert!(!cells[0].in_month);
        let last = cells.last().expect("a last cell");
        assert_eq!(last.date.month, 10);
        assert!(!last.in_month);
    }

    #[test]
    fn stepping_past_december_carries_the_year() {
        assert_eq!(step_month(2026, 12, 1), (2027, 1));
        assert_eq!(step_month(2026, 1, -1), (2025, 12));
        assert_eq!(step_month(2026, 6, -18), (2024, 12));
    }

    #[test]
    fn february_knows_about_leap_years() {
        assert_eq!(days_in_month(2024, 2), 29);
        assert_eq!(days_in_month(2026, 2), 28);
        assert_eq!(days_in_month(2000, 2), 29);
        assert_eq!(days_in_month(1900, 2), 28);
    }

    #[test]
    fn scrolling_carries_a_remainder_instead_of_stepping_per_event() {
        let mut scroll = Scroll::default();
        assert_eq!(scroll.accumulate(0.3), 0);
        assert_eq!(scroll.accumulate(0.3), 0);
        assert_eq!(
            scroll.accumulate(0.5),
            1,
            "three tenths at a time still reaches one step"
        );
        assert_eq!(scroll.accumulate(-1.2), -1);
    }
}
