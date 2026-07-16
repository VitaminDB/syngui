use crate::mss::{StyleEngine, cascade};
use crate::widget::{ElementTree, ElementId};
use super::AppHandler;

impl AppHandler {
    pub(super) fn apply_styles_to_tree(tree: &mut ElementTree, style_engine: &StyleEngine) {
        cascade::apply_styles_to_tree(tree, style_engine);
    }

    pub(super) fn apply_styles(&mut self, _root_id: ElementId) {
        let _apply_start = web_time::Instant::now();
        crate::perf::incr(crate::perf::Counter::ApplyStylesCall);

        let applied = cascade::apply_styles_dirty(&mut self.tree, &self.style_engine);

        if applied {
            if let Some(root_id) = self.root_id {
                if let Some(node) = self.tree.elements.get(&root_id) {
                    if let Some(mss) = node.element.mss() {
                        if let Some(bg) = mss.background_color {
                            self.config.background_color = bg;
                        }
                    }
                }
            }
        }

        crate::perf::add_time(crate::perf::TimeKind::ApplyStyles, _apply_start.elapsed());
    }

    pub(in crate::app) fn logical_surface_size(&self) -> crate::core::Size {
        crate::core::Size::new(
            self.config.width as f32 / self.scale_factor as f32,
            self.config.height as f32 / self.scale_factor as f32,
        )
    }
}
