mod imp;

use glib::closure_local;
use gtk4::{glib, prelude::*, subclass::prelude::*};

use crate::widgets::pager::PagerItemView;
use crate::widgets::pager_item::PagerItem;

glib::wrapper! {
    pub struct PagerStrip(ObjectSubclass<imp::PagerStrip>)
        @extends gtk4::Box, gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget,
                    gtk4::Orientable;
}

#[derive(Debug, Clone)]
pub struct PagerStripEntry {
    pub id: usize,
    pub view: PagerItemView,
}

impl PagerStrip {
    pub fn new() -> Self {
        glib::Object::builder().build()
    }

    pub fn set_placeholder(&self, visible: bool) {
        self.imp().placeholder.set_visible(visible);
    }

    pub fn set_items(&self, entries: &[PagerStripEntry]) {
        let imp = self.imp();
        let current_ids: Vec<usize> = imp.items.borrow().iter().map(|(id, _)| *id).collect();
        let next_ids: Vec<usize> = entries.iter().map(|e| e.id).collect();

        let ops = row_sync_ops(&current_ids, &next_ids);

        for op in ops {
            match op {
                RowSyncOp::Move { from, to } => {
                    let mut items = imp.items.borrow_mut();
                    let (id, widget) = items.remove(from);
                    imp.rows_box.remove(&widget);
                    let sibling = if to == 0 {
                        None
                    } else {
                        Some(items[to - 1].1.clone().upcast::<gtk4::Widget>())
                    };
                    imp.rows_box.insert_child_after(&widget, sibling.as_ref());
                    items.insert(to, (id, widget));
                }
                RowSyncOp::Insert { at } => {
                    let entry = &entries[at];
                    let item = PagerItem::new();
                    item.set_view(&entry.view);
                    let weak = self.downgrade();
                    let item_id = entry.id as u64;
                    item.connect_activated(move |_| {
                        if let Some(strip) = weak.upgrade() {
                            strip.emit_by_name::<()>("activated", &[&item_id]);
                        }
                    });
                    let mut items = imp.items.borrow_mut();
                    let sibling = if at == 0 {
                        None
                    } else {
                        Some(items[at - 1].1.clone().upcast::<gtk4::Widget>())
                    };
                    imp.rows_box.insert_child_after(&item, sibling.as_ref());
                    items.insert(at, (entry.id, item));
                }
                RowSyncOp::Remove { at } => {
                    let (_, widget) = imp.items.borrow_mut().remove(at);
                    imp.rows_box.remove(&widget);
                }
            }
        }

        let items = imp.items.borrow();
        for (index, (_, item)) in items.iter().enumerate() {
            item.set_view(&entries[index].view);
        }
    }

    pub fn connect_activated(&self, f: impl Fn(&Self, u64) + 'static) -> glib::SignalHandlerId {
        self.connect_closure(
            "activated",
            false,
            closure_local!(move |strip: &Self, id: u64| f(strip, id)),
        )
    }
}

impl Default for PagerStrip {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum RowSyncOp {
    Move { from: usize, to: usize },
    Insert { at: usize },
    Remove { at: usize },
}

fn row_sync_ops(current_keys: &[usize], next_keys: &[usize]) -> Vec<RowSyncOp> {
    let mut working = current_keys.to_vec();
    let mut ops = Vec::new();

    for (target_index, key) in next_keys.iter().enumerate() {
        if working.get(target_index) == Some(key) {
            continue;
        }

        if let Some(found_index) = working.iter().position(|current| current == key) {
            let moved = working.remove(found_index);
            working.insert(target_index, moved);
            ops.push(RowSyncOp::Move {
                from: found_index,
                to: target_index,
            });
        } else {
            working.insert(target_index, *key);
            ops.push(RowSyncOp::Insert { at: target_index });
        }
    }

    while working.len() > next_keys.len() {
        working.remove(next_keys.len());
        ops.push(RowSyncOp::Remove {
            at: next_keys.len(),
        });
    }

    ops
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn row_sync_ops_preserves_move_insert_and_remove_order() {
        assert_eq!(
            row_sync_ops(&[1, 2, 3], &[2, 4, 1]),
            vec![
                RowSyncOp::Move { from: 1, to: 0 },
                RowSyncOp::Insert { at: 1 },
                RowSyncOp::Remove { at: 3 },
            ]
        );
    }
}
