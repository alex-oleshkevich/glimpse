pub struct Facts<'a> {
    pub index: Option<u64>,
    pub id: u64,
    pub name: Option<&'a str>,
    pub workspace: Option<&'a str>,
}

impl Facts<'_> {
    fn ordinal(&self) -> String {
        match self.index {
            Some(index) => index.to_string(),
            None => self.id.to_string(),
        }
    }
}

pub fn render(template: &str, facts: &Facts) -> String {
    let ordinal = facts.ordinal();
    let name = facts.name.unwrap_or_default();

    template
        .replace("{workspace-name}", facts.workspace.unwrap_or_default())
        .replace(
            "{name-or-index}",
            match name.is_empty() {
                true => &ordinal,
                false => name,
            },
        )
        .replace("{index}", &ordinal)
        .replace("{id}", &facts.id.to_string())
        .replace("{name}", name)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn niri() -> Facts<'static> {
        Facts {
            index: Some(3),
            id: 42,
            name: Some("chat"),
            workspace: Some("work"),
        }
    }

    #[test]
    fn every_token_resolves_to_its_own_fact() {
        assert_eq!(render("{index}", &niri()), "3");
        assert_eq!(render("{id}", &niri()), "42");
        assert_eq!(render("{name}", &niri()), "chat");
    }

    #[test]
    fn an_index_falls_back_to_the_id_the_way_hyprland_numbers_workspaces() {
        let hyprland = Facts {
            index: None,
            id: 5,
            name: Some("5"),
            workspace: None,
        };

        assert_eq!(
            render("{index}", &hyprland),
            "5",
            "only niri fills idx, and a Hyprland workspace's id is the number the user typed"
        );
    }

    #[test]
    fn name_or_index_prefers_the_name_and_never_renders_empty() {
        assert_eq!(render("{name-or-index}", &niri()), "chat");

        let unnamed = Facts {
            name: None,
            ..niri()
        };
        assert_eq!(render("{name-or-index}", &unnamed), "3");

        let blank = Facts {
            name: Some(""),
            ..niri()
        };
        assert_eq!(render("{name-or-index}", &blank), "3");
    }

    #[test]
    fn a_longer_token_is_not_eaten_by_a_shorter_one_it_contains() {
        let unnamed = Facts {
            name: None,
            ..niri()
        };

        assert_eq!(
            render("{name-or-index}", &unnamed),
            "3",
            "the shorter token is a prefix of the longer one, so the order of the replacements \
             is what keeps the longer one whole"
        );
    }

    #[test]
    fn a_window_numbers_itself_by_position_without_losing_the_text_around_it() {
        let window = Facts {
            index: Some(2),
            id: 94_388_234_684_768,
            name: Some("ghostty"),
            workspace: Some("work"),
        };

        assert_eq!(
            render("w{index}", &window),
            "w2",
            "a window has no compositor index, so the applet passes its position as one; \
             resolving it outside the template would drop everything around the token"
        );
    }

    #[test]
    fn an_unnamed_workspace_falls_back_to_its_index() {
        let unnamed = Facts {
            index: Some(4),
            id: 7,
            name: None,
            workspace: Some("4"),
        };

        assert_eq!(
            render("{workspace-name}", &unnamed),
            "4",
            "an empty slot where a name would be reads as a broken template, not as an unnamed \
             workspace"
        );
    }

    #[test]
    fn workspace_name_is_the_only_way_to_name_the_workspace_while_showing_windows() {
        let window = Facts {
            index: Some(2),
            id: 94_388_234_684_768,
            name: Some("ghostty"),
            workspace: Some("work"),
        };

        assert_eq!(render("{name}", &window), "ghostty");
        assert_eq!(
            render("{workspace-name}", &window),
            "work",
            "in windows mode the name token is the window's, so the workspace needs its own"
        );
        assert_eq!(
            render(
                "{workspace-name}",
                &Facts {
                    workspace: None,
                    ..window
                }
            ),
            "",
            "an unnamed workspace renders as nothing rather than as the word None"
        );
    }

    #[test]
    fn text_around_a_token_survives() {
        assert_eq!(render("ws {index}", &niri()), "ws 3");
        assert_eq!(render("", &niri()), "");
    }
}
