//! Bloom: quality levels, the mip-chain shape, and the GPU chain that blurs the HDR
//! scene's bright pixels for the tone-map pass to add back.

use crate::tone_map::HDR_FORMAT;

/// Total bloom energy added to the scene, split evenly across the chain's levels.
pub(crate) const BLOOM_STRENGTH: f32 = 0.5;

/// Peak channel value above which a pixel blooms (`bloom_threshold` in `bloom.wgsl`).
const BLOOM_THRESHOLD: f32 = 1.0;

/// Width of the soft knee above the threshold (`bloom_knee` in `bloom.wgsl`).
const BLOOM_KNEE: f32 = 0.25;

/// How much of the bloom chain runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BloomQuality {
    Off,
    Low,
    High,
}

impl BloomQuality {
    /// Maps the stored setting: 0 Off, 1 Low, 2 High; any other value gives High.
    pub(crate) fn from_u8(value: u8) -> Self {
        match value {
            0 => Self::Off,
            1 => Self::Low,
            _ => Self::High,
        }
    }

    /// Divisor from the target size to the first level, and the number of levels.
    fn shape(self) -> Option<(u32, usize)> {
        match self {
            Self::Off => None,
            Self::Low => Some((4, 5)),
            Self::High => Some((2, 6)),
        }
    }
}

/// Extents of each bloom mip for a `width` x `height` target, largest first.
pub(crate) fn mip_extents(width: u32, height: u32, quality: BloomQuality) -> Vec<(u32, u32)> {
    let Some((divisor, levels)) = quality.shape() else {
        return Vec::new();
    };
    let first = ((width / divisor).max(1), (height / divisor).max(1));
    std::iter::successors(Some(first), |&(w, h)| {
        Some(((w / 2).max(1), (h / 2).max(1)))
    })
    .take(levels)
    .collect()
}

/// Strength of the bloom added in the composite; Low and High add the same total.
pub(crate) fn bloom_intensity(quality: BloomQuality) -> f32 {
    match quality.shape() {
        None => 0.0,
        Some((_, levels)) => BLOOM_STRENGTH / levels as f32,
    }
}

/// Linear, clamp-to-edge sampler for reading bloom mips.
pub(crate) fn linear_clamp_sampler(device: &wgpu::Device, label: &str) -> wgpu::Sampler {
    device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some(label),
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        address_mode_w: wgpu::AddressMode::ClampToEdge,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        mipmap_filter: wgpu::MipmapFilterMode::Nearest,
        ..Default::default()
    })
}

/// The GPU bloom chain: prefilter from the HDR target into mip 0, downsample down the
/// mips, then upsample back up them additively. Mip 0 ([`output`](Self::output)) is
/// what the tone-map pass adds; with bloom Off it is a 1x1 black texture.
pub(crate) struct BloomChain {
    prefilter: wgpu::RenderPipeline,
    downsample: wgpu::RenderPipeline,
    upsample: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    _black: wgpu::Texture,
    black_view: wgpu::TextureView,
    quality: BloomQuality,
    size: (u32, u32),
    levels: Option<BloomLevels>,
}

/// The mip texture of one chain size and quality, with a bind group reading each mip.
struct BloomLevels {
    _texture: wgpu::Texture,
    views: Vec<wgpu::TextureView>,
    prefilter_bind_group: wgpu::BindGroup,
    mip_bind_groups: Vec<wgpu::BindGroup>,
}

