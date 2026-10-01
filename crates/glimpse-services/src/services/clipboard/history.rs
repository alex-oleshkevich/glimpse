use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hash, Hasher};
use std::sync::Arc;

use chrono::{DateTime, Utc};

use crate::selection::{Capture, Offer};

/// How much of an entry is kept for the panel to render. The applet caps again, to its own
/// configured width; this one exists because another application chose the content and it must be
/// bounded before it is published, not after.
const PREVIEW_CHARS: usize = 240;

pub type EntryId = u64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kind {
    Text,
    Image,
}

/// One remembered selection. `id` is a fingerprint of the content, so two entries can never share
/// one and equality by it is exact.
#[derive(Clone)]
pub struct Entry {
    pub id: EntryId,
    pub kind: Kind,
    pub mime: String,
    pub preview: String,
    pub pinned: bool,
    pub at: DateTime<Utc>,
    pub data: Arc<[u8]>,
}

/// Compared by what can actually move. `id` is the content and `at` is when it last arrived; the
/// preview, kind and mime are all functions of the content. Deriving this would memcmp every
/// entry's bytes on each republish gate, which for a history holding images is megabytes per
/// keystroke elsewhere in the panel.
impl PartialEq for Entry {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id && self.pinned == other.pinned && self.at == other.at
    }
}

/// Never the content or the preview. A clipboard routinely holds passwords, and one
/// `tracing::debug!(?state)` in a future applet would write them to the journal.
impl std::fmt::Debug for Entry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Entry")
            .field("id", &self.id)
            .field("kind", &self.kind)
            .field("mime", &self.mime)
            .field("bytes", &self.data.len())
            .field("pinned", &self.pinned)
            .field("at", &self.at)
            .finish()
    }
}

impl Entry {
    pub fn bytes(&self) -> usize {
        self.data.len()
    }

