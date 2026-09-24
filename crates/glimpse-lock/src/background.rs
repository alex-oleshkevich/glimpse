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
mod tests {
    use super::*;

    fn target(width: i32, height: i32) -> Target {
        Target { width, height }
    }

    fn both() -> Variants {
        Variants {
            light: true,
            dark: true,
        }
    }

    fn landed(cache: &mut Cache<&'static str>, value: &'static str) {
        for key in cache.due() {
            assert!(cache.land(key, Some(value)));
        }
    }

    #[test]
    fn dark_picks_image_dark_when_set_else_the_light_image() {
        let light = Path::new("light.jpg");
        let dark = Path::new("dark.jpg");
        assert_eq!(image(Some(light), Some(dark), true), Some(dark));
        assert_eq!(image(Some(light), Some(dark), false), Some(light));

        let only_light = Variants::of(Some(light), None);
        assert_eq!(only_light.pick(true), Some(false));
        assert_eq!(only_light.pick(false), Some(false));
        let only_dark = Variants::of(None, Some(dark));
        assert_eq!(only_dark.pick(true), Some(true));
        assert_eq!(
            only_dark.pick(false),
            None,
            "light with no image is the color"
        );
        assert_eq!(Variants::of(None, None).pick(true), None);
        assert_eq!(both().pick(true), Some(true));
        assert_eq!(both().pick(false), Some(false));

        assert_eq!(dim(0.35, 0.5, true), 0.5);
        assert_eq!(dim(0.35, 0.5, false), 0.35);
    }

    #[test]
    fn only_what_shapes_the_raster_decodes_again() {
        let old = Config::default();
        let changed = |edit: fn(&mut Config)| {
            let mut new = old.clone();
            edit(&mut new);
            redecodes(&old, &new)
        };
        assert!(!redecodes(&old, &old.clone()));
        assert!(changed(
            |new| new.lock.background.image = Some("a.jpg".into())
        ));
        assert!(changed(
            |new| new.lock.background.image_dark = Some("b.jpg".into())
        ));
        assert!(changed(
            |new| new.lock.background.fit = glimpse_config::Fit::Contain
        ));
        assert!(changed(|new| new.lock.background.blur_radius = 8));
        assert!(
            changed(|new| new.wallpaper.image = Some("c.jpg".into())),
            "an inherited wallpaper is still the lock's image"
        );
        assert!(
            !changed(|new| new.lock.background.dim = 0.9),
            "dim is a scrim, not a decode"
        );
        assert!(!changed(|new| new.lock.background.dim_dark = 0.9));
        assert!(
            !changed(|new| new.lock.background.color = "#ffffff".into()),
            "color is a solid texture, not a decode"
        );
    }

    #[test]
    fn the_lock_inherits_the_wallpaper_pair_until_it_names_an_image_of_its_own() {
        let mut config = Config::default();
        config.wallpaper.image = Some("desk.jpg".into());
        config.wallpaper.image_dark = Some("desk-dark.jpg".into());
        assert_eq!(
            images(&config),
            (
                Some(Path::new("desk.jpg")),
                Some(Path::new("desk-dark.jpg"))
            )
        );

        config.lock.background.image_dark = Some("lock-dark.jpg".into());
        assert_eq!(
            images(&config),
            (None, Some(Path::new("lock-dark.jpg"))),
            "naming either image takes the whole pair, or a lock image would sit beside a desktop one"
        );

        config.lock.background.image_dark = None;
        config.wallpaper.image = None;
        config.wallpaper.image_dark = None;
        assert_eq!(images(&config), (None, None));
    }

    #[test]
    fn each_output_decodes_every_configured_variant_once() {
        let mut cache = Cache::<&str>::default();
        cache.configure(both());
        cache.set_outputs(vec![
            ("eDP-1".into(), target(2880, 1800)),
            ("DP-2".into(), target(3840, 2160)),
        ]);
        let due = cache.due();
        assert_eq!(due.len(), 4);
        assert!(
            cache.due().is_empty(),
            "a pending decode is not asked for twice"
        );

        let mut only_light = Cache::<&str>::default();
        only_light.configure(Variants::of(Some(Path::new("a")), None));
        only_light.set_outputs(vec![("eDP-1".into(), target(10, 10))]);
        let due = only_light.due();
        assert_eq!(due.len(), 1, "no image-dark decodes nothing extra for dark");
        assert!(!due[0].dark);
    }

