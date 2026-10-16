use crate::protocol::cap;
use serde_json::{Map, Value};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Key {
    S(String),
    I(i64),
}

impl From<&str> for Key {
    fn from(value: &str) -> Self {
        Self::S(value.to_owned())
    }
}

impl From<String> for Key {
    fn from(value: String) -> Self {
        Self::S(value)
    }
}

impl From<i64> for Key {
    fn from(value: i64) -> Self {
        Self::I(value)
    }
}

impl From<u64> for Key {
    fn from(value: u64) -> Self {
        Self::I(i64::try_from(value).unwrap_or(i64::MAX))
    }
}

impl From<usize> for Key {
    fn from(value: usize) -> Self {
        Self::I(i64::try_from(value).unwrap_or(i64::MAX))
    }
}

pub(crate) type Handler<M> = Box<dyn Fn(&[Value]) -> Option<M>>;

pub struct Widget<M> {
    pub(crate) kind: &'static str,
    pub(crate) key: Option<Key>,
    pub(crate) props: Map<String, Value>,
    pub(crate) handlers: HashMap<&'static str, Handler<M>>,
    pub(crate) children: Vec<Widget<M>>,
}

impl<M> Widget<M> {
    pub(crate) fn new(kind: &'static str) -> Self {
        Self {
            kind,
            key: None,
            props: Map::new(),
            handlers: HashMap::new(),
            children: Vec::new(),
        }
    }

    pub(crate) fn key(mut self, key: impl Into<Key>) -> Self {
        self.key = Some(key.into());
        self
    }

    pub(crate) fn prop(mut self, name: &str, value: impl Into<Value>) -> Self {
        let mut value = value.into();
        if let Value::String(text) = &value {
            value = Value::String(cap(self.kind, name, text));
        }
        if !value.is_null() {
            self.props.insert(name.to_owned(), value);
        }
        self
    }

    pub(crate) fn remove_prop(mut self, name: &str) -> Self {
        self.props.remove(name);
        self
    }

    pub(crate) fn text(self, text: &str) -> Self {
        self.prop("text", text)
    }

    pub(crate) fn child(mut self, child: Widget<M>) -> Self {
        self.children.push(child);
        self
    }

    pub(crate) fn children(mut self, children: impl IntoIterator<Item = Widget<M>>) -> Self {
        self.children.extend(children);
        self
    }

    fn on(mut self, name: &'static str, handler: Handler<M>) -> Self {
        self.handlers.insert(name, handler);
        self.props.insert(name.to_owned(), Value::Bool(true));
        self
    }

    fn on_first<T: 'static>(
        self,
        name: &'static str,
        extract: fn(&Value) -> Option<T>,
        f: impl Fn(T) -> M + 'static,
    ) -> Self {
        self.on(
            name,
            Box::new(move |args| args.first().and_then(extract).map(&f)),
        )
    }

    fn on_unit(self, name: &'static str, f: impl Fn() -> M + 'static) -> Self {
        self.on(name, Box::new(move |_| Some(f())))
    }

    pub(crate) fn on_press(self, f: impl Fn(u64) -> M + 'static) -> Self {
        self.on_first("onPress", Value::as_u64, f)
    }

    pub(crate) fn on_scroll(self, f: impl Fn(f64, f64) -> M + 'static) -> Self {
        self.on(
            "onScroll",
            Box::new(move |args| {
                match (
                    args.first().and_then(Value::as_f64),
                    args.get(1).and_then(Value::as_f64),
                ) {
                    (Some(dx), Some(dy)) => Some(f(dx, dy)),
                    _ => None,
                }
            }),
        )
    }

    pub(crate) fn on_activate(self, f: impl Fn() -> M + 'static) -> Self {
        self.on_unit("onActivate", f)
    }

    pub(crate) fn on_click(self, f: impl Fn() -> M + 'static) -> Self {
        self.on_unit("onClick", f)
    }

    pub(crate) fn on_toggle(self, f: impl Fn(bool) -> M + 'static) -> Self {
        self.on_first("onToggle", Value::as_bool, f)
    }

    pub(crate) fn on_move(self, f: impl Fn(f64) -> M + 'static) -> Self {
        self.on_first("onMove", Value::as_f64, f)
    }

    pub(crate) fn on_change_f64(self, f: impl Fn(f64) -> M + 'static) -> Self {
        self.on_first("onChange", Value::as_f64, f)
    }

    pub(crate) fn on_change_string(self, f: impl Fn(String) -> M + 'static) -> Self {
        self.on_first("onChange", |value| value.as_str().map(str::to_owned), f)
    }

    pub(crate) fn on_submit(self, f: impl Fn(String) -> M + 'static) -> Self {
        self.on_first("onSubmit", |value| value.as_str().map(str::to_owned), f)
    }

    pub(crate) fn on_select_index(self, f: impl Fn(usize) -> M + 'static) -> Self {
        self.on_first(
            "onSelect",
            |value| {
                value
                    .as_u64()
                    .map(|index| usize::try_from(index).unwrap_or(0))
            },
            f,
        )
    }

    pub(crate) fn on_select_date(self, f: impl Fn(String) -> M + 'static) -> Self {
        self.on_first("onSelect", |value| value.as_str().map(str::to_owned), f)
    }

    pub(crate) fn on_month(self, f: impl Fn(i64, i64) -> M + 'static) -> Self {
        self.on(
            "onMonth",
            Box::new(move |args| {
                match (
                    args.first().and_then(Value::as_i64),
                    args.get(1).and_then(Value::as_i64),
                ) {
                    (Some(year), Some(month)) => Some(f(year, month)),
                    _ => None,
                }
            }),
        )
    }
}
