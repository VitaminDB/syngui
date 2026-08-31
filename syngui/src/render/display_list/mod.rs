mod types;
pub use types::*;

use crate::core::Color;
use crate::mss::{TextAlign, TextDecoration};
use crate::render::{ClipRect, TextureId};
use crate::core::{Rect, Point, Size};
use compact_str::CompactString;

#[derive(Debug, Default)]
pub struct DisplayList {
    commands: Vec<DrawCommand>,
    overlay_levels: Vec<Vec<DrawCommand>>,
    overlay_depth: usize,
    clip_stack: Vec<ClipRect>,
    saved_clip_stacks: Vec<Vec<ClipRect>>,
    transform_stack: Vec<crate::core::Transform>,
    current_transform: crate::core::Transform,
    overlay_base_transforms: Vec<crate::core::Transform>,
    saved_transforms: Vec<crate::core::Transform>,
    current_z: u32,
    surface_size: Size,
    scale_factor: f32,
}

impl DisplayList {
    pub fn new() -> Self {
        Self {
            commands: Vec::with_capacity(1024),
            overlay_levels: Vec::new(),
            overlay_depth: 0,
            clip_stack: vec![ClipRect::full_screen()],
            saved_clip_stacks: Vec::new(),
            transform_stack: Vec::new(),
            current_transform: crate::core::Transform::identity(),
            overlay_base_transforms: Vec::new(),
            saved_transforms: Vec::new(),
            current_z: 0,
            surface_size: Size::zero(),
            scale_factor: 1.0,
        }
    }

    pub fn set_surface_size(&mut self, size: Size) {
        self.surface_size = size;
    }

    pub fn surface_size(&self) -> Size {
        self.surface_size
    }

    pub fn set_scale_factor(&mut self, sf: f32) {
        self.scale_factor = sf;
    }

    pub fn scale_factor(&self) -> f32 {
        self.scale_factor
    }

    #[inline]
    fn clamp_corner_radius(rect: &Rect, radius: [f32; 4]) -> [f32; 4] {
        let max = (rect.size.width.min(rect.size.height) * 0.5).max(0.0);
        [
            radius[0].clamp(0.0, max),
            radius[1].clamp(0.0, max),
            radius[2].clamp(0.0, max),
            radius[3].clamp(0.0, max),
        ]
    }

    pub fn add_rect(&mut self, rect: Rect, color: Color, clip: Rect) {
        self.commands.push(DrawCommand::Rect {
            rect,
            color,
            corner_radius: [0.0; 4],
            border: None,
            per_side_border: None,
            clip_rect: ClipRect::from_rect(clip),
            z_index: self.current_z,
        });
        self.current_z += 1;
    }

    pub fn add_text(&mut self, text: &str, pos: Point, color: Color, font_size: f32, clip: Rect) {
        let width = text.chars().count() as f32 * font_size * 0.65;
        let height = font_size * 1.2;
        let size = Size::new(width, height);
        let rect = Rect::new(pos, size);
        
        self.commands.push(DrawCommand::Text {
            text: CompactString::from(text),
            rect,
            color,
            font_size,
            font_weight: 400,
            text_align: TextAlign::DEFAULT,
            decoration: TextDecoration::None,
            font_family: None,
            letter_spacing: 0.0,
            text_shadow: None,
            bbox_sample: None,
            clip_rect: ClipRect::from_rect(clip),
            z_index: self.current_z,
            no_wrap: false,
        });
        self.current_z += 1;
    }

    pub fn add_shadow(
        &mut self,
        rect: Rect,
        color: Color,
        blur_radius: f32,
        offset: (f32, f32),
        corner_radius: [f32; 4],
        clip: Rect,
    ) {
        let corner_radius = Self::clamp_corner_radius(&rect, corner_radius);
        self.commands.push(DrawCommand::Shadow {
            rect,
            color,
            blur_radius,
            offset,
            corner_radius,
            inset: false,
            clip_rect: ClipRect::from_rect(clip),
            z_index: self.current_z,
        });
        self.current_z += 1;
    }