    #[test]
    fn a_landed_texture_is_served_for_its_output_and_scheme() {
        let mut cache = Cache::<&str>::default();
        cache.configure(both());
        cache.set_outputs(vec![("eDP-1".into(), target(10, 10))]);
        assert_eq!(
            cache.get("eDP-1", false),
            None,
            "nothing is served before it lands"
        );
        for key in cache.due() {
            let value = if key.dark { "dark" } else { "light" };
            assert!(cache.land(key, Some(value)));
        }
        assert_eq!(cache.get("eDP-1", false), Some(&"light"));
        assert_eq!(cache.get("eDP-1", true), Some(&"dark"));
        assert_eq!(cache.get("DP-2", true), None);
        assert!(
            cache.due().is_empty(),
            "a held texture is not decoded again"
        );
    }

    #[test]
    fn a_size_change_redecodes_and_keeps_the_old_texture_until_the_new_one_lands() {
        let mut cache = Cache::<&str>::default();
        cache.configure(Variants::of(Some(Path::new("a")), None));
        cache.set_outputs(vec![("eDP-1".into(), target(10, 10))]);
        landed(&mut cache, "small");

        cache.set_outputs(vec![("eDP-1".into(), target(20, 20))]);
        let due = cache.due();
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].target, target(20, 20));
        assert_eq!(cache.get("eDP-1", false), Some(&"small"));
        assert!(cache.land(due[0].clone(), Some("large")));
        assert_eq!(cache.get("eDP-1", false), Some(&"large"));
    }

    #[test]
    fn a_stale_decode_is_dropped() {
        let mut cache = Cache::<&str>::default();
        cache.configure(Variants::of(Some(Path::new("a")), None));
        cache.set_outputs(vec![("eDP-1".into(), target(10, 10))]);
        let old = cache.due().remove(0);

        cache.configure(Variants::of(Some(Path::new("b")), None));
        assert!(
            !cache.land(old.clone(), Some("old")),
            "a previous configuration's decode"
        );
        assert_eq!(cache.get("eDP-1", false), None);

        let current = cache.due().remove(0);
        cache.set_outputs(Vec::new());
        assert!(
            !cache.land(current, Some("gone")),
            "a decode for an output that left is dropped"
        );
        assert_eq!(cache.get("eDP-1", false), None);
    }

    #[test]
    fn a_failed_decode_or_a_removed_image_falls_back_to_the_color() {
        let mut cache = Cache::<&str>::default();
        cache.configure(Variants::of(Some(Path::new("a")), None));
        cache.set_outputs(vec![("eDP-1".into(), target(10, 10))]);
        landed(&mut cache, "a");

        cache.configure(Variants::of(Some(Path::new("b")), None));
        assert_eq!(
            cache.get("eDP-1", false),
            Some(&"a"),
            "the old image stays until b lands"
        );
        let key = cache.due().remove(0);
        assert!(cache.land(key, None));
        assert_eq!(
            cache.get("eDP-1", false),
            None,
            "b failed: the color, never a"
        );
        assert!(
            cache.due().is_empty(),
            "a failed decode is not retried for the same key"
        );

        cache.set_outputs(vec![("eDP-1".into(), target(20, 20))]);
        assert_eq!(cache.due().len(), 1, "a new size tries again");
        cache.configure(Variants::of(None, None));
        assert_eq!(cache.get("eDP-1", false), None);
        assert!(cache.due().is_empty());
    }

    #[test]
    fn an_output_that_leaves_drops_its_textures() {
        let mut cache = Cache::<&str>::default();
        cache.configure(Variants::of(Some(Path::new("a")), None));
        cache.set_outputs(vec![("eDP-1".into(), target(10, 10))]);
        landed(&mut cache, "a");
        cache.set_outputs(Vec::new());
        cache.set_outputs(vec![("eDP-1".into(), target(10, 10))]);
        assert_eq!(cache.get("eDP-1", false), None);
        assert_eq!(cache.due().len(), 1, "a returning output decodes again");
    }
}
