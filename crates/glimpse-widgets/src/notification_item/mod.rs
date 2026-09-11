mod imp;

use gtk4::{gdk, glib, prelude::*, subclass::prelude::*};

pub use imp::Urgency;

use crate::{icons_equal, truncate};

pub(crate) const BODY_MAX_CHARS: usize = 512;

/// A `Gtk.Picture` asks for its paintable's own height, so an unbounded image makes the
/// notification as tall as whoever sent it decided. Measured: a 400x1200 image asked for 1256px of
/// card. The image is scaled down to this before it is ever handed over, and the bound is a
/// fraction of the card rather than a comfortable thumbnail size — the picture is content somebody
/// else chose, and it should not be the loudest thing on a surface the reader did not open.
const IMAGE_MAX_HEIGHT: i32 = 112;

/// The largest source this will resample, per side. `Texture::download` allocates a second copy of
/// the whole image, and the image is chosen by whoever sent the notification — 4096 is the same
/// ceiling `artwork` applies, and past it the picture is dropped rather than shown unbounded.
const IMAGE_LARGEST: i32 = 4096;
const LABEL_MAX_CHARS: usize = 32;

/// GNOME's HIG and KDE's notification service both stop at three. A sender offering more is not
/// refused, it is trimmed — the alternative is a row that scrolls or a card that grows sideways.
const ACTIONS_MAX: usize = 3;

/// How far past an `&` a `;` may be and still be a reference. Bounding the search is what stops a
/// stray ampersand in a long body from being scanned to the end of it once per character.
const REFERENCE_MAX_BYTES: usize = 12;

const ACTIVATED: &str = "activated";
const DISMISSED: &str = "dismissed";
const ACTION_INVOKED: &str = "action-invoked";
const AVATAR: &str = "notification--avatar";
const ACTIVATABLE: &str = "notification--activatable";

/// `key` is what the widget reports when the action fires. Buttons are rebuilt whenever the set
/// changes, so a position says nothing durable about which action it stands for.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Action {
    pub key: String,
    pub label: String,
}

glib::wrapper! {
    pub struct NotificationItem(ObjectSubclass<imp::NotificationItem>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Default for NotificationItem {
    fn default() -> Self {
        Self::new()
    }
}

impl NotificationItem {
    pub fn new() -> Self {
        glib::Object::new()
    }

    /// A `gdk::Texture` implements `gio::Icon`, which is what lets one setter cover a themed name,
    /// a desktop entry's icon and a sender's own avatar alike.
    ///
    /// The shape follows from which one arrived. A circle is the universal avatar, and every chat
    /// application has trained people to read it as a person — so a themed name, which is an
    /// application saying what it is, gets a rounded square, and pixels, which is almost always
    /// somebody's photo, keep the circle. Nothing has to be declared for that to be right.
    pub fn set_app_icon(&self, icon: Option<&gio::Icon>) {
        let imp = self.imp();
        if icons_equal(imp.gicon.borrow().as_ref(), icon) {
            return;
        }
        imp.gicon.replace(icon.cloned());
        match icon {
            Some(icon) => imp.app_icon.set_from_gicon(icon),
            None => imp.app_icon.clear(),
        }
        imp.app_icon.set_visible(icon.is_some());

        let photograph = icon.is_some_and(|icon| !icon.is::<gio::ThemedIcon>());
        crate::set_css_class(self, AVATAR, photograph);
    }

    /// Compared on the source rather than on the result: `bound` builds a new texture every time
    /// it resamples, so comparing what comes out of it never matches and would resample the same
    /// image on every call.
    pub fn set_image(&self, image: Option<&gdk::Texture>) {
        let imp = self.imp();
        if imp.image.borrow().as_ref() == image {
            return;
        }
        imp.image.replace(image.cloned());

        let bounded = image.and_then(bound);
        let paintable = bounded
            .as_ref()
            .map(|texture| texture.upcast_ref::<gdk::Paintable>());
        imp.picture.set_paintable(paintable);
        imp.picture.set_visible(paintable.is_some());
    }

    pub fn set_actions(&self, actions: &[Action]) {
        let imp = self.imp();
        let actions = &actions[..actions.len().min(ACTIONS_MAX)];
        if *imp.shown.borrow() == actions {
            return;
        }
        imp.shown.replace(actions.to_vec());

        crate::clear_children(&imp.actions);
        for action in actions {
            imp.actions.append(&self.build_action(action));
        }
        imp.actions.set_visible(!actions.is_empty());
    }

    pub fn set_activatable(&self, activatable: bool) {
        let activate = &self.imp().activate;
        if activate.can_target() != activatable {
            activate.set_can_target(activatable);
        }
        if activate.is_focusable() != activatable {
            activate.set_focusable(activatable);
        }
        crate::set_css_class(self, ACTIVATABLE, activatable);
    }

    pub(crate) fn set_controls_visible(&self, visible: bool) {
        let imp = self.imp();
        imp.actions
            .set_visible(visible && !imp.shown.borrow().is_empty());
        imp.close.set_visible(visible);
    }

    fn build_action(&self, action: &Action) -> gtk4::Button {
        let button = gtk4::Button::with_label(&truncate(&action.label, LABEL_MAX_CHARS));
        button.add_css_class("flat");
        button.add_css_class("notification__action");
        button.connect_clicked(glib::clone!(
            #[weak(rename_to = item)]
            self,
            #[strong(rename_to = key)]
            action.key,
            move |_| item.emit_by_name::<()>(ACTION_INVOKED, &[&key])
        ));
        button
    }

    pub fn connect_activated<F: Fn(&Self) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            ACTIVATED,
            false,
            glib::closure_local!(move |item: &Self| f(item)),
        )
    }

    pub fn connect_dismissed<F: Fn(&Self) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            DISMISSED,
            false,
            glib::closure_local!(move |item: &Self| f(item)),
        )
    }

    pub fn connect_action_invoked<F: Fn(&Self, String) + 'static>(
        &self,
        f: F,
    ) -> glib::SignalHandlerId {
        self.connect_closure(
            ACTION_INVOKED,
            false,
            glib::closure_local!(move |item: &Self, key: String| f(item, key)),
        )
    }
}

