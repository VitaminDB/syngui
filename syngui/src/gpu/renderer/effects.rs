use crate::gpu::GpuShared;
use crate::render::RenderOp;
use wgpu::util::DeviceExt;

use super::{BlurUniforms, EffectRenderStep, EffectTarget, PostProcessUniforms, Renderer};

/// На сколько физических пикселей ядро размытия читает в сторону: в
/// `blur.wgsl` не больше 32 отсчётов на сторону, плюс запас на округление.
const BLUR_KERNEL_REACH_PX: u32 = 34;

/// Эффект-цепочка цветовых операций в `postprocess.wgsl` и сколько операций
/// помещается в один проход (коды и величины едут в двух vec4).
const COLOR_CHAIN_EFFECT: f32 = 24.0;
const COLOR_CHAIN_MAX_OPS: usize = 4;

impl Renderer {
    pub(super) fn build_render_plan(
        &mut self,
        render_ops: &[RenderOp],
        device: &wgpu::Device,
        background_color: crate::core::Color,
        elapsed: f32,
    ) -> (
        Vec<EffectRenderStep>,
        Vec<crate::gpu::texture_pool::PoolHandle>,
    ) {
        use crate::render::display_list::Effect;

        let mut plan: Vec<EffectRenderStep> = Vec::with_capacity(render_ops.len() + 8);
        let mut handles: Vec<crate::gpu::texture_pool::PoolHandle> = Vec::new();

        let mut effect_stack: Vec<(
            EffectTarget,
            Effect,
            crate::gpu::texture_pool::PoolHandle,
            [f32; 4],
        )> = Vec::new();
        let mut current_target = EffectTarget::Scene;
        let mut buf_idx = 0usize;
        let mut seg_start = 0usize;
        let mut scene_cleared = false;

        let bg = [
            background_color.r as f64,
            background_color.g as f64,
            background_color.b as f64,
            background_color.a as f64,
        ];

        for op in render_ops {
            match op {
                RenderOp::Draw(batch) => {
                    if !batch.vertices.is_empty() {
                        buf_idx += 1;
                    }
                }
                RenderOp::BeginEffect { effect, bounds } => {
                    let bounds_px = [
                        bounds.origin.x,
                        bounds.origin.y,
                        bounds.width(),
                        bounds.height(),
                    ];
                    if buf_idx > seg_start {
                        let (clear, cc) = match current_target {
                            EffectTarget::Scene => {
                                let first = !scene_cleared;
                                scene_cleared = true;
                                (first, bg)
                            }
                            EffectTarget::Pool(_) => (true, [0.0; 4]),
                        };
                        plan.push(EffectRenderStep::DrawBatches {
                            target: current_target,
                            buf_range: seg_start..buf_idx,
                            clear,
                            clear_color: cc,
                        });
                    } else if matches!(current_target, EffectTarget::Scene) && !scene_cleared {
                        plan.push(EffectRenderStep::DrawBatches {
                            target: EffectTarget::Scene,
                            buf_range: 0..0,
                            clear: true,
                            clear_color: bg,
                        });
                        scene_cleared = true;
                    }
                    seg_start = buf_idx;

                    let pool = self.texture_pool.acquire(device);
                    handles.push(pool);
                    effect_stack.push((current_target, effect.clone(), pool, bounds_px));
                    current_target = EffectTarget::Pool(pool);
                }
                RenderOp::EndEffect => {
                    if buf_idx > seg_start {
                        plan.push(EffectRenderStep::DrawBatches {
                            target: current_target,
                            buf_range: seg_start..buf_idx,
                            clear: true,
                            clear_color: [0.0; 4],
                        });
                    } else {
                        plan.push(EffectRenderStep::DrawBatches {
                            target: current_target,
                            buf_range: 0..0,
                            clear: true,
                            clear_color: [0.0; 4],
                        });
                    }
                    seg_start = buf_idx;

                    if let Some((parent_target, effect, pool, bounds_px)) = effect_stack.pop() {
                        current_target = parent_target;
                        Self::apply_effect_steps(
                            &mut self.texture_pool,
                            device,
                            &effect,
                            pool,
                            parent_target,
                            &mut plan,
                            &mut handles,
                            elapsed,
                            bounds_px,
                        );
                    }
                }
            }
        }

        if buf_idx > seg_start {
            let (clear, cc) = match current_target {
                EffectTarget::Scene => {
                    let first = !scene_cleared;
                    scene_cleared = true;
                    (first, bg)
                }
                EffectTarget::Pool(_) => (true, [0.0; 4]),
            };
            plan.push(EffectRenderStep::DrawBatches {
                target: current_target,
                buf_range: seg_start..buf_idx,
                clear,
                clear_color: cc,
            });
        } else if !scene_cleared {
            plan.push(EffectRenderStep::DrawBatches {
                target: EffectTarget::Scene,
                buf_range: 0..0,
                clear: true,
                clear_color: bg,
            });
        }

        let _ = scene_cleared;
        (plan, handles)
    }

