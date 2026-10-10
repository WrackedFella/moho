//! UI pass logic that needs no GPU: scissor clamping, draw-list building and
//! texture bookkeeping.

use moho_render_api::{UiFrame, UiRect, UiTextureId, UiTextureSet};
use std::collections::HashMap;
use std::ops::Range;

mod gpu;
pub(crate) use gpu::UiPass;

/// Scissor rect in physical pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Scissor {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

/// Clip rect (points) to physical pixels: scale by `pixels_per_point`, round,
/// clamp to `target`. `None` when the result has zero width or height.
pub(crate) fn scissor(clip: UiRect, pixels_per_point: f32, target: [u32; 2]) -> Option<Scissor> {
    // `as u32` saturates, so negative and NaN coordinates become 0.
    let px = |v: f32| (v * pixels_per_point).round() as u32;
    let min_x = px(clip.min[0]).min(target[0]);
    let min_y = px(clip.min[1]).min(target[1]);
    let max_x = px(clip.max[0]).clamp(min_x, target[0]);
    let max_y = px(clip.max[1]).clamp(min_y, target[1]);
    let (width, height) = (max_x - min_x, max_y - min_y);
    (width > 0 && height > 0).then_some(Scissor {
        x: min_x,
        y: min_y,
        width,
        height,
    })
}

/// One indexed draw into the frame's concatenated vertex/index buffers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct UiDraw {
    pub texture: UiTextureId,
    pub indices: Range<u32>,
    pub base_vertex: i32,
    pub scissor: Scissor,
}

/// Meshes in order to draws in order. Index range and `base_vertex` address
/// the mesh's slice in buffers formed by concatenating every mesh's indices
/// and vertices in order; skipped meshes keep their slot so offsets are
/// stable. Skips meshes with an empty scissor, no indices, or a texture
/// `book` does not hold. Empty when `pixels_per_point` is not finite and positive.
pub(crate) fn build_draws(frame: &UiFrame, target: [u32; 2], book: &TextureBook) -> Vec<UiDraw> {
    if !(frame.pixels_per_point.is_finite() && frame.pixels_per_point > 0.0) {
        tracing::warn!(
            pixels_per_point = frame.pixels_per_point,
            "skipping UI draw: invalid pixels_per_point"
        );
        return Vec::new();
    }
    let mut draws = Vec::with_capacity(frame.meshes.len());
    let mut index_start = 0u32;
    let mut base_vertex = 0i32;
    for mesh in &frame.meshes {
        let index_count = u32::try_from(mesh.indices.len()).expect("mesh index count fits in u32");
        let index_end = index_start + index_count;
        if index_count > 0
            && book.contains(mesh.texture)
            && let Some(scissor) = scissor(mesh.clip_rect, frame.pixels_per_point, target)
        {
            draws.push(UiDraw {
                texture: mesh.texture,
                indices: index_start..index_end,
                base_vertex,
                scissor,
            });
        }
        index_start = index_end;
        base_vertex += i32::try_from(mesh.vertices.len()).expect("mesh vertex count fits in i32");
    }
    draws
}

/// Which textures exist and their sizes; the GPU store follows its decisions.
#[derive(Debug, Default)]
pub(crate) struct TextureBook {
    sizes: HashMap<UiTextureId, [u32; 2]>,
}

/// A texture write the GPU side must perform.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TextureWrite {
    pub id: UiTextureId,
    /// `Some(size)` when a new texture of that size must be (re)created first.
    pub allocate: Option<[u32; 2]>,
    pub origin: [u32; 2],
    pub size: [u32; 2],
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub(crate) enum UiTextureError {
    #[error("texture {0:?}: pixel count does not match size")]
    PixelCount(UiTextureId),
    #[error("texture {0:?}: a full set needs a non-zero size")]
    Empty(UiTextureId),
    #[error("texture {0:?}: partial update of a texture that does not exist")]
    Missing(UiTextureId),
    #[error("texture {0:?}: partial update outside the texture")]
    OutOfBounds(UiTextureId),
}

impl TextureBook {
    /// Full set (`pos` None): size must be non-zero; records size, returns
    /// `allocate = Some(size)`, origin `[0, 0]`. Partial: the texture must exist and `pos + size` must
    /// fit; `allocate` is `None`. On error the book is unchanged.
    pub(crate) fn apply_set(&mut self, set: &UiTextureSet) -> Result<TextureWrite, UiTextureError> {
        let id = set.id;
        let size = set.image.size;
        let texels = u64::from(size[0]) * u64::from(size[1]);
        if texels != set.image.pixels.len() as u64 {
            return Err(UiTextureError::PixelCount(id));
        }
        match set.pos {
            None if size.contains(&0) => Err(UiTextureError::Empty(id)),
            None => {
                self.sizes.insert(id, size);
                Ok(TextureWrite {
                    id,
                    allocate: Some(size),
                    origin: [0, 0],
                    size,
                })
            }
            Some(origin) => {
                let existing = self.sizes.get(&id).ok_or(UiTextureError::Missing(id))?;
                let fits = |axis: usize| {
                    u64::from(origin[axis]) + u64::from(size[axis]) <= u64::from(existing[axis])
                };
                if !(fits(0) && fits(1)) {
                    return Err(UiTextureError::OutOfBounds(id));
                }
                Ok(TextureWrite {
                    id,
                    allocate: None,
                    origin,
                    size,
                })
            }
        }
    }

