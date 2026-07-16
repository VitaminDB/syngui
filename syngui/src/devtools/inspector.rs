use crate::core::{Point, Rect, Size};
use crate::render::DisplayList;
use crate::widget::{Element, ElementId, ElementTree};
use super::panel;

pub struct InspectorHitResult {
    pub hovered_node: Option<ElementId>,
    pub clicked_node: Option<ElementId>,
    pub toggle_expand: Option<ElementId>,
}

pub fn render_inspector(
    list: &mut DisplayList,
    content_rect: Rect,
    tree: &ElementTree,
    selected: Option<ElementId>,
    hovered_tree_node: Option<ElementId>,
    expanded: &std::collections::HashSet<ElementId>,
    scroll_offset: f32,
    picking_mode: bool,
) {
    list.push_clip(content_rect);

    let mut y = content_rect.origin.y - scroll_offset;

    if picking_mode {
        let btn_rect = Rect::new(
            Point::new(content_rect.origin.x, y),
            Size::new(content_rect.size.width, panel::LINE_HEIGHT),
        );
        list.push_rect(btn_rect, panel::TAB_INDICATOR, [3.0; 4]);
        let text_rect = Rect::new(
            Point::new(content_rect.origin.x + 4.0, y + 2.0),
            Size::new(content_rect.size.width - 8.0, panel::FONT_SIZE + 2.0),
        );
        list.push_text(">> Pick element (click on viewport) <<", text_rect, panel::TEXT_PRIMARY, panel::FONT_SIZE);
        y += panel::LINE_HEIGHT + 4.0;
    }

    if let Some(root_id) = tree.root_id {
        render_node_recursive(
            list, tree, root_id, 0, &mut y,
            content_rect, selected, hovered_tree_node, expanded,
        );
    }

    list.pop_clip();
}

pub fn compute_content_height(
    tree: &ElementTree,
    expanded: &std::collections::HashSet<ElementId>,
    picking_mode: bool,
) -> f32 {
    let mut height = 0.0;
    if picking_mode {
        height += panel::LINE_HEIGHT + 4.0;
    }
    if let Some(root_id) = tree.root_id {
        height += count_visible_nodes(tree, root_id, expanded) as f32 * panel::LINE_HEIGHT;
    }
    height
}

fn count_visible_nodes(
    tree: &ElementTree,
    node_id: ElementId,
    expanded: &std::collections::HashSet<ElementId>,
) -> usize {
    let mut count = 1;
    if expanded.contains(&node_id) {
        if let Some(node) = tree.elements.get(&node_id) {
            for &child_id in &node.children {
                count += count_visible_nodes(tree, child_id, expanded);
            }
        }
    }
    count
}

fn render_node_recursive(
    list: &mut DisplayList,
    tree: &ElementTree,
    node_id: ElementId,
    depth: usize,
    y: &mut f32,
    content_rect: Rect,
    selected: Option<ElementId>,
    hovered: Option<ElementId>,
    expanded: &std::collections::HashSet<ElementId>,
) {
    let node = match tree.elements.get(&node_id) {
        Some(n) => n,
        None => return,
    };

    let line_y = *y;
    let line_rect = Rect::new(
        Point::new(content_rect.origin.x, line_y),
        Size::new(content_rect.size.width, panel::LINE_HEIGHT),
    );

    let visible = line_y + panel::LINE_HEIGHT > content_rect.origin.y
        && line_y < content_rect.origin.y + content_rect.size.height;

    if visible {
        if Some(node_id) == selected {
            list.push_rect(line_rect, panel::HIGHLIGHT_BG, [0.0; 4]);
        } else if Some(node_id) == hovered {
            list.push_rect(line_rect, panel::HOVER_BG, [0.0; 4]);
        }

        let x_offset = content_rect.origin.x + depth as f32 * panel::INDENT_SIZE;

        let has_children = !node.children.is_empty();
        if has_children {
            let arrow = if expanded.contains(&node_id) { "\u{25BC}" } else { "\u{25B6}" };
            let arrow_rect = Rect::new(
                Point::new(x_offset, line_y + 2.0),
                Size::new(12.0, panel::FONT_SIZE + 2.0),
            );
            list.push_text(arrow, arrow_rect, panel::TEXT_SECONDARY, panel::SMALL_FONT_SIZE);
        }

        let text_x = x_offset + 14.0;
        let available_width = content_rect.origin.x + content_rect.size.width - text_x;

        let type_name = node.element.element_type_name();
        let display_name = if type_name.is_empty() { "Element" } else { type_name };
        let label = match &node.debug_name {
            Some(name) => format!("{}(\"{}\")", display_name, name),
            None => display_name.to_string(),
        };

        let vis_prefix = if !node.element.is_visible() { "[hidden] " } else { "" };

        let text_rect = Rect::new(
            Point::new(text_x, line_y + 2.0),
            Size::new(available_width, panel::FONT_SIZE + 2.0),
        );
        list.push_text(
            &format!("{}{}", vis_prefix, label),
            text_rect,
            if node.element.is_visible() { panel::TEXT_KEYWORD } else { panel::TEXT_SECONDARY },
            panel::FONT_SIZE,
        );

        let classes = node.element.get_classes();
        if !classes.is_empty() {
            let classes_str: String = classes.iter().take(3).map(|c| format!(".{}", c)).collect::<Vec<_>>().join("");
            let suffix = if classes.len() > 3 { "..." } else { "" };
            let class_text = format!("{}{}", classes_str, suffix);

            let name_width = (vis_prefix.chars().count() + label.chars().count()) as f32 * 7.0 + 4.0;
            let class_rect = Rect::new(
                Point::new(text_x + name_width, line_y + 2.0),
                Size::new(available_width - name_width, panel::FONT_SIZE + 2.0),
            );
            list.push_text(&class_text, class_rect, panel::TEXT_STRING, panel::FONT_SIZE);
        }

        let id_text = format!("#{}", node_id.0);
        let id_width = id_text.chars().count() as f32 * 6.0;
        let id_rect = Rect::new(
            Point::new(content_rect.origin.x + content_rect.size.width - id_width - 4.0, line_y + 2.0),
            Size::new(id_width, panel::FONT_SIZE + 2.0),
        );
        list.push_text(&id_text, id_rect, panel::TEXT_SECONDARY, panel::SMALL_FONT_SIZE);
    }

    *y += panel::LINE_HEIGHT;

    if expanded.contains(&node_id) {
        let children: Vec<ElementId> = node.children.clone();
        for child_id in children {
            render_node_recursive(
                list, tree, child_id, depth + 1, y,
                content_rect, selected, hovered, expanded,
            );
        }
    }
}

