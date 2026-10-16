use crate::context::{App, Ctx};
use crate::protocol::{
    Check, Fail, In, MAX_CHILDREN, MAX_ITEMS, MAX_LINE, MAX_NODES, MAX_OPS, MAX_SERIES, controlled,
    oversized, seq_of,
};
use crate::widget::{Handler, Key, Widget};
use serde_json::{Map, Value, json};
use std::collections::HashMap;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::mpsc::Sender;

struct El<M> {
    id: u64,
    kind: &'static str,
    key: Option<Key>,
    props: Map<String, Value>,
    handlers: HashMap<&'static str, Handler<M>>,
    children: Vec<usize>,
    applied_seq: Option<u64>,
    confirm_seq: Option<u64>,
}

pub struct Session<A: App> {
    app: A,
    ctx: Ctx,
    out: Sender<In>,
    els: Vec<El<A::Msg>>,
    free: Vec<usize>,
    by_id: HashMap<u64, usize>,
    root_children: Vec<usize>,
    next_id: u64,
    pub(crate) dirty: bool,
    mounted: bool,
}

impl<A: App> Session<A> {
    pub fn new(app: A, tx: Sender<In>) -> Self {
        Self {
            app,
            out: tx.clone(),
            els: Vec::new(),
            free: Vec::new(),
            by_id: HashMap::new(),
            root_children: Vec::new(),
            next_id: 1,
            dirty: true,
            mounted: false,
            ctx: Ctx {
                name: String::new(),
                state_dir: String::new(),
                options: json!({}),
                placement: None,
                popover_open: false,
                services: Map::new(),
                tx: tx.clone(),
            },
        }
    }

    pub fn ctx(&self) -> &Ctx {
        &self.ctx
    }

    pub fn app(&self) -> &A {
        &self.app
    }

    pub fn app_mut(&mut self) -> &mut A {
        &mut self.app
    }

    fn send(&self, value: Value) -> Check<()> {
        let line =
            serde_json::to_string(&value).map_err(|error| Fail::Render(error.to_string()))?;
        if line.len() > MAX_LINE {
            return Err(Fail::Limit("outgoing line exceeds 1 MiB".to_owned()));
        }
        let _ = self.out.send(In::Send(line));
        Ok(())
    }

    fn new_el(&mut self, widget: Widget<A::Msg>) -> Check<usize> {
        if widget.children.len() > MAX_CHILDREN {
            return Err(Fail::Limit("parent exceeds 200 children".to_owned()));
        }

        let id = self.next_id;
        self.next_id += 1;
        let el = El {
            id,
            kind: widget.kind,
            key: widget.key,
            props: widget.props,
            handlers: widget.handlers,
            children: Vec::new(),
            applied_seq: None,
            confirm_seq: None,
        };

        let index = match self.free.pop() {
            Some(index) => {
                self.els[index] = el;
                index
            }
            None => {
                self.els.push(el);
                self.els.len() - 1
            }
        };
        self.by_id.insert(id, index);

        let mut children = Vec::new();
        for child in widget.children {
            children.push(self.new_el(child)?);
        }

        self.els[index].children = children;
        Ok(index)
    }

    fn serialize(&self, index: usize) -> Value {
        let el = &self.els[index];
        json!({
            "id": el.id,
            "type": el.kind,
            "props": Value::Object(el.props.clone()),
            "children": el.children.iter().map(|child| self.serialize(*child)).collect::<Vec<_>>(),
        })
    }

    fn forget(&mut self, index: usize) {
        let children = std::mem::take(&mut self.els[index].children);
        self.by_id.remove(&self.els[index].id);
        self.els[index].props.clear();
        self.els[index].handlers.clear();
        self.free.push(index);
        for child in children {
            self.forget(child);
        }
    }

