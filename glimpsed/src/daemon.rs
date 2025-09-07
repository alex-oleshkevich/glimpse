struct ActivePlugin {
    name: String,
}

pub struct Daemon {
    plugins: Vec<ActivePlugin>,
}

impl Daemon {
    pub fn new() -> Self {
        Daemon {
            plugins: Vec::new(),
        }
    }

    pub async fn run(&self) {
        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
        }
    }

    pub fn discover_plugins(self) -> Result<Self, anyhow::Error> {
        Ok(self)
    }

    pub fn start_plugins(self) -> Result<Self, anyhow::Error> {
        Ok(self)
    }
}
