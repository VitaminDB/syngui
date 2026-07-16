use crate::core::{Point, Rect, Size};
use crate::render::DisplayList;
use crate::widget::{ElementId, ElementTree};
use super::panel;

pub fn render_highlight(
    list: &mut DisplayList,
    tree: &ElementTree,
    selected: Option<ElementId>,
    hovered: Option<ElementId>,
    picking_hovered: Option<ElementId>,
) {
    let hover_id = picking_hovered.or(hovered);
    if let Some(id) = hover_id {
        if Some(id) != selected {
            if let Some(node) = tree.elements.get(&id) {
                let bounds = node.element.bounds();
                if bounds.size.width > 0.0 && bounds.size.height > 0.0 {
                    list.push_rect(bounds, panel::HOVERED_HIGHLIGHT, [0.0; 4]);
                    render_border(list, bounds, panel::HOVERED_BORDER, 1.0);
                    render_element_label(list, &*node.element, bounds, panel::HOVERED_BORDER);
                }
            }
        }
    }

    if let Some(id) = selected {
        if let Some(node) = tree.elements.get(&id) {
            let bounds = node.element.bounds();
            if bounds.size.width > 0.0 && bounds.size.height > 0.0 {
                let margin = node.element.margin();
                if margin.left > 0.0 || margin.top > 0.0 || margin.right > 0.0 || margin.bottom > 0.0 {
                    let margin_color = crate::core::Color::new(0.976, 0.651, 0.286, 0.15);
                    if margin.top > 0.0 {
                        let r = Rect::new(
                            Point::new(bounds.origin.x - margin.left, bounds.origin.y - margin.top),
                            Size::new(bounds.size.width + margin.left + margin.right, margin.top),
                        );
                        list.push_rect(r, margin_color, [0.0; 4]);
                    }
                    if margin.bottom > 0.0 {
                        let r = Rect::new(
                            Point::new(bounds.origin.x - margin.left, bounds.origin.y + bounds.size.height),
                            Size::new(bounds.size.width + margin.left + margin.right, margin.bottom),
                        );
                        list.push_rect(r, margin_color, [0.0; 4]);
                    }
                    if margin.left > 0.0 {
                        let r = Rect::new(
                            Point::new(bounds.origin.x - margin.left, bounds.origin.y),
                            Size::new(margin.left, bounds.size.height),
                        );
                        list.push_rect(r, margin_color, [0.0; 4]);
                    }
                    if margin.right > 0.0 {
                        let r = Rect::new(
                            Point::new(bounds.origin.x + bounds.size.width, bounds.origin.y),
                            Size::new(margin.right, bounds.size.height),
                        );
                        list.push_rect(r, margin_color, [0.0; 4]);
                    }
                }

                list.push_rect(bounds, panel::SELECTED_HIGHLIGHT, [0.0; 4]);
                render_border(list, bounds, panel::SELECTED_BORDER, 2.0);
                render_element_label(list, &*node.element, bounds, panel::SELECTED_BORDER);
            }
        }
    }
}

fn render_border(list: &mut DisplayList, bounds: Rect, color: crate::core::Color, thickness: f32) {
    list.push_rect(
        Rect::new(bounds.origin, Size::new(bounds.size.width, thickness)),
        color, [0.0; 4],
    );
    list.push_rect(
        Rect::new(
            Point::new(bounds.origin.x, bounds.origin.y + bounds.size.height - thickness),
            Size::new(bounds.size.width, thickness),
        ),
        color, [0.0; 4],
    );
    list.push_rect(
        Rect::new(bounds.origin, Size::new(thickness, bounds.size.height)),
        color, [0.0; 4],
    );
    list.push_rect(
        Rect::new(
            Point::new(bounds.origin.x + bounds.size.width - thickness, bounds.origin.y),
            Size::new(thickness, bounds.size.height),
        ),
        color, [0.0; 4],
    );
}

fn render_element_label(
    list: &mut DisplayList,
    element: &dyn crate::widget::Element,
    bounds: Rect,
    bg_color: crate::core::Color,
) {
    let type_name = element.element_type_name();
    let label = if type_name.is_empty() {
        format!("#{} {:.0}x{:.0}", element.id().0, bounds.size.width, bounds.size.height)
    } else {
        format!("{} {:.0}x{:.0}", type_name, bounds.size.width, bounds.size.height)
    };

    let label_width = label.chars().count() as f32 * 6.5 + 8.0;
    let label_height = 16.0;
    let viewport = list.surface_size();

    let label_y = if bounds.origin.y - label_height - 2.0 >= 0.0 {
        bounds.origin.y - label_height - 2.0
    } else {
        bounds.origin.y + bounds.size.height + 2.0
    };

    let label_x = bounds.origin.x.min(viewport.width - label_width).max(0.0);

    let label_rect = Rect::new(
        Point::new(label_x, label_y),
        Size::new(label_width, label_height),
    );

    let label_bg = crate::core::Color::new(bg_color.r, bg_color.g, bg_color.b, 0.9);
    list.push_rect(label_rect, label_bg, [3.0; 4]);

    let text_rect = Rect::new(
        Point::new(label_rect.origin.x + 4.0, label_rect.origin.y + 2.0),
        Size::new(label_width - 8.0, 12.0),
    );
    list.push_text(&label, text_rect, panel::TEXT_PRIMARY, panel::SMALL_FONT_SIZE);
}
