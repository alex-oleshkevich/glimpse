use std::cell::Cell;
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use glimpse_config::Fit;
use gtk4::gdk_pixbuf::PixbufLoader;
use gtk4::prelude::*;
use gtk4::{gdk, glib, graphene, gsk};

const CEILING: i32 = 8192;
const BACKDROP_FLOOR: i32 = 256;
const CACHE_MAGIC: &[u8; 8] = b"GLMPRAST";
const CACHE_VERSION: u32 = 1;
const CACHE_ENTRIES: usize = 16;
const CACHE_BYTES: u64 = 1 << 30;
const ENTRY_SUFFIX: &str = ".raster";
const TEMP_PREFIX: &str = ".tmp-";
const TEMP_MAX_AGE: Duration = Duration::from_secs(60);
const TAIL_LEN: usize = 17;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Target {
    pub width: i32,
    pub height: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Region {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Plan {
    pub load: Target,
    pub crop: Option<Region>,
}

#[derive(Debug, Clone)]
pub struct Raster {
    pub bytes: glib::Bytes,
    pub width: i32,
    pub height: i32,
    pub rowstride: usize,
    pub has_alpha: bool,
    pub mtime: SystemTime,
}

pub fn output_target(width: i32, height: i32, scale: f64) -> Target {
    clamp(Target {
        width: (f64::from(width) * scale).round() as i32,
        height: (f64::from(height) * scale).round() as i32,
    })
}

pub fn plan(source: Target, output: Target, fit: Fit) -> Option<Plan> {
    if source.width <= 0 || source.height <= 0 || output.width <= 0 || output.height <= 0 {
        return None;
    }

    let horizontal = f64::from(output.width) / f64::from(source.width);
    let vertical = f64::from(output.height) / f64::from(source.height);

    let wanted = match fit {
        Fit::Fill => output,
        Fit::Cover => scaled(source, horizontal.max(vertical)),
        Fit::Contain => scaled(source, horizontal.min(vertical)),
        Fit::Center => scaled(source, horizontal.min(vertical).min(1.0)),
    };
    let shrink = match fit {
        Fit::Cover => (f64::from(CEILING) / f64::from(wanted.width.max(wanted.height))).min(1.0),
        Fit::Fill | Fit::Contain | Fit::Center => 1.0,
    };
    let load = clamp(scaled(wanted, shrink));
    let view = scaled(output, shrink);

    let crop = match fit {
        Fit::Cover => Some(Region {
            x: ((load.width - view.width) / 2).max(0),
            y: ((load.height - view.height) / 2).max(0),
            width: load.width.min(view.width),
            height: load.height.min(view.height),
        }),
        Fit::Fill | Fit::Contain | Fit::Center => None,
    };

    Some(Plan { load, crop })
}

pub fn backdrop_target(output: Target, downscale_factor: u32) -> Target {
    let divisor = i32::try_from(downscale_factor).unwrap_or(i32::MAX).max(1);
    let divided = Target {
        width: (output.width / divisor).max(1),
        height: (output.height / divisor).max(1),
    };
    let shorter = divided.width.min(divided.height);
    if shorter >= BACKDROP_FLOOR {
        return clamp(divided);
    }
    clamp(scaled(
        divided,
        f64::from(BACKDROP_FLOOR) / f64::from(shorter),
    ))
}

fn scaled(source: Target, ratio: f64) -> Target {
    Target {
        width: ((f64::from(source.width) * ratio).round() as i32).max(1),
        height: ((f64::from(source.height) * ratio).round() as i32).max(1),
    }
}

fn clamp(target: Target) -> Target {
    Target {
        width: target.width.clamp(1, CEILING),
        height: target.height.clamp(1, CEILING),
    }
}

pub fn raster(path: &Path, output: Target, fit: Fit) -> Result<Raster> {
    raster_in(cache_root().as_deref(), path, output, fit)
}

pub fn warm(path: &Path, output: Target, fit: Fit) -> Result<()> {
    warm_in(cache_root().as_deref(), path, output, fit)
}

fn cache_root() -> Option<PathBuf> {
    glimpse_config::cache_dir().map(|dir| dir.join("raster"))
}

struct Source {
    file: File,
    mtime: SystemTime,
    key: Option<Key>,
}

fn open_source(dir: Option<&Path>, path: &Path, output: Target, fit: Fit) -> Result<Source> {
    let file = File::open(path)?;
    let metadata = file.metadata()?;
    let mtime = metadata.modified()?;
    let key = dir.and_then(|_| Key::new(path, metadata.len(), mtime, output, fit));
    Ok(Source { file, mtime, key })
}

fn raster_in(dir: Option<&Path>, path: &Path, output: Target, fit: Fit) -> Result<Raster> {
    let mut source = open_source(dir, path, output, fit)?;
    let cache = dir.zip(source.key.as_ref());
    if let Some((dir, key)) = cache
        && let Some(raster) = load(dir, key, source.mtime)
    {
        return Ok(raster);
    }
    let raster = decode(&mut source.file, source.mtime, output, fit)?;
    if let Some((dir, key)) = cache
        && let Err(error) = store(dir, key, &raster)
    {
        tracing::debug!(%error, path = %path.display(), "raster not cached");
    }
    Ok(raster)
}

fn warm_in(dir: Option<&Path>, path: &Path, output: Target, fit: Fit) -> Result<()> {
    let mut source = open_source(dir, path, output, fit)?;
    let cache = dir.zip(source.key.as_ref());
    if let Some((dir, key)) = cache
        && let Some((file, _, _)) = open_entry(dir, key)
    {
        touch(&file);
        return Ok(());
    }
    let raster = decode(&mut source.file, source.mtime, output, fit)?;
    if let Some((dir, key)) = cache
        && let Err(error) = store(dir, key, &raster)
    {
        tracing::debug!(%error, path = %path.display(), "raster not cached");
    }
    Ok(())
}

fn decode(file: &mut File, mtime: SystemTime, output: Target, fit: Fit) -> Result<Raster> {
    let loader = PixbufLoader::new();
    let planned = Rc::new(Cell::new(None));
    loader.connect_size_prepared({
        let planned = planned.clone();
        move |loader, width, height| {
            let plan = plan(Target { width, height }, output, fit);
            if let Some(plan) = plan {
                loader.set_size(plan.load.width, plan.load.height);
            }
            planned.set(plan);
        }
    });
    let fed = feed(&loader, file);
    let closed = loader.close();
    fed?;
    closed?;
    let plan = planned.get().context("image has no size")?;
    let loaded = loader.pixbuf().context("no image loader could decode it")?;
    let cropped = match plan.crop {
        Some(region) => loaded.new_subpixbuf(region.x, region.y, region.width, region.height),
        None => loaded,
    };
    Ok(Raster {
        bytes: cropped.read_pixel_bytes(),
        width: cropped.width(),
        height: cropped.height(),
        rowstride: cropped.rowstride().max(0) as usize,
        has_alpha: cropped.has_alpha(),
        mtime,
    })
}

struct Key {
    prefix: Vec<u8>,
    name: String,
}

impl Key {
    fn new(path: &Path, length: u64, mtime: SystemTime, output: Target, fit: Fit) -> Option<Self> {
        let canonical = path.canonicalize().ok()?;
        let path_bytes = canonical.as_os_str().as_bytes();
        let since_epoch = mtime.duration_since(UNIX_EPOCH).ok()?;
        let mut prefix = Vec::with_capacity(path_bytes.len() + 64);
        prefix.extend_from_slice(CACHE_MAGIC);
        prefix.extend_from_slice(&CACHE_VERSION.to_le_bytes());
        prefix.extend_from_slice(&u32::try_from(path_bytes.len()).ok()?.to_le_bytes());
        prefix.extend_from_slice(path_bytes);
        prefix.extend_from_slice(&length.to_le_bytes());
        prefix.extend_from_slice(&since_epoch.as_secs().to_le_bytes());
        prefix.extend_from_slice(&since_epoch.subsec_nanos().to_le_bytes());
        prefix.extend_from_slice(&output.width.to_le_bytes());
        prefix.extend_from_slice(&output.height.to_le_bytes());
        prefix.push(match fit {
            Fit::Cover => 0,
            Fit::Contain => 1,
            Fit::Fill => 2,
            Fit::Center => 3,
        });
        let name = format!("{:016x}{ENTRY_SUFFIX}", fnv1a(&prefix));
        Some(Self { prefix, name })
    }
}

fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0100_0000_01b3)
    })
}

