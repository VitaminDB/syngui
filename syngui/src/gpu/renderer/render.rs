use crate::core::Color;
use crate::gpu::GpuShared;
use crate::gpu::WindowSurface;
use crate::render::{RenderOp, ShaderType};

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
        let t = web_time::Instant::now();
        let render_ops = {
            let mut atlas = self.font_atlas.lock().unwrap();
            let ops = self.batcher.process(display_list, &mut atlas);
            crate::perf::add_time(crate::perf::TimeKind::RenderBatch, t.elapsed());
            atlas.upload(&gpu.queue);
            ops
        };
        let t = web_time::Instant::now();
        {
            let mut store = self.image_store.lock().unwrap();
            self.image_gpu_cache
                .process_uploads(&gpu.device, &gpu.queue, &mut store);
        }
        #[cfg(feature = "map")]
        self.sync_tile_atlas(gpu);

        let resolution = [self.logical_width as f32, self.logical_height as f32];
        let clip_slot_map =
            self.write_clip_uniform_slots(gpu, &render_ops, resolution, elapsed, scale);

        self.collect_frame_geometry(gpu, &render_ops, &clip_slot_map);
        crate::perf::add_time(crate::perf::TimeKind::RenderUpload, t.elapsed());

        let t = web_time::Instant::now();
        let surface_texture = match surface.surface.get_current_texture() {
            Ok(texture) => texture,
            Err(e) => {
                log::error!("Failed to get surface texture: {:?}", e);
                return RenderStats::default();
            }
        };
        crate::perf::add_time(crate::perf::TimeKind::RenderAcquire, t.elapsed());
        let t = web_time::Instant::now();
        let surface_view = surface_texture
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Render Encoder"),
            });

        if self.staging_belt_enabled {
            if let (Some(belt), Some(buf)) =
                (self.staging_belt.as_mut(), self.throughput_buffer.as_ref())
            {
                let size = std::num::NonZeroU64::new(4 * 1024 * 1024).unwrap();
                let mut view = belt.write_buffer(&mut encoder, buf, 0, size);
                let stamp: [u8; 4] = (elapsed as u32).to_le_bytes();
                view[0..4].copy_from_slice(&stamp);
            }
        }

        let (pool_handles, direct) = {
            let (plan, pool_handles) =
                self.build_render_plan(&render_ops, &gpu.device, background_color, elapsed);
            // Без эффектов (обычный кадр) рисуем сразу в surface: scene-текстура
            // и полноэкранный blit — лишний проход 1920×1080 и resolve на
            // тайловом GPU.
            let direct = std::env::var_os("SYNGUI_NO_DIRECT").is_none() && plan.iter().all(|s| {
                matches!(
                    s,
                    super::EffectRenderStep::DrawBatches {
                        target: super::EffectTarget::Scene,
                        ..
                    }
                )
            });
            let scene_view = self.scene_view.as_ref().unwrap();
            let scene_texture = self.scene_texture.as_ref().unwrap();
            self.execute_render_plan(
                &mut encoder,
                gpu,
                &plan,
                if direct { &surface_view } else { scene_view },
                scene_texture,
                elapsed,
                scale,
            );
            (pool_handles, direct)
        };
        for h in pool_handles {
            self.texture_pool.release(h);
        }

        if !direct {
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
                self.fullscreen_index_buffer.slice(..),
                wgpu::IndexFormat::Uint32,
            );
            render_pass.draw_indexed(0..6, 0, 0..1);
        }

        if self.staging_belt_enabled {
            if let Some(belt) = self.staging_belt.as_mut() {
                belt.finish();
            }
        }
        crate::perf::add_time(crate::perf::TimeKind::RenderEncode, t.elapsed());
        let t = web_time::Instant::now();
        gpu.queue.submit(std::iter::once(encoder.finish()));
        if self.staging_belt_enabled {
            if let Some(belt) = self.staging_belt.as_mut() {
                belt.recall();
            }
        }
        crate::perf::add_time(crate::perf::TimeKind::RenderSubmit, t.elapsed());
        let t = web_time::Instant::now();
        surface_texture.present();
        crate::perf::add_time(crate::perf::TimeKind::RenderPresent, t.elapsed());

        self.texture_pool.end_frame();

        let draw_calls = self.gpu_buffers.len();
        let vertex_count = self
            .gpu_buffers
            .iter()
            .map(|b| b.index_count as usize)
            .sum();
        RenderStats {
            draw_calls,
            vertex_count,
        }
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
            self.image_gpu_cache
                .process_uploads(&gpu.device, &gpu.queue, &mut store);
        }
        #[cfg(feature = "map")]
        self.sync_tile_atlas(gpu);

        let resolution = [self.logical_width as f32, self.logical_height as f32];
        let clip_slot_map =
            self.write_clip_uniform_slots(gpu, &render_ops, resolution, elapsed, scale);

        self.collect_frame_geometry(gpu, &render_ops, &clip_slot_map);

        let mut encoder = gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
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
            let mut current_scissor: (u32, u32, u32, u32) = (0, 0, phys_w, phys_h);
            render_pass.set_pipeline(&self.rect_pipeline);
            let geom = &self.frame_geom[self.frame_geom_idx];
            if let (Some(vb), Some(ib)) = (geom.vertex.as_ref(), geom.index.as_ref()) {
                render_pass.set_vertex_buffer(0, vb.slice(..));
                render_pass.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint32);
            }

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
                                        render_pass.set_bind_group(
                                            0,
                                            &self.uniform_bind_group,
                                            &[batch.uniform_offset],
                                        );
                                    }
                                    ShaderType::Text => {
                                        render_pass.set_pipeline(&self.text_pipeline);
                                        render_pass.set_bind_group(
                                            0,
                                            &self.uniform_bind_group,
                                            &[batch.uniform_offset],
                                        );
                                        render_pass.set_bind_group(1, &self.text_bind_group, &[]);
                                    }
                                    ShaderType::Shadow => {
                                        render_pass.set_pipeline(&self.shadow_pipeline);
                                        render_pass.set_bind_group(
                                            0,
                                            &self.uniform_bind_group,
                                            &[batch.uniform_offset],
                                        );
                                    }
                                    ShaderType::InnerShadow => {
                                        render_pass.set_pipeline(&self.inner_shadow_pipeline);
                                        render_pass.set_bind_group(
                                            0,
                                            &self.uniform_bind_group,
                                            &[batch.uniform_offset],
                                        );
                                    }
                                    ShaderType::Image => {
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
                                        render_pass.set_bind_group(
                                            0,
                                            &self.uniform_bind_group,
                                            &[batch.uniform_offset],
                                        );
                                    }
                                    ShaderType::GlowShadow => {
                                        render_pass.set_pipeline(&self.glow_shadow_pipeline);
                                        render_pass.set_bind_group(
                                            0,
                                            &self.uniform_bind_group,
                                            &[batch.uniform_offset],
                                        );
                                    }
                                    ShaderType::Effect => {
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
                                render_pass.set_bind_group(
                                    0,
                                    &self.uniform_bind_group,
                                    &[batch.uniform_offset],
                                );
                            }

                            if batch.clip_rect.enabled {
                                let Some(scissor) = batch.clip_rect.scissor(scale, phys_w, phys_h)
                                else {
                                    buffer_index += 1;
                                    continue;
                                };
                                if current_scissor != scissor {
                                    current_scissor = scissor;
                                    render_pass.set_scissor_rect(
                                        scissor.0, scissor.1, scissor.2, scissor.3,
                                    );
                                }
                            } else if current_scissor != (0, 0, phys_w, phys_h) {
                                current_scissor = (0, 0, phys_w, phys_h);
                                render_pass.set_scissor_rect(0, 0, phys_w, phys_h);
                            }

                            render_pass.draw_indexed(
                                batch.index_start..batch.index_start + batch.index_count,
                                0,
                                0..1,
                            );

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
        let vertex_count = self
            .gpu_buffers
            .iter()
            .map(|b| b.index_count as usize)
            .sum();
        RenderStats {
            draw_calls,
            vertex_count,
        }
    }

    /// Собирает вершины/индексы всех батчей в общие буферы кадра и
    /// заполняет `gpu_buffers` диапазонами индексов.
    fn collect_frame_geometry(
        &mut self,
        gpu: &GpuShared,
        render_ops: &[RenderOp],
        clip_slot_map: &std::collections::HashMap<crate::render::ClipRect, usize>,
    ) {
        self.gpu_buffers.clear();
        self.frame_vertices.clear();
        self.frame_indices.clear();
        for op in render_ops {
            if let RenderOp::Draw(batch) = op {
                if batch.vertices.is_empty() {
                    continue;
                }
                let base = self.frame_vertices.len() as u32;
                let index_start = self.frame_indices.len() as u32;
                self.frame_vertices.extend_from_slice(&batch.vertices);
                self.frame_indices
                    .extend(batch.indices.iter().map(|&i| i + base));
                let uniform_offset =
                    clip_slot_map.get(&batch.clip_rect).copied().unwrap_or(0) as u32;
                self.gpu_buffers.push(GpuBatchBuffers {
                    index_start,
                    index_count: batch.indices.len() as u32,
                    shader_type: batch.shader_type,
                    clip_rect: batch.clip_rect,
                    texture_id: batch.texture,
                    uniform_offset,
                });
            }
        }
        if self.frame_vertices.is_empty() {
            return;
        }
        self.frame_geom_idx = (self.frame_geom_idx + 1) % self.frame_geom.len();
        let geom = &mut self.frame_geom[self.frame_geom_idx];
        let vbytes: &[u8] = bytemuck::cast_slice(&self.frame_vertices);
        let ibytes: &[u8] = bytemuck::cast_slice(&self.frame_indices);
        super::FrameGeometry::ensure(
            &gpu.device,
            &mut geom.vertex,
            &mut geom.vertex_cap,
            vbytes.len() as u64,
            wgpu::BufferUsages::VERTEX,
            "Frame Vertex Buffer",
        );
        super::FrameGeometry::ensure(
            &gpu.device,
            &mut geom.index,
            &mut geom.index_cap,
            ibytes.len() as u64,
            wgpu::BufferUsages::INDEX,
            "Frame Index Buffer",
        );
        if let Some(vb) = geom.vertex.as_ref() {
            gpu.queue.write_buffer(vb, 0, vbytes);
        }
        if let Some(ib) = geom.index.as_ref() {
            gpu.queue.write_buffer(ib, 0, ibytes);
        }
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
            // Маска в шейдере — только для скруглённых углов. Прямоугольный
            // клип целиком отдан ножницам: маска применяется к каждому слою
            // по отдельности, и на краевом пикселе верхний слой перестаёт
            // полностью закрывать нижний — из-под него проступает полоска.
            let (clip_rect, clip_corner_radius) = if clip.enabled && clip.has_corner_radius() {
                ([clip.x(), clip.y(), clip.width(), clip.height()], radii)
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

        gpu.queue
            .write_buffer(&self.uniform_buffer, 0, &buffer_data);

        clip_map
    }
}
