use crate::core::{Color, Point, Rect, Size};
use crate::input::{Event, EventResult};
use crate::layout::Constraints;
use crate::mss::{ComputedStyle, Overflow};
use crate::mss::MssFields;
use crate::render::{Border, DisplayList};
use crate::widget::context::EventContextExt;
use crate::widget::{DirtyFlags, Element, ElementId, ElementTree, LayoutHint, StyledElement, UpdateContext, Widget};
use super::IntoWidget;
use std::any::Any;
use std::time::Duration;

pub struct DecoratedBox {
    pub child: Option<Box<dyn Widget>>,
    pub clip: bool,
}

impl Default for DecoratedBox {
    fn default() -> Self {
        Self::new()
    }
}

impl DecoratedBox {
    pub fn styled() -> Self {
        Self::new()
    }

    pub fn new() -> Self {
        Self {
            child: None,
            clip: false,
        }
    }

    pub fn child<M>(mut self, child: impl IntoWidget<M>) -> Self {
        self.child = Some(child.into_widget());
        self
    }

    pub fn clip(mut self, clip: bool) -> Self {
        self.clip = clip;
        self
    }
}

impl Widget for DecoratedBox {
    fn create_element(&self) -> Box<dyn Element> {
        Box::new(DecoratedBoxElement {
            id: ElementId::new(),
            background: Color::TRANSPARENT,
            corner_radius: [0.0; 4],
            border: None,
            border_sides: [None; 4],
            shadow: None,
            width: None,
            height: None,
            padding_left: 0.0,
            padding_right: 0.0,
            padding_top: 0.0,
            padding_bottom: 0.0,
            clip: self.clip,
            bounds: Rect::zero(),
            child_id: None,
            classes: Vec::new(),
            dirty_flags: DirtyFlags::LAYOUT | DirtyFlags::RENDER,
            hover: false,
            mss: MssFields::new(),
        })
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

    fn mount(&self, tree: &mut ElementTree, parent_id: ElementId) {
        if let Some(child) = &self.child {
            let child_element = child.create_element();
            let child_id = tree.insert_with_type_id(child_element, Some(parent_id), child.as_any().type_id());
            child.mount(tree, child_id);
        }
    }

    fn child_widgets(&self) -> Vec<&dyn Widget> {
        self.child.as_ref().map(|c| vec![c.as_ref() as &dyn Widget]).unwrap_or_default()
    }
}

pub struct DecoratedBoxElement {
    id: ElementId,
    background: Color,
    corner_radius: [f32; 4],
    border: Option<Border>,
    border_sides: [Option<(f32, Color)>; 4],
    shadow: Option<(Color, f32, f32, f32)>,
    width: Option<crate::mss::Dimension>,
    height: Option<crate::mss::Dimension>,
    padding_left: f32,
    padding_right: f32,
    padding_top: f32,
    padding_bottom: f32,
    clip: bool,
    bounds: Rect,
    child_id: Option<ElementId>,
    classes: Vec<String>,
    dirty_flags: DirtyFlags,
    hover: bool,
    mss: MssFields,
}

impl DecoratedBoxElement {
    fn start_transition_to_current_state(&mut self) {
        self.mss.start_transition_to(self.hover, false, false, false);
    }

    fn active_filter(&self, target: &crate::animation::transition::AnimatedPropertyMap) -> Option<Vec<crate::effects::FilterEffect>> {
        if let Some(ref anim) = self.mss.keyframe_animation {
            if anim.is_running() {
                if let Some(filter) = anim.current_values().filter() {
                    return Some(filter);
                }
            }
        }
        if let Some(chain) = self.mss.transition.filter_chain() {
            if !chain.is_empty() { return Some(chain); }
            return None;
        }
        target.filter().or_else(|| self.mss.filter.clone())
    }

    fn keyframe_opacity(&self) -> Option<f32> {
        self.mss.keyframe_animation.as_ref()
            .filter(|a| a.is_running())
            .and_then(|a| a.current_values().opacity())
    }