struct Layout {
    width: i32,
    height: i32,
    rowstride: usize,
    has_alpha: bool,
}

impl Layout {
    fn of(raster: &Raster) -> Self {
        Self {
            width: raster.width,
            height: raster.height,
            rowstride: raster.rowstride,
            has_alpha: raster.has_alpha,
        }
    }

    fn parse(tail: &[u8; TAIL_LEN]) -> Option<Self> {
        Some(Self {
            width: i32::from_le_bytes(tail[0..4].try_into().ok()?),
            height: i32::from_le_bytes(tail[4..8].try_into().ok()?),
            rowstride: usize::try_from(u64::from_le_bytes(tail[8..16].try_into().ok()?)).ok()?,
            has_alpha: match tail[16] {
                0 => false,
                1 => true,
                _ => return None,
            },
        })
    }

    fn tail(&self) -> [u8; TAIL_LEN] {
        let mut tail = [0; TAIL_LEN];
        tail[0..4].copy_from_slice(&self.width.to_le_bytes());
        tail[4..8].copy_from_slice(&self.height.to_le_bytes());
        tail[8..16].copy_from_slice(&(self.rowstride as u64).to_le_bytes());
        tail[16] = u8::from(self.has_alpha);
        tail
    }

    fn pixels(&self) -> Option<usize> {
        if !(1..=CEILING).contains(&self.width) || !(1..=CEILING).contains(&self.height) {
            return None;
        }
        let channels = if self.has_alpha { 4 } else { 3 };
        if self.rowstride < self.width as usize * channels || self.rowstride > CEILING as usize * 4
        {
            return None;
        }
        self.rowstride.checked_mul(self.height as usize)
    }
}