    fn apply_effect_steps(
        texture_pool: &mut crate::gpu::texture_pool::TexturePool,
        device: &wgpu::Device,
        effect: &crate::render::display_list::Effect,
        source: crate::gpu::texture_pool::PoolHandle,
        dest: EffectTarget,
        plan: &mut Vec<EffectRenderStep>,
        handles: &mut Vec<crate::gpu::texture_pool::PoolHandle>,
        elapsed: f32,
        bounds_px: [f32; 4],
    ) {
        use crate::render::display_list::Effect;

        match effect {
            Effect::None
            | Effect::Opacity { .. }
            | Effect::BlendMode { .. }
            | Effect::Shadow { .. } => {
                plan.push(EffectRenderStep::Composite { source, dest });
            }

            Effect::Glow { radius, .. } => {
                let temp = texture_pool.acquire(device);
                handles.push(temp);
                plan.push(EffectRenderStep::BlurPass {
                    source,
                    dest: temp,
                    radius: *radius,
                    direction: [1.0, 0.0],
                    region: None,
                });
                plan.push(EffectRenderStep::BlurPass {
                    source: temp,
                    dest: source,
                    radius: *radius,
                    direction: [0.0, 1.0],
                    region: None,
                });
                plan.push(EffectRenderStep::CompositeAdditive { source, dest });
            }

            Effect::DirectionalBlur { angle, radius } => {
                let temp = texture_pool.acquire(device);
                handles.push(temp);
                let (sin_a, cos_a) = angle.sin_cos();
                plan.push(EffectRenderStep::BlurPass {
                    source,
                    dest: temp,
                    radius: *radius,
                    direction: [cos_a, sin_a],
                    region: None,
                });
                plan.push(EffectRenderStep::BlurPass {
                    source: temp,
                    dest: source,
                    radius: *radius,
                    direction: [-sin_a, cos_a],
                    region: None,
                });
                plan.push(EffectRenderStep::Composite { source, dest });
            }

            Effect::Blur { radius } => {
                let temp = texture_pool.acquire(device);
                handles.push(temp);
                plan.push(EffectRenderStep::BlurPass {
                    source,
                    dest: temp,
                    radius: *radius,
                    direction: [1.0, 0.0],
                    region: None,
                });
                plan.push(EffectRenderStep::BlurPass {
                    source: temp,
                    dest: source,
                    radius: *radius,
                    direction: [0.0, 1.0],
                    region: None,
                });
                plan.push(EffectRenderStep::Composite { source, dest });
            }

            Effect::BackdropBlur { radius } => {
                let snapshot = texture_pool.acquire(device);
                handles.push(snapshot);
                plan.push(EffectRenderStep::CopySceneToPool { dest: snapshot });
                let temp = texture_pool.acquire(device);
                handles.push(temp);
                // Размытый фон виден только внутри границ элемента
                // (`CompositeBounded`), поэтому и размывать весь кадр
                // незачем: второй проход идёт по границам, первый — с
                // запасом на вылет ядра, откуда второй читает соседей.
                plan.push(EffectRenderStep::BlurPass {
                    source: snapshot,
                    dest: temp,
                    radius: *radius,
                    direction: [1.0, 0.0],
                    region: Some((bounds_px, BLUR_KERNEL_REACH_PX)),
                });
                plan.push(EffectRenderStep::BlurPass {
                    source: temp,
                    dest: snapshot,
                    radius: *radius,
                    direction: [0.0, 1.0],
                    region: Some((bounds_px, 0)),
                });
                plan.push(EffectRenderStep::CompositeBounded {
                    source: snapshot,
                    dest,
                    bounds: bounds_px,
                });
                plan.push(EffectRenderStep::Composite { source, dest });
            }

            Effect::Chain(effects) => {
                let active: Vec<&Effect> = effects.iter().filter(|e| !e.is_identity()).collect();
                if active.is_empty() {
                    plan.push(EffectRenderStep::Composite { source, dest });
                    return;
                }
                // Подряд идущие цветовые фильтры (invert, hue-rotate,
                // brightness, contrast…) сливаются в один проход: каждый
                // отдельный — это полноэкранная отрисовка, а тёмная карта
                // навешивает их четыре на каждый кадр.
                let mut current = source;
                let mut pending: Vec<(f32, f32)> = Vec::new();
                let mut flush =
                    |pending: &mut Vec<(f32, f32)>,
                     current: &mut crate::gpu::texture_pool::PoolHandle,
                     texture_pool: &mut crate::gpu::texture_pool::TexturePool,
                     plan: &mut Vec<EffectRenderStep>,
                     handles: &mut Vec<crate::gpu::texture_pool::PoolHandle>| {
                        for chunk in pending.chunks(COLOR_CHAIN_MAX_OPS) {
                            let mut codes = [0.0; 4];
                            let mut amounts = [0.0; 4];
                            for (i, (code, amount)) in chunk.iter().enumerate() {
                                codes[i] = *code;
                                amounts[i] = *amount;
                            }
                            let output = texture_pool.acquire(device);
                            handles.push(output);
                            plan.push(EffectRenderStep::PostProcess {
                                source: *current,
                                dest: output,
                                effect_type: COLOR_CHAIN_EFFECT,
                                intensity: chunk.len() as f32,
                                params: codes,
                                params2: amounts,
                                time: elapsed,
                                bounds: bounds_px,
                            });
                            *current = output;
                        }
                        pending.clear();
                    };
                for eff in &active {
                    if let Some(ops) = eff.color_ops() {
                        pending.extend(ops);
                        continue;
                    }
                    flush(&mut pending, &mut current, texture_pool, plan, handles);
                    current = Self::apply_single_effect(
                        texture_pool,
                        device,
                        eff,
                        current,
                        plan,
                        handles,
                        elapsed,
                        bounds_px,
                    );
                }
                flush(&mut pending, &mut current, texture_pool, plan, handles);
                plan.push(EffectRenderStep::Composite {
                    source: current,
                    dest,
                });
            }

            other => {
                if let Some((et, intensity, params, params2)) = other.postprocess_type() {
                    let temp = texture_pool.acquire(device);
                    handles.push(temp);
                    plan.push(EffectRenderStep::PostProcess {
                        source,
                        dest: temp,
                        effect_type: et,
                        intensity,
                        params,
                        params2,
                        time: elapsed,
                        bounds: bounds_px,
                    });
                    plan.push(EffectRenderStep::Composite { source: temp, dest });
                } else {
                    plan.push(EffectRenderStep::Composite { source, dest });
                }
            }
        }
    }

