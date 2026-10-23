//! HDR scene target and the tone-map pass that resolves it, plus bloom, to the swapchain.

use crate::gpu_types::ToneMapGpu;

/// Format of the scene target. Guaranteed renderable, blendable and filterable on every adapter.
pub(crate) const HDR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;

const MIN_EXPOSURE: f32 = 0.5;
const MAX_EXPOSURE: f32 = 2.0;

/// Exposure multiplier for a Brightness setting: clamped to `[0.5, 2.0]`, and NaN gives 1.0.
pub fn brightness_exposure(brightness: f32) -> f32 {
    if brightness.is_nan() {
        return 1.0;
    }
    brightness.clamp(MIN_EXPOSURE, MAX_EXPOSURE)
}

/// Screen-sized colour target the main pass draws into.
pub(crate) struct HdrTarget {
    _texture: wgpu::Texture,
    view: wgpu::TextureView,
}

impl HdrTarget {
    pub(crate) fn new(device: &wgpu::Device, width: u32, height: u32) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("hdr-target"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: HDR_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        Self {
            _texture: texture,
            view,
        }
    }

    pub(crate) fn view(&self) -> &wgpu::TextureView {
        &self.view
    }
}

/// Fullscreen pass that tone-maps an [`HdrTarget`], with a bloom texture added, onto a
/// swapchain view.
pub(crate) struct ToneMapPass {
    pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    bind_group: wgpu::BindGroup,
    uniform: wgpu::Buffer,
    bloom_sampler: wgpu::Sampler,
}

impl ToneMapPass {
    /// Builds the pass for a swapchain of `surface_format`, reading `hdr` and `bloom`.
    pub(crate) fn new(
        device: &wgpu::Device,
        surface_format: wgpu::TextureFormat,
        hdr: &HdrTarget,
        bloom: &wgpu::TextureView,
    ) -> Self {
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("tonemap-bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: wgpu::BufferSize::new(
                            std::mem::size_of::<ToneMapGpu>() as u64
                        ),
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("tonemap-pipeline-layout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("tonemap-shader"),
            source: wgpu::ShaderSource::Wgsl(crate::pipeline::tonemap_shader_source().into()),
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("tonemap-pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("tonemap-uniform"),
            size: std::mem::size_of::<ToneMapGpu>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bloom_sampler = crate::bloom::linear_clamp_sampler(device, "tonemap-bloom-sampler");
        let bind_group = Self::create_bind_group(
            device,
            &bind_group_layout,
            [hdr.view(), bloom],
            &uniform,
            &bloom_sampler,
        );
        Self {
            pipeline,
            bind_group_layout,
            bind_group,
            uniform,
            bloom_sampler,
        }
    }

    fn create_bind_group(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        [hdr, bloom]: [&wgpu::TextureView; 2],
        uniform: &wgpu::Buffer,
        bloom_sampler: &wgpu::Sampler,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("tonemap-bind-group"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(hdr),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: uniform.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(bloom),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Sampler(bloom_sampler),
                },
            ],
        })
    }

    /// Points the pass at a recreated HDR target or bloom texture.
    pub(crate) fn rebind(
        &mut self,
        device: &wgpu::Device,
        hdr: &HdrTarget,
        bloom: &wgpu::TextureView,
    ) {
        self.bind_group = Self::create_bind_group(
            device,
            &self.bind_group_layout,
            [hdr.view(), bloom],
            &self.uniform,
            &self.bloom_sampler,
        );
    }

    /// Writes the exposure, curve switch and bloom weight the next
    /// [`render`](Self::render) reads.
    pub(crate) fn set_params(
        &self,
        queue: &wgpu::Queue,
        exposure: f32,
        enabled: bool,
        bloom_intensity: f32,
    ) {
        let gpu = ToneMapGpu::new(exposure, enabled, bloom_intensity);
        queue.write_buffer(&self.uniform, 0, bytemuck::bytes_of(&gpu));
    }

    /// Records the pass, covering every pixel of `target`.
    pub(crate) fn render(&self, encoder: &mut wgpu::CommandEncoder, target: &wgpu::TextureView) {
        let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("tonemap-rpass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
                depth_slice: None,
            })],
            depth_stencil_attachment: None,
            occlusion_query_set: None,
            timestamp_writes: None,
            multiview_mask: None,
        });
        rpass.set_pipeline(&self.pipeline);
        rpass.set_bind_group(0, &self.bind_group, &[]);
        rpass.draw(0..3, 0..1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn brightness_exposure_clamps_and_rejects_nan() {
        assert_eq!(brightness_exposure(1.0), 1.0);
        assert_eq!(brightness_exposure(0.75), 0.75);
        assert_eq!(brightness_exposure(0.5), 0.5);
        assert_eq!(brightness_exposure(2.0), 2.0);
        assert_eq!(brightness_exposure(0.1), 0.5);
        assert_eq!(brightness_exposure(-3.0), 0.5);
        assert_eq!(brightness_exposure(7.0), 2.0);
        assert_eq!(brightness_exposure(f32::INFINITY), 2.0);
        assert_eq!(brightness_exposure(f32::NEG_INFINITY), 0.5);
        assert_eq!(brightness_exposure(f32::NAN), 1.0);
    }
}