    pub fn push_rect(&mut self, rect: crate::core::Rect, color: Color, radius: [f32; 4]) {
        let radius = Self::clamp_corner_radius(&rect, radius);
        let clip = *self.current_clip();
        let z = self.current_z;
        self.target().push(DrawCommand::Rect {
            rect,
            color,
            corner_radius: radius,
            border: None,
            per_side_border: None,
            clip_rect: clip,
            z_index: z,
        });
        self.current_z += 1;
    }

    pub fn push_gradient_rect(&mut self, rect: crate::core::Rect, gradient: crate::core::Gradient, radius: [f32; 4]) {
        let radius = Self::clamp_corner_radius(&rect, radius);
        let clip = *self.current_clip();
        let z = self.current_z;
        self.target().push(DrawCommand::GradientRect {
            rect,
            gradient,
            corner_radius: radius,
            border: None,
            per_side_border: None,
            clip_rect: clip,
            z_index: z,
        });
        self.current_z += 1;
    }

    pub fn push_gradient_rect_bordered(&mut self, rect: crate::core::Rect, gradient: crate::core::Gradient, radius: [f32; 4], border: Border) {
        let radius = Self::clamp_corner_radius(&rect, radius);
        let clip = *self.current_clip();
        let z = self.current_z;
        self.target().push(DrawCommand::GradientRect {
            rect,
            gradient,
            corner_radius: radius,
            border: Some(border),
            per_side_border: None,
            clip_rect: clip,
            z_index: z,
        });
        self.current_z += 1;
    }

    pub fn push_rect_bordered(&mut self, rect: crate::core::Rect, color: Color, radius: [f32; 4], border: Border) {
        let radius = Self::clamp_corner_radius(&rect, radius);
        let clip = *self.current_clip();
        let z = self.current_z;
        self.target().push(DrawCommand::Rect {
            rect,
            color,
            corner_radius: radius,
            border: Some(border),
            per_side_border: None,
            clip_rect: clip,
            z_index: z,
        });
        self.current_z += 1;
    }

    pub fn push_rect_per_side_border(
        &mut self,
        rect: crate::core::Rect,
        color: Color,
        radius: [f32; 4],
        border: Option<Border>,
        per_side: PerSideBorder,
    ) {
        let radius = Self::clamp_corner_radius(&rect, radius);
        let clip = *self.current_clip();
        let z = self.current_z;
        self.target().push(DrawCommand::Rect {
            rect,
            color,
            corner_radius: radius,
            border,
            per_side_border: Some(per_side),
            clip_rect: clip,
            z_index: z,
        });
        self.current_z += 1;
    }

    pub fn push_text(&mut self, text: &str, rect: crate::core::Rect, color: Color, font_size: f32) {
        let clip = *self.current_clip();
        let z = self.current_z;
        self.target().push(DrawCommand::Text {
            text: CompactString::from(text),
            rect,
            color,
            font_size,
            font_weight: 400,
            text_align: TextAlign::DEFAULT,
            decoration: TextDecoration::None,
            font_family: None,
            letter_spacing: 0.0,
            text_shadow: None,
            bbox_sample: None,
            clip_rect: clip,
            z_index: z,
            no_wrap: false,
        });
        self.current_z += 1;
    }

    pub fn push_text_centered(&mut self, text: &str, rect: crate::core::Rect, color: Color, font_size: f32) {
        let clip = *self.current_clip();
        let z = self.current_z;
        self.target().push(DrawCommand::Text {
            text: CompactString::from(text),
            rect,
            color,
            font_size,
            font_weight: 400,
            text_align: TextAlign::CENTER,
            decoration: TextDecoration::None,
            font_family: None,
            letter_spacing: 0.0,
            text_shadow: None,
            bbox_sample: None,
            clip_rect: clip,
            z_index: z,
            no_wrap: false,
        });
        self.current_z += 1;
    }

    pub fn push_text_aligned(&mut self, text: &str, rect: crate::core::Rect, color: Color, font_size: f32, align: TextAlign, decoration: TextDecoration, font_weight: u16) {
        let clip = *self.current_clip();
        let z = self.current_z;
        self.target().push(DrawCommand::Text {
            text: CompactString::from(text),
            rect,
            color,
            font_size,
            font_weight,
            text_align: align,
            decoration,
            font_family: None,
            letter_spacing: 0.0,
            text_shadow: None,
            bbox_sample: None,
            clip_rect: clip,
            z_index: z,
            no_wrap: false,
        });
        self.current_z += 1;
    }

