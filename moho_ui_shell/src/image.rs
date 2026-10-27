//! Game-drawn screen images: pictures a game uploads once and positions each frame, the image
//! counterpart of [`ScreenText`](crate::text::ScreenText).

use crate::text::Anchor;

/// Game-chosen key for one uploaded image.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ScreenImageId(pub u64);

/// One uploaded image drawn over the game for a single frame, stretched to `size`.
///
/// `offset` and `size` are in logical points on screen axes (x right, y down); `offset` is from
/// the anchor point, and the image's matching point sits there, so it grows away from the screen
/// edge like [`ScreenText`](crate::text::ScreenText).
#[derive(Clone, Debug, PartialEq)]
pub struct ScreenImage {
    pub image: ScreenImageId,
    pub anchor: Anchor,
    pub offset: [f32; 2],
    pub size: [f32; 2],
}

/// Why `UiShell::set_images` refused a list.
#[derive(Debug, thiserror::Error)]
pub enum ScreenImageError {
    #[error("screen image {index} names image {id:?}, which was never added or was removed")]
    UnknownImage { index: usize, id: ScreenImageId },
    #[error("screen image {index} has size {size:?}; each side must be finite and positive")]
    BadSize { index: usize, size: [f32; 2] },
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use moho_render_api::{Rgba8Image, UiFrame, UiFrameSource, UiTextureId};

    use super::*;
    use crate::console::Console;
    use crate::console::tests::Cmd;
    use crate::host::UiShell;
    use crate::text::ScreenText;

    const ICON: ScreenImageId = ScreenImageId(7);
    const SCREEN: [u32; 2] = [800, 600];

    /// Opaque red, opaque green, opaque blue and a half-transparent orange.
    const PIXELS: [[u8; 4]; 4] = [
        [255, 0, 0, 255],
        [0, 255, 0, 255],
        [0, 0, 255, 255],
        [200, 100, 50, 128],
    ];

    const ANCHORS: [Anchor; 9] = [
        Anchor::TopLeft,
        Anchor::Top,
        Anchor::TopRight,
        Anchor::Left,
        Anchor::Center,
        Anchor::Right,
        Anchor::BottomLeft,
        Anchor::Bottom,
        Anchor::BottomRight,
    ];

    fn shell() -> UiShell<Cmd, ()> {
        let (console, _) = Console::new(&BTreeMap::new());
        UiShell::new(console)
    }

    fn image(size: [u32; 2], pixels: &[[u8; 4]]) -> Rgba8Image {
        Rgba8Image::new(size, pixels.to_vec()).expect("pixel count matches the size")
    }

    fn icon() -> Rgba8Image {
        image([2, 2], &PIXELS)
    }

    fn screen_image(anchor: Anchor, offset: [f32; 2], size: [f32; 2]) -> ScreenImage {
        ScreenImage {
            image: ICON,
            anchor,
            offset,
            size,
        }
    }

    fn label() -> ScreenText {
        ScreenText {
            text: "30 / 30".to_owned(),
            anchor: Anchor::TopLeft,
            offset: [0.0, 0.0],
            size: 24.0,
            color: [255, 255, 255, 255],
        }
    }

    /// The texture a frame uploaded with exactly `size` pixels, which tells a game image from
    /// egui's font atlas.
    fn uploaded(frame: &UiFrame, size: [u32; 2]) -> Option<(UiTextureId, Vec<[u8; 4]>)> {
        frame
            .textures_set
            .iter()
            .find(|set| set.image.size == size && set.pos.is_none())
            .map(|set| (set.id, set.image.pixels.clone()))
    }

    /// The bounding box of the vertices of every mesh on `texture`, as `(min, max)`.
    fn covered(frame: &UiFrame, texture: UiTextureId) -> ([f32; 2], [f32; 2]) {
        let vertices: Vec<_> = frame
            .meshes
            .iter()
            .filter(|m| m.texture == texture)
            .flat_map(|m| m.vertices.iter())
            .collect();
        assert!(!vertices.is_empty(), "no mesh samples {texture:?}");
        let min = vertices.iter().fold([f32::MAX; 2], |acc, v| {
            [acc[0].min(v.pos[0]), acc[1].min(v.pos[1])]
        });
        let max = vertices.iter().fold([f32::MIN; 2], |acc, v| {
            [acc[0].max(v.pos[0]), acc[1].max(v.pos[1])]
        });
        (min, max)
    }