    fn active_box_shadow(&self, target: &crate::animation::transition::AnimatedPropertyMap) -> Option<crate::core::shadow::Shadows> {
        if let Some(ref anim) = self.mss.keyframe_animation {
            if anim.is_running() {
                if let Some(shadow) = anim.current_values().box_shadow() {
                    return Some(shadow);
                }
            }
        }
        if let Some(shadow) = self.mss.transition.box_shadow() {
            return Some(shadow);
        }
        target.box_shadow().or_else(|| self.mss.box_shadow.clone())
    }

    fn active_glow(&self, target: &crate::animation::transition::AnimatedPropertyMap) -> Option<crate::core::shadow::Shadows> {
        if let Some(ref anim) = self.mss.keyframe_animation {
            if anim.is_running() {
                if let Some(glow) = anim.current_values().glow() {
                    return Some(glow);
                }
            }
        }
        if let Some(glow) = self.mss.transition.glow() {
            return Some(glow);
        }
        target.glow().or_else(|| self.mss.glow.clone())
    }

    fn has_filter_effects(&self, target: &crate::animation::transition::AnimatedPropertyMap) -> bool {
        self.active_filter(target).map_or(false, |f| !f.is_empty())
            || self.mss.noise.is_some()
            || self.mss.vignette.is_some()
    }

    #[inline]
    fn is_plain_render(&self) -> bool {
        !self.mss.has_mss_styles
            && self.mss.keyframe_animation.is_none()
            && !self.mss.transition.is_animating()
            && self.shadow.is_none()
            && self.border.is_none()
            && self.border_sides[0].is_none()
            && self.border_sides[1].is_none()
            && self.border_sides[2].is_none()
            && self.border_sides[3].is_none()
            && self.mss.box_shadow.is_none()
            && self.mss.glow.is_none()
            && self.mss.filter.is_none()
            && self.mss.backdrop_filter.is_none()
            && self.mss.outline_width.is_none()
            && self.mss.noise.is_none()
            && self.mss.vignette.is_none()
            && self.mss.color_tint.is_none()
            && self.mss.background_gradient.is_none()
            && self.mss.opacity.is_none()
    }

    fn build_filter_effect(&self, target: &crate::animation::transition::AnimatedPropertyMap) -> crate::render::display_list::Effect {
        use crate::render::display_list::Effect;
        let mut effects: Vec<Effect> = Vec::new();

        if let Some(filters) = self.active_filter(target) {
            for f in &filters {
                let e = f.to_effect();
                if !e.is_identity() {
                    effects.push(e);
                }
            }
        }
        if let Some(intensity) = self.mss.noise {
            if intensity > 0.0 {
                effects.push(Effect::Noise { intensity });
            }
        }
        if let Some(radius) = self.mss.vignette {
            if radius > 0.0 {
                effects.push(Effect::Vignette { radius, softness: 0.3 });
            }
        }

        match effects.len() {
            0 => Effect::None,
            1 => effects.remove(0),
            _ => Effect::Chain(effects),
        }
    }
}

impl Element for DecoratedBoxElement {
    fn update(&mut self, widget: &dyn Widget, _ctx: &mut UpdateContext) {
        if let Some(db) = widget.as_any().downcast_ref::<DecoratedBox>() {
            self.clip = db.clip;
            self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
        }
    }

