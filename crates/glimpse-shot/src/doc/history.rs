use super::document::{Document, Mark, MarkId, Op};
use super::units::{BufferPx, Point, Rect};

#[derive(Debug, Clone, PartialEq)]
enum Change {
    Insert {
        index: usize,
        id: MarkId,
        mark: Mark,
    },
    Delete {
        index: usize,
        id: MarkId,
        mark: Mark,
    },
    Replace {
        index: usize,
        before: Mark,
        after: Mark,
    },
    Crop {
        before: Option<Rect<BufferPx>>,
        after: Option<Rect<BufferPx>>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct History {
    next_id: MarkId,
    undo_stack: Vec<Change>,
    redo_stack: Vec<Change>,
}

impl History {
    pub fn new(base_len: usize) -> Self {
        Self {
            next_id: base_len as MarkId,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
        }
    }

    pub fn apply(&mut self, doc: &mut Document, op: Op) {
        let change = match op {
            Op::Add(mark) => {
                let id = self.next_id;
                self.next_id += 1;
                let index = doc.marks.len();
                doc.marks.push((id, mark.clone()));
                Change::Insert { index, id, mark }
            }
            Op::Remove(id) => {
                let Some(index) = doc.marks.iter().position(|(mark_id, _)| *mark_id == id) else {
                    return;
                };
                let (id, mark) = doc.marks.remove(index);
                Change::Delete { index, id, mark }
            }
            Op::Move(id, to) => {
                let Some(index) = doc.marks.iter().position(|(mark_id, _)| *mark_id == id) else {
                    return;
                };
                let before = doc.marks[index].1.clone();
                let after = translate(&before, to);
                doc.marks[index].1 = after.clone();
                Change::Replace {
                    index,
                    before,
                    after,
                }
            }
            Op::Replace(id, after) => {
                let Some(index) = doc.marks.iter().position(|(mark_id, _)| *mark_id == id) else {
                    return;
                };
                let before = doc.marks[index].1.clone();
                if before == after {
                    return;
                }
                doc.marks[index].1 = after.clone();
                Change::Replace {
                    index,
                    before,
                    after,
                }
            }
            Op::SetCrop(new_crop) => {
                let before = doc.crop;
                doc.crop = new_crop;
                Change::Crop {
                    before,
                    after: new_crop,
                }
            }
        };
        self.redo_stack.clear();
        self.undo_stack.push(change);
    }

    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }

    pub fn undo(&mut self, doc: &mut Document) -> bool {
        let Some(change) = self.undo_stack.pop() else {
            return false;
        };
        match &change {
            Change::Insert { index, .. } => {
                doc.marks.remove(*index);
            }
            Change::Delete { index, id, mark } => {
                doc.marks.insert(*index, (*id, mark.clone()));
            }
            Change::Replace { index, before, .. } => {
                doc.marks[*index].1 = before.clone();
            }
            Change::Crop { before, .. } => {
                doc.crop = *before;
            }
        }
        self.redo_stack.push(change);
        true
    }

    pub fn redo(&mut self, doc: &mut Document) -> bool {
        let Some(change) = self.redo_stack.pop() else {
            return false;
        };
        match &change {
            Change::Insert { index, id, mark } => {
                doc.marks.insert(*index, (*id, mark.clone()));
            }
            Change::Delete { index, .. } => {
                doc.marks.remove(*index);
            }
            Change::Replace { index, after, .. } => {
                doc.marks[*index].1 = after.clone();
            }
            Change::Crop { after, .. } => {
                doc.crop = *after;
            }
        }
        self.undo_stack.push(change);
        true
    }
}

fn translate(mark: &Mark, to: Point<BufferPx>) -> Mark {
    let anchor = anchor_of(mark);
    let dx = to.x.0 - anchor.x.0;
    let dy = to.y.0 - anchor.y.0;
    shift(mark.clone(), dx, dy)
}

fn anchor_of(mark: &Mark) -> Point<BufferPx> {
    match mark {
        Mark::Arrow { from, .. } | Mark::Line { from, .. } => *from,
        Mark::Path { points, .. } | Mark::Pixelate { points, .. } => {
            points.first().copied().unwrap_or(Point {
                x: BufferPx(0),
                y: BufferPx(0),
            })
        }
        Mark::Rect { rect, .. } | Mark::Oval { rect, .. } | Mark::Blur { rect } => Point {
            x: rect.x,
            y: rect.y,
        },
        Mark::Number { at, .. } | Mark::Text { at, .. } => *at,
    }
}

fn shift(mark: Mark, dx: i32, dy: i32) -> Mark {
    let p = |pt: Point<BufferPx>| Point {
        x: BufferPx(pt.x.0 + dx),
        y: BufferPx(pt.y.0 + dy),
    };
    let r = |rect: Rect<BufferPx>| Rect {
        x: BufferPx(rect.x.0 + dx),
        y: BufferPx(rect.y.0 + dy),
        ..rect
    };
    match mark {
        Mark::Arrow { from, to, style } => Mark::Arrow {
            from: p(from),
            to: p(to),
            style,
        },
        Mark::Line { from, to, style } => Mark::Line {
            from: p(from),
            to: p(to),
            style,
        },
        Mark::Path {
            points,
            style,
            highlighter,
        } => Mark::Path {
            points: points.into_iter().map(p).collect(),
            style,
            highlighter,
        },
        Mark::Pixelate { points, width } => Mark::Pixelate {
            points: points.into_iter().map(p).collect(),
            width,
        },
        Mark::Rect {
            rect,
            style,
            filled,
        } => Mark::Rect {
            rect: r(rect),
            style,
            filled,
        },
        Mark::Oval { rect, style } => Mark::Oval {
            rect: r(rect),
            style,
        },
        Mark::Number { at, style } => Mark::Number { at: p(at), style },
        Mark::Text { at, text, style } => Mark::Text {
            at: p(at),
            text,
            style,
        },
        Mark::Blur { rect } => Mark::Blur { rect: r(rect) },
    }
}

#[cfg(test)]
mod tests;
