//! Проверка пера: окно, где перо рисует (толщина — по давлению, ластик —
//! красным), а события печатаются в stderr.
//! `cargo run --example pen_probe` (на Wayland с tablet-v2; без него — мышь).
use std::any::Any;

use syngui::layout::Constraints;
use syngui::prelude::*;
use syngui::render::DisplayList;
use syngui::widget::context::{EventContext, UpdateContext};
use syngui::widget::{DirtyFlags, Element, ElementId, ElementTree};

struct Probe;

impl Widget for Probe {
    fn create_element(&self) -> Box<dyn Element> {
        Box::new(ProbeEl { id: ElementId::new(), bounds: Rect::zero(), dots: Vec::new(), dirty: DirtyFlags::all() })
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
    fn mount(&self, _: &mut ElementTree, _: ElementId) {}
}

struct ProbeEl {
    id: ElementId,
    bounds: Rect,
    dots: Vec<(Point, f32, bool)>,
    dirty: DirtyFlags,
}

impl Element for ProbeEl {
    fn update(&mut self, _: &dyn Widget, _: &mut UpdateContext) {}
    fn layout(&mut self, c: Constraints) -> Size {
        self.bounds.size = Size::new(c.max_width, c.max_height);
        self.bounds.size
    }
    fn build_display_list(&self, list: &mut DisplayList, _clip: Rect) {
        list.push_rect(self.bounds, Color::rgb(1.0, 1.0, 1.0), [0.0; 4]);
        for &(p, w, eraser) in &self.dots {
            let c = if eraser { Color::rgb(0.9, 0.1, 0.1) } else { Color::rgb(0.0, 0.0, 0.0) };
            let r = Rect::new(Point::new(p.x - w / 2.0, p.y - w / 2.0), Size::new(w, w));
            list.push_rect(r, c, [w / 2.0; 4]);
        }
    }
    fn handle_event(&mut self, event: &Event, ctx: &mut EventContext) -> EventResult {
        match event {
            Event::Pen { phase, sample } => {
                eprintln!(
                    "pen {phase:?} ({:.1}, {:.1}) p={:.3} tilt=({:.0},{:.0}) {:?} barrel={} t={}",
                    sample.position.x,
                    sample.position.y,
                    sample.pressure,
                    sample.tilt_x,
                    sample.tilt_y,
                    sample.tool,
                    sample.barrel,
                    sample.time_ms
                );
                if matches!(phase, PenPhase::Down | PenPhase::Move) {
                    let w = 1.0 + sample.pressure * 8.0;
                    self.dots.push((sample.position, w, sample.tool == PenTool::Eraser));
                    syngui::widget::context::EventContextExt::mark_dirty(ctx, DirtyFlags::RENDER);
                }
                // Наведение не берём — проверка запасного пути мыши.
                if *phase == PenPhase::Hover {
                    EventResult::Ignored
                } else {
                    EventResult::Handled
                }
            }
            Event::MouseDown { button, position } => {
                eprintln!("mouse down {button:?} {position:?}");
                EventResult::Handled
            }
            _ => EventResult::Ignored,
        }
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
    fn mark_dirty(&mut self, f: DirtyFlags) {
        self.dirty |= f;
    }
    fn clear_dirty(&mut self, f: DirtyFlags) {
        self.dirty.remove(f);
    }
    fn is_dirty(&self, f: DirtyFlags) -> bool {
        self.dirty.contains(f)
    }
    fn id(&self) -> ElementId {
        self.id
    }
    fn set_id(&mut self, id: ElementId) {
        self.id = id;
    }
    fn mount(&mut self, _: &mut ElementTree) {}
}

impl syngui::widget::StyledElement for ProbeEl {
    fn apply_style(&mut self, _: &syngui::mss::ComputedStyle) {}
    fn classes(&self) -> &[String] {
        &[]
    }
    fn set_classes(&mut self, _: Vec<String>) {}
}

fn main() {
    // PEN_PROBE_FULLSCREEN=1 — во весь экран (перо попадает в окно в любой точке).
    let full = std::env::var_os("PEN_PROBE_FULLSCREEN").is_some();
    App::new()
        .title("pen probe")
        .app_id("syngui-pen-probe")
        .size(800, 600)
        .fullscreen(full)
        .run(|_| Box::new(Probe));
}
