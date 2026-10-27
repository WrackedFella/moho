//! Handle-indexed GPU textures for the main pass.

use moho_render_api::Rgba8Image;

/// `image` followed by each successive half-size level down to 1x1, every texel the
/// average of its 2x2 source block taken in linear colour (alpha averaged as stored).
pub(crate) fn mip_chain(image: &Rgba8Image) -> Vec<Rgba8Image> {
    let _ = image;
    panic!("mip_chain is not implemented")
}

/// Descriptor of the GPU texture holding `image` with its full mip chain: 2D,
/// `Rgba8UnormSrgb` so sampling returns linear colour, uploadable and samplable.
pub(crate) fn texture_descriptor(image: &Rgba8Image) -> wgpu::TextureDescriptor<'static> {
    let _ = image;
    panic!("texture_descriptor is not implemented")
}

/// The one sampler every texture is read through: trilinear, repeating in u and v.
pub(crate) fn sampler_descriptor() -> wgpu::SamplerDescriptor<'static> {
    panic!("sampler_descriptor is not implemented")
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
