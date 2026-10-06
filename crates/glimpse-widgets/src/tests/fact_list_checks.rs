use super::*;

pub(super) fn check() {
    let facts = FactList::new();
    facts.set_facts(&[
        Fact::new("Humidity", "78%"),
        Fact::warning("Wind", "14 km/h"),
    ]);
    let fact_rows: Vec<Row> = children_of(&facts);
    assert_eq!(fact_rows.len(), 2);
    assert_eq!(fact_rows[0].title().as_deref(), Some("Humidity"));
    assert_eq!(fact_rows[0].value().as_deref(), Some("78%"));
    assert_eq!(fact_rows[0].tone(), RowTone::Normal);
    assert_eq!(fact_rows[1].value().as_deref(), Some("14 km/h"));
    assert_eq!(fact_rows[1].tone(), RowTone::Warning);
    facts.set_facts(&[Fact::new("Humidity", "80%")]);
    assert_eq!(children_of::<Row>(&facts).len(), 1);
    assert_eq!(
        fact_rows[0],
        children_of::<Row>(&facts)[0],
        "a position reuses its row"
    );
    let long = "я".repeat(TEXT_MAX_CHARS + 40);
    facts.set_facts(&[Fact::new("Pressure", &long)]);
    assert_eq!(
        children_of::<Row>(&facts)[0]
            .value()
            .unwrap()
            .chars()
            .count(),
        TEXT_MAX_CHARS
    );
}