fn read_layout(file: &mut File, key: &Key) -> Option<(Layout, usize)> {
    let total = file.metadata().ok()?.len();
    let mut prefix = vec![0; key.prefix.len()];
    file.read_exact(&mut prefix).ok()?;
    if prefix != key.prefix {
        return None;
    }
    let mut tail = [0; TAIL_LEN];
    file.read_exact(&mut tail).ok()?;
    let layout = Layout::parse(&tail)?;
    let pixels = layout.pixels()?;
    let header = (key.prefix.len() + TAIL_LEN) as u64;
    (total == header + pixels as u64).then_some((layout, pixels))
}

fn open_entry(dir: &Path, key: &Key) -> Option<(File, Layout, usize)> {
    let path = dir.join(&key.name);
    let opened = fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(&path);
    let mut file = match opened {
        Ok(file) => file,
        Err(error) => {
            if error.raw_os_error() == Some(libc::ELOOP) {
                let _ = fs::remove_file(&path);
            } else if error.kind() != io::ErrorKind::NotFound {
                tracing::debug!(%error, path = %path.display(), "raster cache entry unreadable");
            }
            return None;
        }
    };
    let layout = match file.metadata() {
        Ok(metadata) if metadata.is_file() => read_layout(&mut file, key),
        _ => None,
    };
    match layout {
        Some((layout, pixels)) => Some((file, layout, pixels)),
        None => {
            let _ = fs::remove_file(&path);
            None
        }
    }
}

fn touch(file: &File) {
    let _ = file.set_modified(SystemTime::now());
}

