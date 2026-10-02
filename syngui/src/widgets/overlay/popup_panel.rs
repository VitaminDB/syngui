use crate::animation::{Animation, Easing};
use crate::core::sync::Mutex;
use crate::core::{Color, Point, Rect, Size};
use crate::input::{Event, EventResult, MouseButton};
use crate::layout::Constraints;
use crate::mss::{ComputedStyle, MssFields};
use crate::render::{Border, DisplayList};
use crate::signal::{use_signal, RwSignal};
use crate::widget::context::{EventContext, EventContextExt};
use crate::widget::{
    DirtyFlags, Element, ElementId, ElementTree, LayoutHint, StyledElement, UpdateContext, Widget,
};
use crate::widgets::containers::{AnimationAxis, IntoWidget};
use crate::widgets::overlay::menu::PopupAnchor;
use crate::widgets::overlay::placement::{clamp_span, fit_span};
use std::any::Any;
use std::cell::Cell;
use std::sync::Arc;
use std::time::Duration;

/// Длительность и кривая появления по умолчанию (`.reveal`); MSS
/// `transition: size|width|height …` их перекрывает.
const REVEAL_MS: u32 = 280;

pub struct PopupPanel {
    children: Vec<Box<dyn Widget>>,
    is_open: RwSignal<bool>,
    anchor_rect: RwSignal<Rect>,
    anchor: PopupAnchor,
    min_width: f32,
    max_width: Option<f32>,
    max_height: f32,
    on_close: Option<Arc<Mutex<dyn FnMut() + Send>>>,
    classes: Vec<String>,
    reveal: Option<AnimationAxis>,
}

impl PopupPanel {
    pub fn new() -> Self {
        Self {
            children: Vec::new(),
            is_open: use_signal(false),
            anchor_rect: use_signal(Rect::zero()),
            anchor: PopupAnchor::BottomEnd,
            min_width: 180.0,
            max_width: None,
            max_height: 600.0,
            on_close: None,
            classes: Vec::new(),
            reveal: None,
        }
    }

    /// Панель выезжает и уезжает обратно: по ширине — от стороны якоря
    /// (у `EndCenter`/`BottomStart`/`Position` слева направо, у `BottomEnd`
    /// справа налево), по высоте — сверху вниз. Закрытие (клик мимо,
    /// Escape, `is_open = false` снаружи) проигрывается в обратную сторону;
    /// всё это время панель видна, но ввод уже не принимает. Длительность
    /// и кривая — из MSS `transition: size …` (`spring(k, c)` — пружина),
    /// по умолчанию 280 мс `emphasized-decelerate`.
    pub fn reveal(mut self, axis: AnimationAxis) -> Self {
        self.reveal = Some(axis);
        self
    }

    pub fn child<M>(mut self, widget: impl IntoWidget<M>) -> Self {
        self.children.push(widget.into_widget());
        self
    }

    pub fn is_open(mut self, state: RwSignal<bool>) -> Self {
        self.is_open = state;
        self
    }

    pub fn anchor_rect(mut self, rect: RwSignal<Rect>) -> Self {
        self.anchor_rect = rect;
        self
    }

    pub fn anchor(mut self, anchor: PopupAnchor) -> Self {
        self.anchor = anchor;
        self
    }

    pub fn min_width(mut self, width: f32) -> Self {
        self.min_width = width;
        self
    }

    pub fn max_width(mut self, width: f32) -> Self {
        self.max_width = Some(width);
        self
    }

    pub fn max_height(mut self, height: f32) -> Self {
        self.max_height = height;
        self
    }

    pub fn on_close(mut self, callback: impl FnMut() + Send + 'static) -> Self {
        self.on_close = Some(Arc::new(Mutex::new(callback)));
        self
    }

    pub fn class(mut self, class: &str) -> Self {
        crate::widget::push_classes(&mut self.classes, class.to_string());
        self
    }
}

