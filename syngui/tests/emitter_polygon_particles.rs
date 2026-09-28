//! Полигональные частицы (сердечки, звёзды) рисуются там, где эмиттер, а
//! не со сдвигом на его положение ещё раз.
use std::time::Duration;
use syngui::prelude::*;
use syngui::render::DrawCommand;
use syngui::testing::*;
use syngui::widgets::{ParticleEmitter, ParticlePreset};

#[test]
fn polygon_particles_stay_near_emitter() {
    let token = use_signal(0u32);
    let root: Box<dyn Widget> = Box::new(
        Row::new()
            .child(DecoratedBox::new().class("sp"))
            .child(move || {
                ParticleEmitter::new()
                    .preset(ParticlePreset::Hearts)
                    .burst_token(token.get())
                    .child(DecoratedBox::new().class("em"))
            }),
    );
    let mut h = TestHarness::new(root);
    h.rebuild();
    let engine = h.apply_mss(".sp { width: 300px; height: 40px; } .em { width: 120px; height: 40px; }");
    h.layout(800.0, 600.0);
    let sp = h.find_by_class("sp")[0];
    assert_eq!(h.element_bounds(sp).size.width, 300.0);
    token.set(1);
    h.rebuild();
    h.apply_styles_dirty(&engine);
    h.layout(800.0, 600.0);
    h.animate(Duration::from_millis(16));
    h.animate(Duration::from_millis(100));
    let list = h.paint();
    let mut n = 0;
    for c in list.commands() {
        if let DrawCommand::Canvas { vertices, .. } = c {
            for v in vertices {
                n += 1;
                let (x, y) = (v.position[0], v.position[1]);
                assert!((180.0..=560.0).contains(&x) && (-200.0..=200.0).contains(&y), "частица далеко от эмиттера: ({x}, {y})");
            }
        }
    }
    assert!(n > 0, "сердечки должны дать геометрию");
}
