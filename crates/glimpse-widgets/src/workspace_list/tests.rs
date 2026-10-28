use super::{Workspace, grouped};

fn workspace(id: u64, output: &str) -> Workspace {
    Workspace {
        id,
        label: format!("ws{id}"),
        output: output.to_owned(),
        ..Workspace::default()
    }
}

fn shape(workspaces: &[Workspace]) -> Vec<(String, Vec<u64>)> {
    grouped(workspaces)
        .into_iter()
        .map(|(output, group)| {
            (
                output,
                group
                    .iter()
                    .map(|workspace| workspace.id)
                    .collect::<Vec<u64>>(),
            )
        })
        .collect()
}

#[test]
fn workspaces_on_one_display_are_one_section() {
    assert_eq!(
        shape(&[
            workspace(1, "DP-2"),
            workspace(2, "DP-2"),
            workspace(9, "eDP-1"),
        ]),
        [
            ("DP-2".to_owned(), vec![1, 2]),
            ("eDP-1".to_owned(), vec![9]),
        ]
    );
}

#[test]
fn a_display_that_comes_back_later_is_a_second_section() {
    assert_eq!(
        shape(&[
            workspace(1, "DP-2"),
            workspace(9, "eDP-1"),
            workspace(3, "DP-2"),
        ]),
        [
            ("DP-2".to_owned(), vec![1]),
            ("eDP-1".to_owned(), vec![9]),
            ("DP-2".to_owned(), vec![3]),
        ],
        "grouping follows the order the compositor reports, so a section header never claims \
             workspaces that are listed somewhere else"
    );
}

#[test]
fn a_session_with_no_workspaces_has_no_sections() {
    assert!(shape(&[]).is_empty());
}
