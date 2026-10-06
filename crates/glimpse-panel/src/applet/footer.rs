use std::cell::RefCell;
use std::rc::Rc;

use glimpse_config::AppletCommon;

use super::popover::run;

type Entry = (String, Vec<String>);

#[derive(Clone, Default)]
pub struct Footer(Rc<RefCell<Option<Entry>>>);

impl Footer {
    pub fn configure(&self, common: &AppletCommon) {
        self.set(
            common
                .settings()
                .map(|(label, command)| (label.to_owned(), command.to_vec())),
        );
    }

    pub fn set(&self, footer: Option<Entry>) {
        self.0.replace(footer);
    }

    pub fn label(&self) -> Option<String> {
        self.0.borrow().as_ref().map(|(label, _)| label.clone())
    }

    pub fn run(&self) {
        self.run_with(run);
    }

    fn run_with(&self, launch: impl FnOnce(&[String])) {
        if let Some((_, command)) = self.0.borrow().as_ref() {
            launch(command);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ran(footer: &Footer) -> Option<Vec<String>> {
        let mut ran = None;
        footer.run_with(|command| ran = Some(command.to_vec()));
        ran
    }

    fn command(program: &str) -> Option<Entry> {
        Some(("Settings".to_owned(), vec![program.to_owned()]))
    }

    #[test]
    fn a_footer_reconfigured_after_it_was_cloned_runs_the_new_command() {
        let footer = Footer::default();
        footer.set(command("old"));
        let held = footer.clone();
        footer.set(command("new"));
        assert_eq!(ran(&held), Some(vec!["new".to_owned()]));
    }

    #[test]
    fn a_footer_unconfigured_when_cloned_runs_the_command_set_later() {
        let footer = Footer::default();
        let held = footer.clone();
        assert_eq!(ran(&held), None);
        assert_eq!(held.label(), None);
        footer.set(command("later"));
        assert_eq!(ran(&held), Some(vec!["later".to_owned()]));
        assert_eq!(held.label().as_deref(), Some("Settings"));
    }

    #[test]
    fn a_footer_cleared_after_it_was_cloned_runs_nothing() {
        let footer = Footer::default();
        footer.set(command("gone"));
        let held = footer.clone();
        footer.set(None);
        assert_eq!(ran(&held), None);
    }
}