    fn apply_single_effect(
        texture_pool: &mut crate::gpu::texture_pool::TexturePool,
        device: &wgpu::Device,
        effect: &crate::render::display_list::Effect,
        input: crate::gpu::texture_pool::PoolHandle,
        plan: &mut Vec<EffectRenderStep>,
        handles: &mut Vec<crate::gpu::texture_pool::PoolHandle>,
        elapsed: f32,
        bounds_px: [f32; 4],
    ) -> crate::gpu::texture_pool::PoolHandle {
        use crate::render::display_list::Effect;

        match effect {
            Effect::Blur { radius }
            | Effect::BackdropBlur { radius }
            | Effect::Glow { radius, .. } => {
                let temp = texture_pool.acquire(device);
                handles.push(temp);
                plan.push(EffectRenderStep::BlurPass {
                    source: input,
                    dest: temp,
                    radius: *radius,
                    direction: [1.0, 0.0],
                    region: None,
                });
                plan.push(EffectRenderStep::BlurPass {
                    source: temp,
                    dest: input,
                    radius: *radius,
                    direction: [0.0, 1.0],
                    region: None,
                });
                input
            }
            Effect::DirectionalBlur { angle, radius } => {
                let temp = texture_pool.acquire(device);
                handles.push(temp);
                let (sin_a, cos_a) = angle.sin_cos();
                plan.push(EffectRenderStep::BlurPass {
                    source: input,
                    dest: temp,
                    radius: *radius,
                    direction: [cos_a, sin_a],
                    region: None,
                });
                plan.push(EffectRenderStep::BlurPass {
                    source: temp,
                    dest: input,
                    radius: *radius,
                    direction: [-sin_a, cos_a],
                    region: None,
                });
                input
            }
            other => {
                if let Some((et, intensity, params, params2)) = other.postprocess_type() {
                    let output = texture_pool.acquire(device);
                    handles.push(output);
                    plan.push(EffectRenderStep::PostProcess {
                        source: input,
                        dest: output,
                        effect_type: et,
                        intensity,
                        params,
                        params2,
                        time: elapsed,
                        bounds: bounds_px,
                    });
                    output
                } else {
                    input
                }
            }
        }
    }

