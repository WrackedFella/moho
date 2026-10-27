#[cfg(test)]
mod tests {
    use super::skin_vertex;
    use crate::types::{DrawKey, Vertex, VertexSource};
    use glam::{Affine3A, Quat, Vec3};
    use proptest::prelude::*;
    use std::mem::size_of;

    /// A joint matrix as the palette buffer holds it: three rows of `[m_r0, m_r1, m_r2, t_r]`.
    fn rows(joint: Affine3A) -> [[f32; 4]; 3] {
        let (x, y, z, t) = (
            joint.matrix3.x_axis,
            joint.matrix3.y_axis,
            joint.matrix3.z_axis,
            joint.translation,
        );
        [
            [x.x, y.x, z.x, t.x],
            [x.y, y.y, z.y, t.y],
            [x.z, y.z, z.z, t.z],
        ]
    }

    fn close(actual: impl Into<Vec3>, expected: Vec3, eps: f32) -> bool {
        actual.into().abs_diff_eq(expected, eps)
    }

    fn direction(v: [f32; 3]) -> Vec3 {
        Vec3::from(v).normalize()
    }

    fn any_vec3() -> impl Strategy<Value = Vec3> {
        (-10.0f32..10.0, -10.0f32..10.0, -10.0f32..10.0).prop_map(|(x, y, z)| Vec3::new(x, y, z))
    }

    fn any_unit_vec3() -> impl Strategy<Value = Vec3> {
        any_vec3()
            .prop_filter("a direction needs length", |v| v.length() > 0.1)
            .prop_map(Vec3::normalize)
    }

    /// Four non-negative weights summing to one.
    fn any_weights() -> impl Strategy<Value = [f32; 4]> {
        prop::array::uniform4(0.0f32..1.0)
            .prop_filter("weights need a total", |w| w.iter().sum::<f32>() > 0.1)
            .prop_map(|w| {
                let total: f32 = w.iter().sum();
                w.map(|x| x / total)
            })
    }

    #[test]
    fn a_vertex_bound_to_a_turned_joint_turns_and_others_stay() {
        let turn = Affine3A::from_quat(Quat::from_rotation_z(std::f32::consts::FRAC_PI_2));
        let palette = [rows(Affine3A::IDENTITY), rows(turn)];
        let (position, normal) = ([1.0, 0.0, 0.0], [1.0, 0.0, 0.0]);

        let turned = skin_vertex(
            position,
            normal,
            [1, 0, 0, 0],
            [1.0, 0.0, 0.0, 0.0],
            &palette,
        );
        let kept = skin_vertex(
            position,
            normal,
            [0, 0, 0, 0],
            [1.0, 0.0, 0.0, 0.0],
            &palette,
        );

        assert!(close(turned.0, Vec3::Y, 1e-5), "position {:?}", turned.0);
        assert!(
            close(direction(turned.1), Vec3::Y, 1e-5),
            "normal {:?}",
            turned.1
        );
        assert!(close(kept.0, Vec3::X, 1e-5), "position {:?}", kept.0);
        assert!(
            close(direction(kept.1), Vec3::X, 1e-5),
            "normal {:?}",
            kept.1
        );
    }

    #[test]
    fn a_joint_translation_moves_the_position_but_not_the_normal() {
        let palette = [rows(Affine3A::from_translation(Vec3::new(0.0, 5.0, -2.0)))];

        let (position, normal) = skin_vertex(
            [1.0, 1.0, 1.0],
            [0.0, 1.0, 0.0],
            [0, 0, 0, 0],
            [1.0, 0.0, 0.0, 0.0],
            &palette,
        );

        assert!(
            close(position, Vec3::new(1.0, 6.0, -1.0), 1e-5),
            "{position:?}"
        );
        assert!(close(direction(normal), Vec3::Y, 1e-5), "{normal:?}");
    }