    /// Returns whether the texture existed.
    pub(crate) fn free(&mut self, id: UiTextureId) -> bool {
        self.sizes.remove(&id).is_some()
    }

    pub(crate) fn contains(&self, id: UiTextureId) -> bool {
        self.sizes.contains_key(&id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use moho_render_api::{UiImage, UiMesh, UiSampler, UiVertex};

    const TEX_A: UiTextureId = UiTextureId(1);
    const TEX_B: UiTextureId = UiTextureId(2);

    fn vertex() -> UiVertex {
        UiVertex {
            pos: [0.0, 0.0],
            uv: [0.0, 0.0],
            color: [255; 4],
        }
    }

    fn rect(min: [f32; 2], max: [f32; 2]) -> UiRect {
        UiRect { min, max }
    }

    fn mesh(texture: UiTextureId, clip_rect: UiRect, vertices: usize, indices: Vec<u32>) -> UiMesh {
        UiMesh {
            clip_rect,
            texture,
            vertices: vec![vertex(); vertices],
            indices,
        }
    }

    fn set(id: UiTextureId, pos: Option<[u32; 2]>, size: [u32; 2]) -> UiTextureSet {
        UiTextureSet {
            id,
            pos,
            image: UiImage {
                size,
                pixels: vec![[1, 2, 3, 4]; (size[0] * size[1]) as usize],
            },
            sampler: UiSampler::default(),
        }
    }

    fn book_with(ids: &[UiTextureId]) -> TextureBook {
        let mut book = TextureBook::default();
        for id in ids {
            book.apply_set(&set(*id, None, [8, 8])).expect("full set");
        }
        book
    }

    #[test]
    fn invalid_pixels_per_point_yields_no_draws() {
        let book = book_with(&[TEX_A]);

        for ppp in [0.0, -1.0, f32::NAN, f32::INFINITY] {
            let frame = UiFrame {
                pixels_per_point: ppp,
                meshes: vec![mesh(
                    TEX_A,
                    rect([0.0, 0.0], [10.0, 10.0]),
                    3,
                    vec![0, 1, 2],
                )],
                ..UiFrame::default()
            };

            assert!(build_draws(&frame, [100, 100], &book).is_empty(), "{ppp}");
        }
    }

    #[test]
    fn meshes_become_ordered_draws_with_texture_and_range() {
        let frame = UiFrame {
            pixels_per_point: 2.0,
            meshes: vec![
                mesh(
                    TEX_A,
                    rect([1.0, 2.0], [5.0, 6.0]),
                    4,
                    vec![0, 1, 2, 2, 1, 3],
                ),
                mesh(TEX_B, rect([0.0, 0.0], [10.0, 10.0]), 3, vec![0, 1, 2]),
            ],
            ..UiFrame::default()
        };
        let book = book_with(&[TEX_A, TEX_B]);

        let draws = build_draws(&frame, [100, 100], &book);

        assert_eq!(
            draws,
            vec![
                UiDraw {
                    texture: TEX_A,
                    indices: 0..6,
                    base_vertex: 0,
                    scissor: Scissor {
                        x: 2,
                        y: 4,
                        width: 8,
                        height: 8
                    },
                },
                UiDraw {
                    texture: TEX_B,
                    indices: 6..9,
                    base_vertex: 4,
                    scissor: Scissor {
                        x: 0,
                        y: 0,
                        width: 20,
                        height: 20
                    },
                },
            ]
        );
    }

    #[test]
    fn skipped_meshes_keep_their_slot_in_later_offsets() {
        let frame = UiFrame {
            pixels_per_point: 1.0,
            meshes: vec![
                mesh(
                    UiTextureId(99),
                    rect([0.0, 0.0], [5.0, 5.0]),
                    4,
                    vec![0, 1, 2],
                ),
                mesh(TEX_A, rect([0.0, 0.0], [5.0, 5.0]), 3, vec![0, 1, 2]),
            ],
            ..UiFrame::default()
        };
        let book = book_with(&[TEX_A]);

        let draws = build_draws(&frame, [10, 10], &book);

        assert_eq!(draws.len(), 1);
        assert_eq!(draws[0].texture, TEX_A);
        assert_eq!(draws[0].indices, 3..6);
        assert_eq!(draws[0].base_vertex, 4);
    }

    #[test]
    fn scissor_is_clamped_and_empty_clip_yields_no_draw() {
        let target = [100, 80];

        let past_edge = scissor(rect([30.0, 10.0], [80.0, 30.0]), 2.0, target);
        let negative_min = scissor(rect([-5.0, -5.0], [10.0, 10.0]), 2.0, target);
        let rounded = scissor(rect([0.3, 0.0], [1.2, 1.0]), 2.0, target);
        let outside = scissor(rect([60.0, 0.0], [70.0, 10.0]), 2.0, target);

        assert_eq!(
            past_edge,
            Some(Scissor {
                x: 60,
                y: 20,
                width: 40,
                height: 40
            })
        );
        assert_eq!(
            negative_min,
            Some(Scissor {
                x: 0,
                y: 0,
                width: 20,
                height: 20
            })
        );
        assert_eq!(
            rounded,
            Some(Scissor {
                x: 1,
                y: 0,
                width: 1,
                height: 2
            })
        );
        assert_eq!(outside, None);

        let frame = UiFrame {
            pixels_per_point: 2.0,
            meshes: vec![mesh(
                TEX_A,
                rect([60.0, 0.0], [70.0, 10.0]),
                3,
                vec![0, 1, 2],
            )],
            ..UiFrame::default()
        };
        assert!(build_draws(&frame, target, &book_with(&[TEX_A])).is_empty());
    }

    #[test]
    fn deltas_apply_in_order_partial_update_and_free() {
        let mut book = TextureBook::default();

        let full = book
            .apply_set(&set(TEX_A, None, [16, 16]))
            .expect("full set A");
        book.apply_set(&set(TEX_B, None, [4, 4]))
            .expect("full set B");
        let partial = book
            .apply_set(&set(TEX_A, Some([3, 5]), [4, 2]))
            .expect("partial update A");
        let freed = book.free(TEX_B);

        assert_eq!(
            full,
            TextureWrite {
                id: TEX_A,
                allocate: Some([16, 16]),
                origin: [0, 0],
                size: [16, 16]
            }
        );
        assert_eq!(
            partial,
            TextureWrite {
                id: TEX_A,
                allocate: None,
                origin: [3, 5],
                size: [4, 2]
            }
        );
        assert!(freed);
        assert!(book.contains(TEX_A));
        assert!(!book.contains(TEX_B));
        assert!(!book.free(TEX_B));

        let frame = UiFrame {
            pixels_per_point: 1.0,
            meshes: vec![mesh(TEX_B, rect([0.0, 0.0], [5.0, 5.0]), 3, vec![0, 1, 2])],
            ..UiFrame::default()
        };
        assert!(build_draws(&frame, [10, 10], &book).is_empty());
    }

    #[test]
    fn invalid_partial_updates_error_and_leave_book_unchanged() {
        let mut book = book_with(&[TEX_A]);

        let missing = book.apply_set(&set(TEX_B, Some([0, 0]), [1, 1]));
        let overflow_x = book.apply_set(&set(TEX_A, Some([5, 0]), [4, 1]));
        let overflow_y = book.apply_set(&set(TEX_A, Some([0, 7]), [1, 2]));
        let mut bad_pixels = set(TEX_B, None, [2, 2]);
        bad_pixels.image.pixels.pop();
        let pixel_count = book.apply_set(&bad_pixels);

        assert_eq!(missing, Err(UiTextureError::Missing(TEX_B)));
        assert_eq!(overflow_x, Err(UiTextureError::OutOfBounds(TEX_A)));
        assert_eq!(overflow_y, Err(UiTextureError::OutOfBounds(TEX_A)));
        assert_eq!(pixel_count, Err(UiTextureError::PixelCount(TEX_B)));
        assert!(!book.contains(TEX_B));
        let exact = book.apply_set(&set(TEX_A, Some([4, 6]), [4, 2]));
        assert_eq!(
            exact,
            Ok(TextureWrite {
                id: TEX_A,
                allocate: None,
                origin: [4, 6],
                size: [4, 2]
            })
        );
    }
    #[test]
    fn empty_full_set_is_rejected_and_leaves_book_unchanged() {
        let mut book = book_with(&[TEX_A]);

        let empty_new = book.apply_set(&set(TEX_B, None, [0, 4]));
        let empty_replace = book.apply_set(&set(TEX_A, None, [4, 0]));

        assert_eq!(empty_new, Err(UiTextureError::Empty(TEX_B)));
        assert_eq!(empty_replace, Err(UiTextureError::Empty(TEX_A)));
        assert!(!book.contains(TEX_B));
        let still_fits = book.apply_set(&set(TEX_A, Some([7, 7]), [1, 1]));
        assert!(still_fits.is_ok(), "{still_fits:?}");
    }
}
