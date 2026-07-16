use crate::core::{Point, Rect, Size};
use crate::render::DisplayList;
use crate::widget::{Element, ElementId, ElementTree};
use crate::mss::{StyleEngine, selector_matches, selector_pseudo};
use super::panel;

pub fn render_styles(
    list: &mut DisplayList,
    content_rect: Rect,
    tree: &ElementTree,
    style_engine: &StyleEngine,
    selected_id: ElementId,
    scroll_offset: f32,
) {
    list.push_clip(content_rect);

    let node = match tree.elements.get(&selected_id) {
        Some(n) => n,
        None => {
            render_no_selection(list, content_rect);
            list.pop_clip();
            return;
        }
    };

    let mut y = content_rect.origin.y - scroll_offset;
    let x = content_rect.origin.x;
    let w = content_rect.size.width;

    y = render_section_header(list, x, y, w, "Element Info");

    let element = &*node.element;
    let type_name = element.element_type_name();
    let bounds = element.bounds();

    let info_lines = [
        ("Type", if type_name.is_empty() { "Element" } else { type_name }.to_string()),
        ("ID", format!("#{}", selected_id.0)),
        ("Classes", {
            let classes = element.get_classes();
            if classes.is_empty() { "(none)".to_string() }
            else { classes.iter().map(|c| format!(".{}", c)).collect::<Vec<_>>().join(" ") }
        }),
        ("Bounds", format!("({:.0}, {:.0}) {:.0}x{:.0}",
            bounds.origin.x, bounds.origin.y, bounds.size.width, bounds.size.height)),
        ("Visible", format!("{}", element.is_visible())),
        ("Layout", format!("{:?}", element.layout_hint())),
        ("Dirty", format_dirty_flags(element)),
        ("Children", format!("{}", element.children().len())),
    ];

    for (label, value) in &info_lines {
        if y + panel::LINE_HEIGHT > content_rect.origin.y
            && y < content_rect.origin.y + content_rect.size.height
        {
            let label_rect = Rect::new(
                Point::new(x + 4.0, y + 2.0),
                Size::new(80.0, panel::FONT_SIZE + 2.0),
            );
            list.push_text(label, label_rect, panel::TEXT_SECONDARY, panel::FONT_SIZE);

            let val_rect = Rect::new(
                Point::new(x + 84.0, y + 2.0),
                Size::new(w - 88.0, panel::FONT_SIZE + 2.0),
            );
            list.push_text(value, val_rect, panel::TEXT_PRIMARY, panel::FONT_SIZE);
        }
        y += panel::LINE_HEIGHT;
    }

    y += 8.0;

    y = render_section_header(list, x, y, w, "Matching Rules");

    let rules = style_engine.stylesheet().rules();
    let mut has_rules = false;

    for rule in rules {
        let pseudo = selector_pseudo(&rule.selector);
        if !selector_matches(&rule.selector, selected_id, tree) {
            continue;
        }
        has_rules = true;

        let selector_str = format!(
            "{}{} {{",
            &rule.selector_str,
            pseudo.map(|p| format!(":{}", p)).unwrap_or_default()
        );
        let avail_w = w - 12.0;
        let lines = wrap_text(&selector_str, avail_w, panel::FONT_SIZE);
        for line in &lines {
            if y + panel::LINE_HEIGHT > content_rect.origin.y
                && y < content_rect.origin.y + content_rect.size.height
            {
                let sel_rect = Rect::new(
                    Point::new(x + 4.0, y + 2.0),
                    Size::new(avail_w, panel::FONT_SIZE + 2.0),
                );
                list.push_text(line, sel_rect, panel::TEXT_KEYWORD, panel::FONT_SIZE);
            }
            y += panel::LINE_HEIGHT;
        }

        for (prop, val) in &rule.declarations {
            if y + panel::LINE_HEIGHT > content_rect.origin.y
                && y < content_rect.origin.y + content_rect.size.height
            {
                let prop_rect = Rect::new(
                    Point::new(x + 16.0, y + 2.0),
                    Size::new(120.0, panel::FONT_SIZE + 2.0),
                );
                list.push_text(&format!("{}:", prop), prop_rect, panel::TEXT_PRIMARY, panel::FONT_SIZE);

                let val_str = format_style_value(val);
                let val_color = style_value_color(val);
                let val_rect = Rect::new(
                    Point::new(x + 136.0, y + 2.0),
                    Size::new(w - 140.0, panel::FONT_SIZE + 2.0),
                );
                list.push_text(&val_str, val_rect, val_color, panel::FONT_SIZE);
            }
            y += panel::LINE_HEIGHT;
        }

        if y + panel::LINE_HEIGHT > content_rect.origin.y
            && y < content_rect.origin.y + content_rect.size.height
        {
            let close_rect = Rect::new(
                Point::new(x + 4.0, y + 2.0),
                Size::new(w - 8.0, panel::FONT_SIZE + 2.0),
            );
            list.push_text("}", close_rect, panel::TEXT_KEYWORD, panel::FONT_SIZE);
        }
        y += panel::LINE_HEIGHT + 4.0;
    }

    if !has_rules {
        let no_rules_rect = Rect::new(
            Point::new(x + 4.0, y + 2.0),
            Size::new(w - 8.0, panel::FONT_SIZE + 2.0),
        );
        list.push_text("No matching rules", no_rules_rect, panel::TEXT_SECONDARY, panel::FONT_SIZE);
    }

    list.pop_clip();
}

