use std::os::unix::fs::PermissionsExt;
use std::sync::{Arc, Mutex};

use super::*;

type Mutation = (&'static str, fn(&Path));

#[test]
fn fill_loads_the_output_exactly_and_never_crops() {
    let plan = plan(
        Target {
            width: 1920,
            height: 1080,
        },
        Target {
            width: 800,
            height: 600,
        },
        Fit::Fill,
    )
    .expect("a plan");
    assert_eq!(
        plan.load,
        Target {
            width: 800,
            height: 600
        }
    );
    assert_eq!(plan.crop, None);
}

#[test]
fn cover_scales_by_the_larger_ratio_and_crops_to_the_output() {
    let plan = plan(
        Target {
            width: 1920,
            height: 1080,
        },
        Target {
            width: 800,
            height: 800,
        },
        Fit::Cover,
    )
    .expect("a plan");
    assert!(plan.load.width >= 800 && plan.load.height >= 800);
    let crop = plan.crop.expect("cover always crops");
    assert_eq!((crop.width, crop.height), (800, 800));
}

#[test]
fn contain_scales_by_the_smaller_ratio_and_never_crops() {
    let plan = plan(
        Target {
            width: 1920,
            height: 1080,
        },
        Target {
            width: 800,
            height: 800,
        },
        Fit::Contain,
    )
    .expect("a plan");
    assert!(plan.load.width <= 800 && plan.load.height <= 800);
    assert_eq!(plan.crop, None);
}

#[test]
fn center_never_enlarges_a_source_smaller_than_the_output() {
    let plan = plan(
        Target {
            width: 64,
            height: 64,
        },
        Target {
            width: 800,
            height: 800,
        },
        Fit::Center,
    )
    .expect("a plan");
    assert_eq!(
        plan.load,
        Target {
            width: 64,
            height: 64
        }
    );
    assert_eq!(plan.crop, None);
}

#[test]
fn center_downscales_a_source_larger_than_the_output() {
    let plan = plan(
        Target {
            width: 1600,
            height: 1600,
        },
        Target {
            width: 800,
            height: 800,
        },
        Fit::Center,
    )
    .expect("a plan");
    assert_eq!(
        plan.load,
        Target {
            width: 800,
            height: 800
        }
    );
}

#[test]
fn a_zero_extent_source_yields_nothing() {
    assert_eq!(
        plan(
            Target {
                width: 0,
                height: 100
            },
            Target {
                width: 800,
                height: 600
            },
            Fit::Cover
        ),
        None
    );
    assert_eq!(
        plan(
            Target {
                width: 100,
                height: -1
            },
            Target {
                width: 800,
                height: 600
            },
            Fit::Cover
        ),
        None
    );
}

#[test]
fn a_zero_extent_output_yields_nothing() {
    assert_eq!(
        plan(
            Target {
                width: 1920,
                height: 1080
            },
            Target {
                width: 0,
                height: 600
            },
            Fit::Cover
        ),
        None
    );
}

#[test]
fn a_huge_reported_output_never_loads_past_the_ceiling() {
    let plan = plan(
        Target {
            width: 1920,
            height: 1080,
        },
        Target {
            width: 100_000,
            height: 100_000,
        },
        Fit::Cover,
    )
    .expect("a plan");
    assert!(plan.load.width <= CEILING);
    assert!(plan.load.height <= CEILING);
}

#[test]
fn the_cover_crop_never_reaches_outside_the_loaded_image() {
    for (source, output) in [
        (
            Target {
                width: 1920,
                height: 1080,
            },
            Target {
                width: 3440,
                height: 1440,
            },
        ),
        (
            Target {
                width: 1080,
                height: 1920,
            },
            Target {
                width: 1920,
                height: 1080,
            },
        ),
        (
            Target {
                width: 7,
                height: 4000,
            },
            Target {
                width: 640,
                height: 480,
            },
        ),
    ] {
        let plan = plan(source, output, Fit::Cover).expect("a plan");
        let crop = plan.crop.expect("cover always crops");
        assert!(crop.x >= 0 && crop.y >= 0);
        assert!(crop.x + crop.width <= plan.load.width);
        assert!(crop.y + crop.height <= plan.load.height);
    }
}

#[test]
fn backdrop_target_divides_a_large_output_and_preserves_aspect() {
    let output = Target {
        width: 3840,
        height: 2160,
    };
    assert_eq!(
        backdrop_target(output, 4),
        Target {
            width: 960,
            height: 540
        }
    );
    assert_eq!(backdrop_target(output, 1), output);
}

#[test]
fn backdrop_target_floors_a_small_output_at_256_and_keeps_its_aspect() {
    let target = backdrop_target(
        Target {
            width: 800,
            height: 600,
        },
        4,
    );
    assert!(target.width.min(target.height) == BACKDROP_FLOOR);
    let source_ratio = 800.0 / 600.0;
    let target_ratio = f64::from(target.width) / f64::from(target.height);
    assert!((source_ratio - target_ratio).abs() < 0.01);
}

#[test]
fn output_target_applies_a_fractional_scale_and_rounds() {
    assert_eq!(
        output_target(2560, 1440, 1.25),
        Target {
            width: 3200,
            height: 1800
        }
    );
}

#[test]
fn output_target_never_exceeds_the_ceiling() {
    let target = output_target(100_000, 100_000, 2.0);
    assert!(target.width <= CEILING);
    assert!(target.height <= CEILING);
}

static LOG_CAPTURE: Mutex<()> = Mutex::new(());

#[derive(Clone, Default)]
struct Logged(Arc<Mutex<Vec<u8>>>);

impl Logged {
    fn lines(&self) -> String {
        let bytes = self.0.lock().expect("not poisoned").clone();
        String::from_utf8(bytes).expect("tracing writes utf-8")
    }

    fn capture(
        &self,
    ) -> (
        std::sync::MutexGuard<'static, ()>,
        tracing::subscriber::DefaultGuard,
    ) {
        let serialize = LOG_CAPTURE
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let sink = self.clone();
        let dispatch = tracing::subscriber::set_default(
            tracing_subscriber::fmt()
                .with_writer(move || sink.clone())
                .with_max_level(tracing::Level::WARN)
                .without_time()
                .finish(),
        );
        (serialize, dispatch)
    }
}

impl std::io::Write for Logged {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().expect("not poisoned").extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[test]
fn color_falls_back_to_opaque_black_on_garbage() {
    let parsed = color("not a color");
    assert_eq!(parsed, gdk::RGBA::BLACK);
}

#[test]
fn color_logs_exactly_one_warning_on_garbage() {
    let logged = Logged::default();
    let _guards = logged.capture();

    color("not a color");

    let lines = logged.lines();
    assert_eq!(
        lines.matches("wallpaper color did not parse").count(),
        1,
        "{lines}"
    );
}

#[test]
fn a_cover_plan_past_the_ceiling_keeps_the_source_and_output_aspects() {
    let source = Target {
        width: 1080,
        height: 2400,
    };
    let output = Target {
        width: 3840,
        height: 2160,
    };
    let plan = plan(source, output, Fit::Cover).expect("a plan");
    let crop = plan.crop.expect("cover always crops");

    let aspect = |width: i32, height: i32| f64::from(width) / f64::from(height);
    assert!(
        (aspect(plan.load.width, plan.load.height) - aspect(source.width, source.height)).abs()
            < 0.01
    );
    assert!((aspect(crop.width, crop.height) - aspect(output.width, output.height)).abs() < 0.01);
}

#[test]
fn a_blurred_copy_keeps_the_aspect_and_is_never_enlarged() {
    assert_eq!(blurred_size(3840, 2160, 480), (480.0, 270.0));
    assert_eq!(blurred_size(300, 200, 480), (300.0, 200.0));
    assert_eq!(blurred_size(1000, 1, 10), (10.0, 1.0));
    assert_eq!(blurred_size(100, 100, 0), (1.0, 1.0));
}

#[test]
fn the_middle_of_an_axis_is_the_image_itself() {
    let axis = axis(0, 480.0, 30.0);
    assert_eq!((axis.start, axis.length), (0.0, 480.0));
    assert_eq!((axis.shift, axis.scale, axis.origin), (0.0, 1.0, 0.0));
}

fn world(axis: &Axis, local: f32) -> f32 {
    axis.shift + axis.scale * local
}

#[test]
fn the_border_cells_mirror_the_image_across_its_edge() {
    let (size, bleed) = (480.0_f32, 30.0_f32);

    let low = axis(-1, size, bleed);
    assert_eq!((low.start, low.length), (-bleed, bleed));
    assert_eq!(
        world(&low, low.origin),
        0.0,
        "the image's first column stays at the edge"
    );
    assert_eq!(
        world(&low, low.origin + size),
        -size,
        "and the rest folds outwards"
    );

    let high = axis(1, size, bleed);
    assert_eq!((high.start, high.length), (size, bleed));
    assert_eq!(
        world(&high, high.origin + size),
        size,
        "the last column stays at the edge"
    );
    assert_eq!(
        world(&high, high.origin),
        2.0 * size,
        "and the rest folds outwards"
    );
}

#[test]
fn every_tile_is_no_larger_than_the_image() {
    for side in [-1, 0, 1] {
        let axis = axis(side, 480.0, 30.0);
        assert!(axis.length <= 480.0);
        assert!(
            axis.scale.abs() == 1.0,
            "no tile is stretched, so none exceeds the image"
        );
    }
}

#[test]
fn a_bleed_wider_than_the_image_is_cut_to_it_so_no_gap_opens() {
    let low = axis(-1, 10.0, 30.0);
    assert_eq!((low.start, low.length), (-10.0, 10.0));
    let high = axis(1, 10.0, 30.0);
    assert_eq!((high.start, high.length), (10.0, 10.0));
    assert_eq!(
        world(&high, high.origin),
        20.0,
        "the mirrored tile still ends at the cell"
    );
}

static SCRATCH_COUNTER: AtomicU64 = AtomicU64::new(0);

struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "glimpse-raster-test-{}-{}",
            std::process::id(),
            SCRATCH_COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).expect("scratch dir");
        Self(path)
    }

    fn cache(&self) -> PathBuf {
        self.0.join("cache").join("raster")
    }

    fn image(&self, name: &str, width: i32, height: i32, alpha: bool) -> PathBuf {
        let channels = if alpha { 4 } else { 3 };
        let data: Vec<u8> = (0..width * height * channels)
            .map(|index| (index * 7 % 200) as u8)
            .collect();
        let pixbuf = gtk4::gdk_pixbuf::Pixbuf::from_bytes(
            &glib::Bytes::from_owned(data),
            gtk4::gdk_pixbuf::Colorspace::Rgb,
            alpha,
            8,
            width,
            height,
            width * channels,
        );
        let path = self.0.join(name);
        pixbuf.savev(&path, "png", &[]).expect("save png");
        path
    }

    fn entries(&self) -> Vec<PathBuf> {
        let Ok(listing) = fs::read_dir(self.cache()) else {
            return Vec::new();
        };
        let mut entries: Vec<PathBuf> = listing
            .flatten()
            .map(|item| item.path())
            .filter(|path| path.to_string_lossy().ends_with(ENTRY_SUFFIX))
            .collect();
        entries.sort();
        entries
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn target(width: i32, height: i32) -> Target {
    Target { width, height }
}

fn rows(raster: &Raster) -> Vec<u8> {
    let channels = if raster.has_alpha { 4 } else { 3 };
    let row = raster.width as usize * channels;
    (0..raster.height as usize)
        .flat_map(|index| raster.bytes[index * raster.rowstride..][..row].to_vec())
        .collect()
}

fn key_for(source: &Path, output: Target, fit: Fit) -> Key {
    let metadata = fs::metadata(source).expect("source metadata");
    Key::new(
        source,
        metadata.len(),
        metadata.modified().expect("source mtime"),
        output,
        fit,
    )
    .expect("key")
}

fn set_mtime(path: &Path, mtime: SystemTime) {
    File::options()
        .write(true)
        .open(path)
        .expect("open for mtime")
        .set_modified(mtime)
        .expect("set mtime");
}

fn mtime_of(path: &Path) -> SystemTime {
    fs::metadata(path)
        .expect("metadata")
        .modified()
        .expect("mtime")
}

const MARKER: u8 = 0xFE;

fn mark(entry: &Path) {
    let mut data = fs::read(entry).expect("read entry");
    let first_pixel = pixel_start(&data);
    data[first_pixel] = MARKER;
    fs::write(entry, data).expect("write entry");
}

fn pixel_start(entry: &[u8]) -> usize {
    let path_len = u32::from_le_bytes(entry[12..16].try_into().expect("path length")) as usize;
    16 + path_len + 8 + 8 + 4 + 4 + 4 + 1 + TAIL_LEN
}

fn first_byte(raster: &Raster) -> u8 {
    raster.bytes[0]
}

#[test]
fn a_stored_entry_round_trips_pixels_size_and_alpha() {
    for alpha in [false, true] {
        let scratch = Scratch::new();
        let source = scratch.image("a.png", 8, 6, alpha);
        let cache = scratch.cache();
        let decoded = raster_in(Some(&cache), &source, target(4, 3), Fit::Fill).expect("decode");
        assert_eq!(scratch.entries().len(), 1);
        let hit = raster_in(Some(&cache), &source, target(4, 3), Fit::Fill).expect("hit");
        assert_eq!(
            (hit.width, hit.height, hit.has_alpha),
            (4, 3, alpha),
            "alpha {alpha}"
        );
        assert_eq!(rows(&hit), rows(&decoded), "alpha {alpha}");
        assert_eq!(hit.mtime, mtime_of(&source));
    }
}

#[test]
fn a_hit_is_served_from_the_entry_not_decoded_again() {
    let scratch = Scratch::new();
    let source = scratch.image("a.png", 8, 6, true);
    let cache = scratch.cache();
    let decoded = raster_in(Some(&cache), &source, target(4, 3), Fit::Fill).expect("decode");
    assert_ne!(first_byte(&decoded), MARKER);
    mark(&scratch.entries()[0]);
    let hit = raster_in(Some(&cache), &source, target(4, 3), Fit::Fill).expect("hit");
    assert_eq!(first_byte(&hit), MARKER);
}

#[test]
fn a_changed_source_target_or_fit_is_a_miss() {
    let scratch = Scratch::new();
    let source = scratch.image("a.png", 8, 6, true);
    let cache = scratch.cache();
    raster_in(Some(&cache), &source, target(4, 3), Fit::Fill).expect("decode");
    mark(&scratch.entries()[0]);

    let other_target = raster_in(Some(&cache), &source, target(2, 3), Fit::Fill).expect("target");
    assert_eq!((other_target.width, other_target.height), (2, 3));
    assert_eq!(scratch.entries().len(), 2);

    let other_fit = raster_in(Some(&cache), &source, target(4, 3), Fit::Cover).expect("fit");
    assert_ne!(first_byte(&other_fit), MARKER);
    assert_eq!(scratch.entries().len(), 3);

    let original_mtime = mtime_of(&source);
    let later = original_mtime + Duration::from_secs(5);
    set_mtime(&source, later);
    let touched = raster_in(Some(&cache), &source, target(4, 3), Fit::Fill).expect("mtime");
    assert_ne!(first_byte(&touched), MARKER);
    assert_eq!(touched.mtime, later);
    assert_eq!(scratch.entries().len(), 4);

    set_mtime(&source, original_mtime);
    let hit = raster_in(Some(&cache), &source, target(4, 3), Fit::Fill).expect("restored");
    assert_eq!(first_byte(&hit), MARKER, "the original key still hits");

    let replaced = scratch.image("a.png", 10, 6, true);
    set_mtime(&replaced, original_mtime);
    let resized = raster_in(Some(&cache), &replaced, target(4, 3), Fit::Fill).expect("length");
    assert_ne!(first_byte(&resized), MARKER);
    assert_eq!(scratch.entries().len(), 5);
}

#[test]
fn a_damaged_entry_is_a_miss_is_deleted_and_the_call_still_decodes() {
    let mutations: [Mutation; 7] = [
        ("truncated", |entry| {
            let length = fs::metadata(entry).expect("metadata").len();
            File::options()
                .write(true)
                .open(entry)
                .expect("open")
                .set_len(length - 1)
                .expect("truncate");
        }),
        ("extended", |entry| {
            let mut file = File::options().append(true).open(entry).expect("open");
            file.write_all(&[0]).expect("append");
        }),
        ("empty", |entry| {
            fs::write(entry, []).expect("empty");
        }),
        ("bad magic", |entry| {
            let mut data = fs::read(entry).expect("read");
            data[0] ^= 0xFF;
            fs::write(entry, data).expect("write");
        }),
        ("other key", |entry| {
            let mut data = fs::read(entry).expect("read");
            data[17] ^= 0xFF;
            fs::write(entry, data).expect("write");
        }),
        ("short rowstride", |entry| {
            let mut data = fs::read(entry).expect("read");
            let at = pixel_start(&data) - 9;
            data[at..at + 8].copy_from_slice(&1u64.to_le_bytes());
            fs::write(entry, data).expect("write");
        }),
        ("huge rowstride", |entry| {
            let mut data = fs::read(entry).expect("read");
            let start = pixel_start(&data);
            data[start - 17..start - 13].copy_from_slice(&1i32.to_le_bytes());
            data[start - 13..start - 9].copy_from_slice(&1i32.to_le_bytes());
            let rowstride = CEILING as usize * 4 + 4096;
            data[start - 9..start - 1].copy_from_slice(&(rowstride as u64).to_le_bytes());
            data.resize(start + rowstride, 0);
            fs::write(entry, data).expect("write");
        }),
    ];
    for (name, mutate) in mutations {
        let scratch = Scratch::new();
        let source = scratch.image("a.png", 8, 6, true);
        let cache = scratch.cache();
        let decoded = raster_in(Some(&cache), &source, target(4, 3), Fit::Fill).expect("decode");
        let entry = scratch.entries().remove(0);
        mutate(&entry);

        let key = key_for(&source, target(4, 3), Fit::Fill);
        assert!(load(&cache, &key, mtime_of(&source)).is_none(), "{name}");
        assert!(!entry.exists(), "{name}: the damaged entry is deleted");

        assert!(scratch.entries().is_empty(), "{name}");
        let recovered =
            raster_in(Some(&cache), &source, target(4, 3), Fit::Fill).expect("recovered");
        assert_eq!(rows(&recovered), rows(&decoded), "{name}");
        assert_eq!(scratch.entries().len(), 1, "{name}: re-stored");
    }
}

#[test]
fn a_damaged_entry_met_by_raster_still_returns_the_decoded_image() {
    let scratch = Scratch::new();
    let source = scratch.image("a.png", 8, 6, false);
    let cache = scratch.cache();
    let decoded = raster_in(Some(&cache), &source, target(4, 3), Fit::Fill).expect("decode");
    let entry = scratch.entries().remove(0);
    let length = fs::metadata(&entry).expect("metadata").len();
    File::options()
        .write(true)
        .open(&entry)
        .expect("open")
        .set_len(length / 2)
        .expect("truncate");
    let recovered = raster_in(Some(&cache), &source, target(4, 3), Fit::Fill).expect("recovered");
    assert_eq!(rows(&recovered), rows(&decoded));
    assert_eq!(
        fs::metadata(&entry).expect("rewritten").len(),
        length,
        "the entry is stored again whole"
    );
}

#[test]
fn a_fifo_entry_is_removed_without_blocking_and_the_image_is_decoded() {
    let scratch = Scratch::new();
    let source = scratch.image("a.png", 8, 6, true);
    let cache = scratch.cache();
    let decoded = raster_in(Some(&cache), &source, target(4, 3), Fit::Fill).expect("decode");
    let entry = scratch.entries().remove(0);
    fs::remove_file(&entry).expect("remove entry");
    let path = std::ffi::CString::new(entry.as_os_str().as_bytes()).expect("path");
    assert_eq!(unsafe { libc::mkfifo(path.as_ptr(), 0o600) }, 0, "mkfifo");

    let (sender, receiver) = std::sync::mpsc::channel();
    let worker = {
        let source = source.clone();
        let cache = cache.clone();
        std::thread::spawn(move || {
            let _ = sender.send(raster_in(Some(&cache), &source, target(4, 3), Fit::Fill).ok());
        })
    };
    let recovered = receiver
        .recv_timeout(std::time::Duration::from_secs(10))
        .expect("raster_in must not block on a fifo")
        .expect("recovered");
    worker.join().expect("join");
    assert_eq!(rows(&recovered), rows(&decoded));
    assert!(
        fs::metadata(&entry).expect("stored again").is_file(),
        "the fifo is replaced by a regular entry"
    );
}

#[test]
fn the_cache_directory_is_private() {
    let scratch = Scratch::new();
    let source = scratch.image("a.png", 8, 6, true);
    raster_in(Some(&scratch.cache()), &source, target(4, 3), Fit::Fill).expect("decode");
    let mode = fs::metadata(scratch.cache())
        .expect("cache dir")
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o700);
}

