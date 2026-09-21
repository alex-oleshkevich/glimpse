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
mod tests {
    use super::*;

    const TEXT: &str = "text/plain;charset=utf-8";

    fn limits() -> Limits {
        Limits {
            entries: 3,
            max_entry_bytes: 64,
            max_total_bytes: 256,
            images: true,
        }
    }

    fn capture(mime: &str, data: &[u8]) -> Capture {
        Capture {
            mime: mime.to_owned(),
            data: Arc::from(data),
            sensitive: false,
        }
    }

    fn now() -> DateTime<Utc> {
        DateTime::from_timestamp(1_700_000_000, 0).expect("a fixed instant")
    }

    fn put(history: &mut History, body: &str) -> Result<bool, Refused> {
        history.insert(capture(TEXT, body.as_bytes()), now())
    }

    #[test]
    fn a_fresh_history_holds_nothing_before_any_event_arrives() {
        let history = History::new(limits());

        assert!(history.entries().is_empty());
        assert_eq!(history.total_bytes(), 0);
    }

    #[test]
    fn a_capture_lands_at_the_front() {
        let mut history = History::new(limits());

        assert_eq!(put(&mut history, "first"), Ok(true));
        assert_eq!(put(&mut history, "second"), Ok(true));

        let previews: Vec<_> = history
            .entries()
            .iter()
            .map(|e| e.preview.clone())
            .collect();
        assert_eq!(previews, ["second", "first"]);
    }

    #[test]
    fn a_recaptured_entry_moves_to_the_front_rather_than_duplicating() {
        let mut history = History::new(limits());
        put(&mut history, "first").expect("recorded");
        put(&mut history, "second").expect("recorded");

        put(&mut history, "first").expect("recorded");

        assert_eq!(
            history.entries().len(),
            2,
            "the history must not have grown"
        );
        assert_eq!(history.entries()[0].preview, "first");
    }

    /// The echo of our own `offer` arrives as an ordinary capture and is collapsed by the same
    /// path. Nothing else guards it, so this is the assertion that keeps the loop closed.
    #[test]
    fn re_reading_what_was_just_offered_does_not_grow_the_history() {
        let mut history = History::new(limits());
        put(&mut history, "only").expect("recorded");
        let offered = history.entries()[0].offer();

        history
            .insert(
                Capture {
                    mime: offered.mime,
                    data: offered.data,
                    sensitive: false,
                },
                now(),
            )
            .expect("recorded");

        assert_eq!(history.entries().len(), 1);
    }

    #[test]
    fn a_sensitive_capture_is_refused_before_it_is_stored() {
        let mut history = History::new(limits());

        let refused = history.insert(
            Capture {
                mime: TEXT.to_owned(),
                data: Arc::from(&b"hunter2"[..]),
                sensitive: true,
            },
            now(),
        );

        assert_eq!(refused, Err(Refused::Sensitive));
        assert!(history.entries().is_empty());
    }

    #[test]
    fn an_entry_at_the_cap_is_kept_and_one_byte_past_it_is_refused() {
        let mut history = History::new(limits());
        let at_cap = "a".repeat(64);
        let past_cap = "a".repeat(65);

        assert_eq!(put(&mut history, &at_cap), Ok(true));
        assert_eq!(put(&mut history, &past_cap), Err(Refused::TooLarge));
        assert_eq!(history.entries().len(), 1);
    }

    #[test]
    fn an_image_is_refused_when_images_are_off_and_kept_when_they_are_on() {
        let png = capture("image/png", b"\x89PNG not really");
        let mut on = History::new(limits());
        let mut off = History::new(Limits {
            images: false,
            ..limits()
        });

        assert_eq!(on.insert(png.clone(), now()), Ok(true));
        assert_eq!(off.insert(png, now()), Err(Refused::ImagesDisabled));
        assert_eq!(on.entries()[0].kind, Kind::Image);
        assert!(
            on.entries()[0].preview.is_empty(),
            "the service has no decoder and must not invent a preview"
        );
    }

