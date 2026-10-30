use std::cell::RefCell;
use std::time::{Duration, Instant};

use glimpse_config::{PaletteColor, Stroke};
use glimpse_shot::doc::document::{Document, Mark, MarkId, Op, Source, Style, Tool};
use glimpse_shot::doc::esc::{EscOutcome, Interaction, esc};
use glimpse_shot::doc::history::History;
use glimpse_shot::doc::units::{BufferPx, Point, Rect};
use glimpse_shot::doc::wire::{MAX_PATH_POINTS, MAX_TEXT_CHARS};
use gtk4::subclass::prelude::*;
use gtk4::{gdk, glib, graphene, gsk, prelude::*};

use crate::render::{self, Cache};

const CLICK_SLOP: f64 = 3.0;
const HANDLE: f32 = 10.0;
const TOOL_HOLD: Duration = Duration::from_millis(200);
const WINDOW_MARGIN: f64 = 48.0;
const TYPING: MarkId = MarkId::MAX;
const SCRIM: gdk::RGBA = gdk::RGBA::new(0.0, 0.0, 0.0, 0.6);

pub type Selection = (f64, f64, f64, f64);

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct View {
    pub x: f64,
    pub y: f64,
    pub factor: f64,
}

impl View {
    pub fn fit(frame: (f64, f64), area: (f64, f64), largest: f64, margin: f64) -> Self {
        let factor = ((area.0 - 2.0 * margin) / frame.0)
            .min((area.1 - 2.0 * margin) / frame.1)
            .min(largest)
            .max(f64::MIN_POSITIVE);
        Self {
            x: (area.0 - frame.0 * factor) / 2.0,
            y: (area.1 - frame.1 * factor) / 2.0,
            factor,
        }
    }

    pub fn to_buffer(self, x: f64, y: f64) -> Point<BufferPx> {
        Point {
            x: BufferPx(((x - self.x) / self.factor).round() as i32),
            y: BufferPx(((y - self.y) / self.factor).round() as i32),
        }
    }

    pub fn to_logical(self, point: Point<BufferPx>) -> (f64, f64) {
        (
            self.x + f64::from(point.x.0) * self.factor,
            self.y + f64::from(point.y.0) * self.factor,
        )
    }
}