#[test]
fn eviction_keeps_sixteen_entries_and_drops_the_oldest() {
    let scratch = Scratch::new();
    let source = scratch.image("a.png", 20, 3, true);
    let cache = scratch.cache();
    let paths: Vec<PathBuf> = (1..=18)
        .map(|width| cache.join(key_for(&source, target(width, 3), Fit::Fill).name))
        .collect();
    let epoch = SystemTime::now() - Duration::from_secs(10_000);
    for width in 1..=16 {
        raster_in(Some(&cache), &source, target(width, 3), Fit::Fill).expect("decode");
        set_mtime(
            &paths[width as usize - 1],
            epoch + Duration::from_secs(width as u64),
        );
    }
    assert_eq!(scratch.entries().len(), 16);

    raster_in(Some(&cache), &source, target(17, 3), Fit::Fill).expect("decode");
    assert_eq!(scratch.entries().len(), 16);
    assert!(!paths[0].exists(), "the oldest entry went");
    assert!(paths[1].exists());
    assert!(paths[16].exists(), "the new entry stays");

    raster_in(Some(&cache), &source, target(2, 3), Fit::Fill).expect("hit");
    assert!(
        mtime_of(&paths[1]) > epoch + Duration::from_secs(1_000),
        "a hit refreshes the entry"
    );
    raster_in(Some(&cache), &source, target(18, 3), Fit::Fill).expect("decode");
    assert_eq!(scratch.entries().len(), 16);
    assert!(paths[1].exists(), "a refreshed entry outlives older ones");
    assert!(!paths[2].exists(), "the next oldest went");
}

