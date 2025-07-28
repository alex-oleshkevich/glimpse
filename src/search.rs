// use iced::futures::{SinkExt, channel::mpsc};
// use serde::Deserialize;

// use crate::{extensions::ExtensionManager, icons::Icon};

// #[derive(Debug, Clone, Deserialize)]
// pub struct Action {}

// #[derive(Debug, Clone, Deserialize)]
// pub struct SearchItem {
//     pub title: String,
//     pub subtitle: String,
//     pub category: String,
//     pub icon: Icon,
//     pub actions: Vec<Action>,
// }

// impl SearchItem {
//     pub fn primary_action(&self) -> Option<&Action> {
//         self.actions.first()
//     }
// }

// pub struct Search {}

// impl Search {
//     pub fn new() -> Self {
//         Self {}
//     }

//     // pub async fn search(
//     //     &self,
//     //     extensions: &ExtensionManager,
//     //     query: String,
//     // ) -> mpsc::Receiver<Option<SearchItem>> {
//     //     let (sender, receiver) = mpsc::channel(10);
//     //     let mut sender = sender.clone();
//     //     let extensions = extensions.clone();

//     //     tokio::spawn(async move {
//     //         let results = extensions.query(query.clone()).await;
//     //         let _ = sender
//     //             .send(Some(SearchItem {
//     //                 title: query.clone(),
//     //                 subtitle: "Some dummy subtitle".to_string(),
//     //                 category: "Dummy Category".to_string(),
//     //                 icon: Icon::Path(
//     //                     "/usr/share/icons/Adwaita/scalable/devices/phone.svg".to_string(),
//     //                 ),
//     //                 actions: vec![],
//     //             }))
//     //             .await;
//     //         let _ = sender.send(None).await;
//     //     });

//     //     receiver
//     // }
// }
