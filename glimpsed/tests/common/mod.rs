use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use glimpse_sdk::{Message, Metadata, Method, MethodResult};
use serde_json;
use tempfile::TempDir;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::time::timeout;

#[allow(dead_code)]
pub struct MockPlugin {
    pub name: String,
    pub responses: Vec<Message>,
    pub delay: Duration,
    pub should_crash: bool,
    pub invalid_json: bool,
    pub process: Option<Child>,
}

#[allow(dead_code)]
impl MockPlugin {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            responses: Vec::new(),
            delay: Duration::from_millis(0),
            should_crash: false,
            invalid_json: false,
            process: None,
        }
    }

    pub fn with_responses(mut self, responses: Vec<Message>) -> Self {
        self.responses = responses;
        self
    }

    pub fn with_delay(mut self, delay: Duration) -> Self {
        self.delay = delay;
        self
    }

    pub fn with_crash(mut self) -> Self {
        self.should_crash = true;
        self
    }

    pub fn with_invalid_json(mut self) -> Self {
        self.invalid_json = true;
        self
    }
}

#[allow(dead_code)]
pub struct TestHarness {
    pub temp_dir: TempDir,
    pub plugin_dir: PathBuf,
    pub plugins: HashMap<String, MockPlugin>,
}

#[allow(dead_code)]
impl TestHarness {
    pub fn new() -> Self {
        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let plugin_dir = temp_dir.path().join("plugins");
        std::fs::create_dir_all(&plugin_dir).expect("Failed to create plugin dir");

        Self {
            temp_dir,
            plugin_dir,
            plugins: HashMap::new(),
        }
    }

    pub fn add_plugin(&mut self, plugin: MockPlugin) {
        let plugin_path = self.plugin_dir.join(&plugin.name);
        self.create_mock_plugin_binary(&plugin_path, &plugin);
        self.plugins.insert(plugin.name.clone(), plugin);
    }

    pub fn plugin_dir_path(&self) -> String {
        self.plugin_dir.to_string_lossy().to_string()
    }

    fn create_mock_plugin_binary(&self, path: &Path, plugin: &MockPlugin) {
        let script_content = if plugin.invalid_json {
            r#"#!/bin/bash
echo "invalid json content"
"#
        } else if plugin.should_crash {
            r#"#!/bin/bash
exit 1
"#
        } else {
            &format!(
                r#"#!/bin/bash
while IFS= read -r line; do
    sleep {}
    echo '{{"id": 1, "result": {{"Authenticate": {{"id": "{}", "name": "{}", "version": "1.0.0", "description": "Test plugin", "author": "Test"}}}}, "source": "{}"}}'
done
"#,
                plugin.delay.as_secs(),
                plugin.name,
                plugin.name,
                plugin.name
            )
        };

        std::fs::write(path, script_content).expect("Failed to write mock plugin");

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(path).unwrap().permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(path, perms).unwrap();
        }
    }

    pub fn create_non_executable_file(&self, name: &str) {
        let path = self.plugin_dir.join(name);
        std::fs::write(&path, "non-executable content").expect("Failed to write file");
    }

    pub fn create_directory(&self, name: &str) {
        let path = self.plugin_dir.join(name);
        std::fs::create_dir(&path).expect("Failed to create directory");
    }
}

#[allow(dead_code)]
pub async fn send_message_to_daemon(
    stdin: &mut tokio::process::ChildStdin,
    message: &Message,
) -> Result<(), Box<dyn std::error::Error>> {
    let json = serde_json::to_string(message)?;
    stdin.write_all(json.as_bytes()).await?;
    stdin.write_all(b"\n").await?;
    stdin.flush().await?;
    Ok(())
}

#[allow(dead_code)]
pub async fn read_message_from_daemon(
    stdout: &mut BufReader<tokio::process::ChildStdout>,
) -> Result<Message, Box<dyn std::error::Error>> {
    let mut line = String::new();
    stdout.read_line(&mut line).await?;
    let message: Message = serde_json::from_str(&line)?;
    Ok(message)
}

pub fn create_search_request(id: usize, query: &str) -> Message {
    Message::Request {
        id,
        method: Method::Search(query.to_string()),
        target: None,
        context: None,
    }
}

#[allow(dead_code)]
pub fn create_cancel_request(id: usize) -> Message {
    Message::Request {
        id,
        method: Method::Cancel,
        target: None,
        context: None,
    }
}

#[allow(dead_code)]
pub fn create_quit_request(id: usize) -> Message {
    Message::Request {
        id,
        method: Method::Quit,
        target: None,
        context: None,
    }
}

#[allow(dead_code)]
pub fn create_auth_response(id: usize, plugin_name: &str) -> Message {
    Message::Response {
        id,
        error: None,
        source: Some(plugin_name.to_string()),
        result: Some(MethodResult::Authenticate(Metadata {
            id: plugin_name.to_string(),
            name: plugin_name.to_string(),
            version: "1.0.0".to_string(),
            description: "Test plugin".to_string(),
            author: "Test".to_string(),
        })),
    }
}

#[allow(dead_code)]
pub async fn with_timeout<T, F>(
    duration: Duration,
    future: F,
) -> Result<T, Box<dyn std::error::Error>>
where
    F: std::future::Future<Output = T>,
{
    timeout(duration, future)
        .await
        .map_err(|_| "Operation timed out".into())
}

#[allow(dead_code)]
pub struct SignalTester {
    child: Option<Child>,
}

#[allow(dead_code)]
impl SignalTester {
    pub fn new() -> Self {
        Self { child: None }
    }

    pub async fn spawn_daemon(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let mut cmd = Command::new("cargo");
        cmd.args(&["run", "--bin", "glimpsed"])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .current_dir(std::env::current_dir().unwrap());

        self.child = Some(cmd.spawn()?);
        Ok(())
    }

    pub async fn send_signal(&mut self, signal: i32) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(child) = &mut self.child {
            #[cfg(unix)]
            {
                use nix::sys::signal::{self, Signal};
                use nix::unistd::Pid;

                let pid = Pid::from_raw(child.id().unwrap() as i32);
                let signal = match signal {
                    15 => Signal::SIGTERM,
                    2 => Signal::SIGINT,
                    _ => return Err("Unsupported signal".into()),
                };
                signal::kill(pid, signal)?;
            }
        }
        Ok(())
    }

    pub async fn wait_for_exit(
        &mut self,
    ) -> Result<std::process::ExitStatus, Box<dyn std::error::Error>> {
        if let Some(mut child) = self.child.take() {
            Ok(child.wait().await?)
        } else {
            Err("No child process".into())
        }
    }
}