#[derive(Debug, Clone, PartialEq)]
enum Gesture {
    Pending {
        x: f64,
        y: f64,
    },
    Drawing {
        from: Point<BufferPx>,
    },
    Path {
        points: Vec<Point<BufferPx>>,
    },
    Moving {
        id: MarkId,
        from: Point<BufferPx>,
    },
    Cropping {
        anchor: Point<BufferPx>,
    },
    MovingCrop {
        crop: Rect<BufferPx>,
        from: Point<BufferPx>,
    },
    Resizing {
        crop: Rect<BufferPx>,
        side: Side,
    },
    ResizeMark {
        id: MarkId,
        original: Mark,
        bounds: Rect<BufferPx>,
        handle: ResizeHandle,
        from: Point<BufferPx>,
        grip: Point<BufferPx>,
    },
    Region {
        x: f64,
        y: f64,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Side {
    Left,
    Top,
    Right,
    Bottom,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum ResizeHandle {
    Corner(Point<BufferPx>),
    Side(Side),
}

struct BorrowedTool {
    previous: Tool,
    active: Tool,
    since: Instant,
    used: bool,
    released: bool,
}

pub struct Editor {
    pub doc: Document,
    pub history: History,
    pub tool: Tool,
    temporary_tool: Option<BorrowedTool>,
    start: Tool,
    pub style: Style,
    pub typing: Option<Point<BufferPx>>,
    pub list_open: bool,
    pub picking_region: bool,
    pub flash: f64,
    pub fitted: bool,
    pub pending_crop: Option<Rect<BufferPx>>,
    cache: Cache,
    gesture: Option<Gesture>,
    pointer: (f64, f64),
    shift: bool,
    area: (f64, f64),
}

impl Editor {
    pub fn new(doc: Document, style: Style) -> Self {
        let history = History::new(doc.marks.len());
        Self {
            cache: Cache::new(&doc),
            doc,
            history,
            tool: Tool::Arrow,
            temporary_tool: None,
            start: Tool::Arrow,
            style,
            typing: None,
            list_open: false,
            picking_region: false,
            flash: 0.0,
            fitted: false,
            pending_crop: None,
            gesture: None,
            pointer: (0.0, 0.0),
            shift: false,
            area: (1.0, 1.0),
        }
    }

    pub fn replace_source(&mut self, source: Source) {
        self.doc.source = source;
        self.cache = Cache::new(&self.doc);
        self.doc.crop = None;
        self.doc.marks.clear();
        self.doc.selected = None;
        self.history = History::new(0);
        self.gesture = None;
        self.pending_crop = None;
        self.set_tool(self.start);
    }

    pub fn starting_with(mut self, tool: Option<Tool>) -> Self {
        if let Some(tool) = tool {
            self.start = tool;
            self.set_tool(tool);
        }
        self
    }

    pub fn set_tool(&mut self, tool: Tool) {
        self.tool = tool;
        self.temporary_tool = None;
        if tool == Tool::Crop && self.pending_crop.is_none() {
            self.pending_crop = self.doc.crop;
        }
    }

    pub fn tool_key_down(&mut self, tool: Tool) -> bool {
        if self.temporary_tool.is_some() || self.gesture.is_some() || self.picking_region {
            return false;
        }
        if matches!(self.tool, Tool::Crop | Tool::Text) {
            self.set_tool(tool);
            return true;
        }
        self.temporary_tool = Some(BorrowedTool {
            previous: self.tool,
            active: tool,
            since: Instant::now(),
            used: false,
            released: false,
        });
        let changed = self.tool != tool;
        self.tool = tool;
        changed
    }

    pub fn tool_key_up(&mut self, tool: Tool) -> bool {
        if self.gesture.is_some() {
            if let Some(held) = self.temporary_tool.as_mut()
                && held.active == tool
            {
                held.released = true;
            }
            return false;
        }
        let Some(held) = self.temporary_tool.take() else {
            return false;
        };
        if held.active != tool {
            self.temporary_tool = Some(held);
            return false;
        }
        if held.used || held.since.elapsed() >= TOOL_HOLD {
            let changed = self.tool != held.previous;
            self.tool = held.previous;
            changed
        } else {
            false
        }
    }

    pub fn cancel_tool_key(&mut self) -> bool {
        let Some(held) = self.temporary_tool.take() else {
            return false;
        };
        let changed = self.tool != held.previous;
        self.tool = held.previous;
        changed
    }

    pub fn step_stroke(&mut self, by: i32) {
        let width = (self.style.stroke as i32 + by).clamp(1, 3);
        self.style.stroke = stroke(width as u32);
    }

    pub fn scroll_stroke(&mut self, dy: f64) -> bool {
        if !matches!(
            self.tool,
            Tool::Text | Tool::Number | Tool::Arrow | Tool::Line | Tool::Blur
        ) || dy == 0.0
        {
            return false;
        }
        self.step_stroke(if dy < 0.0 { 1 } else { -1 });
        true
    }

    pub fn apply_crop(&mut self) -> bool {
        let Some(crop) = self.pending_crop.take() else {
            return false;
        };
        if self.doc.crop != Some(crop) {
            self.apply(Op::SetCrop(Some(crop)));
        }
        true
    }

    pub fn exported(&self) -> Document {
        let mut doc = self.doc.clone();
        if let Some(crop) = self.pending_crop {
            doc.crop = Some(crop);
        }
        doc
    }

    pub fn shown(&self) -> Rect<BufferPx> {
        let (width, height) = frame_size(&self.doc);
        let whole = Rect {
            x: BufferPx(0),
            y: BufferPx(0),
            width: BufferPx(width as i32),
            height: BufferPx(height as i32),
        };
        match (self.pending_crop, self.doc.crop) {
            (None, Some(crop)) => clipped(crop, whole),
            _ => whole,
        }
    }

    pub fn drawn(&self) -> bool {
        self.history.can_undo()
    }

    pub fn view(&self) -> View {
        let shown = self.shown();
        let size = (f64::from(shown.width.0), f64::from(shown.height.0));
        let fit = match &self.doc.source {
            Source::Window { scale, .. } if !self.fitted => View::fit(
                size,
                self.area,
                1.0 / scale.0.max(f64::MIN_POSITIVE),
                WINDOW_MARGIN,
            ),
            Source::Output { .. } if !self.fitted && self.doc.crop.is_none() => {
                return View::fit(frame_size(&self.doc), self.area, f64::INFINITY, 0.0);
            }
            _ => View::fit(size, self.area, f64::INFINITY, 0.0),
        };
        View {
            x: fit.x - f64::from(shown.x.0) * fit.factor,
            y: fit.y - f64::from(shown.y.0) * fit.factor,
            factor: fit.factor,
        }
    }

    pub fn interaction(&self) -> Interaction {
        match (
            self.typing,
            self.gesture.as_ref(),
            self.doc.selected,
            self.list_open,
        ) {
            (Some(_), ..) => Interaction::Typing { id: TYPING },
            (None, Some(gesture), ..) if !matches!(gesture, Gesture::Pending { .. }) => {
                Interaction::Dragging { tool: self.tool }
            }
            _ if self.pending_crop.is_some() => Interaction::CropPending,
            (None, _, Some(id), _) => Interaction::Selected { id },
            (None, _, None, true) => Interaction::ListOpen,
            _ => Interaction::Idle,
        }
    }

    pub fn escape(&mut self) -> EscOutcome {
        let outcome = esc(&self.interaction());
        match outcome {
            EscOutcome::DiscardText => self.typing = None,
            EscOutcome::AbortDrag => self.gesture = None,
            EscOutcome::DiscardCrop => self.pending_crop = None,
            EscOutcome::Deselect => self.doc.selected = None,
            EscOutcome::CloseList => self.list_open = false,
            EscOutcome::Cancel => {}
        }
        if outcome == EscOutcome::AbortDrag
            && self
                .temporary_tool
                .as_ref()
                .is_some_and(|held| held.released)
        {
            self.cancel_tool_key();
        }
        outcome
    }

    pub fn apply(&mut self, op: Op) {
        self.history.apply(&mut self.doc, op);
        self.forget_missing_selection();
    }

    pub fn undo(&mut self) {
        self.history.undo(&mut self.doc);
        self.forget_missing_selection();
    }

    pub fn redo(&mut self) {
        self.history.redo(&mut self.doc);
        self.forget_missing_selection();
    }

    pub fn delete(&mut self) {
        if let Some(id) = self.doc.selected.take() {
            self.apply(Op::Remove(id));
        }
    }

    pub fn commit_text(&mut self, text: &str) {
        let Some(at) = self.typing.take() else {
            return;
        };
        let text: String = text.chars().take(MAX_TEXT_CHARS).collect();
        if !text.trim().is_empty() {
            self.apply(Op::Add(Mark::Text {
                at,
                text,
                style: self.style,
            }));
        }
    }

    pub fn region(&self) -> Option<(Selection, Rect<BufferPx>)> {
        let Some(Gesture::Region { x, y }) = self.gesture.as_ref() else {
            return None;
        };
        let (x, y) = (*x, *y);
        let (px, py) = self.pointer;
        let view = self.view();
        Some((
            (x, y, px - x, py - y),
            between(view.to_buffer(x, y), view.to_buffer(px, py), false),
        ))
    }

    fn forget_missing_selection(&mut self) {
        if let Some(id) = self.doc.selected
            && !self.doc.marks.iter().any(|(mark, _)| *mark == id)
        {
            self.doc.selected = None;
        }
    }

    fn at(&self, x: f64, y: f64) -> Point<BufferPx> {
        self.view().to_buffer(x, y)
    }

    fn mark_at(&self, point: Point<BufferPx>) -> Option<MarkId> {
        self.doc.hit(point, |mark| {
            outline_rect(render::outline(&self.doc, mark, &self.cache))
        })
    }

    pub fn press(&mut self, x: f64, y: f64) {
        if let Some(held) = self.temporary_tool.as_mut() {
            held.used = true;
        }
        self.pointer = (x, y);
        self.gesture = Some(if self.picking_region {
            Gesture::Region { x, y }
        } else if self.tool == Tool::Blur {
            Gesture::Path {
                points: vec![self.at(x, y)],
            }
        } else {
            Gesture::Pending { x, y }
        });
    }

    pub fn press_resize(&mut self, x: f64, y: f64) -> bool {
        let Some((id, mark)) = self
            .doc
            .selected
            .and_then(|id| self.doc.marks.iter().find(|(mark_id, _)| *mark_id == id))
        else {
            return false;
        };
        if matches!(mark, Mark::Number { .. } | Mark::Text { .. }) {
            return false;
        }
        let bounds = outline_rect(render::outline(&self.doc, mark, &self.cache));
        let at = self.at(x, y);
        let handle = handle_near(bounds, at, self.handle());
        let Some(handle) = handle else {
            return false;
        };
        self.pointer = (x, y);
        self.gesture = Some(Gesture::ResizeMark {
            id: *id,
            original: mark.clone(),
            bounds,
            handle,
            from: at,
            grip: resize_grip(bounds, handle),
        });
        true
    }

    pub fn resizing_mark(&self) -> bool {
        matches!(self.gesture, Some(Gesture::ResizeMark { .. }))
    }

    pub fn cursor_at(&self, x: f64, y: f64) -> &'static str {
        if self.picking_region {
            return "crosshair";
        }
        match self.gesture.as_ref() {
            Some(Gesture::Moving { .. } | Gesture::MovingCrop { .. }) => return "move",
            Some(Gesture::Resizing { crop, side }) => {
                return resize_cursor(*crop, ResizeHandle::Side(*side));
            }
            Some(Gesture::ResizeMark { bounds, handle, .. }) => {
                return resize_cursor(*bounds, *handle);
            }
            Some(Gesture::Cropping { anchor }) => {
                return self.pending_crop.map_or("crosshair", |crop| {
                    if corners(crop).contains(anchor) {
                        resize_cursor(crop, ResizeHandle::Corner(*anchor))
                    } else {
                        "crosshair"
                    }
                });
            }
            Some(Gesture::Drawing { .. } | Gesture::Path { .. } | Gesture::Region { .. }) => {
                return "crosshair";
            }
            _ => {}
        }
        let at = self.at(x, y);
        if self.tool == Tool::Crop {
            return match self.pending_crop {
                Some(crop) => handle_near(crop, at, self.handle())
                    .map(|handle| resize_cursor(crop, handle))
                    .unwrap_or(if inside(crop, at) {
                        "move"
                    } else {
                        "crosshair"
                    }),
                None => "crosshair",
            };
        }
        if let Some(id) = self.doc.selected
            && let Some((_, mark)) = self.doc.marks.iter().find(|(mark_id, _)| *mark_id == id)
        {
            if !matches!(mark, Mark::Number { .. } | Mark::Text { .. }) {
                let bounds = outline_rect(render::outline(&self.doc, mark, &self.cache));
                if let Some(handle) = handle_near(bounds, at, self.handle()) {
                    return resize_cursor(bounds, handle);
                }
            }
            if self.mark_at(at) == Some(id) {
                return "move";
            }
        }
        "crosshair"
    }

    pub fn hover(&mut self, x: f64, y: f64) -> &'static str {
        self.pointer = (x, y);
        self.cursor_at(x, y)
    }

    pub fn cursor(&self) -> &'static str {
        self.cursor_at(self.pointer.0, self.pointer.1)
    }

