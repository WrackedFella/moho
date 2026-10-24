//! Contact ambient occlusion for smooth terrain, a pure function of the world's voxels.

use super::super::grid::BlockPos;
use glam::IVec3;

/// Solid neighbours, of the 26 around an air voxel, that still read unoccluded. An air voxel
/// beside a planar surface (flat or sloped) has at most this many, so open ground and ramps
/// stay at full brightness.
const UNOCCLUDED_NEIGHBOURS: usize = 12;

/// Brightness lost per solid neighbour past [`UNOCCLUDED_NEIGHBOURS`].
const AO_PER_NEIGHBOUR: f32 = 0.075;

/// Darkest contact AO, reached at 20 solid neighbours.
const AO_FLOOR: f32 = 0.4;

/// Contact ambient occlusion of the air voxel `air`: 1.0 up to [`UNOCCLUDED_NEIGHBOURS`]
/// solid voxels among its 26 neighbours, then [`AO_PER_NEIGHBOUR`] darker for each one more,
/// down to [`AO_FLOOR`]. The voxel itself is not counted.
pub(super) fn contact_ao(solid_at: impl Fn(BlockPos) -> bool, air: BlockPos) -> f32 {
    let solid = (-1..=1)
        .flat_map(|x| (-1..=1).flat_map(move |y| (-1..=1).map(move |z| IVec3::new(x, y, z))))
        .filter(|&offset| offset != IVec3::ZERO && solid_at(air + offset))
        .count();
    let excess = solid.saturating_sub(UNOCCLUDED_NEIGHBOURS);
    (1.0 - AO_PER_NEIGHBOUR * excess as f32).max(AO_FLOOR)
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::IVec3;
    use proptest::prelude::*;
    use std::collections::HashSet;

    /// The 26 offsets of the neighbours of a voxel, in a fixed order.
    fn ring() -> Vec<IVec3> {
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

    const AIR: IVec3 = IVec3::new(3, -4, 5);

    /// AO of `AIR` when its first `count` ring neighbours are solid and nothing else is.
    fn ao_with_first_neighbours_solid(count: usize) -> f32 {
        let solid: HashSet<IVec3> = ring().into_iter().take(count).map(|o| AIR + o).collect();

        contact_ao(|p| solid.contains(&p), AIR)
    }

    fn assert_close(actual: f32, expected: f32, why: &str) {
        assert!(
            (actual - expected).abs() < 1e-5,
            "{why}: got {actual}, expected {expected}"
        );
    }

    #[test]
    fn planar_slopes_read_one() {
        type Solid = fn(IVec3) -> bool;
        let slopes: [(&str, Solid); 4] = [
            ("1:1 ramp", |p| p.y <= p.x),
            ("2:1 ramp", |p| 2 * p.y <= p.x),
            ("1:2 ramp", |p| p.y <= 2 * p.x),
            ("diagonal plane", |p| p.x + p.y + p.z <= 0),
        ];

        for (name, solid) in slopes {
            let mut checked = 0;
            for x in -5..=5 {
                for y in -5..=5 {
                    for z in -5..=5 {
                        let air = IVec3::new(x, y, z);
                        if solid(air) {
                            continue;
                        }

                        let ao = contact_ao(solid, air);

                        assert_eq!(ao, 1.0, "{name}: air voxel {air}");
                        checked += 1;
                    }
                }
            }
            assert!(checked > 500, "{name}: air voxels were checked");
        }
    }

    #[test]
    fn twelve_neighbours_read_one() {
        assert_eq!(ao_with_first_neighbours_solid(0), 1.0);
        assert_eq!(ao_with_first_neighbours_solid(12), 1.0);
    }

    #[test]
    fn neighbours_past_twelve_darken_linearly_to_the_floor() {
        let rows = [(13, 0.925), (16, 0.7), (19, 0.475)];

        for (count, expected) in rows {
            assert_close(
                ao_with_first_neighbours_solid(count),
                expected,
                &format!("{count} solid neighbours"),
            );
        }
    }

    #[test]
    fn twenty_or_more_read_the_floor() {
        for count in [20, 23, 26] {
            assert_close(
                ao_with_first_neighbours_solid(count),
                0.4,
                &format!("{count} solid neighbours"),
            );
        }
    }

    #[test]
    fn centre_voxel_is_not_counted() {
        let twelve: HashSet<IVec3> = ring().into_iter().take(12).map(|o| AIR + o).collect();
        let nineteen: HashSet<IVec3> = ring().into_iter().take(19).map(|o| AIR + o).collect();

        let with_centre_twelve = contact_ao(|p| p == AIR || twelve.contains(&p), AIR);
        let with_centre_nineteen = contact_ao(|p| p == AIR || nineteen.contains(&p), AIR);

        assert_eq!(with_centre_twelve, 1.0, "12 neighbours plus the centre");
        assert_close(with_centre_nineteen, 0.475, "19 neighbours plus the centre");
    }

    #[test]
    fn voxels_beyond_the_ring_are_not_counted() {
        let beyond = contact_ao(|p| (p - AIR).abs().max_element() >= 2, AIR);

        assert_eq!(beyond, 1.0);
    }

    fn ao_for_mask(air: IVec3, mask: u32, centre: bool) -> f32 {
        let ring = ring();
        contact_ao(
            |p| {
                let offset = p - air;
                if offset == IVec3::ZERO {
                    return centre;
                }
                ring.iter()
                    .position(|&o| o == offset)
                    .is_some_and(|i| (mask >> i) & 1 == 1)
            },
            air,
        )
    }

    proptest! {
        #[test]
        fn prop_more_solid_neighbours_never_brighten(
            mask in 0u32..(1 << 26),
            extra in 0u32..26,
            centre in any::<bool>(),
            air in (-100..100i32, -100..100i32, -100..100i32),
        ) {
            let air = IVec3::new(air.0, air.1, air.2);
            let before = ao_for_mask(air, mask, centre);
            let after = ao_for_mask(air, mask | (1 << extra), centre);

            let count = mask.count_ones();
            prop_assert!((0.4 - 1e-6..=1.0).contains(&before), "ao {before} out of range");
            prop_assert!(after <= before, "adding a neighbour raised AO {before} -> {after}");
            if count <= 12 {
                prop_assert_eq!(before, 1.0, "{} solid neighbours", count);
            }
            if count >= 20 {
                prop_assert!((before - 0.4).abs() < 1e-5, "{count} solid neighbours: {before}");
            }
        }
    }
}
