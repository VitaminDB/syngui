use crate::core::{Color, Point, Rect, RectExt, Size};
use crate::gpu::image_store::{ImageHandle, ImageLoadState, ImageSource, ImageStore};
use crate::input::{Event, EventResult};
use crate::layout::Constraints;
use crate::mss::{ComputedStyle, Dimension};
use crate::mss::MssFields;
use crate::render::{DisplayList, TextureId};
use crate::widget::{DirtyFlags, Element, ElementId, ElementTree, StyledElement, UpdateContext, Widget};
use std::any::Any;
use std::sync::Arc;
use crate::core::sync::Mutex;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum ImageFit {
    #[default]
    Contain,
    Cover,
    Fill,
    None,
}

pub struct Image {
    source: ImageSource,
    fit: ImageFit,
    tint: Option<Color>,
    /// Рисовать ли встроенные заглушки «загрузка»/«ошибка».
    placeholder: bool,
}

impl Image {
    pub fn new(path: impl Into<String>) -> Self {
        Self {
            source: ImageSource::Path(path.into()),
            fit: ImageFit::default(),
            tint: None,
            placeholder: true,
        }
    }

    pub fn from_bytes(key: impl Into<String>, data: Vec<u8>) -> Self {
        Self {
            source: ImageSource::Bytes {
                key: key.into(),
                data: Arc::new(data),
            },
            fit: ImageFit::default(),
            tint: None,
            placeholder: true,
        }
    }

    pub fn from_url(url: impl Into<String>) -> Self {
        Self {
            source: ImageSource::Url(url.into()),
            fit: ImageFit::default(),
            tint: None,
            placeholder: true,
        }
    }

    pub fn from_rgba(key: impl Into<String>, width: u32, height: u32, rgba: Vec<u8>) -> Self {
        Self {
            source: ImageSource::RawRgba {
                key: key.into(),
                width,
                height,
                rgba: Arc::new(rgba),
            },
            fit: ImageFit::default(),
            tint: None,
            placeholder: true,
        }
    }

    pub fn fit(mut self, fit: ImageFit) -> Self {
        self.fit = fit;
        self
    }

    pub fn tint(mut self, tint: Color) -> Self {
        self.tint = Some(tint);
        self
    }

    /// Рисовать ли встроенные заглушки, пока картинка грузится или если она
    /// не загрузилась (серый/розовый прямоугольник со значком). `false` — до
    /// готовности не рисуется ничего: подложку даёт родитель (обложка с
    /// градиентом, аватар с инициалами), и битая ссылка не портит вид.
    pub fn placeholder(mut self, show: bool) -> Self {
        self.placeholder = show;
        self
    }
}

