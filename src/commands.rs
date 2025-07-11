#[derive(Debug, Clone)]
pub enum Action {
    LaunchApp { app_id: String },
    ShellExec(),
    CopyToClipboard(),
    OpenUrl(),
    OpenFile(),
    OpenFolder(),
}

#[derive(Debug, Clone)]
pub struct Command {
    id: String,
    pub title: String,
    pub subtitle: String,
    pub icon: String,
    pub actions: Vec<Action>,
}

impl Command {
    pub fn new(title: String, subtitle: String, icon: String, actions: Vec<Action>) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            title,
            subtitle,
            icon,
            actions,
        }
    }

    pub fn id(&self) -> String {
        self.id.clone()
    }

    pub fn primary_action(&self) -> Option<&Action> {
        self.actions.first()
    }

    pub fn secondary_action(&self) -> Option<&Action> {
        if self.actions.len() > 1 {
            Some(&self.actions[1])
        } else {
            None
        }
    }
}