    fn assert_near(actual: [f32; 2], expected: [f32; 2], context: impl std::fmt::Debug) {
        assert!(
            (actual[0] - expected[0]).abs() <= 0.5 && (actual[1] - expected[1]).abs() <= 0.5,
            "{context:?}: {actual:?} is not near {expected:?}"
        );
    }

    /// The texture coordinate of the vertex on `texture` nearest to `point`, which must be
    /// within half a point of it.
    fn uv_at(frame: &UiFrame, texture: UiTextureId, point: [f32; 2]) -> [f32; 2] {
        let distance = |pos: [f32; 2]| (pos[0] - point[0]).hypot(pos[1] - point[1]);
        let nearest = frame
            .meshes
            .iter()
            .filter(|m| m.texture == texture)
            .flat_map(|m| m.vertices.iter())
            .min_by(|a, b| distance(a.pos).total_cmp(&distance(b.pos)))
            .expect("a vertex on the texture");
        assert_near(nearest.pos, point, "nearest vertex");
        nearest.uv
    }

    fn point_of(min: [f32; 2], max: [f32; 2], anchor: Anchor) -> [f32; 2] {
        let mid = |a: f32, b: f32| f32::midpoint(a, b);
        match anchor {
            Anchor::TopLeft => [min[0], min[1]],
            Anchor::Top => [mid(min[0], max[0]), min[1]],
            Anchor::TopRight => [max[0], min[1]],
            Anchor::Left => [min[0], mid(min[1], max[1])],
            Anchor::Center => [mid(min[0], max[0]), mid(min[1], max[1])],
            Anchor::Right => [max[0], mid(min[1], max[1])],
            Anchor::BottomLeft => [min[0], max[1]],
            Anchor::Bottom => [mid(min[0], max[0]), max[1]],
            Anchor::BottomRight => [max[0], max[1]],
        }
    }

    #[test]
    fn a_screen_image_paints_its_texture_premultiplied_in_its_rect() {
        let mut shell = shell();
        shell.add_image(ICON, &icon());
        shell
            .set_images(vec![screen_image(
                Anchor::BottomRight,
                [-10.0, -12.0],
                [64.0, 32.0],
            )])
            .expect("a known image with a valid size");

        let frame = shell
            .ui_frame(SCREEN)
            .expect("an image alone draws a frame");

        let (texture, pixels) = uploaded(&frame, [2, 2]).expect("the image is uploaded");
        // Straight [200, 100, 50] at alpha 128 premultiplied in gamma space with rounding:
        // 200 * 128 / 255 = 100.4, 100 * 128 / 255 = 50.2, 50 * 128 / 255 = 25.1.
        assert_eq!(
            pixels,
            vec![
                [255, 0, 0, 255],
                [0, 255, 0, 255],
                [0, 0, 255, 255],
                [100, 50, 25, 128],
            ],
            "the pass expects premultiplied pixels"
        );
        let (min, max) = covered(&frame, texture);
        assert_near(min, [726.0, 556.0], "rect min");
        assert_near(max, [790.0, 588.0], "rect max");
        let corner = |point| uv_at(&frame, texture, point);
        assert_eq!(corner([min[0], min[1]]), [0.0, 0.0], "top left");
        assert_eq!(corner([max[0], min[1]]), [1.0, 0.0], "top right");
        assert_eq!(corner([min[0], max[1]]), [0.0, 1.0], "bottom left");
        assert_eq!(corner([max[0], max[1]]), [1.0, 1.0], "bottom right");
    }

