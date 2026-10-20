use std::cell::RefCell;
use std::collections::HashMap;

use anyhow::{Context as _, Result, anyhow};
use glimpse_config::PaletteColor;
use glimpse_shot::doc::document::{Document, Mark, MarkId, Source, Style};
use glimpse_shot::doc::redact::{pixelate_stroke, redact};
use glimpse_shot::doc::units::{BufferPx, Point, Rect};
use glimpse_wayland::capture::{Frame, Layout};
use gtk4::prelude::*;
use gtk4::{gdk, glib, graphene, gsk, pango};

fn ink_origin(layout: &pango::Layout, (x, y): (f32, f32)) -> (f32, f32) {
    let (ink, _) = layout.extents();
    let scale = pango::SCALE as f32;
    (
        x - (ink.x() as f32 + ink.width() as f32 / 2.0) / scale,
        y - (ink.y() as f32 + ink.height() as f32 / 2.0) / scale,
    )
}

pub struct Png {
    pub bytes: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

pub struct Rendered {
    texture: gdk::Texture,
    width: u32,
    height: u32,
}

impl Rendered {
    pub fn encode(self) -> Png {
        Png {
            bytes: self.texture.save_to_png_bytes().to_vec(),
            width: self.width,
            height: self.height,
        }
    }
}

type Tiles = HashMap<(i32, i32, i32, i32), Option<gdk::MemoryTexture>>;
type Strokes = HashMap<MarkId, (Mark, Option<(Rect<BufferPx>, gdk::MemoryTexture)>)>;

pub struct Cache {
    frame: gdk::Texture,
    label: gtk4::Label,
    tiles: RefCell<Tiles>,
    strokes: RefCell<Strokes>,
}

impl Cache {
    pub fn new(doc: &Document) -> Self {
        Self {
            frame: frame_texture(frame_of(doc)).upcast(),
            label: gtk4::Label::new(None),
            tiles: RefCell::default(),
            strokes: RefCell::default(),
        }
    }
}

pub type Moving = Option<(MarkId, Mark)>;

pub fn scene(snapshot: &gtk4::Snapshot, doc: &Document, cache: &Cache, moving: Moving) {
    image(snapshot, doc, cache);
    marks(snapshot, doc, cache, moving);
}

pub fn image(snapshot: &gtk4::Snapshot, doc: &Document, cache: &Cache) {
    let frame = frame_of(doc);
    snapshot.append_texture(
        &cache.frame,
        &graphene::Rect::new(0.0, 0.0, frame.width as f32, frame.height as f32),
    );
}

pub fn marks(snapshot: &gtk4::Snapshot, doc: &Document, cache: &Cache, moving: Moving) {
    let frame = frame_of(doc);
    let ink = Ink {
        snapshot,
        cache,
        scale: doc.scale() as f32,
    };
    let mut kept = Vec::new();
    let mut kept_strokes = Vec::new();
    for (id, mark) in &doc.marks {
        match mark {
            Mark::Blur { rect: at } => {
                let tile = match &moving {
                    Some((moved, Mark::Blur { rect })) if moved == id => *rect,
                    _ => *at,
                };
                ink.blur(frame, tile, *at);
                kept.push(key(tile));
            }
            Mark::Pixelate { .. } => {
                let source = match &moving {
                    Some((moved, original @ Mark::Pixelate { .. })) if moved == id => original,
                    _ => mark,
                };
                ink.pixelate(frame, *id, source, mark);
                kept_strokes.push(*id);
            }
            _ => ink.draw(doc, *id, mark),
        }
    }
    cache
        .tiles
        .borrow_mut()
        .retain(|tile, _| kept.contains(tile));
    cache
        .strokes
        .borrow_mut()
        .retain(|id, _| kept_strokes.contains(id));
}

fn key(bounds: Rect<BufferPx>) -> (i32, i32, i32, i32) {
    (bounds.x.0, bounds.y.0, bounds.width.0, bounds.height.0)
}

pub fn outline(doc: &Document, mark: &Mark, cache: &Cache) -> graphene::Rect {
    let scale = doc.scale() as f32;
    let stroke = |style: &Style| 2.0 * style.stroke as u8 as f32 * scale;
    let (bounds, pad) = match mark {
        Mark::Arrow { from, to, style } => {
            let (x0, y0) = point(*from);
            let (x1, y1) = point(*to);
            (
                graphene::Rect::new(x0.min(x1), y0.min(y1), (x1 - x0).abs(), (y1 - y0).abs()),
                stroke(style) * 2.5,
            )
        }
        Mark::Line { from, to, style } => {
            let (x0, y0) = point(*from);
            let (x1, y1) = point(*to);
            (
                graphene::Rect::new(x0.min(x1), y0.min(y1), (x1 - x0).abs(), (y1 - y0).abs()),
                stroke(style),
            )
        }
        Mark::Path {
            points,
            style,
            highlighter,
        } => {
            let Some(first) = points.first() else {
                return graphene::Rect::new(0.0, 0.0, 0.0, 0.0);
            };
            let (mut x0, mut y0) = point(*first);
            let (mut x1, mut y1) = (x0, y0);
            for at in points {
                let (x, y) = point(*at);
                x0 = x0.min(x);
                y0 = y0.min(y);
                x1 = x1.max(x);
                y1 = y1.max(y);
            }
            (
                graphene::Rect::new(x0, y0, x1 - x0, y1 - y0),
                stroke(style) * if *highlighter { 3.0 } else { 1.0 },
            )
        }
        Mark::Rect {
            rect: area,
            filled: true,
            ..
        } => (rect(*area), 0.0),
        Mark::Rect {
            rect: area, style, ..
        }
        | Mark::Oval { rect: area, style } => (rect(*area), stroke(style)),
        Mark::Blur { rect: area } => (rect(*area), 0.0),
        Mark::Pixelate { points, width } => {
            let Some(first) = points.first() else {
                return graphene::Rect::new(0.0, 0.0, 0.0, 0.0);
            };
            let (mut x0, mut y0, mut x1, mut y1) = (first.x.0, first.y.0, first.x.0, first.y.0);
            for at in points {
                x0 = x0.min(at.x.0);
                y0 = y0.min(at.y.0);
                x1 = x1.max(at.x.0);
                y1 = y1.max(at.y.0);
            }
            (
                graphene::Rect::new(x0 as f32, y0 as f32, (x1 - x0) as f32, (y1 - y0) as f32),
                *width as f32 / 2.0,
            )
        }
        Mark::Number { at, style } => {
            let radius = (8.0 + 4.0 * style.stroke as u8 as f32) * scale;
            let (x, y) = point(*at);
            (
                graphene::Rect::new(x - radius, y - radius, radius * 2.0, radius * 2.0),
                0.0,
            )
        }
        Mark::Text { at, text, style } => {
            let ink = Ink {
                snapshot: &gtk4::Snapshot::new(),
                cache,
                scale,
            };
            let (_, logical) = ink.layout(text, ink.text_size(*style)).pixel_extents();
            let (x, y) = point(*at);
            (
                graphene::Rect::new(
                    x + logical.x() as f32,
                    y + logical.y() as f32,
                    logical.width() as f32,
                    logical.height() as f32,
                ),
                stroke(style),
            )
        }
    };
    bounds.inset_r(-pad, -pad)
}

fn frame_of(doc: &Document) -> &Frame {
    match &doc.source {
        Source::Output { frame, .. }
        | Source::Window { frame, .. }
        | Source::File { frame, .. } => frame,
    }
}

pub fn export(doc: &Document, factor: f64) -> Result<Png> {
    render(doc, factor).map(Rendered::encode)
}

pub fn render(doc: &Document, factor: f64) -> Result<Rendered> {
    gtk4::init().context("no Wayland session to render in")?;
    let frame = frame_of(doc);
    let crop = within_frame(doc.exported_rect(), frame);
    let width = scaled(crop.width.0, factor);
    let height = scaled(crop.height.0, factor);

    let snapshot = gtk4::Snapshot::new();
    snapshot.scale(factor as f32, factor as f32);
    snapshot.translate(&graphene::Point::new(-crop.x.0 as f32, -crop.y.0 as f32));
    snapshot.push_clip(&rect(crop));
    scene(&snapshot, doc, &Cache::new(doc), None);
    snapshot.pop();

    let node = snapshot
        .to_node()
        .ok_or_else(|| anyhow!("the screenshot rendered to nothing"))?;
    let renderer = gsk::CairoRenderer::new();
    renderer
        .realize(None)
        .context("cannot start the offscreen renderer")?;
    let texture = renderer.render_texture(
        &node,
        Some(&graphene::Rect::new(0.0, 0.0, width as f32, height as f32)),
    );
    renderer.unrealize();
    Ok(Rendered {
        texture,
        width,
        height,
    })
}

fn scaled(length: i32, factor: f64) -> u32 {
    (f64::from(length) * factor).round().max(1.0) as u32
}

pub fn within_frame(crop: Rect<BufferPx>, frame: &Frame) -> Rect<BufferPx> {
    let x0 = crop.x.0.clamp(0, frame.width as i32);
    let y0 = crop.y.0.clamp(0, frame.height as i32);
    let x1 = crop
        .x
        .0
        .saturating_add(crop.width.0)
        .clamp(0, frame.width as i32);
    let y1 = crop
        .y
        .0
        .saturating_add(crop.height.0)
        .clamp(0, frame.height as i32);
    Rect {
        x: BufferPx(x0),
        y: BufferPx(y0),
        width: BufferPx((x1 - x0).max(1)),
        height: BufferPx((y1 - y0).max(1)),
    }
}

fn rect(rect: Rect<BufferPx>) -> graphene::Rect {
    graphene::Rect::new(
        rect.x.0 as f32,
        rect.y.0 as f32,
        rect.width.0 as f32,
        rect.height.0 as f32,
    )
}

fn point(point: Point<BufferPx>) -> (f32, f32) {
    (point.x.0 as f32, point.y.0 as f32)
}

fn frame_texture(frame: &Frame) -> gdk::MemoryTexture {
    gdk::MemoryTexture::new(
        frame.width as i32,
        frame.height as i32,
        match (frame.layout, frame.alpha) {
            (Layout::Rgbx, true) => gdk::MemoryFormat::R8g8b8a8,
            (Layout::Rgbx, false) => gdk::MemoryFormat::R8g8b8x8,
            (Layout::Bgrx, true) => gdk::MemoryFormat::B8g8r8a8,
            (Layout::Bgrx, false) => gdk::MemoryFormat::B8g8r8x8,
        },
        &frame.pixels,
        frame.width as usize * 4,
    )
}

fn rgba(color: PaletteColor) -> gdk::RGBA {
    let [red, green, blue]: [f32; 3] = match color {
        PaletteColor::Red => [224.0, 27.0, 36.0],
        PaletteColor::Yellow => [246.0, 211.0, 45.0],
        PaletteColor::Green => [46.0, 194.0, 126.0],
        PaletteColor::Blue => [53.0, 132.0, 228.0],
        PaletteColor::White => [255.0, 255.0, 255.0],
    };
    gdk::RGBA::new(red / 255.0, green / 255.0, blue / 255.0, 1.0)
}

fn contrast(color: PaletteColor) -> gdk::RGBA {
    match color {
        PaletteColor::Yellow | PaletteColor::White => gdk::RGBA::BLACK,
        PaletteColor::Red | PaletteColor::Green | PaletteColor::Blue => gdk::RGBA::WHITE,
    }
}

struct Ink<'a> {
    snapshot: &'a gtk4::Snapshot,
    cache: &'a Cache,
    scale: f32,
}

