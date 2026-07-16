use crate::core::Color;
use crate::gpu::GpuShared;
use crate::gpu::WindowSurface;
use crate::render::{RenderOp, ShaderType};
use wgpu::util::DeviceExt;

use super::{GpuBatchBuffers, RenderStats, Renderer, Uniforms, MAX_CLIP_SLOTS, UNIFORM_ALIGN};

impl Renderer {
    pub fn render(
        &mut self,
        gpu: &GpuShared,
        surface: &WindowSurface,
        display_list: &crate::render::DisplayList,
        background_color: Color,
    ) -> RenderStats {
        self.ensure_scene_texture(&gpu.device);

        let elapsed = self.start_time.elapsed().as_secs_f32();
        let scale = if self.logical_width > 0 && self.width > 0 {
            self.width as f32 / self.logical_width as f32
        } else {
            1.0
        };
        self.batcher.set_scale_factor(scale);
        let render_ops = {
            let mut atlas = self.font_atlas.lock().unwrap();
            let ops = self.batcher.process(display_list, &mut atlas);
            atlas.upload(&gpu.queue);
            ops
        };
        {
            let mut store = self.image_store.lock().unwrap();
            self.image_gpu_cache.process_uploads(&gpu.device, &gpu.queue, &mut store);
        }
        #[cfg(feature = "map")]
        if let Some(ref tile_atlas) = self.tile_atlas {
            let mut atlas = tile_atlas.lock().unwrap();
            atlas.upload(&gpu.queue);
        }

        let resolution = [self.logical_width as f32, self.logical_height as f32];
        let clip_slot_map = self.write_clip_uniform_slots(gpu, &render_ops, resolution, elapsed, scale);

        self.gpu_buffers.clear();
        for op in &render_ops {
            if let RenderOp::Draw(batch) = op {
                if batch.vertices.is_empty() {
                    continue;
                }
                let vertex_buffer =
                    gpu.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("Vertex Buffer"),
                        contents: bytemuck::cast_slice(&batch.vertices),
                        usage: wgpu::BufferUsages::VERTEX,
                    });
                let index_buffer =
                    gpu.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("Index Buffer"),
                        contents: bytemuck::cast_slice(&batch.indices),
                        usage: wgpu::BufferUsages::INDEX,
                    });
                let uniform_offset = clip_slot_map.get(&batch.clip_rect)
                    .copied()
                    .unwrap_or(0) as u32;
                self.gpu_buffers.push(GpuBatchBuffers {
                    vertex_buffer,
                    index_buffer,
                    index_count: batch.indices.len() as u32,
                    shader_type: batch.shader_type,
                    clip_rect: batch.clip_rect,
                    texture_id: batch.texture,
                    uniform_offset,
                });
            }
        }

        let surface_texture = match surface.surface.get_current_texture() {
            Ok(texture) => texture,
            Err(e) => {
                log::error!("Failed to get surface texture: {:?}", e);
                return RenderStats::default();
            }
        };
        let surface_view = surface_texture
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder =
            gpu.device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("Render Encoder"),
                });

        if self.staging_belt_enabled {
            if let (Some(belt), Some(buf)) = (
                self.staging_belt.as_mut(),
                self.throughput_buffer.as_ref(),
            ) {
                let size = std::num::NonZeroU64::new(4 * 1024 * 1024).unwrap();
                let mut view = belt.write_buffer(&mut encoder, buf, 0, size);
                let stamp: [u8; 4] = (elapsed as u32).to_le_bytes();
                view[0..4].copy_from_slice(&stamp);
            }
        }

        let pool_handles = {
            let (plan, pool_handles) = self.build_render_plan(
                &render_ops, &gpu.device, background_color, elapsed,
            );
            let scene_view = self.scene_view.as_ref().unwrap();
            let scene_texture = self.scene_texture.as_ref().unwrap();
            self.execute_render_plan(
                &mut encoder, gpu, &plan, scene_view, scene_texture, elapsed, scale,
            );
            pool_handles
        };
        for h in pool_handles {
            self.texture_pool.release(h);
        }

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Blit Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &surface_view,
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

            render_pass.set_pipeline(&self.blit_pipeline);
            render_pass.set_bind_group(0, self.scene_bind_group.as_ref().unwrap(), &[]);
            render_pass.set_vertex_buffer(0, self.fullscreen_vertex_buffer.slice(..));
            render_pass.set_index_buffer(
                self.fullscreen_index_buffer.slice(..), wgpu::IndexFormat::Uint32,
            );
            render_pass.draw_indexed(0..6, 0, 0..1);
        }

        if self.staging_belt_enabled {
            if let Some(belt) = self.staging_belt.as_mut() {
                belt.finish();
            }
        }
        gpu.queue.submit(std::iter::once(encoder.finish()));
        if self.staging_belt_enabled {
            if let Some(belt) = self.staging_belt.as_mut() {
                belt.recall();
            }
        }
        surface_texture.present();

        self.texture_pool.end_frame();

        let draw_calls = self.gpu_buffers.len();
        let vertex_count = self.gpu_buffers.iter().map(|b| b.index_count as usize).sum();
        RenderStats { draw_calls, vertex_count }
    }

    pub fn render_to_view(
        &mut self,
        gpu: &GpuShared,
        target_view: &wgpu::TextureView,
        target_size: (u32, u32),
        display_list: &crate::render::DisplayList,
        background_color: Color,
    ) -> RenderStats {
        let (phys_w, phys_h) = target_size;

        let elapsed = self.start_time.elapsed().as_secs_f32();
        let scale = if self.logical_width > 0 && phys_w > 0 {
            phys_w as f32 / self.logical_width as f32
        } else {
            1.0
        };
        self.batcher.set_scale_factor(scale);
        let render_ops = {
            let mut atlas = self.font_atlas.lock().unwrap();
            let ops = self.batcher.process(display_list, &mut atlas);
            atlas.upload(&gpu.queue);
            ops
        };
        {
            let mut store = self.image_store.lock().unwrap();
            self.image_gpu_cache.process_uploads(&gpu.device, &gpu.queue, &mut store);
        }
        #[cfg(feature = "map")]
        if let Some(ref tile_atlas) = self.tile_atlas {
            let mut atlas = tile_atlas.lock().unwrap();
            atlas.upload(&gpu.queue);
        }

        let resolution = [self.logical_width as f32, self.logical_height as f32];
        let clip_slot_map = self.write_clip_uniform_slots(gpu, &render_ops, resolution, elapsed, scale);

        self.gpu_buffers.clear();
        for op in &render_ops {
            if let RenderOp::Draw(batch) = op {
                if batch.vertices.is_empty() {
                    continue;
                }
                let vertex_buffer =
                    gpu.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("Vertex Buffer"),
                        contents: bytemuck::cast_slice(&batch.vertices),
                        usage: wgpu::BufferUsages::VERTEX,
                    });
                let index_buffer =
                    gpu.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("Index Buffer"),
                        contents: bytemuck::cast_slice(&batch.indices),
                        usage: wgpu::BufferUsages::INDEX,
                    });
                let uniform_offset = clip_slot_map.get(&batch.clip_rect)
                    .copied()
                    .unwrap_or(0) as u32;
                self.gpu_buffers.push(GpuBatchBuffers {
                    vertex_buffer,
                    index_buffer,
                    index_count: batch.indices.len() as u32,
                    shader_type: batch.shader_type,
                    clip_rect: batch.clip_rect,
                    texture_id: batch.texture,
                    uniform_offset,
                });
            }
        }

        let mut encoder =
            gpu.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Offscreen Render Encoder"),
            });

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Offscreen UI Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: background_color.r as f64,
                            g: background_color.g as f64,
                            b: background_color.b as f64,
                            a: background_color.a as f64,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });

            let mut current_pipeline = ShaderType::Rect;
            let mut current_texture_id: Option<crate::render::TextureId> = None;
            let mut current_uniform_offset = u32::MAX;
            render_pass.set_pipeline(&self.rect_pipeline);

            let mut buffer_index = 0;
            for op in &render_ops {
                match op {
                    RenderOp::Draw(_) => {
                        if buffer_index < self.gpu_buffers.len() {
                            let batch = &self.gpu_buffers[buffer_index];

                            let need_pipeline_switch = batch.shader_type != current_pipeline;
                            let need_texture_switch = batch.shader_type == ShaderType::Image
                                && batch.texture_id != current_texture_id;
                            let need_offset_switch = batch.uniform_offset != current_uniform_offset;

                            if need_pipeline_switch || need_texture_switch {
                                current_pipeline = batch.shader_type;
                                current_texture_id = batch.texture_id;
                                current_uniform_offset = batch.uniform_offset;
                                match current_pipeline {
                                    ShaderType::Rect => {
                                        render_pass.set_pipeline(&self.rect_pipeline);
                                        render_pass
                                            .set_bind_group(0, &self.uniform_bind_group, &[batch.uniform_offset]);
                                    }
                                    ShaderType::Text => {
                                        render_pass.set_pipeline(&self.text_pipeline);
                                        render_pass
                                            .set_bind_group(0, &self.uniform_bind_group, &[batch.uniform_offset]);
                                        render_pass
                                            .set_bind_group(1, &self.text_bind_group, &[]);
                                    }
                                    ShaderType::Shadow => {
                                        render_pass.set_pipeline(&self.shadow_pipeline);
                                        render_pass
                                            .set_bind_group(0, &self.uniform_bind_group, &[batch.uniform_offset]);
                                    }
                                    ShaderType::InnerShadow => {
                                        render_pass.set_pipeline(&self.inner_shadow_pipeline);
                                        render_pass
                                            .set_bind_group(0, &self.uniform_bind_group, &[batch.uniform_offset]);
                                    }
                                    ShaderType::Image => {
                                        render_pass.set_pipeline(&self.image_pipeline);
                                        render_pass
                                            .set_bind_group(0, &self.uniform_bind_group, &[batch.uniform_offset]);
                                        let mut bound = false;
                                        if let Some(tex_id) = batch.texture_id {
                                            if tex_id.0 == 0 {
                                                #[cfg(feature = "map")]
                                                if let Some(ref bg) =
                                                    self.tile_atlas_bind_group
                                                {
                                                    render_pass
                                                        .set_bind_group(1, bg, &[]);
                                                    bound = true;
                                                }
                                            } else if let Some(bg) =
                                                self.image_gpu_cache.get_bind_group(tex_id.0)
                                            {
                                                render_pass.set_bind_group(1, bg, &[]);
                                                bound = true;
                                            }
                                        }
                                        if !bound {
                                            buffer_index += 1;
                                            continue;
                                        }
                                    }
                                    ShaderType::Line => {
                                        render_pass.set_pipeline(&self.line_pipeline);
                                        render_pass
                                            .set_bind_group(0, &self.uniform_bind_group, &[batch.uniform_offset]);
                                    }
                                    ShaderType::GlowShadow => {
                                        render_pass.set_pipeline(&self.glow_shadow_pipeline);
                                        render_pass
                                            .set_bind_group(0, &self.uniform_bind_group, &[batch.uniform_offset]);
                                    }
                                    ShaderType::Effect => {
                                        render_pass.set_pipeline(&self.rect_pipeline);
                                        render_pass
                                            .set_bind_group(0, &self.uniform_bind_group, &[batch.uniform_offset]);
                                    }
                                }
                            } else if need_offset_switch {
                                current_uniform_offset = batch.uniform_offset;
                                render_pass.set_bind_group(0, &self.uniform_bind_group, &[batch.uniform_offset]);
                            }

                            if batch.clip_rect.enabled {
                                let sx = (batch.clip_rect.x as f32 * scale).floor() as u32;
                                let sy = (batch.clip_rect.y as f32 * scale).floor() as u32;
                                let sr = ((batch.clip_rect.x as f32
                                    + batch.clip_rect.width as f32)
                                    * scale)
                                    .ceil() as u32;
                                let sb = ((batch.clip_rect.y as f32
                                    + batch.clip_rect.height as f32)
                                    * scale)
                                    .ceil() as u32;
                                let sw = sr
                                    .saturating_sub(sx)
                                    .min(phys_w.saturating_sub(sx));
                                let sh = sb
                                    .saturating_sub(sy)
                                    .min(phys_h.saturating_sub(sy));
                                if sx >= phys_w || sy >= phys_h || sw == 0 || sh == 0 {
                                    buffer_index += 1;
                                    continue;
                                }
                                render_pass.set_scissor_rect(sx, sy, sw, sh);
                            } else {
                                render_pass.set_scissor_rect(0, 0, phys_w, phys_h);
                            }

                            render_pass
                                .set_vertex_buffer(0, batch.vertex_buffer.slice(..));
                            render_pass.set_index_buffer(
                                batch.index_buffer.slice(..),
                                wgpu::IndexFormat::Uint32,
                            );
                            render_pass.draw_indexed(0..batch.index_count, 0, 0..1);

                            buffer_index += 1;
                        }
                    }
                    RenderOp::BeginEffect { .. } | RenderOp::EndEffect => {}
                }
            }
        }

        if self.staging_belt_enabled {
            if let Some(belt) = self.staging_belt.as_mut() {
                belt.finish();
            }
        }
        gpu.queue.submit(std::iter::once(encoder.finish()));
        if self.staging_belt_enabled {
            if let Some(belt) = self.staging_belt.as_mut() {
                belt.recall();
            }
        }

        let draw_calls = self.gpu_buffers.len();
        let vertex_count = self.gpu_buffers.iter().map(|b| b.index_count as usize).sum();
        RenderStats { draw_calls, vertex_count }
    }

    fn write_clip_uniform_slots(
        &self,
        gpu: &GpuShared,
        render_ops: &[RenderOp],
        resolution: [f32; 2],
        elapsed: f32,
        scale_factor: f32,
    ) -> std::collections::HashMap<crate::render::ClipRect, usize> {
        use std::collections::HashMap;

        let mut clip_map: HashMap<crate::render::ClipRect, usize> = HashMap::new();

        let default_clip = crate::render::ClipRect::full_screen();
        clip_map.insert(default_clip, 0);
        let mut slot_index = 1usize;

        for op in render_ops {
            if let RenderOp::Draw(batch) = op {
                if !batch.vertices.is_empty() && !clip_map.contains_key(&batch.clip_rect) {
                    if slot_index < MAX_CLIP_SLOTS {
                        clip_map.insert(batch.clip_rect, slot_index * UNIFORM_ALIGN);
                        slot_index += 1;
                    }
                }
            }
        }

        let mut buffer_data = vec![0u8; slot_index * UNIFORM_ALIGN];
        for (&clip, &byte_offset) in &clip_map {
            let radii = clip.corner_radius_f32();
            let (clip_rect, clip_corner_radius) = if clip.has_corner_radius() {
                (
                    [clip.x as f32, clip.y as f32, clip.width as f32, clip.height as f32],
                    radii,
                )
            } else {
                ([0.0; 4], [0.0; 4])
            };

            let uniforms = Uniforms {
                resolution,
                time: elapsed,
                scale_factor,
                clip_rect,
                clip_corner_radius,
            };

            let src = bytemuck::bytes_of(&uniforms);
            buffer_data[byte_offset..byte_offset + src.len()].copy_from_slice(src);
        }

        gpu.queue.write_buffer(&self.uniform_buffer, 0, &buffer_data);

        clip_map
    }
}
