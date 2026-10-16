//! Several UI sources composed into the one the renderer takes.

use std::collections::HashMap;

use moho_render_api::{UiFrame, UiFrameSource, UiMesh, UiRect, UiSource, UiTextureId};

/// Pulls every layer in order each frame and merges them into one frame at
/// `pixels_per_point = 1.0`; later layers draw on top.
///
/// The renderer locks this, then each layer: a caller must not hold a layer's lock while it
/// draws.
pub struct UiLayers {
    layers: Vec<UiSource>,
    /// Each layer numbers its textures itself, so `(layer, id)` maps to an id unique here.
    textures: HashMap<(usize, UiTextureId), UiTextureId>,
    next_texture: u64,
}

impl UiLayers {
    pub fn new(layers: Vec<UiSource>) -> Self {
        Self {
            layers,
            textures: HashMap::new(),
            next_texture: 0,
        }
    }

    fn map_texture(&mut self, layer: usize, id: UiTextureId) -> UiTextureId {
        *self.textures.entry((layer, id)).or_insert_with(|| {
            let fresh = UiTextureId(self.next_texture);
            self.next_texture += 1;
            fresh
        })
    }

    fn merge(&mut self, layer: usize, frame: UiFrame, out: &mut UiFrame) {
        let scale = frame.pixels_per_point;
        for mut set in frame.textures_set {
            set.id = self.map_texture(layer, set.id);
            out.textures_set.push(set);
        }
        for mesh in frame.meshes {
            out.meshes.push(UiMesh {
                clip_rect: UiRect {
                    min: scaled(mesh.clip_rect.min, scale),
                    max: scaled(mesh.clip_rect.max, scale),
                },
                texture: self.map_texture(layer, mesh.texture),
                vertices: mesh
                    .vertices
                    .into_iter()
                    .map(|mut v| {
                        v.pos = scaled(v.pos, scale);
                        v
                    })
                    .collect(),
                indices: mesh.indices,
            });
        }
        for id in frame.textures_free {
            if let Some(mapped) = self.textures.remove(&(layer, id)) {
                out.textures_free.push(mapped);
            }
        }
    }
}

fn scaled(point: [f32; 2], scale: f32) -> [f32; 2] {
    [point[0] * scale, point[1] * scale]
}