    #[test]
    fn every_anchor_places_the_images_matching_point_at_the_anchor_plus_offset() {
        let screen = [800.0, 600.0];
        for anchor in ANCHORS {
            let mut shell = shell();
            shell.add_image(ICON, &image([4, 2], &[[9, 9, 9, 255]; 8]));
            shell
                .set_images(vec![screen_image(anchor, [5.0, -7.0], [40.0, 20.0])])
                .expect("valid image");

            let frame = shell.ui_frame(SCREEN).expect("an image draws a frame");

            let (texture, _) = uploaded(&frame, [4, 2]).expect("the image is uploaded");
            let (min, max) = covered(&frame, texture);
            let anchor_point = point_of([0.0, 0.0], screen, anchor);
            assert_near(
                point_of(min, max, anchor),
                [anchor_point[0] + 5.0, anchor_point[1] - 7.0],
                (anchor, "matching point"),
            );
            assert_near(
                [max[0] - min[0], max[1] - min[1]],
                [40.0, 20.0],
                (anchor, "size"),
            );
        }
    }

    #[test]
    fn an_added_image_is_uploaded_once_across_frames() {
        let mut shell = shell();
        shell.add_image(ICON, &icon());
        let placed = || vec![screen_image(Anchor::Center, [0.0, 0.0], [32.0, 32.0])];
        shell.set_images(placed()).expect("valid image");
        let first = shell.ui_frame(SCREEN).expect("draws");
        let (texture, _) = uploaded(&first, [2, 2]).expect("uploaded on first use");

        shell.set_images(placed()).expect("valid image");
        let second = shell.ui_frame(SCREEN).expect("draws");

        assert!(uploaded(&second, [2, 2]).is_none(), "not sent again");
        assert_near(
            covered(&second, texture).1,
            covered(&first, texture).1,
            "second frame",
        );
    }

    #[test]
    fn images_are_cleared_by_the_frame_that_paints_them() {
        let mut shell = shell();
        shell.add_image(ICON, &icon());
        shell
            .set_images(vec![screen_image(Anchor::Center, [0.0, 0.0], [32.0, 32.0])])
            .expect("valid image");
        assert!(shell.ui_frame(SCREEN).is_some());

        let next = shell.ui_frame(SCREEN);

        assert_eq!(next, None);
    }

    #[test]
    fn images_paint_before_text_and_the_console() {
        let mut shell = shell();
        shell.add_image(ICON, &icon());
        shell.console_mut().open();
        shell.set_texts(vec![label()]).expect("valid text");
        shell
            .set_images(vec![screen_image(
                Anchor::TopLeft,
                [0.0, 0.0],
                [32.0, 32.0],
            )])
            .expect("valid image");

        let frame = shell.ui_frame(SCREEN).expect("draws");

        let (texture, _) = uploaded(&frame, [2, 2]).expect("the image is uploaded");
        assert_eq!(
            frame.meshes[0].texture, texture,
            "the image is the first mesh"
        );
        assert!(frame.meshes.len() > 1, "text and console paint after it");
        assert!(frame.meshes[1..].iter().all(|m| m.texture != texture));
    }

    #[test]
    fn set_images_refuses_an_unknown_image() {
        let mut shell = shell();
        shell.add_image(ICON, &icon());
        let known = screen_image(Anchor::Center, [0.0, 0.0], [32.0, 32.0]);
        let unknown = ScreenImage {
            image: ScreenImageId(99),
            ..known.clone()
        };

        let refused = shell.set_images(vec![known, unknown]);

        assert!(
            matches!(
                refused,
                Err(ScreenImageError::UnknownImage {
                    index: 1,
                    id: ScreenImageId(99)
                })
            ),
            "{refused:?}"
        );
        assert_eq!(
            shell.ui_frame(SCREEN),
            None,
            "a refused list leaves no images"
        );
    }

