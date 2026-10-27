//! Handle-indexed GPU textures for the main pass.

use moho_render_api::Rgba8Image;

/// The handle of the 1x1 white texture every untextured draw samples; never freed.
pub(crate) const WHITE_TEXTURE: u32 = 0;

/// Base-colour textures by handle, each with the group-2 bind group the main pass sets
/// for its draws. Slot [`WHITE_TEXTURE`] holds the white default from creation on.
pub(crate) struct TextureTable {
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    slots: Vec<Option<TextureSlot>>,
}

struct TextureSlot {
    /// Held so the texture's lifetime is the slot's, whatever the bind group keeps.
    _texture: wgpu::Texture,
    bind_group: wgpu::BindGroup,
}

impl TextureTable {
    /// A table holding only the white default, bound through `layout` (group 2 of the main
    /// pipeline).
    pub(crate) fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        layout: wgpu::BindGroupLayout,
    ) -> Self {
        let mut table = Self {
            layout,
            sampler: device.create_sampler(&sampler_descriptor()),
            slots: Vec::new(),
        };
        let white = Rgba8Image::new([1, 1], vec![[255; 4]]).expect("1x1 with one pixel");
        let handle = table.register(device, queue, &white);
        debug_assert_eq!(
            handle, WHITE_TEXTURE,
            "the first texture is the white default"
        );
        table
    }

    /// Upload `image` with its full mip chain and return its handle.
    pub(crate) fn register(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        image: &Rgba8Image,
    ) -> u32 {
        let texture = device.create_texture(&texture_descriptor(image));
        for (level, mip) in (0u32..).zip(mip_chain(image)) {
            let [width, height] = mip.size();
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: level,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                mip.pixels().as_flattened(),
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(4 * width),
                    rows_per_image: Some(height),
                },
                wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
            );
        }
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("base-colour-bind-group"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        });
        let handle = self.slots.len() as u32;
        self.slots.push(Some(TextureSlot {
            _texture: texture,
            bind_group,
        }));
        handle
    }

    /// Free `handle`'s texture; the white default and unknown handles are left alone.
    pub(crate) fn unregister(&mut self, handle: u32) {
        if handle == WHITE_TEXTURE {
            return;
        }
        if let Some(slot) = self.slots.get_mut(handle as usize) {
            *slot = None;
        }
    }

    /// The bind group of `handle`, or the white default's when `handle` was never
    /// registered or has been freed, so the draw still shows untextured.
    pub(crate) fn bind_group(&self, handle: u32) -> &wgpu::BindGroup {
        let white = || {
            self.slots[WHITE_TEXTURE as usize]
                .as_ref()
                .expect("the white default is never freed")
        };
        let slot = self
            .slots
            .get(handle as usize)
            .and_then(Option::as_ref)
            .unwrap_or_else(white);
        &slot.bind_group
    }
}

/// `image` followed by each successive half-size level down to 1x1, every texel the
/// average of its 2x2 source block taken in linear colour (alpha averaged as stored).
///
/// Level sizes are `max(1, n >> 1)` per axis, as wgpu requires, so an odd trailing row
/// or column is not read by the next level; a side already 1 wide reuses its one texel.
pub(crate) fn mip_chain(image: &Rgba8Image) -> Vec<Rgba8Image> {
    let mut chain = vec![image.clone()];
    loop {
        let source = chain.last().expect("the chain starts with the image");
        let [width, height] = source.size();
        if width == 1 && height == 1 {
            return chain;
        }
        let next = [(width >> 1).max(1), (height >> 1).max(1)];
        let texel = |x: u32, y: u32| source.pixels()[(y * width + x) as usize];
        let mut pixels = Vec::with_capacity((next[0] * next[1]) as usize);
        for y in 0..next[1] {
            for x in 0..next[0] {
                let (x0, y0) = (2 * x, 2 * y);
                let (x1, y1) = ((x0 + 1).min(width - 1), (y0 + 1).min(height - 1));
                pixels.push(average([
                    texel(x0, y0),
                    texel(x1, y0),
                    texel(x0, y1),
                    texel(x1, y1),
                ]));
            }
        }
        let level = Rgba8Image::new(next, pixels).expect("one pixel per texel of the level");
        chain.push(level);
    }
}