pub fn hit_test_tree(
    tree: &ElementTree,
    expanded: &std::collections::HashSet<ElementId>,
    content_rect: Rect,
    scroll_offset: f32,
    click_pos: Point,
    picking_mode: bool,
) -> InspectorHitResult {
    let mut result = InspectorHitResult {
        hovered_node: None,
        clicked_node: None,
        toggle_expand: None,
    };

    if click_pos.x < content_rect.origin.x || click_pos.x > content_rect.origin.x + content_rect.size.width {
        return result;
    }

    let mut y = content_rect.origin.y - scroll_offset;

    if picking_mode {
        y += panel::LINE_HEIGHT + 4.0;
    }

    if let Some(root_id) = tree.root_id {
        hit_test_node_recursive(
            tree, root_id, 0, &mut y, content_rect,
            expanded, click_pos, &mut result,
        );
    }

    result
}

fn hit_test_node_recursive(
    tree: &ElementTree,
    node_id: ElementId,
    depth: usize,
    y: &mut f32,
    content_rect: Rect,
    expanded: &std::collections::HashSet<ElementId>,
    click_pos: Point,
    result: &mut InspectorHitResult,
) {
    let node = match tree.elements.get(&node_id) {
        Some(n) => n,
        None => return,
    };

    let line_y = *y;

    if click_pos.y >= line_y && click_pos.y < line_y + panel::LINE_HEIGHT {
        result.hovered_node = Some(node_id);
        result.clicked_node = Some(node_id);

        let arrow_x = content_rect.origin.x + depth as f32 * panel::INDENT_SIZE;
        if click_pos.x >= arrow_x && click_pos.x < arrow_x + 14.0 && !node.children.is_empty() {
            result.toggle_expand = Some(node_id);
        }
    }

    *y += panel::LINE_HEIGHT;

    if expanded.contains(&node_id) {
        let children: Vec<ElementId> = node.children.clone();
        for child_id in children {
            hit_test_node_recursive(
                tree, child_id, depth + 1, y,
                content_rect, expanded, click_pos, result,
            );
        }
    }
}

pub fn pick_element_at(
    tree: &ElementTree,
    pos: Point,
) -> Option<ElementId> {
    let root_id = tree.root_id?;
    pick_element_recursive(tree, root_id, pos)
}

fn pick_element_recursive(
    tree: &ElementTree,
    node_id: ElementId,
    pos: Point,
) -> Option<ElementId> {
    let node = tree.elements.get(&node_id)?;

    if !node.element.is_visible() {
        return None;
    }

    if !node.element.hit_test(pos) {
        return None;
    }

    for &child_id in node.children.iter().rev() {
        if let Some(found) = pick_element_recursive(tree, child_id, pos) {
            return Some(found);
        }
    }

    Some(node_id)
}
