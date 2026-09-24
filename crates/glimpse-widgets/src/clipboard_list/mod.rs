mod imp;

use gtk4::{gdk, glib, prelude::*, subclass::prelude::*};

pub use imp::Actions;

use crate::reconcile::by_key;
use crate::{Expandable, Fact, FactList, Row, SplitRow, Swatch, none_if_empty};

const TILE_HEIGHT: i32 = 200;
const NARROW: i32 = 1;
const EXCERPT_LINES: i32 = 4;
const PIN: &str = "clipboard-list__pin";

glib::wrapper! {
    pub struct ClipboardList(ObjectSubclass<imp::ClipboardList>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct Clip {
    pub id: u64,
    pub title: String,
    pub subtitle: String,
    pub icon: String,
    pub swatch: Option<gdk::RGBA>,
    pub image: Option<gdk::Texture>,
    pub excerpt: String,
    pub actions: Vec<ClipAction>,
    pub facts: Vec<Fact>,
    pub pinned: bool,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ClipAction {
    pub key: String,
    pub title: String,
    pub value: String,
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
            |clip| self.build(clip),
            |holder, clip| self.apply(holder, clip),
        );
    }

    fn build(&self, clip: &Clip) -> Expandable {
        let id = clip.id;
        match &clip.image {
            Some(texture) => self.tile(id, texture),
            None => {
                let split = SplitRow::new();
                split.set_detail_tooltip(Some(gettextrs::gettext("Details")));
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
        }
    }

    fn tile(&self, id: u64, texture: &gdk::Texture) -> Expandable {
        let picture = gtk4::Picture::for_paintable(texture);
        picture.set_content_fit(gtk4::ContentFit::Cover);
        picture.set_can_shrink(true);
        picture.set_accessible_role(gtk4::AccessibleRole::Presentation);
        let clamp = adw::Clamp::builder()
            .orientation(gtk4::Orientation::Vertical)
            .maximum_size(TILE_HEIGHT)
            .tightening_threshold(TILE_HEIGHT)
            .child(&picture)
            .build();

        let body = gtk4::Button::builder()
            .child(&clamp)
            .css_classes(["clip-tile__body"])
            .build();
        body.connect_clicked(glib::clone!(
            #[weak(rename_to = list)]
            self,
            move |_| list.emit_by_name::<()>("restored", &[&id])
        ));

        let badge = gtk4::Button::builder()
            .icon_name("go-next-symbolic")
            .tooltip_text(gettextrs::gettext("Details"))
            .halign(gtk4::Align::End)
            .valign(gtk4::Align::End)
            .css_classes(["clip-tile__badge", crate::expandable::OPENER])
            .build();
        badge.connect_clicked(glib::clone!(
            #[weak(rename_to = list)]
            self,
            move |_| list.fill(id)
        ));

        let tile = gtk4::Overlay::builder()
            .child(&body)
            .overflow(gtk4::Overflow::Hidden)
            .css_classes(["clip-tile"])
            .build();
        tile.add_overlay(&badge);
        Expandable::new(&tile)
    }

    fn apply(&self, holder: &Expandable, clip: &Clip) {
        if let Some(split) = holder.head::<SplitRow>() {
            let row = split.row();
            row.set_title(none_if_empty(&clip.title));
            row.set_subtitle(none_if_empty(&clip.subtitle));
            match clip.swatch {
                Some(color) => {
                    let swatch = match row.lead().and_downcast::<Swatch>() {
                        Some(swatch) => swatch,
                        None => {
                            let swatch = Swatch::default();
                            swatch.set_valign(gtk4::Align::Center);
                            row.set_lead(&swatch);
                            swatch
                        }
                    };
                    swatch.set_color(Some(color));
                }
                None => {
                    if row.lead().and_downcast::<Swatch>().is_some() {
                        row.clear_lead();
                    }
                    row.set_lead_icon(none_if_empty(&clip.icon));
                }
            }
        }
        if let Some(tile) = holder.head::<gtk4::Overlay>()
            && let Some(body) = tile.child()
        {
            body.update_property(&[gtk4::accessible::Property::Label(&clip.title)]);
        }
        if let Some(pin) = holder
            .details::<gtk4::Box>()
            .and_then(|card| pin_row(&card))
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
        let clip = self
            .imp()
            .clips
            .borrow()
            .iter()
            .find(|clip| clip.id == id)
            .cloned();
        if let (Some(holder), Some(clip)) = (holder, clip)
            && holder.details::<gtk4::Widget>().is_none()
        {
            holder.set_details(Some(&self.card(&clip)));
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

    fn card(&self, clip: &Clip) -> gtk4::Box {
        let id = clip.id;
        let card = gtk4::Box::new(gtk4::Orientation::Vertical, 0);

        if let Some(color) = clip.swatch {
            let strip = Swatch::default();
            strip.set_color(Some(color));
            strip.add_css_class("clip-card__swatch");
            card.append(&strip);
        }
        if !clip.excerpt.is_empty() {
            let excerpt = gtk4::Label::builder()
                .label(&clip.excerpt)
                .wrap(true)
                .wrap_mode(gtk4::pango::WrapMode::WordChar)
                .lines(EXCERPT_LINES)
                .ellipsize(gtk4::pango::EllipsizeMode::End)
                .max_width_chars(NARROW)
                .hexpand(true)
                .xalign(0.0)
                .selectable(true)
                .css_classes(["clip-card__excerpt"])
                .build();
            card.append(&excerpt);
        }
        for action in &clip.actions {
            let row = Row::new();
            row.set_title(Some(action.title.as_str()));
            row.set_value(none_if_empty(&action.value));
            let key = action.key.clone();
            row.connect_clicked(glib::clone!(
                #[weak(rename_to = list)]
                self,
                move |_| list.emit_by_name::<()>("acted", &[&id, &key])
            ));
            card.append(&row);
        }

        let pin = Row::new();
        pin.add_css_class(PIN);
        pin.set_title(Some(self.pin_label(clip.pinned).as_str()));
        pin.connect_clicked(glib::clone!(
            #[weak(rename_to = list)]
            self,
            move |_| list.emit_by_name::<()>("pinned", &[&id, &!list.pinned(id)])
        ));
        card.append(&pin);

        let remove = Row::new();
        remove.set_title(Some(self.imp().actions.borrow().forget.as_str()));
        remove.add_css_class(crate::DESTRUCTIVE);
        remove.connect_clicked(glib::clone!(
            #[weak(rename_to = list)]
            self,
            move |_| list.emit_by_name::<()>("removed", &[&id])
        ));
        card.append(&remove);

        if !clip.facts.is_empty() {
            let facts = FactList::new();
            facts.set_facts(&clip.facts);
            card.append(&facts);
        }
        card
    }

    pub fn connect_acted<F: Fn(&Self, u64, String) + 'static>(
        &self,
        f: F,
    ) -> glib::SignalHandlerId {
        self.connect_closure(
            "acted",
            false,
            glib::closure_local!(move |list: Self, id: u64, key: String| f(&list, id, key)),
        )
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

fn pin_row(card: &gtk4::Box) -> Option<Row> {
    let mut child = card.first_child();
    while let Some(widget) = child {
        if widget.has_css_class(PIN) {
            return widget.downcast().ok();
        }
        child = widget.next_sibling();
    }
    None
}
