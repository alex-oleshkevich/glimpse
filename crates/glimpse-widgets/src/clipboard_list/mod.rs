mod imp;

use gtk4::{gdk, glib, prelude::*, subclass::prelude::*};

pub use imp::Actions;

use crate::reconcile::by_key;
use crate::{Expandable, Row, SplitRow, none_if_empty};

/// Decorative: the row's title already names the entry, and an unlabelled image beside it is
/// announced twice by a screen reader.
fn picture_for(texture: &gdk::Texture) -> gtk4::Picture {
    let picture = gtk4::Picture::for_paintable(texture);
    picture.set_content_fit(gtk4::ContentFit::Cover);
    picture.set_can_shrink(true);
    picture.set_accessible_role(gtk4::AccessibleRole::Presentation);
    picture
}

glib::wrapper! {
    pub struct ClipboardList(ObjectSubclass<imp::ClipboardList>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

/// One row's worth of clipboard entry, already worded by the applet. The widget owns no
/// formatting; `title` arrives finished.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Clip {
    pub id: u64,
    pub title: String,
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

    fn render(&self) {
        let imp = self.imp();
        let clips = imp.clips.borrow();
        by_key(
            self,
            &mut imp.holders.borrow_mut(),
            &clips,
            |clip| clip.id,
            |clip| self.build(clip.id),
            |holder, clip| self.apply(holder, clip),
        );
    }

    fn build(&self, id: u64) -> Expandable {
        let split = SplitRow::new();
        split.set_property("detail-icon", "pan-end-symbolic");
        split.connect_activated(glib::clone!(
            #[weak(rename_to = list)]
            self,
            move |_| list.emit_by_name::<()>("restored", &[&id])
        ));
        split.connect_details(glib::clone!(
            #[weak(rename_to = list)]
            self,
            move |_| list.fill(id)
        ));
        Expandable::new(&split)
    }

    fn apply(&self, holder: &Expandable, clip: &Clip) {
        if let Some(split) = holder.head::<SplitRow>() {
            let row = split.row();
            row.set_title(none_if_empty(&clip.title));
            match &clip.image {
                // Reused rather than rebuilt: `fill_slot` compares by widget identity, so a
                // fresh `Picture` replaces the slot on every render even for the same texture.
                Some(texture) => match row.lead().and_downcast::<gtk4::Picture>() {
                    Some(picture) if picture.paintable().as_ref() == Some(texture.upcast_ref()) => {
                    }
                    _ => row.set_lead(&picture_for(texture)),
                },
                None => {
                    row.clear_lead();
                    row.set_lead_icon(none_if_empty(&clip.icon));
                }
            }
        }
        if let Some(pin) = holder
            .details::<gtk4::Box>()
            .and_then(|panel| panel.first_child())
            .and_downcast::<Row>()
        {
            pin.set_title(Some(self.pin_label(clip.pinned).as_str()));
        }
    }

    fn fill(&self, id: u64) {
        let holder = self
            .imp()
            .holders
            .borrow()
            .iter()
            .find(|(held, _)| *held == id)
            .map(|(_, holder)| holder.clone());
        if let Some(holder) = holder
            && holder.details::<gtk4::Widget>().is_none()
        {
            holder.set_details(Some(&self.panel(id)));
        }
    }

    fn pinned(&self, id: u64) -> bool {
        self.imp()
            .clips
            .borrow()
            .iter()
            .any(|clip| clip.id == id && clip.pinned)
    }

    fn pin_label(&self, pinned: bool) -> String {
        let actions = self.imp().actions.borrow();
        match pinned {
            true => actions.unpin.clone(),
            false => actions.pin.clone(),
        }
    }

    /// Built once and never rebuilt: rebuilding unparents the row under a press in flight and
    /// `clicked` is then never emitted, so the pin row reads the entry's state when it fires and
    /// `apply` only rewrites its wording.
    fn panel(&self, id: u64) -> gtk4::Box {
        let panel = gtk4::Box::new(gtk4::Orientation::Vertical, 0);

        let pin = Row::new();
        pin.set_title(Some(self.pin_label(self.pinned(id)).as_str()));
        pin.connect_clicked(glib::clone!(
            #[weak(rename_to = list)]
            self,
            move |_| list.emit_by_name::<()>("pinned", &[&id, &!list.pinned(id)])
        ));

        let remove = Row::new();
        remove.set_title(Some(self.imp().actions.borrow().forget.as_str()));
        remove.connect_clicked(glib::clone!(
            #[weak(rename_to = list)]
            self,
            move |_| list.emit_by_name::<()>("removed", &[&id])
        ));

        panel.append(&pin);
        panel.append(&remove);
        panel
    }

    pub fn connect_restored<F: Fn(&Self, u64) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            "restored",
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
