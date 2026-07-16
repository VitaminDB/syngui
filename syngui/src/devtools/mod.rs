mod panel;
mod inspector;
mod styles;
mod highlight;
pub mod profiler;
pub mod event_log;

use std::collections::{HashSet, VecDeque};
use crate::core::{Point, Size};
use crate::input::{Event, Key, MouseButton};
use crate::mss::StyleEngine;
use crate::render::DisplayList;
use crate::widget::{ElementId, ElementTree};

pub use profiler::FrameTiming;
pub use event_log::EventLogEntry;

const MAX_TIMING_SAMPLES: usize = 240;
const MAX_LOG_ENTRIES: usize = 200;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DevToolsTab {
    Inspector,
    Styles,
    Profiler,
    EventLog,
}

pub struct DevTools {
    enabled: bool,
    panel_width: f32,
    active_tab: DevToolsTab,

    selected_element: Option<ElementId>,
    hovered_tree_node: Option<ElementId>,
    expanded_nodes: HashSet<ElementId>,
    tree_scroll_offset: f32,
    picking_mode: bool,
    picking_hovered: Option<ElementId>,

    styles_scroll_offset: f32,

    frame_timings: VecDeque<FrameTiming>,

    event_log_entries: VecDeque<EventLogEntry>,
    event_log_scroll: f32,
    event_log_paused: bool,

    mouse_pos: Point,
    mouse_pressed: bool,
    resizing: bool,
    resize_start_x: f32,
    resize_start_width: f32,
}

impl DevTools {
    pub fn new() -> Self {
        Self {
            enabled: false,
            panel_width: panel::PANEL_DEFAULT_WIDTH,
            active_tab: DevToolsTab::Inspector,
            selected_element: None,
            hovered_tree_node: None,
            expanded_nodes: HashSet::new(),
            tree_scroll_offset: 0.0,
            picking_mode: false,
            picking_hovered: None,
            styles_scroll_offset: 0.0,
            frame_timings: VecDeque::with_capacity(MAX_TIMING_SAMPLES),
            event_log_entries: VecDeque::with_capacity(MAX_LOG_ENTRIES),
            event_log_scroll: 0.0,
            event_log_paused: false,
            mouse_pos: Point::zero(),
            mouse_pressed: false,
            resizing: false,
            resize_start_x: 0.0,
            resize_start_width: 0.0,
        }
    }

    pub fn toggle(&mut self) {
        self.enabled = !self.enabled;
        if !self.enabled {
            self.picking_mode = false;
            self.picking_hovered = None;
            self.resizing = false;
        }
    }

    pub fn auto_expand(&mut self, tree: &ElementTree, max_depth: usize) {
        if let Some(root_id) = tree.root_id {
            self.expand_recursive(tree, root_id, 0, max_depth);
        }
    }

