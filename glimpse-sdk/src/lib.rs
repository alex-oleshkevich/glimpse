pub enum Kind {
    LaunchApp,  // launch an application
    ExecShell,  // execute a shell command
    Entrypoint, // opens a detail view when activated
    Generator,  // generates new commands
    Inline,     // display content in a result row
}

pub trait Action {
    fn title(&self) -> String;
    fn activate(&self, command: &Command) -> String;
}

pub struct Command {
    pub title: String,
    pub subtitle: String,
    pub category: Option<String>,
    pub icon: String,
    pub kind: Kind,
}

pub struct Extension {
    pub name: String,
    pub description: String,
    pub version: String,
    pub author: String,
}

impl Extension {
    pub fn new(name: &str, description: &str, version: &str, author: &str) -> Self {
        Self {
            name: name.to_string(),
            description: description.to_string(),
            version: version.to_string(),
            author: author.to_string(),
        }
    }

    pub fn query(&self, query: &str) -> Vec<Command> {
        vec![]
    }
}
