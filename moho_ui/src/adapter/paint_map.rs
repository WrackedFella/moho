//! Pure mapping from egui tessellation output to the render-api UI paint contract.

use moho_render_api::{UiFrame, UiTextureId};

/// Maps an egui texture id to a game-assigned id. Injective: `Managed(n)` and
/// `User(n)` never collide.
pub(crate) fn to_ui_texture_id(_id: egui::TextureId) -> UiTextureId {
    todo!("tag User ids so they cannot collide with Managed ids")
}

/// Maps tessellated egui output to a [`UiFrame`]. Paint callbacks are skipped.
pub(crate) fn to_ui_frame(
    _primitives: &[egui::ClippedPrimitive],
    _textures_delta: &egui::TexturesDelta,
    _pixels_per_point: f32,
) -> UiFrame {
    todo!("map egui primitives and texture deltas to UiFrame")
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::epaint::{Mesh, Vertex};
    use egui::{Color32, ColorImage, ImageData, TextureFilter, TextureWrapMode};
    use moho_render_api::{UiFilter, UiRect, UiVertex, UiWrap};
    use std::sync::Arc;

    fn vertex(x: f32, y: f32, u: f32, v: f32, c: Color32) -> Vertex {
        Vertex {
            pos: egui::pos2(x, y),
            uv: egui::pos2(u, v),
            color: c,
        }
    }

    fn mesh_primitive(texture_id: egui::TextureId) -> egui::ClippedPrimitive {
        egui::ClippedPrimitive {
            clip_rect: egui::Rect::from_min_max(egui::pos2(1.0, 2.0), egui::pos2(30.0, 40.0)),
            primitive: egui::epaint::Primitive::Mesh(Mesh {
                indices: vec![0, 1, 2, 2, 1, 3],
                vertices: vec![
                    vertex(
                        0.5,
                        1.5,
                        0.0,
                        0.0,
                        Color32::from_rgba_premultiplied(10, 20, 30, 40),
                    ),
                    vertex(
                        9.0,
                        1.5,
                        1.0,
                        0.0,
                        Color32::from_rgba_premultiplied(1, 2, 3, 4),
                    ),
                    vertex(
                        0.5,
                        7.0,
                        0.0,
                        1.0,
                        Color32::from_rgba_premultiplied(5, 6, 7, 8),
                    ),
                    vertex(
                        9.0,
                        7.0,
                        1.0,
                        1.0,
                        Color32::from_rgba_premultiplied(250, 251, 252, 255),
                    ),
                ],
                texture_id,
            }),
        }
    }

    fn delta(
        pos: Option<[usize; 2]>,
        size: [usize; 2],
        px: Vec<Color32>,
        options: egui::TextureOptions,
    ) -> egui::epaint::ImageDelta {
        egui::epaint::ImageDelta {
            image: ImageData::Color(Arc::new(ColorImage::new(size, px))),
            options,
            pos,
        }
    }

    #[test]
    fn mesh_clip_and_texture_delta_survive_mapping() {
        let tex = egui::TextureId::Managed(7);
        let primitives = vec![mesh_primitive(tex)];
        let full_px = vec![
            Color32::from_rgba_premultiplied(1, 2, 3, 4),
            Color32::from_rgba_premultiplied(5, 6, 7, 8),
        ];
        let patch_px = vec![Color32::from_rgba_premultiplied(9, 10, 11, 12)];
        let textures_delta = egui::TexturesDelta {
            set: vec![
                (
                    egui::TextureId::Managed(7),
                    delta(None, [2, 1], full_px, egui::TextureOptions::LINEAR),
                ),
                (
                    egui::TextureId::Managed(8),
                    delta(
                        Some([3, 4]),
                        [1, 1],
                        patch_px,
                        egui::TextureOptions {
                            magnification: TextureFilter::Nearest,
                            minification: TextureFilter::Linear,
                            wrap_mode: TextureWrapMode::Repeat,
                            mipmap_mode: None,
                        },
                    ),
                ),
            ],
            free: vec![egui::TextureId::Managed(3), egui::TextureId::User(3)],
        };

        let frame = to_ui_frame(&primitives, &textures_delta, 1.5);

        assert_eq!(frame.pixels_per_point, 1.5);
        assert_eq!(frame.meshes.len(), 1);
        let mesh = &frame.meshes[0];
        assert_eq!(mesh.indices, vec![0, 1, 2, 2, 1, 3]);
        assert_eq!(
            mesh.clip_rect,
            UiRect {
                min: [1.0, 2.0],
                max: [30.0, 40.0]
            }
        );
        assert_eq!(mesh.texture, to_ui_texture_id(tex));
        assert_eq!(
            mesh.vertices,
            vec![
                UiVertex {
                    pos: [0.5, 1.5],
                    uv: [0.0, 0.0],
                    color: [10, 20, 30, 40]
                },
                UiVertex {
                    pos: [9.0, 1.5],
                    uv: [1.0, 0.0],
                    color: [1, 2, 3, 4]
                },
                UiVertex {
                    pos: [0.5, 7.0],
                    uv: [0.0, 1.0],
                    color: [5, 6, 7, 8]
                },
                UiVertex {
                    pos: [9.0, 7.0],
                    uv: [1.0, 1.0],
                    color: [250, 251, 252, 255]
                },
            ]
        );

        assert_eq!(frame.textures_set.len(), 2);
        let full = &frame.textures_set[0];
        assert_eq!(full.id, to_ui_texture_id(egui::TextureId::Managed(7)));
        assert_eq!(full.id, mesh.texture);
        assert_eq!(full.pos, None);
        assert_eq!(full.image.size, [2, 1]);
        assert_eq!(full.image.pixels, vec![[1, 2, 3, 4], [5, 6, 7, 8]]);
        assert_eq!(full.sampler.magnification, UiFilter::Linear);
        assert_eq!(full.sampler.minification, UiFilter::Linear);
        assert_eq!(full.sampler.wrap, UiWrap::ClampToEdge);

        let patch = &frame.textures_set[1];
        assert_eq!(patch.id, to_ui_texture_id(egui::TextureId::Managed(8)));
        assert_eq!(patch.pos, Some([3, 4]));
        assert_eq!(patch.image.size, [1, 1]);
        assert_eq!(patch.image.pixels, vec![[9, 10, 11, 12]]);
        assert_eq!(patch.sampler.magnification, UiFilter::Nearest);
        assert_eq!(patch.sampler.minification, UiFilter::Linear);
        assert_eq!(patch.sampler.wrap, UiWrap::Repeat);

        assert_eq!(
            frame.textures_free,
            vec![
                to_ui_texture_id(egui::TextureId::Managed(3)),
                to_ui_texture_id(egui::TextureId::User(3)),
            ]
        );
    }

    #[test]
    fn managed_and_user_ids_map_to_distinct_texture_ids() {
        let managed = to_ui_texture_id(egui::TextureId::Managed(5));
        let user = to_ui_texture_id(egui::TextureId::User(5));

        assert_ne!(managed, user);
        assert_eq!(managed, to_ui_texture_id(egui::TextureId::Managed(5)));
        assert_ne!(managed, to_ui_texture_id(egui::TextureId::Managed(6)));
    }

    #[test]
    fn mirrored_repeat_wrap_maps_to_mirrored_repeat() {
        let textures_delta = egui::TexturesDelta {
            set: vec![(
                egui::TextureId::Managed(1),
                delta(
                    None,
                    [1, 1],
                    vec![Color32::WHITE],
                    egui::TextureOptions {
                        magnification: TextureFilter::Linear,
                        minification: TextureFilter::Nearest,
                        wrap_mode: TextureWrapMode::MirroredRepeat,
                        mipmap_mode: None,
                    },
                ),
            )],
            free: vec![],
        };

        let frame = to_ui_frame(&[], &textures_delta, 1.0);

        let sampler = frame.textures_set[0].sampler;
        assert_eq!(sampler.wrap, UiWrap::MirroredRepeat);
        assert_eq!(sampler.magnification, UiFilter::Linear);
        assert_eq!(sampler.minification, UiFilter::Nearest);
    }

    #[test]
    fn paint_callbacks_are_skipped() {
        let callback = egui::ClippedPrimitive {
            clip_rect: egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(5.0, 5.0)),
            primitive: egui::epaint::Primitive::Callback(egui::epaint::PaintCallback {
                rect: egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(5.0, 5.0)),
                callback: Arc::new(()),
            }),
        };
        let primitives = vec![callback, mesh_primitive(egui::TextureId::Managed(0))];

        let frame = to_ui_frame(&primitives, &egui::TexturesDelta::default(), 1.0);

        assert_eq!(frame.meshes.len(), 1);
        assert_eq!(frame.meshes[0].indices.len(), 6);
    }
}
