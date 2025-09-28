use std::collections::HashMap;

use glimpse_sdk::{Message, Method};
use tokio::{process::Command, sync::mpsc};

pub async fn shell_exec(command: &str, args: &Vec<String>) {
    tracing::debug!("executing command: {} {:?}", command, args);
    let command = command.to_string();
    let args = args.clone();
    tokio::spawn(async move {
        if let Err(err) = Command::new(&command).args(&args).spawn() {
            tracing::error!("failed to execute command: {}", err);
        } else {
            tracing::debug!("executed command: {} {:?}", command, args);
        }
    });
}

pub async fn launch_app(app: &str, args: &Vec<String>, new_instance: bool) {
    tracing::debug!(
        "launching app: {} {:?} (new_instance={})",
        app,
        args,
        new_instance
    );
    // if let Err(err) = Command::new(app).args(args).spawn() {
    //     tracing::error!("failed to launch app: {}", err);
    // } else {
    //     tracing::debug!("launched app: {} {:?}", app, args);
    // }
}

pub async fn copy_to_clipboard(text: &str) {
    let text = text.to_string();
    tokio::spawn(async move {
        tracing::debug!("copying to clipboard: {}", text);
        if let Err(err) = Command::new("wl-copy").arg(&text).spawn() {
            tracing::error!("failed to copy to clipboard: {}", err);
        } else {
            tracing::debug!("copied to clipboard: {}", text);
        }
    });
}

pub async fn open_url(uri: &str) {
    tracing::debug!("opening uri: {}", uri);
    let uri = uri.to_string();
    tokio::spawn(async move {
        if let Err(err) = Command::new("xdg-open").arg(&uri).spawn() {
            tracing::error!("failed to open uri: {}", err);
        } else {
            tracing::debug!("opened uri: {}", uri);
        }
    });
}

pub async fn plugin_callback(
    plugin_tx: mpsc::Sender<Message>,
    key: &str,
    params: &HashMap<String, String>,
) {
    tracing::debug!("call plugin callback: {} {:?}", key, params);
    let key = key.to_string();
    let params = params.clone();
    let plugin_tx = plugin_tx.clone();
    tokio::spawn(async move {
        if let Err(err) = plugin_tx
            .send(Message::Notification {
                method: Method::CallAction(key.clone(), params),
                plugin_id: None,
            })
            .await
        {
            tracing::error!("failed to send plugin callback: {}", err);
        }
    });
}
