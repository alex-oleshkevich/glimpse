// use tokio::sync::mpsc;
// use tokio_stream::StreamExt;
// use zbus::proxy;

// #[proxy(
//     interface = "org.gnome.Shell",
//     default_service = "org.gnome.Shell",
//     default_path = "/org/gnome/Shell"
// )]
// trait GnomeShell {
//     async fn grab_accelerator(
//         &self,
//         accelerator: &str,
//         mode_flags: u32,
//         grab_flags: u32,
//     ) -> zbus::Result<u32>;
//     async fn ungrab_accelerator(&self, action_id: u32) -> zbus::Result<bool>;

//     #[zbus(signal)]
//     async fn accelerator_activated(
//         &self,
//         action_id: u32,
//         device_id: u32,
//         timestamp: u32,
//     ) -> zbus::Result<()>;
// }

// async fn setup_hotkey_listener(
//     dbus_tx: mpsc::UnboundedSender<DbusCommand>,
// ) -> Result<(), Box<dyn std::error::Error>> {
//     let connection = zbus::Connection::session().await?;
//     let shell_proxy = GnomeShellProxy::new(&connection).await?;

//     // Register hotkeys (e.g., Super+Space)
//     let hotkeys = vec![
//         ("Super_L+space", "toggle_glimpse"),
//         ("Super_L+g", "show_glimpse"),
//     ];

//     let mut action_ids = Vec::new();
//     for (hotkey, _action) in &hotkeys {
//         match shell_proxy.grab_accelerator(hotkey, 0, 0).await {
//             Ok(action_id) => {
//                 action_ids.push(action_id);
//                 tracing::info!("Registered hotkey: {} -> {}", hotkey, action_id);
//             }
//             Err(e) => tracing::warn!("Failed to register hotkey {}: {}", hotkey, e),
//         }
//     }

//     // Listen for hotkey activations
//     tokio::spawn(async move {
//         let mut accelerator_stream = shell_proxy.receive_accelerator_activated().await.unwrap();

//         while let Some(signal) = accelerator_stream.next().await {
//             if let Ok(args) = signal.args() {
//                 let action_id = args.action_id;
//                 tracing::debug!("Hotkey activated: action_id={}", action_id);

//                 // Map action_id back to command
//                 if action_ids.contains(&action_id) {
//                     dbus_tx.send(DbusCommand::Toggle).ok();
//                 }
//             }
//         }
//     });

//     Ok(())
// }