/// The mean of four sRGB texels, colour averaged in linear space and alpha as stored.
fn average(texels: [[u8; 4]; 4]) -> [u8; 4] {
    let mut out = [0; 4];
    for (channel, value) in out.iter_mut().enumerate().take(3) {
        let linear: f32 = texels.iter().map(|t| srgb_to_linear(t[channel])).sum();
        *value = linear_to_srgb(linear / 4.0);
    }
    let alpha: u32 = texels.iter().map(|t| u32::from(t[3])).sum();
    out[3] = ((alpha + 2) / 4) as u8;
    out
}

/// The linear value of an sRGB-encoded byte, from a table built on first use: every mip
/// texel reads four per channel, and `powf` dominates the chain's cost.
fn srgb_to_linear(encoded: u8) -> f32 {
    static TABLE: std::sync::LazyLock<[f32; 256]> = std::sync::LazyLock::new(|| {
        std::array::from_fn(|i| {
            let c = i as f32 / 255.0;
            if c <= 0.040_45 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        })
    });
    TABLE[usize::from(encoded)]
}

fn linear_to_srgb(linear: f32) -> u8 {
    let c = if linear <= 0.003_130_8 {
        linear * 12.92
    } else {
        1.055 * linear.powf(1.0 / 2.4) - 0.055
    };
    (c.clamp(0.0, 1.0) * 255.0).round() as u8
}

/// Descriptor of the GPU texture holding `image` with its full mip chain: 2D,
/// `Rgba8UnormSrgb` so sampling returns linear colour, uploadable and samplable.
pub(crate) fn texture_descriptor(image: &Rgba8Image) -> wgpu::TextureDescriptor<'static> {
    let [width, height] = image.size();
    wgpu::TextureDescriptor {
        label: Some("base-colour-texture"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: u32::BITS - width.max(height).leading_zeros(),
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    }
}