    fn layout(&mut self, constraints: Constraints) -> Size {
        let is_childless_spacer = self.child_id.is_none();
        let cb_w = constraints.containing_block.width;
        let cb_h = constraints.containing_block.height;
        let mut width = match self.width {
            Some(d) if d.is_intrinsic() => {
                if constraints.max_width.is_finite() {
                    constraints.min_width.max(0.0).min(constraints.max_width)
                } else {
                    constraints.min_width.max(0.0)
                }
            }
            Some(d) if d.is_auto() => {
                if is_childless_spacer {
                    constraints.min_width
                } else if constraints.max_width.is_finite() {
                    constraints.max_width
                } else {
                    100.0
                }
            }
            Some(d) => d.resolve(cb_w),
            None => {
                if is_childless_spacer {
                    constraints.min_width
                } else if constraints.max_width.is_finite() {
                    constraints.max_width
                } else {
                    100.0
                }
            }
        };
        let mut height = match self.height {
            Some(d) if d.is_intrinsic() => {
                if constraints.max_height.is_finite() {
                    constraints.min_height.max(0.0).min(constraints.max_height)
                } else {
                    constraints.min_height.max(0.0)
                }
            }
            Some(d) if d.is_auto() => {
                if is_childless_spacer {
                    constraints.min_height
                } else if constraints.max_height.is_finite() {
                    constraints.max_height
                } else {
                    100.0
                }
            }
            Some(d) => d.resolve(cb_h),
            None => {
                if is_childless_spacer {
                    constraints.min_height
                } else if constraints.max_height.is_finite() {
                    constraints.max_height
                } else {
                    100.0
                }
            }
        };
        if let Some(min_w) = self.mss.min_width { width = width.max(min_w.resolve(cb_w)); }
        if let Some(max_w) = self.mss.max_width { width = width.min(max_w.resolve(cb_w)); }
        if let Some(min_h) = self.mss.min_height { height = height.max(min_h.resolve(cb_h)); }
        if let Some(max_h) = self.mss.max_height { height = height.min(max_h.resolve(cb_h)); }
        self.bounds = Rect::new(Point::zero(), Size::new(width, height));
        Size::new(width, height)
    }

    fn build_display_list(&self, list: &mut DisplayList, _clip: Rect) {
        if self.is_plain_render() {
            let resolve_size = self.bounds.size.width.min(self.bounds.size.height);
            let radii = self.mss.border_radius
                .map(|dims| dims.map(|d| d.resolve(resolve_size)))
                .unwrap_or(self.corner_radius);
            list.push_rect(self.bounds, self.background, radii);
            return;
        }

        let target = self.mss.target_props(self.hover, false, false, false);

        let eff_opacity = self.keyframe_opacity()
            .or(self.mss.transition.opacity())
            .or(target.opacity())
            .or(self.mss.opacity);
        if let Some(opacity) = eff_opacity {
            list.push_opacity(opacity);
        }
        let bg = self.mss.transition.background_color()
            .or(target.background_color())
            .unwrap_or(self.background);

        let resolve_size = self.bounds.size.width.min(self.bounds.size.height);
        let radii = self.mss.border_radius
            .map(|dims| dims.map(|d| d.resolve(resolve_size)))
            .unwrap_or(self.corner_radius);

        let active_shadows = self.active_box_shadow(&target);
        if let Some(ref shadows) = active_shadows {
            for shadow in &shadows.0 {
                if !shadow.inset {
                    list.push_shadow(
                        self.bounds,
                        shadow.color,
                        shadow.blur_radius,
                        (shadow.offset_x, shadow.offset_y),
                        radii,
                    );
                }
            }
        } else if let Some((color, blur, ox, oy)) = self.shadow {
            list.push_shadow(self.bounds, color, blur, (ox, oy), radii);
        }

        if let Some(ref glow) = self.active_glow(&target) {
            for shadow in &glow.0 {
                list.push_glow_shadow(
                    self.bounds,
                    shadow.color,
                    shadow.blur_radius,
                    (shadow.offset_x, shadow.offset_y),
                    radii,
                );
            }
        }

        if let Some(filters) = &self.mss.backdrop_filter {
            if let Some(crate::effects::FilterEffect::Blur(r)) = filters.first() {
                list.push_effect_layer(
                    crate::render::display_list::Effect::BackdropBlur { radius: *r },
                    self.bounds,
                );
            }
        }
        if self.has_filter_effects(&target) {
            list.push_effect_layer(self.build_filter_effect(&target), self.bounds);
        }

        let inset_shadows: Vec<_> = active_shadows.as_ref()
            .or(self.mss.box_shadow.as_ref())
            .map(|s| s.0.iter().filter(|sh| sh.inset).copied().collect())
            .unwrap_or_default();

        let use_gradient = self.mss.background_gradient.is_some()
            && self.mss.transition.background_color().is_none()
            && target.background_color().is_none();

        if use_gradient {
            let grad = self.mss.background_gradient.as_ref().unwrap().clone();
            list.push_gradient_rect(self.bounds, grad, radii);
        } else {
            list.push_rect(self.bounds, bg, radii);
        }

        if let Some(tint) = self.mss.color_tint {
            list.push_rect(self.bounds, tint, radii);
        }

        for shadow in &inset_shadows {
            list.push_inner_shadow(
                self.bounds,
                shadow.color,
                shadow.blur_radius,
                (shadow.offset_x, shadow.offset_y),
                radii,
            );
        }
    }

