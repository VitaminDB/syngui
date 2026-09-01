use std::sync::atomic::{AtomicU64, Ordering};
use crate::core::{Rect, Size, Point};
use crate::input::{CursorIcon, DragData};
use crate::widget::{DirtyFlags, Element, Widget};
use crate::widget::context::UpdateContext;

mod layout;
mod event;
mod render;
mod overlay;
mod storage;

pub(crate) use storage::ElementStorage;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct ElementId(pub u64);

impl ElementId {
    pub fn new() -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        ElementId(COUNTER.fetch_add(1, Ordering::Relaxed) + 1)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct RenderHandle(pub u64);

pub(crate) struct ElementNode {
    pub(crate) element: Box<dyn Element>,
    pub(crate) parent: Option<ElementId>,
    pub(crate) children: Vec<ElementId>,
    pub(crate) children_idx: Vec<u32>,
    pub(crate) id: ElementId,
    pub(crate) widget_type_id: std::any::TypeId,
    pub(crate) mss_margin: crate::core::EdgeInsets,
    pub(crate) mss_margin_set: bool,
    pub(crate) mss_flex_grow: f32,
    pub(crate) debug_name: Option<String>,
    pub(crate) inline_styles: Vec<(String, crate::mss::StyleValue)>,
    pub(crate) had_mss_rules: bool,
    pub(crate) styles_dirty: bool,
    pub(crate) hint_cache: crate::widget::LayoutHint,
}

impl ElementNode {
    pub(crate) fn effective_margin(&self) -> crate::core::EdgeInsets {
        if self.mss_margin_set {
            self.mss_margin
        } else {
            self.element.margin()
        }
    }