    fn expand_recursive(&mut self, tree: &ElementTree, id: ElementId, depth: usize, max_depth: usize) {
        if depth >= max_depth { return; }
        if let Some(node) = tree.elements.get(&id) {
            if !node.children.is_empty() {
                self.expanded_nodes.insert(id);
                let children: Vec<ElementId> = node.children.clone();
                for child_id in children {
                    self.expand_recursive(tree, child_id, depth + 1, max_depth);
                }
            }
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub fn expanded_nodes_count(&self) -> usize {
        self.expanded_nodes.len()
    }

    pub fn is_resizing(&self) -> bool {
        self.resizing
    }

    pub fn is_picking(&self) -> bool {
        self.picking_mode && self.enabled
    }

    pub fn toggle_picking(&mut self) {
        self.picking_mode = !self.picking_mode;
        if !self.picking_mode {
            self.picking_hovered = None;
        }
    }

    pub fn event_log_paused(&self) -> bool {
        self.event_log_paused
    }

    pub fn panel_width(&self) -> f32 {
        self.panel_width
    }

    pub fn contains_point(&self, pos: Point, surface_size: Size) -> bool {
        if !self.enabled { return false; }
        let panel = panel::panel_rect(surface_size, self.panel_width);
        pos.x >= panel.origin.x - panel::RESIZE_HANDLE_WIDTH
    }

    pub fn update_mouse_pos(&mut self, pos: Point) {
        self.mouse_pos = pos;
    }

    pub fn update_picking_hover(&mut self, tree: &ElementTree, pos: Point) {
        if self.picking_mode && self.enabled {
            self.picking_hovered = inspector::pick_element_at(tree, pos);
        }
    }

    pub fn complete_pick(&mut self, tree: &ElementTree, pos: Point) {
        if let Some(id) = inspector::pick_element_at(tree, pos) {
            self.selected_element = Some(id);
            self.expand_to_element(tree, id);
        }
        self.picking_mode = false;
        self.picking_hovered = None;
    }

    pub fn record_frame_timing(&mut self, timing: FrameTiming) {
        if self.frame_timings.len() >= MAX_TIMING_SAMPLES {
            self.frame_timings.pop_front();
        }
        self.frame_timings.push_back(timing);
    }

    pub fn log_event(&mut self, entry: EventLogEntry) {
        if self.event_log_paused { return; }
        if self.event_log_entries.len() >= MAX_LOG_ENTRIES {
            self.event_log_entries.pop_front();
        }
        self.event_log_entries.push_back(entry);
    }

    pub fn handle_mouse_event(&mut self, event: &Event, surface_size: Size, tree: &ElementTree) -> bool {
        let panel = panel::panel_rect(surface_size, self.panel_width);
        let content = panel::content_rect(panel);

        match event {
            Event::MouseDown { button: MouseButton::Left, position } => {
                self.mouse_pressed = true;
                let pos = *position;

                if pos.x >= panel.origin.x - panel::RESIZE_HANDLE_WIDTH
                    && pos.x <= panel.origin.x + 2.0
                {
                    self.resizing = true;
                    self.resize_start_x = pos.x;
                    self.resize_start_width = self.panel_width;
                    return true;
                }

                if let Some(tab) = panel::hit_test_tab(pos, panel) {
                    self.active_tab = tab;
                    return true;
                }

                if pos.x >= content.origin.x && pos.x <= content.origin.x + content.size.width
                    && pos.y >= content.origin.y && pos.y <= content.origin.y + content.size.height
                {
                    match self.active_tab {
                        DevToolsTab::Inspector => {
                            if self.picking_mode
                                && pos.y < content.origin.y - self.tree_scroll_offset + panel::LINE_HEIGHT + 4.0
                            {
                                self.picking_mode = false;
                                return true;
                            }

                            let hit = inspector::hit_test_tree(
                                tree, &self.expanded_nodes, content,
                                self.tree_scroll_offset, pos, self.picking_mode,
                            );

                            if let Some(toggle_id) = hit.toggle_expand {
                                if self.expanded_nodes.contains(&toggle_id) {
                                    self.expanded_nodes.remove(&toggle_id);
                                } else {
                                    self.expanded_nodes.insert(toggle_id);
                                }
                            } else if let Some(clicked_id) = hit.clicked_node {
                                self.selected_element = Some(clicked_id);
                            }
                        }
                        DevToolsTab::EventLog => {
                            if pos.y >= content.origin.y && pos.y < content.origin.y + panel::LINE_HEIGHT {
                                self.event_log_paused = !self.event_log_paused;
                            }
                        }
                        _ => {}
                    }
                }

                true
            }
            Event::MouseUp { button: MouseButton::Left, .. } => {
                self.mouse_pressed = false;
                self.resizing = false;
                true
            }
            Event::MouseMove(pos) => {
                let pos = *pos;

                if self.resizing {
                    let delta = self.resize_start_x - pos.x;
                    self.panel_width = (self.resize_start_width + delta)
                        .max(panel::PANEL_MIN_WIDTH)
                        .min(panel::PANEL_MAX_WIDTH);
                    return true;
                }

                if self.active_tab == DevToolsTab::Inspector
                    && pos.x >= content.origin.x && pos.y >= content.origin.y
                    && pos.y <= content.origin.y + content.size.height
                {
                    let hit = inspector::hit_test_tree(
                        tree, &self.expanded_nodes, content,
                        self.tree_scroll_offset, pos, self.picking_mode,
                    );
                    self.hovered_tree_node = hit.hovered_node;
                } else {
                    self.hovered_tree_node = None;
                }

                true
            }
            Event::MouseWheel { delta, .. } => {
                let scroll_amount = *delta;
                match self.active_tab {
                    DevToolsTab::Inspector => {
                        let max_scroll = inspector::compute_content_height(
                            tree, &self.expanded_nodes, self.picking_mode,
                        ) - content.size.height;
                        self.tree_scroll_offset = (self.tree_scroll_offset - scroll_amount)
                            .max(0.0)
                            .min(max_scroll.max(0.0));
                    }
                    DevToolsTab::Styles => {
                        if self.selected_element.is_some() {
                            self.styles_scroll_offset = (self.styles_scroll_offset - scroll_amount)
                                .max(0.0)
                                .min(2000.0);
                        }
                    }
                    DevToolsTab::EventLog => {
                        let max_scroll = event_log::compute_content_height(&self.event_log_entries)
                            - content.size.height;
                        self.event_log_scroll = (self.event_log_scroll - scroll_amount)
                            .max(0.0)
                            .min(max_scroll.max(0.0));
                    }
                    _ => {}
                }
                true
            }
            _ => false,
        }
    }

    pub fn handle_key_event(&mut self, key: Key, _tree: &ElementTree) -> bool {
        match key {
            Key::C => {
                false
            }
            _ => false,
        }
    }

    pub fn build_display_list(
        &self,
        list: &mut DisplayList,
        tree: &ElementTree,
        style_engine: &StyleEngine,
    ) {
        if !self.enabled { return; }

        let surface_size = list.surface_size();
        if surface_size.width <= 0.0 || surface_size.height <= 0.0 { return; }

        list.begin_overlay_absolute();

        highlight::render_highlight(
            list, tree,
            self.selected_element,
            self.hovered_tree_node,
            self.picking_hovered,
        );

        let panel = panel::panel_rect(surface_size, self.panel_width);
        panel::render_panel_background(list, panel);
        panel::render_tab_bar(list, panel, &self.active_tab);
        panel::render_resize_handle(list, panel, self.resizing);

        let content = panel::content_rect(panel);

        match self.active_tab {
            DevToolsTab::Inspector => {
                inspector::render_inspector(
                    list, content, tree,
                    self.selected_element,
                    self.hovered_tree_node,
                    &self.expanded_nodes,
                    self.tree_scroll_offset,
                    self.picking_mode,
                );
            }
            DevToolsTab::Styles => {
                if let Some(sel_id) = self.selected_element {
                    styles::render_styles(
                        list, content, tree, style_engine,
                        sel_id, self.styles_scroll_offset,
                    );
                } else {
                    styles::render_styles(
                        list, content, tree, style_engine,
                        ElementId::default(), self.styles_scroll_offset,
                    );
                }
            }
            DevToolsTab::Profiler => {
                profiler::render_profiler(list, content, &self.frame_timings);
            }
            DevToolsTab::EventLog => {
                event_log::render_event_log(
                    list, content,
                    &self.event_log_entries,
                    self.event_log_scroll,
                    self.event_log_paused,
                );
            }
        }

        list.end_overlay();
    }

    fn expand_to_element(&mut self, tree: &ElementTree, id: ElementId) {
        let mut current = id;
        while let Some(node) = tree.elements.get(&current) {
            if let Some(parent_id) = node.parent {
                self.expanded_nodes.insert(parent_id);
                current = parent_id;
            } else {
                break;
            }
        }
    }
}
