use glimpsectl::render::{Section, Table, styled, visible};

fn row<const N: usize>(texts: [&str; N]) -> [String; N] {
    texts.map(str::to_owned)
}

#[test]
fn columns_align_to_their_widest_cell() {
    let rendered = Table::new()
        .with_headers(["NAME", "STATE"])
        .with_rows([row(["audio", "running"])])
        .with_row(row(["network", "degraded"]))
        .render();
    assert_eq!(
        visible(&rendered),
        "NAME     STATE\naudio    running\nnetwork  degraded"
    );
}

#[test]
fn a_styled_cell_does_not_shift_its_column() {
    let rendered = Table::new()
        .with_rows([
            [styled::good("audio"), "running".to_owned()],
            [styled::bad("network"), "degraded".to_owned()],
        ])
        .render();
    assert_eq!(visible(&rendered), "audio    running\nnetwork  degraded");
}

#[test]
fn an_empty_trailing_cell_leaves_no_whitespace() {
    let rendered = Table::new()
        .with_row(row(["audio", "running", ""]))
        .render();
    assert_eq!(visible(&rendered), "audio  running");
    assert!(
        !rendered.ends_with(' '),
        "an empty last cell left its separator behind"
    );
}

#[test]
fn no_rows_says_so_instead_of_printing_headers() {
    let rendered = Table::<2>::new()
        .with_headers(["NAME", "STATE"])
        .with_empty("nothing matches")
        .render();
    assert_eq!(visible(&rendered), "nothing matches");
}

#[test]
fn rows_win_over_the_empty_message() {
    let rendered = Table::new()
        .with_headers(["NAME", "STATE"])
        .with_empty("nothing matches")
        .with_row(row(["audio", "running"]))
        .render();
    assert_eq!(visible(&rendered), "NAME   STATE\naudio  running");
}

#[test]
fn a_section_indents_its_content_under_a_heading() {
    let rendered = Section::new("D-Bus")
        .with("session bus  ok")
        .with_note("network is degraded")
        .render();
    assert_eq!(
        visible(&rendered),
        "D-BUS\n  session bus  ok\n  → network is degraded"
    );
}
