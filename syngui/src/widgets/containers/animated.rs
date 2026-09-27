use crate::animation::Animation;
use crate::core::{Point, Rect, Size, Transform};
use crate::input::{Event, EventResult};
use crate::layout::Constraints;
use crate::mss::ComputedStyle;
use crate::mss::MssFields;
use crate::render::DisplayList;
use crate::widget::context::EventContext;
use crate::widget::{
    DirtyFlags, Element, ElementId, ElementTree, LayoutHint, StyledElement, UpdateContext, Widget,
};
use std::any::Any;
use std::time::Duration;

#[derive(Clone, Copy, Debug)]
pub enum TransformOrigin {
    TopLeft,
    Center,
    Custom(f32, f32),
}

impl Default for TransformOrigin {
    fn default() -> Self {
        Self::Center
    }
}

#[derive(Clone, Copy, Debug)]
pub enum RepeatMode {
    None,
    Count(u32),
    PingPong(u32),
}

impl Default for RepeatMode {
    fn default() -> Self {
        Self::None
    }
}

pub struct Animated {
    child: Box<dyn Widget>,
    translate_x: Option<Animation>,
    translate_y: Option<Animation>,
    scale: Option<Animation>,
    scale_x: Option<Animation>,
    scale_y: Option<Animation>,
    rotate: Option<Animation>,
    rotate_x: Option<Animation>,
    rotate_y: Option<Animation>,
    translate_z: Option<Animation>,
    perspective: Option<f32>,
    backface_visible: bool,
    opacity_anim: Option<Animation>,
    repeat_mode: RepeatMode,
    origin: TransformOrigin,
}

impl Animated {
    pub fn new(child: impl Widget + 'static) -> Self {
        Self {
            child: Box::new(child),
            translate_x: None,
            translate_y: None,
            scale: None,
            scale_x: None,
            scale_y: None,
            rotate: None,
            rotate_x: None,
            rotate_y: None,
            translate_z: None,
            perspective: None,
            backface_visible: true,
            opacity_anim: None,
            repeat_mode: RepeatMode::None,
            origin: TransformOrigin::default(),
        }
    }

    /// 3D-поворот вокруг горизонтальной оси, градусы.
    pub fn rotate_x(mut self, anim: Animation) -> Self {
        self.rotate_x = Some(anim);
        self
    }

    /// 3D-поворот вокруг вертикальной оси, градусы (переворот карточки,
    /// вращение значка).
    pub fn rotate_y(mut self, anim: Animation) -> Self {
        self.rotate_y = Some(anim);
        self
    }

    /// Сдвиг к зрителю, px (с перспективой — увеличение).
    pub fn translate_z(mut self, anim: Animation) -> Self {
        self.translate_z = Some(anim);
        self
    }

    /// Расстояние до зрителя для 3D, px (`0` — без перспективы). По
    /// умолчанию — 4 × больший размер элемента.
    pub fn perspective(mut self, px: f32) -> Self {
        self.perspective = Some(px);
        self
    }

    /// Показывать ли обратную сторону при 3D-повороте.
    pub fn backface_visible(mut self, visible: bool) -> Self {
        self.backface_visible = visible;
        self
    }

    pub fn translate_x(mut self, anim: Animation) -> Self {
        self.translate_x = Some(anim);
        self
    }

    pub fn translate_y(mut self, anim: Animation) -> Self {
        self.translate_y = Some(anim);
        self
    }

    pub fn scale(mut self, anim: Animation) -> Self {
        self.scale = Some(anim);
        self
    }

    pub fn scale_x(mut self, anim: Animation) -> Self {
        self.scale_x = Some(anim);
        self
    }

    pub fn scale_y(mut self, anim: Animation) -> Self {
        self.scale_y = Some(anim);
        self
    }

    pub fn rotate(mut self, anim: Animation) -> Self {
        self.rotate = Some(anim);
        self
    }

    pub fn opacity(mut self, anim: Animation) -> Self {
        self.opacity_anim = Some(anim);
        self
    }

    pub fn repeat(mut self, repeat: bool) -> Self {
        self.repeat_mode = if repeat {
            RepeatMode::Count(0)
        } else {
            RepeatMode::None
        };
        self
    }

    pub fn repeat_mode(mut self, mode: RepeatMode) -> Self {
        self.repeat_mode = mode;
        self
    }

    pub fn origin(mut self, origin: TransformOrigin) -> Self {
        self.origin = origin;
        self
    }
}

