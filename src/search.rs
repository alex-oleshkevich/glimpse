#[derive(Debug, Clone)]
pub enum Icon {
    Path(String),
}

#[derive(Debug, Clone)]
pub struct Action {}

#[derive(Debug, Clone)]
pub struct SearchItem {
    pub title: String,
    pub subtitle: String,
    pub category: String,
    pub icon: Icon,
    pub actions: Vec<Action>,
}

impl SearchItem {
    pub fn primary_action(&self) -> Option<&Action> {
        self.actions.first()
    }
}

pub struct Search {}

impl Search {
    pub fn new() -> Self {
        Self {}
    }

    pub async fn search(&self, query: String) -> Vec<SearchItem> {
        vec![
            SearchItem {
                title: "Example Item".to_string(),
                subtitle: "This is an example subtitle".to_string(),
                icon: Icon::Path(
                    "/usr/share/icons/Adwaita/scalable/devices/computer.svg".to_string(),
                ),
                category: "Apps".to_string(),
                actions: vec![Action {}],
            },
            SearchItem {
                title: "Another Item".to_string(),
                subtitle: "This is another example subtitle".to_string(),
                icon: Icon::Path(
                    "/usr/share/icons/Adwaita/scalable/devices/computer.svg".to_string(),
                ),
                category: "Apps".to_string(),
                actions: vec![Action {}],
            },
            SearchItem {
                title: "Third Item".to_string(),
                subtitle: "This is a third example subtitle".to_string(),
                icon: Icon::Path(
                    "/usr/share/icons/Adwaita/scalable/devices/computer.svg".to_string(),
                ),
                category: "Apps".to_string(),
                actions: vec![Action {}],
            },
            SearchItem {
                title: "Fourth Item".to_string(),
                subtitle: "This is a fourth example subtitle".to_string(),
                icon: Icon::Path(
                    "/usr/share/icons/Adwaita/scalable/devices/computer.svg".to_string(),
                ),
                category: "Apps".to_string(),
                actions: vec![Action {}],
            },
            SearchItem {
                title: "Fifth Item".to_string(),
                subtitle: "This is a fifth example subtitle".to_string(),
                icon: Icon::Path(
                    "/usr/share/icons/Adwaita/scalable/devices/computer.svg".to_string(),
                ),
                category: "Apps".to_string(),
                actions: vec![Action {}],
            },
            SearchItem {
                title: "Sixth Item".to_string(),
                subtitle: "This is a sixth example subtitle".to_string(),
                icon: Icon::Path(
                    "/usr/share/icons/Adwaita/scalable/devices/computer.svg".to_string(),
                ),
                category: "Apps".to_string(),
                actions: vec![Action {}],
            },
            SearchItem {
                title: "Seventh Item".to_string(),
                subtitle: "This is a seventh example subtitle".to_string(),
                icon: Icon::Path(
                    "/usr/share/icons/Adwaita/scalable/devices/computer.svg".to_string(),
                ),
                category: "Apps".to_string(),
                actions: vec![Action {}],
            },
            SearchItem {
                title: "Eighth Item".to_string(),
                subtitle: "This is an eighth example subtitle".to_string(),
                icon: Icon::Path(
                    "/usr/share/icons/Adwaita/scalable/devices/computer.svg".to_string(),
                ),
                category: "Apps".to_string(),
                actions: vec![Action {}],
            },
            SearchItem {
                title: "Ninth Item".to_string(),
                subtitle: "This is a ninth example subtitle".to_string(),
                icon: Icon::Path(
                    "/usr/share/icons/Adwaita/scalable/devices/computer.svg".to_string(),
                ),
                category: "Apps".to_string(),
                actions: vec![Action {}],
            },
            SearchItem {
                title: "Tenth Item".to_string(),
                subtitle: "This is a tenth example subtitle".to_string(),
                icon: Icon::Path(
                    "/usr/share/icons/Adwaita/scalable/devices/computer.svg".to_string(),
                ),
                category: "Apps".to_string(),
                actions: vec![Action {}],
            },
        ]
        .into_iter()
        .filter(|item| item.title.to_lowercase().contains(&query.to_lowercase()))
        .collect()
    }
}
