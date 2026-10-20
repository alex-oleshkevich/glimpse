mod imp;

use gtk4::{gdk, gio, glib, prelude::*, subclass::prelude::*};

pub use imp::Actions;

use crate::reconcile::by_key;
use crate::{ActionRow, Expandable, ExpanderRow, Fact, Row, RowTone, Swatch, none_if_empty};

const TILE_HEIGHT: i32 = 200;
const NARROW: i32 = 1;
const EXCERPT_LINES: i32 = 4;
const PIN: &str = "clipboard-list__pin";
const CHEVRON: &str = "go-next-symbolic";

glib::wrapper! {
    pub struct ClipboardList(ObjectSubclass<imp::ClipboardList>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct Clip {
    pub id: u64,
    pub title: String,
    pub icon: String,
    pub swatch: Option<gdk::RGBA>,
    pub image: Option<gdk::Texture>,
    pub file: Option<gio::File>,
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
    pub choices: Vec<ClipAction>,
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
            |clip| (clip.id, clip.image.is_some()),
            |clip| self.build(clip),
            |holder, clip| self.apply(holder, clip),
        );
    }

    fn build(&self, clip: &Clip) -> gtk4::Widget {
        let id = clip.id;
        match &clip.image {
            Some(texture) => self.tile(id, texture, clip.file.as_ref()).upcast(),
            None => {
                let row = ExpanderRow::new();
                row.connect_toggled(glib::clone!(
                    #[weak(rename_to = list)]
                    self,
                    move |_, open| {
                        if open {
                            list.fill(id);
                        }
                    }
                ));
                row.upcast()
            }
        }
    }

    fn tile(&self, id: u64, texture: &gdk::Texture, file: Option<&gio::File>) -> Expandable {
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
        if let Some(file) = file {
            body.add_controller(drag_source(texture, file));
        }

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

    fn apply(&self, holder: &gtk4::Widget, clip: &Clip) {
        if let Some(row) = holder.downcast_ref::<ExpanderRow>() {
            row.set_title(none_if_empty(&clip.title));
            match clip.swatch {
                Some(color) => {
                    row.set_icon_name(None::<&str>);
                    let swatch = Swatch::default();
                    swatch.set_valign(gtk4::Align::Center);
                    swatch.set_color(Some(color));
                    row.set_start(&swatch);
                }
                None => {
                    row.clear_start();
                    row.set_icon_name(none_if_empty(&clip.icon));
                }
            }
            if let Some(pin) = pin_row(&row.details()) {
                pin.upcast_ref::<Row>()
                    .set_title(Some(self.pin_label(clip.pinned).as_str()));
            }
        }
        if let Some(holder) = holder.downcast_ref::<Expandable>() {
            if let Some(tile) = holder.head::<gtk4::Overlay>()
                && let Some(body) = tile.child()
            {
                body.update_property(&[gtk4::accessible::Property::Label(&clip.title)]);
            }
            if let Some(pin) = holder
                .details::<gtk4::Box>()
                .and_then(|card| pin_row(&card))
            {
                pin.upcast_ref::<Row>()
                    .set_title(Some(self.pin_label(clip.pinned).as_str()));
            }
        }
    }

    fn fill(&self, id: u64) {
        let holder = self
            .imp()
            .holders
            .borrow()
            .iter()
            .find(|(held, _)| held.0 == id)
            .map(|(_, holder)| holder.clone());
        let clip = self
            .imp()
            .clips
            .borrow()
            .iter()
            .find(|clip| clip.id == id)
            .cloned();
        let (Some(holder), Some(clip)) = (holder, clip) else {
            return;
        };
        if let Some(row) = holder.downcast_ref::<ExpanderRow>() {
            let details = row.details();
            if details.first_child().is_none() {
                self.drawer(&details, &clip, true);
            }
        } else if let Some(holder) = holder.downcast_ref::<Expandable>()
            && holder.details::<gtk4::Widget>().is_none()
        {
            let card = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
            self.drawer(&card, &clip, false);
            holder.set_details(Some(&card));
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

    fn drawer(&self, card: &gtk4::Box, clip: &Clip, restore: bool) {
        let id = clip.id;

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
        if !clip.facts.is_empty() {
            let facts = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
            crate::set_facts(&facts, &clip.facts);
            card.append(&facts);
        }
        if restore {
            let copy = action_row(&gettextrs::gettext("Copy"), None);
            copy.connect_activated(glib::clone!(
                #[weak(rename_to = list)]
                self,
                move |_| list.emit_by_name::<()>("restored", &[&id])
            ));
            card.append(&copy);
        }
        for action in &clip.actions {
            let row = action_row(&action.title, none_if_empty(&action.value));
            let key = action.key.clone();
            match action.choices.is_empty() {
                true => {
                    row.connect_activated(glib::clone!(
                        #[weak(rename_to = list)]
                        self,
                        move |_| list.emit_by_name::<()>("acted", &[&id, &key])
                    ));
                }
                false => {
                    row.upcast_ref::<Row>()
                        .set_end(&gtk4::Image::from_icon_name(CHEVRON));
                    row.connect_activated(glib::clone!(
                        #[weak(rename_to = list)]
                        self,
                        move |row| list.aside(row, id, &key)
                    ));
                }
            }
            card.append(&row);
        }

        let pin = action_row(self.pin_label(clip.pinned).as_str(), None);
        pin.add_css_class(PIN);
        pin.connect_activated(glib::clone!(
            #[weak(rename_to = list)]
            self,
            move |_| list.emit_by_name::<()>("pinned", &[&id, &!list.pinned(id)])
        ));
        card.append(&pin);

        let remove = action_row(self.imp().actions.borrow().forget.as_str(), None);
        remove.upcast_ref::<Row>().set_tone(RowTone::Destructive);
        remove.connect_activated(glib::clone!(
            #[weak(rename_to = list)]
            self,
            move |_| list.emit_by_name::<()>("removed", &[&id])
        ));
        card.append(&remove);
    }

    fn aside(&self, row: &ActionRow, id: u64, key: &str) {
        let choices: Vec<ClipAction> = self
            .imp()
            .clips
            .borrow()
            .iter()
            .find(|clip| clip.id == id)
            .and_then(|clip| clip.actions.iter().find(|action| action.key == key))
            .map(|action| action.choices.clone())
            .unwrap_or_default();
        if choices.is_empty() {
            return;
        }
        let menu = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        let popover = gtk4::Popover::builder()
            .child(&menu)
            .position(gtk4::PositionType::Right)
            .has_arrow(false)
            .build();
        for choice in choices {
            let item = action_row(&choice.title, none_if_empty(&choice.value));
            item.connect_activated(glib::clone!(
                #[weak(rename_to = list)]
                self,
                #[weak]
                popover,
                move |_| {
                    popover.popdown();
                    list.emit_by_name::<()>("acted", &[&id, &choice.key]);
                }
            ));
            menu.append(&item);
        }
        popover.set_parent(row);
        popover.connect_closed(|popover| {
            let popover = popover.clone();
            glib::idle_add_local_once(move || popover.unparent());
        });
        popover.popup();
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

fn drag_source(texture: &gdk::Texture, file: &gio::File) -> gtk4::DragSource {
    let files = gdk::FileList::from_array(std::slice::from_ref(file));
    let source = gtk4::DragSource::builder()
        .actions(gdk::DragAction::COPY)
        .content(&gdk::ContentProvider::for_value(&files.to_value()))
        .build();
    let texture = texture.clone();
    source.connect_drag_begin(move |source, _| source.set_icon(Some(&texture), 0, 0));
    source
}

fn action_row(title: &str, value: Option<&str>) -> ActionRow {
    let row = ActionRow::new();
    let base: &Row = row.upcast_ref();
    base.set_title(Some(title));
    base.set_value(value);
    row
}

fn pin_row(card: &gtk4::Box) -> Option<ActionRow> {
    let mut child = card.first_child();
    while let Some(widget) = child {
        if widget.has_css_class(PIN) {
            return widget.downcast().ok();
        }
        child = widget.next_sibling();
    }
    None
}