impl UiFrameSource for UiLayers {
    fn ui_frame(&mut self, size_in_pixels: [u32; 2]) -> Option<UiFrame> {
        let mut out = UiFrame {
            pixels_per_point: 1.0,
            ..UiFrame::default()
        };
        let mut drew = false;
        for index in 0..self.layers.len() {
            let Ok(mut layer) = self.layers[index].lock() else {
                tracing::warn!(layer = index, "skipped a ui layer with a poisoned lock");
                continue;
            };
            let frame = layer.ui_frame(size_in_pixels);
            drop(layer);
            if let Some(frame) = frame {
                drew = true;
                self.merge(index, frame, &mut out);
            }
        }
        drew.then_some(out)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::sync::{Arc, Mutex};

    use moho_render_api::{
        UiImage, UiMesh, UiRect, UiSampler, UiTextureId, UiTextureSet, UiVertex,
    };
    use proptest::prelude::*;

    use super::*;

    /// Hands out its scripted frames in order, then `None`.
    struct Scripted(VecDeque<Option<UiFrame>>);

    fn layer(frames: Vec<Option<UiFrame>>) -> UiSource {
        Arc::new(Mutex::new(Scripted(frames.into())))
    }

    impl UiFrameSource for Scripted {
        fn ui_frame(&mut self, _size_in_pixels: [u32; 2]) -> Option<UiFrame> {
            self.0.pop_front().flatten()
        }
    }

    fn vertex(x: f32, y: f32) -> UiVertex {
        UiVertex {
            pos: [x, y],
            uv: [0.25, 0.75],
            color: [1, 2, 3, 4],
        }
    }

    fn texture_set(id: u64) -> UiTextureSet {
        UiTextureSet {
            id: UiTextureId(id),
            pos: None,
            image: UiImage {
                size: [1, 1],
                pixels: vec![[255; 4]],
            },
            sampler: UiSampler::default(),
        }
    }

    fn mesh(texture: u64, at: [f32; 2]) -> UiMesh {
        UiMesh {
            clip_rect: UiRect {
                min: at,
                max: [at[0] + 10.0, at[1] + 10.0],
            },
            texture: UiTextureId(texture),
            vertices: vec![vertex(at[0], at[1])],
            indices: vec![0],
        }
    }

    fn frame(ppp: f32, sets: &[u64], meshes: Vec<UiMesh>, free: &[u64]) -> UiFrame {
        UiFrame {
            pixels_per_point: ppp,
            textures_set: sets.iter().map(|&id| texture_set(id)).collect(),
            meshes,
            textures_free: free.iter().map(|&id| UiTextureId(id)).collect(),
        }
    }

    const SIZE: [u32; 2] = [800, 600];

    fn merged(layers: Vec<UiSource>) -> Option<UiFrame> {
        UiLayers::new(layers).ui_frame(SIZE)
    }

    #[test]
    fn two_layers_with_texture_zero_get_distinct_ids() {
        let below = layer(vec![Some(frame(1.0, &[0], vec![mesh(0, [1.0, 1.0])], &[]))]);
        let above = layer(vec![Some(frame(1.0, &[0], vec![mesh(0, [2.0, 2.0])], &[]))]);

        let out = merged(vec![below, above]).expect("two layers drew");

        assert_eq!(out.pixels_per_point, 1.0);
        assert_eq!(out.textures_set.len(), 2);
        let (first, second) = (out.textures_set[0].id, out.textures_set[1].id);
        assert_ne!(first, second);
        assert_eq!(out.meshes.len(), 2);
        assert_eq!(out.meshes[0].vertices[0].pos, [1.0, 1.0]);
        assert_eq!(out.meshes[0].texture, first);
        assert_eq!(out.meshes[1].vertices[0].pos, [2.0, 2.0]);
        assert_eq!(out.meshes[1].texture, second);
    }

    #[test]
    fn a_layer_at_two_points_per_pixel_is_scaled_to_pixels() {
        let source = layer(vec![Some(frame(2.0, &[0], vec![mesh(0, [3.0, 5.0])], &[]))]);

        let out = merged(vec![source]).expect("the layer drew");

        assert_eq!(out.pixels_per_point, 1.0);
        assert_eq!(out.meshes[0].vertices[0].pos, [6.0, 10.0]);
        assert_eq!(out.meshes[0].vertices[0].uv, [0.25, 0.75]);
        assert_eq!(out.meshes[0].vertices[0].color, [1, 2, 3, 4]);
        assert_eq!(out.meshes[0].clip_rect.min, [6.0, 10.0]);
        assert_eq!(out.meshes[0].clip_rect.max, [26.0, 30.0]);
    }

    proptest! {
        #[test]
        fn layer_positions_scale_by_the_layers_own_points_per_pixel(
            ppp in 0.25_f32..4.0,
            x in -1000.0_f32..1000.0,
            y in -1000.0_f32..1000.0,
        ) {
            let source = layer(vec![Some(frame(ppp, &[0], vec![mesh(0, [x, y])], &[]))]);

            let out = merged(vec![source]).expect("the layer drew");

            prop_assert_eq!(out.meshes.len(), 1);
            let near = |actual: f32, expected: f32| (actual - expected).abs() <= 1e-3;
            let v = out.meshes[0].vertices[0].pos;
            prop_assert_eq!(out.pixels_per_point, 1.0);
            prop_assert!(near(v[0], x * ppp) && near(v[1], y * ppp), "{:?}", v);
            let clip = out.meshes[0].clip_rect;
            prop_assert!(near(clip.min[0], x * ppp) && near(clip.min[1], y * ppp));
            prop_assert!(near(clip.max[0], (x + 10.0) * ppp));
            prop_assert!(near(clip.max[1], (y + 10.0) * ppp));
        }
    }

    #[test]
    fn a_freed_texture_maps_fresh_when_set_again() {
        let source = layer(vec![
            Some(frame(1.0, &[0], vec![mesh(0, [0.0, 0.0])], &[])),
            Some(frame(1.0, &[], vec![], &[0])),
            Some(frame(1.0, &[0], vec![mesh(0, [0.0, 0.0])], &[])),
        ]);
        let mut layers = UiLayers::new(vec![source]);

        let first = layers.ui_frame(SIZE).expect("first frame");
        let freed = layers.ui_frame(SIZE).expect("second frame");
        let again = layers.ui_frame(SIZE).expect("third frame");

        assert_eq!(first.textures_set.len(), 1);
        assert_eq!(again.textures_set.len(), 1);
        assert_eq!(again.meshes.len(), 1);
        let first_id = first.textures_set[0].id;
        assert_eq!(freed.textures_free, vec![first_id]);
        let again_id = again.textures_set[0].id;
        assert_ne!(again_id, first_id);
        assert_eq!(again.meshes[0].texture, again_id);
    }

    #[test]
    fn all_layers_empty_yields_no_frame() {
        let a = layer(vec![None]);
        let b = layer(vec![None]);

        assert_eq!(merged(vec![a, b]), None);
        assert_eq!(merged(vec![]), None);
    }

    #[test]
    fn a_layer_returning_none_is_skipped() {
        let silent = layer(vec![None]);
        let drawing = layer(vec![Some(frame(1.0, &[0], vec![mesh(0, [4.0, 4.0])], &[]))]);

        let out = merged(vec![silent, drawing]).expect("one layer drew");

        assert_eq!(out.meshes.len(), 1);
        assert_eq!(out.meshes[0].vertices[0].pos, [4.0, 4.0]);
    }

    #[test]
    fn a_poisoned_layer_is_skipped() {
        let poisoned = Arc::new(Mutex::new(Scripted(VecDeque::new())));
        let held = poisoned.clone();
        let _ = std::thread::spawn(move || {
            let _guard = held.lock().unwrap();
            panic!("poisoning the layer on purpose");
        })
        .join();
        let drawing = layer(vec![Some(frame(1.0, &[0], vec![mesh(0, [4.0, 4.0])], &[]))]);

        let out = merged(vec![poisoned, drawing]).expect("the healthy layer drew");

        assert_eq!(out.meshes.len(), 1);
    }
}