    fn post_build_display_list(&self, list: &mut DisplayList, _clip: Rect) {
        if self.is_plain_render() {
            return;
        }

        let target = self.mss.target_props(self.hover, false, false, false);
        let eff_opacity = self.keyframe_opacity()
            .or(self.mss.transition.opacity())
            .or(target.opacity())
            .or(self.mss.opacity);

        let needs_border = self.border.is_some()
            || self.border_sides.iter().any(|s| s.is_some())
            || self.mss.transition.border_color().is_some()
            || target.border_color().is_some()
            || self.mss.keyframe_animation.as_ref().map_or(false, |a| a.is_running());

        if needs_border {
            let resolve_size = self.bounds.size.width.min(self.bounds.size.height);
            let radii = self.mss.border_radius
                .map(|dims| dims.map(|d| d.resolve(resolve_size)))
                .unwrap_or(self.corner_radius);

            let keyframe_bc = self.mss.keyframe_animation.as_ref()
                .filter(|a| a.is_running())
                .and_then(|a| a.current_values().border_color());
            let border = if let Some(bc) = keyframe_bc
                .or(self.mss.transition.border_color())
                .or(target.border_color())
            {
                let bw = self.border.map(|b| b.width).unwrap_or(0.0);
                Some(Border::new(bw, bc))
            } else {
                self.border
            };

            let has_per_side = self.border_sides.iter().any(|s| s.is_some());
            if has_per_side {
                if let Some(border) = border {
                    let fill = Color::new(border.color.r, border.color.g, border.color.b, 0.0);
                    list.push_rect_bordered(self.bounds, fill, radii, border);
                }
                let mut groups: Vec<(Color, [f32; 4])> = Vec::new();
                for (i, side) in self.border_sides.iter().enumerate() {
                    if let Some((w, c)) = side {
                        if let Some(g) = groups.iter_mut().find(|(gc, _)| gc == c) {
                            g.1[i] = *w;
                        } else {
                            let mut widths = [0.0f32; 4];
                            widths[i] = *w;
                            groups.push((*c, widths));
                        }
                    }
                }
                for (color, widths) in &groups {
                    list.push_rect_per_side_border(
                        self.bounds,
                        Color::TRANSPARENT,
                        radii,
                        None,
                        crate::render::PerSideBorder { widths: *widths, color: *color },
                    );
                }
            } else if let Some(border) = border {
                let fill = Color::new(border.color.r, border.color.g, border.color.b, 0.0);
                list.push_rect_bordered(self.bounds, fill, radii, border);
            }
        }

        if self.has_filter_effects(&target) {
            list.pop_effect_layer();
        }
        if self.mss.backdrop_filter.as_ref().map_or(false, |f| !f.is_empty()) {
            list.pop_effect_layer();
        }

        let eff_outline_width = self.mss.transition.outline_width()
            .or(target.outline_width())
            .or(self.mss.outline_width);
        if let Some(outline_width) = eff_outline_width {
            if outline_width > 0.01 {
                let resolve_size = self.bounds.size.width.min(self.bounds.size.height);
                let radii = self.mss.border_radius
                    .map(|dims| dims.map(|d| d.resolve(resolve_size)))
                    .unwrap_or(self.corner_radius);
                let eff_outline_color = self.mss.transition.outline_color()
                    .or(target.outline_color())
                    .or(self.mss.outline_color)
                    .unwrap_or(crate::Color::new(0.0955, 0.3005, 0.9130, 1.0));
                let offset = self.mss.outline_offset.unwrap_or(2.0);
                list.push_outline(self.bounds, eff_outline_color, outline_width, offset, radii);
            }
        }

        if eff_opacity.is_some() {
            list.pop_opacity();
        }

        let _ = &target;
    }

