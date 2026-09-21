mod imp;

use gtk4::{gdk, glib, prelude::*, subclass::prelude::*};

pub use imp::Actions;

use crate::{Row, SplitRow, drawer, none_if_empty};

/// Decorative: the row's title already names the entry, and an unlabelled image beside it is
/// announced twice by a screen reader.
fn picture_for(texture: &gdk::Texture) -> gtk4::Picture {
    let picture = gtk4::Picture::for_paintable(texture);
    picture.set_content_fit(gtk4::ContentFit::Cover);
    picture.set_size_request(24, 24);
    picture.set_can_shrink(true);
    picture.set_accessible_role(gtk4::AccessibleRole::Presentation);
    picture
}

glib::wrapper! {
    pub struct ClipboardList(ObjectSubclass<imp::ClipboardList>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

/// One row's worth of clipboard entry, already worded by the applet. The widget owns no formatting
/// and no clock; `subtitle` arrives finished.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Clip {
    pub id: u64,
    pub title: String,
    pub subtitle: String,
    pub icon: String,
    /// A decoded thumbnail for an image entry. `None` renders `icon` instead, which is also what a
    /// picture that would not decode falls back to.
    pub image: Option<gdk::Texture>,
    pub pinned: bool,
}

impl Default for ClipboardList {
    fn default() -> Self {
        Self::new()
    }
}

impl ClipboardList {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn set_clips(&self, clips: &[Clip]) {
        if self.imp().clips.borrow().as_slice() == clips {
            return;
        }
        self.imp().clips.replace(clips.to_vec());
        self.render();
    }

    /// Wording for the two action rows, supplied by the applet: a widget owns structure and no
    /// content. Set before the first detail is opened, since a panel is built on first reveal.
    pub fn set_actions(&self, actions: Actions) {
        if *self.imp().actions.borrow() == actions {
            return;
        }
        self.imp().actions.replace(actions);
    }

    /// Which entry has its actions unfolded, or `None`. Everything else recedes, which is how the
    /// viewer can tell which row the panel belongs to.
    pub fn set_open(&self, open: Option<u64>) {
        if *self.imp().open.borrow() == open {
            return;
        }
        self.imp().open.replace(open);
        self.reveal();
    }

    fn render(&self) {
        let imp = self.imp();
        let clips = imp.clips.borrow();
        let mut holders = imp.holders.borrow_mut();

        for (index, clip) in clips.iter().enumerate() {
            if holders.len() == index {
                let holder = drawer::holder(&self.build_row(index as u32));
                holder.insert_after(self, holders.last());
                holders.push(holder);
            }
            let holder = &holders[index];
            if let Some(split) = drawer::head::<SplitRow>(holder) {
                let row = split.row();
                row.set_title(none_if_empty(&clip.title));
                row.set_subtitle(none_if_empty(&clip.subtitle));
                match &clip.image {
                    // Reused rather than rebuilt: `fill_slot` compares by widget identity, so a
                    // fresh `Picture` replaces the slot on every render even for the same texture.
                    Some(texture) => match row.lead().and_downcast::<gtk4::Picture>() {
                        Some(picture)
                            if picture.paintable().as_ref() == Some(texture.upcast_ref()) => {}
                        _ => row.set_lead(&picture_for(texture)),
                    },
                    None => {
                        row.clear_lead();
                        row.set_lead_icon(none_if_empty(&clip.icon));
                    }
                }
            }
        }

        for holder in holders.split_off(clips.len()) {
            holder.unparent();
        }
        drop(clips);
        drop(holders);
        self.reveal();
    }

    /// Built once per position, capturing the **index** and reading the id back when it fires: a
    /// row outlives the entry that was in it when the list is reconciled.
    fn build_row(&self, index: u32) -> SplitRow {
        let split = SplitRow::new();
        split.set_property("detail-icon", "pan-end-symbolic");

        split.connect_activated(glib::clone!(
            #[weak(rename_to = list)]
            self,
            move |_| list.report("restored", index)
        ));
        split.connect_details(glib::clone!(
            #[weak(rename_to = list)]
            self,
            move |_| list.report("detailed", index)
        ));
        split
    }

    fn panel(&self, id: u64, pinned: bool) -> gtk4::Box {
        let panel = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        panel.add_css_class("detail-card");

        let actions = self.imp().actions.borrow().clone();
        let pin = Row::new();
        pin.set_lead_icon(Some("view-pin-symbolic"));
        pin.set_title(Some(match pinned {
            true => actions.unpin.as_str(),
            false => actions.pin.as_str(),
        }));
        pin.connect_clicked(glib::clone!(
            #[weak(rename_to = list)]
            self,
            move |_| list.emit_by_name::<()>("pinned", &[&id, &!pinned])
        ));

        let remove = Row::new();
        remove.set_lead_icon(Some("user-trash-symbolic"));
        remove.set_title(Some(actions.forget.as_str()));
        remove.connect_clicked(glib::clone!(
            #[weak(rename_to = list)]
            self,
            move |_| list.emit_by_name::<()>("removed", &[&id])
        ));

        panel.append(&pin);
        panel.append(&remove);
        panel
    }

    /// Derived from what is revealed rather than from remembered state, so two lists cannot
    /// disagree about which row is open.
    fn reveal(&self) {
        let imp = self.imp();
        let open = *imp.open.borrow();
        let clips = imp.clips.borrow();
        let holders = imp.holders.borrow();

        for (holder, clip) in holders.iter().zip(clips.iter()) {
            let Some(panel) = drawer::panel(holder) else {
                continue;
            };
            let wanted = open == Some(clip.id);
            // Rebuilt only when the entry or its pinned state changes. The wording depends on
            // `pinned`, so a cached panel could say "Pin" over something already pinned — but
            // rebuilding on every publish unparents the row under a press in flight, and `clicked`
            // is then never emitted.
            let stamp = (clip.id, clip.pinned);
            match wanted {
                true if *imp.built.borrow() != Some(stamp) => {
                    panel.set_child(Some(&self.panel(clip.id, clip.pinned)));
                    imp.built.replace(Some(stamp));
                }
                true => {}
                false => {
                    if *imp.built.borrow() == Some(stamp) {
                        imp.built.replace(None);
                    }
                    panel.set_child(gtk4::Widget::NONE);
                }
            }
            drawer::set(&panel, wanted);
            if let Some(split) = drawer::head::<SplitRow>(holder) {
                crate::set_css_class(&split, drawer::RECEDED, open.is_some() && !wanted);
                crate::set_css_class(&split, drawer::OPEN, wanted);
            }
        }
    }

    fn report(&self, signal: &str, index: u32) {
        let id = self
            .imp()
            .clips
            .borrow()
            .get(index as usize)
            .map(|clip| clip.id);
        if let Some(id) = id {
            self.emit_by_name::<()>(signal, &[&id]);
        }
    }

    pub fn connect_restored<F: Fn(&Self, u64) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            "restored",
            false,
            glib::closure_local!(move |list: Self, id: u64| f(&list, id)),
        )
    }

    pub fn connect_detailed<F: Fn(&Self, u64) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            "detailed",
            false,
            glib::closure_local!(move |list: Self, id: u64| f(&list, id)),
        )
    }

    pub fn connect_pinned<F: Fn(&Self, u64, bool) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            "pinned",
            false,
            glib::closure_local!(move |list: Self, id: u64, pinned: bool| f(&list, id, pinned)),
        )
    }

    pub fn connect_removed<F: Fn(&Self, u64) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            "removed",
            false,
            glib::closure_local!(move |list: Self, id: u64| f(&list, id)),
        )
    }
}