    fn diff(&mut self, index: usize, widget: Widget<A::Msg>, ops: &mut Vec<Value>) -> Check<()> {
        let mut diff = Map::new();
        {
            let el = &mut self.els[index];
            el.handlers = widget.handlers;
            for (key, new) in &widget.props {
                if el.props.get(key) != Some(new) {
                    diff.insert(key.clone(), new.clone());
                }
            }

            for key in el.props.keys() {
                if !widget.props.contains_key(key) {
                    diff.insert(key.clone(), Value::Null);
                }
            }
        }

        if !diff.is_empty() {
            let mut op = json!({"op": "set", "id": self.els[index].id, "props": diff});
            if let Some((_, prop)) = controlled(self.els[index].kind)
                && self.els[index].confirm_seq.is_some()
                && op["props"].get(prop).is_some()
            {
                op["seq"] = json!(self.els[index].confirm_seq.take());
            }
            ops.push(op);
            self.els[index].props = widget.props;
        }

        let children = self.reconcile_children(
            self.els[index].children.clone(),
            self.els[index].id,
            widget.children,
            ops,
        )?;

        self.els[index].children = children;
        Ok(())
    }

    fn reconcile_children(
        &mut self,
        old: Vec<usize>,
        parent_id: u64,
        widgets: Vec<Widget<A::Msg>>,
        ops: &mut Vec<Value>,
    ) -> Check<Vec<usize>> {
        if widgets.len() > MAX_CHILDREN {
            return Err(Fail::Limit("parent exceeds 200 children".to_owned()));
        }

        let mut keyed: HashMap<Key, usize> = HashMap::new();
        let mut unkeyed: Vec<usize> = Vec::new();
        for index in &old {
            match &self.els[*index].key {
                Some(key) => {
                    keyed.insert(key.clone(), *index);
                }
                None => unkeyed.push(*index),
            }
        }

        let mut used: std::collections::HashSet<usize> = std::collections::HashSet::new();
        let mut wanted: Vec<(usize, bool)> = Vec::new();
        let mut cursor = 0;
        for widget in widgets {
            let matched = match &widget.key {
                Some(key) => keyed
                    .get(key)
                    .copied()
                    .filter(|index| self.els[*index].kind == widget.kind && !used.contains(index)),
                None => {
                    let candidate = unkeyed.get(cursor).copied();
                    cursor += 1;
                    candidate.filter(|index| self.els[*index].kind == widget.kind)
                }
            };

            match matched {
                Some(index) => {
                    used.insert(index);
                    self.diff(index, widget, ops)?;
                    wanted.push((index, false));
                }
                None => wanted.push((self.new_el(widget)?, true)),
            }
        }

        for index in &old {
            if !used.contains(index) {
                ops.push(json!({"op": "remove", "parent": parent_id, "id": self.els[*index].id}));
                self.forget(*index);
            }
        }

        let mut current: Vec<usize> = old
            .into_iter()
            .filter(|index| used.contains(index))
            .collect();

        for (position, (index, fresh)) in wanted.iter().enumerate() {
            let before = current
                .get(position)
                .map(|current| json!(self.els[*current].id));

            if *fresh {
                ops.push(json!({
                    "op": "insert", "parent": parent_id,
                    "before": before, "node": self.serialize(*index),
                }));
                current.insert(position, *index);
            } else if current.get(position) != Some(index) {
                ops.push(json!({
                    "op": "move", "parent": parent_id,
                    "id": self.els[*index].id, "before": before,
                }));
                current.retain(|current| current != index);
                current.insert(position, *index);
            }
        }

        Ok(wanted.into_iter().map(|(index, _)| index).collect())
    }

