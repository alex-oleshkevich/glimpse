use crate::{
    commands::{Action, Command},
    extensions::{Extension, ExtensionMetadata},
};

pub struct Shell {}

impl Shell {
    pub fn new() -> Self {
        Shell {}
    }
}

impl Extension for Shell {
    fn id(&self) -> String {
        "me.aresa.glimpse.shell".to_string()
    }

    fn metadata(&self) -> ExtensionMetadata {
        ExtensionMetadata {
            name: "Shell".to_string(),
            description: "Executes shell commands".to_string(),
            version: "1.0.0".to_string(),
            author: "Alex Oleshkevich".to_string(),
        }
    }

    fn query(&self, query: &crate::messages::Message) -> Vec<Command> {
        match query {
            crate::messages::Message::Query(q) => {
                let command = q.split_whitespace().next().unwrap_or("");
                let args = q
                    .split_whitespace()
                    .skip(1)
                    .collect::<Vec<_>>()
                    .to_vec()
                    .into_iter()
                    .map(|s| s.to_string())
                    .collect::<Vec<_>>();

                let paths_var = std::env::var("PATH").unwrap_or_default();
                let mut paths = paths_var
                    .split(':')
                    .map(|p| p.to_string())
                    .collect::<Vec<_>>();
                paths.push("/bin".to_string());
                paths.push("/usr/bin".to_string());
                paths.push("/usr/local/bin".to_string());

                let mut full_path = command.to_string();
                if !full_path.starts_with('/') {
                    for p in paths {
                        full_path = format!("{}/{}", p, command.to_string());
                        let path_info = std::path::Path::new(&full_path);
                        if path_info.exists() && path_info.is_file() {
                            break;
                        }
                    }
                }

                let path_info = std::path::Path::new(&full_path);
                if !path_info.exists() || !path_info.is_file() {
                    return vec![];
                }

                return vec![Command::new(
                    q.to_string(),
                    "Execute shell command".to_string(),
                    "utilities-terminal".to_string(),
                    vec![Action::ShellExec(
                        path_info.to_string_lossy().to_string(),
                        args,
                    )],
                )];
            }
            _ => vec![],
        }
    }
}