fn render_no_selection(list: &mut DisplayList, content_rect: Rect) {
    let text_rect = Rect::new(
        Point::new(content_rect.origin.x + 4.0, content_rect.origin.y + 20.0),
        Size::new(content_rect.size.width - 8.0, panel::FONT_SIZE + 2.0),
    );
    list.push_text("Select an element to view styles", text_rect, panel::TEXT_SECONDARY, panel::FONT_SIZE);
}

fn render_section_header(
    list: &mut DisplayList,
    x: f32, y: f32, w: f32,
    title: &str,
) -> f32 {
    let header_rect = Rect::new(
        Point::new(x, y),
        Size::new(w, panel::LINE_HEIGHT),
    );
    list.push_rect(header_rect, panel::TAB_BG, [0.0; 4]);

    let text_rect = Rect::new(
        Point::new(x + 4.0, y + 2.0),
        Size::new(w - 8.0, panel::FONT_SIZE + 2.0),
    );
    list.push_text(title, text_rect, panel::TEXT_PRIMARY, panel::FONT_SIZE);

    y + panel::LINE_HEIGHT
}

fn format_dirty_flags(element: &dyn Element) -> String {
    use crate::widget::DirtyFlags;
    let mut flags = Vec::new();
    if element.is_dirty(DirtyFlags::LAYOUT) { flags.push("LAYOUT"); }
    if element.is_dirty(DirtyFlags::RENDER) { flags.push("RENDER"); }
    if element.is_dirty(DirtyFlags::PAINT) { flags.push("PAINT"); }
    if element.is_dirty(DirtyFlags::STATE) { flags.push("STATE"); }
    if element.is_dirty(DirtyFlags::CHILDREN) { flags.push("CHILDREN"); }
    if element.is_dirty(DirtyFlags::ANIMATION) { flags.push("ANIMATION"); }
    if flags.is_empty() { "clean".to_string() } else { flags.join(" | ") }
}

fn format_style_value(val: &crate::mss::StyleValue) -> String {
    use crate::mss::StyleValue;
    match val {
        StyleValue::Color(c) => format!("#{:02X}{:02X}{:02X}{}", c.r, c.g, c.b,
            if c.a < 255 { format!("{:02X}", c.a) } else { String::new() }),
        StyleValue::Length(v, unit) => format!("{}{}", v, format_unit(unit)),
        StyleValue::Number(v) => format!("{}", v),
        StyleValue::String(s) => format!("\"{}\"", s),
        StyleValue::Var(name) => format!("var(--{})", name),
        StyleValue::VarWithFallback(name, fallback) => {
            format!("var(--{}, {})", name, format_style_value(fallback))
        }
        StyleValue::List(items) => items.iter().map(|i| format_style_value(i)).collect::<Vec<_>>().join(", "),
        StyleValue::Gradient(g) => match g {
            crate::core::Gradient::Linear { angle_deg, stops } => format!("linear-gradient({}deg, {} stops)", angle_deg, stops.len()),
            crate::core::Gradient::Radial { stops, .. } => format!("radial-gradient({} stops)", stops.len()),
            crate::core::Gradient::Conic { stops, .. } => format!("conic-gradient({} stops)", stops.len()),
        },
        StyleValue::None => "none".to_string(),
        StyleValue::Inherit => "inherit".to_string(),
        StyleValue::Initial => "initial".to_string(),
        StyleValue::Unset => "unset".to_string(),
    }
}

fn format_unit(unit: &crate::mss::Unit) -> &'static str {
    use crate::mss::Unit;
    match unit {
        Unit::Px => "px",
        Unit::Percent => "%",
        Unit::Em => "em",
        Unit::Rem => "rem",
        Unit::Vw => "vw",
        Unit::Vh => "vh",
        Unit::Auto => "auto",
        Unit::FitContent => "fit-content",
        Unit::MaxContent => "max-content",
        Unit::MinContent => "min-content",
    }
}

fn style_value_color(val: &crate::mss::StyleValue) -> crate::core::Color {
    use crate::mss::StyleValue;
    match val {
        StyleValue::Color(_) => panel::TEXT_STRING,
        StyleValue::Length(_, _) | StyleValue::Number(_) => panel::TEXT_NUMBER,
        StyleValue::String(_) => panel::TEXT_STRING,
        StyleValue::Var(_) | StyleValue::VarWithFallback(_, _) => panel::TEXT_KEYWORD,
        _ => panel::TEXT_PRIMARY,
    }
}

fn wrap_text(text: &str, avail_width: f32, font_size: f32) -> Vec<String> {
    let char_w = font_size * 0.6;
    let max_chars = (avail_width / char_w).max(10.0) as usize;

    if text.chars().count() <= max_chars {
        return vec![text.to_string()];
    }

    let mut lines = Vec::new();
    let mut remaining = text;

    while !remaining.is_empty() {
        if remaining.chars().count() <= max_chars {
            lines.push(remaining.to_string());
            break;
        }

        let byte_limit = remaining.char_indices()
            .nth(max_chars)
            .map(|(i, _)| i)
            .unwrap_or(remaining.len());

        let break_at = remaining[..byte_limit]
            .rfind(|c: char| c == ',' || c == ' ')
            .map(|i| i + 1)
            .unwrap_or(byte_limit);

        lines.push(remaining[..break_at].to_string());
        remaining = remaining[break_at..].trim_start();
    }

    if lines.is_empty() {
        lines.push(text.to_string());
    }
    lines
}