    pub fn rebuild(&mut self) -> Check<()> {
        let tree = catch_unwind(AssertUnwindSafe(|| self.app.build(&self.ctx)))
            .map_err(|_| Fail::Render("build panicked".to_owned()))?;
        let mut ops = Vec::new();
        let children = self.reconcile_children(self.root_children.clone(), 0, tree, &mut ops)?;

        self.root_children = children;
        if self.by_id.len() + 1 > MAX_NODES {
            return Err(Fail::Limit("tree exceeds 300 nodes".to_owned()));
        }

        let live: Vec<usize> = self.by_id.values().copied().collect();
        for index in live {
            let over = self.els[index].props.iter().any(|(key, value)| {
                let limit = if key == "values" || key == "tooltips" {
                    MAX_SERIES
                } else {
                    MAX_ITEMS
                };
                oversized(value, limit)
            });

            if over {
                return Err(Fail::Limit("array prop exceeds 64 items".to_owned()));
            }

            if let (Some(seq), Some((_, prop))) = (
                self.els[index].confirm_seq,
                controlled(self.els[index].kind),
            ) {
                ops.push(json!({
                    "op": "set", "id": self.els[index].id,
                    "props": {prop: self.els[index].props.get(prop).cloned().unwrap_or(Value::Null)},
                    "seq": seq,
                }));
                self.els[index].confirm_seq = None;
            }
        }

        if ops.len() > MAX_OPS {
            return Err(Fail::Limit("commit exceeds 1200 operations".to_owned()));
        }

        if !ops.is_empty() {
            self.send(json!({"t": "commit", "ops": ops}))?;
        }

        self.dirty = false;
        Ok(())
    }

    pub fn handle(&mut self, msg: &Value) {
        match msg["t"].as_str() {
            Some("hello") if !self.mounted => {
                if msg["v"].as_u64() != Some(1) {
                    panic!("unsupported host protocol version");
                }

                self.ctx.name = msg["name"].as_str().unwrap_or("").to_owned();
                self.ctx.state_dir = msg["state"].as_str().unwrap_or("").to_owned();
                self.ctx.options = if msg["options"].is_object() {
                    msg["options"].clone()
                } else {
                    json!({})
                };
                self.ctx.placement = msg["placement"]
                    .as_object()
                    .map(|_| msg["placement"].clone());
                self.mounted = true;
                self.dirty = true;
                self.app.start(&self.ctx);
            }
            Some("hello") | None => {}
            Some(_) if !self.mounted => {}
            Some("options") => {
                self.ctx.options = if msg["options"].is_object() {
                    msg["options"].clone()
                } else {
                    json!({})
                };
                self.dirty = true;
            }
            Some("placement") => {
                self.ctx.placement = msg["placement"]
                    .as_object()
                    .map(|_| msg["placement"].clone());
                self.dirty = true;
            }
            Some("popover") => {
                self.ctx.popover_open = msg["open"].as_bool().unwrap_or(false);
                self.dirty = true;
            }
            Some("service") => {
                if let Some(name) = msg["name"].as_str() {
                    self.ctx
                        .services
                        .insert(name.to_owned(), msg["value"].clone());
                    self.dirty = true;
                }
            }
            Some("event") => self.event(msg),
            Some(_) => {}
        }
    }

    fn event(&mut self, msg: &Value) {
        let Some(id) = msg["id"].as_u64() else { return };
        let Some(index) = self.by_id.get(&id).copied() else {
            return;
        };
        let Some(name) = msg["name"].as_str() else {
            return;
        };

        if !self.els[index].handlers.contains_key(name) {
            return;
        }

        if let Some((event, _)) = controlled(self.els[index].kind)
            && event == name
            && let Some(seq) = seq_of(&msg["seq"])
        {
            if self.els[index]
                .applied_seq
                .is_some_and(|applied| seq < applied)
            {
                return;
            }
            self.els[index].applied_seq = Some(seq);
            self.els[index].confirm_seq = Some(seq);
        }

        let args: Vec<Value> = msg["args"].as_array().cloned().unwrap_or_default();
        let message = self.els[index]
            .handlers
            .get(name)
            .and_then(|handler| handler(&args));

        if let Some(message) = message {
            self.app.update(&self.ctx, message);
        }

        self.dirty = true;
    }

    pub fn flush(&mut self) -> Check<()> {
        if self.mounted && self.dirty {
            self.rebuild()?;
        }
        Ok(())
    }
}
