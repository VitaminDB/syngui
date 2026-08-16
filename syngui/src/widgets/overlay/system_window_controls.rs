//! Кнопки управления окном в системном виде — для приложений с собственным
//! титлбаром (CSD).
//!
//! Набор, порядок и метрики берутся из настроек рабочего стола
//! ([`crate::appearance::decorations`]). На KDE с темой Aurorae кнопки
//! растеризуются прямо из SVG темы, поэтому выглядят ровно так же, как у всех
//! остальных окон, включая состояния hover/pressed/inactive. В остальных
//! окружениях рисуется встроенный вектор, который красится из MSS (`color`,
//! `background-color` — подложка под курсором).

use std::any::Any;
use std::collections::HashMap;
use std::sync::Arc;

use crate::appearance::decorations::{
    read_system_decorations, ButtonState, DecorationMetrics, DecorationStyle, SystemDecorations,
    WindowButton,
};
use crate::core::sync::Mutex;
use crate::core::{Color, Point, Rect, RectExt, Size};
use crate::gpu::image_store::{ImageHandle, ImageSource, ImageStore};
use crate::input::{CursorIcon, Event, EventResult, MouseButton};
use crate::layout::Constraints;
use crate::mss::{ComputedStyle, MssFields};
use crate::render::{DisplayList, TextureId};
use crate::widget::context::{EventContext, EventContextExt};
use crate::widget::{
    DirtyFlags, Element, ElementId, ElementTree, StyledElement, UpdateContext, Widget,
};

/// Сторона титлбара: раскладка DE задаёт левую и правую группы отдельно.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlsSide {
    Left,
    Right,
}

/// Кнопки, которые приложение действительно умеет выполнять.
fn is_supported(button: WindowButton) -> bool {
    matches!(button, WindowButton::Minimize | WindowButton::Maximize | WindowButton::Close)
}

/// Масштаб экрана: SVG растеризуется ровно в физические пиксели, которые займёт
/// кнопка. Промежуточный «запас» здесь только вредит — текстуры грузятся без
/// mip-уровней, поэтому уменьшение при отрисовке даёт рваные края.
fn scale_factor() -> f32 {
    crate::signal::primary_window()
        .map(|w| w.scale_factor() as f32)
        .unwrap_or(1.0)
        .clamp(0.5, 8.0)
}

pub struct SystemWindowControls {
    side: ControlsSide,
    decorations: Option<SystemDecorations>,
    button_size: Option<f32>,
    spacing: Option<f32>,
    active: bool,
    maximized: bool,
}

impl SystemWindowControls {
    /// Кнопки той стороны титлбара, которую задал рабочий стол. Если для этой
    /// стороны кнопок нет, виджет занимает нулевую ширину.
    pub fn new(side: ControlsSide) -> Self {
        Self {
            side,
            decorations: None,
            button_size: None,
            spacing: None,
            active: true,
            maximized: false,
        }
    }

    pub fn left() -> Self {
        Self::new(ControlsSide::Left)
    }

    pub fn right() -> Self {
        Self::new(ControlsSide::Right)
    }

    /// Готовые декорации вместо чтения системы — например прочитанные один раз
    /// при старте и сложенные в сигнал.
    pub fn decorations(mut self, decorations: SystemDecorations) -> Self {
        self.decorations = Some(decorations);
        self
    }

    /// Переопределяет размер кнопки из темы.
    pub fn button_size(mut self, size: f32) -> Self {
        self.button_size = Some(size);
        self
    }

    pub fn spacing(mut self, spacing: f32) -> Self {
        self.spacing = Some(spacing);
        self
    }

    /// Окно в фокусе. В неактивном окне тема рисует приглушённые кнопки.
    pub fn active(mut self, active: bool) -> Self {
        self.active = active;
        self
    }

    /// Окно развёрнуто — кнопка «развернуть» показывает иконку «восстановить».
    pub fn maximized(mut self, maximized: bool) -> Self {
        self.maximized = maximized;
        self
    }
}

