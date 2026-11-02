//! iOS: безопасная область (вырез камеры, статусбар, полоска «домой»).
//!
//! winit на iOS отдаёт `inner_*` — прямоугольник safe area, `outer_*` — весь
//! экран; поверхность Metal занимает весь экран (см. `Window::size`), а
//! отступы между ними уходят в `tree.safe_area`, как на Android.

use super::AppHandler;

impl AppHandler {
    pub(in crate::app) fn update_safe_area(&mut self) {
        let Some(window) = self.window.as_ref() else {
            return;
        };
        let w = window.winit_window();
        let (Ok(inner), Ok(outer)) = (w.inner_position(), w.outer_position()) else {
            return;
        };
        let (is, os) = (w.inner_size(), w.outer_size());
        let sf = self.effective_scale_factor() as f32;
        let left = (inner.x - outer.x).max(0) as f32;
        let top = (inner.y - outer.y).max(0) as f32;
        let right = (os.width as f32 - is.width as f32 - left).max(0.0);
        let bottom = (os.height as f32 - is.height as f32 - top).max(0.0);
        let insets = crate::core::EdgeInsets::new(left / sf, top / sf, right / sf, bottom / sf);
        log::info!(
            "безопасная область: слева {:.0}, сверху {:.0}, справа {:.0}, снизу {:.0} (масштаб {:.3})",
            insets.left,
            insets.top,
            insets.right,
            insets.bottom,
            sf
        );
        self.tree.safe_area = insets;
    }
}