impl Default for PopupPanel {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for PopupPanel {
    fn create_element(&self) -> Box<dyn Element> {
        Box::new(PopupPanelElement {
            id: ElementId::new(),
            is_open: self.is_open,
            anchor_rect: self.anchor_rect,
            anchor: self.anchor,
            min_width: self.min_width,
            max_width: self.max_width,
            max_height: self.max_height,
            on_close: self.on_close.clone(),
            child_ids: Vec::new(),
            bounds: Rect::zero(),
            viewport_size: Cell::new(Size::zero()),
            content_size: Cell::new(Size::zero()),
            placed_rect: Cell::new(Rect::zero()),
            classes: self.classes.clone(),
            dirty_flags: DirtyFlags::LAYOUT | DirtyFlags::RENDER,
            overlay_registered: false,
            mss: MssFields::new(),
            reveal: self.reveal,
            reveal_transition: None,
            reveal_anim: None,
            shown: if self.reveal.is_some() { 0.0 } else { 1.0 },
            was_open: false,
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

    fn mount(&self, tree: &mut ElementTree, parent_id: ElementId) {
        for child in &self.children {
            let el = child.create_element();
            let id = tree.insert_with_type_id(el, Some(parent_id), child.as_any().type_id());
            child.mount(tree, id);
        }
    }

    fn child_widgets(&self) -> Vec<&dyn Widget> {
        self.children
            .iter()
            .map(|c| c.as_ref() as &dyn Widget)
            .collect()
    }

    fn widget_classes(&self) -> &[String] {
        &self.classes
    }
}

struct PopupPanelElement {
    id: ElementId,
    is_open: RwSignal<bool>,
    anchor_rect: RwSignal<Rect>,
    anchor: PopupAnchor,
    min_width: f32,
    max_width: Option<f32>,
    max_height: f32,
    on_close: Option<Arc<Mutex<dyn FnMut() + Send>>>,
    child_ids: Vec<ElementId>,
    bounds: Rect,
    viewport_size: Cell<Size>,
    content_size: Cell<Size>,
    placed_rect: Cell<Rect>,
    classes: Vec<String>,
    dirty_flags: DirtyFlags,
    overlay_registered: bool,
    mss: MssFields,
    reveal: Option<AnimationAxis>,
    reveal_transition: Option<(u32, Easing)>,
    reveal_anim: Option<Animation>,
    /// Доля показа 0..1 (у пружины — с перелётом за 1).
    shown: f32,
    /// Состояние `is_open`, под которое уже запущено движение.
    was_open: bool,
}

impl PopupPanelElement {
    fn is_open(&self) -> bool {
        self.is_open.get_untracked()
    }

    /// Панель на экране: открыта либо ещё уезжает.
    fn is_shown(&self) -> bool {
        self.is_open() || (self.reveal.is_some() && self.shown > 0.001)
    }

    /// Запустить появление/уход, если `is_open` сменился. Зовётся отовсюду,
    /// где элемент получает управление: сигнал могут переключить и клик по
    /// самой панели, и код приложения.
    fn sync_reveal(&mut self) -> bool {
        let open = self.is_open();
        if open == self.was_open {
            return false;
        }
        self.was_open = open;
        if self.reveal.is_none() {
            self.shown = if open { 1.0 } else { 0.0 };
            return true;
        }
        let target = if open { 1.0 } else { 0.0 };
        let (ms, easing) = self.reveal_transition.unwrap_or((REVEAL_MS, Easing::EMPHASIZED_DECELERATE));
        let from = self.shown;
        self.reveal_anim = Some(match easing {
            Easing::Spring { stiffness, damping } => {
                Animation::spring().from(from).to(target).stiffness(stiffness).damping(damping).build()
            }
            e => {
                // Уход быстрее и с разгоном, как у Presence.
                let (e, ms) = if open {
                    (e, ms)
                } else {
                    let e = match e {
                        Easing::EMPHASIZED | Easing::EMPHASIZED_DECELERATE => Easing::EMPHASIZED_ACCELERATE,
                        Easing::STANDARD_DECELERATE => Easing::STANDARD_ACCELERATE,
                        e => e,
                    };
                    (e, ms.saturating_mul(3) / 4)
                };
                let span = (target - from).abs().min(1.0);
                Animation::tween(e)
                    .from(from)
                    .to(target)
                    .duration_ms(((ms as f32) * span).round().max(1.0) as u32)
                    .build()
            }
        });
        self.mark_dirty(DirtyFlags::RENDER | DirtyFlags::LAYOUT | DirtyFlags::ANIMATION);
        true
    }

    /// Видимая часть панели при доле показа `shown`.
    fn revealed_rect(&self, panel: Rect) -> Rect {
        let Some(axis) = self.reveal else { return panel };
        let p = self.shown.max(0.0);
        let mut r = panel;
        if matches!(axis, AnimationAxis::Width | AnimationAxis::Both) {
            r.size.width = panel.size.width * p;
            match self.anchor {
                PopupAnchor::BottomEnd => r.origin.x = panel.origin.x + panel.size.width - r.size.width,
                PopupAnchor::BottomCenter => r.origin.x = panel.origin.x + (panel.size.width - r.size.width) / 2.0,
                _ => {}
            }
        }
        if matches!(axis, AnimationAxis::Height | AnimationAxis::Both) {
            r.size.height = panel.size.height * p;
        }
        r
    }

    fn close(&mut self, ctx: &mut EventContext) {
        self.is_open.set(false);
        self.sync_reveal();
        if self.overlay_registered {
            ctx.unregister_overlay();
            self.overlay_registered = false;
        }
        if let Some(ref cb) = self.on_close {
            if let Ok(mut f) = cb.lock() {
                f();
            }
        }
        ctx.request_paint();
    }

    fn border_radius(&self) -> [f32; 4] {
        self.mss.border_radius_resolved(0.0, 8.0)
    }

    /// Внутренние отступы панели из MSS: `[left, top, right, bottom]`.
    /// Раньше `padding` в стиле панели ничего не давал — содержимое
    /// клалось от кромки к кромке, и текст полей касался краёв окна.
    fn padding(&self) -> [f32; 4] {
        self.mss.padding_ltrb([0.0; 4])
    }

    fn panel_rect(&self) -> Rect {
        let viewport = self.viewport_size.get();
        let content = self.content_size.get();
        let ar = self.anchor_rect.get_untracked();

        let pad = self.padding();
        let width = (content.width + pad[0] + pad[2])
            .max(self.min_width)
            .min(self.content_width_limit());
        let natural_height = content.height + pad[1] + pad[3];
        let height = natural_height.min(self.max_height);

        // `flip_up_to` — низ перевёрнутого варианта: панель раскроется вверх,
        // упершись в эту линию (верх якоря либо сама точка открытия).
        let (x, y, flip_up_to) = match self.anchor {
            PopupAnchor::BottomStart => (ar.origin.x, ar.origin.y + ar.size.height, ar.origin.y),
            PopupAnchor::BottomEnd => (
                ar.origin.x + ar.size.width - width,
                ar.origin.y + ar.size.height,
                ar.origin.y,
            ),
            PopupAnchor::BottomCenter => {
                (ar.origin.x + (ar.size.width - width) / 2.0, ar.origin.y + ar.size.height, ar.origin.y)
            }
            PopupAnchor::Position => (ar.origin.x, ar.origin.y, ar.origin.y),
            PopupAnchor::EndCenter => {
                let y = ar.origin.y + (ar.size.height - height) / 2.0;
                let origin = crate::viewport::viewport_origin();
                let y = origin.y + clamp_span(y - origin.y, height, viewport.height);
                (ar.origin.x + ar.size.width, y, y)
            }
        };

        // Координаты якоря глобальные (включают safe area), а viewport_size —
        // только размер layout-области. Зажимаем позицию в её реальных
        // границах `[origin .. origin + size]`, иначе на Android панель,
        // которой не хватает места (открытая клавиатура), прижимается к нулю
        // окна — под статусбар.
        let origin = crate::viewport::viewport_origin();
        let x = origin.x + clamp_span(x - origin.x, width, viewport.width);
        let y = origin.y + fit_span(y - origin.y, height, flip_up_to - origin.y, viewport.height);

        let max_available = (origin.y + viewport.height - y).max(0.0);
        let final_height = height.min(max_available);

        Rect::new(Point::new(x, y), Size::new(width, final_height))
    }

    fn content_width_limit(&self) -> f32 {
        let viewport = self.viewport_size.get();
        let limit = self.max_width.unwrap_or(self.min_width);
        if viewport.width > 0.0 {
            limit.min(viewport.width)
        } else {
            limit
        }
    }

    fn placed_rect(&self) -> Rect {
        let placed = self.placed_rect.get();
        if placed.size.width > 0.0 {
            placed
        } else {
            self.panel_rect()
        }
    }
}

impl Element for PopupPanelElement {
    fn update(&mut self, widget: &dyn Widget, _ctx: &mut UpdateContext) {
        if let Some(p) = widget.as_any().downcast_ref::<PopupPanel>() {
            self.is_open = p.is_open;
            self.anchor_rect = p.anchor_rect;
            self.anchor = p.anchor;
            self.min_width = p.min_width;
            self.max_width = p.max_width;
            self.max_height = p.max_height;
            self.on_close = p.on_close.clone();
            self.reveal = p.reveal;
            self.is_open.subscribe_element(self.id);
            self.mark_dirty(DirtyFlags::RENDER | DirtyFlags::LAYOUT);
        }
    }

    fn layout(&mut self, constraints: Constraints) -> Size {
        let w = if constraints.max_width.is_finite() {
            constraints.max_width
        } else {
            0.0
        };
        let h = if constraints.max_height.is_finite() {
            constraints.max_height
        } else {
            0.0
        };
        self.bounds = Rect::new(Point::zero(), Size::new(w, h));
        self.sync_reveal();
        Size::zero()
    }

    fn on_signal_change(&mut self) -> bool {
        self.sync_reveal()
    }

    fn animate(&mut self, dt: Duration) -> bool {
        self.sync_reveal();
        let Some(anim) = self.reveal_anim.as_mut() else { return false };
        let running = anim.tick(dt);
        self.shown = anim.current_value();
        if !running {
            self.reveal_anim = None;
            self.shown = if self.was_open { 1.0 } else { 0.0 };
        }
        self.mark_dirty(DirtyFlags::RENDER);
        running
    }

    fn needs_repaint(&self) -> bool {
        self.reveal_anim.is_some()
    }

    fn child_at_position(&self, _pos: Point) -> crate::widget::ChildHit {
        // Уезжающая панель ввод не принимает.
        if self.is_open() {
            crate::widget::ChildHit::Unknown
        } else {
            crate::widget::ChildHit::None
        }
    }

    fn is_relayout_boundary(&self) -> bool {
        true
    }

    fn intercepts_child_events(&self) -> bool {
        !self.is_open()
    }

    fn layout_hint(&self) -> LayoutHint {
        let panel = self.panel_rect();
        self.placed_rect.set(panel);
        let pad = self.padding();
        LayoutHint::FloatingWindow {
            x: panel.origin.x + pad[0],
            y: panel.origin.y + pad[1],
        }
    }

    fn is_visible(&self) -> bool {
        self.is_shown()
    }

    fn explicit_dimensions(
        &self,
        _parent_width: f32,
        _parent_height: f32,
    ) -> (Option<f32>, Option<f32>) {
        let pad = self.padding();
        (
            Some((self.content_width_limit() - pad[0] - pad[2]).max(0.0)),
            Some((self.max_height - pad[1] - pad[3]).max(0.0)),
        )
    }

    fn set_content_size(&mut self, size: Size) {
        self.content_size.set(size);
    }

    fn set_viewport_size(&mut self, size: Size) {
        self.viewport_size.set(size);
    }

    fn build_display_list(&self, list: &mut DisplayList, _clip: Rect) {
        if !self.is_shown() {
            list.push_clip(Rect::zero());
            return;
        }

        list.begin_overlay_absolute();

        let bg = self.mss.background_color.unwrap_or(Color::WHITE);
        let radii = self.border_radius();

        let panel = self.revealed_rect(self.placed_rect());
        let revealing = self.reveal.is_some() && (self.shown - 1.0).abs() > 1e-3;
        if revealing {
            // Вместе с выездом панель проявляется: в начале движения узкая
            // полоска со скруглениями не мелькает плотным пятном.
            list.push_opacity((self.shown * 1.6).clamp(0.0, 1.0));
        }

        // Тень и рамка — из MSS, если заданы (`box-shadow`, `border-width:
        // 0` убирает рамку), иначе прежние: мягкая тень и рамка в 1 px.
        match &self.mss.box_shadow {
            Some(shadows) => {
                for sh in shadows.0.iter().filter(|sh| !sh.inset) {
                    list.push_shadow(panel, sh.color, sh.blur_radius, (sh.offset_x, sh.offset_y), radii);
                }
            }
            None => list.push_shadow(panel, Color::new(0.0, 0.0, 0.0, 0.15), 16.0, (0.0, 4.0), radii),
        }
        let border_width = self.mss.border_width.unwrap_or(1.0);
        if self.mss.flow_edge.is_some() {
            // Панель перетекает в соседнюю поверхность (`flow-edge`): фон и
            // «ушки» одним жёстким многоугольником по целым пикселям, без шва.
            let o = panel.origin;
            let e = Point::new(o.x + panel.size.width, o.y + panel.size.height);
            let snapped = Rect::new(
                Point::new(o.x.round(), o.y.round()),
                Size::new(e.x.round() - o.x.round(), e.y.round() - o.y.round()),
            );
            self.mss.paint_flow_box(list, snapped, bg, radii);
        } else if border_width > 0.0 {
            let border_color = self.mss.border_color.unwrap_or(Color::from_hex("#E5E7EB"));
            list.push_rect_bordered(panel, bg, radii, Border { width: border_width, color: border_color });
        } else {
            list.push_rect(panel, bg, radii);
        }

        list.push_clip(panel);
    }

    fn post_build_display_list(&self, list: &mut DisplayList, _clip: Rect) {
        if !self.is_shown() {
            list.pop_clip();
            return;
        }
        list.pop_clip();
        if self.reveal.is_some() && (self.shown - 1.0).abs() > 1e-3 {
            list.pop_opacity();
        }
        list.end_overlay();
    }

    fn handle_event(&mut self, event: &Event, ctx: &mut EventContext) -> EventResult {
        self.sync_reveal();
        let is_open = self.is_open();

        if is_open && !self.overlay_registered {
            let overlay_bounds = Rect::new(Point::zero(), self.viewport_size.get());
            ctx.register_overlay(overlay_bounds, true);
            self.overlay_registered = true;
            ctx.request_layout();
            ctx.request_paint();
        } else if !is_open && self.overlay_registered {
            ctx.unregister_overlay();
            self.overlay_registered = false;
            ctx.request_layout();
            ctx.request_paint();
        }

        if !is_open {
            return EventResult::Ignored;
        }

        let panel = self.placed_rect();

        match event {
            Event::MouseDown { button, position } => {
                if *button == MouseButton::Left && !panel.contains(*position) {
                    self.close(ctx);
                    return EventResult::Handled;
                }
                EventResult::Ignored
            }
            Event::KeyDown(crate::input::Key::Escape) | Event::BackPressed => {
                self.close(ctx);
                EventResult::Handled
            }
            _ => EventResult::Ignored,
        }
    }

    fn children(&self) -> &[ElementId] {
        &self.child_ids
    }

    fn as_any_mut(&mut self) -> Option<&mut dyn Any> {
        Some(self)
    }
    fn bounds(&self) -> Rect {
        self.bounds
    }

    fn hit_test(&self, _point: Point) -> bool {
        self.is_open()
    }

    fn overlay_request(&self) -> Option<(Rect, bool)> {
        if !self.is_open() {
            return None;
        }
        let viewport = self.viewport_size.get();
        if viewport.width <= 0.0 || viewport.height <= 0.0 {
            return None;
        }
        Some((Rect::new(Point::zero(), viewport), true))
    }

    fn set_position(&mut self, pos: Point) {
        self.bounds.origin = pos;
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

    fn mount(&mut self, _tree: &mut ElementTree) {
        self.is_open.subscribe_element(self.id);
    }

    fn set_classes(&mut self, classes: Vec<String>) {
        self.classes = classes;
        self.mark_dirty(DirtyFlags::RENDER);
    }

    fn get_classes(&self) -> &[String] {
        &self.classes
    }
    fn element_type_name(&self) -> &str {
        "PopupPanel"
    }

    fn reset_mss_styles(&mut self) {
        self.mss.reset();
    }
    fn mss(&self) -> Option<&crate::mss::MssFields> {
        Some(&self.mss)
    }

    fn apply_computed_style(&mut self, style: &ComputedStyle) {
        self.mss.apply(style);
        let ts = crate::animation::TransitionState::parse_from_style(style);
        self.reveal_transition = ["size", "width", "height"]
            .iter()
            .find_map(|p| ts.spec_for(p))
            .map(|sp| ((sp.duration_secs * 1000.0).round() as u32, sp.easing));
        self.mark_dirty(DirtyFlags::RENDER | DirtyFlags::LAYOUT);
    }

    fn apply_transition_styles(
        &mut self,
        base: &ComputedStyle,
        hover: Option<&ComputedStyle>,
        active: Option<&ComputedStyle>,
        focus: Option<&ComputedStyle>,
        selected: Option<&ComputedStyle>,
        _checked: Option<&ComputedStyle>,
    ) {
        self.mss
            .apply_transitions(base, hover, active, focus, selected);
    }

    fn accessibility_info(&self) -> Option<crate::a11y::AccessibilityInfo> {
        Some(crate::a11y::AccessibilityInfo {
            role: crate::a11y::Role::Group,
            state: crate::a11y::NodeState {
                hidden: !self.is_open(),
                ..Default::default()
            },
            properties: crate::a11y::NodeProperties::default(),
        })
    }
}

impl StyledElement for PopupPanelElement {
    fn apply_style(&mut self, _style: &ComputedStyle) {
        self.mark_dirty(DirtyFlags::RENDER | DirtyFlags::LAYOUT);
    }
    fn classes(&self) -> &[String] {
        &self.classes
    }
    fn set_classes(&mut self, classes: Vec<String>) {
        self.classes = classes;
        self.mark_dirty(DirtyFlags::RENDER);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TestHarness;
    use crate::widgets::{Column, Stack};

    fn with_el<R>(h: &mut TestHarness, f: impl FnOnce(&PopupPanelElement) -> R) -> R {
        let id = h.find_by_type_name("PopupPanel")[0];
        let el = h.tree.get_mut(id).unwrap();
        f(el.as_any_mut().unwrap().downcast_ref::<PopupPanelElement>().unwrap())
    }

    fn panel_rect(h: &mut TestHarness) -> Rect {
        with_el(h, |p| p.revealed_rect(p.placed_rect()))
    }

    fn shown(h: &mut TestHarness) -> f32 {
        with_el(h, |p| p.shown)
    }

    fn harness(open: RwSignal<bool>, anchor: RwSignal<Rect>) -> TestHarness {
        let panel = PopupPanel::new()
            .is_open(open)
            .anchor_rect(anchor)
            .anchor(PopupAnchor::EndCenter)
            .min_width(0.0)
            .max_width(400.0)
            .reveal(AnimationAxis::Width)
            .child(Column::new().width(200.0).height(80.0));
        TestHarness::new(Box::new(Stack::new().child(Column::new().width(800.0).height(600.0)).child(panel)))
    }

    /// Выезд справа от якоря по центру его высоты, уход по сигналу снаружи
    /// проигрывается до конца, и только потом панель скрывается.
    #[test]
    fn reveal_from_anchor_and_back() {
        let open = use_signal(false);
        let anchor = use_signal(Rect::new(Point::new(10.0, 300.0), Size::new(60.0, 40.0)));
        let mut h = harness(open, anchor);
        h.frame(None, 800.0, 600.0);
        let id = h.find_by_type_name("PopupPanel")[0];
        assert!(!h.tree.get(id).unwrap().is_visible());

        open.set(true);
        h.frame(None, 800.0, 600.0);
        assert!(h.is_animating(id), "выезд тикает");
        h.animate(Duration::from_millis(60));
        let mid = panel_rect(&mut h);
        assert!(mid.size.width > 0.0 && mid.size.width < 200.0, "наполовину: {mid:?}");
        assert_eq!(mid.origin.x, 70.0, "от правого края якоря");
        assert!((mid.origin.y + mid.size.height / 2.0 - 320.0).abs() < 0.5, "по центру якоря: {mid:?}");
        for _ in 0..20 {
            h.animate(Duration::from_millis(30));
        }
        assert_eq!(shown(&mut h), 1.0);
        assert_eq!(panel_rect(&mut h).size.width, 200.0);

        // Закрыли снаружи (выбор пункта): панель уезжает, не исчезая сразу.
        open.set(false);
        h.frame(None, 800.0, 600.0);
        assert!(h.is_animating(id), "уход тикает");
        h.animate(Duration::from_millis(40));
        assert!(h.tree.get(id).unwrap().is_visible(), "ещё видна, пока уезжает");
        assert!(shown(&mut h) < 1.0 && shown(&mut h) > 0.0);
        for _ in 0..20 {
            h.animate(Duration::from_millis(30));
        }
        assert_eq!(shown(&mut h), 0.0);
        assert!(!h.tree.get(id).unwrap().is_visible());
    }

    /// Тень и рамка из MSS: `box-shadow` и `border-width: 0` заменяют
    /// прежние жёсткие тень и рамку в 1 px.
    #[test]
    fn mss_border_zero_drops_border() {
        let open = use_signal(true);
        let anchor = use_signal(Rect::new(Point::new(10.0, 300.0), Size::new(60.0, 40.0)));
        let panel = PopupPanel::new()
            .is_open(open)
            .anchor_rect(anchor)
            .anchor(PopupAnchor::EndCenter)
            .class("fly")
            .child(Column::new().width(200.0).height(80.0));
        let mut h = TestHarness::new(Box::new(Stack::new().child(Column::new().width(800.0).height(600.0)).child(panel)));
        h.apply_mss(".fly { background-color: #336699; border-width: 0px; }");
        h.frame(None, 800.0, 600.0);
        let list = h.paint();
        let bordered = list
            .iter_all_commands()
            .filter(|c| matches!(c, crate::render::DrawCommand::Rect { border: Some(_), .. }))
            .count();
        assert_eq!(bordered, 0, "без рамки");
        assert!(
            list.iter_all_commands().any(|c| matches!(c, crate::render::DrawCommand::Rect { border: None, .. })),
            "фон нарисован"
        );
    }

    /// `BottomCenter`: панель под якорем по центру его ширины и выезжает
    /// вниз по высоте.
    #[test]
    fn bottom_center_reveals_down() {
        let open = use_signal(false);
        let anchor = use_signal(Rect::new(Point::new(300.0, 100.0), Size::new(100.0, 36.0)));
        let panel = PopupPanel::new()
            .is_open(open)
            .anchor_rect(anchor)
            .anchor(PopupAnchor::BottomCenter)
            .min_width(0.0)
            .max_width(400.0)
            .reveal(AnimationAxis::Height)
            .child(Column::new().width(240.0).height(120.0));
        let mut h = TestHarness::new(Box::new(Stack::new().child(Column::new().width(800.0).height(600.0)).child(panel)));
        h.frame(None, 800.0, 600.0);
        open.set(true);
        h.frame(None, 800.0, 600.0);
        h.animate(Duration::from_millis(60));
        let mid = panel_rect(&mut h);
        assert_eq!(mid.origin.x, 230.0, "по центру якоря: {mid:?}");
        assert_eq!(mid.origin.y, 136.0, "под якорем: {mid:?}");
        assert!(mid.size.height > 0.0 && mid.size.height < 120.0, "выезжает: {mid:?}");
        assert_eq!(mid.size.width, 240.0);
        for _ in 0..20 {
            h.animate(Duration::from_millis(30));
        }
        assert_eq!(panel_rect(&mut h).size.height, 120.0);
    }
}
