use super::*;

pub(super) fn check() {
    let facts = FactList::new();
    facts.set_facts(&[Fact::new("Humidity", "78%"), Fact::new("Wind", "14 km/h")]);
    let fact_rows: Vec<LegacyRow> = children_of(&facts);
    assert_eq!(fact_rows.len(), 2);
    assert_eq!(fact_rows[0].title().as_deref(), Some("Humidity"));
    assert_eq!(fact_rows[1].value().as_deref(), Some("14 km/h"));
    assert!(
        !fact_rows[0].activatable(),
        "a fact states something; it does not lead anywhere, so it does not light up"
    );
    facts.set_facts(&[Fact::new("Humidity", "80%")]);
    assert_eq!(children_of::<LegacyRow>(&facts).len(), 1);
    assert_eq!(
        fact_rows[0],
        children_of::<LegacyRow>(&facts)[0],
        "a position reuses its row"
    );
}