    proptest! {
        #[test]
        fn the_identity_palette_leaves_every_vertex_unchanged(
            position in any_vec3(),
            normal in any_unit_vec3(),
            joints in prop::array::uniform4(0u16..4),
            weights in any_weights(),
        ) {
            let palette = [rows(Affine3A::IDENTITY); 4];

            let (skinned_position, skinned_normal) =
                skin_vertex(position.into(), normal.into(), joints, weights, &palette);

            prop_assert!(close(skinned_position, position, 1e-4), "{skinned_position:?} != {position:?}");
            prop_assert!(close(skinned_normal, normal, 1e-4), "{skinned_normal:?} != {normal:?}");
        }

        #[test]
        fn half_weights_on_two_joints_land_halfway(
            position in any_vec3(),
            normal in any_unit_vec3(),
            first in any_vec3(),
            second in any_vec3(),
            weight in 0.0f32..=1.0,
        ) {
            let palette = [
                rows(Affine3A::from_translation(second)),
                rows(Affine3A::from_translation(Vec3::splat(1000.0))),
                rows(Affine3A::from_translation(first)),
            ];

            let (skinned_position, skinned_normal) = skin_vertex(
                position.into(),
                normal.into(),
                [2, 0, 0, 0],
                [weight, 1.0 - weight, 0.0, 0.0],
                &palette,
            );

            let expected = position + first * weight + second * (1.0 - weight);
            prop_assert!(close(skinned_position, expected, 1e-3), "{skinned_position:?} != {expected:?}");
            prop_assert!(close(skinned_normal, normal, 1e-4), "{skinned_normal:?} != {normal:?}");
        }
    }

    #[test]
    fn skinned_normals_use_the_cofactor_of_the_blended_matrix() {
        let stretch = Affine3A::from_scale(Vec3::new(2.0, 1.0, 1.0));
        let palette = [rows(stretch), rows(Affine3A::IDENTITY)];
        let slanted = [1.0, 1.0, 0.0];

        let whole = skin_vertex(
            [1.0, 0.0, 0.0],
            slanted,
            [0, 0, 0, 0],
            [1.0, 0.0, 0.0, 0.0],
            &palette,
        );
        let blended = skin_vertex(
            [1.0, 0.0, 0.0],
            slanted,
            [0, 1, 0, 0],
            [0.5, 0.5, 0.0, 0.0],
            &palette,
        );

        assert!(
            close(whole.0, Vec3::new(2.0, 0.0, 0.0), 1e-5),
            "{:?}",
            whole.0
        );
        assert!(
            close(
                direction(whole.1),
                Vec3::new(1.0, 2.0, 0.0).normalize(),
                1e-5
            ),
            "diag(2,1,1) bends (1,1,0) toward y, not x: {:?}",
            whole.1
        );
        assert!(
            close(blended.0, Vec3::new(1.5, 0.0, 0.0), 1e-5),
            "{:?}",
            blended.0
        );
        assert!(
            close(
                direction(blended.1),
                Vec3::new(2.0, 3.0, 0.0).normalize(),
                1e-5
            ),
            "diag(1.5,1,1) has cofactor diag(1,1.5,1.5): {:?}",
            blended.1
        );
    }

    #[test]
    fn a_skinned_draw_reads_the_arena_in_both_passes() {
        let stride = size_of::<Vertex>() as u64;
        let posed = DrawKey::skinned(3, 9, 10);
        let plain = DrawKey {
            mesh: 3,
            texture: 9,
            source: VertexSource::Mesh,
        };

        assert_eq!(posed.source, VertexSource::Arena { first_vertex: 10 });
        assert_eq!(
            posed.source.arena_bytes(5),
            Some(10 * stride..15 * stride),
            "the slice is the placement's own vertices in the frame arena"
        );
        assert_eq!(
            plain.source.arena_bytes(5),
            None,
            "mesh draws keep the mesh buffer"
        );
        assert_eq!(
            (posed.mesh, posed.texture),
            (3, 9),
            "the mesh still names the index buffer"
        );
    }
}