    pub fn offer(&self) -> Offer {
        Offer {
            mime: self.mime.clone(),
            data: Arc::clone(&self.data),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    pub entries: usize,
    pub max_entry_bytes: usize,
    pub max_total_bytes: usize,
    pub images: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refused {
    Sensitive,
    TooLarge,
    UnsupportedKind,
    ImagesDisabled,
}

/// Newest first. Pinned entries live in the same list and are marked, not moved: which section a
/// row belongs to is the applet's decision, and a second list is a second thing to keep ordered.
pub struct History {
    entries: Vec<Entry>,
    limits: Limits,
    /// Seeded per instance. `DefaultHasher` has fixed keys, so its output is identical in every
    /// process — and clipboard content is chosen by other applications, which this project treats
    /// as hostile. A predictable fingerprint lets one of them craft a collision and decide which
    /// entry the viewer's next paste resolves to.
    keys: RandomState,
}

impl History {
    pub fn new(limits: Limits) -> Self {
        Self {
            entries: Vec::new(),
            limits,
            keys: RandomState::new(),
        }
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    pub fn get(&self, id: EntryId) -> Option<&Entry> {
        self.entries.iter().find(|entry| entry.id == id)
    }

    pub fn total_bytes(&self) -> usize {
        self.entries.iter().map(Entry::bytes).sum()
    }

    pub fn set_limits(&mut self, limits: Limits) -> bool {
        if self.limits == limits {
            return false;
        }
        self.limits = limits;
        self.evict()
    }

    /// Records a capture, or says why it was refused. A capture whose content is already held moves
    /// that entry to the front rather than duplicating it — which is also what makes re-reading our
    /// own `offer` harmless, and is the only guard the echo has.
    pub fn insert(&mut self, capture: Capture, at: DateTime<Utc>) -> Result<bool, Refused> {
        if capture.sensitive {
            return Err(Refused::Sensitive);
        }
        let kind = kind_of(&capture.mime).ok_or(Refused::UnsupportedKind)?;
        if kind == Kind::Image && !self.limits.images {
            return Err(Refused::ImagesDisabled);
        }
        // Both budgets, before the push. Checking only the per-entry cap let an entry that cannot
        // fit the total reach `evict`, which removes oldest-first — destroying every other unpinned
        // entry before finally dropping the one that overflowed, and reporting success.
        if capture.data.len() > self.limits.max_entry_bytes
            || capture.data.len() > self.limits.max_total_bytes
        {
            return Err(Refused::TooLarge);
        }

        // The content is compared as well as the fingerprint. A seeded hash makes a collision
        // unforgeable but not impossible, and taking the dedup branch on one would discard the
        // bytes just copied and hand back the older entry's instead — the viewer pastes something
        // other than what they copied. Equal data is the overwhelming case, so this is a memcmp of
        // two identical buffers rather than a scan that usually fails.
        let id = self.fingerprint(kind, &capture.data);
        let held = self.entries.iter().position(|entry| {
            entry.id == id
                && (Arc::ptr_eq(&entry.data, &capture.data) || entry.data == capture.data)
        });
        if let Some(index) = held {
            let mut held = self.entries.remove(index);
            let moved = index != 0 || held.at != at;
            held.at = at;
            self.entries.insert(0, held);
            return Ok(moved);
        }

        self.entries.insert(
            0,
            Entry {
                id,
                kind,
                preview: preview(kind, &capture.data),
                mime: capture.mime,
                pinned: false,
                at,
                data: capture.data,
            },
        );
        self.evict();
        Ok(true)
    }

    pub fn pin(&mut self, id: EntryId, pinned: bool) -> bool {
        let Some(entry) = self.entries.iter_mut().find(|entry| entry.id == id) else {
            return false;
        };
        if entry.pinned == pinned {
            return false;
        }
        entry.pinned = pinned;
        self.evict();
        true
    }

    pub fn remove(&mut self, id: EntryId) -> bool {
        let before = self.entries.len();
        self.entries.retain(|entry| entry.id != id);
        before != self.entries.len()
    }

    /// Forgets everything that was not pinned. A pin is the one statement the viewer made about an
    /// entry, and clearing the history is not a request to undo it.
    pub fn clear(&mut self) -> bool {
        let before = self.entries.len();
        self.entries.retain(|entry| entry.pinned);
        before != self.entries.len()
    }

    /// Everything, pins included. Turning the clipboard off is a request to stop holding content,
    /// and a pin is not an exception to it — `clear` is the viewer's own action and keeps them.
    pub fn forget_all(&mut self) -> bool {
        let before = self.entries.len();
        self.entries.clear();
        before != 0
    }

    /// Drops the oldest unpinned entry until both budgets are met. A history of nothing but pinned
    /// entries is over budget and stays that way: the alternative is discarding the one thing the
    /// viewer asked to keep.
    fn evict(&mut self) -> bool {
        let mut dropped = false;
        while self.over_budget() {
            let Some(index) = self.entries.iter().rposition(|entry| !entry.pinned) else {
                break;
            };
            self.entries.remove(index);
            dropped = true;
        }
        dropped
    }

    /// Both budgets count only what eviction can actually reach. Measuring pinned bytes against
    /// `max_total_bytes` made pins consume the budget, so a history pinned to the cap evicted every
    /// new entry the moment it arrived — a silent black hole with nothing to show for it.
    /// Keyed on the **kind**, not the mime string. `kind_of` accepts five spellings of text, so the
    /// same line copied from two applications arrives under two mimes; hashing the spelling would
    /// file them as two rows of identical content — and would reopen the echo, because a backend
    /// that offers a ladder can read its own offer back under a different member of it.
    fn fingerprint(&self, kind: Kind, data: &[u8]) -> EntryId {
        let mut hasher = self.keys.build_hasher();
        kind.hash(&mut hasher);
        data.hash(&mut hasher);
        hasher.finish()
    }

    fn over_budget(&self) -> bool {
        let (count, bytes) = self
            .entries
            .iter()
            .filter(|entry| !entry.pinned)
            .fold((0, 0), |(count, bytes), entry| {
                (count + 1, bytes + entry.bytes())
            });
        count > self.limits.entries || bytes > self.limits.max_total_bytes
    }
}

/// The mime ladder the backend offers from, classified defensively: the backend chooses one of
/// these, and anything else reaching here is a bug worth refusing rather than rendering blank.
pub fn kind_of(mime: &str) -> Option<Kind> {
    let mime = mime.split(';').next().unwrap_or(mime).trim();
    match mime {
        "text/plain" | "text/html" | "TEXT" | "STRING" | "UTF8_STRING" => Some(Kind::Text),
        "image/png" | "image/jpeg" => Some(Kind::Image),
        _ => None,
    }
}

/// An image has no preview the service can make — it has no decoder and no business holding one.
/// The applet builds a thumbnail from the bytes and words the row from `kind` and the size.
fn preview(kind: Kind, data: &[u8]) -> String {
    match kind {
        Kind::Text => glimpse_utils::text::clean(&String::from_utf8_lossy(data), PREVIEW_CHARS),
        Kind::Image => String::new(),
    }
}

#[cfg(test)]
mod tests;
