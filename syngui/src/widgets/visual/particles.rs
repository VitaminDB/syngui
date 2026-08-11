//! Система частиц: конфетти / искры / фейерверк.
//!
//! Переиспользуемый виджет-оверлей. В отличие от [`Canvas`](super::Canvas),
//! держит изменяемое состояние частиц и симулирует его в `animate(dt)`.
//! Всплеск запускается изменением `burst_token` (идиома «инкремент счётчика»).

use crate::core::canvas::CanvasContext;
use crate::core::{Color, Point, Rect, Size};
use crate::input::{Event, EventResult};
use crate::layout::Constraints;
use crate::mss::{ComputedStyle, MssFields};
use crate::render::DisplayList;
use crate::widget::{
    DirtyFlags, Element, ElementId, ElementTree, StyledElement, UpdateContext, Widget,
};
use std::any::Any;

/// Тип эмиттера.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EmitKind {
    /// Падающие сверху цветные прямоугольники/кружки.
    Confetti,
    /// Разлетающиеся из центра искры.
    Sparks,
    /// Взрыв из центра вверх с гравитацией.
    Fireworks,
}

/// Форма частицы.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PShape {
    Circle,
    Rect,
    Triangle,
}

fn default_palette() -> Vec<Color> {
    vec![
        Color::from_hex("#2fbf71"),
        Color::from_hex("#f4a259"),
        Color::from_hex("#4c9be8"),
        Color::from_hex("#ef6f6c"),
        Color::from_hex("#f7d154"),
        Color::from_hex("#a86ff0"),
    ]
}

/// Виджет-оверлей частиц.
pub struct ParticleSystem {
    burst_token: u32,
    kind: EmitKind,
    palette: Vec<Color>,
    count: usize,
}

impl ParticleSystem {
    /// Оверлей конфетти. Всплеск — при изменении `burst_token`.
    pub fn new(burst_token: u32) -> Self {
        Self {
            burst_token,
            kind: EmitKind::Confetti,
            palette: default_palette(),
            count: 120,
        }
    }

    pub fn confetti(burst_token: u32) -> Self {
        Self::new(burst_token).kind(EmitKind::Confetti).count(120)
    }

    pub fn sparks(burst_token: u32) -> Self {
        Self::new(burst_token).kind(EmitKind::Sparks).count(48)
    }

    pub fn fireworks(burst_token: u32) -> Self {
        Self::new(burst_token).kind(EmitKind::Fireworks).count(90)
    }

    pub fn kind(mut self, kind: EmitKind) -> Self {
        self.kind = kind;
        self
    }

    pub fn count(mut self, count: usize) -> Self {
        self.count = count;
        self
    }

    pub fn palette(mut self, palette: Vec<Color>) -> Self {
        if !palette.is_empty() {
            self.palette = palette;
        }
        self
    }
}

