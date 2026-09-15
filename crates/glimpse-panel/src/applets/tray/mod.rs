mod icon;
mod menu;
mod render;

use glimpse_config::{Applet as AppletConfig, AppletKind, TrayAppletConfig};
use glimpse_dbus::dbusmenu::MenuNode;
use glimpse_dbus::status_notifier_item::{TrayItem, TrayStatus};
use glimpse_services::TrayHandle;
use glimpse_widgets::{IndicatorSpec, TrayChip, TrayStrip};
use gtk4::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

/// The slot a chip's icon occupies, before the output's scale is applied. `-gtk-icon-size` in the
/// stylesheet is the same measurement; a mismatch shows up as a blurry pixmap, not as an error.
const ICON_SIZE: i32 = 22;

use crate::applet::popover::PopoverHandle;
use crate::applet::{Applet, Button, Ctx, Input, spawn_command};

pub struct Tray {
    strip: TrayStrip,
    /// The layout a press fetched, waiting for the runtime to ask for a popover. A menu is
    /// fetched before it is shown, because `AboutToShow` is where an application fills one in.
    pending: Rc<RefCell<Option<(String, MenuNode)>>>,
    /// Shared with the theme and scale callbacks, which have no `&mut self` and must re-render
    /// from the same values the applet last saw.
    settings: Rc<RefCell<TrayAppletConfig>>,
    items: Rc<RefCell<Vec<TrayItem>>>,
    tray: TrayHandle,
}

impl Applet for Tray {
    fn view(&mut self, ctx: &Ctx) -> Option<gtk4::Widget> {
        self.strip.connect_activated({
            let tray = self.tray.clone();
            let items = self.items.clone();
            let pending = self.pending.clone();
            let opener = ctx.opener();
            move |strip, key, button| {
                let tray = tray.clone();
                let anchor = strip.anchor();
                let (x, y) = position(anchor.as_ref());
                // An item that says it *is* a menu opens one on the left button too, and one that
                // offers no menu at all opens nothing — not a placeholder, not an empty popover.
                let (offers_menu, is_menu) = items
                    .borrow()
                    .iter()
                    .find(|item| item.key == key)
                    .map(|item| (item.menu.is_some(), item.item_is_menu))
                    .unwrap_or_default();
                let pressed = Button::from_code(button);
                let wants_menu = offers_menu && (pressed == Button::Right || is_menu);

                if wants_menu {
                    let pending = pending.clone();
                    let opener = opener.clone();
                    relm4::spawn_local(async move {
                        match tray.menu(key.clone()).await {
                            Ok(node) => {
                                pending.replace(Some((key, node)));
                                opener.open_popover();
                            }
                            Err(error) => {
                                tracing::debug!(%key, %error, "the item has no menu to show");
                            }
                        }
                    });
                    return;
                }

                match pressed {
                    Button::Right => spawn_command("tray.context_menu", async move {
                        tray.context_menu(key, x, y).await
                    }),
                    Button::Middle => spawn_command("tray.secondary_activate", async move {
                        tray.secondary_activate(key, x, y).await
                    }),
                    _ => spawn_command(
                        "tray.activate",
                        async move { tray.activate(key, x, y).await },
                    ),
                }
            }
        });

        self.strip.connect_scrolled({
            let tray = self.tray.clone();
            // A touchpad delivers a gesture as many fractional deltas. Rounding each one alone
            // discards everything under half a notch, so a slow scroll reaches the item as
            // nothing at all; the runtime accumulates for ordinary applets and a view-supplying
            // one has to do the same.
            let carried = Rc::new(Cell::new((0.0_f64, 0.0_f64)));
            move |_, key, dx, dy| {
                let (mut horizontal, mut vertical) = carried.get();
                horizontal += dx;
                vertical += dy;

                let (whole, orientation) = match horizontal.abs() > vertical.abs() {
                    true => (horizontal.trunc(), "horizontal"),
                    false => (vertical.trunc(), "vertical"),
                };
                match orientation {
                    "horizontal" => horizontal -= whole,
                    _ => vertical -= whole,
                }
                carried.set((horizontal, vertical));

                let delta = whole as i32;
                if delta == 0 {
                    return;
                }
                let tray = tray.clone();
                spawn_command("tray.scroll", async move {
                    tray.scroll(key, delta, orientation.to_owned()).await
                });
            }
        });

        // Only name-based icons need re-resolving; pixels are bytes and immune. Connected once,
        // in the applet rather than per item, and a scale change moves the target size too.
        // The icon theme is process-wide and this handler is never disconnected, so it holds the
        // strip *weakly*: a rebuilt applet would otherwise leave a permanent handler repainting a
        // widget nobody can see.
        // Weak all the way through: a handler on the process-wide icon theme is never
        // disconnected, so holding the items or the settings strongly would keep a removed
        // applet's whole tray snapshot — pixmaps included — alive for the process's lifetime.
        let redraw = {
            let strip = self.strip.downgrade();
            let items = Rc::downgrade(&self.items);
            let settings = Rc::downgrade(&self.settings);
            move || {
                icon::forget_resolutions();
                if let (Some(strip), Some(items), Some(settings)) =
                    (strip.upgrade(), items.upgrade(), settings.upgrade())
                {
                    paint(&strip, &items.borrow(), &settings.borrow());
                }
            }
        };
        if let Some(display) = gtk4::gdk::Display::default() {
            let again = redraw.clone();
            gtk4::IconTheme::for_display(&display).connect_changed(move |_| again());
        }
        self.strip.connect_scale_factor_notify(move |_| redraw());

        Some(self.strip.clone().upcast())
    }