impl Ink<'_> {
    fn width(&self, style: Style) -> f32 {
        2.0 * style.stroke as u8 as f32 * self.scale
    }

    fn line(&self, width: f32) -> gsk::Stroke {
        let stroke = gsk::Stroke::new(width);
        stroke.set_line_cap(gsk::LineCap::Round);
        stroke.set_line_join(gsk::LineJoin::Round);
        stroke
    }

    fn draw(&self, doc: &Document, id: MarkId, mark: &Mark) {
        match mark {
            Mark::Arrow { from, to, style } => self.arrow(point(*from), point(*to), *style),
            Mark::Line { from, to, style } => {
                self.path(&[*from, *to], *style, false);
            }
            Mark::Path {
                points,
                style,
                highlighter,
            } => self.path(points, *style, *highlighter),
            Mark::Rect {
                rect: bounds,
                style,
                filled: true,
            } => self
                .snapshot
                .append_color(&rgba(style.color), &rect(*bounds)),
            Mark::Rect {
                rect: bounds,
                style,
                filled: false,
            } => {
                let path = gsk::PathBuilder::new();
                path.add_rect(&rect(*bounds));
                self.snapshot.append_stroke(
                    &path.to_path(),
                    &self.line(self.width(*style)),
                    &rgba(style.color),
                );
            }
            Mark::Oval {
                rect: bounds,
                style,
            } => {
                let radius =
                    graphene::Size::new(bounds.width.0 as f32 / 2.0, bounds.height.0 as f32 / 2.0);
                let path = gsk::PathBuilder::new();
                path.add_rounded_rect(&gsk::RoundedRect::new(
                    rect(*bounds),
                    radius,
                    radius,
                    radius,
                    radius,
                ));
                self.snapshot.append_stroke(
                    &path.to_path(),
                    &self.line(self.width(*style)),
                    &rgba(style.color),
                );
            }
            Mark::Number { at, style } => {
                let n = doc.number_of(id).unwrap_or(0);
                self.number(point(*at), n, *style);
            }
            Mark::Text { at, text, style } => self.text(point(*at), text, *style),
            Mark::Blur { .. } | Mark::Pixelate { .. } => {}
        }
    }

    fn arrow(&self, (x0, y0): (f32, f32), (x1, y1): (f32, f32), style: Style) {
        let width = self.width(style);
        let (dx, dy) = (x1 - x0, y1 - y0);
        let length = dx.hypot(dy);
        if length < 1.0 {
            return;
        }
        let (ux, uy) = (dx / length, dy / length);
        let head = (width * 4.0).min(length);
        let half = width * 2.5;
        let (bx, by) = (x1 - ux * head, y1 - uy * head);

        let shaft = gsk::PathBuilder::new();
        shaft.move_to(x0, y0);
        shaft.line_to(bx + ux * head * 0.5, by + uy * head * 0.5);
        self.snapshot
            .append_stroke(&shaft.to_path(), &self.line(width), &rgba(style.color));

        let tip = gsk::PathBuilder::new();
        tip.move_to(x1, y1);
        tip.line_to(bx - uy * half, by + ux * half);
        tip.line_to(bx + uy * half, by - ux * half);
        tip.close();
        self.snapshot
            .append_fill(&tip.to_path(), gsk::FillRule::Winding, &rgba(style.color));
    }

    fn path(&self, points: &[Point<BufferPx>], style: Style, highlighter: bool) {
        let Some((first, rest)) = points.split_first() else {
            return;
        };
        let path = gsk::PathBuilder::new();
        let (x, y) = point(*first);
        path.move_to(x, y);
        for at in rest {
            let (x, y) = point(*at);
            path.line_to(x, y);
        }
        if highlighter {
            self.snapshot.push_opacity(0.35);
        }
        self.snapshot.append_stroke(
            &path.to_path(),
            &self.line(self.width(style) * if highlighter { 6.0 } else { 1.0 }),
            &rgba(style.color),
        );
        if highlighter {
            self.snapshot.pop();
        }
    }

    fn layout(&self, text: &str, pixels: f32) -> pango::Layout {
        let layout = self.cache.label.create_pango_layout(Some(text));
        let mut font = pango::FontDescription::from_string("Sans Bold");
        font.set_absolute_size(f64::from(pixels) * f64::from(pango::SCALE));
        layout.set_font_description(Some(&font));
        layout
    }

    fn number(&self, (x, y): (f32, f32), n: u32, style: Style) {
        let radius = (8.0 + 4.0 * style.stroke as u8 as f32) * self.scale;
        let disc = gsk::PathBuilder::new();
        disc.add_circle(&graphene::Point::new(x, y), radius);
        self.snapshot
            .append_fill(&disc.to_path(), gsk::FillRule::Winding, &rgba(style.color));

        let layout = self.layout(&n.to_string(), radius * 1.2);
        let (left, top) = ink_origin(&layout, (x, y));
        self.snapshot.save();
        self.snapshot.translate(&graphene::Point::new(left, top));
        self.snapshot.append_layout(&layout, &contrast(style.color));
        self.snapshot.restore();
    }

    fn text_size(&self, style: Style) -> f32 {
        (12.0 + 4.0 * style.stroke as u8 as f32) * self.scale
    }

    fn text(&self, (x, y): (f32, f32), text: &str, style: Style) {
        let layout = self.layout(text, self.text_size(style));
        let glyphs = gsk::PathBuilder::new();
        glyphs.add_layout(&layout);
        let path = glyphs.to_path();
        self.snapshot.save();
        self.snapshot.translate(&graphene::Point::new(x, y));
        self.snapshot
            .append_stroke(&path, &self.line(self.width(style)), &contrast(style.color));
        self.snapshot
            .append_fill(&path, gsk::FillRule::Winding, &rgba(style.color));
        self.snapshot.restore();
    }

    fn blur(&self, frame: &Frame, bounds: Rect<BufferPx>, at: Rect<BufferPx>) {
        let texture = self
            .cache
            .tiles
            .borrow_mut()
            .entry(key(bounds))
            .or_insert_with(|| tile_texture(frame, bounds))
            .clone();
        let Some(texture) = texture else {
            return;
        };
        self.snapshot.append_scaled_texture(
            &texture,
            gsk::ScalingFilter::Nearest,
            &graphene::Rect::new(
                (at.x.0 + bounds.x.0.max(0) - bounds.x.0) as f32,
                (at.y.0 + bounds.y.0.max(0) - bounds.y.0) as f32,
                texture.width() as f32,
                texture.height() as f32,
            ),
        );
    }

    fn pixelate(&self, frame: &Frame, id: MarkId, source: &Mark, at: &Mark) {
        let (Mark::Pixelate { points, width }, Mark::Pixelate { points: moved, .. }) = (source, at)
        else {
            return;
        };
        let mut strokes = self.cache.strokes.borrow_mut();
        let entry = strokes.entry(id).or_insert_with(|| (source.clone(), None));
        if entry.0 != *source {
            *entry = (source.clone(), None);
        }
        if entry.1.is_none() {
            let (bounds, tile) = pixelate_stroke(frame, points, *width);
            if tile.width == 0 || tile.height == 0 {
                return;
            }
            let texture = gdk::MemoryTexture::new(
                tile.width as i32,
                tile.height as i32,
                gdk::MemoryFormat::R8g8b8a8,
                &glib::Bytes::from_owned(tile.rgba),
                tile.width as usize * 4,
            );
            entry.1 = Some((bounds, texture));
        }
        let Some((bounds, texture)) = &entry.1 else {
            return;
        };
        let (Some(from), Some(to)) = (points.first(), moved.first()) else {
            return;
        };
        self.snapshot.append_scaled_texture(
            texture,
            gsk::ScalingFilter::Nearest,
            &graphene::Rect::new(
                bounds.x.0.saturating_add(to.x.0.saturating_sub(from.x.0)) as f32,
                bounds.y.0.saturating_add(to.y.0.saturating_sub(from.y.0)) as f32,
                texture.width() as f32,
                texture.height() as f32,
            ),
        );
    }
}

fn tile_texture(frame: &Frame, bounds: Rect<BufferPx>) -> Option<gdk::MemoryTexture> {
    let tile = redact(frame, bounds);
    if tile.width == 0 || tile.height == 0 {
        return None;
    }
    Some(gdk::MemoryTexture::new(
        tile.width as i32,
        tile.height as i32,
        gdk::MemoryFormat::R8g8b8a8,
        &glib::Bytes::from_owned(tile.rgba),
        tile.width as usize * 4,
    ))
}

#[cfg(test)]
mod tests;
