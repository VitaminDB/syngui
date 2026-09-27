// Вывод текстуры слоя четырёхугольником в однородных координатах:
// позиция приходит уже в clip space с W ≠ 1, поэтому текстурные координаты
// и непрозрачность интерполируются перспективно-корректно (3D-поворот
// элемента, отражение).

struct VertexInput {
    @location(0) position: vec4<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) alpha: f32,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) alpha: f32,
};

@group(0) @binding(0)
var source_texture: texture_2d<f32>;

@group(0) @binding(1)
var source_sampler: sampler;

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = in.position;
    out.uv = in.uv;
    out.alpha = in.alpha;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let c = textureSample(source_texture, source_sampler, in.uv);
    return vec4<f32>(c.rgb, c.a * in.alpha);
}
