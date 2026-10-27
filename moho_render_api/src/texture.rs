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
    /// Build an image, refusing a zero dimension or a pixel count that is not
    /// `width * height`.
    pub fn new(size: [u32; 2], pixels: Vec<[u8; 4]>) -> Result<Self, ImageSizeError> {
        let _ = (size, pixels);
        panic!("Rgba8Image::new is not implemented")
    }

    /// Width and height in pixels.
    pub fn size(&self) -> [u32; 2] {
        self.size
    }

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
        assert!(too_few.is_err(), "three pixels for 2x2");
        assert!(too_many.is_err(), "five pixels for 2x2");
        assert!(transposed.is_err(), "three pixels for 4x1");
        assert_eq!(good.size(), [2, 2]);
        assert_eq!(good.pixels(), pixels);
        assert_eq!(wide.size(), [4, 1]);
    }
}
