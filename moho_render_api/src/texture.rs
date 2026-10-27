//! Texture contract: straight-alpha RGBA8 images keyed by a game-chosen id.

use std::fmt;

/// Stable, game-chosen key for one uploaded texture.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TextureId(pub u64);

/// Why an [`Rgba8Image`] was refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ImageSizeError {
    /// A dimension is zero.
    ZeroDimension { size: [u32; 2] },
    /// The pixel count is not `width * height`.
    PixelCountMismatch { size: [u32; 2], pixels: usize },
    /// A side exceeds [`Rgba8Image::MAX_SIDE`].
    TooLarge { size: [u32; 2] },
}

impl fmt::Display for ImageSizeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroDimension { size } => {
                write!(f, "image size {}x{} has a zero dimension", size[0], size[1])
            }
            Self::PixelCountMismatch { size, pixels } => write!(
                f,
                "image size {}x{} does not match {pixels} pixels",
                size[0], size[1]
            ),
            Self::TooLarge { size } => write!(
                f,
                "image size {}x{} exceeds the {} pixel side limit",
                size[0],
                size[1],
                Rgba8Image::MAX_SIDE
            ),
        }
    }
}

impl std::error::Error for ImageSizeError {}

/// Straight-alpha sRGB RGBA8 pixels, row-major from the top-left.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rgba8Image {
    size: [u32; 2],
    pixels: Vec<[u8; 4]>,
}

impl Rgba8Image {
    /// The largest width or height every backend accepts: wgpu's default
    /// `max_texture_dimension_2d`, which `moho_renderer`'s device setup requests.
    pub const MAX_SIDE: u32 = 8192;

    /// Build an image, refusing a zero dimension, a pixel count that is not
    /// `width * height`, or a side over [`Self::MAX_SIDE`].
    pub fn new(size: [u32; 2], pixels: Vec<[u8; 4]>) -> Result<Self, ImageSizeError> {
        if size[0] == 0 || size[1] == 0 {
            return Err(ImageSizeError::ZeroDimension { size });
        }
        let expected = u64::from(size[0]) * u64::from(size[1]);
        if u64::try_from(pixels.len()).ok() != Some(expected) {
            return Err(ImageSizeError::PixelCountMismatch {
                size,
                pixels: pixels.len(),
            });
        }
        if size[0] > Self::MAX_SIDE || size[1] > Self::MAX_SIDE {
            return Err(ImageSizeError::TooLarge { size });
        }
        Ok(Self { size, pixels })
    }

    /// Width and height in pixels.
    pub fn size(&self) -> [u32; 2] {
        self.size
    }

    /// The pixels, `width * height` of them, row-major from the top-left.
    pub fn pixels(&self) -> &[[u8; 4]] {
        &self.pixels
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rgba8_image_refuses_zero_size_and_wrong_pixel_count() {
        let zero_width = Rgba8Image::new([0, 2], Vec::new());
        let zero_height = Rgba8Image::new([3, 0], Vec::new());
        let too_few = Rgba8Image::new([2, 2], vec![[0; 4]; 3]);
        let too_many = Rgba8Image::new([2, 2], vec![[0; 4]; 5]);
        let transposed = Rgba8Image::new([4, 1], vec![[0; 4]; 3]);

        let pixels = vec![
            [1, 2, 3, 4],
            [5, 6, 7, 8],
            [9, 10, 11, 12],
            [13, 14, 15, 16],
        ];
        let good = Rgba8Image::new([2, 2], pixels.clone()).expect("2x2 with four pixels");
        let wide = Rgba8Image::new([4, 1], pixels.clone()).expect("4x1 with four pixels");

        assert!(matches!(
            zero_width,
            Err(ImageSizeError::ZeroDimension { .. })
        ));
        assert!(matches!(
            zero_height,
            Err(ImageSizeError::ZeroDimension { .. })
        ));
        assert!(
            matches!(too_few, Err(ImageSizeError::PixelCountMismatch { .. })),
            "three pixels for 2x2"
        );
        assert!(
            matches!(too_many, Err(ImageSizeError::PixelCountMismatch { .. })),
            "five pixels for 2x2"
        );
        assert!(
            matches!(transposed, Err(ImageSizeError::PixelCountMismatch { .. })),
            "three pixels for 4x1"
        );
        assert_eq!(good.size(), [2, 2]);
        assert_eq!(good.pixels(), pixels);
        assert_eq!(wide.size(), [4, 1]);
    }

    #[test]
    fn rgba8_image_refuses_a_side_over_the_texture_limit() {
        let over = Rgba8Image::MAX_SIDE + 1;
        let wide = Rgba8Image::new([over, 1], vec![[0; 4]; over as usize]);
        let tall = Rgba8Image::new([1, over], vec![[0; 4]; over as usize]);

        assert_eq!(wide, Err(ImageSizeError::TooLarge { size: [8193, 1] }));
        assert_eq!(tall, Err(ImageSizeError::TooLarge { size: [1, 8193] }));
    }

    #[test]
    fn rgba8_image_accepts_a_side_exactly_at_the_texture_limit() {
        let side = Rgba8Image::MAX_SIDE;

        let wide = Rgba8Image::new([side, 1], vec![[0; 4]; side as usize]);
        let tall = Rgba8Image::new([1, side], vec![[0; 4]; side as usize]);

        assert_eq!(wide.expect("8192x1").size(), [8192, 1]);
        assert_eq!(tall.expect("1x8192").size(), [1, 8192]);
    }

    #[test]
    fn rgba8_image_refuses_a_size_whose_pixel_count_overflows_u32() {
        let huge = Rgba8Image::new([65536, 65536], Vec::new());

        assert!(
            matches!(huge, Err(ImageSizeError::PixelCountMismatch { .. })),
            "2^32 pixels expected, none given; got {huge:?}"
        );
    }
}
