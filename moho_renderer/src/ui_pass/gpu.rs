//! The wgpu half of the UI pass: pipeline, texture store and buffer upload.
//! Every decision (what to draw, what to write) comes from the pure logic in
//! the parent module.

use super::{TextureBook, TextureWrite, UiDraw, build_draws};
use moho_render_api::{UiFilter, UiFrame, UiSampler, UiTextureId, UiTextureSet, UiVertex, UiWrap};
use std::collections::HashMap;
use std::num::NonZeroU64;
use wgpu::util::DeviceExt;

const TEXTURE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Locals {
    screen_size_in_points: [f32; 2],
    dithering: u32,
    _padding: u32,
}

struct GpuTexture {
    texture: wgpu::Texture,
    bind_group: wgpu::BindGroup,
    sampler: UiSampler,
}

/// A buffer that is recreated at a larger size when a frame needs more room.
struct GrowBuffer {
    buffer: wgpu::Buffer,
    capacity: u64,
    label: &'static str,
    usage: wgpu::BufferUsages,
}

impl GrowBuffer {
    fn new(device: &wgpu::Device, label: &'static str, usage: wgpu::BufferUsages) -> Self {
        let capacity = 1024;
        Self {
            buffer: Self::create(device, label, usage, capacity),
            capacity,
            label,
            usage,
        }
    }

    fn create(
        device: &wgpu::Device,
        label: &'static str,
        usage: wgpu::BufferUsages,
        size: u64,
    ) -> wgpu::Buffer {
        device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(label),
            size,
            usage: usage | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    }

    fn reserve(&mut self, device: &wgpu::Device, size: u64) {
        if size > self.capacity {
            self.capacity = size.next_power_of_two();
            self.buffer = Self::create(device, self.label, self.usage, self.capacity);
        }
    }
}

/// Draws a game's [`UiFrame`] over the finished scene.
pub(crate) struct UiPass {
    pipeline: wgpu::RenderPipeline,
    uniform_buffer: wgpu::Buffer,
    uniform_bind_group: wgpu::BindGroup,
    texture_layout: wgpu::BindGroupLayout,
    samplers: HashMap<UiSampler, wgpu::Sampler>,
    textures: HashMap<UiTextureId, GpuTexture>,
    book: TextureBook,
    vertices: GrowBuffer,
    indices: GrowBuffer,
}

