mod imp;

use gtk4::{gio, glib, prelude::*, subclass::prelude::*};

pub const FINISH_ACTIONS: [&str; 4] = ["save", "save-as", "open-with", "open-window"];

glib::wrapper! {
    pub struct ShotToolbar(ObjectSubclass<imp::ShotToolbar>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Default for ShotToolbar {
    fn default() -> Self {
        Self::new()
    }
}

impl ShotToolbar {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn set_tool(&self, tool: &str) {
        let Some(tool) = canonical_tool(tool) else {
            return;
        };
        let imp = self.imp();
        if imp.tool.get() == tool {
            return;
        }
        imp.tool.set(tool);
        let button = self.tool_button(tool);
        if !button.is_active() {
            button.set_active(true);
        }
    }

    pub fn set_target(&self, target: &str) {
        let Some(target) = canonical_target(target) else {
            return;
        };
        let imp = self.imp();
        if imp.target.get() == target {
            return;
        }
        imp.target.set(target);
        let button = match target {
            "window" => &imp.window,
            _ => &imp.screen,
        };
        if !button.is_active() {
            button.set_active(true);
        }
    }

    pub fn set_color(&self, color: u32) {
        if color > 4 {
            return;
        }
        let imp = self.imp();
        if imp.color.get() == color {
            return;
        }
        imp.color.set(color);
        let button = self.color_button(color);
        if !button.is_active() {
            button.set_active(true);
        }
        self.sync_color_dot(color);
    }

    pub fn set_stroke(&self, stroke: u32) {
        if !(1..=3).contains(&stroke) {
            return;
        }
        let imp = self.imp();
        if imp.stroke.get() == stroke {
            return;
        }
        imp.stroke.set(stroke);
        let name = match stroke {
            1 => "thin",
            3 => "thick",
            _ => "medium",
        };
        if imp.strokes.active_name().as_deref() != Some(name) {
            imp.strokes.set_active_name(Some(name));
        }
    }

    pub fn set_can_undo(&self, can: bool) {
        let imp = self.imp();
        if imp.can_undo.get() == can {
            return;
        }
        imp.can_undo.set(can);
        imp.undo.set_sensitive(can);
    }

    pub fn set_can_redo(&self, can: bool) {
        let imp = self.imp();
        if imp.can_redo.get() == can {
            return;
        }
        imp.can_redo.set(can);
        imp.redo.set_sensitive(can);
    }

    pub fn set_target_visible(&self, visible: bool) {
        let imp = self.imp();
        if imp.targets.get_visible() == visible && imp.target_gap.get_visible() == visible {
            return;
        }
        imp.targets.set_visible(visible);
        imp.target_gap.set_visible(visible);
    }

    pub fn set_movable(&self, movable: bool) {
        let handle = &self.imp().handle;
        if handle.get_visible() != movable {
            handle.set_visible(movable);
        }
    }

    pub fn set_target_sensitive(&self, sensitive: bool) {
        let targets = &self.imp().targets;
        if targets.is_sensitive() == sensitive {
            return;
        }
        targets.set_sensitive(sensitive);
    }

    pub fn set_apply_visible(&self, visible: bool) {
        let button = &self.imp().apply_crop;
        if button.get_visible() == visible {
            return;
        }
        button.set_visible(visible);
    }

    pub fn set_finish_visible(&self, visible: bool) {
        let imp = self.imp();
        if imp.finish.get_visible() == visible && imp.finish_gap.get_visible() == visible {
            return;
        }
        imp.finish.set_visible(visible);
        imp.finish_gap.set_visible(visible);
    }

    pub fn set_history_visible(&self, visible: bool) {
        let imp = self.imp();
        if imp.history.get_visible() == visible && imp.history_gap.get_visible() == visible {
            return;
        }
        imp.history.set_visible(visible);
        imp.history_gap.set_visible(visible);
    }

    pub fn set_finish_actions(&self, enabled: &[&str]) {
        let group = &self.imp().finishing;
        for name in FINISH_ACTIONS {
            let Some(action) = group
                .lookup_action(name)
                .and_downcast::<gio::SimpleAction>()
            else {
                continue;
            };
            let on = enabled.contains(&name);
            if action.is_enabled() != on {
                action.set_enabled(on);
            }
        }
    }

    pub fn connect_tool_selected<F: Fn(&Self, String) + 'static>(
        &self,
        f: F,
    ) -> glib::SignalHandlerId {
        self.connect_closure(
            "tool-selected",
            false,
            glib::closure_local!(move |toolbar: Self, tool: String| f(&toolbar, tool)),
        )
    }

    pub fn connect_target_selected<F: Fn(&Self, String) + 'static>(
        &self,
        f: F,
    ) -> glib::SignalHandlerId {
        self.connect_closure(
            "target-selected",
            false,
            glib::closure_local!(move |toolbar: Self, target: String| f(&toolbar, target)),
        )
    }

    pub fn connect_color_selected<F: Fn(&Self, u32) + 'static>(
        &self,
        f: F,
    ) -> glib::SignalHandlerId {
        self.connect_closure(
            "color-selected",
            false,
            glib::closure_local!(move |toolbar: Self, color: u32| f(&toolbar, color)),
        )
    }

    pub fn connect_stroke_selected<F: Fn(&Self, u32) + 'static>(
        &self,
        f: F,
    ) -> glib::SignalHandlerId {
        self.connect_closure(
            "stroke-selected",
            false,
            glib::closure_local!(move |toolbar: Self, stroke: u32| f(&toolbar, stroke)),
        )
    }

    pub fn connect_action<F: Fn(&Self, String) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            "action",
            false,
            glib::closure_local!(move |toolbar: Self, action: String| f(&toolbar, action)),
        )
    }

    pub fn connect_moved<F: Fn(&Self, f64, f64) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            "moved",
            false,
            glib::closure_local!(move |toolbar: Self, dx: f64, dy: f64| f(&toolbar, dx, dy)),
        )
    }

    fn wire(&self) {
        self.wire_tool(&self.imp().crop, "crop");
        self.wire_tool(&self.imp().arrow, "arrow");
        self.wire_tool(&self.imp().line, "line");
        self.wire_tool(&self.imp().freehand, "freehand");
        self.wire_tool(&self.imp().highlighter, "highlighter");
        self.wire_tool(&self.imp().rect, "rect");
        self.wire_tool(&self.imp().filled_rect, "filled-rect");
        self.wire_tool(&self.imp().oval, "oval");
        self.wire_tool(&self.imp().number, "number");
        self.wire_tool(&self.imp().text, "text");
        self.wire_tool(&self.imp().blur, "blur");
        self.wire_target(&self.imp().screen, "screen");
        self.wire_target(&self.imp().window, "window");
        self.wire_color(&self.imp().red, 0);
        self.wire_color(&self.imp().yellow, 1);
        self.wire_color(&self.imp().green, 2);
        self.wire_color(&self.imp().blue, 3);
        self.wire_color(&self.imp().white, 4);
        self.imp().strokes.connect_active_name_notify(glib::clone!(
            #[weak(rename_to = toolbar)]
            self,
            move |group| {
                let width = match group.active_name().as_deref() {
                    Some("thin") => 1,
                    Some("thick") => 3,
                    _ => 2,
                };
                toolbar.choose_stroke(width);
            }
        ));
        self.wire_action(&self.imp().undo, "undo", true);
        self.wire_action(&self.imp().redo, "redo", true);
        self.imp().copy_close.connect_clicked(glib::clone!(
            #[weak(rename_to = toolbar)]
            self,
            move |_| toolbar.emit_by_name::<()>("action", &[&"copy-close".to_owned()])
        ));
        let group = &self.imp().finishing;
        for name in FINISH_ACTIONS {
            let action = gio::SimpleAction::new(name, None);
            action.connect_activate(glib::clone!(
                #[weak(rename_to = toolbar)]
                self,
                move |_, _| toolbar.emit_by_name::<()>("action", &[&name.to_owned()])
            ));
            group.add_action(&action);
        }
        self.insert_action_group("shot", Some(group));
        self.wire_action(&self.imp().cancel, "cancel", false);
        self.wire_action(&self.imp().apply_crop, "apply-crop", false);
        self.wire_handle();
    }

    fn wire_handle(&self) {
        let handle = &self.imp().handle;
        handle.set_cursor_from_name(Some("grab"));
        let drag = gtk4::GestureDrag::new();
        drag.connect_drag_begin(glib::clone!(
            #[weak(rename_to = toolbar)]
            self,
            move |drag, _, _| toolbar.imp().drag_from.set(surface_point(drag))
        ));
        drag.connect_drag_update(glib::clone!(
            #[weak(rename_to = toolbar)]
            self,
            move |drag, _, _| {
                let imp = toolbar.imp();
                let (Some((x0, y0)), Some((x, y))) = (imp.drag_from.get(), surface_point(drag))
                else {
                    return;
                };
                imp.drag_from.set(Some((x, y)));
                toolbar.emit_by_name::<()>("moved", &[&(x - x0), &(y - y0)]);
            }
        ));
        drag.connect_drag_end(glib::clone!(
            #[weak(rename_to = toolbar)]
            self,
            move |_, _, _| toolbar.imp().drag_from.set(None)
        ));
        handle.add_controller(drag);
    }

    fn wire_tool(&self, button: &gtk4::ToggleButton, tool: &'static str) {
        button.connect_clicked(glib::clone!(
            #[weak(rename_to = toolbar)]
            self,
            move |button| {
                if button.is_active() {
                    toolbar.choose_tool(tool);
                }
            }
        ));
    }

    fn wire_target(&self, button: &gtk4::ToggleButton, target: &'static str) {
        button.connect_clicked(glib::clone!(
            #[weak(rename_to = toolbar)]
            self,
            move |button| {
                if button.is_active() {
                    toolbar.choose_target(target);
                }
            }
        ));
    }

    fn wire_color(&self, button: &gtk4::ToggleButton, color: u32) {
        button.connect_clicked(glib::clone!(
            #[weak(rename_to = toolbar)]
            self,
            move |button| {
                if button.is_active() {
                    toolbar.choose_color(color);
                }
            }
        ));
    }

    fn wire_action(&self, button: &gtk4::Button, action: &'static str, gated: bool) {
        button.connect_clicked(glib::clone!(
            #[weak(rename_to = toolbar)]
            self,
            move |button| {
                if gated && !button.is_sensitive() {
                    return;
                }
                toolbar.emit_by_name::<()>("action", &[&action.to_string()]);
            }
        ));
    }

    fn choose_tool(&self, tool: &'static str) {
        if self.imp().tool.get() == tool {
            return;
        }
        self.imp().tool.set(tool);
        self.emit_by_name::<()>("tool-selected", &[&tool.to_string()]);
    }

    fn choose_target(&self, target: &'static str) {
        if self.imp().target.get() == target {
            return;
        }
        self.imp().target.set(target);
        self.emit_by_name::<()>("target-selected", &[&target.to_string()]);
    }

    fn choose_color(&self, color: u32) {
        if self.imp().color.get() == color {
            return;
        }
        self.imp().color.set(color);
        self.sync_color_dot(color);
        self.emit_by_name::<()>("color-selected", &[&color]);
    }

    fn choose_stroke(&self, stroke: u32) {
        if self.imp().stroke.get() == stroke {
            return;
        }
        self.imp().stroke.set(stroke);
        self.emit_by_name::<()>("stroke-selected", &[&stroke]);
    }

    fn tool_button(&self, tool: &str) -> gtk4::ToggleButton {
        let imp = self.imp();
        match tool {
            "crop" => imp.crop.get(),
            "line" => imp.line.get(),
            "freehand" => imp.freehand.get(),
            "highlighter" => imp.highlighter.get(),
            "rect" => imp.rect.get(),
            "filled-rect" => imp.filled_rect.get(),
            "oval" => imp.oval.get(),
            "number" => imp.number.get(),
            "text" => imp.text.get(),
            "blur" => imp.blur.get(),
            _ => imp.arrow.get(),
        }
    }

    fn color_button(&self, color: u32) -> gtk4::ToggleButton {
        let imp = self.imp();
        match color {
            1 => imp.yellow.get(),
            2 => imp.green.get(),
            3 => imp.blue.get(),
            4 => imp.white.get(),
            _ => imp.red.get(),
        }
    }

    fn sync_color_dot(&self, color: u32) {
        let dot = &self.imp().dot;
        for class in ["red", "yellow", "green", "blue", "white"] {
            dot.remove_css_class(class);
        }
        dot.add_css_class(["red", "yellow", "green", "blue", "white"][color as usize]);
    }
}

fn canonical_tool(tool: &str) -> Option<&'static str> {
    Some(match tool {
        "crop" => "crop",
        "arrow" => "arrow",
        "line" => "line",
        "freehand" => "freehand",
        "highlighter" => "highlighter",
        "rect" => "rect",
        "filled-rect" => "filled-rect",
        "oval" => "oval",
        "number" => "number",
        "text" => "text",
        "blur" => "blur",
        _ => return None,
    })
}

fn surface_point(drag: &gtk4::GestureDrag) -> Option<(f64, f64)> {
    drag.last_event(drag.current_sequence().as_ref())?
        .position()
}

fn canonical_target(target: &str) -> Option<&'static str> {
    Some(match target {
        "screen" => "screen",
        "window" => "window",
        _ => return None,
    })
}