    pub fn motion(&mut self, x: f64, y: f64, shift: bool) {
        self.pointer = (x, y);
        self.shift = shift;
        let to = self.at(x, y);
        if let Some(Gesture::Path { points }) = &mut self.gesture {
            if points.len() < MAX_PATH_POINTS && points.last() != Some(&to) {
                points.push(to);
            }
            return;
        }
        let Some(Gesture::Pending { x: x0, y: y0 }) = self.gesture.clone() else {
            return;
        };
        if (x - x0).hypot(y - y0) < CLICK_SLOP {
            return;
        }
        let from = self.at(x0, y0);
        let selected = self
            .doc
            .selected
            .filter(|id| self.mark_at(from) == Some(*id));
        self.gesture = Some(match (self.tool, selected, self.pending_crop) {
            (_, Some(id), _) if self.tool != Tool::Crop => Gesture::Moving { id, from },
            (Tool::Crop, _, Some(crop)) => match (
                corner_near(crop, from, self.handle()),
                side_near(crop, from, self.handle()),
            ) {
                (Some(anchor), _) => Gesture::Cropping { anchor },
                (None, Some(side)) => Gesture::Resizing { crop, side },
                (None, None) if inside(crop, from) => Gesture::MovingCrop { crop, from },
                (None, None) => Gesture::Cropping { anchor: from },
            },
            (Tool::Crop, _, None) => Gesture::Cropping { anchor: from },
            (Tool::Freehand | Tool::Highlighter, _, _) => Gesture::Path {
                points: vec![from, to],
            },
            _ => Gesture::Drawing { from },
        });
    }

