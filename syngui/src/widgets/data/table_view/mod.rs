mod element;

use std::sync::Arc;
use std::fmt;
use crate::core::Color;
use crate::core::sync::Mutex;
use crate::mss::{Dimension, TextAlign};
use crate::widget::Widget;

pub type CellRendererFn = Arc<dyn Fn(usize, &str) -> Box<dyn Widget> + Send + Sync>;

pub type CellRendererWithRowFn =
    Arc<dyn Fn(usize, &[String]) -> Box<dyn Widget> + Send + Sync>;

pub type SortKeyFn = Arc<dyn Fn(&str) -> SortKey + Send + Sync>;

#[derive(Clone, Debug)]
pub enum SortKey {
    Number(f64),
    Text(String),
    Empty,
}

impl SortKey {
    pub fn from_cell(text: &str) -> Self {
        let trimmed = text.trim();
        if trimmed.is_empty() || trimmed == "—" || trimmed == "-" {
            return SortKey::Empty;
        }
        let normalized = trimmed.replace(',', ".").replace(' ', "");
        if let Ok(n) = normalized.parse::<f64>() {
            return SortKey::Number(n);
        }
        SortKey::Text(trimmed.to_lowercase())
    }

    pub(super) fn cmp(&self, other: &SortKey) -> std::cmp::Ordering {
        use std::cmp::Ordering;
        match (self, other) {
            (SortKey::Empty, SortKey::Empty) => Ordering::Equal,
            (SortKey::Empty, _) => Ordering::Greater,
            (_, SortKey::Empty) => Ordering::Less,
            (SortKey::Number(a), SortKey::Number(b)) => a.partial_cmp(b).unwrap_or(Ordering::Equal),
            (SortKey::Number(_), SortKey::Text(_)) => Ordering::Less,
            (SortKey::Text(_), SortKey::Number(_)) => Ordering::Greater,
            (SortKey::Text(a), SortKey::Text(b)) => a.cmp(b),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum ColumnAlign {
    #[default]
    Left,
    Center,
    Right,
}

impl ColumnAlign {
    pub(super) fn to_text_align(self) -> TextAlign {
        match self {
            ColumnAlign::Left => TextAlign::LEFT | TextAlign::VCENTER,
            ColumnAlign::Center => TextAlign::CENTER,
            ColumnAlign::Right => TextAlign::RIGHT | TextAlign::VCENTER,
        }
    }
}

pub struct TableColumn {
    pub header: String,
    pub width: ColumnWidth,
    pub min_width: f32,
    pub max_width: f32,
    pub resizable: bool,
    pub sortable: bool,
    pub hideable: bool,
    pub visible: bool,
    pub sort_key: Option<SortKeyFn>,
    pub cell_renderer: Option<CellRendererFn>,
    pub cell_renderer_with_row: Option<CellRendererWithRowFn>,
    pub align: ColumnAlign,
}

impl Clone for TableColumn {
    fn clone(&self) -> Self {
        Self {
            header: self.header.clone(),
            width: self.width,
            min_width: self.min_width,
            max_width: self.max_width,
            resizable: self.resizable,
            sortable: self.sortable,
            hideable: self.hideable,
            visible: self.visible,
            sort_key: self.sort_key.clone(),
            cell_renderer: self.cell_renderer.clone(),
            cell_renderer_with_row: self.cell_renderer_with_row.clone(),
            align: self.align,
        }
    }
}

impl fmt::Debug for TableColumn {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TableColumn")
            .field("header", &self.header)
            .field("width", &self.width)
            .field("min_width", &self.min_width)
            .field("max_width", &self.max_width)
            .field("resizable", &self.resizable)
            .field("sortable", &self.sortable)
            .field("hideable", &self.hideable)
            .field("visible", &self.visible)
            .field("sort_key", &self.sort_key.as_ref().map(|_| ".."))
            .field("cell_renderer", &self.cell_renderer.as_ref().map(|_| ".."))
            .field("cell_renderer_with_row", &self.cell_renderer_with_row.as_ref().map(|_| ".."))
            .field("align", &self.align)
            .finish()
    }
}

impl TableColumn {
    pub fn new(header: impl Into<String>) -> Self {
        Self {
            header: header.into(),
            width: ColumnWidth::default(),
            min_width: 50.0,
            max_width: f32::INFINITY,
            resizable: true,
            sortable: true,
            hideable: true,
            visible: true,
            sort_key: None,
            cell_renderer: None,
            cell_renderer_with_row: None,
            align: ColumnAlign::default(),
        }
    }

    pub fn fixed(header: impl Into<String>, width: f32) -> Self {
        Self {
            header: header.into(),
            width: ColumnWidth::Fixed(width),
            min_width: 50.0,
            max_width: f32::INFINITY,
            resizable: true,
            sortable: true,
            hideable: true,
            visible: true,
            sort_key: None,
            cell_renderer: None,
            cell_renderer_with_row: None,
            align: ColumnAlign::default(),
        }
    }

    pub fn flex(header: impl Into<String>, flex: f32) -> Self {
        Self {
            header: header.into(),
            width: ColumnWidth::Flex(flex),
            min_width: 50.0,
            max_width: f32::INFINITY,
            resizable: true,
            sortable: true,
            hideable: true,
            visible: true,
            sort_key: None,
            cell_renderer: None,
            cell_renderer_with_row: None,
            align: ColumnAlign::default(),
        }
    }

    pub fn min_width(mut self, w: f32) -> Self { self.min_width = w; self }

    pub fn max_width(mut self, w: f32) -> Self { self.max_width = w; self }

    pub fn resizable(mut self, r: bool) -> Self { self.resizable = r; self }

    pub fn sortable(mut self, s: bool) -> Self { self.sortable = s; self }

    pub fn hideable(mut self, h: bool) -> Self { self.hideable = h; self }

    pub fn visible(mut self, v: bool) -> Self { self.visible = v; self }

    pub fn sort_key(
        mut self,
        f: impl Fn(&str) -> SortKey + Send + Sync + 'static,
    ) -> Self {
        self.sort_key = Some(Arc::new(f));
        self
    }

    pub fn align(mut self, align: ColumnAlign) -> Self { self.align = align; self }

    pub fn cell_renderer(
        mut self,
        f: impl Fn(usize, &str) -> Box<dyn Widget> + Send + Sync + 'static,
    ) -> Self {
        self.cell_renderer = Some(Arc::new(f));
        self
    }

    pub fn cell_renderer_with_row(
        mut self,
        f: impl Fn(usize, &[String]) -> Box<dyn Widget> + Send + Sync + 'static,
    ) -> Self {
        self.cell_renderer_with_row = Some(Arc::new(f));
        self
    }
}

#[derive(Clone, Copy, Debug)]
pub enum ColumnWidth {
    Fixed(f32),
    Flex(f32),
}

impl Default for ColumnWidth {
    fn default() -> Self { ColumnWidth::Flex(1.0) }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SortDirection {
    #[default]
    None,
    Ascending,
    Descending,
}

pub(super) type RowBuilderFn = Arc<dyn Fn(usize) -> Vec<String> + Send + Sync>;

pub(super) enum TableDataSource {
    Eager(Vec<Vec<String>>),
    Virtual {
        row_count: usize,
        row_builder: RowBuilderFn,
    },
}

impl TableDataSource {
    pub(super) fn row_count(&self) -> usize {
        match self {
            TableDataSource::Eager(rows) => rows.len(),
            TableDataSource::Virtual { row_count, .. } => *row_count,
        }
    }
}

pub struct TableView {
    pub(super) columns: Vec<TableColumn>,
    pub(super) data: TableDataSource,
    pub(super) sortable: bool,
    pub(super) row_height: f32,
    pub(super) header_height: f32,
    pub(super) striped: bool,
    pub(super) buffer_size: usize,
    pub(super) on_sort: Option<Arc<Mutex<dyn FnMut(usize, SortDirection) + Send>>>,
    pub(super) on_row_click: Option<Arc<Mutex<dyn FnMut(usize) + Send>>>,
    pub(super) selected_rows: Vec<usize>,
    pub(super) width: Option<Dimension>,
    pub(super) height: Option<Dimension>,
    pub(super) custom_header_bg: Option<Color>,
    pub(super) custom_header_color: Option<Color>,
    pub(super) custom_header_font_size: Option<f32>,
    pub(super) custom_cell_font_size: Option<f32>,
    pub(super) custom_cell_padding: Option<f32>,
    pub(super) custom_cell_min_width: Option<f32>,
    pub(super) custom_cell_max_width: Option<f32>,
    pub(super) custom_row_hover_bg: Option<Color>,
    pub(super) custom_row_selected_bg: Option<Color>,
    pub(super) custom_row_padding: Option<[f32; 4]>,
    pub(super) scroll_state: Option<Arc<Mutex<f32>>>,
    pub(super) column_widths_state: Option<Arc<Mutex<Vec<Option<f32>>>>>,
    pub(super) on_column_resize: Option<Arc<Mutex<dyn FnMut(usize, f32) + Send>>>,
    pub(super) table_id: Option<String>,
    pub(super) column_visibility_state: Option<Arc<Mutex<Vec<bool>>>>,
    pub(super) on_column_visibility_change:
        Option<Arc<Mutex<dyn FnMut(usize, bool) + Send>>>,
    pub(super) keyboard_nav: bool,
    pub(super) editable: bool,
    pub(super) on_cell_select: Option<Arc<Mutex<dyn FnMut(usize, usize) + Send>>>,
    pub(super) on_cell_edit: Option<Arc<Mutex<dyn FnMut(usize, usize, String, String) + Send>>>,
}

impl TableView {
    pub fn new(columns: Vec<TableColumn>, rows: Vec<Vec<String>>) -> Self {
        Self {
            columns,
            data: TableDataSource::Eager(rows),
            sortable: true,
            row_height: 40.0,
            header_height: 44.0,
            striped: true,
            buffer_size: 5,
            on_sort: None,
            on_row_click: None,
            selected_rows: Vec::new(),
            width: None,
            height: None,
            custom_header_bg: None,
            custom_header_color: None,
            custom_header_font_size: None,
            custom_cell_font_size: None,
            custom_cell_padding: None,
            custom_cell_min_width: None,
            custom_cell_max_width: None,
            custom_row_hover_bg: None,
            custom_row_selected_bg: None,
            custom_row_padding: None,
            scroll_state: None,
            column_widths_state: None,
            on_column_resize: None,
            table_id: None,
            column_visibility_state: None,
            on_column_visibility_change: None,
            keyboard_nav: false,
            editable: false,
            on_cell_select: None,
            on_cell_edit: None,
        }
    }

    pub fn virtual_new(
        columns: Vec<TableColumn>,
        row_count: usize,
        row_builder: impl Fn(usize) -> Vec<String> + Send + Sync + 'static,
    ) -> Self {
        Self {
            columns,
            data: TableDataSource::Virtual {
                row_count,
                row_builder: Arc::new(row_builder),
            },
            sortable: true,
            row_height: 40.0,
            header_height: 44.0,
            striped: true,
            buffer_size: 5,
            on_sort: None,
            on_row_click: None,
            selected_rows: Vec::new(),
            width: None,
            height: None,
            custom_header_bg: None,
            custom_header_color: None,
            custom_header_font_size: None,
            custom_cell_font_size: None,
            custom_cell_padding: None,
            custom_cell_min_width: None,
            custom_cell_max_width: None,
            custom_row_hover_bg: None,
            custom_row_selected_bg: None,
            custom_row_padding: None,
            scroll_state: None,
            column_widths_state: None,
            on_column_resize: None,
            table_id: None,
            column_visibility_state: None,
            on_column_visibility_change: None,
            keyboard_nav: false,
            editable: false,
            on_cell_select: None,
            on_cell_edit: None,
        }
    }

    pub fn sortable(mut self, s: bool) -> Self { self.sortable = s; self }
    pub fn row_height(mut self, h: f32) -> Self { self.row_height = h; self }
    pub fn header_height(mut self, h: f32) -> Self { self.header_height = h; self }
    pub fn striped(mut self, s: bool) -> Self { self.striped = s; self }
    pub fn buffer_size(mut self, n: usize) -> Self { self.buffer_size = n; self }
    pub fn selected_rows(mut self, rows: Vec<usize>) -> Self { self.selected_rows = rows; self }
    pub fn width(mut self, w: f32) -> Self { self.width = Some(Dimension::Px(w)); self }
    pub fn height(mut self, h: f32) -> Self { self.height = Some(Dimension::Px(h)); self }
    pub fn scroll_state(mut self, state: Arc<Mutex<f32>>) -> Self { self.scroll_state = Some(state); self }

    pub fn column_widths_state(mut self, state: Arc<Mutex<Vec<Option<f32>>>>) -> Self {
        self.column_widths_state = Some(state);
        self
    }

    pub fn on_column_resize(mut self, f: impl FnMut(usize, f32) + Send + 'static) -> Self {
        self.on_column_resize = Some(Arc::new(Mutex::new(f)));
        self
    }

    pub fn table_id(mut self, id: impl Into<String>) -> Self {
        self.table_id = Some(id.into());
        self
    }

    pub fn column_visibility_state(mut self, state: Arc<Mutex<Vec<bool>>>) -> Self {
        self.column_visibility_state = Some(state);
        self
    }

    pub fn on_column_visibility_change(
        mut self,
        f: impl FnMut(usize, bool) + Send + 'static,
    ) -> Self {
        self.on_column_visibility_change = Some(Arc::new(Mutex::new(f)));
        self
    }

    pub fn keyboard_nav(mut self, enabled: bool) -> Self { self.keyboard_nav = enabled; self }

    pub fn editable(mut self, enabled: bool) -> Self { self.editable = enabled; self }

    pub fn on_sort(mut self, callback: impl FnMut(usize, SortDirection) + Send + 'static) -> Self {
        self.on_sort = Some(Arc::new(Mutex::new(callback)));
        self
    }

    pub fn on_row_click(mut self, callback: impl FnMut(usize) + Send + 'static) -> Self {
        self.on_row_click = Some(Arc::new(Mutex::new(callback)));
        self
    }

    pub fn on_cell_select(mut self, callback: impl FnMut(usize, usize) + Send + 'static) -> Self {
        self.on_cell_select = Some(Arc::new(Mutex::new(callback)));
        self
    }

    pub fn on_cell_edit(
        mut self,
        callback: impl FnMut(usize, usize, String, String) + Send + 'static,
    ) -> Self {
        self.on_cell_edit = Some(Arc::new(Mutex::new(callback)));
        self
    }

    pub fn header_bg(mut self, color: Color) -> Self { self.custom_header_bg = Some(color); self }
    pub fn header_color(mut self, color: Color) -> Self { self.custom_header_color = Some(color); self }
    pub fn header_font_size(mut self, size: f32) -> Self { self.custom_header_font_size = Some(size); self }
    pub fn cell_font_size(mut self, size: f32) -> Self { self.custom_cell_font_size = Some(size); self }
    pub fn cell_padding(mut self, p: f32) -> Self { self.custom_cell_padding = Some(p); self }
    pub fn row_hover_bg(mut self, color: Color) -> Self { self.custom_row_hover_bg = Some(color); self }
    pub fn row_selected_bg(mut self, color: Color) -> Self { self.custom_row_selected_bg = Some(color); self }
    pub fn row_padding(mut self, padding: [f32; 4]) -> Self { self.custom_row_padding = Some(padding); self }
}