impl Widget for Animated {
    fn create_element(&self) -> Box<dyn Element> {
        Box::new(AnimatedElement {
            id: ElementId::new(),
            bounds: Rect::zero(),
            child_size: Size::zero(),
            translate_x: self.translate_x.clone(),
            translate_y: self.translate_y.clone(),
            scale: self.scale.clone(),
            scale_x: self.scale_x.clone(),
            scale_y: self.scale_y.clone(),
            rotate: self.rotate.clone(),
            rotate_x: self.rotate_x.clone(),
            rotate_y: self.rotate_y.clone(),
            translate_z: self.translate_z.clone(),
            perspective: self.perspective,
            backface_visible: self.backface_visible,
            opacity_anim: self.opacity_anim.clone(),
            repeat_mode: self.repeat_mode,
            origin: self.origin,
            reverse: false,
            remaining_repeats: match self.repeat_mode {
                RepeatMode::None => 0,
                RepeatMode::Count(0) | RepeatMode::PingPong(0) => u32::MAX,
                RepeatMode::Count(n) | RepeatMode::PingPong(n) => n,
            },
            classes: Vec::new(),
            dirty_flags: DirtyFlags::LAYOUT | DirtyFlags::RENDER | DirtyFlags::ANIMATION,
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
        let child_element = self.child.create_element();
        let child_id = tree.insert_with_type_id(
            child_element,
            Some(parent_id),
            self.child.as_any().type_id(),
        );
        self.child.mount(tree, child_id);
    }

    fn child_widgets(&self) -> Vec<&dyn Widget> {
        vec![self.child.as_ref() as &dyn Widget]
    }
}

pub struct AnimatedElement {
    id: ElementId,
    bounds: Rect,
    child_size: Size,
    translate_x: Option<Animation>,
    translate_y: Option<Animation>,
    scale: Option<Animation>,
    scale_x: Option<Animation>,
    scale_y: Option<Animation>,
    rotate: Option<Animation>,
    rotate_x: Option<Animation>,
    rotate_y: Option<Animation>,
    translate_z: Option<Animation>,
    perspective: Option<f32>,
    backface_visible: bool,
    opacity_anim: Option<Animation>,
    repeat_mode: RepeatMode,
    origin: TransformOrigin,
    reverse: bool,
    remaining_repeats: u32,
    classes: Vec<String>,
    dirty_flags: DirtyFlags,
    mss: MssFields,
}

impl AnimatedElement {
    fn any_running(&self) -> bool {
        let check = |a: &Option<Animation>| a.as_ref().map_or(false, |a| !a.is_complete());
        check(&self.translate_x)
            || check(&self.translate_y)
            || check(&self.scale)
            || check(&self.scale_x)
            || check(&self.scale_y)
            || check(&self.rotate)
            || check(&self.rotate_x)
            || check(&self.rotate_y)
            || check(&self.translate_z)
            || check(&self.opacity_anim)
    }

    fn has_3d(&self) -> bool {
        self.rotate_x.is_some() || self.rotate_y.is_some() || self.translate_z.is_some()
    }

    fn transform_3d(&self) -> crate::core::Transform3D {
        let rev = self.reverse;
        let (ox, oy) = self.transform_origin();
        crate::core::Transform3D {
            rotate_x: Self::anim_value(&self.rotate_x, rev),
            rotate_y: Self::anim_value(&self.rotate_y, rev),
            rotate_z: 0.0,
            translate_z: Self::anim_value(&self.translate_z, rev),
            perspective: self.perspective,
            origin: Point::new(ox - self.bounds.origin.x, oy - self.bounds.origin.y),
            backface_visible: self.backface_visible,
        }
    }

    fn layer_bounds(&self) -> Rect {
        Rect::new(self.bounds.origin, self.child_size)
    }

    fn has_repeats_left(&self) -> bool {
        !matches!(self.repeat_mode, RepeatMode::None) && self.remaining_repeats > 0
    }

    fn anim_value(anim: &Option<Animation>, reverse: bool) -> f32 {
        match anim {
            Some(a) => {
                let v = a.current_value();
                if reverse {
                    a.initial_value() + a.target_value() - v
                } else {
                    v
                }
            }
            None => 0.0,
        }
    }

    fn transform_origin(&self) -> (f32, f32) {
        match self.origin {
            TransformOrigin::TopLeft => (self.bounds.origin.x, self.bounds.origin.y),
            TransformOrigin::Center => (
                self.bounds.origin.x + self.child_size.width / 2.0,
                self.bounds.origin.y + self.child_size.height / 2.0,
            ),
            TransformOrigin::Custom(x, y) => (self.bounds.origin.x + x, self.bounds.origin.y + y),
        }
    }
}

impl Element for AnimatedElement {
    fn update(&mut self, widget: &dyn Widget, _ctx: &mut UpdateContext) {
        if let Some(w) = widget.as_any().downcast_ref::<Animated>() {
            self.translate_x = w.translate_x.clone();
            self.translate_y = w.translate_y.clone();
            self.scale = w.scale.clone();
            self.scale_x = w.scale_x.clone();
            self.scale_y = w.scale_y.clone();
            self.rotate = w.rotate.clone();
            self.rotate_x = w.rotate_x.clone();
            self.rotate_y = w.rotate_y.clone();
            self.translate_z = w.translate_z.clone();
            self.perspective = w.perspective;
            self.backface_visible = w.backface_visible;
            self.opacity_anim = w.opacity_anim.clone();
            self.repeat_mode = w.repeat_mode;
            self.origin = w.origin;
            self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
        }
    }

    fn layout(&mut self, constraints: Constraints) -> Size {
        if constraints.min_width == constraints.max_width
            && constraints.min_height == constraints.max_height
            && constraints.max_width > 0.0
        {
            self.child_size = Size::new(constraints.max_width, constraints.max_height);
        }
        Size::zero()
    }

    fn set_content_size(&mut self, size: Size) {
        self.bounds.size = size;
        if size.width > 0.0 || size.height > 0.0 {
            self.child_size = size;
        }
    }

    fn build_display_list(&self, list: &mut DisplayList, _clip: Rect) {
        let rev = self.reverse;
        let tx = Self::anim_value(&self.translate_x, rev);
        let ty = Self::anim_value(&self.translate_y, rev);

        let su = self.scale.as_ref().map_or(1.0, |a| {
            let v = a.current_value();
            if rev {
                a.initial_value() + a.target_value() - v
            } else {
                v
            }
        });
        let sxi = self.scale_x.as_ref().map_or(1.0, |a| {
            let v = a.current_value();
            if rev {
                a.initial_value() + a.target_value() - v
            } else {
                v
            }
        });
        let syi = self.scale_y.as_ref().map_or(1.0, |a| {
            let v = a.current_value();
            if rev {
                a.initial_value() + a.target_value() - v
            } else {
                v
            }
        });
        let sx = su * sxi;
        let sy = su * syi;

        let rot_deg = self.rotate.as_ref().map_or(0.0, |a| {
            let v = a.current_value();
            if rev {
                a.initial_value() + a.target_value() - v
            } else {
                v
            }
        });

        let has_transform = self.translate_x.is_some()
            || self.translate_y.is_some()
            || self.scale.is_some()
            || self.scale_x.is_some()
            || self.scale_y.is_some()
            || self.rotate.is_some();

        if has_transform {
            let (ox, oy) = self.transform_origin();
            let needs_origin = sx != 1.0 || sy != 1.0 || rot_deg != 0.0;

            let mut transform = Transform::identity();

            if needs_origin {
                transform = transform.then(&Transform::translation(-ox, -oy));
            }

            if sx != 1.0 || sy != 1.0 {
                transform = transform.then(&Transform::new(sx, 0.0, 0.0, sy, 0.0, 0.0));
            }

            if rot_deg != 0.0 {
                let radians = rot_deg * std::f32::consts::PI / 180.0;
                transform = transform.then_rotate(euclid::Angle::radians(radians));
            }

            if needs_origin {
                transform = transform.then_translate(euclid::Vector2D::new(ox + tx, oy + ty));
            } else if tx != 0.0 || ty != 0.0 {
                transform = transform.then_translate(euclid::Vector2D::new(tx, ty));
            }

            list.push_transform(transform);
        }

        if self.has_3d() {
            let t = self.transform_3d();
            list.push_projected_layer(self.layer_bounds(), Some(&t), None);
        }

        if self.opacity_anim.is_some() {
            let opacity = Self::anim_value(&self.opacity_anim, rev).max(0.0).min(1.0);
            list.push_opacity(opacity);
        }
    }

    fn post_build_display_list(&self, list: &mut DisplayList, _clip: Rect) {
        if self.opacity_anim.is_some() {
            list.pop_opacity();
        }

        if self.has_3d() {
            list.pop_effect_layer();
        }

        if self.translate_x.is_some()
            || self.translate_y.is_some()
            || self.scale.is_some()
            || self.scale_x.is_some()
            || self.scale_y.is_some()
            || self.rotate.is_some()
        {
            list.pop_transform();
        }
    }

    fn animate(&mut self, dt: Duration) -> bool {
        let mut all_complete = true;

        let anims: [&mut Option<Animation>; 10] = [
            &mut self.translate_x,
            &mut self.translate_y,
            &mut self.scale,
            &mut self.scale_x,
            &mut self.scale_y,
            &mut self.rotate,
            &mut self.rotate_x,
            &mut self.rotate_y,
            &mut self.translate_z,
            &mut self.opacity_anim,
        ];

        for anim in anims {
            if let Some(a) = anim {
                if a.tick(dt) {
                    all_complete = false;
                }
            }
        }

        if all_complete && self.has_repeats_left() {
            if self.remaining_repeats != u32::MAX {
                self.remaining_repeats -= 1;
            }

            if self.remaining_repeats > 0 || self.remaining_repeats == 0 {
                if matches!(self.repeat_mode, RepeatMode::PingPong(_)) {
                    self.reverse = !self.reverse;
                }

                let reset_anims: [&mut Option<Animation>; 10] = [
                    &mut self.translate_x,
                    &mut self.translate_y,
                    &mut self.scale,
                    &mut self.scale_x,
                    &mut self.scale_y,
                    &mut self.rotate,
                    &mut self.rotate_x,
                    &mut self.rotate_y,
                    &mut self.translate_z,
                    &mut self.opacity_anim,
                ];
                for anim in reset_anims {
                    if let Some(a) = anim {
                        a.reset();
                    }
                }
                all_complete = false;
            }
        }

        if !all_complete {
            self.mark_dirty(DirtyFlags::RENDER);
        }
        !all_complete
    }

    fn needs_repaint(&self) -> bool {
        self.any_running() || self.has_repeats_left()
    }

    fn handle_event(&mut self, _event: &Event, _ctx: &mut EventContext) -> EventResult {
        EventResult::Ignored
    }

    fn children(&self) -> &[ElementId] {
        &[]
    }

    fn bounds(&self) -> Rect {
        self.bounds
    }

    fn set_position(&mut self, pos: Point) {
        self.bounds.origin = pos;
    }

    fn mark_dirty(&mut self, flags: DirtyFlags) {
        self.dirty_flags |= flags;
    }

    fn clear_dirty(&mut self, flags: DirtyFlags) {
        self.dirty_flags.remove(flags);
    }

    fn is_dirty(&self, flags: DirtyFlags) -> bool {
        self.dirty_flags.contains(flags)
    }

    fn id(&self) -> ElementId {
        self.id
    }

    fn set_id(&mut self, id: ElementId) {
        self.id = id;
    }

    fn mount(&mut self, _tree: &mut ElementTree) {}

    fn element_type_name(&self) -> &str {
        "Animated"
    }

    fn layout_hint(&self) -> LayoutHint {
        LayoutHint::Padding {
            left: 0.0,
            top: 0.0,
            right: 0.0,
            bottom: 0.0,
        }
    }

    fn set_classes(&mut self, classes: Vec<String>) {
        self.classes = classes;
        self.mark_dirty(DirtyFlags::RENDER);
    }

    fn get_classes(&self) -> &[String] {
        &self.classes
    }

    fn reset_mss_styles(&mut self) {
        self.mss.reset();
    }
    fn mss(&self) -> Option<&crate::mss::MssFields> {
        Some(&self.mss)
    }
    fn apply_computed_style(&mut self, style: &ComputedStyle) {
        self.mss.apply(style);
        self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
    }

    fn apply_transition_styles(
        &mut self,
        base: &ComputedStyle,
        hover: Option<&ComputedStyle>,
        active: Option<&ComputedStyle>,
        focus: Option<&ComputedStyle>,
        selected: Option<&ComputedStyle>,
        _checked: Option<&ComputedStyle>,
    ) {
        self.mss
            .apply_transitions(base, hover, active, focus, selected);
    }
}

impl StyledElement for AnimatedElement {
    fn apply_style(&mut self, _style: &ComputedStyle) {
        self.mark_dirty(DirtyFlags::RENDER);
    }

    fn classes(&self) -> &[String] {
        &self.classes
    }

    fn set_classes(&mut self, classes: Vec<String>) {
        self.classes = classes;
        self.mark_dirty(DirtyFlags::RENDER);
    }
}