    pub fn release(&mut self, x: f64, y: f64, shift: bool) {
        self.motion(x, y, shift);
        let Some(gesture) = self.gesture.take() else {
            return;
        };
        let to = self.at(x, y);
        match gesture {
            Gesture::Pending { x, y } => self.click(self.at(x, y)),
            Gesture::Region { x, y } => {
                self.gesture = Some(Gesture::Region { x, y });
                if let Some((_, crop)) = self.region().filter(|(_, crop)| !degenerate(*crop)) {
                    self.doc.crop = Some(crop);
                    self.picking_region = false;
                }
                self.gesture = None;
            }
            Gesture::Drawing { from } => {
                if let Some(mark) = self.drawn_mark(from, to) {
                    self.doc.selected = None;
                    self.apply(Op::Add(mark));
                }
            }
            Gesture::Path { points } => {
                if self.tool == Tool::Blur {
                    self.doc.selected = None;
                    self.apply(Op::Add(Mark::Pixelate {
                        points,
                        width: self.brush_width(),
                    }));
                } else if points.len() >= 2 {
                    self.doc.selected = None;
                    self.apply(Op::Add(Mark::Path {
                        points,
                        style: self.style,
                        highlighter: self.tool == Tool::Highlighter,
                    }));
                }
            }
            Gesture::Moving { id, from } => {
                if let Some(anchor) = self.anchor(id) {
                    self.apply(Op::Move(id, offset(anchor, from, to)));
                }
            }
            Gesture::Cropping { anchor } => {
                let crop = between(anchor, to, self.shift);
                if !degenerate(crop) {
                    self.pending_crop = Some(crop);
                }
            }
            Gesture::MovingCrop { crop, from } => {
                let origin = offset(
                    Point {
                        x: crop.x,
                        y: crop.y,
                    },
                    from,
                    to,
                );
                self.pending_crop = Some(Rect {
                    x: origin.x,
                    y: origin.y,
                    ..crop
                });
            }
            Gesture::Resizing { crop, side } => {
                let crop = resized(crop, side, to);
                if !degenerate(crop) {
                    self.pending_crop = Some(crop);
                }
            }
            Gesture::ResizeMark {
                id,
                original,
                bounds,
                handle,
                from,
                grip,
            } => {
                if let Some(mark) = resize_mark(&original, bounds, handle, offset(grip, from, to)) {
                    self.apply(Op::Replace(id, mark));
                }
            }
        }
        if self
            .temporary_tool
            .as_ref()
            .is_some_and(|held| held.released)
        {
            self.cancel_tool_key();
        }
    }

    fn click(&mut self, at: Point<BufferPx>) {
        if let Some(id) = self.mark_at(at) {
            self.doc.selected = Some(id);
            return;
        }
        self.doc.selected = None;
        match self.tool {
            Tool::Number => self.apply(Op::Add(Mark::Number {
                at,
                style: self.style,
            })),
            Tool::Text => self.typing = Some(at),
            _ => {}
        }
    }

    fn anchor(&self, id: MarkId) -> Option<Point<BufferPx>> {
        let (_, mark) = self.doc.marks.iter().find(|(mark, _)| *mark == id)?;
        Some(match mark {
            Mark::Arrow { from, .. } | Mark::Line { from, .. } => *from,
            Mark::Path { points, .. } | Mark::Pixelate { points, .. } => points.first().copied()?,
            Mark::Rect { rect, .. } | Mark::Oval { rect, .. } | Mark::Blur { rect } => Point {
                x: rect.x,
                y: rect.y,
            },
            Mark::Number { at, .. } | Mark::Text { at, .. } => *at,
        })
    }

    fn drawn_mark(&self, from: Point<BufferPx>, to: Point<BufferPx>) -> Option<Mark> {
        let style = self.style;
        let area = between(from, to, self.shift);
        let mark = match self.tool {
            Tool::Arrow => Mark::Arrow {
                from,
                to: snapped(from, to, self.shift),
                style,
            },
            Tool::Line => Mark::Line {
                from,
                to: snapped(from, to, self.shift),
                style,
            },
            Tool::Rect => Mark::Rect {
                rect: area,
                style,
                filled: false,
            },
            Tool::FilledRect => Mark::Rect {
                rect: area,
                style,
                filled: true,
            },
            Tool::Oval => Mark::Oval { rect: area, style },
            Tool::Number => Mark::Number { at: from, style },
            Tool::Text | Tool::Crop | Tool::Freehand | Tool::Highlighter | Tool::Blur => {
                return None;
            }
        };
        let empty = match &mark {
            Mark::Arrow { from, to, .. } | Mark::Line { from, to, .. } => from == to,
            Mark::Rect { rect, .. } | Mark::Oval { rect, .. } | Mark::Blur { rect } => {
                degenerate(*rect)
            }
            _ => false,
        };
        (!empty).then_some(mark)
    }

    fn handle(&self) -> i32 {
        (f64::from(HANDLE) / self.view().factor).round() as i32
    }

    fn brush_width(&self) -> u32 {
        ((16.0 + 12.0 * f64::from(self.style.stroke as u8)) * self.doc.scale())
            .round()
            .clamp(1.0, 128.0) as u32
    }

    fn preview(&self) -> Preview {
        let mut doc = self.doc.clone();
        let mut pending = self.pending_crop;
        let (px, py) = self.pointer;
        let to = self.at(px, py);
        let mut moving = None;
        match self.gesture.clone() {
            Some(Gesture::Drawing { from }) => match self.tool {
                Tool::Text => {}
                _ => {
                    if let Some(mark) = self.drawn_mark(from, to) {
                        doc.marks.push((TYPING, mark));
                    }
                }
            },
            Some(Gesture::Path { points }) => {
                if self.tool == Tool::Blur {
                    doc.marks.push((
                        TYPING,
                        Mark::Pixelate {
                            points,
                            width: self.brush_width(),
                        },
                    ));
                } else if points.len() >= 2 {
                    doc.marks.push((
                        TYPING,
                        Mark::Path {
                            points,
                            style: self.style,
                            highlighter: self.tool == Tool::Highlighter,
                        },
                    ));
                }
            }
            Some(Gesture::Moving { id, from }) => {
                if let Some((_, mark @ (Mark::Blur { .. } | Mark::Pixelate { .. }))) =
                    self.doc.marks.iter().find(|(mark, _)| *mark == id)
                {
                    moving = Some((id, mark.clone()));
                }
                if let Some(anchor) = self.anchor(id) {
                    History::new(0).apply(&mut doc, Op::Move(id, offset(anchor, from, to)));
                }
            }
            Some(Gesture::Cropping { anchor }) => pending = Some(between(anchor, to, self.shift)),
            Some(Gesture::MovingCrop { crop, from }) => {
                let origin = offset(
                    Point {
                        x: crop.x,
                        y: crop.y,
                    },
                    from,
                    to,
                );
                pending = Some(Rect {
                    x: origin.x,
                    y: origin.y,
                    ..crop
                });
            }
            Some(Gesture::Resizing { crop, side }) => pending = Some(resized(crop, side, to)),
            Some(Gesture::ResizeMark {
                id,
                original,
                bounds,
                handle,
                from,
                grip,
            }) => {
                if let Some(mark) = resize_mark(&original, bounds, handle, offset(grip, from, to))
                    && let Some((_, shown)) =
                        doc.marks.iter_mut().find(|(mark_id, _)| *mark_id == id)
                {
                    *shown = mark;
                }
            }
            _ => {}
        }
        Preview {
            doc,
            pending,
            moving,
        }
    }

