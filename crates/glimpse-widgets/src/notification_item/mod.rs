mod imp;

use gtk4::{gdk, glib, prelude::*, subclass::prelude::*};

pub use imp::Urgency;

use crate::truncate;

pub(crate) const BODY_MAX_CHARS: usize = 512;

/// A `Gtk.Picture` asks for its paintable's own height, so an unbounded image makes the
/// notification as tall as whoever sent it decided. Measured: a 400x1200 image asked for 1256px of
/// card. The image is scaled down to this before it is ever handed over, and the bound is a
/// fraction of the card rather than a comfortable thumbnail size — the picture is content somebody
/// else chose, and it should not be the loudest thing on a surface the reader did not open.
const IMAGE_MAX_HEIGHT: i32 = 112;
const LABEL_MAX_CHARS: usize = 32;

/// GNOME's HIG and KDE's notification service both stop at three. A sender offering more is not
/// refused, it is trimmed — the alternative is a row that scrolls or a card that grows sideways.
const ACTIONS_MAX: usize = 3;

const ACTIVATED: &str = "activated";
const DISMISSED: &str = "dismissed";
const ACTION_INVOKED: &str = "action-invoked";
const AVATAR: &str = "notification--avatar";

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

    pub fn set_image(&self, image: Option<&gdk::Texture>) {
        let imp = self.imp();
        let bounded = image.map(bound);
        let paintable = bounded.as_ref().map(|texture| texture.upcast_ref());
        if imp.picture.paintable().as_ref() == paintable {
            return;
        }
        imp.picture.set_paintable(paintable);
        imp.picture.set_visible(paintable.is_some());
    }

    pub fn set_actions(&self, actions: &[Action]) {
        let imp = self.imp();
        let actions = &actions[..actions.len().min(ACTIONS_MAX)];
        let keys: Vec<String> = actions.iter().map(|action| action.key.clone()).collect();
        if *imp.keys.borrow() == keys {
            return;
        }
        imp.keys.replace(keys);

        while let Some(child) = imp.actions.first_child() {
            imp.actions.remove(&child);
        }
        for (index, action) in actions.iter().enumerate() {
            imp.actions.append(&self.build_action(action, index == 0));
        }
        imp.actions.set_visible(!actions.is_empty());
    }

    /// The freedesktop specification gives actions no priority, so the first one a sender lists is
    /// taken as the primary and is the only one that carries the accent.
    fn build_action(&self, action: &Action, primary: bool) -> gtk4::Button {
        let button = gtk4::Button::with_label(&truncate(&action.label, LABEL_MAX_CHARS));
        button.add_css_class("notification__action");
        if primary {
            button.add_css_class("notification__action--primary");
        }
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

/// Scaled rather than cropped: a screenshot cropped to a strip says less than the same screenshot
/// made smaller, and the aspect ratio is the one thing a sender's image is entitled to keep.
///
/// The pixels are averaged here rather than handed to `gdk-pixbuf`, whose two entry points for this
/// — `pixbuf_get_from_texture` and `Texture::for_pixbuf` — are both deprecated, and `just lint`
/// runs with `-D warnings`. `Texture::download` writes `B8g8r8a8Premultiplied`, and the result is
/// built back in the same format, so nothing is swizzled on the way through.
fn bound(texture: &gdk::Texture) -> gdk::Texture {
    let (width, height) = (texture.width(), texture.height());
    if height <= IMAGE_MAX_HEIGHT || width <= 0 {
        return texture.clone();
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

    gdk::MemoryTexture::new(
        target_width,
        IMAGE_MAX_HEIGHT,
        gdk::MemoryFormat::B8g8r8a8Premultiplied,
        &glib::Bytes::from_owned(target),
        target_stride,
    )
    .upcast()
}

/// The half-open source range one target pixel averages over, never empty.
fn span(at: usize, out_of: usize, source: usize) -> (usize, usize) {
    let from = at * source / out_of;
    let to = ((at + 1) * source / out_of).max(from + 1).min(source);
    (from, to)
}

fn icons_equal(current: Option<&gio::Icon>, next: Option<&gio::Icon>) -> bool {
    match (current, next) {
        (None, None) => true,
        (Some(current), Some(next)) => current.equal(Some(next)),
        _ => false,
    }
}