impl Widget for SystemWindowControls {
    fn create_element(&self) -> Box<dyn Element> {
        let decorations = self.decorations.clone().unwrap_or_else(read_system_decorations);
        let buttons: Vec<WindowButton> = match self.side {
            ControlsSide::Left => decorations.layout.left.clone(),
            ControlsSide::Right => decorations.layout.right.clone(),
        }
        .into_iter()
        .filter(|b| is_supported(*b))
        .collect();

        let mut metrics = decorations.metrics;
        if let Some(size) = self.button_size {
            metrics.button_size = size;
        }
        if let Some(spacing) = self.spacing {
            metrics.button_spacing = spacing;
        }

        Box::new(SystemWindowControlsElement {
            id: ElementId::new(),
            buttons,
            metrics,
            style: decorations.style,
            active: self.active,
            maximized: self.maximized,
            images: HashMap::new(),
            raster_scale: 0.0,
            image_store: None,
            hovered: None,
            pressed: None,
            bounds: Rect::zero(),
            classes: Vec::new(),
            dirty_flags: DirtyFlags::LAYOUT | DirtyFlags::RENDER,
            mss: MssFields::new(),
        })
    }

    fn can_update(&self, other: &dyn Any) -> bool {
        other.is::<Self>()
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
    fn mount(&self, _tree: &mut ElementTree, _parent_id: ElementId) {}
}

pub struct SystemWindowControlsElement {
    id: ElementId,
    buttons: Vec<WindowButton>,
    metrics: DecorationMetrics,
    style: DecorationStyle,
    active: bool,
    maximized: bool,
    /// Растеризованные состояния: ключ — (индекс кнопки, состояние).
    images: HashMap<(usize, ButtonState), ImageHandle>,
    /// Масштаб экрана, под который растеризованы `images`.
    raster_scale: f32,
    image_store: Option<Arc<Mutex<ImageStore>>>,
    hovered: Option<usize>,
    pressed: Option<usize>,
    bounds: Rect,
    classes: Vec<String>,
    dirty_flags: DirtyFlags,
    mss: MssFields,
}

impl SystemWindowControlsElement {
    fn button_rect(&self, index: usize) -> Rect {
        let size = self.metrics.button_size;
        let step = size + self.metrics.button_spacing;
        let y = self.bounds.y() + (self.bounds.size.height - size).max(0.0) / 2.0;
        Rect::new(
            Point::new(self.bounds.x() + index as f32 * step, y),
            Size::new(size, size),
        )
    }

    fn index_at(&self, position: Point) -> Option<usize> {
        (0..self.buttons.len()).find(|i| self.button_rect(*i).contains(position))
    }

    fn state_of(&self, index: usize) -> ButtonState {
        if self.pressed == Some(index) {
            ButtonState::Pressed
        } else if self.hovered == Some(index) {
            ButtonState::Hover
        } else if self.active {
            ButtonState::Normal
        } else {
            ButtonState::Inactive
        }
    }

    /// Один раз растеризует все состояния кнопок темы Aurorae и складывает их в
    /// общий `ImageStore` под стабильными ключами.
    #[cfg(feature = "svg")]
    fn load_theme_images(&mut self) {
        use crate::appearance::decorations::rasterize_aurorae_button;

        let DecorationStyle::Aurorae(theme) = &self.style else {
            return;
        };
        let Some(store) = self.image_store.clone() else {
            return;
        };
        let scale = scale_factor();
        let px = ((self.metrics.button_size * scale).round() as u32).clamp(8, 512);
        self.raster_scale = scale;

        for (index, button) in self.buttons.iter().enumerate() {
            let svg = if *button == WindowButton::Maximize && self.maximized {
                theme.restore_svg()
            } else {
                theme.button_svg(*button)
            };
            let Some(svg) = svg else { continue };

            for state in [
                ButtonState::Normal,
                ButtonState::Hover,
                ButtonState::Pressed,
                ButtonState::Inactive,
            ] {
                let Some(raster) = rasterize_aurorae_button(&svg, state, px) else {
                    continue;
                };
                let key = format!(
                    "syngui-aurorae://{}/{}/{state:?}@{px}",
                    theme.name,
                    svg.file_name().and_then(|n| n.to_str()).unwrap_or("button")
                );
                let source = ImageSource::RawRgba {
                    key,
                    width: raster.width,
                    height: raster.height,
                    rgba: Arc::new(raster.rgba),
                };
                if let Ok(mut store) = store.lock() {
                    let (handle, _) = store.request(&source);
                    self.images.insert((index, state), handle);
                }
            }
        }
    }

    #[cfg(not(feature = "svg"))]
    fn load_theme_images(&mut self) {}