    fn draw(&mut self, snapshot: &gtk4::Snapshot, width: f32, height: f32) {
        self.area = (f64::from(width), f64::from(height));
        let view = self.view();
        let everything = graphene::Rect::new(0.0, 0.0, width, height);
        if !self.fitted && matches!(self.doc.source, Source::Window { .. }) {
            snapshot.append_color(&SCRIM, &everything);
        }
        let Preview {
            doc,
            pending,
            moving,
        } = self.preview();
        let (frame_width, frame_height) = frame_size(&doc);
        let frame = graphene::Rect::new(0.0, 0.0, frame_width as f32, frame_height as f32);
        let factor = view.factor as f32;
        snapshot.save();
        snapshot.translate(&graphene::Point::new(view.x as f32, view.y as f32));
        snapshot.scale(factor, factor);
        let clip = match (pending, doc.crop) {
            (None, Some(crop)) => Some(rect(crop)),
            _ => None,
        };
        if let Some(clip) = &clip {
            snapshot.push_clip(clip);
        }
        render::image(snapshot, &doc, &self.cache);
        if let Some(crop) = pending {
            dim_outside(snapshot, frame, rect(crop));
        }
        render::marks(snapshot, &doc, &self.cache, moving);
        if clip.is_some() {
            snapshot.pop();
        }
        if let Some(crop) = pending
            && self.tool == Tool::Crop
        {
            handles(snapshot, rect(crop), HANDLE / factor);
        }
        if let Some(id) = doc.selected
            && let Some((_, mark)) = doc.marks.iter().find(|(mark, _)| *mark == id)
        {
            let outline = render::outline(&doc, mark, &self.cache);
            dashed(snapshot, outline, factor);
            if !matches!(mark, Mark::Number { .. } | Mark::Text { .. }) && self.tool != Tool::Crop {
                handles(snapshot, outline, HANDLE / factor);
            }
        }
        snapshot.restore();
        if self.flash > 0.0 {
            snapshot.append_color(
                &gdk::RGBA::new(1.0, 1.0, 1.0, self.flash as f32),
                &everything,
            );
        }
    }
}

pub fn palette(index: u32) -> PaletteColor {
    match index {
        1 => PaletteColor::Yellow,
        2 => PaletteColor::Green,
        3 => PaletteColor::Blue,
        4 => PaletteColor::White,
        _ => PaletteColor::Red,
    }
}

pub fn palette_index(color: PaletteColor) -> u32 {
    match color {
        PaletteColor::Red => 0,
        PaletteColor::Yellow => 1,
        PaletteColor::Green => 2,
        PaletteColor::Blue => 3,
        PaletteColor::White => 4,
    }
}

pub fn stroke(width: u32) -> Stroke {
    match width {
        1 => Stroke::Thin,
        3 => Stroke::Thick,
        _ => Stroke::Medium,
    }
}

pub fn text_entry() -> (gtk4::ScrolledWindow, gtk4::TextView) {
    let view = gtk4::TextView::builder()
        .wrap_mode(gtk4::WrapMode::WordChar)
        .accepts_tab(false)
        .left_margin(6)
        .right_margin(6)
        .top_margin(4)
        .bottom_margin(4)
        .build();
    let scroller = gtk4::ScrolledWindow::builder()
        .child(&view)
        .width_request(240)
        .height_request(40)
        .propagate_natural_height(true)
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .build();
    (scroller, view)
}

pub fn resize_text_entry(view: &gtk4::TextView, scroller: &gtk4::ScrolledWindow, max_height: i32) {
    let end = view.buffer().end_iter();
    let location = view.iter_location(&end);
    let height = location.y() + location.height() + view.top_margin() + view.bottom_margin();
    scroller.set_height_request(height.clamp(40, max_height.max(40)));
}

pub fn tool_name(tool: Tool) -> &'static str {
    match tool {
        Tool::Crop => "crop",
        Tool::Arrow => "arrow",
        Tool::Line => "line",
        Tool::Freehand => "freehand",
        Tool::Highlighter => "highlighter",
        Tool::Rect => "rect",
        Tool::FilledRect => "filled-rect",
        Tool::Oval => "oval",
        Tool::Number => "number",
        Tool::Text => "text",
        Tool::Blur => "blur",
    }
}

pub fn tool_named(name: &str) -> Option<Tool> {
    clap::ValueEnum::from_str(name, false).ok()
}

pub fn spring_tool(key: gdk::Key) -> Option<Tool> {
    match key {
        gdk::Key::n | gdk::Key::N => Some(Tool::Number),
        gdk::Key::b | gdk::Key::B => Some(Tool::Blur),
        gdk::Key::r | gdk::Key::R => Some(Tool::Rect),
        _ => None,
    }
}

pub fn set_cursor(widget: &impl IsA<gtk4::Widget>, name: &str) {
    let widget = widget.as_ref();
    if widget.cursor().and_then(|cursor| cursor.name()).as_deref() != Some(name) {
        widget.set_cursor_from_name(Some(name));
    }
}

struct Preview {
    doc: Document,
    pending: Option<Rect<BufferPx>>,
    moving: render::Moving,
}

