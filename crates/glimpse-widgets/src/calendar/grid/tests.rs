use super::*;

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
