use super::AppHandler;
use crate::mss::cascade;
use crate::widget::ElementId;

impl AppHandler {
    pub(super) fn apply_styles(&mut self, _root_id: ElementId) {
        let _apply_start = web_time::Instant::now();
        crate::perf::incr(crate::perf::Counter::ApplyStylesCall);

        let applied = cascade::apply_styles_dirty(&mut self.win.tree, &self.style_engine);

        if applied {
            if let Some(root_id) = self.win.root_id {
                if let Some(node) = self.win.tree.elements.get(&root_id) {
                    if let Some(mss) = node.element.mss() {
                        if let Some(bg) = mss.background_color {
                            self.win.background_color = bg;
                        }
                    }
                }
            }
        }

        crate::perf::add_time(crate::perf::TimeKind::ApplyStyles, _apply_start.elapsed());
    }

    pub(in crate::app) fn logical_surface_size(&self) -> crate::core::Size {
        crate::core::Size::new(
            self.win.width as f32 / self.win.scale_factor as f32,
            self.win.height as f32 / self.win.scale_factor as f32,
        )
    }
}