    /// Встроенная отрисовка для окружений без SVG-темы: подложка под курсором
    /// плюс глиф кнопки линиями.
    fn draw_builtin(&self, list: &mut DisplayList, index: usize, rect: Rect) {
        let state = self.state_of(index);
        let color = self.mss.color.unwrap_or(Color::from_hex("#1C1D22"));
        let is_close = self.buttons[index] == WindowButton::Close;

        // Подложка: у «закрыть» — красная, у остальных — из MSS.
        let hover_bg = if is_close {
            Color::from_hex("#E81123")
        } else {
            self.mss.background_color.unwrap_or(Color::from_hex("#00000018"))
        };
        match state {
            ButtonState::Hover => list.push_rect(rect, hover_bg, [rect.size.width * 0.5; 4]),
            ButtonState::Pressed => {
                list.push_rect(rect, hover_bg.darken(0.15), [rect.size.width * 0.5; 4])
            }
            _ => {}
        }

        let glyph = if is_close && matches!(state, ButtonState::Hover | ButtonState::Pressed) {
            Color::WHITE
        } else if matches!(state, ButtonState::Inactive) {
            color.with_alpha(0.45)
        } else {
            color
        };

        // Глиф вписан в 40% кнопки — как в Breeze.
        let inset = rect.size.width * 0.3;
        let g = Rect::new(
            Point::new(rect.x() + inset, rect.y() + inset),
            Size::new(rect.size.width - inset * 2.0, rect.size.height - inset * 2.0),
        );
        let thickness = (rect.size.width * 0.08).max(1.0);

        match self.buttons[index] {
            WindowButton::Minimize => {
                let bar = Rect::new(
                    Point::new(g.x(), g.y() + g.size.height / 2.0 - thickness / 2.0),
                    Size::new(g.size.width, thickness),
                );
                list.push_rect(bar, glyph, [0.0; 4]);
            }
            WindowButton::Maximize => {
                let border = crate::render::Border::new(thickness, glyph);
                list.push_rect_bordered(g, Color::TRANSPARENT, [1.0; 4], border);
            }
            WindowButton::Close => {
                list.push_line_strip(
                    vec![
                        [g.x(), g.y()],
                        [g.x() + g.size.width, g.y() + g.size.height],
                    ],
                    glyph,
                    thickness,
                );
                list.push_line_strip(
                    vec![
                        [g.x() + g.size.width, g.y()],
                        [g.x(), g.y() + g.size.height],
                    ],
                    glyph,
                    thickness,
                );
            }
            _ => {}
        }
    }

    fn activate(&self, index: usize, ctx: &mut EventContext) {
        match self.buttons[index] {
            WindowButton::Close => ctx.close_window(),
            WindowButton::Minimize => ctx.minimize_window(),
            WindowButton::Maximize => ctx.toggle_maximize_window(),
            _ => {}
        }
    }
}

impl Element for SystemWindowControlsElement {
    fn update(&mut self, widget: &dyn Widget, _ctx: &mut UpdateContext) {
        let Some(w) = widget.as_any().downcast_ref::<SystemWindowControls>() else {
            return;
        };
        let maximized_changed = self.maximized != w.maximized;
        self.active = w.active;
        self.maximized = w.maximized;
        if maximized_changed {
            // Иконка «развернуть/восстановить» — другой файл темы.
            self.images.clear();
            self.load_theme_images();
        }
        self.mark_dirty(DirtyFlags::RENDER);
    }

    fn layout(&mut self, constraints: Constraints) -> Size {
        // Окно могло переехать на экран с другим масштабом — тогда текстуры
        // нужного размера ещё не растеризованы.
        if !self.images.is_empty() && (self.raster_scale - scale_factor()).abs() > 0.01 {
            self.images.clear();
            self.load_theme_images();
        }
        let count = self.buttons.len();
        let width = if count == 0 {
            0.0
        } else {
            count as f32 * self.metrics.button_size
                + (count - 1) as f32 * self.metrics.button_spacing
        };
        // Занимаем всю высоту титлбара и центрируем кнопки внутри себя: иначе
        // группа кнопок оказывается ростом с саму кнопку и прижимается к
        // верхнему краю полосы.
        let height = if constraints.max_height.is_finite() {
            constraints.max_height.max(self.metrics.button_size)
        } else {
            self.metrics.button_size.max(constraints.min_height)
        };
        let size = Size::new(width.min(constraints.max_width), height);
        self.bounds = Rect::new(self.bounds.origin, size);
        size
    }