    /// Текст без переноса: рисуется одной строкой, лишнее обрезается текущим
    /// клипом. Для ячеек таблиц, где узкая колонка иначе ломала бы дату на
    /// несколько строк.
    pub fn push_text_singleline(&mut self, text: &str, rect: crate::core::Rect, color: Color, font_size: f32, align: TextAlign, font_weight: u16) {
        let clip = *self.current_clip();
        let z = self.current_z;
        self.target().push(DrawCommand::Text {
            text: CompactString::from(text),
            rect,
            color,
            font_size,
            font_weight,
            text_align: align,
            decoration: TextDecoration::None,
            font_family: None,
            letter_spacing: 0.0,
            text_shadow: None,
            bbox_sample: None,
            clip_rect: clip,
            z_index: z,
            no_wrap: true,
        });
        self.current_z += 1;
    }

    /// Как `push_text_styled`, но без переноса: текст рисуется одной строкой,
    /// лишнее обрезается клипом. Для подписей контролов (Dropdown, кнопки),
    /// где узкий бокс иначе ломал бы подпись на несколько строк.
    #[allow(clippy::too_many_arguments)]
    pub fn push_text_styled_singleline(&mut self, text: &str, rect: crate::core::Rect, color: Color, font_size: f32, align: TextAlign, decoration: TextDecoration, font_weight: u16, font_family: Option<String>) {
        let clip = *self.current_clip();
        let z = self.current_z;
        self.target().push(DrawCommand::Text {
            text: CompactString::from(text),
            rect,
            color,
            font_size,
            font_weight,
            text_align: align,
            decoration,
            font_family: font_family.map(CompactString::from),
            letter_spacing: 0.0,
            text_shadow: None,
            bbox_sample: None,
            clip_rect: clip,
            z_index: z,
            no_wrap: true,
        });
        self.current_z += 1;
    }

    pub fn push_text_styled(&mut self, text: &str, rect: crate::core::Rect, color: Color, font_size: f32, align: TextAlign, decoration: TextDecoration, font_weight: u16, font_family: Option<String>) {
        let clip = *self.current_clip();
        let z = self.current_z;
        self.target().push(DrawCommand::Text {
            text: CompactString::from(text),
            rect,
            color,
            font_size,
            font_weight,
            text_align: align,
            decoration,
            font_family: font_family.map(CompactString::from),
            letter_spacing: 0.0,
            text_shadow: None,
            bbox_sample: None,
            clip_rect: clip,
            z_index: z,
            no_wrap: false,
        });
        self.current_z += 1;
    }

    #[allow(clippy::too_many_arguments)]
    pub fn push_text_full(
        &mut self,
        text: &str,
        rect: crate::core::Rect,
        color: Color,
        font_size: f32,
        align: TextAlign,
        decoration: TextDecoration,
        font_weight: u16,
        font_family: Option<String>,
        letter_spacing: f32,
        text_shadow: Option<crate::mss::fields::TextShadow>,
        no_wrap: bool,
    ) {
        let clip = *self.current_clip();
        let z = self.current_z;
        self.target().push(DrawCommand::Text {
            text: CompactString::from(text),
            rect,
            color,
            font_size,
            font_weight,
            text_align: align,
            decoration,
            font_family: font_family.map(CompactString::from),
            letter_spacing,
            text_shadow,
            bbox_sample: None,
            clip_rect: clip,
            z_index: z,
            no_wrap,
        });
        self.current_z += 1;
    }