fn clipped(crop: Rect<BufferPx>, whole: Rect<BufferPx>) -> Rect<BufferPx> {
    let x0 = crop.x.0.clamp(whole.x.0, whole.x.0 + whole.width.0);
    let y0 = crop.y.0.clamp(whole.y.0, whole.y.0 + whole.height.0);
    let x1 = (crop.x.0 + crop.width.0).clamp(x0, whole.x.0 + whole.width.0);
    let y1 = (crop.y.0 + crop.height.0).clamp(y0, whole.y.0 + whole.height.0);
    match x1 > x0 && y1 > y0 {
        true => Rect {
            x: BufferPx(x0),
            y: BufferPx(y0),
            width: BufferPx(x1 - x0),
            height: BufferPx(y1 - y0),
        },
        false => whole,
    }
}

fn outline_rect(bounds: graphene::Rect) -> Rect<BufferPx> {
    let (x, y) = (bounds.x().floor() as i32, bounds.y().floor() as i32);
    Rect {
        x: BufferPx(x),
        y: BufferPx(y),
        width: BufferPx((bounds.x() + bounds.width()).ceil() as i32 - x),
        height: BufferPx((bounds.y() + bounds.height()).ceil() as i32 - y),
    }
}

fn frame_size(doc: &Document) -> (f64, f64) {
    let frame = match &doc.source {
        Source::Output { frame, .. }
        | Source::Window { frame, .. }
        | Source::File { frame, .. } => frame,
    };
    (f64::from(frame.width), f64::from(frame.height))
}

fn offset(anchor: Point<BufferPx>, from: Point<BufferPx>, to: Point<BufferPx>) -> Point<BufferPx> {
    Point {
        x: BufferPx(anchor.x.0 + to.x.0 - from.x.0),
        y: BufferPx(anchor.y.0 + to.y.0 - from.y.0),
    }
}

fn between(from: Point<BufferPx>, to: Point<BufferPx>, square: bool) -> Rect<BufferPx> {
    let (mut dx, mut dy) = (to.x.0 - from.x.0, to.y.0 - from.y.0);
    if square {
        let side = dx.abs().max(dy.abs());
        let sign = |delta: i32| if delta < 0 { -1 } else { 1 };
        dx = side * sign(dx);
        dy = side * sign(dy);
    }
    Rect {
        x: BufferPx(from.x.0.min(from.x.0 + dx)),
        y: BufferPx(from.y.0.min(from.y.0 + dy)),
        width: BufferPx(dx.abs()),
        height: BufferPx(dy.abs()),
    }
}

fn snapped(from: Point<BufferPx>, to: Point<BufferPx>, snap: bool) -> Point<BufferPx> {
    if !snap {
        return to;
    }
    let (dx, dy) = (f64::from(to.x.0 - from.x.0), f64::from(to.y.0 - from.y.0));
    let step = 15f64.to_radians();
    let angle = (dy.atan2(dx) / step).round() * step;
    let length = dx.hypot(dy);
    Point {
        x: BufferPx(from.x.0 + (length * angle.cos()).round() as i32),
        y: BufferPx(from.y.0 + (length * angle.sin()).round() as i32),
    }
}

fn degenerate(rect: Rect<BufferPx>) -> bool {
    rect.width.0 == 0 || rect.height.0 == 0
}

fn inside(rect: Rect<BufferPx>, at: Point<BufferPx>) -> bool {
    (rect.x.0..rect.x.0 + rect.width.0).contains(&at.x.0)
        && (rect.y.0..rect.y.0 + rect.height.0).contains(&at.y.0)
}

fn corners(rect: Rect<BufferPx>) -> [Point<BufferPx>; 4] {
    let (x0, y0) = (rect.x.0, rect.y.0);
    let (x1, y1) = (x0 + rect.width.0, y0 + rect.height.0);
    [(x0, y0), (x1, y0), (x1, y1), (x0, y1)].map(|(x, y)| Point {
        x: BufferPx(x),
        y: BufferPx(y),
    })
}

fn sides(rect: Rect<BufferPx>) -> [(Side, Point<BufferPx>); 4] {
    let (x0, y0) = (rect.x.0, rect.y.0);
    let (x1, y1) = (x0 + rect.width.0, y0 + rect.height.0);
    let (mx, my) = (x0 + rect.width.0 / 2, y0 + rect.height.0 / 2);
    [
        (Side::Top, mx, y0),
        (Side::Right, x1, my),
        (Side::Bottom, mx, y1),
        (Side::Left, x0, my),
    ]
    .map(|(side, x, y)| {
        (
            side,
            Point {
                x: BufferPx(x),
                y: BufferPx(y),
            },
        )
    })
}

fn side_near(rect: Rect<BufferPx>, at: Point<BufferPx>, reach: i32) -> Option<Side> {
    sides(rect)
        .into_iter()
        .find(|(_, mid)| (mid.x.0 - at.x.0).abs() <= reach && (mid.y.0 - at.y.0).abs() <= reach)
        .map(|(side, _)| side)
}

fn handle_near(rect: Rect<BufferPx>, at: Point<BufferPx>, reach: i32) -> Option<ResizeHandle> {
    corner_near(rect, at, reach)
        .map(ResizeHandle::Corner)
        .or_else(|| side_near(rect, at, reach).map(ResizeHandle::Side))
}

fn resize_cursor(bounds: Rect<BufferPx>, handle: ResizeHandle) -> &'static str {
    match handle {
        ResizeHandle::Corner(anchor) => {
            if (anchor.x == bounds.x) == (anchor.y == bounds.y) {
                "nwse-resize"
            } else {
                "nesw-resize"
            }
        }
        ResizeHandle::Side(Side::Left | Side::Right) => "ew-resize",
        ResizeHandle::Side(Side::Top | Side::Bottom) => "ns-resize",
    }
}

fn resize_grip(bounds: Rect<BufferPx>, handle: ResizeHandle) -> Point<BufferPx> {
    match handle {
        ResizeHandle::Corner(anchor) => Point {
            x: BufferPx(bounds.x.0 + bounds.width.0 - (anchor.x.0 - bounds.x.0)),
            y: BufferPx(bounds.y.0 + bounds.height.0 - (anchor.y.0 - bounds.y.0)),
        },
        ResizeHandle::Side(Side::Left) => sides(bounds)[3].1,
        ResizeHandle::Side(Side::Top) => sides(bounds)[0].1,
        ResizeHandle::Side(Side::Right) => sides(bounds)[1].1,
        ResizeHandle::Side(Side::Bottom) => sides(bounds)[2].1,
    }
}