    fn build_display_list(&self, list: &mut DisplayList, _clip: Rect) {
        for index in 0..self.buttons.len() {
            let rect = self.button_rect(index);
            let handle = self.images.get(&(index, self.state_of(index)));
            match handle {
                Some(handle) => {
                    let uv = Rect::new(Point::new(0.0, 0.0), Size::new(1.0, 1.0));
                    list.push_image(rect, TextureId(handle.0), uv, Color::WHITE);
                }
                None => self.draw_builtin(list, index, rect),
            }
        }
    }

    fn handle_event(&mut self, event: &Event, ctx: &mut EventContext) -> EventResult {
        if self.buttons.is_empty() {
            return EventResult::Ignored;
        }
        match event {
            Event::MouseMove(pos) => {
                let was = self.hovered;
                self.hovered = self.index_at(*pos);
                if self.hovered.is_some() {
                    ctx.set_cursor(CursorIcon::Pointer);
                }
                if self.hovered != was {
                    ctx.request_paint();
                    return EventResult::Handled;
                }
                if self.hovered.is_some() {
                    EventResult::Handled
                } else {
                    EventResult::Ignored
                }
            }
            Event::MouseDown { button, position } if *button == MouseButton::Left => {
                self.pressed = self.index_at(*position);
                if self.pressed.is_some() {
                    ctx.request_paint();
                    EventResult::Handled
                } else {
                    EventResult::Ignored
                }
            }
            Event::MouseUp { button, position } if *button == MouseButton::Left => {
                let Some(pressed) = self.pressed.take() else {
                    return EventResult::Ignored;
                };
                ctx.request_paint();
                if self.index_at(*position) == Some(pressed) {
                    self.activate(pressed, ctx);
                }
                EventResult::Handled
            }
            _ => EventResult::Ignored,
        }
    }

    fn animate(&mut self, _dt: std::time::Duration) -> bool {
        false
    }

    fn needs_repaint(&self) -> bool {
        false
    }

    fn children(&self) -> &[ElementId] {
        &[]
    }

    fn bounds(&self) -> Rect {
        self.bounds
    }

    fn set_position(&mut self, pos: Point) {
        self.bounds.origin = pos;
    }

    fn set_content_size(&mut self, size: Size) {
        self.bounds = Rect::new(self.bounds.origin, size);
    }

    fn mark_dirty(&mut self, flags: DirtyFlags) {
        self.dirty_flags |= flags;
    }

    fn clear_dirty(&mut self, flags: DirtyFlags) {
        self.dirty_flags.remove(flags);
    }

    fn is_dirty(&self, flags: DirtyFlags) -> bool {
        self.dirty_flags.contains(flags)
    }

    fn id(&self) -> ElementId {
        self.id
    }

    fn set_id(&mut self, id: ElementId) {
        self.id = id;
    }

    fn mount(&mut self, tree: &mut ElementTree) {
        self.image_store = tree.image_store.clone();
        self.load_theme_images();
    }

    fn set_classes(&mut self, classes: Vec<String>) {
        self.classes = classes;
        self.mark_dirty(DirtyFlags::RENDER);
    }

    fn get_classes(&self) -> &[String] {
        &self.classes
    }

    fn element_type_name(&self) -> &str {
        "SystemWindowControls"
    }

    fn reset_mss_styles(&mut self) {
        self.mss.reset();
    }

    fn mss(&self) -> Option<&MssFields> {
        Some(&self.mss)
    }

    fn apply_computed_style(&mut self, style: &ComputedStyle) {
        self.mss.apply(style);
        if let Some(v) = style.get("button-size").and_then(|v| v.as_px()) {
            self.metrics.button_size = v;
        }
        if let Some(v) = style.get("button-spacing").and_then(|v| v.as_px()) {
            self.metrics.button_spacing = v;
        }
        self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
    }

    fn accessibility_info(&self) -> Option<crate::a11y::AccessibilityInfo> {
        Some(crate::a11y::AccessibilityInfo {
            role: crate::a11y::Role::Group,
            state: crate::a11y::NodeState::default(),
            properties: crate::a11y::NodeProperties {
                label: Some("Window controls".into()),
                ..Default::default()
            },
        })
    }
}

impl StyledElement for SystemWindowControlsElement {
    fn apply_style(&mut self, _style: &ComputedStyle) {
        self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
    }

    fn classes(&self) -> &[String] {
        &self.classes
    }

    fn set_classes(&mut self, classes: Vec<String>) {
        self.classes = classes;
        self.mark_dirty(DirtyFlags::RENDER);
    }
}