    #[allow(clippy::too_many_arguments)]
    pub fn push_text_with_bbox(
        &mut self,
        text: &str,
        rect: crate::core::Rect,
        color: Color,
        font_size: f32,
        align: TextAlign,
        decoration: TextDecoration,
        font_weight: u16,
        font_family: Option<String>,
        bbox_sample: &str,
    ) {
        let clip = *self.current_clip();
        let z = self.current_z;
        self.target().push(DrawCommand::Text {
            text: CompactString::from(text),
            rect,
            color,
            font_size,
            font_weight,
            text_align: align,
            decoration,
            font_family: font_family.map(CompactString::from),
            letter_spacing: 0.0,
            text_shadow: None,
            bbox_sample: Some(CompactString::from(bbox_sample)),
            clip_rect: clip,
            z_index: z,
            no_wrap: false,
        });
        self.current_z += 1;
    }

    pub fn push_image(
        &mut self,
        rect: crate::core::Rect,
        texture_id: crate::render::TextureId,
        uv_rect: crate::core::Rect,
        tint: Color,
    ) {
        let clip = *self.current_clip();
        let z = self.current_z;
        self.target().push(DrawCommand::Image {
            rect,
            texture_id,
            uv_rect,
            color: tint,
            clip_rect: clip,
            z_index: z,
        });
        self.current_z += 1;
    }

    pub fn push_shadow(
        &mut self,
        rect: crate::core::Rect,
        color: Color,
        blur_radius: f32,
        offset: (f32, f32),
        corner_radius: [f32; 4],
    ) {
        let corner_radius = Self::clamp_corner_radius(&rect, corner_radius);
        let clip = *self.current_clip();
        let z = self.current_z;
        self.target().push(DrawCommand::Shadow {
            rect,
            color,
            blur_radius,
            offset,
            corner_radius,
            inset: false,
            clip_rect: clip,
            z_index: z,
        });
        self.current_z += 1;
    }

    pub fn push_inner_shadow(
        &mut self,
        rect: crate::core::Rect,
        color: Color,
        blur_radius: f32,
        offset: (f32, f32),
        corner_radius: [f32; 4],
    ) {
        let corner_radius = Self::clamp_corner_radius(&rect, corner_radius);
        let clip = *self.current_clip();
        let z = self.current_z;
        self.target().push(DrawCommand::Shadow {
            rect,
            color,
            blur_radius,
            offset,
            corner_radius,
            inset: true,
            clip_rect: clip,
            z_index: z,
        });
        self.current_z += 1;
    }

    pub fn push_glow_shadow(
        &mut self,
        rect: crate::core::Rect,
        color: Color,
        blur_radius: f32,
        offset: (f32, f32),
        corner_radius: [f32; 4],
    ) {
        let corner_radius = Self::clamp_corner_radius(&rect, corner_radius);
        let clip = *self.current_clip();
        let z = self.current_z;
        self.target().push(DrawCommand::GlowShadow {
            rect,
            color,
            blur_radius,
            offset,
            corner_radius,
            clip_rect: clip,
            z_index: z,
        });
        self.current_z += 1;
    }

    pub fn push_outline(
        &mut self,
        bounds: crate::core::Rect,
        color: Color,
        width: f32,
        offset: f32,
        corner_radius: [f32; 4],
    ) {
        let clip = *self.current_clip();
        let z = self.current_z;
        let outer_expand = offset + width;
        let outer_rect = crate::core::Rect::new(
            Point::new(bounds.origin.x - outer_expand, bounds.origin.y - outer_expand),
            Size::new(
                bounds.size.width + outer_expand * 2.0,
                bounds.size.height + outer_expand * 2.0,
            ),
        );
        self.target().push(DrawCommand::Outline {
            rect: outer_rect,
            color,
            ring_width: width,
            corner_radius,
            clip_rect: clip,
            z_index: z,
        });
        self.current_z += 1;
    }

    pub fn push_text_selection(
        &mut self,
        text: &str,
        sel_start: usize,
        sel_end: usize,
        base_x: f32,
        y: f32,
        height: f32,
        font_size: f32,
        color: Color,
    ) {
        self.push_text_selection_styled(
            text, sel_start, sel_end, base_x, y, height, font_size, color, None,
        );
    }

