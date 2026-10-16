//! Translucent box markers a game places over the world.

use bytemuck::{Pod, Zeroable};

/// Stable, game-chosen key for one marker.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct MarkerId(pub u64);

/// An axis-aligned box in world space with a translucent colour.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Marker {
    pub min: [f32; 3],
    pub max: [f32; 3],
    /// Linear RGBA, straight (not premultiplied) alpha.
    pub color: [f32; 4],
}

impl Marker {
    /// A marker spanning the two corners, ordered per axis so `min <= max`.
    pub fn new(a: [f32; 3], b: [f32; 3], color: [f32; 4]) -> Self {
        Self {
            min: std::array::from_fn(|i| a[i].min(b[i])),
            max: std::array::from_fn(|i| a[i].max(b[i])),
            color,
        }
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
}