impl Widget for Image {
    fn create_element(&self) -> Box<dyn Element> {
        Box::new(ImageElement {
            id: ElementId::new(),
            source: self.source.clone(),
            width: None,
            height: None,
            fit: self.fit,
            tint: self.tint,
            placeholder: self.placeholder,
            opacity: 1.0,
            bounds: Rect::zero(),
            classes: Vec::new(),
            dirty_flags: DirtyFlags::LAYOUT | DirtyFlags::RENDER,
            image_handle: None,
            image_state: ImageLoadState::Loading,
            image_store: None,
            natural_width: None,
            natural_height: None,
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

    fn mount(&self, _tree: &mut ElementTree, _parent_id: ElementId) {}
}

pub struct ImageElement {
    id: ElementId,
    source: ImageSource,
    width: Option<Dimension>,
    height: Option<Dimension>,
    fit: ImageFit,
    tint: Option<Color>,
    placeholder: bool,
    opacity: f32,
    bounds: Rect,
    classes: Vec<String>,
    dirty_flags: DirtyFlags,
    image_handle: Option<ImageHandle>,
    image_state: ImageLoadState,
    image_store: Option<Arc<Mutex<ImageStore>>>,
    natural_width: Option<u32>,
    natural_height: Option<u32>,
    mss: MssFields,
}

impl ImageElement {
    fn request_load(&mut self) {
        if let Some(ref store) = self.image_store {
            let mut store = store.lock().unwrap();
            let (handle, state) = store.request(&self.source);
            self.image_handle = Some(handle);
            self.image_state = state;
            if state == ImageLoadState::Ready {
                if let Some((w, h)) = store.dimensions(handle) {
                    self.natural_width = Some(w);
                    self.natural_height = Some(h);
                }
            }
        }
    }

    fn compute_fit_rect(&self) -> Rect {
        let (nw, nh) = match (self.natural_width, self.natural_height) {
            (Some(w), Some(h)) if w > 0 && h > 0 => (w as f32, h as f32),
            _ => return self.bounds,
        };

        let bw = self.bounds.size.width;
        let bh = self.bounds.size.height;

        match self.fit {
            ImageFit::Fill => self.bounds,
            ImageFit::None => {
                let x = self.bounds.x() + (bw - nw) / 2.0;
                let y = self.bounds.y() + (bh - nh) / 2.0;
                Rect::new(Point::new(x, y), Size::new(nw, nh))
            }
            ImageFit::Contain => {
                let scale = (bw / nw).min(bh / nh);
                let sw = nw * scale;
                let sh = nh * scale;
                let x = self.bounds.x() + (bw - sw) / 2.0;
                let y = self.bounds.y() + (bh - sh) / 2.0;
                Rect::new(Point::new(x, y), Size::new(sw, sh))
            }
            ImageFit::Cover => {
                let scale = (bw / nw).max(bh / nh);
                let sw = nw * scale;
                let sh = nh * scale;
                let x = self.bounds.x() + (bw - sw) / 2.0;
                let y = self.bounds.y() + (bh - sh) / 2.0;
                Rect::new(Point::new(x, y), Size::new(sw, sh))
            }
        }
    }
}

impl Element for ImageElement {
    fn update(&mut self, widget: &dyn Widget, _ctx: &mut UpdateContext) {
        if let Some(image) = widget.as_any().downcast_ref::<Image>() {
            let source_changed = match (&self.source, &image.source) {
                (ImageSource::Path(a), ImageSource::Path(b)) => a != b,
                (ImageSource::Bytes { key: a, .. }, ImageSource::Bytes { key: b, .. }) => a != b,
                (ImageSource::RawRgba { key: a, .. }, ImageSource::RawRgba { key: b, .. }) => a != b,
                (ImageSource::Url(a), ImageSource::Url(b)) => a != b,
                _ => true,
            };
            if source_changed {
                self.source = image.source.clone();
                self.image_handle = None;
                self.image_state = ImageLoadState::Loading;
                self.natural_width = None;
                self.natural_height = None;
                self.request_load();
                self.mark_dirty(DirtyFlags::LAYOUT);
            }
            if self.fit != image.fit {
                self.fit = image.fit;
                self.mark_dirty(DirtyFlags::LAYOUT);
            }
            self.tint = image.tint;
            self.placeholder = image.placeholder;
            self.mark_dirty(DirtyFlags::RENDER);
        }
    }

    fn layout(&mut self, constraints: Constraints) -> Size {
        // Cover/Fill заполняют отведённый бокс (как `object-fit` в CSS): размер
        // задаёт родитель, а не файл. Иначе до загрузки (и при битой ссылке)
        // картинка получала 200×150 и заглушка торчала в углу бокса, а после —
        // натуральную ширину с высотой по пропорции, и «cover» не накрывал бокс.
        let fills = matches!(self.fit, ImageFit::Cover | ImageFit::Fill);
        let width = if let Some(d) = self.width {
            d.resolve(constraints.max_width).min(constraints.max_width)
        } else if fills && constraints.has_bounded_width() {
            constraints.max_width
        } else if let (Some(h), Some(nw), Some(nh)) = (self.height, self.natural_width, self.natural_height) {
            let aspect = nw as f32 / nh as f32;
            (h.resolve(constraints.max_height).min(constraints.max_height) * aspect).min(constraints.max_width)
        } else if let Some(nw) = self.natural_width {
            (nw as f32).min(constraints.max_width)
        } else {
            constraints.max_width.min(200.0)
        };

        let height = if let Some(d) = self.height {
            d.resolve(constraints.max_height).min(constraints.max_height)
        } else if fills && constraints.has_bounded_height() {
            constraints.max_height
        } else if let (Some(nw), Some(nh)) = (self.natural_width, self.natural_height) {
            let aspect = nh as f32 / nw as f32;
            (width * aspect).min(constraints.max_height)
        } else if let Some(nh) = self.natural_height {
            (nh as f32).min(constraints.max_height)
        } else {
            constraints.max_height.min(150.0)
        };

        self.bounds = Rect::new(Point::zero(), Size::new(width, height));
        Size::new(width, height)
    }

    fn build_display_list(&self, list: &mut DisplayList, _clip: Rect) {
        match self.image_state {
            ImageLoadState::Ready => {
                if let Some(handle) = self.image_handle {
                    let fit_rect = self.compute_fit_rect();
                    let tint = self.tint.or(self.mss.color_tint).unwrap_or(Color::WHITE);

                    if self.opacity < 1.0 {
                        list.push_opacity(self.opacity);
                    }

                    let uv_rect = Rect::new(Point::new(0.0, 0.0), Size::new(1.0, 1.0));
                    list.push_image(fit_rect, TextureId(handle.0), uv_rect, tint);

                    if self.opacity < 1.0 {
                        list.pop_opacity();
                    }
                }
            }
            ImageLoadState::Loading => {
                if !self.placeholder {
                    return;
                }
                let bg_color = self.mss.background_color.unwrap_or_else(|| Color::from_hex("#F3F4F6"));
                list.push_rect(self.bounds, bg_color, [4.0; 4]);

                let icon_color = self.mss.color.map(|c| c.with_alpha(0.5)).unwrap_or_else(|| Color::from_hex("#9CA3AF"));
                let icon_size = 20.0f32.min(self.bounds.size.width * 0.5).min(self.bounds.size.height * 0.5);
                let icon_rect = Rect::new(
                    Point::new(
                        self.bounds.x() + (self.bounds.size.width - icon_size) / 2.0,
                        self.bounds.y() + (self.bounds.size.height - icon_size) / 2.0 - 8.0,
                    ),
                    Size::new(icon_size, icon_size),
                );
                list.push_text_centered("🖼", icon_rect, icon_color, icon_size * 0.8);

                if self.bounds.size.height > 40.0 {
                    let text_rect = Rect::new(
                        Point::new(
                            self.bounds.x(),
                            self.bounds.y() + (self.bounds.size.height) / 2.0 + 8.0,
                        ),
                        Size::new(self.bounds.size.width, 16.0),
                    );
                    list.push_text_centered(&crate::i18n::builtin("image.loading", "Loading..."), text_rect, icon_color, 11.0);
                }
            }
            ImageLoadState::Failed => {
                if !self.placeholder {
                    return;
                }
                let bg_color = Color::from_hex("#FEE2E2");
                list.push_rect(self.bounds, bg_color, [4.0; 4]);

                let icon_size = 20.0f32.min(self.bounds.size.width * 0.5).min(self.bounds.size.height * 0.5);
                let icon_rect = Rect::new(
                    Point::new(
                        self.bounds.x() + (self.bounds.size.width - icon_size) / 2.0,
                        self.bounds.y() + (self.bounds.size.height - icon_size) / 2.0,
                    ),
                    Size::new(icon_size, icon_size),
                );
                list.push_text_centered("⚠", icon_rect, Color::from_hex("#EF4444"), icon_size * 0.8);
            }
        }
    }

    fn handle_event(&mut self, _event: &Event, _ctx: &mut crate::widget::context::EventContext) -> EventResult {
        EventResult::Ignored
    }

    fn animate(&mut self, _dt: std::time::Duration) -> bool {
        if self.image_state == ImageLoadState::Loading {
            let mut new_state = None;
            let mut new_dims = None;
            if let Some(ref store) = self.image_store {
                if let Some(handle) = self.image_handle {
                    let store = store.lock().unwrap();
                    if let Some(state) = store.state_of(handle) {
                        if state != self.image_state {
                            new_state = Some(state);
                            if state == ImageLoadState::Ready {
                                new_dims = store.dimensions(handle);
                            }
                        }
                    }
                }
            }
            if let Some(state) = new_state {
                self.image_state = state;
                if let Some((w, h)) = new_dims {
                    self.natural_width = Some(w);
                    self.natural_height = Some(h);
                }
                self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
            }
            return true;
        }
        false
    }

    /// Пока картинка грузится/декодится, элемент обязан числиться в
    /// точечном реестре анимаций: готовность стора он узнаёт опросом в
    /// [`Self::animate`]. Без этого реестр не звал `animate` вовсе, и уже
    /// декодированная картинка навсегда оставалась плейсхолдером 🖼
    /// (логотип в рейле synthos).
    fn wants_animate_tick(&self) -> bool {
        self.image_state == ImageLoadState::Loading
    }

    fn clip_content(&self) -> bool {
        self.fit == ImageFit::Cover
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

    fn mount(&mut self, tree: &mut ElementTree) {
        self.image_store = tree.image_store.clone();
        self.request_load();
    }

    fn set_classes(&mut self, classes: Vec<String>) {
        self.classes = classes;
        self.mark_dirty(DirtyFlags::RENDER);
    }

    fn get_classes(&self) -> &[String] {
        &self.classes
    }

    fn element_type_name(&self) -> &str { "Image" }

    fn reset_mss_styles(&mut self) { self.mss.reset(); }
    fn mss(&self) -> Option<&crate::mss::MssFields> { Some(&self.mss) }
    fn apply_computed_style(&mut self, style: &ComputedStyle) {
        self.mss.apply(style);
        if let Some(d) = self.mss.width { self.width = Some(d); }
        if let Some(d) = self.mss.height { self.height = Some(d); }
        self.mark_dirty(DirtyFlags::RENDER | DirtyFlags::LAYOUT);
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
        self.mss.apply_transitions(base, hover, active, focus, selected);
    }

    fn accessibility_info(&self) -> Option<crate::a11y::AccessibilityInfo> {
        let label = match &self.source {
            ImageSource::Path(p) => p.clone(),
            ImageSource::Bytes { key, .. } => key.clone(),
            ImageSource::RawRgba { key, .. } => key.clone(),
            ImageSource::Url(url) => url.clone(),
        };
        Some(crate::a11y::AccessibilityInfo {
            role: crate::a11y::Role::Image,
            state: crate::a11y::NodeState::default(),
            properties: crate::a11y::NodeProperties {
                label: Some(label),
                ..Default::default()
            },
        })
    }
}

impl StyledElement for ImageElement {
    fn apply_style(&mut self, _style: &ComputedStyle) {
        self.mark_dirty(DirtyFlags::RENDER | DirtyFlags::LAYOUT);
    }

    fn classes(&self) -> &[String] {
        &self.classes
    }

    fn set_classes(&mut self, classes: Vec<String>) {
        self.classes = classes;
        self.mark_dirty(DirtyFlags::RENDER);
    }
}
