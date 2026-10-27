//! Handle-indexed GPU textures for the main pass.

use moho_render_api::Rgba8Image;

/// `image` followed by each successive half-size level down to 1x1, every texel the
/// average of its 2x2 source block taken in linear colour (alpha averaged as stored).
pub(crate) fn mip_chain(image: &Rgba8Image) -> Vec<Rgba8Image> {
    let _ = image;
    panic!("mip_chain is not implemented")
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
        // Left 2x2 block is a black and white checker, right block is white.
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
        // Half black, half white is 0.5 linear, which is 188 encoded as sRGB (a plain
        // average of the encoded values would give 127 or 128).
        assert_near(chain[1].pixels()[0], [188, 188, 188, 255], 1);
        assert_near(chain[1].pixels()[1], WHITE, 0);
        // (0.5 + 1.0) / 2 = 0.75 linear, 225 encoded.
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
    fn mip_chain_averages_alpha_as_stored_not_as_colour() {
        let base = image([2, 1], vec![[255, 255, 255, 0], [255, 255, 255, 255]]);

        let chain = mip_chain(&base);

        let top = chain.last().expect("a chain has a last level");
        assert_eq!(top.size(), [1, 1]);
        assert_near(top.pixels()[0], [255, 255, 255, 128], 1);
    }
}
