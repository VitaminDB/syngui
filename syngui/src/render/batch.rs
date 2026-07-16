use crate::render::{ClipRect, Vertex};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum ShaderType {
    #[default]
    Rect,
    Text,
    Shadow,
    InnerShadow,
    Image,
    Effect,
    Line,
    GlowShadow,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct TextureId(pub u32);

impl TextureId {
    pub const SCREEN: Self = Self(0);
}

#[derive(Debug, Default)]
pub struct Batch {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
    pub shader_type: ShaderType,
    pub texture: Option<TextureId>,
    pub clip_rect: ClipRect,
    pub vertex_offset: u32,
    pub index_offset: u32,
}

impl Batch {
    pub fn new(shader_type: ShaderType, texture: Option<TextureId>, clip_rect: ClipRect) -> Self {
        Self {
            vertices: Vec::with_capacity(256),
            indices: Vec::with_capacity(384),
            shader_type,
            texture,
            clip_rect,
            vertex_offset: 0,
            index_offset: 0,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.vertices.is_empty()
    }

    pub fn len(&self) -> usize {
        self.vertices.len()
    }

    pub fn can_merge(&self, shader: ShaderType, texture: Option<TextureId>, clip: ClipRect) -> bool {
        self.shader_type == shader &&
        self.texture == texture &&
        self.clip_rect == clip
    }
}

#[derive(Debug)]
pub enum RenderOp {
    Draw(Batch),
    BeginEffect { effect: crate::render::display_list::Effect, bounds: crate::core::Rect },
    EndEffect,
}