impl UiPass {
    pub(crate) fn new(device: &wgpu::Device, target_format: wgpu::TextureFormat) -> Self {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("ui-shader"),
            source: wgpu::ShaderSource::Wgsl(crate::pipeline::ui_shader_source().into()),
        });

        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("ui-uniform-buffer"),
            contents: bytemuck::bytes_of(&Locals {
                screen_size_in_points: [0.0, 0.0],
                dithering: 1,
                _padding: 0,
            }),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let uniform_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ui-uniform-layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: NonZeroU64::new(size_of::<Locals>() as u64),
                },
                count: None,
            }],
        });
        let uniform_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ui-uniform-bind-group"),
            layout: &uniform_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });
        let texture_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ui-texture-layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("ui-pipeline-layout"),
            bind_group_layouts: &[Some(&uniform_layout), Some(&texture_layout)],
            immediate_size: 0,
        });

        let fragment_entry = if target_format.is_srgb() {
            "fs_main_linear_framebuffer"
        } else {
            "fs_main_gamma_framebuffer"
        };
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("ui-pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs_main"),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: size_of::<UiVertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Uint32],
                }],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some(fragment_entry),
                targets: &[Some(wgpu::ColorTargetState {
                    format: target_format,
                    blend: Some(wgpu::BlendState {
                        color: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::One,
                            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                            operation: wgpu::BlendOperation::Add,
                        },
                        alpha: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::OneMinusDstAlpha,
                            dst_factor: wgpu::BlendFactor::One,
                            operation: wgpu::BlendOperation::Add,
                        },
                    }),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            multiview_mask: None,
            cache: None,
        });

        Self {
            pipeline,
            uniform_buffer,
            uniform_bind_group,
            texture_layout,
            samplers: HashMap::new(),
            textures: HashMap::new(),
            book: TextureBook::default(),
            vertices: GrowBuffer::new(device, "ui-vertex-buffer", wgpu::BufferUsages::VERTEX),
            indices: GrowBuffer::new(device, "ui-index-buffer", wgpu::BufferUsages::INDEX),
        }
    }

    /// Applies the frame's texture sets, draws its meshes onto `view` over
    /// what is already there, then applies its frees.
    pub(crate) fn render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        target: [u32; 2],
        frame: &UiFrame,
    ) {
        for set in &frame.textures_set {
            self.set_texture(device, queue, set);
        }
        self.draw(device, queue, encoder, view, target, frame);
        for id in &frame.textures_free {
            if self.book.free(*id) {
                self.textures.remove(id);
            }
        }
    }

    fn set_texture(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, set: &UiTextureSet) {
        let write = match self.book.apply_set(set) {
            Ok(write) => write,
            Err(error) => {
                tracing::warn!(%error, "skipping invalid UI texture update");
                return;
            }
        };
        let TextureWrite {
            id,
            allocate,
            origin,
            size,
        } = write;

        if let Some(full) = allocate {
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("ui-texture"),
                size: extent(full),
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: TEXTURE_FORMAT,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            let bind_group = texture_bind_group(
                device,
                &self.texture_layout,
                &mut self.samplers,
                &texture,
                set.sampler,
            );
            self.textures.insert(
                id,
                GpuTexture {
                    texture,
                    bind_group,
                    sampler: set.sampler,
                },
            );
        } else if let Some(existing) = self.textures.get_mut(&id)
            && existing.sampler != set.sampler
        {
            existing.bind_group = texture_bind_group(
                device,
                &self.texture_layout,
                &mut self.samplers,
                &existing.texture,
                set.sampler,
            );
            existing.sampler = set.sampler;
        }

        let entry = self
            .textures
            .get(&id)
            .expect("the book only reports textures the store holds");
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &entry.texture,
                mip_level: 0,
                origin: wgpu::Origin3d {
                    x: origin[0],
                    y: origin[1],
                    z: 0,
                },
                aspect: wgpu::TextureAspect::All,
            },
            bytemuck::cast_slice(&set.image.pixels),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4 * size[0]),
                rows_per_image: Some(size[1]),
            },
            extent(size),
        );
    }

    fn draw(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        target: [u32; 2],
        frame: &UiFrame,
    ) {
        if !(frame.pixels_per_point.is_finite() && frame.pixels_per_point > 0.0) {
            tracing::warn!(
                pixels_per_point = frame.pixels_per_point,
                "skipping UI draw: invalid pixels_per_point"
            );
            return;
        }
        let draws = build_draws(frame, target, &self.book);
        if draws.is_empty() {
            return;
        }

        queue.write_buffer(
            &self.uniform_buffer,
            0,
            bytemuck::bytes_of(&Locals {
                screen_size_in_points: [
                    target[0] as f32 / frame.pixels_per_point,
                    target[1] as f32 / frame.pixels_per_point,
                ],
                dithering: 1,
                _padding: 0,
            }),
        );
        self.upload(device, queue, frame);

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("ui-pass"),
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
            occlusion_query_set: None,
            timestamp_writes: None,
            multiview_mask: None,
        });
        pass.set_viewport(0.0, 0.0, target[0] as f32, target[1] as f32, 0.0, 1.0);
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.uniform_bind_group, &[]);
        pass.set_vertex_buffer(0, self.vertices.buffer.slice(..));
        pass.set_index_buffer(self.indices.buffer.slice(..), wgpu::IndexFormat::Uint32);
        for UiDraw {
            texture,
            indices,
            base_vertex,
            scissor,
        } in draws
        {
            let Some(gpu_texture) = self.textures.get(&texture) else {
                tracing::warn!(?texture, "UI texture missing from the GPU store");
                continue;
            };
            pass.set_scissor_rect(scissor.x, scissor.y, scissor.width, scissor.height);
            pass.set_bind_group(1, &gpu_texture.bind_group, &[]);
            pass.draw_indexed(indices, base_vertex, 0..1);
        }
    }

    /// Writes every mesh's vertices and indices back to back, in order.
    fn upload(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, frame: &UiFrame) {
        let vertex_bytes: usize = frame
            .meshes
            .iter()
            .map(|m| m.vertices.len() * size_of::<UiVertex>())
            .sum();
        let index_bytes: usize = frame.meshes.iter().map(|m| m.indices.len() * 4).sum();
        self.vertices.reserve(device, vertex_bytes as u64);
        self.indices.reserve(device, index_bytes as u64);

        let (mut vertex_offset, mut index_offset) = (0u64, 0u64);
        for mesh in &frame.meshes {
            let vertices: &[u8] = bytemuck::cast_slice(&mesh.vertices);
            let indices: &[u8] = bytemuck::cast_slice(&mesh.indices);
            if !vertices.is_empty() {
                queue.write_buffer(&self.vertices.buffer, vertex_offset, vertices);
            }
            if !indices.is_empty() {
                queue.write_buffer(&self.indices.buffer, index_offset, indices);
            }
            vertex_offset += vertices.len() as u64;
            index_offset += indices.len() as u64;
        }
    }
}

fn extent(size: [u32; 2]) -> wgpu::Extent3d {
    wgpu::Extent3d {
        width: size[0],
        height: size[1],
        depth_or_array_layers: 1,
    }
}

fn filter_mode(filter: UiFilter) -> wgpu::FilterMode {
    match filter {
        UiFilter::Nearest => wgpu::FilterMode::Nearest,
        UiFilter::Linear => wgpu::FilterMode::Linear,
    }
}

fn create_sampler(device: &wgpu::Device, sampler: UiSampler) -> wgpu::Sampler {
    let address_mode = match sampler.wrap {
        UiWrap::ClampToEdge => wgpu::AddressMode::ClampToEdge,
        UiWrap::Repeat => wgpu::AddressMode::Repeat,
        UiWrap::MirroredRepeat => wgpu::AddressMode::MirrorRepeat,
    };
    device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("ui-sampler"),
        mag_filter: filter_mode(sampler.magnification),
        min_filter: filter_mode(sampler.minification),
        address_mode_u: address_mode,
        address_mode_v: address_mode,
        ..Default::default()
    })
}

fn texture_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    samplers: &mut HashMap<UiSampler, wgpu::Sampler>,
    texture: &wgpu::Texture,
    sampler: UiSampler,
) -> wgpu::BindGroup {
    let sampler = samplers
        .entry(sampler)
        .or_insert_with(|| create_sampler(device, sampler));
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("ui-texture-bind-group"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(
                    &texture.create_view(&wgpu::TextureViewDescriptor::default()),
                ),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
        ],
    })
}