    pub fn push_text_selection_styled(
        &mut self,
        text: &str,
        sel_start: usize,
        sel_end: usize,
        base_x: f32,
        y: f32,
        height: f32,
        font_size: f32,
        color: Color,
        font_family: Option<String>,
    ) {
        if sel_start >= sel_end {
            return;
        }
        let clip = *self.current_clip();
        let z = self.current_z;
        self.target().push(DrawCommand::TextSelection {
            text: CompactString::from(text),
            sel_start,
            sel_end,
            base_x,
            y,
            height,
            font_size,
            color,
            font_family: font_family.map(CompactString::from),
            clip_rect: clip,
            z_index: z,
        });
        self.current_z += 1;
    }

    pub fn push_text_cursor(
        &mut self,
        text: &str,
        cursor_pos: usize,
        base_x: f32,
        y: f32,
        height: f32,
        font_size: f32,
        font_weight: u16,
        color: Color,
    ) {
        self.push_text_cursor_styled(text, cursor_pos, base_x, y, height, font_size, font_weight, color, None);
    }

    pub fn push_text_cursor_styled(
        &mut self,
        text: &str,
        cursor_pos: usize,
        base_x: f32,
        y: f32,
        height: f32,
        font_size: f32,
        font_weight: u16,
        color: Color,
        font_family: Option<String>,
    ) {
        let clip = *self.current_clip();
        let z = self.current_z;
        self.target().push(DrawCommand::TextCursor {
            text: CompactString::from(text),
            cursor_pos,
            base_x,
            y,
            height,
            font_size,
            font_weight,
            color,
            font_family: font_family.map(CompactString::from),
            clip_rect: clip,
            z_index: z,
        });
        self.current_z += 1;
    }

    pub fn push_clip(&mut self, rect: crate::core::Rect) {
        let clip_rect = if self.current_transform != crate::core::Transform::identity() {
            self.current_transform.outer_transformed_rect(&rect)
        } else {
            rect
        };
        let new_clip = if let Some(current) = self.clip_stack.last() {
            current.intersect(clip_rect)
        } else {
            ClipRect::from_rect(clip_rect)
        };
        self.clip_stack.push(new_clip);
        self.target().push(DrawCommand::PushClip { rect: clip_rect });
    }

    pub fn push_clip_rounded(&mut self, rect: crate::core::Rect, corner_radius: [f32; 4]) {
        let clip_rect = if self.current_transform != crate::core::Transform::identity() {
            self.current_transform.outer_transformed_rect(&rect)
        } else {
            rect
        };

        let new_clip = if let Some(current) = self.clip_stack.last() {
            current.intersect_rounded(clip_rect, corner_radius)
        } else {
            ClipRect::from_rect_rounded(clip_rect, corner_radius)
        };
        self.clip_stack.push(new_clip);
        self.target().push(DrawCommand::PushClip { rect: clip_rect });
    }

    pub fn pop_clip(&mut self) {
        if self.clip_stack.len() > 1 {
            self.clip_stack.pop();
        }
        self.target().push(DrawCommand::PopClip);
    }

    pub fn push_z_barrier(&mut self) {
        self.target().push(DrawCommand::ZBarrier);
    }

    pub fn push_transform(&mut self, transform: crate::core::Transform) {
        self.transform_stack.push(self.current_transform);
        self.current_transform = transform.then(&self.current_transform);
        self.target().push(DrawCommand::PushTransform(transform));
    }

    pub fn pop_transform(&mut self) {
        if let Some(prev) = self.transform_stack.pop() {
            self.current_transform = prev;
        }
        self.target().push(DrawCommand::PopTransform);
    }

    pub fn push_opacity(&mut self, opacity: f32) {
        self.target().push(DrawCommand::PushOpacity(opacity));
    }

    pub fn pop_opacity(&mut self) {
        self.target().push(DrawCommand::PopOpacity);
    }

    pub fn push_effect_layer(&mut self, effect: Effect, bounds: crate::core::Rect) {
        let screen_bounds = if self.current_transform != crate::core::Transform::identity() {
            self.current_transform.outer_transformed_rect(&bounds)
        } else {
            bounds
        };
        self.target().push(DrawCommand::BeginEffectLayer { effect, bounds: screen_bounds });
    }

    pub fn pop_effect_layer(&mut self) {
        self.target().push(DrawCommand::EndEffectLayer { texture_id: TextureId(0) });
    }