#[test]
fn eviction_removes_only_stale_temporary_files() {
    let scratch = Scratch::new();
    let source = scratch.image("a.png", 8, 6, true);
    let cache = scratch.cache();
    fs::create_dir_all(&cache).expect("cache dir");
    let stale = cache.join(".tmp-1-1-1");
    let fresh = cache.join(".tmp-1-1-2");
    fs::write(&stale, [0]).expect("stale");
    fs::write(&fresh, [0]).expect("fresh");
    set_mtime(&stale, SystemTime::now() - Duration::from_secs(120));
    raster_in(Some(&cache), &source, target(4, 3), Fit::Fill).expect("decode");
    assert!(!stale.exists());
    assert!(fresh.exists());
}

#[test]
fn warm_decodes_a_missing_entry_and_leaves_a_valid_one_alone() {
    let scratch = Scratch::new();
    let source = scratch.image("a.png", 8, 6, true);
    let cache = scratch.cache();
    warm_in(Some(&cache), &source, target(4, 3), Fit::Fill).expect("warm");
    assert_eq!(scratch.entries().len(), 1);
    let entry = scratch.entries().remove(0);

    mark(&entry);
    warm_in(Some(&cache), &source, target(4, 3), Fit::Fill).expect("warm again");
    assert_eq!(
        fs::read(&entry).expect("read")[pixel_start(&fs::read(&entry).expect("read"))],
        MARKER,
        "a valid entry is not rewritten"
    );

    File::options()
        .write(true)
        .open(&entry)
        .expect("open")
        .set_len(5)
        .expect("truncate");
    warm_in(Some(&cache), &source, target(4, 3), Fit::Fill).expect("repair");
    let key = key_for(&source, target(4, 3), Fit::Fill);
    assert!(load(&cache, &key, mtime_of(&source)).is_some());
}