    #[test]
    fn set_images_refuses_a_size_that_is_not_finite_and_positive() {
        let bad_sizes = [
            [0.0, 10.0],
            [10.0, 0.0],
            [-1.0, 10.0],
            [10.0, -1.0],
            [f32::NAN, 10.0],
            [10.0, f32::NAN],
            [f32::INFINITY, 10.0],
            [10.0, f32::NEG_INFINITY],
        ];
        for size in bad_sizes {
            let mut shell = shell();
            shell.add_image(ICON, &icon());
            let good = screen_image(Anchor::Center, [0.0, 0.0], [32.0, 32.0]);
            let bad = ScreenImage {
                size,
                ..good.clone()
            };

            let refused = shell.set_images(vec![good, bad]);

            assert!(
                matches!(
                    refused,
                    Err(ScreenImageError::BadSize { index: 1, size: s })
                        if s.map(f32::to_bits) == size.map(f32::to_bits)
                ),
                "size {size:?}: {refused:?}"
            );
            assert_eq!(shell.ui_frame(SCREEN), None, "size {size:?}");
        }
    }

    #[test]
    fn the_smallest_positive_size_is_accepted() {
        let mut shell = shell();
        shell.add_image(ICON, &icon());
        let tiny = screen_image(Anchor::Center, [0.0, 0.0], [f32::MIN_POSITIVE; 2]);

        let accepted = shell.set_images(vec![tiny]);

        assert!(accepted.is_ok(), "{accepted:?}");
    }

    #[test]
    fn removing_an_image_frees_its_texture() {
        let mut shell = shell();
        shell.add_image(ICON, &icon());
        shell
            .set_images(vec![screen_image(Anchor::Center, [0.0, 0.0], [32.0, 32.0])])
            .expect("valid image");
        let first = shell.ui_frame(SCREEN).expect("draws");
        let (texture, _) = uploaded(&first, [2, 2]).expect("uploaded");

        shell.remove_image(ICON);
        shell.set_texts(vec![label()]).expect("valid text");
        let next = shell.ui_frame(SCREEN).expect("the text draws a frame");

        assert!(
            next.textures_free.contains(&texture),
            "{:?}",
            next.textures_free
        );
        let refused = shell.set_images(vec![screen_image(Anchor::Center, [0.0, 0.0], [8.0, 8.0])]);
        assert!(
            matches!(
                refused,
                Err(ScreenImageError::UnknownImage { index: 0, id: ICON })
            ),
            "{refused:?}"
        );
    }

    #[test]
    fn removing_an_image_delivers_the_free_next_frame_even_with_nothing_else_on_screen() {
        let mut shell = shell();
        shell.add_image(ICON, &icon());
        shell
            .set_images(vec![screen_image(Anchor::Center, [0.0, 0.0], [32.0, 32.0])])
            .expect("valid image");
        let first = shell.ui_frame(SCREEN).expect("draws");
        let (texture, _) = uploaded(&first, [2, 2]).expect("uploaded");

        shell.remove_image(ICON);
        let next = shell
            .ui_frame(SCREEN)
            .expect("a pending free still produces a frame");

        assert!(
            next.textures_free.contains(&texture),
            "{:?}",
            next.textures_free
        );
        assert_eq!(shell.ui_frame(SCREEN), None, "the free is delivered once");
    }

    #[test]
    fn replacing_an_image_frees_the_old_texture_and_uploads_the_new() {
        let mut shell = shell();
        shell.add_image(ICON, &icon());
        shell
            .set_images(vec![screen_image(Anchor::Center, [0.0, 0.0], [32.0, 32.0])])
            .expect("valid image");
        let first = shell.ui_frame(SCREEN).expect("draws");
        let (old, _) = uploaded(&first, [2, 2]).expect("uploaded");

        shell.add_image(ICON, &image([3, 1], &[[1, 2, 3, 255]; 3]));
        shell
            .set_images(vec![screen_image(Anchor::Center, [0.0, 0.0], [32.0, 32.0])])
            .expect("valid image");
        let next = shell.ui_frame(SCREEN).expect("draws");

        let (new, _) = uploaded(&next, [3, 1]).expect("the replacement is uploaded");
        assert_ne!(new, old);
        assert!(
            next.textures_free.contains(&old),
            "{:?}",
            next.textures_free
        );
        assert!(next.meshes.iter().any(|m| m.texture == new));
        assert!(next.meshes.iter().all(|m| m.texture != old));
    }
}