impl BloomChain {
    /// Builds the chain for a `width` x `height` HDR target viewed by `hdr`.
    pub(crate) fn new(
        device: &wgpu::Device,
        hdr: &wgpu::TextureView,
        (width, height): (u32, u32),
        quality: BloomQuality,
    ) -> Self {
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("bloom-bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
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
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("bloom-pipeline-layout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("bloom-shader"),
            source: wgpu::ShaderSource::Wgsl(crate::pipeline::bloom_shader_source().into()),
        });
        let constants = [
            ("bloom_threshold", f64::from(BLOOM_THRESHOLD)),
            ("bloom_knee", f64::from(BLOOM_KNEE)),
        ];
        let pipeline = |entry_point: &str, constants: &[(&str, f64)], blend| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(entry_point),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    buffers: &[],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(entry_point),
                    compilation_options: wgpu::PipelineCompilationOptions {
                        constants,
                        ..Default::default()
                    },
                    targets: &[Some(wgpu::ColorTargetState {
                        format: HDR_FORMAT,
                        blend,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview_mask: None,
                cache: None,
            })
        };
        let additive = wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::One,
            dst_factor: wgpu::BlendFactor::One,
            operation: wgpu::BlendOperation::Add,
        };
        let prefilter = pipeline("fs_prefilter", &constants, None);
        let downsample = pipeline("fs_downsample", &[], None);
        let upsample = pipeline(
            "fs_upsample",
            &[],
            Some(wgpu::BlendState {
                color: additive,
                alpha: additive,
            }),
        );
        let sampler = linear_clamp_sampler(device, "bloom-sampler");
        // wgpu zero-initialises a texture before its first read, so this one stays black.
        let black = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("bloom-off"),
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: HDR_FORMAT,
            usage: wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let black_view = black.create_view(&wgpu::TextureViewDescriptor::default());
        let mut chain = Self {
            prefilter,
            downsample,
            upsample,
            bind_group_layout,
            sampler,
            _black: black,
            black_view,
            quality,
            size: (width, height),
            levels: None,
        };
        chain.rebuild(device, hdr);
        chain
    }

    pub(crate) fn quality(&self) -> BloomQuality {
        self.quality
    }

    /// Weight the tone-map pass gives [`output`](Self::output).
    pub(crate) fn intensity(&self) -> f32 {
        bloom_intensity(self.quality)
    }

    /// The finished bloom: mip 0, or a 1x1 black texture when Off.
    pub(crate) fn output(&self) -> &wgpu::TextureView {
        self.levels
            .as_ref()
            .and_then(|levels| levels.views.first())
            .unwrap_or(&self.black_view)
    }

    /// Recreates the mips for a resized HDR target viewed by `hdr`.
    pub(crate) fn resize(
        &mut self,
        device: &wgpu::Device,
        hdr: &wgpu::TextureView,
        size: (u32, u32),
    ) {
        self.size = size;
        self.rebuild(device, hdr);
    }

    /// Recreates the mips for `quality`, reading the HDR target viewed by `hdr`.
    pub(crate) fn set_quality(
        &mut self,
        device: &wgpu::Device,
        hdr: &wgpu::TextureView,
        quality: BloomQuality,
    ) {
        self.quality = quality;
        self.rebuild(device, hdr);
    }

    fn rebuild(&mut self, device: &wgpu::Device, hdr: &wgpu::TextureView) {
        let mut extents = mip_extents(self.size.0, self.size.1, self.quality);
        // A texture holds mips only until both sides reach 1 px; the levels past that
        // would repeat 1x1 and wgpu rejects them.
        if let Some(first_unit) = extents.iter().position(|&e| e == (1, 1)) {
            extents.truncate(first_unit + 1);
        }
        self.levels = extents
            .first()
            .map(|&first| self.create_levels(device, hdr, first, extents.len() as u32));
    }

    fn create_levels(
        &self,
        device: &wgpu::Device,
        hdr: &wgpu::TextureView,
        (width, height): (u32, u32),
        mip_level_count: u32,
    ) -> BloomLevels {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("bloom-mips"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: HDR_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let views: Vec<wgpu::TextureView> = (0..mip_level_count)
            .map(|mip| {
                texture.create_view(&wgpu::TextureViewDescriptor {
                    label: Some("bloom-mip-view"),
                    base_mip_level: mip,
                    mip_level_count: Some(1),
                    ..Default::default()
                })
            })
            .collect();
        let mip_bind_groups = views
            .iter()
            .map(|view| self.create_bind_group(device, view))
            .collect();
        BloomLevels {
            _texture: texture,
            prefilter_bind_group: self.create_bind_group(device, hdr),
            views,
            mip_bind_groups,
        }
    }

    fn create_bind_group(
        &self,
        device: &wgpu::Device,
        source: &wgpu::TextureView,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("bloom-bind-group"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(source),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        })
    }

    /// Records the chain, one render pass per step. Nothing when Off.
    pub(crate) fn render(&self, encoder: &mut wgpu::CommandEncoder) {
        let Some(levels) = &self.levels else {
            return;
        };
        let last = levels.views.len() - 1;
        let step = |encoder: &mut wgpu::CommandEncoder,
                    pipeline: &wgpu::RenderPipeline,
                    source: &wgpu::BindGroup,
                    target: &wgpu::TextureView,
                    load: wgpu::LoadOp<wgpu::Color>| {
            let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("bloom-rpass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load,
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
                multiview_mask: None,
            });
            rpass.set_pipeline(pipeline);
            rpass.set_bind_group(0, source, &[]);
            rpass.draw(0..3, 0..1);
        };
        let clear = wgpu::LoadOp::Clear(wgpu::Color::BLACK);

        step(
            encoder,
            &self.prefilter,
            &levels.prefilter_bind_group,
            &levels.views[0],
            clear,
        );
        for mip in 0..last {
            step(
                encoder,
                &self.downsample,
                &levels.mip_bind_groups[mip],
                &levels.views[mip + 1],
                clear,
            );
        }
        for mip in (0..last).rev() {
            step(
                encoder,
                &self.upsample,
                &levels.mip_bind_groups[mip + 1],
                &levels.views[mip],
                wgpu::LoadOp::Load,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn high_starts_at_half_resolution_with_six_levels() {
        let extents = mip_extents(1920, 1080, BloomQuality::High);

        assert_eq!(
            extents,
            [
                (960, 540),
                (480, 270),
                (240, 135),
                (120, 67),
                (60, 33),
                (30, 16)
            ]
        );
    }

    #[test]
    fn low_starts_at_quarter_resolution_with_five() {
        let extents = mip_extents(1920, 1080, BloomQuality::Low);

        assert_eq!(
            extents,
            [(480, 270), (240, 135), (120, 67), (60, 33), (30, 16)]
        );
    }

    #[test]
    fn odd_sizes_floor_each_halving() {
        let extents = mip_extents(101, 75, BloomQuality::High);

        assert_eq!(
            extents,
            [(50, 37), (25, 18), (12, 9), (6, 4), (3, 2), (1, 1)]
        );
    }

    #[test]
    fn tiny_targets_keep_every_level_at_one_pixel() {
        assert_eq!(mip_extents(1, 1, BloomQuality::High), [(1, 1); 6]);
        assert_eq!(mip_extents(2, 1, BloomQuality::Low), [(1, 1); 5]);
    }

    #[test]
    fn off_has_no_levels_and_zero_intensity() {
        let extents = mip_extents(1920, 1080, BloomQuality::Off);
        let intensity = bloom_intensity(BloomQuality::Off);

        assert!(extents.is_empty());
        assert_eq!(intensity, 0.0);
    }

    #[test]
    fn intensity_is_strength_over_level_count() {
        let high = bloom_intensity(BloomQuality::High);
        let low = bloom_intensity(BloomQuality::Low);

        assert_eq!(high, BLOOM_STRENGTH / 6.0);
        assert_eq!(low, BLOOM_STRENGTH / 5.0);
    }

    #[test]
    fn from_u8_maps_the_stored_setting() {
        assert_eq!(BloomQuality::from_u8(0), BloomQuality::Off);
        assert_eq!(BloomQuality::from_u8(1), BloomQuality::Low);
        assert_eq!(BloomQuality::from_u8(2), BloomQuality::High);
        assert_eq!(BloomQuality::from_u8(3), BloomQuality::High);
        assert_eq!(BloomQuality::from_u8(255), BloomQuality::High);
    }

    proptest! {
        #[test]
        fn levels_never_go_below_one_pixel(
            width in 1u32..=16_384,
            height in 1u32..=16_384,
            quality in prop_oneof![Just(BloomQuality::Low), Just(BloomQuality::High)],
        ) {
            let extents = mip_extents(width, height, quality);

            prop_assert!(!extents.is_empty());
            for (w, h) in &extents {
                prop_assert!(*w >= 1 && *h >= 1, "{extents:?}");
            }
            for pair in extents.windows(2) {
                prop_assert_eq!(pair[1], ((pair[0].0 / 2).max(1), (pair[0].1 / 2).max(1)));
            }
            let divisor = if quality == BloomQuality::High { 2 } else { 4 };
            prop_assert_eq!(extents[0], ((width / divisor).max(1), (height / divisor).max(1)));
        }
    }
}
