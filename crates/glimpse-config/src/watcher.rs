use tokio::sync::mpsc;

use crate::Config;

pub async fn watch(_sender: mpsc::Sender<Config>) {}
