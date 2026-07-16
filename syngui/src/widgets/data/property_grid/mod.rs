mod element;

use crate::core::Color;
use crate::mss::Dimension;
use std::sync::Arc;
use crate::core::sync::Mutex;

#[derive(Clone, Debug)]
pub enum PropertyValue {
    Text(String),
    Number(f64),
    Bool(bool),
    Color(Color),
    Choice(Vec<String>, usize),
}

impl PropertyValue {
    fn display(&self) -> String {
        match self {
            PropertyValue::Text(s) => s.clone(),
            PropertyValue::Number(n) => {
                if *n == (*n as i64) as f64 { format!("{}", *n as i64) } else { format!("{:.2}", n) }
            }
            PropertyValue::Bool(b) => if *b { "true".to_string() } else { "false".to_string() },
            PropertyValue::Color(c) => format!("#{:02X}{:02X}{:02X}",
                (c.r * 255.0) as u8, (c.g * 255.0) as u8, (c.b * 255.0) as u8),
            PropertyValue::Choice(items, idx) => items.get(*idx).cloned().unwrap_or_default(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Property {
    pub name: String,
    pub value: PropertyValue,
}

impl Property {
    pub fn text(name: impl Into<String>, value: impl Into<String>) -> Self {
        Self { name: name.into(), value: PropertyValue::Text(value.into()) }
    }

    pub fn number(name: impl Into<String>, value: f64) -> Self {
        Self { name: name.into(), value: PropertyValue::Number(value) }
    }

    pub fn boolean(name: impl Into<String>, value: bool) -> Self {
        Self { name: name.into(), value: PropertyValue::Bool(value) }
    }

    pub fn color(name: impl Into<String>, value: Color) -> Self {
        Self { name: name.into(), value: PropertyValue::Color(value) }
    }

    pub fn choice(name: impl Into<String>, items: Vec<String>, selected: usize) -> Self {
        Self { name: name.into(), value: PropertyValue::Choice(items, selected) }
    }
}

pub struct PropertyGrid {
    properties: Vec<Property>,
    on_change: Option<Arc<Mutex<dyn FnMut(usize, PropertyValue) + Send>>>,
    on_add: Option<Arc<Mutex<dyn FnMut(&str, PropertyValue) + Send>>>,
    on_remove: Option<Arc<Mutex<dyn FnMut(usize, &str) + Send>>>,
    label_width: Option<f32>,
    row_height: f32,
    width: Option<Dimension>,
    height: Option<Dimension>,
    editable: bool,
    suggestions: Vec<String>,
}

impl PropertyGrid {
    pub fn new() -> Self {
        Self {
            properties: Vec::new(),
            on_change: None,
            on_add: None,
            on_remove: None,
            label_width: None,
            row_height: 32.0,
            width: None,
            height: None,
            editable: false,
            suggestions: Vec::new(),
        }
    }

    pub fn property(mut self, prop: Property) -> Self {
        self.properties.push(prop);
        self
    }

    pub fn properties(mut self, props: Vec<Property>) -> Self {
        self.properties = props;
        self
    }

    pub fn on_change(mut self, f: impl FnMut(usize, PropertyValue) + Send + 'static) -> Self {
        self.on_change = Some(Arc::new(Mutex::new(f)));
        self
    }

    pub fn label_width(mut self, w: f32) -> Self {
        self.label_width = Some(w);
        self
    }

    pub fn row_height(mut self, h: f32) -> Self {
        self.row_height = h;
        self
    }

    pub fn width(mut self, w: f32) -> Self {
        self.width = Some(Dimension::Px(w));
        self
    }

    pub fn height(mut self, h: f32) -> Self {
        self.height = Some(Dimension::Px(h));
        self
    }

    pub fn editable(mut self, v: bool) -> Self {
        self.editable = v;
        self
    }

    pub fn suggestions(mut self, items: Vec<impl Into<String>>) -> Self {
        self.suggestions = items.into_iter().map(|s| s.into()).collect();
        self
    }

    pub fn on_add(mut self, f: impl FnMut(&str, PropertyValue) + Send + 'static) -> Self {
        self.on_add = Some(Arc::new(Mutex::new(f)));
        self
    }

    pub fn on_remove(mut self, f: impl FnMut(usize, &str) + Send + 'static) -> Self {
        self.on_remove = Some(Arc::new(Mutex::new(f)));
        self
    }
}

impl Default for PropertyGrid {
    fn default() -> Self { Self::new() }
}