/// What a body Pango refused reads as. Handing the markup itself to `set_text` shows the reader
/// tag soup — `<b>Alice</b> &nbsp; <a href="…">…</a>` — which looks exactly like a broken
/// application. This is the text with the markup taken out instead. Nothing here is interpreted:
/// the result goes to `set_text`, so stripping is for legibility rather than for safety.
pub(crate) fn plain(markup: &str) -> String {
    let mut stripped = String::with_capacity(markup.len());
    let mut rest = markup;
    while let Some(open) = rest.find('<') {
        stripped.push_str(&rest[..open]);
        rest = match rest[open..].find('>') {
            Some(close) => &rest[open + close + 1..],
            None => "",
        };
    }
    stripped.push_str(rest);

    unescape(&stripped)
}

/// The five XML entities Pango knows, plus `&nbsp;`, which it does not and which is the one named
/// entity `ammonia` emits. Anything unrecognised is left exactly as written: a reader seeing
/// `&whoops;` is better served than one seeing it silently swallowed.
fn unescape(text: &str) -> String {
    if !text.contains('&') {
        return text.to_owned();
    }

    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        rest = &rest[at..];

        let end = rest.find(';').filter(|end| *end <= REFERENCE_MAX_BYTES);
        match end.and_then(|end| reference(&rest[1..end]).map(|character| (character, end))) {
            Some((character, end)) => {
                out.push(character);
                rest = &rest[end + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

fn reference(name: &str) -> Option<char> {
    if let Some(hex) = name.strip_prefix("#x").or_else(|| name.strip_prefix("#X")) {
        return u32::from_str_radix(hex, 16).ok().and_then(char::from_u32);
    }
    if let Some(decimal) = name.strip_prefix('#') {
        return decimal.parse::<u32>().ok().and_then(char::from_u32);
    }
    match name {
        "amp" => Some('&'),
        "lt" => Some('<'),
        "gt" => Some('>'),
        "quot" => Some('"'),
        "apos" => Some('\''),
        "nbsp" => Some('\u{a0}'),
        _ => None,
    }
}

/// Scaled rather than cropped: a screenshot cropped to a strip says less than the same screenshot
/// made smaller, and the aspect ratio is the one thing a sender's image is entitled to keep.
///
/// The pixels are averaged here rather than handed to `gdk-pixbuf`, whose two entry points for this
/// — `pixbuf_get_from_texture` and `Texture::for_pixbuf` — are both deprecated, and `just lint`
/// runs with `-D warnings`. `Texture::download` writes `B8g8r8a8Premultiplied`, and the result is
/// built back in the same format, so nothing is swizzled on the way through.
fn bound(texture: &gdk::Texture) -> Option<gdk::Texture> {
    let (width, height) = (texture.width(), texture.height());
    if width <= 0 || height <= 0 {
        return None;
    }
    if width > IMAGE_LARGEST || height > IMAGE_LARGEST {
        tracing::debug!(
            width,
            height,
            "a notification image is larger than this will resample"
        );
        return None;
    }
    if height <= IMAGE_MAX_HEIGHT {
        return Some(texture.clone());
    }

    let factor = f64::from(IMAGE_MAX_HEIGHT) / f64::from(height);
    let target_width = ((f64::from(width) * factor).round() as i32).max(1);
    let (source_stride, target_stride) = (width as usize * 4, target_width as usize * 4);

    let mut source = vec![0u8; source_stride * height as usize];
    texture.download(&mut source, source_stride);

    let mut target = vec![0u8; target_stride * IMAGE_MAX_HEIGHT as usize];
    for row in 0..IMAGE_MAX_HEIGHT as usize {
        let (from_y, to_y) = span(row, IMAGE_MAX_HEIGHT as usize, height as usize);
        for column in 0..target_width as usize {
            let (from_x, to_x) = span(column, target_width as usize, width as usize);
            let mut totals = [0u32; 4];
            let mut counted = 0u32;
            for y in from_y..to_y {
                for x in from_x..to_x {
                    let at = y * source_stride + x * 4;
                    for (channel, total) in totals.iter_mut().enumerate() {
                        *total += u32::from(source[at + channel]);
                    }
                    counted += 1;
                }
            }
            let at = row * target_stride + column * 4;
            for (channel, total) in totals.iter().enumerate() {
                target[at + channel] = (total / counted.max(1)) as u8;
            }
        }
    }

    Some(
        gdk::MemoryTexture::new(
            target_width,
            IMAGE_MAX_HEIGHT,
            gdk::MemoryFormat::B8g8r8a8Premultiplied,
            &glib::Bytes::from_owned(target),
            target_stride,
        )
        .upcast(),
    )
}

/// The half-open source range one target pixel averages over, never empty.
fn span(at: usize, out_of: usize, source: usize) -> (usize, usize) {
    let from = at * source / out_of;
    let to = ((at + 1) * source / out_of).max(from + 1).min(source);
    (from, to)
}

#[cfg(test)]
mod tests {
    use super::plain;

    #[test]
    fn a_refused_body_reads_as_its_text_rather_than_as_its_tags() {
        assert_eq!(
            plain(r#"<b>Alice</b>&nbsp;<a href="https://x">said hello</a>"#),
            "Alice\u{a0}said hello"
        );
    }

    #[test]
    fn markup_is_removed_and_the_text_between_it_is_kept() {
        assert_eq!(plain("<b><i>both</i></b>"), "both");
        assert_eq!(plain("no markup at all"), "no markup at all");
        assert_eq!(plain(""), "");
    }

    /// A body that fails to parse is exactly the kind that arrives unbalanced, so the stripper
    /// cannot assume a closing bracket is there.
    #[test]
    fn an_unclosed_tag_takes_the_rest_of_the_string_with_it() {
        assert_eq!(plain("before <b>after"), "before after");
        assert_eq!(plain("before <never closed"), "before ");
    }

    #[test]
    fn the_five_xml_entities_and_the_one_pango_does_not_know_decode() {
        assert_eq!(
            plain("&amp; &lt; &gt; &quot; &apos; &nbsp;"),
            "& < > \" ' \u{a0}"
        );
    }

    #[test]
    fn numeric_references_decode_in_both_bases_and_beyond_the_basic_plane() {
        assert_eq!(
            plain("&#9733; &#x2605; &#127881;"),
            "\u{2605} \u{2605} \u{1f389}"
        );
    }

    /// Better that a reader sees `&whoops;` than that it vanishes: the text is somebody else's and
    /// silently dropping part of it is worse than showing it as written.
    #[test]
    fn anything_unrecognised_is_left_exactly_as_written() {
        assert_eq!(
            plain("&whoops; &#999999999; AT&T"),
            "&whoops; &#999999999; AT&T"
        );
        assert_eq!(plain("a & b"), "a & b");
    }

    /// A body is somebody else's text. Bounding the search for `;` by bytes put the slice inside a
    /// character whenever a multi-byte one straddled the window — `&` followed by six `é` was a
    /// panic in the panel, from a message anybody could send.
    #[test]
    fn a_reference_window_never_lands_inside_a_character() {
        assert_eq!(
            plain(&format!("&{}", "é".repeat(6))),
            format!("&{}", "é".repeat(6))
        );
        assert_eq!(plain("&日本語テキストです;"), "&日本語テキストです;");
        assert_eq!(plain("&amp;é"), "&é");
    }

    #[test]
    fn a_reference_that_never_closes_is_not_scanned_to_the_end_of_the_body() {
        let long = format!("&{}", "x".repeat(4096));

        assert_eq!(plain(&long), long);
    }
}