fn resized(rect: Rect<BufferPx>, side: Side, to: Point<BufferPx>) -> Rect<BufferPx> {
    let (mut x0, mut y0) = (rect.x.0, rect.y.0);
    let (mut x1, mut y1) = (x0 + rect.width.0, y0 + rect.height.0);
    match side {
        Side::Left => x0 = to.x.0,
        Side::Top => y0 = to.y.0,
        Side::Right => x1 = to.x.0,
        Side::Bottom => y1 = to.y.0,
    }
    Rect {
        x: BufferPx(x0.min(x1)),
        y: BufferPx(y0.min(y1)),
        width: BufferPx((x1 - x0).abs()),
        height: BufferPx((y1 - y0).abs()),
    }
}

fn resize_mark(
    mark: &Mark,
    bounds: Rect<BufferPx>,
    handle: ResizeHandle,
    to: Point<BufferPx>,
) -> Option<Mark> {
    let next = match handle {
        ResizeHandle::Corner(anchor) => between(anchor, to, false),
        ResizeHandle::Side(side) => resized(bounds, side, to),
    };
    if degenerate(bounds) || degenerate(next) {
        return None;
    }
    let point = |at| scaled_point(at, bounds, next);
    let area = |rect: Rect<BufferPx>| {
        between(
            point(Point {
                x: rect.x,
                y: rect.y,
            }),
            point(Point {
                x: BufferPx(rect.x.0 + rect.width.0),
                y: BufferPx(rect.y.0 + rect.height.0),
            }),
            false,
        )
    };
    let changed = match mark {
        Mark::Arrow { from, to, style } => Mark::Arrow {
            from: point(*from),
            to: point(*to),
            style: *style,
        },
        Mark::Line { from, to, style } => Mark::Line {
            from: point(*from),
            to: point(*to),
            style: *style,
        },
        Mark::Path {
            points,
            style,
            highlighter,
        } => Mark::Path {
            points: points.iter().copied().map(point).collect(),
            style: *style,
            highlighter: *highlighter,
        },
        Mark::Pixelate { points, width } => Mark::Pixelate {
            points: points.iter().copied().map(point).collect(),
            width: (f64::from(*width)
                * ((f64::from(next.width.0) / f64::from(bounds.width.0))
                    + (f64::from(next.height.0) / f64::from(bounds.height.0)))
                / 2.0)
                .round()
                .clamp(1.0, 128.0) as u32,
        },
        Mark::Rect {
            rect,
            style,
            filled,
        } => Mark::Rect {
            rect: area(*rect),
            style: *style,
            filled: *filled,
        },
        Mark::Oval { rect, style } => Mark::Oval {
            rect: area(*rect),
            style: *style,
        },
        Mark::Blur { rect } => Mark::Blur { rect: area(*rect) },
        Mark::Number { .. } | Mark::Text { .. } => return None,
    };
    let valid = match &changed {
        Mark::Arrow { from, to, .. } | Mark::Line { from, to, .. } => from != to,
        Mark::Path { points, .. } => points.windows(2).any(|pair| pair[0] != pair[1]),
        Mark::Pixelate { points, .. } => !points.is_empty(),
        Mark::Rect { rect, .. } | Mark::Oval { rect, .. } | Mark::Blur { rect } => {
            !degenerate(*rect)
        }
        Mark::Number { .. } | Mark::Text { .. } => false,
    };
    valid.then_some(changed)
}

fn scaled_point(
    at: Point<BufferPx>,
    before: Rect<BufferPx>,
    after: Rect<BufferPx>,
) -> Point<BufferPx> {
    let scale = |value: i32, start: i32, size: i32, next: i32, extent: i32| {
        (f64::from(next)
            + (f64::from(value) - f64::from(start)) * f64::from(extent) / f64::from(size))
        .round() as i32
    };
    Point {
        x: BufferPx(scale(
            at.x.0,
            before.x.0,
            before.width.0,
            after.x.0,
            after.width.0,
        )),
        y: BufferPx(scale(
            at.y.0,
            before.y.0,
            before.height.0,
            after.y.0,
            after.height.0,
        )),
    }
}

fn corner_near(rect: Rect<BufferPx>, at: Point<BufferPx>, reach: i32) -> Option<Point<BufferPx>> {
    let corners = corners(rect);
    let near = corners.iter().position(|corner| {
        (corner.x.0 - at.x.0).abs() <= reach && (corner.y.0 - at.y.0).abs() <= reach
    })?;
    Some(corners[(near + 2) % 4])
}

fn rect(rect: Rect<BufferPx>) -> graphene::Rect {
    graphene::Rect::new(
        rect.x.0 as f32,
        rect.y.0 as f32,
        rect.width.0 as f32,
        rect.height.0 as f32,
    )
}

fn dim_outside(snapshot: &gtk4::Snapshot, frame: graphene::Rect, crop: graphene::Rect) {
    let dim = gdk::RGBA::new(0.0, 0.0, 0.0, 0.5);
    let (x0, y0) = (frame.x(), frame.y());
    let (x1, y1) = (x0 + frame.width(), y0 + frame.height());
    let left = crop.x().clamp(x0, x1);
    let top = crop.y().clamp(y0, y1);
    let right = (crop.x() + crop.width()).clamp(left, x1);
    let bottom = (crop.y() + crop.height()).clamp(top, y1);
    for (x, y, width, height) in [
        (x0, y0, x1 - x0, top - y0),
        (x0, bottom, x1 - x0, y1 - bottom),
        (x0, top, left - x0, bottom - top),
        (right, top, x1 - right, bottom - top),
    ] {
        snapshot.append_color(&dim, &graphene::Rect::new(x, y, width, height));
    }
}