    pub fn is_in_overlay(&self) -> bool {
        self.overlay_depth > 0
    }

    pub fn begin_overlay(&mut self) {
        let base = self.current_transform;
        self.begin_overlay_with_base(base);
    }

    /// Оверлей в системе координат родителя: собственный `transform`
    /// элемента (поворот кнопки, масштаб) на слой не распространяется, а
    /// трансформации предков — да.
    ///
    /// Для подсказок: подсказка у крутящейся на hover кнопки не должна
    /// крутиться вместе с ней, но обязана уезжать вместе со скроллом.
    /// Вызывать только когда элемент действительно запушил свой transform —
    /// иначе снимется трансформация предка.
    pub fn begin_overlay_parent_space(&mut self) {
        let base = self
            .transform_stack
            .last()
            .copied()
            .unwrap_or_else(crate::core::Transform::identity);
        self.begin_overlay_with_base(base);
    }

    fn begin_overlay_with_base(&mut self, transform: crate::core::Transform) {
        self.overlay_depth += 1;
        while self.overlay_levels.len() < self.overlay_depth {
            self.overlay_levels.push(Vec::new());
        }
        self.saved_clip_stacks.push(std::mem::replace(
            &mut self.clip_stack,
            vec![ClipRect::full_screen()],
        ));
        if transform != crate::core::Transform::identity() {
            self.overlay_levels[self.overlay_depth - 1].push(DrawCommand::PushTransform(transform));
        }
        self.overlay_base_transforms.push(transform);
    }

    pub fn begin_overlay_absolute(&mut self) {
        self.overlay_depth += 1;
        while self.overlay_levels.len() < self.overlay_depth {
            self.overlay_levels.push(Vec::new());
        }
        self.saved_clip_stacks.push(std::mem::replace(
            &mut self.clip_stack,
            vec![ClipRect::full_screen()],
        ));
        self.saved_transforms.push(self.current_transform);
        self.current_transform = crate::core::Transform::identity();
        self.overlay_base_transforms.push(crate::core::Transform::identity());
    }

    pub fn end_overlay(&mut self) {
        if let Some(base) = self.overlay_base_transforms.pop() {
            if self.overlay_depth > 0 && base != crate::core::Transform::identity() {
                self.overlay_levels[self.overlay_depth - 1].push(DrawCommand::PopTransform);
            }
        }
        if self.overlay_depth > 0 {
            self.overlay_depth -= 1;
        }
        if let Some(saved) = self.saved_clip_stacks.pop() {
            self.clip_stack = saved;
        }
        if let Some(saved_transform) = self.saved_transforms.pop() {
            self.current_transform = saved_transform;
        }
    }

    pub fn commands(&self) -> Vec<DrawCommand> {
        let mut all = self.commands.clone();
        for level in &self.overlay_levels {
            all.extend(level.iter().cloned());
        }
        all
    }

    pub fn iter_all_commands(&self) -> impl Iterator<Item = &DrawCommand> {
        self.commands.iter().chain(self.overlay_levels.iter().flat_map(|l| l.iter()))
    }

    pub fn normal_commands(&self) -> &[DrawCommand] {
        &self.commands
    }

    pub fn overlay_level_slices(&self) -> &[Vec<DrawCommand>] {
        &self.overlay_levels
    }

    pub fn overlay_commands(&self) -> Vec<DrawCommand> {
        let mut all = Vec::new();
        for level in &self.overlay_levels {
            all.extend(level.iter().cloned());
        }
        all
    }

    pub fn stats(&self) -> DisplayListStats {
        let overlay_count: usize = self.overlay_levels.iter().map(|l| l.len()).sum();
        DisplayListStats {
            command_count: self.commands.len(),
            overlay_command_count: overlay_count,
            capacity: self.commands.capacity(),
        }
    }

    pub fn current_command_count(&self) -> usize {
        self.target_ref().len()
    }

    pub fn clear(&mut self) {
        self.commands.clear();
        self.overlay_levels.clear();
        self.overlay_depth = 0;
        self.clip_stack.clear();
        self.clip_stack.push(ClipRect::full_screen());
        self.saved_clip_stacks.clear();
        self.transform_stack.clear();
        self.current_transform = crate::core::Transform::identity();
        self.overlay_base_transforms.clear();
        self.current_z = 0;
    }