/// The one sampler every texture is read through: trilinear, repeating in u and v.
pub(crate) fn sampler_descriptor() -> wgpu::SamplerDescriptor<'static> {
    wgpu::SamplerDescriptor {
        label: Some("base-colour-sampler"),
        address_mode_u: wgpu::AddressMode::Repeat,
        address_mode_v: wgpu::AddressMode::Repeat,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        mipmap_filter: wgpu::MipmapFilterMode::Linear,
        anisotropy_clamp: 16,
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BLACK: [u8; 4] = [0, 0, 0, 255];
    const WHITE: [u8; 4] = [255, 255, 255, 255];

    fn image(size: [u32; 2], pixels: Vec<[u8; 4]>) -> Rgba8Image {
        Rgba8Image::new(size, pixels).expect("pixel count matches the size")
    }

    fn assert_near(actual: [u8; 4], expected: [u8; 4], tolerance: u8) {
        for (a, e) in actual.into_iter().zip(expected) {
            assert!(
                a.abs_diff(e) <= tolerance,
                "{actual:?} is not within {tolerance} of {expected:?}"
            );
        }
    }

    #[test]
    fn mip_chain_halves_to_one_pixel_averaging_in_linear_space() {
        let base = image(
            [4, 2],
            vec![
                BLACK, WHITE, WHITE, WHITE, //
                WHITE, BLACK, WHITE, WHITE,
            ],
        );

        let chain = mip_chain(&base);

        let sizes: Vec<[u32; 2]> = chain.iter().map(Rgba8Image::size).collect();
        assert_eq!(sizes, vec![[4, 2], [2, 1], [1, 1]]);
        assert_eq!(chain[0], base, "level 0 is the source image");
        // 0.5 linear is 188 as sRGB; averaging the encoded values would give 127 or 128.
        assert_near(chain[1].pixels()[0], [188, 188, 188, 255], 1);
        assert_near(chain[1].pixels()[1], WHITE, 0);
        // 0.75 linear is 225 as sRGB.
        assert_near(chain[2].pixels()[0], [225, 225, 225, 255], 2);
    }

    #[test]
    fn mip_chain_of_a_one_pixel_image_is_that_image() {
        let base = image([1, 1], vec![[10, 20, 30, 40]]);

        let chain = mip_chain(&base);

        assert_eq!(chain, vec![base]);
    }

    #[test]
    fn mip_chain_halves_each_axis_until_it_reaches_one() {
        let base = image([1, 4], vec![WHITE; 4]);

        let chain = mip_chain(&base);

        let sizes: Vec<[u32; 2]> = chain.iter().map(Rgba8Image::size).collect();
        assert_eq!(sizes, vec![[1, 4], [1, 2], [1, 1]]);
        assert!(
            chain
                .iter()
                .flat_map(Rgba8Image::pixels)
                .all(|p| *p == WHITE)
        );
    }

    #[test]
    fn mip_chain_of_an_odd_width_uses_floor_sizes_and_ignores_the_last_column() {
        let red = [255, 0, 0, 255];
        let base = image(
            [3, 2],
            vec![
                BLACK, WHITE, red, //
                WHITE, WHITE, red,
            ],
        );

        let chain = mip_chain(&base);

        let sizes: Vec<[u32; 2]> = chain.iter().map(Rgba8Image::size).collect();
        assert_eq!(sizes, vec![[3, 2], [1, 1]]);
        assert_eq!(chain[0], base);
        // One black and three white texels: 0.75 linear is 225 as sRGB. Any weight on the
        // red column would pull green and blue below it.
        assert_near(chain[1].pixels()[0], [225, 225, 225, 255], 1);
    }

    #[test]
    fn mip_chain_box_filter_reuses_the_last_row_when_the_source_is_one_wide() {
        let base = image([1, 2], vec![BLACK, WHITE]);

        let chain = mip_chain(&base);

        let sizes: Vec<[u32; 2]> = chain.iter().map(Rgba8Image::size).collect();
        assert_eq!(sizes, vec![[1, 2], [1, 1]]);
        // The missing second column repeats the first: black and white at 0.5 linear is
        // 188 as sRGB.
        assert_near(chain[1].pixels()[0], [188, 188, 188, 255], 1);
    }

    #[test]
    fn mip_chain_averages_alpha_as_stored_not_as_colour() {
        let base = image([2, 1], vec![[255, 255, 255, 0], [255, 255, 255, 255]]);

        let chain = mip_chain(&base);

        let top = chain.last().expect("a chain has a last level");
        assert_eq!(top.size(), [1, 1]);
        assert_near(top.pixels()[0], [255, 255, 255, 128], 1);
    }

    #[test]
    fn texture_descriptor_is_srgb_with_the_full_mip_chain() {
        let wide = image([5, 3], vec![WHITE; 15]);
        let one = image([1, 1], vec![WHITE]);

        let descriptor = texture_descriptor(&wide);
        let single = texture_descriptor(&one);

        assert_eq!(descriptor.format, wgpu::TextureFormat::Rgba8UnormSrgb);
        assert_eq!(descriptor.dimension, wgpu::TextureDimension::D2);
        assert_eq!(
            (
                descriptor.size.width,
                descriptor.size.height,
                descriptor.size.depth_or_array_layers
            ),
            (5, 3, 1)
        );
        assert_eq!(descriptor.sample_count, 1);
        assert_eq!(
            descriptor.mip_level_count,
            u32::try_from(mip_chain(&wide).len()).expect("few levels"),
            "5x3 halves to 2x1 then 1x1: three levels"
        );
        assert_eq!(descriptor.mip_level_count, 3);
        assert_eq!(single.mip_level_count, 1);
        assert!(
            descriptor
                .usage
                .contains(wgpu::TextureUsages::TEXTURE_BINDING)
        );
        assert!(descriptor.usage.contains(wgpu::TextureUsages::COPY_DST));
    }

    #[test]
    fn sampler_descriptor_is_trilinear_repeat_with_sixteen_times_anisotropy() {
        let sampler = sampler_descriptor();

        assert_eq!(sampler.min_filter, wgpu::FilterMode::Linear);
        assert_eq!(sampler.mag_filter, wgpu::FilterMode::Linear);
        assert_eq!(sampler.mipmap_filter, wgpu::MipmapFilterMode::Linear);
        assert_eq!(sampler.address_mode_u, wgpu::AddressMode::Repeat);
        assert_eq!(sampler.address_mode_v, wgpu::AddressMode::Repeat);
        assert_eq!(sampler.anisotropy_clamp, 16);
        assert!(
            sampler.compare.is_none(),
            "a colour sampler, not a comparison one"
        );
    }
}