fn handles(snapshot: &gtk4::Snapshot, crop: graphene::Rect, size: f32) {
    let path = gsk::PathBuilder::new();
    let (x0, y0) = (crop.x(), crop.y());
    let (x1, y1) = (x0 + crop.width(), y0 + crop.height());
    let (mx, my) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
    for (x, y) in [
        (x0, y0),
        (mx, y0),
        (x1, y0),
        (x1, my),
        (x1, y1),
        (mx, y1),
        (x0, y1),
        (x0, my),
    ] {
        path.add_rect(&graphene::Rect::new(
            x - size / 2.0,
            y - size / 2.0,
            size,
            size,
        ));
    }
    snapshot.append_fill(&path.to_path(), gsk::FillRule::Winding, &gdk::RGBA::WHITE);
    snapshot.append_stroke(&path.to_path(), &gsk::Stroke::new(1.0), &gdk::RGBA::BLACK);
}

fn dashed(snapshot: &gtk4::Snapshot, bounds: graphene::Rect, factor: f32) {
    let path = gsk::PathBuilder::new();
    path.add_rect(&bounds);
    let path = path.to_path();
    let stroke = gsk::Stroke::new(1.5 / factor);
    snapshot.append_stroke(&path, &stroke, &gdk::RGBA::new(0.0, 0.0, 0.0, 0.6));
    stroke.set_dash(&[6.0 / factor, 4.0 / factor]);
    snapshot.append_stroke(&path, &stroke, &gdk::RGBA::WHITE);
}

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct Canvas {
        pub editor: RefCell<Option<Editor>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Canvas {
        const NAME: &'static str = "ShotCanvas";
        type Type = super::Canvas;
        type ParentType = gtk4::Widget;
    }

    impl ObjectImpl for Canvas {}

    impl WidgetImpl for Canvas {
        fn snapshot(&self, snapshot: &gtk4::Snapshot) {
            let widget = self.obj();
            let (width, height) = (widget.width() as f32, widget.height() as f32);
            if let Some(editor) = self.editor.borrow_mut().as_mut() {
                editor.draw(snapshot, width, height);
            }
        }
    }
}

glib::wrapper! {
    pub struct Canvas(ObjectSubclass<imp::Canvas>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Canvas {
    pub fn new(editor: Editor) -> Self {
        let canvas: Self = glib::Object::new();
        canvas.imp().editor.replace(Some(editor));
        canvas.set_focusable(true);
        canvas.set_hexpand(true);
        canvas.set_vexpand(true);
        canvas.set_cursor_from_name(Some("crosshair"));
        canvas
    }

    pub fn read<R>(&self, look: impl FnOnce(&Editor) -> R) -> Option<R> {
        self.imp().editor.borrow().as_ref().map(look)
    }

    pub fn with<R>(&self, change: impl FnOnce(&mut Editor) -> R) -> Option<R> {
        let result = self.imp().editor.borrow_mut().as_mut().map(change);
        self.queue_draw();
        result
    }

    pub fn hover(&self, x: f64, y: f64) -> Option<&'static str> {
        self.imp()
            .editor
            .borrow_mut()
            .as_mut()
            .map(|editor| editor.hover(x, y))
    }

    pub fn take_editor(&self) -> Option<Editor> {
        self.imp().editor.take()
    }
}

mod sheet {
    use super::*;

    #[derive(Default)]
    pub struct Sheet {
        pub editor: RefCell<Option<Editor>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Sheet {
        const NAME: &'static str = "ShotSheet";
        type Type = super::Sheet;
        type Interfaces = (gdk::Paintable,);
    }

    impl ObjectImpl for Sheet {}

    impl PaintableImpl for Sheet {
        fn snapshot(&self, snapshot: &gdk::Snapshot, width: f64, height: f64) {
            let Some(snapshot) = snapshot.downcast_ref::<gtk4::Snapshot>() else {
                return;
            };
            if let Some(editor) = self.editor.borrow_mut().as_mut() {
                editor.draw(snapshot, width as f32, height as f32);
            }
        }

        fn intrinsic_width(&self) -> i32 {
            self.logical().0
        }

        fn intrinsic_height(&self) -> i32 {
            self.logical().1
        }
    }

    impl Sheet {
        fn logical(&self) -> (i32, i32) {
            self.editor.borrow().as_ref().map_or((0, 0), |editor| {
                let shown = editor.shown();
                let scale = editor.doc.scale().max(f64::MIN_POSITIVE);
                (
                    (f64::from(shown.width.0) / scale).round() as i32,
                    (f64::from(shown.height.0) / scale).round() as i32,
                )
            })
        }
    }
}

glib::wrapper! {
    pub struct Sheet(ObjectSubclass<sheet::Sheet>) @implements gdk::Paintable;
}

impl Sheet {
    pub fn new(mut editor: Editor) -> Self {
        editor.fitted = true;
        let sheet: Self = glib::Object::new();
        sheet.imp().editor.replace(Some(editor));
        sheet
    }

    pub fn read<R>(&self, look: impl FnOnce(&Editor) -> R) -> Option<R> {
        self.imp().editor.borrow().as_ref().map(look)
    }

    pub fn with<R>(&self, change: impl FnOnce(&mut Editor) -> R) -> Option<R> {
        let before = self.read(Editor::shown);
        let result = self.imp().editor.borrow_mut().as_mut().map(change);
        if self.read(Editor::shown) != before {
            self.invalidate_size();
        }
        self.invalidate_contents();
        result
    }

    pub fn hover(&self, x: f64, y: f64) -> Option<&'static str> {
        self.imp()
            .editor
            .borrow_mut()
            .as_mut()
            .map(|editor| editor.hover(x, y))
    }

    pub fn origin(&self, width: i32, height: i32) -> (f64, f64) {
        let (drawn_width, drawn_height) = self.read(|editor| editor.area).unwrap_or_default();
        (
            (f64::from(width) - drawn_width.ceil()) / 2.0,
            (f64::from(height) - drawn_height.ceil()) / 2.0,
        )
    }
}

#[cfg(test)]
mod tests;