    fn current_clip(&self) -> &ClipRect {
        self.clip_stack.last().unwrap()
    }

    fn target(&mut self) -> &mut Vec<DrawCommand> {
        if self.overlay_depth > 0 {
            &mut self.overlay_levels[self.overlay_depth - 1]
        } else {
            &mut self.commands
        }
    }

    fn target_ref(&self) -> &Vec<DrawCommand> {
        if self.overlay_depth > 0 {
            &self.overlay_levels[self.overlay_depth - 1]
        } else {
            &self.commands
        }
    }

    pub fn push_canvas(&mut self, vertices: Vec<crate::render::Vertex>, indices: Vec<u32>) {
        if vertices.is_empty() {
            return;
        }
        let clip = *self.current_clip();
        let z = self.current_z;
        self.target().push(DrawCommand::Canvas {
            vertices,
            indices,
            clip_rect: clip,
            z_index: z,
        });
        self.current_z += 1;
    }

    pub fn push_line_strip(&mut self, points: Vec<[f32; 2]>, color: Color, width: f32) {
        if points.len() < 2 {
            return;
        }
        let clip = *self.current_clip();
        let z = self.current_z;
        self.target().push(DrawCommand::LineStrip {
            points,
            color,
            width,
            clip_rect: clip,
            z_index: z,
        });
        self.current_z += 1;
    }

    pub fn push_command(&mut self, command: DrawCommand) {
        self.target().push(command);
    }
}

#[cfg(test)]
mod overlay_transform_tests {
    use super::*;
    use crate::core::{Color, Point, Rect, Size, Transform};

    fn overlay_transform(list: &DisplayList) -> Option<Transform> {
        list.overlay_level_slices()
            .iter()
            .flat_map(|level| level.iter())
            .find_map(|cmd| match cmd {
                DrawCommand::PushTransform(t) => Some(*t),
                _ => None,
            })
    }

    fn rect() -> Rect {
        Rect::new(Point::new(0.0, 0.0), Size::new(10.0, 10.0))
    }

    /// Обычный overlay тянет за собой всю текущую цепочку трансформаций —
    /// включая собственный поворот элемента.
    #[test]
    fn plain_overlay_inherits_the_whole_transform_chain() {
        let mut list = DisplayList::new();
        let parent = Transform::translation(10.0, 20.0);
        let own = Transform::translation(0.0, 5.0);
        list.push_transform(parent);
        list.push_transform(own);
        list.begin_overlay();
        list.push_rect(rect(), Color::WHITE, [0.0; 4]);
        list.end_overlay();
        list.pop_transform();
        list.pop_transform();

        assert_eq!(overlay_transform(&list), Some(own.then(&parent)));
    }

    /// Оверлей в координатах родителя отбрасывает собственный transform
    /// элемента, но сохраняет трансформации предков: подсказка не крутится
    /// вместе с кнопкой, но уезжает вместе со скроллом.
    #[test]
    fn parent_space_overlay_drops_only_the_own_transform() {
        let mut list = DisplayList::new();
        let parent = Transform::translation(10.0, 20.0);
        let own = Transform::translation(0.0, 5.0);
        list.push_transform(parent);
        list.push_transform(own);
        list.begin_overlay_parent_space();
        list.push_rect(rect(), Color::WHITE, [0.0; 4]);
        list.end_overlay();
        list.pop_transform();
        list.pop_transform();

        assert_eq!(overlay_transform(&list), Some(parent));
    }

    /// Единственный transform — свой: оверлей остаётся в экранных координатах.
    #[test]
    fn parent_space_overlay_without_ancestors_is_identity() {
        let mut list = DisplayList::new();
        list.push_transform(Transform::translation(0.0, 5.0));
        list.begin_overlay_parent_space();
        list.push_rect(rect(), Color::WHITE, [0.0; 4]);
        list.end_overlay();
        list.pop_transform();

        // identity в оверлей не пишется вовсе — команды PushTransform нет.
        assert_eq!(overlay_transform(&list), None);
    }
}