#[test]
fn without_a_cache_directory_the_image_is_decoded_and_nothing_is_written() {
    let scratch = Scratch::new();
    let source = scratch.image("a.png", 8, 6, true);
    let decoded = raster_in(None, &source, target(4, 3), Fit::Fill).expect("decode");
    assert_eq!((decoded.width, decoded.height), (4, 3));
    warm_in(None, &source, target(4, 3), Fit::Fill).expect("warm");
    assert!(scratch.entries().is_empty());
}

#[test]
fn an_unwritable_cache_directory_never_fails_the_call() {
    let scratch = Scratch::new();
    let source = scratch.image("a.png", 8, 6, true);
    let blocker = scratch.0.join("blocker");
    fs::write(&blocker, [0]).expect("blocker");
    let decoded = raster_in(
        Some(&blocker.join("raster")),
        &source,
        target(4, 3),
        Fit::Fill,
    )
    .expect("decode");
    assert_eq!((decoded.width, decoded.height), (4, 3));
}

#[test]
fn concurrent_writers_of_one_key_leave_one_whole_entry() {
    let scratch = Scratch::new();
    let source = scratch.image("a.png", 8, 6, true);
    let cache = scratch.cache();
    let results: Vec<Vec<u8>> = std::thread::scope(|scope| {
        let workers: Vec<_> = (0..8)
            .map(|_| {
                scope.spawn(|| {
                    rows(
                        &raster_in(Some(&cache), &source, target(4, 3), Fit::Fill).expect("raster"),
                    )
                })
            })
            .collect();
        workers
            .into_iter()
            .map(|worker| worker.join().expect("worker"))
            .collect()
    });
    assert!(results.windows(2).all(|pair| pair[0] == pair[1]));
    assert_eq!(scratch.entries().len(), 1);
    let leftovers = fs::read_dir(&cache)
        .expect("cache dir")
        .flatten()
        .filter(|item| item.file_name().to_string_lossy().starts_with(TEMP_PREFIX))
        .count();
    assert_eq!(leftovers, 0);
}