fn load(dir: &Path, key: &Key, mtime: SystemTime) -> Option<Raster> {
    let (mut file, layout, pixels) = open_entry(dir, key)?;
    let mut data = vec![0; pixels];
    if file.read_exact(&mut data).is_err() {
        let _ = fs::remove_file(dir.join(&key.name));
        return None;
    }
    touch(&file);
    Some(Raster {
        bytes: glib::Bytes::from_owned(data),
        width: layout.width,
        height: layout.height,
        rowstride: layout.rowstride,
        has_alpha: layout.has_alpha,
        mtime,
    })
}

fn store(dir: &Path, key: &Key, raster: &Raster) -> Result<()> {
    let layout = Layout::of(raster);
    let pixels = layout
        .pixels()
        .context("raster is outside the cacheable bounds")?;
    anyhow::ensure!(
        raster.bytes.len() <= pixels,
        "raster is larger than its layout"
    );
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(dir)?;
    let thread = format!("{:?}", std::thread::current().id())
        .chars()
        .filter(char::is_ascii_digit)
        .collect::<String>();
    let temp = dir.join(format!(
        "{TEMP_PREFIX}{}-{thread}-{}",
        std::process::id(),
        TEMP_COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    let written = write_entry(&temp, key, &layout, &raster.bytes, pixels)
        .and_then(|()| fs::rename(&temp, dir.join(&key.name)));
    if let Err(error) = written {
        let _ = fs::remove_file(&temp);
        return Err(error.into());
    }
    evict(dir, &key.name, CACHE_BYTES);
    Ok(())
}

fn write_entry(
    temp: &Path,
    key: &Key,
    layout: &Layout,
    bytes: &[u8],
    pixels: usize,
) -> io::Result<()> {
    let file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(temp)?;
    let mut out = io::BufWriter::with_capacity(64 * 1024, file);
    out.write_all(&key.prefix)?;
    out.write_all(&layout.tail())?;
    out.write_all(bytes)?;
    io::copy(
        &mut io::repeat(0).take((pixels - bytes.len()) as u64),
        &mut out,
    )?;
    out.flush()
}

fn evict(dir: &Path, keep: &str, budget: u64) {
    let Ok(listing) = fs::read_dir(dir) else {
        return;
    };
    let now = SystemTime::now();
    let mut used = fs::metadata(dir.join(keep)).map_or(0, |metadata| metadata.len());
    let mut entries = Vec::new();
    for item in listing.flatten() {
        let name = item.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        let Ok(metadata) = item.metadata() else {
            continue;
        };
        let Ok(modified) = metadata.modified() else {
            continue;
        };
        if name.starts_with(TEMP_PREFIX) {
            if now
                .duration_since(modified)
                .is_ok_and(|age| age > TEMP_MAX_AGE)
            {
                let _ = fs::remove_file(item.path());
            }
        } else if name.ends_with(ENTRY_SUFFIX) && name != keep {
            entries.push((modified, metadata.len(), item.path()));
        }
    }
    entries.sort_by_key(|entry| std::cmp::Reverse(entry.0));
    let mut kept = 1;
    let mut full = false;
    for (_, length, path) in entries {
        full = full || kept >= CACHE_ENTRIES || used.saturating_add(length) > budget;
        if full {
            let _ = fs::remove_file(path);
        } else {
            kept += 1;
            used += length;
        }
    }
}

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

fn feed(loader: &PixbufLoader, file: &mut File) -> Result<()> {
    let mut chunk = vec![0; 64 * 1024];
    loop {
        let read = file.read(&mut chunk)?;
        if read == 0 {
            return Ok(());
        }
        loader.write(&chunk[..read])?;
    }
}

pub fn blur_sigma(radius: u32, texture_width: i32, output_width: i32) -> Option<f32> {
    (radius > 0 && output_width > 0)
        .then(|| radius as f32 * texture_width as f32 / output_width as f32)
}

pub fn blurred(
    renderer: &gsk::Renderer,
    texture: &gdk::Texture,
    sigma: f32,
) -> Option<gdk::Texture> {
    blurred_to(renderer, texture, sigma, texture.width())
}

pub fn blurred_to(
    renderer: &gsk::Renderer,
    texture: &gdk::Texture,
    sigma: f32,
    width: i32,
) -> Option<gdk::Texture> {
    let (source_width, source_height) = (texture.width().max(1), texture.height().max(1));
    let (width, height) = blurred_size(source_width, source_height, width);
    let bounds = graphene::Rect::new(0.0, 0.0, width, height);
    let bleed = sigma * 3.0;
    let snapshot = gtk4::Snapshot::new();
    snapshot.push_blur(f64::from(sigma));
    for row in [-1, 0, 1] {
        for column in [-1, 0, 1] {
            let x = axis(column, width, bleed);
            let y = axis(row, height, bleed);
            snapshot.push_clip(&graphene::Rect::new(x.start, y.start, x.length, y.length));
            snapshot.save();
            snapshot.translate(&graphene::Point::new(x.shift, y.shift));
            snapshot.scale(x.scale, y.scale);
            snapshot.append_scaled_texture(
                texture,
                gsk::ScalingFilter::Trilinear,
                &graphene::Rect::new(x.origin, y.origin, width, height),
            );
            snapshot.restore();
            snapshot.pop();
        }
    }
    snapshot.pop();
    Some(renderer.render_texture(snapshot.to_node()?, Some(&bounds)))
}

fn blurred_size(source_width: i32, source_height: i32, width: i32) -> (f32, f32) {
    let width = width.clamp(1, source_width.max(1)) as f32;
    let height = (width * source_height as f32 / source_width.max(1) as f32)
        .round()
        .max(1.0);
    (width, height)
}

#[derive(Debug, PartialEq)]
struct Axis {
    start: f32,
    length: f32,
    shift: f32,
    scale: f32,
    origin: f32,
}

fn axis(side: i32, size: f32, bleed: f32) -> Axis {
    let bleed = bleed.min(size);
    match side {
        0 => Axis {
            start: 0.0,
            length: size,
            shift: 0.0,
            scale: 1.0,
            origin: 0.0,
        },
        -1 => Axis {
            start: -bleed,
            length: bleed,
            shift: 0.0,
            scale: -1.0,
            origin: 0.0,
        },
        _ => Axis {
            start: size,
            length: bleed,
            shift: size,
            scale: -1.0,
            origin: -size,
        },
    }
}

pub fn texture(raster: &Raster) -> gdk::Texture {
    gdk::MemoryTexture::new(
        raster.width,
        raster.height,
        match raster.has_alpha {
            true => gdk::MemoryFormat::R8g8b8a8,
            false => gdk::MemoryFormat::R8g8b8,
        },
        &raster.bytes,
        raster.rowstride,
    )
    .upcast()
}

pub fn solid(color: gdk::RGBA) -> gdk::Texture {
    let pixel: [u8; 4] = [
        (color.red() * 255.0).round() as u8,
        (color.green() * 255.0).round() as u8,
        (color.blue() * 255.0).round() as u8,
        (color.alpha() * 255.0).round() as u8,
    ];
    gdk::MemoryTexture::new(
        1,
        1,
        gdk::MemoryFormat::R8g8b8a8,
        &glib::Bytes::from(&pixel[..]),
        4,
    )
    .upcast()
}

pub fn content_fit(fit: Fit) -> gtk4::ContentFit {
    match fit {
        Fit::Fill | Fit::Cover => gtk4::ContentFit::Fill,
        Fit::Contain => gtk4::ContentFit::Contain,
        Fit::Center => gtk4::ContentFit::ScaleDown,
    }
}

pub fn color(text: &str) -> gdk::RGBA {
    match gdk::RGBA::parse(text) {
        Ok(color) => color,
        Err(_) => {
            tracing::warn!(
                color = text,
                "wallpaper color did not parse; using opaque black"
            );
            gdk::RGBA::BLACK
        }
    }
}

#[cfg(test)]
mod tests;