    pub(super) fn execute_render_plan(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        gpu: &GpuShared,
        plan: &[EffectRenderStep],
        scene_view: &wgpu::TextureView,
        scene_texture: &wgpu::Texture,
        _elapsed: f32,
        scale: f32,
    ) {
        for step in plan {
            match step {
                EffectRenderStep::DrawBatches {
                    target,
                    buf_range,
                    clear,
                    clear_color,
                } => {
                    let view = match target {
                        EffectTarget::Scene => scene_view,
                        EffectTarget::Pool(h) => self.texture_pool.view(*h),
                    };
                    let load = if *clear {
                        wgpu::LoadOp::Clear(wgpu::Color {
                            r: clear_color[0],
                            g: clear_color[1],
                            b: clear_color[2],
                            a: clear_color[3],
                        })
                    } else {
                        wgpu::LoadOp::Load
                    };

                    if buf_range.is_empty() {
                        if *clear {
                            let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                                label: Some("Clear Pass"),
                                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                                    view,
                                    resolve_target: None,
                                    ops: wgpu::Operations {
                                        load,
                                        store: wgpu::StoreOp::Store,
                                    },
                                    depth_slice: None,
                                })],
                                depth_stencil_attachment: None,
                                timestamp_writes: None,
                                occlusion_query_set: None,
                                multiview_mask: None,
                            });
                        }
                        continue;
                    }

                    let mut rp = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("Draw Pass"),
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view,
                            resolve_target: None,
                            ops: wgpu::Operations {
                                load,
                                store: wgpu::StoreOp::Store,
                            },
                            depth_slice: None,
                        })],
                        depth_stencil_attachment: None,
                        timestamp_writes: None,
                        occlusion_query_set: None,
                        multiview_mask: None,
                    });
                    self.execute_draw_batches(&mut rp, buf_range.clone(), scale);
                }

                EffectRenderStep::BlurPass {
                    source,
                    dest,
                    radius,
                    direction,
                    region,
                } => {
                    let scissor = match region {
                        None => Some((0, 0, self.width, self.height)),
                        Some((rect, grow)) => {
                            crate::render::scissor_px(*rect, scale, self.width, self.height).map(
                                |(x, y, w, h)| {
                                    let x0 = x.saturating_sub(*grow);
                                    let y0 = y.saturating_sub(*grow);
                                    let x1 = (x + w + grow).min(self.width);
                                    let y1 = (y + h + grow).min(self.height);
                                    (x0, y0, x1 - x0, y1 - y0)
                                },
                            )
                        }
                    };
                    let uniforms = BlurUniforms {
                        resolution: [self.width as f32, self.height as f32],
                        direction: *direction,
                        radius: *radius,
                        _padding: 0.0,
                        _padding2: [0.0; 2],
                    };
                    let staging =
                        gpu.device
                            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                                label: Some("Blur Uniform Staging"),
                                contents: bytemuck::cast_slice(&[uniforms]),
                                usage: wgpu::BufferUsages::COPY_SRC,
                            });
                    encoder.copy_buffer_to_buffer(
                        &staging,
                        0,
                        &self.blur_uniform_buffer,
                        0,
                        std::mem::size_of::<BlurUniforms>() as u64,
                    );

                    let mut rp = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("Blur Pass"),
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: self.texture_pool.view(*dest),
                            resolve_target: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                                store: wgpu::StoreOp::Store,
                            },
                            depth_slice: None,
                        })],
                        depth_stencil_attachment: None,
                        timestamp_writes: None,
                        occlusion_query_set: None,
                        multiview_mask: None,
                    });
                    let Some((sx, sy, sw, sh)) = scissor else {
                        continue;
                    };
                    rp.set_scissor_rect(sx, sy, sw, sh);
                    rp.set_pipeline(&self.blur_pipeline);
                    rp.set_bind_group(0, &self.blur_uniform_bind_group, &[]);
                    rp.set_bind_group(1, self.texture_pool.bind_group(*source), &[]);
                    rp.set_vertex_buffer(0, self.fullscreen_vertex_buffer.slice(..));
                    rp.set_index_buffer(
                        self.fullscreen_index_buffer.slice(..),
                        wgpu::IndexFormat::Uint32,
                    );
                    rp.draw_indexed(0..6, 0, 0..1);
                }

                EffectRenderStep::PostProcess {
                    source,
                    dest,
                    effect_type,
                    intensity,
                    params,
                    params2,
                    time,
                    bounds,
                } => {
                    let log_w = self.logical_width as f32;
                    let log_h = self.logical_height as f32;
                    let phys_w = self.width as f32;
                    let phys_h = self.height as f32;
                    let bounds_uv = [
                        bounds[0] / log_w,
                        bounds[1] / log_h,
                        bounds[2] / log_w,
                        bounds[3] / log_h,
                    ];
                    let uniforms = PostProcessUniforms {
                        resolution: [phys_w, phys_h],
                        effect_type: *effect_type,
                        intensity: *intensity,
                        params: *params,
                        time: *time,
                        _padding: [0.0; 3],
                        params2: *params2,
                        bounds: bounds_uv,
                    };
                    let staging =
                        gpu.device
                            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                                label: Some("PostProcess Uniform Staging"),
                                contents: bytemuck::cast_slice(&[uniforms]),
                                usage: wgpu::BufferUsages::COPY_SRC,
                            });
                    encoder.copy_buffer_to_buffer(
                        &staging,
                        0,
                        &self.postprocess_uniform_buffer,
                        0,
                        std::mem::size_of::<PostProcessUniforms>() as u64,
                    );

                    let mut rp = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("PostProcess Pass"),
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: self.texture_pool.view(*dest),
                            resolve_target: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                                store: wgpu::StoreOp::Store,
                            },
                            depth_slice: None,
                        })],
                        depth_stencil_attachment: None,
                        timestamp_writes: None,
                        occlusion_query_set: None,
                        multiview_mask: None,
                    });
                    rp.set_pipeline(&self.postprocess_pipeline);
                    rp.set_bind_group(0, &self.postprocess_uniform_bind_group, &[]);
                    rp.set_bind_group(1, self.texture_pool.bind_group(*source), &[]);
                    rp.set_vertex_buffer(0, self.fullscreen_vertex_buffer.slice(..));
                    rp.set_index_buffer(
                        self.fullscreen_index_buffer.slice(..),
                        wgpu::IndexFormat::Uint32,
                    );
                    rp.draw_indexed(0..6, 0, 0..1);
                }

                EffectRenderStep::Composite { source, dest } => {
                    let view = match dest {
                        EffectTarget::Scene => scene_view,
                        EffectTarget::Pool(h) => self.texture_pool.view(*h),
                    };
                    let mut rp = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("Composite Pass"),
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view,
                            resolve_target: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Load,
                                store: wgpu::StoreOp::Store,
                            },
                            depth_slice: None,
                        })],
                        depth_stencil_attachment: None,
                        timestamp_writes: None,
                        occlusion_query_set: None,
                        multiview_mask: None,
                    });
                    rp.set_pipeline(&self.blit_pipeline);
                    rp.set_bind_group(0, self.texture_pool.bind_group(*source), &[]);
                    rp.set_vertex_buffer(0, self.fullscreen_vertex_buffer.slice(..));
                    rp.set_index_buffer(
                        self.fullscreen_index_buffer.slice(..),
                        wgpu::IndexFormat::Uint32,
                    );
                    rp.draw_indexed(0..6, 0, 0..1);
                }

                EffectRenderStep::CompositeAdditive { source, dest } => {
                    let view = match dest {
                        EffectTarget::Scene => scene_view,
                        EffectTarget::Pool(h) => self.texture_pool.view(*h),
                    };
                    let mut rp = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("Composite Additive Pass"),
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view,
                            resolve_target: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Load,
                                store: wgpu::StoreOp::Store,
                            },
                            depth_slice: None,
                        })],
                        depth_stencil_attachment: None,
                        timestamp_writes: None,
                        occlusion_query_set: None,
                        multiview_mask: None,
                    });
                    rp.set_pipeline(&self.glow_blit_pipeline);
                    rp.set_bind_group(0, self.texture_pool.bind_group(*source), &[]);
                    rp.set_vertex_buffer(0, self.fullscreen_vertex_buffer.slice(..));
                    rp.set_index_buffer(
                        self.fullscreen_index_buffer.slice(..),
                        wgpu::IndexFormat::Uint32,
                    );
                    rp.draw_indexed(0..6, 0, 0..1);
                }

                EffectRenderStep::CompositeBounded {
                    source,
                    dest,
                    bounds,
                } => {
                    let view = match dest {
                        EffectTarget::Scene => scene_view,
                        EffectTarget::Pool(h) => self.texture_pool.view(*h),
                    };
                    let mut rp = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("Composite Bounded Pass"),
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view,
                            resolve_target: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Load,
                                store: wgpu::StoreOp::Store,
                            },
                            depth_slice: None,
                        })],
                        depth_stencil_attachment: None,
                        timestamp_writes: None,
                        occlusion_query_set: None,
                        multiview_mask: None,
                    });
                    // Границы области эффекта округляются так же, как клип, —
                    // иначе размытый слой лёг бы на пиксель за краем.
                    if let Some((sx, sy, sw, sh)) =
                        crate::render::scissor_px(*bounds, scale, self.width, self.height)
                    {
                        rp.set_scissor_rect(sx, sy, sw, sh);
                        rp.set_pipeline(&self.blit_pipeline);
                        rp.set_bind_group(0, self.texture_pool.bind_group(*source), &[]);
                        rp.set_vertex_buffer(0, self.fullscreen_vertex_buffer.slice(..));
                        rp.set_index_buffer(
                            self.fullscreen_index_buffer.slice(..),
                            wgpu::IndexFormat::Uint32,
                        );
                        rp.draw_indexed(0..6, 0, 0..1);
                    }
                }

                EffectRenderStep::CopySceneToPool { dest } => {
                    encoder.copy_texture_to_texture(
                        wgpu::TexelCopyTextureInfo {
                            texture: scene_texture,
                            mip_level: 0,
                            origin: wgpu::Origin3d::ZERO,
                            aspect: wgpu::TextureAspect::All,
                        },
                        wgpu::TexelCopyTextureInfo {
                            texture: self.texture_pool.texture(*dest),
                            mip_level: 0,
                            origin: wgpu::Origin3d::ZERO,
                            aspect: wgpu::TextureAspect::All,
                        },
                        wgpu::Extent3d {
                            width: self.width.max(1),
                            height: self.height.max(1),
                            depth_or_array_layers: 1,
                        },
                    );
                }
            }
        }
    }

    pub(super) fn execute_draw_batches(
        &self,
        render_pass: &mut wgpu::RenderPass,
        buf_range: std::ops::Range<usize>,
        scale: f32,
    ) {
        let mut current_pipeline = crate::render::ShaderType::Rect;
        let mut current_texture_id: Option<crate::render::TextureId> = None;
        let mut current_uniform_offset = u32::MAX;
        let mut current_scissor: (u32, u32, u32, u32) = (0, 0, self.width, self.height);
        render_pass.set_pipeline(&self.rect_pipeline);
        let geom = &self.frame_geom[self.frame_geom_idx];
        let (Some(vb), Some(ib)) = (geom.vertex.as_ref(), geom.index.as_ref()) else {
            return;
        };
        render_pass.set_vertex_buffer(0, vb.slice(..));
        render_pass.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint32);

        for batch in &self.gpu_buffers[buf_range] {
            let need_pipeline_switch = batch.shader_type != current_pipeline;
            let need_texture_switch = batch.shader_type == crate::render::ShaderType::Image
                && batch.texture_id != current_texture_id;
            let need_offset_switch = batch.uniform_offset != current_uniform_offset;

            if need_pipeline_switch || need_texture_switch {
                current_pipeline = batch.shader_type;
                current_texture_id = batch.texture_id;
                current_uniform_offset = batch.uniform_offset;
                match current_pipeline {
                    crate::render::ShaderType::Rect => {
                        render_pass.set_pipeline(&self.rect_pipeline);
                        render_pass.set_bind_group(
                            0,
                            &self.uniform_bind_group,
                            &[batch.uniform_offset],
                        );
                    }
                    crate::render::ShaderType::Text => {
                        render_pass.set_pipeline(&self.text_pipeline);
                        render_pass.set_bind_group(
                            0,
                            &self.uniform_bind_group,
                            &[batch.uniform_offset],
                        );
                        render_pass.set_bind_group(1, &self.text_bind_group, &[]);
                    }
                    crate::render::ShaderType::Shadow => {
                        render_pass.set_pipeline(&self.shadow_pipeline);
                        render_pass.set_bind_group(
                            0,
                            &self.uniform_bind_group,
                            &[batch.uniform_offset],
                        );
                    }
                    crate::render::ShaderType::InnerShadow => {
                        render_pass.set_pipeline(&self.inner_shadow_pipeline);
                        render_pass.set_bind_group(
                            0,
                            &self.uniform_bind_group,
                            &[batch.uniform_offset],
                        );
                    }
                    crate::render::ShaderType::Image => {
                        render_pass.set_pipeline(&self.image_pipeline);
                        render_pass.set_bind_group(
                            0,
                            &self.uniform_bind_group,
                            &[batch.uniform_offset],
                        );
                        let mut bound = false;
                        if let Some(tex_id) = batch.texture_id {
                            if tex_id.0 == 0 {
                                #[cfg(feature = "map")]
                                if let Some(ref bg) = self.tile_atlas_bind_group {
                                    render_pass.set_bind_group(1, bg, &[]);
                                    bound = true;
                                }
                            } else if let Some(bg) = self.image_gpu_cache.get_bind_group(tex_id.0) {
                                render_pass.set_bind_group(1, bg, &[]);
                                bound = true;
                            }
                        }
                        if !bound {
                            continue;
                        }
                    }
                    crate::render::ShaderType::Line => {
                        render_pass.set_pipeline(&self.line_pipeline);
                        render_pass.set_bind_group(
                            0,
                            &self.uniform_bind_group,
                            &[batch.uniform_offset],
                        );
                    }
                    crate::render::ShaderType::GlowShadow => {
                        render_pass.set_pipeline(&self.glow_shadow_pipeline);
                        render_pass.set_bind_group(
                            0,
                            &self.uniform_bind_group,
                            &[batch.uniform_offset],
                        );
                    }
                    crate::render::ShaderType::Effect => {
                        render_pass.set_pipeline(&self.rect_pipeline);
                        render_pass.set_bind_group(
                            0,
                            &self.uniform_bind_group,
                            &[batch.uniform_offset],
                        );
                    }
                }
            } else if need_offset_switch {
                current_uniform_offset = batch.uniform_offset;
                render_pass.set_bind_group(0, &self.uniform_bind_group, &[batch.uniform_offset]);
            }

            if batch.clip_rect.enabled {
                let Some(scissor) = batch.clip_rect.scissor(scale, self.width, self.height) else {
                    continue;
                };
                if current_scissor != scissor {
                    current_scissor = scissor;
                    render_pass.set_scissor_rect(scissor.0, scissor.1, scissor.2, scissor.3);
                }
            } else if current_scissor != (0, 0, self.width, self.height) {
                current_scissor = (0, 0, self.width, self.height);
                render_pass.set_scissor_rect(0, 0, self.width, self.height);
            }

            render_pass.draw_indexed(
                batch.index_start..batch.index_start + batch.index_count,
                0,
                0..1,
            );
        }
    }
}