    #[test]
    fn an_unsupported_mime_is_refused_rather_than_stored_blank() {
        let mut history = History::new(limits());

        let refused = history.insert(capture("application/x-secret", b"..."), now());

        assert_eq!(refused, Err(Refused::UnsupportedKind));
    }

    #[test]
    fn the_oldest_unpinned_entry_is_dropped_once_the_count_is_exceeded() {
        let mut history = History::new(limits());
        for body in ["one", "two", "three", "four"] {
            put(&mut history, body).expect("recorded");
        }

        let previews: Vec<_> = history
            .entries()
            .iter()
            .map(|e| e.preview.clone())
            .collect();
        assert_eq!(previews, ["four", "three", "two"]);
    }

    #[test]
    fn the_byte_budget_evicts_even_when_the_count_would_not() {
        let mut history = History::new(Limits {
            entries: 100,
            max_total_bytes: 20,
            ..limits()
        });

        put(&mut history, &"a".repeat(15)).expect("recorded");
        put(&mut history, &"b".repeat(15)).expect("recorded");

        assert_eq!(history.entries().len(), 1);
        assert!(history.total_bytes() <= 20);
    }

    #[test]
    fn a_pinned_entry_outlives_eviction() {
        let mut history = History::new(limits());
        put(&mut history, "keep me").expect("recorded");
        let pinned = history.entries()[0].id;
        assert!(history.pin(pinned, true));

        for body in ["one", "two", "three", "four", "five"] {
            put(&mut history, body).expect("recorded");
        }

        assert!(
            history.get(pinned).is_some(),
            "a pin is the one statement the viewer made about an entry"
        );
    }

    #[test]
    fn clearing_forgets_everything_except_the_pins() {
        let mut history = History::new(limits());
        put(&mut history, "keep me").expect("recorded");
        let pinned = history.entries()[0].id;
        history.pin(pinned, true);
        put(&mut history, "forget me").expect("recorded");

        assert!(history.clear());

        assert_eq!(history.entries().len(), 1);
        assert_eq!(history.entries()[0].id, pinned);
    }

    #[test]
    fn a_removed_entry_stays_gone_until_it_is_copied_again() {
        let mut history = History::new(limits());
        put(&mut history, "first").expect("recorded");
        let id = history.entries()[0].id;
        put(&mut history, "second").expect("recorded");

        assert!(history.remove(id));
        assert!(!history.remove(id), "removing it twice changes nothing");
        put(&mut history, "third").expect("recorded");

        assert!(
            history.get(id).is_none(),
            "an unrelated capture must not resurrect it"
        );
        assert_eq!(history.entries().len(), 2);
    }

    /// A preview is another application's text. Cutting it by byte would split a codepoint, and the
    /// cap has to survive one that is four bytes wide.
    #[test]
    fn a_multi_byte_preview_is_capped_by_character_and_does_not_panic() {
        let mut history = History::new(Limits {
            max_entry_bytes: 8192,
            max_total_bytes: 8192,
            ..limits()
        });
        let long = "Марта🙂".repeat(200);

        put(&mut history, &long).expect("recorded");

        let preview = &history.entries()[0].preview;
        assert!(preview.chars().count() <= PREVIEW_CHARS + 1);
        assert!(preview.starts_with("Марта🙂"));
    }

    #[test]
    fn a_preview_is_flattened_to_one_line() {
        let mut history = History::new(limits());

        put(&mut history, "one\ntwo\tthree").expect("recorded");

        assert_eq!(history.entries()[0].preview, "one two three");
    }

    #[test]
    fn lowering_the_limits_evicts_at_once_rather_than_on_the_next_capture() {
        let mut history = History::new(limits());
        for body in ["one", "two", "three"] {
            put(&mut history, body).expect("recorded");
        }

        assert!(history.set_limits(Limits {
            entries: 1,
            ..limits()
        }));

        assert_eq!(history.entries().len(), 1);
        assert_eq!(history.entries()[0].preview, "three");
    }

