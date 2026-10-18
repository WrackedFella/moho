//! Translucent box markers a game places over the world.

use bytemuck::{Pod, Zeroable};

/// Stable, game-chosen key for one marker.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct MarkerId(pub u64);

/// How a marker's box is drawn.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash)]
pub enum MarkerStyle {
    /// A translucent solid box.
    #[default]
    Fill,
    /// The box's twelve edges only.
    Outline,
}

/// An axis-aligned box in world space with a colour.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Marker {
    pub min: [f32; 3],
    pub max: [f32; 3],
    /// Linear RGBA, straight (not premultiplied) alpha.
    pub color: [f32; 4],
    pub style: MarkerStyle,
}

impl Marker {
    /// A [`MarkerStyle::Fill`] marker spanning the two corners, ordered per axis so `min <= max`.
    pub fn new(a: [f32; 3], b: [f32; 3], color: [f32; 4]) -> Self {
        Self {
            min: std::array::from_fn(|i| a[i].min(b[i])),
            max: std::array::from_fn(|i| a[i].max(b[i])),
            color,
            style: MarkerStyle::Fill,
        }
    }

    /// This marker drawn in `style`.
    pub fn with_style(self, style: MarkerStyle) -> Self {
        Self { style, ..self }
    }
}

/// GPU-visible marker instance: two box corners and a colour, each padded to a
/// vec4 (the `w` of a corner is unused).
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct MarkerGpu {
    pub min: [f32; 4],
    pub max: [f32; 4],
    pub color: [f32; 4],
}

const _: () = assert!(std::mem::size_of::<MarkerGpu>() == 48);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_orders_corners_per_axis() {
        let color = [0.2, 0.4, 0.6, 0.5];

        let marker = Marker::new([3.0, -1.0, 5.0], [1.0, 2.0, 4.0], color);

        assert_eq!(marker.min, [1.0, -1.0, 4.0]);
        assert_eq!(marker.max, [3.0, 2.0, 5.0]);
        assert_eq!(marker.color, color);
    }

    #[test]
    fn new_keeps_ordered_corners_as_given() {
        let marker = Marker::new([0.0, 1.0, 2.0], [3.0, 4.0, 5.0], [1.0; 4]);

        assert_eq!(marker.min, [0.0, 1.0, 2.0]);
        assert_eq!(marker.max, [3.0, 4.0, 5.0]);
    }

    #[test]
    fn new_marker_is_filled() {
        let marker = Marker::new([0.0; 3], [1.0; 3], [1.0; 4]);

        assert_eq!(marker.style, MarkerStyle::Fill);
    }

    #[test]
    fn with_style_outline_sets_style_and_keeps_other_fields() {
        let color = [0.1, 0.2, 0.3, 0.4];
        let filled = Marker::new([3.0, 1.0, 2.0], [0.0, 4.0, 5.0], color);

        let outlined = filled.with_style(MarkerStyle::Outline);

        assert_eq!(outlined.style, MarkerStyle::Outline);
        assert_eq!(outlined.min, filled.min);
        assert_eq!(outlined.max, filled.max);
        assert_eq!(outlined.color, color);
    }

    #[test]
    fn with_style_fill_on_outline_marker_returns_fill() {
        let outlined = Marker::new([0.0; 3], [1.0; 3], [1.0; 4]).with_style(MarkerStyle::Outline);

        let filled = outlined.with_style(MarkerStyle::Fill);

        assert_eq!(filled.style, MarkerStyle::Fill);
    }
}