    #[inline]
    pub(crate) fn refresh_hint_cache(&mut self) {
        self.hint_cache = self.element.layout_hint();
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct LayoutCache {
    pub(crate) size: Size,
    pub(crate) constraints_hash: u64,
    pub(crate) visible: bool,
}

impl LayoutCache {
    pub(crate) fn empty() -> Self {
        Self {
            size: Size::zero(),
            constraints_hash: u64::MAX,
            visible: false,
        }
    }

    #[inline]
    pub(crate) fn is_empty_slot(&self) -> bool {
        self.constraints_hash == u64::MAX
    }
}

#[derive(Debug, Clone)]
pub struct OverlayEntry {
    pub element_id: ElementId,
    pub bounds: Rect,
    pub modal: bool,
    pub declarative: bool,
}

#[derive(Debug, Clone)]
pub struct DragState {
    pub data: DragData,
    pub start_pos: Point,
    pub current_pos: Point,
    pub hover_element: Option<ElementId>,
    pub drag_offset: Point,
    pub source_bounds: Rect,
}

pub struct ElementTree {
    pub(crate) elements: ElementStorage,
    pub(crate) root_id: Option<ElementId>,
    pub(crate) next_id: u64,
    pub(crate) layout_cache: Vec<LayoutCache>,
    pub(crate) indent_level: usize,
    pub(crate) overlay_stack: Vec<OverlayEntry>,
    pub drag_state: Option<DragState>,
    pub cursor_request: Option<CursorIcon>,
    pub text_measure: Option<std::sync::Arc<dyn crate::widget::context::TextMeasure>>,
    pub viewport_size: Size,
    pub(crate) pixel_snap_scale: f32,
    pub modifiers: crate::input::Modifiers,
    #[cfg(feature = "clipboard")]
    pub clipboard: Option<std::sync::Arc<crate::core::sync::Mutex<arboard::Clipboard>>>,
    pub image_store: Option<std::sync::Arc<crate::core::sync::Mutex<crate::gpu::image_store::ImageStore>>>,
    #[cfg(feature = "map")]
    pub tile_atlas: Option<std::sync::Arc<crate::core::sync::Mutex<crate::gpu::tile_atlas::TileAtlas>>>,
    pub(crate) last_constraints_hash: u64,
    pub virtual_keyboard_request: Option<bool>,
    pub keyboard_numeric: bool,
    pub window_drag_request: bool,
    pub window_resize_request: Option<crate::input::ResizeDirection>,
    pub window_close_request: bool,
    pub window_minimize_request: bool,
    pub window_toggle_maximize_request: bool,
    pub window_toggle_fullscreen_request: bool,
    pub window_hide_request: bool,
    pub window_show_request: bool,
    pub window_toggle_visibility_request: bool,
    pub window_flags: u8,
    pub focused_text_content: Option<String>,
    pub focused_element: Option<ElementId>,
    pub safe_area: crate::core::EdgeInsets,
    pub root_offset: Point,
    pub(crate) drop_targets: Vec<ElementId>,
    pub(crate) layout_log_enabled: bool,
    pub(crate) last_mousedown_element: Option<ElementId>,
    pub(crate) scroll_cull_stack: Vec<ScrollCullContext>,
    pub(crate) force_full_measure: bool,
    pub(crate) animation_registry: std::collections::HashSet<ElementId>,
    /// «Взведено»: с последнего обхода animate() могли начаться анимации
    /// (были события, рендер или новые элементы). Пустой обход снимает флаг —
    /// в простое update() не гоняет O(все элементы) каждый тик.
    pub(crate) animations_armed: bool,
    pub(crate) rebuild_registry: std::collections::HashSet<ElementId>,
    pub(crate) last_hovered_path: Vec<ElementId>,
    pub(crate) mouse_captor: Option<ElementId>,
    pub(crate) post_layout_sync_registry: std::collections::HashSet<ElementId>,
}

#[derive(Clone, Debug)]
pub(crate) struct ScrollCullContext {
    pub viewport_height: f32,
    pub scroll_offset_y: f32,
}

impl ElementTree {
    pub fn new() -> Self {
        Self {
            elements: ElementStorage::new(),
            root_id: None,
            next_id: 1,
            layout_cache: Vec::new(),
            indent_level: 0,
            overlay_stack: Vec::new(),
            drag_state: None,
            cursor_request: None,
            text_measure: None,
            viewport_size: Size::new(1280.0, 720.0),
            pixel_snap_scale: 0.0,
            modifiers: crate::input::Modifiers::empty(),
            #[cfg(feature = "clipboard")]
            clipboard: None,
            image_store: None,
            #[cfg(feature = "map")]
            tile_atlas: None,
            last_constraints_hash: 0,
            virtual_keyboard_request: None,
            keyboard_numeric: false,
            window_drag_request: false,
            window_resize_request: None,
            window_close_request: false,
            window_minimize_request: false,
            window_toggle_maximize_request: false,
            window_toggle_fullscreen_request: false,
            window_hide_request: false,
            window_show_request: false,
            window_toggle_visibility_request: false,
            window_flags: 0,
            focused_text_content: None,
            focused_element: None,
            safe_area: crate::core::EdgeInsets::zero(),
            root_offset: Point::zero(),
            drop_targets: Vec::new(),
            layout_log_enabled: std::env::var("MGUI_LAYOUT_LOG").is_ok(),
            last_mousedown_element: None,
            scroll_cull_stack: Vec::new(),
            force_full_measure: false,
            animation_registry: std::collections::HashSet::new(),
            animations_armed: true,
            rebuild_registry: std::collections::HashSet::new(),
            last_hovered_path: Vec::new(),
            mouse_captor: None,
            post_layout_sync_registry: std::collections::HashSet::new(),
        }
    }

    pub(crate) fn log(&self, msg: String) {
        if std::env::var("MGUI_LAYOUT_LOG").is_ok() {
            let indent = "  ".repeat(self.indent_level);
            eprintln!("{}{}", indent, msg);
        }
    }

    pub fn root(&self) -> Option<&Box<dyn Element>> {
        self.root_id.and_then(|id| self.elements.get(&id).map(|n| &n.element))
    }

    pub fn set_pixel_snap_scale(&mut self, scale: f32) {
        self.pixel_snap_scale = scale;
    }

    #[inline]
    pub(crate) fn snap_point(&self, p: Point) -> Point {
        let sf = self.pixel_snap_scale;
        if sf <= 0.0 {
            return p;
        }
        Point::new((p.x * sf).round() / sf, (p.y * sf).round() / sf)
    }

    #[inline]
    pub(crate) fn cache_get(&self, id: &ElementId) -> Option<LayoutCache> {
        let idx = self.elements.resolve(*id)? as usize;
        let c = *self.layout_cache.get(idx)?;
        if c.is_empty_slot() { None } else { Some(c) }
    }

    #[inline]
    pub(crate) fn cache_get_by_idx(&self, idx: u32) -> Option<LayoutCache> {
        let c = *self.layout_cache.get(idx as usize)?;
        if c.is_empty_slot() { None } else { Some(c) }
    }

    pub(crate) fn cache_set_by_idx(&mut self, idx: u32, cache: LayoutCache) {
        let idx = idx as usize;
        if self.layout_cache.len() <= idx {
            self.layout_cache.resize(idx + 1, LayoutCache::empty());
        }
        self.layout_cache[idx] = cache;
    }

    pub(crate) fn cache_remove(&mut self, id: &ElementId) {
        let Some(idx) = self.elements.resolve(*id).map(|i| i as usize) else { return };
        if let Some(slot) = self.layout_cache.get_mut(idx) {
            *slot = LayoutCache::empty();
        }
    }

    pub(crate) fn cache_clear(&mut self) {
        for slot in self.layout_cache.iter_mut() {
            *slot = LayoutCache::empty();
        }
    }

    pub fn root_mut(&mut self) -> Option<&mut Box<dyn Element>> {
        self.root_id.and_then(|id| self.elements.get_mut(&id).map(|n| &mut n.element))
    }

    pub fn insert_widget(&mut self, widget: &dyn super::Widget, parent: Option<ElementId>) -> ElementId {
        let element = widget.create_element();
        let type_id = widget.as_any().type_id();
        let inline = widget.widget_inline_styles().to_vec();
        self.insert_with_type_id_and_inline(element, parent, type_id, inline)
    }

    pub fn set_node_inline_styles(&mut self, id: ElementId, styles: Vec<(String, crate::mss::StyleValue)>) {
        if let Some(node) = self.elements.get_mut(&id) {
            node.inline_styles = styles;
        }
    }

    pub fn insert(&mut self, element: Box<dyn Element>, parent: Option<ElementId>) -> ElementId {
        self.insert_with_type_id(element, parent, std::any::TypeId::of::<()>())
    }

    pub fn insert_with_type_id(
        &mut self,
        element: Box<dyn Element>,
        parent: Option<ElementId>,
        widget_type_id: std::any::TypeId,
    ) -> ElementId {
        self.insert_with_type_id_and_inline(element, parent, widget_type_id, Vec::new())
    }

    pub fn insert_with_type_id_and_inline(
        &mut self,
        mut element: Box<dyn Element>,
        parent: Option<ElementId>,
        widget_type_id: std::any::TypeId,
        inline_styles: Vec<(String, crate::mss::StyleValue)>,
    ) -> ElementId {
        let id = ElementId(self.next_id);
        self.next_id += 1;

        element.set_id(id);

        element.mount(self);

        let wants_focus = element.take_focus_request();

        let needs_anim = element.needs_repaint() || element.wants_animate_tick();
        let needs_rebuild = element.needs_rebuild();
        let manages_children = element.manages_own_children();
        let hint_cache = element.layout_hint();

        let node = ElementNode {
            element,
            parent,
            children: Vec::new(),
            children_idx: Vec::new(),
            id,
            widget_type_id,
            mss_margin: crate::core::EdgeInsets::default(),
            mss_margin_set: false,
            mss_flex_grow: 0.0,
            debug_name: None,
            inline_styles,
            had_mss_rules: false,
            styles_dirty: true,
            hint_cache,
        };

        self.elements.insert(id, node);
        if needs_anim {
            self.animation_registry.insert(id);
        }
        if manages_children {
            self.post_layout_sync_registry.insert(id);
        }
        if needs_rebuild {
            self.rebuild_registry.insert(id);
        }

        if let Some(parent_id) = parent {
            let child_idx = self.elements.resolve(id);
            if let Some(parent_node) = self.elements.get_mut(&parent_id) {
                parent_node.children.push(id);
                if let Some(cidx) = child_idx {
                    parent_node.children_idx.push(cidx);
                }
            }
        } else {
            self.root_id = Some(id);
        }

        if wants_focus {
            self.focused_element = Some(id);
        }

        id
    }

    /// Регистрирует элемент в реестре анимаций, если ему нужны тики.
    /// Вызывается из всех точек, где анимация могла стартовать: каскад стилей
    /// (keyframes/transitions), измерение и позиционирование (AnimatedSize,
    /// плавный скролл), scroll-into-view. animate() обходит только реестр.
    pub(crate) fn note_animation_started(&mut self, id: ElementId) {
        if let Some(node) = self.elements.get(&id) {
            if node.element.needs_repaint() || node.element.wants_animate_tick() {
                self.animation_registry.insert(id);
                self.animations_armed = true;
            }
        }
    }

    pub(crate) fn sync_registries_for(&mut self, id: ElementId) {
        if let Some(node) = self.elements.get(&id) {
            if node.element.needs_repaint() || node.element.wants_animate_tick() {
                self.animation_registry.insert(id);
            } else {
                self.animation_registry.remove(&id);
            }
            if node.element.needs_rebuild() {
                self.rebuild_registry.insert(id);
            } else {
                self.rebuild_registry.remove(&id);
            }
        } else {
            self.animation_registry.remove(&id);
            self.rebuild_registry.remove(&id);
        }
    }

    pub fn set_debug_name(&mut self, id: ElementId, name: String) {
        if let Some(node) = self.elements.get_mut(&id) {
            node.debug_name = Some(name);
        }
    }

    pub fn get_debug_name(&self, id: ElementId) -> Option<&str> {
        self.elements.get(&id).and_then(|n| n.debug_name.as_deref())
    }

    pub fn get(&self, id: ElementId) -> Option<&Box<dyn Element>> {
        self.elements.get(&id).map(|n| &n.element)
    }

    pub fn get_mut(&mut self, id: ElementId) -> Option<&mut Box<dyn Element>> {
        self.elements.get_mut(&id).map(|n| &mut n.element)
    }

    pub fn children_of(&self, id: ElementId) -> &[ElementId] {
        self.elements.get(&id).map_or(&[], |n| &n.children)
    }

    pub fn scroll_to_reveal(&mut self, _root_id: ElementId, touch_pos: crate::core::Point, visible_height: f32) {

        let result = self.find_scroll_container_at(_root_id, touch_pos, self.root_offset);
        let Some((scroll_id, abs_y)) = result else { return };

        let mut current_id = scroll_id;
        let mut current_abs_y = abs_y;

        loop {
            if let Some(node) = self.elements.get_mut(&current_id) {
                let bounds = node.element.bounds();
                let scroll_offset = node.element.scroll_offset();
                let remaining_visible = (visible_height - current_abs_y).max(0.0);

                if remaining_visible > 50.0 && node.element.is_scroll_container() {
                    let local_y = touch_pos.y - current_abs_y + scroll_offset.y;
                    let viewport_h = bounds.size.height.min(remaining_visible);
                    let visible_bottom = scroll_offset.y + viewport_h;
                    let margin = 80.0;

                    if local_y + margin > visible_bottom {
                        let target_y = local_y + margin - viewport_h;
                        node.element.ensure_visible(crate::core::Rect::new(
                            crate::core::Point::new(0.0, target_y + viewport_h - margin),
                            crate::core::Size::new(0.0, 40.0),
                        ));
                        return;
                    }
                }
            }

            let parent = self.elements.get(&current_id).and_then(|n| n.parent);
            match parent {
                Some(pid) => {
                    if let Some(pnode) = self.elements.get(&pid) {
                        current_abs_y -= pnode.element.bounds().origin.y;
                    }
                    current_id = pid;
                }
                None => return,
            }
        }
    }

    fn find_scroll_container_at(&self, id: ElementId, pos: crate::core::Point, parent_offset: crate::core::Point) -> Option<(ElementId, f32)> {
        let node = self.elements.get(&id)?;
        let bounds = node.element.bounds();
        let abs_x = parent_offset.x + bounds.origin.x;
        let abs_y = parent_offset.y + bounds.origin.y;
        let abs_bounds = crate::core::Rect::new(
            crate::core::Point::new(abs_x, abs_y),
            bounds.size,
        );

        if !abs_bounds.contains(pos) {
            return None;
        }

        let scroll_off = node.element.scroll_offset();
        let child_offset = crate::core::Point::new(
            abs_x - scroll_off.x,
            abs_y - scroll_off.y,
        );

        let children = node.children.clone();
        for &child_id in children.iter().rev() {
            if let Some(found) = self.find_scroll_container_at(child_id, pos, child_offset) {
                return Some(found);
            }
        }

        if node.element.is_scroll_container() {
            return Some((id, abs_y));
        }
        None
    }

    pub fn ensure_element_visible(&mut self, element_id: ElementId) -> bool {
        let element_bounds = match self.elements.get(&element_id) {
            Some(n) => n.element.bounds(),
            None => return false,
        };

        let mut current_id = element_id;
        loop {
            let parent_id = match self.elements.get(&current_id).and_then(|n| n.parent) {
                Some(pid) => pid,
                None => return false,
            };

            let is_scroll = self.elements.get(&parent_id)
                .map(|n| n.element.is_scroll_container())
                .unwrap_or(false);

            if is_scroll {
                let scroll_bounds = self.elements.get(&parent_id)
                    .map(|n| n.element.bounds())
                    .unwrap_or(crate::core::Rect::zero());
                let scroll_offset = self.elements.get(&parent_id)
                    .map(|n| n.element.scroll_offset())
                    .unwrap_or(crate::core::Point::zero());

                let content_y = element_bounds.origin.y - scroll_bounds.origin.y + scroll_offset.y;

                let child_rect = crate::core::Rect::new(
                    crate::core::Point::new(0.0, content_y),
                    crate::core::Size::new(0.0, element_bounds.size.height),
                );
                if let Some(parent_node) = self.elements.get_mut(&parent_id) {
                    let result = parent_node.element.ensure_visible(child_rect);
                    if result {
                        parent_node.element.mark_dirty(
                            DirtyFlags::RENDER | DirtyFlags::PAINT
                        );
                    }
                    // Плавный скролл — анимация на предке-контейнере.
                    self.note_animation_started(parent_id);
                    return result;
                }
                return false;
            }

            current_id = parent_id;
        }
    }

    pub fn move_element(&mut self, id: ElementId, new_parent: ElementId, index: usize) -> bool {
        if !self.elements.contains_key(&id) || !self.elements.contains_key(&new_parent) {
            return false;
        }
        let mut check = Some(new_parent);
        while let Some(check_id) = check {
            if check_id == id {
                return false;
            }
            check = self.elements.get(&check_id).and_then(|n| n.parent);
        }
        if let Some(old_parent_id) = self.elements.get(&id).and_then(|n| n.parent) {
            if let Some(old_parent) = self.elements.get_mut(&old_parent_id) {
                let pos = old_parent.children.iter().position(|c| *c == id);
                if let Some(p) = pos {
                    old_parent.children.remove(p);
                    if p < old_parent.children_idx.len() {
                        old_parent.children_idx.remove(p);
                    }
                }
                old_parent.element.mark_dirty(DirtyFlags::LAYOUT);
            }
        }
        let child_idx = self.elements.resolve(id);
        if let Some(new_parent_node) = self.elements.get_mut(&new_parent) {
            let idx = index.min(new_parent_node.children.len());
            new_parent_node.children.insert(idx, id);
            if let Some(cidx) = child_idx {
                new_parent_node.children_idx.insert(idx, cidx);
            }
            new_parent_node.element.mark_dirty(DirtyFlags::LAYOUT);
        }
        if let Some(node) = self.elements.get_mut(&id) {
            node.parent = Some(new_parent);
        }
        true
    }

    pub fn mark_dirty(&mut self, id: ElementId, flags: DirtyFlags) {
        if let Some(node) = self.elements.get_mut(&id) {
            node.element.mark_dirty(flags);
        }
    }

    pub fn mark_all_dirty(&mut self, flags: DirtyFlags) {
        for node in self.elements.values_mut() {
            node.element.mark_dirty(flags);
        }
    }

    pub fn set_window_flags(&mut self, flags: u8) -> bool {
        if self.window_flags == flags {
            return false;
        }
        self.window_flags = flags;
        for node in self.elements.values_mut() {
            node.styles_dirty = true;
        }
        true
    }

    pub fn set_root(&mut self, id: ElementId) {
        self.root_id = Some(id);
    }

    pub fn rebuild_if_needed(&mut self, _root_id: ElementId) -> bool {
        let mut any_rebuilt = false;

        for dirty_id in crate::signal::dirty_element_ids() {
            if self.elements.contains_key(&dirty_id) {
                self.rebuild_registry.insert(dirty_id);
            }
        }

        for _pass in 0..8 {
            if self.rebuild_registry.is_empty() {
                break;
            }
            let rebuild_ids: Vec<ElementId> = self.rebuild_registry.iter().copied().collect();
            self.rebuild_registry.clear();

            for id in rebuild_ids {
                let new_children = {
                    let node = match self.elements.get(&id) {
                        Some(n) => n,
                        None => {
                            crate::signal::clear_element_dirty(id);
                            continue;
                        }
                    };
                    if !node.element.needs_rebuild() {
                        crate::signal::clear_element_dirty(id);
                        continue;
                    }
                    node.element.build_children()
                };
                any_rebuilt = true;

                self.reconcile_children_of(id, &new_children);

                if let Some(node) = self.elements.get_mut(&id) {
                    node.element.clear_rebuild();
                    node.element.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
                    node.refresh_hint_cache();
                }
                self.sync_registries_for(id);
            }
        }
        any_rebuilt
    }

    fn reconcile_children_of(&mut self, parent_id: ElementId, new_widgets: &[Box<dyn Widget>]) {
        let old_child_ids: Vec<ElementId> = self.elements.get(&parent_id)
            .map(|n| n.children.clone())
            .unwrap_or_default();

        let new_len = new_widgets.len();
        let old_len = old_child_ids.len();
        let mut kept_ids: Vec<ElementId> = Vec::with_capacity(new_len);

        for i in 0..new_len {
            let new_widget = &new_widgets[i];
            let new_type_id = new_widget.as_any().type_id();

            if i < old_len {
                let old_id = old_child_ids[i];
                let old_type_matches = self.elements.get(&old_id)
                    .map(|n| n.widget_type_id == new_type_id)
                    .unwrap_or(false);

                if old_type_matches {
                    self.update_element(old_id, new_widget.as_ref());

                    let manages_children = self.elements.get(&old_id)
                        .map(|n| n.element.manages_own_children())
                        .unwrap_or(false);
                    if !manages_children {
                        let needs_own_rebuild = self.elements.get(&old_id)
                            .map(|n| n.element.needs_rebuild())
                            .unwrap_or(false);
                        if !needs_own_rebuild {
                            self.reconcile_children_ref(old_id, new_widget.child_widgets());
                        }
                    }

                    kept_ids.push(old_id);
                    continue;
                }
            }

            let child_element = new_widget.create_element();
            let inline = new_widget.widget_inline_styles().to_vec();
            let child_id = self.insert_with_type_id_and_inline(child_element, Some(parent_id), new_type_id, inline);
            let widget_classes = new_widget.widget_classes();
            if !widget_classes.is_empty() {
                if let Some(node) = self.elements.get_mut(&child_id) {
                    node.element.set_classes(widget_classes.to_vec());
                    node.styles_dirty = true;
                }
            }
            new_widget.mount(self, child_id);
            if let Some(node) = self.elements.get_mut(&parent_id) {
                if let Some(pos) = node.children.iter().position(|&c| c == child_id) {
                    node.children.remove(pos);
                    if pos < node.children_idx.len() {
                        node.children_idx.remove(pos);
                    }
                }
            }
            kept_ids.push(child_id);
        }

        for i in 0..old_len {
            if !kept_ids.contains(&old_child_ids[i]) {
                self.remove_subtree(old_child_ids[i]);
            }
        }

        let kept_idx: Vec<u32> = kept_ids.iter()
            .filter_map(|id| self.elements.resolve(*id))
            .collect();
        if let Some(node) = self.elements.get_mut(&parent_id) {
            node.children = kept_ids;
            node.children_idx = kept_idx;
        }
    }

    fn reconcile_children_ref(&mut self, parent_id: ElementId, new_child_widgets: Vec<&dyn Widget>) {
        let old_child_ids: Vec<ElementId> = self.elements.get(&parent_id)
            .map(|n| n.children.clone())
            .unwrap_or_default();

        let new_len = new_child_widgets.len();
        let old_len = old_child_ids.len();

        let structure_matches = new_len == old_len && (0..new_len).all(|i| {
            let new_type_id = new_child_widgets[i].as_any().type_id();
            self.elements.get(&old_child_ids[i])
                .map(|n| n.widget_type_id == new_type_id)
                .unwrap_or(false)
        });

        if !structure_matches {
            self.remove_children(parent_id);
            for new_widget in &new_child_widgets {
                let child_element = new_widget.create_element();
                let type_id = new_widget.as_any().type_id();
                let inline = new_widget.widget_inline_styles().to_vec();
                let child_id = self.insert_with_type_id_and_inline(child_element, Some(parent_id), type_id, inline);
                let widget_classes = new_widget.widget_classes();
                if !widget_classes.is_empty() {
                    if let Some(node) = self.elements.get_mut(&child_id) {
                        node.element.set_classes(widget_classes.to_vec());
                        node.styles_dirty = true;
                    }
                }
                new_widget.mount(self, child_id);
            }
            return;
        }

        for i in 0..new_len {
            let old_id = old_child_ids[i];
            let new_widget = new_child_widgets[i];

            self.update_element(old_id, new_widget);

            let manages_children = self.elements.get(&old_id)
                .map(|n| n.element.manages_own_children())
                .unwrap_or(false);
            if !manages_children {
                let needs_own_rebuild = self.elements.get(&old_id)
                    .map(|n| n.element.needs_rebuild())
                    .unwrap_or(false);
                if !needs_own_rebuild {
                    self.reconcile_children_ref(old_id, new_widget.child_widgets());
                }
            }
        }
    }

    fn update_element(&mut self, id: ElementId, widget: &dyn Widget) {
        let mut ctx = UpdateContext {
            element_id: id,
            needs_layout: false,
            needs_render: false,
        };
        let mut cascade_context_changed = false;

        if let Some(node) = self.elements.get_mut(&id) {
            node.element.update(widget, &mut ctx);

            let new_classes = widget.widget_classes();
            let old_classes = node.element.get_classes();
            if new_classes != old_classes {
                let classes = new_classes.to_vec();
                node.element.set_classes(classes);
                node.styles_dirty = true;
                ctx.needs_layout = true;
                ctx.needs_render = true;
                cascade_context_changed = true;
            }

            let new_inline = widget.widget_inline_styles();
            if node.inline_styles != new_inline {
                node.inline_styles = new_inline.to_vec();
                node.styles_dirty = true;
                ctx.needs_layout = true;
                ctx.needs_render = true;
                cascade_context_changed = true;
            }
        }
        if cascade_context_changed {
            crate::mss::cascade::mark_subtree_styles_dirty(self, id);
        }
        if ctx.needs_layout || ctx.needs_render {
            let mut flags = DirtyFlags::empty();
            if ctx.needs_layout { flags |= DirtyFlags::LAYOUT; }
            if ctx.needs_render { flags |= DirtyFlags::RENDER; }
            if let Some(node) = self.elements.get_mut(&id) {
                node.element.mark_dirty(flags);
            }
        }
        if let Some(node) = self.elements.get_mut(&id) {
            node.refresh_hint_cache();
        }
        let wants_focus = self
            .elements
            .get_mut(&id)
            .map(|n| n.element.take_focus_request())
            .unwrap_or(false);
        if wants_focus {
            self.focused_element = Some(id);
        }
        self.sync_registries_for(id);
    }

    #[allow(dead_code)]
    fn collect_rebuild_ids(&self, id: ElementId, out: &mut Vec<ElementId>) {
        crate::perf::incr(crate::perf::Counter::RebuildVisit);
        let node = match self.elements.get(&id) {
            Some(n) => n,
            None => return,
        };
        if node.element.needs_rebuild() {
            out.push(id);
        }
        let active_count = node.element.active_child_count();
        let children: Vec<_> = node.children.iter().take(active_count).copied().collect();
        for child_id in children {
            self.collect_rebuild_ids(child_id, out);
        }
    }

    pub fn remove_children(&mut self, parent_id: ElementId) {
        let child_ids = match self.elements.get(&parent_id) {
            Some(node) => node.children.clone(),
            None => return,
        };
        for child_id in &child_ids {
            self.remove_subtree(*child_id);
        }
        if let Some(node) = self.elements.get_mut(&parent_id) {
            node.children.clear();
            node.children_idx.clear();
        }
    }

    fn remove_subtree(&mut self, id: ElementId) {
        let child_ids = match self.elements.get(&id) {
            Some(node) => node.children.clone(),
            None => return,
        };
        for child_id in &child_ids {
            self.remove_subtree(*child_id);
        }
        self.elements.remove(&id);
        self.cache_remove(&id);
        self.overlay_stack.retain(|e| e.element_id != id);
        self.drop_targets.retain(|&dt| dt != id);
        if self.last_mousedown_element == Some(id) {
            self.last_mousedown_element = None;
        }
        if self.mouse_captor == Some(id) {
            self.mouse_captor = None;
        }
        if self.focused_element == Some(id) {
            self.focused_element = None;
        }
        self.animation_registry.remove(&id);
        self.rebuild_registry.remove(&id);
        self.post_layout_sync_registry.remove(&id);
    }

    pub fn register_drop_target(&mut self, id: ElementId) {
        if !self.drop_targets.contains(&id) {
            self.drop_targets.push(id);
        }
    }

    pub fn unregister_drop_target(&mut self, id: ElementId) {
        self.drop_targets.retain(|&dt| dt != id);
    }

    pub fn drop_targets_len(&self) -> usize {
        self.drop_targets.len()
    }
}

impl Default for ElementTree {
    fn default() -> Self {
        Self::new()
    }
}

impl crate::mss::SelectorMatchContext for ElementTree {
    fn element_classes(&self, id: ElementId) -> &[String] {
        self.elements.get(&id)
            .map(|n| n.element.get_classes())
            .unwrap_or(&[])
    }

    fn element_type_name(&self, id: ElementId) -> &str {
        self.elements.get(&id)
            .map(|n| n.element.element_type_name())
            .unwrap_or("")
    }

    fn parent_id(&self, id: ElementId) -> Option<ElementId> {
        self.elements.get(&id).and_then(|n| n.parent)
    }

    fn previous_sibling(&self, id: ElementId) -> Option<ElementId> {
        let parent_id = self.elements.get(&id)?.parent?;
        let parent = self.elements.get(&parent_id)?;
        let pos = parent.children.iter().position(|&c| c == id)?;
        if pos > 0 { Some(parent.children[pos - 1]) } else { None }
    }

    fn previous_siblings(&self, id: ElementId) -> Vec<ElementId> {
        let parent_id = match self.elements.get(&id).and_then(|n| n.parent) {
            Some(p) => p,
            None => return vec![],
        };
        let parent = match self.elements.get(&parent_id) {
            Some(p) => p,
            None => return vec![],
        };
        let pos = match parent.children.iter().position(|&c| c == id) {
            Some(p) => p,
            None => return vec![],
        };
        parent.children[..pos].iter().rev().copied().collect()
    }
}

impl ElementTree {
    pub fn scroll_element_into_view(&mut self, element_id: ElementId, _element_rect: crate::core::Rect) {
        let element_bounds = match self.elements.get(&element_id) {
            Some(n) => n.element.bounds(),
            None => return,
        };

        let mut content_y = element_bounds.origin.y;
        let element_height = element_bounds.size.height;
        let mut current_id = element_id;

        loop {
            let parent_id = match self.elements.get(&current_id).and_then(|n| n.parent) {
                Some(pid) => pid,
                None => return,
            };

            let is_scroll = self.elements.get(&parent_id)
                .map(|n| n.element.is_scroll_container())
                .unwrap_or(false);

            if is_scroll {
                let child_rect = crate::core::Rect::new(
                    crate::core::Point::new(0.0, content_y),
                    crate::core::Size::new(0.0, element_height),
                );
                if let Some(node) = self.elements.get_mut(&parent_id) {
                    node.element.ensure_visible(child_rect);
                }
                // Плавный скролл — анимация на предке, не на исходном элементе.
                self.note_animation_started(parent_id);
                return;
            }

            if let Some(node) = self.elements.get(&parent_id) {
                content_y += node.element.bounds().origin.y;
            }

            current_id = parent_id;
        }
    }

    pub fn apply_styles(&mut self, style_engine: &crate::mss::StyleEngine) {
        use crate::mss::{selector_matches, selector_pseudo, ComputedStyle, window_flags as wf};

        let element_ids: Vec<super::ElementId> = self.elements.keys().copied().collect();
        let rules = style_engine.stylesheet().rules().to_vec();
        let window_flags = self.window_flags;

        for id in element_ids {
            let has_identity = if let Some(node) = self.elements.get(&id) {
                !node.element.get_classes().is_empty()
                    || !node.element.element_type_name().is_empty()
            } else {
                continue;
            };

            if !has_identity {
                continue;
            }

            let mut base = ComputedStyle::default();
            let mut has_base = false;

            let mut matching: Vec<(usize, (u32, u32, u32), &crate::mss::StyleRule)> = rules
                .iter()
                .enumerate()
                .filter(|(_, rule)| selector_matches(&rule.selector, id, self))
                .map(|(i, rule)| (i, rule.selector.specificity(), rule))
                .collect();
            matching.sort_by(|a, b| a.1.cmp(&b.1).then(a.0.cmp(&b.0)));

            for (_, _, rule) in &matching {
                let pseudo = selector_pseudo(&rule.selector);
                let apply = match pseudo {
                    None => true,
                    Some("window-maximized")  => window_flags & wf::MAXIMIZED  != 0,
                    Some("window-fullscreen") => window_flags & wf::FULLSCREEN != 0,
                    Some("window-focused")    => window_flags & wf::FOCUSED    != 0,
                    Some(_) => false,
                };
                if apply {
                    has_base = true;
                    for (prop, val) in &rule.declarations {
                        base.set(prop, style_engine.resolve_variable(val));
                    }
                }
            }

            {
                let has_inline = self.elements.get(&id)
                    .map(|n| !n.inline_styles.is_empty())
                    .unwrap_or(false);
                if has_inline {
                    if let Some(node) = self.elements.get(&id) {
                        let inline = node.inline_styles.clone();
                        has_base = true;
                        for (prop, val) in &inline {
                            base.set(prop, val.clone());
                        }
                    }
                }
            }

            if has_base {
                if let Some(node) = self.elements.get_mut(&id) {
                    node.element.apply_computed_style(&base);
                    node.refresh_hint_cache();
                }
                self.note_animation_started(id);
            }
        }
    }
}