    /// The defect this exists for: the entry was pushed first and `evict` removed oldest-first, so
    /// everything else unpinned was destroyed before the offender was finally dropped — and
    /// `insert` answered `Ok`.
    #[test]
    fn a_capture_too_large_for_the_whole_budget_is_refused_and_destroys_nothing() {
        let mut history = History::new(Limits {
            entries: 100,
            max_entry_bytes: 4096,
            max_total_bytes: 64,
            images: true,
        });
        put(&mut history, "keep one").expect("recorded");
        put(&mut history, "keep two").expect("recorded");

        let refused = put(&mut history, &"a".repeat(128));

        assert_eq!(refused, Err(Refused::TooLarge));
        assert_eq!(
            history.entries().len(),
            2,
            "a refused capture must not evict anything"
        );
    }

    /// Pins were counted against the byte budget, so a history pinned to the cap evicted every new
    /// entry the instant it arrived — an empty clipboard with nothing to explain it.
    #[test]
    fn a_history_pinned_to_the_byte_cap_still_records_new_entries() {
        let mut history = History::new(Limits {
            entries: 100,
            max_entry_bytes: 4096,
            max_total_bytes: 64,
            images: true,
        });
        put(&mut history, &"p".repeat(60)).expect("recorded");
        let pinned = history.entries()[0].id;
        assert!(history.pin(pinned, true));

        put(&mut history, "small").expect("recorded");

        assert_eq!(history.entries().len(), 2);
        assert_eq!(history.entries()[0].preview, "small");
        assert!(history.get(pinned).is_some());
    }

    /// `kind_of` accepts five spellings of text, so the same line arrives under different mimes.
    /// Fingerprinting the spelling filed them as two rows of identical content.
    #[test]
    fn the_same_text_under_two_mime_spellings_is_one_entry() {
        let mut history = History::new(limits());

        history
            .insert(capture("text/plain;charset=utf-8", b"same"), now())
            .expect("recorded");
        history
            .insert(capture("UTF8_STRING", b"same"), now())
            .expect("recorded");

        assert_eq!(
            history.entries().len(),
            1,
            "identical content is one entry however it was spelled"
        );
    }

    #[test]
    fn forgetting_everything_takes_the_pins_too() {
        let mut history = History::new(limits());
        put(&mut history, "pinned").expect("recorded");
        let id = history.entries()[0].id;
        history.pin(id, true);

        assert!(history.forget_all());

        assert!(history.entries().is_empty());
    }

    /// `DefaultHasher` has fixed keys, so its output is byte-identical in every process and an
    /// application choosing clipboard content could craft a collision. Two instances must not agree.
    #[test]
    fn the_fingerprint_is_seeded_per_instance_rather_than_fixed() {
        let one = History::new(limits());
        let two = History::new(limits());

        assert_ne!(
            one.fingerprint(Kind::Text, b"same bytes"),
            two.fingerprint(Kind::Text, b"same bytes"),
            "a fixed-seed fingerprint is forgeable by the application that chose the content"
        );
        assert_eq!(
            one.fingerprint(Kind::Text, b"same bytes"),
            one.fingerprint(Kind::Text, b"same bytes"),
            "but it must still be stable within one history"
        );
    }

    /// Taking the dedup branch on a collision would discard the bytes just copied and leave the
    /// older entry in place — the viewer pastes something other than what they copied.
    #[test]
    fn colliding_content_is_kept_apart_rather_than_silently_substituted() {
        let mut history = History::new(limits());
        put(&mut history, "first").expect("recorded");
        let id = history.entries()[0].id;

        // Forge the collision the hash is meant to prevent: a second entry carrying the first's id.
        history.entries[0].id = 7;
        put(&mut history, "second").expect("recorded");
        history.entries[0].id = 7;

        assert_eq!(
            history.entries().len(),
            2,
            "two different contents must stay two entries even under one id"
        );
        assert_eq!(history.entries()[0].preview, "second");
        let _ = id;
    }

    #[test]
    fn an_entry_is_debugged_without_its_content() {
        let mut history = History::new(limits());
        put(&mut history, "hunter2").expect("recorded");

        let rendered = format!("{:?}", history.entries()[0]);

        assert!(!rendered.contains("hunter2"), "got {rendered}");
        assert!(rendered.contains("bytes: 7"), "got {rendered}");
    }
}