impl Widget for ParticleSystem {
    fn create_element(&self) -> Box<dyn Element> {
        Box::new(ParticleElement {
            id: ElementId::new(),
            kind: self.kind,
            palette: self.palette.clone(),
            count: self.count,
            burst_token: self.burst_token,
            last_token: self.burst_token, // не всплескиваем на первом кадре
            pending: false,
            particles: Vec::new(),
            bounds: Rect::zero(),
            rng: 0x9E3779B9 ^ (self.burst_token.wrapping_mul(2654435761)),
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

#[derive(Clone, Copy)]
struct Particle {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    color: Color,
    size: f32,
    rot: f32,
    vrot: f32,
    life: f32,
    max_life: f32,
    shape: PShape,
}

struct ParticleElement {
    id: ElementId,
    kind: EmitKind,
    palette: Vec<Color>,
    count: usize,
    burst_token: u32,
    last_token: u32,
    pending: bool,
    particles: Vec<Particle>,
    bounds: Rect,
    rng: u32,
    dirty_flags: DirtyFlags,
    mss: MssFields,
}

impl ParticleElement {
    /// xorshift32 → f32 в [0, 1).
    fn rand(&mut self) -> f32 {
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.rng = x;
        (x >> 8) as f32 / (1u32 << 24) as f32
    }

    fn rand_range(&mut self, a: f32, b: f32) -> f32 {
        a + (b - a) * self.rand()
    }

    fn spawn(&mut self) {
        let w = self.bounds.size.width.max(1.0);
        let h = self.bounds.size.height.max(1.0);
        self.particles.clear();
        self.particles.reserve(self.count);
        let palette = self.palette.clone();
        for _ in 0..self.count {
            let color = palette[(self.rand() * palette.len() as f32) as usize % palette.len()];
            let shape = match (self.rand() * 3.0) as u32 {
                0 => PShape::Circle,
                1 => PShape::Rect,
                _ => PShape::Triangle,
            };
            let size = self.rand_range(5.0, 11.0);
            let max_life = self.rand_range(1.4, 2.6);
            let (x, y, vx, vy) = match self.kind {
                EmitKind::Confetti => (
                    self.rand_range(0.0, w),
                    self.rand_range(-h * 0.15, 0.0),
                    self.rand_range(-60.0, 60.0),
                    self.rand_range(40.0, 160.0),
                ),
                EmitKind::Sparks => {
                    let ang = self.rand_range(0.0, std::f32::consts::TAU);
                    let sp = self.rand_range(120.0, 340.0);
                    (w * 0.5, h * 0.5, ang.cos() * sp, ang.sin() * sp)
                }
                EmitKind::Fireworks => {
                    let ang = self.rand_range(-std::f32::consts::PI, 0.0);
                    let sp = self.rand_range(160.0, 380.0);
                    (w * 0.5, h * 0.45, ang.cos() * sp, ang.sin() * sp)
                }
            };
            let rot = self.rand_range(0.0, std::f32::consts::TAU);
            let vrot = self.rand_range(-6.0, 6.0);
            self.particles.push(Particle {
                x,
                y,
                vx,
                vy,
                color,
                size,
                rot,
                vrot,
                life: max_life,
                max_life,
                shape,
            });
        }
    }

    fn gravity(&self) -> f32 {
        match self.kind {
            EmitKind::Confetti => 220.0,
            EmitKind::Sparks => 120.0,
            EmitKind::Fireworks => 320.0,
        }
    }
}

impl Element for ParticleElement {
    fn update(&mut self, widget: &dyn Widget, _ctx: &mut UpdateContext) {
        if let Some(w) = widget.as_any().downcast_ref::<ParticleSystem>() {
            self.kind = w.kind;
            self.palette = w.palette.clone();
            self.count = w.count;
            self.burst_token = w.burst_token;
            if w.burst_token != self.last_token {
                self.last_token = w.burst_token;
                self.pending = true; // фактический спавн — в animate, когда есть bounds
            }
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
        self.bounds = Rect::new(self.bounds.origin, Size::new(w, h));
        Size::new(w, h)
    }

    fn build_display_list(&self, list: &mut DisplayList, _clip: Rect) {
        if self.particles.is_empty() {
            return;
        }
        let mut ctx = CanvasContext::new(self.bounds.origin, self.bounds.size);
        for p in &self.particles {
            let a = (p.life / p.max_life).clamp(0.0, 1.0);
            ctx.set_color(p.color.with_alpha(a));
            match p.shape {
                PShape::Circle => ctx.fill_circle(p.x, p.y, p.size * 0.5),
                PShape::Rect => {
                    let s = p.size;
                    // Прямоугольник со «вращением» через 4 повёрнутые вершины.
                    let (c, si) = (p.rot.cos(), p.rot.sin());
                    let hw = s * 0.5;
                    let hh = s * 0.32;
                    let corners = [
                        (-hw, -hh),
                        (hw, -hh),
                        (hw, hh),
                        (-hw, hh),
                    ];
                    let pts: Vec<(f32, f32)> = corners
                        .iter()
                        .map(|&(dx, dy)| (p.x + dx * c - dy * si, p.y + dx * si + dy * c))
                        .collect();
                    ctx.fill_polygon(&pts);
                }
                PShape::Triangle => {
                    let s = p.size;
                    let (c, si) = (p.rot.cos(), p.rot.sin());
                    let corners = [(0.0, -s * 0.6), (s * 0.55, s * 0.5), (-s * 0.55, s * 0.5)];
                    let pts: Vec<(f32, f32)> = corners
                        .iter()
                        .map(|&(dx, dy)| (p.x + dx * c - dy * si, p.y + dx * si + dy * c))
                        .collect();
                    ctx.fill_polygon(&pts);
                }
            }
        }
        ctx.flush(list);
    }

    fn animate(&mut self, dt: std::time::Duration) -> bool {
        let dt = dt.as_secs_f32().min(0.05);

        if self.pending && self.bounds.size.width > 1.0 {
            self.spawn();
            self.pending = false;
        }

        if self.particles.is_empty() {
            return self.pending; // ждём bounds, если спавн отложен
        }

        let g = self.gravity();
        let h = self.bounds.size.height;
        for p in &mut self.particles {
            p.vy += g * dt;
            p.vx *= 1.0 - 0.6 * dt; // сопротивление
            p.x += p.vx * dt;
            p.y += p.vy * dt;
            p.rot += p.vrot * dt;
            p.life -= dt;
        }
        self.particles
            .retain(|p| p.life > 0.0 && p.y < h + 40.0);

        !self.particles.is_empty()
    }

    fn handle_event(&mut self, _event: &Event, _ctx: &mut crate::widget::context::EventContext) -> EventResult {
        EventResult::Ignored
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

    fn passthrough_hit_test(&self) -> bool {
        true
    }

    fn clip_content(&self) -> bool {
        false
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

    fn mount(&mut self, _tree: &mut ElementTree) {}

    fn set_classes(&mut self, _classes: Vec<String>) {}

    fn get_classes(&self) -> &[String] {
        &[]
    }

    fn reset_mss_styles(&mut self) {
        self.mss.reset();
    }

    fn mss(&self) -> Option<&MssFields> {
        Some(&self.mss)
    }

    fn apply_computed_style(&mut self, style: &ComputedStyle) {
        self.mss.apply(style);
    }
}

impl StyledElement for ParticleElement {
    fn apply_style(&mut self, _style: &ComputedStyle) {
        self.mark_dirty(DirtyFlags::RENDER);
    }

    fn classes(&self) -> &[String] {
        &[]
    }

    fn set_classes(&mut self, _classes: Vec<String>) {}
}