    fn orient(&mut self, orientation: gtk4::Orientation) {
        self.strip.set_orientation(orientation);
    }

    fn anchor(&self) -> Option<gtk4::Widget> {
        self.strip.anchor()
    }

    fn popover(&mut self, _seat: &crate::applet::popover::Seat) -> Option<Box<dyn PopoverHandle>> {
        let (key, node) = self.pending.borrow_mut().take()?;
        let sections = menu::sections(&node);
        if sections.is_empty() {
            return None;
        }

        let tray = self.tray.clone();
        let actions = menu::actions(&sections, move |id| {
            let tray = tray.clone();
            let key = key.clone();
            spawn_command("tray.menu_event", async move {
                tray.menu_event(key, id, "clicked".to_owned()).await
            });
        });

        let shown = gtk4::PopoverMenu::from_model(Some(&menu::model(&sections)));
        shown.insert_action_group(menu::GROUP, Some(&actions));
        shown.set_has_arrow(false);
        Some(Box::new(shown))
    }

    fn configure(&mut self, _ctx: &Ctx, config: &AppletConfig) {
        let AppletKind::Tray(settings) = &config.kind else {
            return;
        };
        self.settings.replace(settings.clone());
        self.render();
    }

    fn handle(&mut self, _ctx: &Ctx, input: &Input) {
        if !matches!(input, Input::Woken) {
            return;
        }
        self.items.replace(self.tray.snapshot().items);
        self.render();
    }
}

impl Tray {
    pub fn start(tray: TrayHandle) -> Self {
        Self {
            strip: TrayStrip::new(),
            pending: Rc::new(RefCell::new(None)),
            settings: Rc::new(RefCell::new(TrayAppletConfig::default())),
            items: Rc::new(RefCell::new(tray.snapshot().items)),
            tray,
        }
    }

    fn render(&self) {
        paint(&self.strip, &self.items.borrow(), &self.settings.borrow());
    }
}

fn paint(strip: &TrayStrip, items: &[TrayItem], settings: &TrayAppletConfig) {
    let shown = render::arrange(items, &settings.hide, &settings.pin);
    let target = ICON_SIZE * strip.scale_factor().max(1);
    let chips: Vec<TrayChip> = shown.iter().map(|item| chip(item, target)).collect();
    // `pin` promises the bar whatever the cap says, so the cap has to make room for every pinned
    // item that is actually present — otherwise the third of three pinned ids lands in the drawer.
    let cap = render::cap(&shown, &settings.pin, settings.max_visible);
    let hidden = render::hidden(chips.len(), cap);

    strip.set_max_visible(u32::from(cap));
    strip.set_overflow_tooltip((hidden > 0).then(|| hidden_label(hidden)).as_deref());
    strip.set_items(&chips);
}

/// A tray icon is the application's choice and is rendered as given — the symbolic-icon rule stops
/// at the panel's own chips.
fn chip(item: &TrayItem, target: i32) -> TrayChip {
    if let Some(path) = item.icon_theme_path.as_deref() {
        icon::learn_theme_path(path);
    }
    let attention = item.status == TrayStatus::NeedsAttention;
    // `NeedsAttention` swaps to the attention icon when the application supplied one, and falls
    // back to the ordinary one when it did not.
    let (name, pixmaps) = match attention {
        true if item.attention_icon_name.is_some() || !item.attention_pixmaps.is_empty() => {
            (item.attention_icon_name.as_deref(), &item.attention_pixmaps)
        }
        _ => (item.icon_name.as_deref(), &item.icon_pixmaps),
    };

    let source = icon::resolve(name, item.icon_theme_path.as_deref(), pixmaps, target);
    let overlay = icon::resolve(
        item.overlay_icon_name.as_deref(),
        item.icon_theme_path.as_deref(),
        &item.overlay_pixmaps,
        target,
    );

    TrayChip {
        key: item.key.clone(),
        spec: IndicatorSpec {
            icon: icon::build(source, pixmaps),
            overlay: match overlay {
                icon::Source::Missing => None,
                overlay => icon::build(overlay, &item.overlay_pixmaps),
            },
            label: item.label.clone(),
            tooltip: render::tooltip(item),
            attention,
            // Both can be true; attention wins, because styling notice loudly destroys the only
            // distinction it carries.
            notice: item.notice && !attention,
            ..Default::default()
        },
    }
}

/// The strip authors no text of its own, so the wording is supplied here.
fn hidden_label(hidden: usize) -> String {
    gettextrs::ngettext(
        "Show {count} hidden item",
        "Show {count} hidden items",
        hidden as u32,
    )
    .replace("{count}", &hidden.to_string())
}

/// Where a menu should point. The spec hands an item screen coordinates; a layer-shell panel can
/// honestly offer the chip's origin within its own toplevel, which is what an application uses to
/// place a menu near the thing that was clicked.
///
/// Measured against the widget itself this is always `(0, 0)` — `compute_bounds` answers in the
/// *target's* coordinate space, so the target has to be the root, exactly as `runtime::anchor` does.
fn position(anchor: Option<&gtk4::Widget>) -> (i32, i32) {
    let Some(anchor) = anchor else {
        return (0, 0);
    };
    let Some(root) = anchor.root() else {
        return (0, 0);
    };
    match anchor.compute_bounds(root.upcast_ref::<gtk4::Widget>()) {
        Some(bounds) => (bounds.x() as i32, bounds.y() as i32),
        None => (0, 0),
    }
}
