//! Contact ambient occlusion for smooth terrain: how enclosed an air voxel is by solid
//! voxels around it.

use crate::grid::BlockPos;

/// Solid neighbours, out of the 26 around an air voxel, up to which it reads fully
/// unoccluded. A planar surface of any slope touches at most this many.
pub(crate) const UNOCCLUDED_NEIGHBOURS: u32 = 12;

/// Solid neighbours from which an air voxel reads as dark as it gets.
pub(crate) const FULLY_OCCLUDED_NEIGHBOURS: u32 = 20;

/// The ambient occlusion factor of a voxel with at least [`FULLY_OCCLUDED_NEIGHBOURS`]
/// solid neighbours; 1.0 is unoccluded.
pub(crate) const DARKEST_AO: f32 = 0.4;

/// The ambient occlusion factor (1.0 unoccluded, [`DARKEST_AO`] darkest) of the air voxel
/// `air`, from how many of its 26 neighbours `solid_at` reports solid. The voxel itself
/// is not counted. Falls linearly from 1.0 at [`UNOCCLUDED_NEIGHBOURS`] to
/// [`DARKEST_AO`] at [`FULLY_OCCLUDED_NEIGHBOURS`].
pub(crate) fn contact_ao(_solid_at: impl Fn(BlockPos) -> bool, _air: BlockPos) -> f32 {
    1.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::IVec3;
    use proptest::prelude::*;
    use std::collections::HashSet;

    const AIR: IVec3 = IVec3::new(5, -3, 7);

    /// The 26 offsets to the neighbours of a voxel, in a fixed order.
    fn neighbour_offsets() -> Vec<IVec3> {
        let mut offsets = Vec::new();
        for x in -1..=1 {
            for y in -1..=1 {
                for z in -1..=1 {
                    if (x, y, z) != (0, 0, 0) {
                        offsets.push(IVec3::new(x, y, z));
                    }
                }
            }
        }
        offsets
    }

    /// The AO of [`AIR`] with exactly the neighbours at `offsets` solid.
    fn ao_with_solid(offsets: &[IVec3]) -> f32 {
        let solid: HashSet<IVec3> = offsets.iter().map(|&o| AIR + o).collect();
        contact_ao(|p| solid.contains(&p), AIR)
    }

    fn assert_close(actual: f32, expected: f32, why: &str) {
        assert!(
            (actual - expected).abs() < 1e-6,
            "{why}: expected {expected}, got {actual}"
        );
    }

    #[test]
    fn twelve_neighbours_read_one() {
        let offsets = neighbour_offsets();

        let first_twelve = ao_with_solid(&offsets[..12]);
        let last_twelve = ao_with_solid(&offsets[14..]);
        let none = ao_with_solid(&[]);
        let thirteen = ao_with_solid(&offsets[..13]);

        assert_eq!(first_twelve, 1.0);
        assert_eq!(last_twelve, 1.0);
        assert_eq!(none, 1.0);
        assert_close(thirteen, 0.925, "13 neighbours");
    }

    #[test]
    fn twenty_or_more_read_the_floor() {
        let offsets = neighbour_offsets();

        for count in [20, 21, 23, 26] {
            assert_close(
                ao_with_solid(&offsets[..count]),
                0.4,
                &format!("{count} neighbours"),
            );
        }
        assert_close(ao_with_solid(&offsets[6..]), 0.4, "the last twenty");
    }

    #[test]
    fn sixteen_neighbours_read_the_midpoint() {
        let offsets = neighbour_offsets();

        assert_close(ao_with_solid(&offsets[..16]), 0.7, "first sixteen");
        assert_close(ao_with_solid(&offsets[10..]), 0.7, "last sixteen");
        assert_close(ao_with_solid(&offsets[..19]), 0.475, "nineteen");
    }

    #[test]
    fn centre_voxel_is_not_counted() {
        let offsets = neighbour_offsets();
        let mut twelve = offsets[..12].to_vec();
        twelve.push(IVec3::ZERO);
        let mut twenty = offsets[..20].to_vec();
        twenty.push(IVec3::ZERO);

        let twelve_and_centre = ao_with_solid(&twelve);
        let twenty_and_centre = ao_with_solid(&twenty);
        let nineteen_and_centre = {
            let mut nineteen = offsets[..19].to_vec();
            nineteen.push(IVec3::ZERO);
            ao_with_solid(&nineteen)
        };

        assert_eq!(twelve_and_centre, 1.0);
        assert_close(twenty_and_centre, 0.4, "twenty and the centre");
        assert_close(nineteen_and_centre, 0.475, "nineteen and the centre");
    }

    #[test]
    fn voxels_beyond_the_ring_are_not_counted() {
        let offsets = neighbour_offsets();
        let solid: HashSet<IVec3> = offsets[..12]
            .iter()
            .map(|&o| AIR + o)
            .chain([AIR + IVec3::new(2, 0, 0), AIR + IVec3::new(0, -2, 1)])
            .collect();

        let ao = contact_ao(|p| solid.contains(&p), AIR);

        assert_eq!(ao, 1.0);
    }

    /// Planes `a*x + b*y + c*z <= d` are solid: flat, 1:1, 2:1, 1:2 and diagonal.
    #[test]
    fn planar_slopes_read_one() {
        let planes: [([i32; 3], i32, &str); 8] = [
            ([0, 1, 0], 0, "flat"),
            ([-1, 1, 0], 0, "1:1 ramp"),
            ([-1, 2, 0], 0, "2:1 ramp"),
            ([-2, 1, 0], 0, "1:2 ramp"),
            ([0, 1, -1], 1, "1:1 ramp along z"),
            ([1, 1, 1], 0, "diagonal plane"),
            ([1, -1, 1], 0, "tilted diagonal plane"),
            ([-1, -2, 2], 1, "oblique plane"),
        ];

        for (normal, offset, name) in planes {
            let normal = IVec3::from(normal);
            let solid_at = |p: IVec3| normal.dot(p) <= offset;
            let mut checked = 0;
            for x in -4..=4 {
                for y in -4..=4 {
                    for z in -4..=4 {
                        let air = IVec3::new(x, y, z);
                        if solid_at(air) {
                            continue;
                        }
                        checked += 1;

                        let ao = contact_ao(solid_at, air);

                        assert_eq!(ao, 1.0, "{name}: air voxel {air}");
                    }
                }
            }
            assert!(checked > 0, "{name}: some air voxels were checked");
        }
    }

    fn mask() -> impl Strategy<Value = Vec<bool>> {
        prop::collection::vec(any::<bool>(), 26)
    }

    fn selected(mask: &[bool]) -> Vec<IVec3> {
        neighbour_offsets()
            .into_iter()
            .zip(mask)
            .filter_map(|(offset, &on)| on.then_some(offset))
            .collect()
    }

    proptest! {
        #[test]
        fn prop_more_solid_neighbours_never_brighten(
            base in mask(),
            extra in mask(),
        ) {
            let superset_mask: Vec<bool> =
                base.iter().zip(&extra).map(|(a, b)| *a || *b).collect();
            let (few, many) = (selected(&base), selected(&superset_mask));

            let (ao_few, ao_many) = (ao_with_solid(&few), ao_with_solid(&many));

            prop_assert!((0.4 - 1e-6..=1.0).contains(&ao_few), "{} solid: {}", few.len(), ao_few);
            prop_assert!((0.4 - 1e-6..=1.0).contains(&ao_many), "{} solid: {}", many.len(), ao_many);
            prop_assert!(ao_many <= ao_few, "{} solid read {}, {} solid read {}",
                few.len(), ao_few, many.len(), ao_many);
            if many.len() > 12 && many.len() < 20 {
                prop_assert!(ao_many < 1.0, "{} solid read {}", many.len(), ao_many);
            }
            if many.len() <= 12 {
                prop_assert_eq!(ao_many, 1.0);
            }
            if many.len() >= 20 {
                prop_assert!((ao_many - 0.4).abs() < 1e-6, "{} solid read {}", many.len(), ao_many);
            }
        }
    }
}