    fn handle_event(&mut self, event: &Event, ctx: &mut crate::widget::context::EventContext) -> EventResult {
        if let Event::MouseMove(pos) = event {
            if let Some(cursor) = self.mss.cursor {
                if self.bounds.contains(*pos) {
                    ctx.set_cursor(cursor);
                }
            }
            if self.mss.has_mss_styles {
                let was_hover = self.hover;
                self.hover = self.bounds.contains(*pos);
                if self.hover != was_hover {
                    self.start_transition_to_current_state();
                    ctx.request_paint();
                    return EventResult::Handled;
                }
            }
        }
        EventResult::Ignored
    }

    fn animate(&mut self, dt: Duration) -> bool {
        let dt_secs = dt.as_secs_f32();
        let transition_active = self.mss.transition.tick(dt_secs);
        let keyframe_active = self.mss.keyframe_animation
            .as_mut()
            .map(|a| a.tick(dt_secs))
            .unwrap_or(false);
        transition_active || keyframe_active
    }

    fn needs_repaint(&self) -> bool {
        self.mss.transition.is_animating()
            || self.mss.keyframe_animation.as_ref().map_or(false, |a| a.is_running())
    }

    fn children(&self) -> &[ElementId] {
        static EMPTY: &[ElementId] = &[];
        match self.child_id {
            Some(ref id) => std::slice::from_ref(id),
            None => EMPTY,
        }
    }

    fn bounds(&self) -> Rect { self.bounds }
    fn set_position(&mut self, pos: Point) { self.bounds.origin = pos; }
    fn mark_dirty(&mut self, flags: DirtyFlags) { self.dirty_flags |= flags; }
    fn clear_dirty(&mut self, flags: DirtyFlags) { self.dirty_flags.remove(flags); }
    fn is_dirty(&self, flags: DirtyFlags) -> bool { self.dirty_flags.contains(flags) }
    fn id(&self) -> ElementId { self.id }
    fn set_id(&mut self, id: ElementId) { self.id = id; }
    fn mount(&mut self, _tree: &mut ElementTree) {}

    fn element_type_name(&self) -> &str { "DecoratedBox" }

    fn explicit_dimensions(&self, parent_width: f32, parent_height: f32) -> (Option<f32>, Option<f32>) {
        (
            self.width.and_then(|d| d.resolve_opt(parent_width)),
            self.height.and_then(|d| d.resolve_opt(parent_height)),
        )
    }

    fn min_max_dimensions(&self, parent_width: f32, parent_height: f32)
        -> (Option<f32>, Option<f32>, Option<f32>, Option<f32>)
    {
        (
            self.mss.min_width.and_then(|d| d.resolve_opt(parent_width)),
            self.mss.max_width.and_then(|d| d.resolve_opt(parent_width)),
            self.mss.min_height.and_then(|d| d.resolve_opt(parent_height)),
            self.mss.max_height.and_then(|d| d.resolve_opt(parent_height)),
        )
    }

    fn layout_hint(&self) -> LayoutHint {
        let has_padding = self.padding_left > 0.0 || self.padding_right > 0.0
            || self.padding_top > 0.0 || self.padding_bottom > 0.0;
        let has_size_constraints = self.width.is_some() || self.height.is_some()
            || self.mss.min_width.is_some() || self.mss.max_width.is_some()
            || self.mss.min_height.is_some() || self.mss.max_height.is_some();
        if has_size_constraints {
            LayoutHint::Container {
                left: self.padding_left, top: self.padding_top,
                right: self.padding_right, bottom: self.padding_bottom,
            }
        } else if has_padding {
            LayoutHint::Padding {
                left: self.padding_left, top: self.padding_top,
                right: self.padding_right, bottom: self.padding_bottom,
            }
        } else {
            LayoutHint::Padding {
                left: 0.0, top: 0.0, right: 0.0, bottom: 0.0,
            }
        }
    }

    fn clip_content(&self) -> bool {
        self.clip || matches!(self.mss.overflow, Some(Overflow::Hidden) | Some(Overflow::Scroll))
    }

    fn clip_corner_radius(&self) -> [f32; 4] {
        if self.clip_content() {
            let resolve_size = self.bounds.size.width.min(self.bounds.size.height);
            self.mss.border_radius
                .map(|dims| dims.map(|d| d.resolve(resolve_size)))
                .unwrap_or(self.corner_radius)
        } else {
            [0.0; 4]
        }
    }

