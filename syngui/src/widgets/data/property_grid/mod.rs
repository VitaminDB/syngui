mod element;

use crate::core::sync::Mutex;
use crate::core::Color;
use crate::mss::Dimension;
use std::sync::Arc;

#[derive(Clone, Debug)]
pub enum PropertyValue {
    Text(String),
    Number(f64),
    Bool(bool),
    Color(Color),
    Choice(Vec<String>, usize),
    /// Число в пределах: полоса заполнения, протяжка мышью по шагу, щелчок — ввод.
    Range { value: f64, min: f64, max: f64, step: f64 },
    /// Заголовок раздела (строка без значения).
    Header,
    /// Кнопка в строке: щелчок — `on_change(строка, Action(подпись))`.
    Action(String),
}

impl PropertyValue {
    fn display(&self) -> String {
        match self {
            PropertyValue::Text(s) => s.clone(),
            PropertyValue::Number(n) => {
                if *n == (*n as i64) as f64 {
                    format!("{}", *n as i64)
                } else {
                    format!("{:.2}", n)
                }
            }
            PropertyValue::Bool(b) => {
                if *b {
                    "true".to_string()
                } else {
                    "false".to_string()
                }
            }
            // Color хранит линейные компоненты — печатаем sRGB, как в from_hex
            PropertyValue::Color(c) => c.to_hex(),
            PropertyValue::Choice(items, idx) => items.get(*idx).cloned().unwrap_or_default(),
            PropertyValue::Range { value, step, .. } => {
                let d = if *step >= 1.0 { 0 } else if *step >= 0.1 { 1 } else if *step >= 0.01 { 2 } else { 3 };
                format!("{value:.prec$}", prec = d)
            }
            PropertyValue::Header => String::new(),
            PropertyValue::Action(label) => label.clone(),
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
        Self {
            name: name.into(),
            value: PropertyValue::Text(value.into()),
        }
    }

    pub fn number(name: impl Into<String>, value: f64) -> Self {
        Self {
            name: name.into(),
            value: PropertyValue::Number(value),
        }
    }

    pub fn boolean(name: impl Into<String>, value: bool) -> Self {
        Self {
            name: name.into(),
            value: PropertyValue::Bool(value),
        }
    }

    pub fn color(name: impl Into<String>, value: Color) -> Self {
        Self {
            name: name.into(),
            value: PropertyValue::Color(value),
        }
    }

    /// Число в пределах `min..=max` с шагом `step` (протяжка мышью, ввод щелчком).
    pub fn range(name: impl Into<String>, value: f64, min: f64, max: f64, step: f64) -> Self {
        Self { name: name.into(), value: PropertyValue::Range { value: value.clamp(min, max), min, max, step } }
    }

    /// Заголовок раздела.
    pub fn header(name: impl Into<String>) -> Self {
        Self { name: name.into(), value: PropertyValue::Header }
    }

    /// Кнопка `label` в строке `name`.
    pub fn action(name: impl Into<String>, label: impl Into<String>) -> Self {
        Self { name: name.into(), value: PropertyValue::Action(label.into()) }
    }

    pub fn choice(name: impl Into<String>, items: Vec<String>, selected: usize) -> Self {
        Self {
            name: name.into(),
            value: PropertyValue::Choice(items, selected),
        }
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
    fn default() -> Self {
        Self::new()
    }
}
