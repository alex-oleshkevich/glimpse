use std::collections::HashMap;
use std::path::Path;

use glimpse_config::Config;
use glimpse_widgets::raster::Target;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Variants {
    pub light: bool,
    pub dark: bool,
}

impl Variants {
    pub fn of(image: Option<&Path>, image_dark: Option<&Path>) -> Self {
        Self {
            light: image.is_some(),
            dark: image_dark.is_some(),
        }
    }

    pub fn pick(self, dark: bool) -> Option<bool> {
        match (dark && self.dark, self.light) {
            (true, _) => Some(true),
            (false, true) => Some(false),
            (false, false) => None,
        }
    }

    fn wanted(self) -> impl Iterator<Item = bool> {
        [(false, self.light), (true, self.dark)]
            .into_iter()
            .filter_map(|(variant, wanted)| wanted.then_some(variant))
    }
}

pub fn image<'a>(
    image: Option<&'a Path>,
    image_dark: Option<&'a Path>,
    dark: bool,
) -> Option<&'a Path> {
    match dark {
        true => image_dark,
        false => image,
    }
}

pub fn images(config: &Config) -> (Option<&Path>, Option<&Path>) {
    let (lock, wallpaper) = (&config.lock.background, &config.wallpaper);
    match lock.image.is_some() || lock.image_dark.is_some() {
        true => (lock.image.as_deref(), lock.image_dark.as_deref()),
        false => (wallpaper.image.as_deref(), wallpaper.image_dark.as_deref()),
    }
}

pub fn variants(config: &Config) -> Variants {
    let (image, image_dark) = images(config);
    Variants::of(image, image_dark)
}

pub fn redecodes(old: &Config, new: &Config) -> bool {
    let (old_background, new_background) = (&old.lock.background, &new.lock.background);
    images(old) != images(new)
        || old_background.fit != new_background.fit
        || old_background.blur_radius != new_background.blur_radius
}

pub fn dim(dim: f64, dim_dark: f64, dark: bool) -> f64 {
    match dark {
        true => dim_dark,
        false => dim,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Key {
    pub connector: String,
    pub dark: bool,
    pub target: Target,
    pub generation: u64,
}

pub struct Cache<T> {
    generation: u64,
    variants: Variants,
    outputs: Vec<(String, Target)>,
    slots: HashMap<(String, bool), (Key, T)>,
    pending: Vec<Key>,
    failed: Vec<Key>,
}

impl<T> Default for Cache<T> {
    fn default() -> Self {
        Self {
            generation: 0,
            variants: Variants::default(),
            outputs: Vec::new(),
            slots: HashMap::new(),
            pending: Vec::new(),
            failed: Vec::new(),
        }
    }
}

impl<T> Cache<T> {
    pub fn configure(&mut self, variants: Variants) {
        self.generation += 1;
        self.variants = variants;
        self.purge();
    }

    pub fn set_outputs(&mut self, outputs: Vec<(String, Target)>) {
        self.outputs = outputs;
        self.purge();
    }

    pub fn clear(&mut self) {
        self.generation += 1;
        self.slots.clear();
        self.pending.clear();
        self.failed.clear();
    }

    pub fn due(&mut self) -> Vec<Key> {
        let due = self
            .wanted()
            .filter(|key| {
                let held = self
                    .slots
                    .get(&(key.connector.clone(), key.dark))
                    .map(|(held, _)| held);
                held != Some(key) && !self.pending.contains(key) && !self.failed.contains(key)
            })
            .collect::<Vec<_>>();
        self.pending.extend(due.iter().cloned());
        due
    }

    pub fn is_wanted(&self, key: &Key) -> bool {
        self.wanted().any(|wanted| wanted == *key)
    }

    pub fn land(&mut self, key: Key, value: Option<T>) -> bool {
        self.pending.retain(|pending| pending != &key);
        if !self.wanted().any(|wanted| wanted == key) {
            return false;
        }
        let slot = (key.connector.clone(), key.dark);
        match value {
            Some(value) => {
                self.slots.insert(slot, (key, value));
            }
            None => {
                self.slots.remove(&slot);
                self.failed.push(key);
            }
        }
        true
    }

    pub fn get(&self, connector: &str, dark: bool) -> Option<&T> {
        let variant = self.variants.pick(dark)?;
        self.slots
            .get(&(connector.to_owned(), variant))
            .map(|(_, value)| value)
    }

    fn wanted(&self) -> impl Iterator<Item = Key> + '_ {
        self.outputs.iter().flat_map(move |(connector, target)| {
            self.variants.wanted().map(move |dark| Key {
                connector: connector.clone(),
                dark,
                target: *target,
                generation: self.generation,
            })
        })
    }

    fn purge(&mut self) {
        let wanted = self.wanted().collect::<Vec<_>>();
        self.failed.retain(|key| wanted.contains(key));
        self.slots.retain(|(connector, dark), _| {
            wanted
                .iter()
                .any(|key| &key.connector == connector && key.dark == *dark)
        });
    }
}

#[cfg(test)]
mod tests;