    fn set_classes(&mut self, classes: Vec<String>) { self.classes = classes; self.mark_dirty(DirtyFlags::RENDER); }
    fn get_classes(&self) -> &[String] { &self.classes }
    fn reset_mss_styles(&mut self) { self.mss.reset(); }
    fn apply_computed_style(&mut self, style: &ComputedStyle) { self.apply_style(style); }
    fn mss(&self) -> Option<&crate::mss::MssFields> { Some(&self.mss) }

    fn apply_transition_styles(
        &mut self,
        base: &ComputedStyle,
        hover: Option<&ComputedStyle>,
        _active: Option<&ComputedStyle>,
        _focus: Option<&ComputedStyle>,
        _selected: Option<&ComputedStyle>,
        _checked: Option<&ComputedStyle>,
    ) {
        self.mss.apply_transitions(base, hover, None, None, None);
    }

    fn setup_keyframe_animation(
        &mut self,
        style: &crate::mss::ComputedStyle,
        stylesheet: &crate::mss::StyleSheet,
    ) {
        self.mss.setup_keyframe_animation(style, stylesheet);
    }
}

impl StyledElement for DecoratedBoxElement {
    fn apply_style(&mut self, style: &ComputedStyle) {
        self.mss.apply(style);

        self.background = self.mss.background_color.unwrap_or(Color::TRANSPARENT);
        self.padding_left = self.mss.padding_left.unwrap_or(0.0);
        self.padding_right = self.mss.padding_right.unwrap_or(0.0);
        self.padding_top = self.mss.padding_top.unwrap_or(0.0);
        self.padding_bottom = self.mss.padding_bottom.unwrap_or(0.0);
        self.width = self.mss.width;
        self.height = self.mss.height;

        self.shadow = self.mss.box_shadow.as_ref()
            .and_then(|s| s.0.first())
            .map(|s| (s.color, s.blur_radius, s.offset_x, s.offset_y))
            .filter(|_| {
                !self.mss.box_shadow.as_ref()
                    .and_then(|s| s.0.first())
                    .map(|s| s.inset)
                    .unwrap_or(false)
            });

        let mss_bw = self.mss.border_width_or(0.0);
        self.border = if mss_bw > 0.0 {
            self.mss.border_color.map(|bc| Border::new(mss_bw, bc))
        } else {
            None
        };

        self.border_sides = [None; 4];
        let default_bc = self.mss.border_color;
        let mut raw_sides: [Option<(f32, Color)>; 4] = [None; 4];
        let hidden = |prop: &str| {
            matches!(style.get(prop).and_then(|v| v.as_string()), Some("none") | Some("hidden"))
        };
        let all_hidden = hidden("border-style");
        for (i, (width_prop, color_prop, style_prop)) in [
            ("border-left-width", "border-left-color", "border-left-style"),
            ("border-top-width", "border-top-color", "border-top-style"),
            ("border-right-width", "border-right-color", "border-right-style"),
            ("border-bottom-width", "border-bottom-color", "border-bottom-style"),
        ].iter().enumerate() {
            if all_hidden || hidden(style_prop) {
                continue;
            }
            if let Some(w) = style.get(width_prop).and_then(|v| v.as_px()) {
                let color = style.get(color_prop)
                    .and_then(|v| v.as_color())
                    .map(|c| Color::from_srgb(c.r, c.g, c.b, c.a as f32 / 255.0))
                    .or(default_bc)
                    .unwrap_or(Color::BLACK);
                raw_sides[i] = Some((w, color));
            }
        }
        let uniform_equivalent = raw_sides.iter().all(|s| s.is_some()) && {
            let (w0, c0) = raw_sides[0].unwrap();
            raw_sides.iter().all(|s| s.map(|(w, c)| w == w0 && c == c0).unwrap_or(false))
                && self.border.map(|b| b.width == w0 && b.color == c0).unwrap_or(false)
        };
        if !uniform_equivalent {
            self.border_sides = raw_sides;
        }

        self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
    }
    fn classes(&self) -> &[String] { &self.classes }
    fn set_classes(&mut self, classes: Vec<String>) { self.classes = classes; self.mark_dirty(DirtyFlags::RENDER); }
}
