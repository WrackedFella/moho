//! Renderer-side store of contract meshes keyed by [`WorldMeshId`].

use crate::RendererBackend;
use moho_render_api::{InstanceGpu, WorldMesh, WorldMeshId};
use std::collections::BTreeMap;

/// A mesh waiting for the next `flush`; `None` is a queued remove.
type Pending = Option<(WorldMesh, u32)>;

/// A mesh uploaded to the backend.
#[derive(Debug, Clone, Copy)]
struct Live {
    handle: u32,
    material_idx: u32,
}

/// Queues world-mesh changes and applies them to a backend on `flush`.
///
/// Live meshes are kept in id order so the draw order is deterministic.
#[derive(Debug, Default)]
pub struct WorldMeshes {
    live: BTreeMap<WorldMeshId, Live>,
    pending: BTreeMap<WorldMeshId, Pending>,
}

impl WorldMeshes {
    pub fn new() -> Self {
        Self::default()
    }

    /// Queue adding or replacing the mesh under `id`. An empty mesh is a remove.
    pub fn upsert(&mut self, id: WorldMeshId, mesh: WorldMesh, material_idx: u32) {
        if mesh.is_empty() {
            self.remove(id);
        } else {
            self.pending.insert(id, Some((mesh, material_idx)));
        }
    }

    /// Queue removing the mesh under `id`.
    pub fn remove(&mut self, id: WorldMeshId) {
        self.pending.insert(id, None);
    }

    /// Apply queued changes: register new meshes, unregister replaced or removed ones.
    pub fn flush(&mut self, backend: &mut dyn RendererBackend) {
        for (id, change) in std::mem::take(&mut self.pending) {
            if let Some(old) = self.live.remove(&id) {
                backend.unregister_mesh(old.handle);
            }
            let Some((mesh, material_idx)) = change else {
                continue;
            };
            let light_level: Vec<f32> = mesh
                .light_rgb()
                .iter()
                .map(|rgb| rgb[0].max(rgb[1]).max(rgb[2]))
                .collect();
            let handle = backend.register_indexed_mesh(
                mesh.positions(),
                mesh.normals(),
                mesh.ao(),
                mesh.surface(),
                &light_level,
                mesh.light_rgb(),
                mesh.sky_exposure(),
                mesh.indices(),
            );
            self.live.insert(
                id,
                Live {
                    handle,
                    material_idx,
                },
            );
        }
    }

    /// One identity-transform instance per live mesh, as `(backend handle, instance)`.
    pub fn draws(&self) -> impl Iterator<Item = (u32, InstanceGpu)> + '_ {
        self.live.values().map(|live| {
            let instance = InstanceGpu {
                model: glam::Mat4::IDENTITY.to_cols_array_2d(),
                material: live.material_idx,
                object_type: 0, // terrain: the fragment shader colours by normal
                padding: [0, 0],
            };
            (live.handle, instance)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use std::collections::{BTreeMap, BTreeSet};

    /// Records register/unregister calls. Handles start at 1 and are never reused,
    /// so a stale handle can't be mistaken for a live one.
    #[derive(Default)]
    struct RecordingBackend {
        next_handle: u32,
        registered: Vec<(u32, usize)>,
        unregistered: Vec<u32>,
    }

    impl RecordingBackend {
        fn live_handles(&self) -> BTreeSet<u32> {
            let freed: BTreeSet<u32> = self.unregistered.iter().copied().collect();
            self.registered
                .iter()
                .map(|(h, _)| *h)
                .filter(|h| !freed.contains(h))
                .collect()
        }

        fn handle_with_index_count(&self, index_count: usize) -> u32 {
            let Some((handle, _)) = self.registered.iter().find(|(_, n)| *n == index_count) else {
                panic!("no mesh registered with {index_count} indices");
            };
            *handle
        }
    }

    impl RendererBackend for RecordingBackend {
        fn resize(&mut self, _width: u32, _height: u32) {}
        fn register_mesh(&mut self, _vertices: &[[f32; 3]]) -> u32 {
            unreachable!("world meshes upload indexed meshes")
        }
        fn register_indexed_mesh(
            &mut self,
            _vertices: &[[f32; 3]],
            _normals: &[[f32; 3]],
            _ao: &[f32],
            _geometry_type: &[u32],
            _light_level: &[f32],
            _block_light_rgb: &[[f32; 3]],
            _sky_exposed: &[f32],
            indices: &[u32],
        ) -> u32 {
            self.next_handle += 1;
            self.registered.push((self.next_handle, indices.len()));
            self.next_handle
        }
        fn unregister_mesh(&mut self, mesh: u32) {
            self.unregistered.push(mesh);
        }
        fn begin_frame(
            &mut self,
            _camera: (glam::Mat4, glam::Mat4, glam::Vec3),
        ) -> Result<(), crate::FrameError> {
            Ok(())
        }
        fn enqueue_draw(&mut self, _mesh: u32, _instances: &[InstanceGpu]) {}
        fn submit_frame(&mut self) {}
        fn set_materials(&mut self, _materials: &[crate::MaterialGpu]) {}
        fn update_lighting(&mut self, _lighting: crate::LightingGpu) {}
        fn add_point_light(
            &mut self,
            _position: glam::Vec3,
            _color: glam::Vec3,
            _intensity: f32,
            _range: f32,
        ) -> u32 {
            0
        }
        fn remove_light(&mut self, _id: u32) -> bool {
            true
        }
        fn set_light_position(&mut self, _id: u32, _position: glam::Vec3) {}
        fn set_light_enabled(&mut self, _id: u32, _enabled: bool) {}
        fn set_shadow_quality(&mut self, _quality: u8) {}
        fn set_ssao_quality(&mut self, _quality: u8) {}
        fn set_frame_callback_raw(&mut self, _ptr: Option<*mut dyn crate::FrameCallback>) {}
        fn set_frame_callback_arc(
            &mut self,
            _cb: Option<std::sync::Arc<std::sync::Mutex<dyn crate::FrameCallback>>>,
        ) {
        }
    }

    /// A mesh of `triangles` triangles over three shared vertices. The index
    /// count (3 * triangles) tells meshes apart in the recording backend.
    fn mesh(triangles: usize) -> WorldMesh {
        WorldMesh::new(
            vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]],
            vec![[0.0, 1.0, 0.0]; 3],
            vec![1.0; 3],
            vec![[1.0; 3]; 3],
            vec![1.0; 3],
            vec![0; 3],
            [0, 1, 2].repeat(triangles),
        )
        .expect("well-formed test mesh")
    }

    fn draws(meshes: &WorldMeshes) -> Vec<(u32, InstanceGpu)> {
        meshes.draws().collect()
    }

    const A: WorldMeshId = WorldMeshId(1);
    const B: WorldMeshId = WorldMeshId(2);

    #[test]
    fn registered_mesh_is_drawn_after_flush() {
        let mut meshes = WorldMeshes::new();
        let mut backend = RecordingBackend::default();
        meshes.upsert(A, mesh(1), 7);

        meshes.flush(&mut backend);

        let drawn = draws(&meshes);
        assert_eq!(drawn.len(), 1);
        let (handle, instance) = drawn[0];
        assert_eq!(handle, backend.handle_with_index_count(3));
        assert_eq!(instance.material, 7);
        assert_eq!(
            instance.model,
            glam::Mat4::IDENTITY.to_cols_array_2d(),
            "world meshes are already in world space"
        );
    }

    #[test]
    fn replacing_mesh_draws_new_and_frees_old() {
        let mut meshes = WorldMeshes::new();
        let mut backend = RecordingBackend::default();
        meshes.upsert(A, mesh(1), 0);
        meshes.flush(&mut backend);
        let old = backend.handle_with_index_count(3);

        meshes.upsert(A, mesh(2), 0);
        meshes.flush(&mut backend);

        let new = backend.handle_with_index_count(6);
        let handles: Vec<u32> = draws(&meshes).iter().map(|(h, _)| *h).collect();
        assert_eq!(handles, vec![new], "the new mesh is drawn exactly once");
        assert_eq!(backend.unregistered, vec![old]);
        assert_eq!(backend.live_handles(), BTreeSet::from([new]));
    }

    #[test]
    fn removed_mesh_is_not_drawn_and_is_freed() {
        let mut meshes = WorldMeshes::new();
        let mut backend = RecordingBackend::default();
        meshes.upsert(A, mesh(1), 0);
        meshes.upsert(B, mesh(2), 0);
        meshes.flush(&mut backend);
        let a = backend.handle_with_index_count(3);
        let b = backend.handle_with_index_count(6);

        meshes.remove(A);
        meshes.flush(&mut backend);

        let handles: Vec<u32> = draws(&meshes).iter().map(|(h, _)| *h).collect();
        assert_eq!(handles, vec![b], "only the other mesh is still drawn");
        assert_eq!(backend.unregistered, vec![a]);
    }

    #[test]
    fn mesh_queued_before_first_flush_is_drawn() {
        let mut meshes = WorldMeshes::new();
        let mut backend = RecordingBackend::default();
        meshes.upsert(A, mesh(1), 0);
        meshes.upsert(B, mesh(2), 0);
        assert!(
            backend.registered.is_empty(),
            "nothing is uploaded before flush"
        );

        meshes.flush(&mut backend);

        let handles: BTreeSet<u32> = draws(&meshes).iter().map(|(h, _)| *h).collect();
        assert_eq!(handles, backend.live_handles());
        assert_eq!(handles.len(), 2);
    }

    #[test]
    fn empty_mesh_draws_nothing_and_frees_previous() {
        let mut meshes = WorldMeshes::new();
        let mut backend = RecordingBackend::default();
        meshes.upsert(A, mesh(1), 0);
        meshes.flush(&mut backend);
        let old = backend.handle_with_index_count(3);

        meshes.upsert(A, mesh(0), 0);
        meshes.flush(&mut backend);

        assert!(draws(&meshes).is_empty());
        assert_eq!(backend.unregistered, vec![old]);
        assert_eq!(
            backend.registered.len(),
            1,
            "an empty mesh is never uploaded"
        );
    }

    #[test]
    fn last_queued_change_per_id_wins() {
        let mut meshes = WorldMeshes::new();
        let mut backend = RecordingBackend::default();
        meshes.upsert(A, mesh(1), 0);
        meshes.upsert(A, mesh(2), 0);
        meshes.upsert(B, mesh(1), 0);
        meshes.remove(B);

        meshes.flush(&mut backend);

        assert_eq!(
            backend
                .registered
                .iter()
                .map(|(_, n)| *n)
                .collect::<Vec<_>>(),
            vec![6],
            "only A's last upsert is uploaded; B was removed before upload"
        );
        assert_eq!(draws(&meshes).len(), 1);
        assert!(backend.unregistered.is_empty());

        meshes.remove(A);
        meshes.upsert(A, mesh(3), 0);
        meshes.flush(&mut backend);

        let handles: Vec<u32> = draws(&meshes).iter().map(|(h, _)| *h).collect();
        assert_eq!(handles, vec![backend.handle_with_index_count(9)]);
    }

    #[derive(Clone, Debug)]
    enum Op {
        Upsert(u64, bool),
        Remove(u64),
        Flush,
    }

    fn ops() -> impl Strategy<Value = Vec<Op>> {
        let op = prop_oneof![
            (0u64..4, any::<bool>()).prop_map(|(id, non_empty)| Op::Upsert(id, non_empty)),
            (0u64..4).prop_map(Op::Remove),
            Just(Op::Flush),
        ];
        prop::collection::vec(op, 0..40)
    }

    proptest! {
        #[test]
        fn live_handles_match_live_ids(ops in ops()) {
            let mut meshes = WorldMeshes::new();
            let mut backend = RecordingBackend::default();
            let mut model: BTreeMap<u64, bool> = BTreeMap::new();

            for op in ops {
                match op {
                    Op::Upsert(id, non_empty) => {
                        meshes.upsert(WorldMeshId(id), mesh(usize::from(non_empty)), 0);
                        model.insert(id, non_empty);
                    }
                    Op::Remove(id) => {
                        meshes.remove(WorldMeshId(id));
                        model.insert(id, false);
                    }
                    Op::Flush => meshes.flush(&mut backend),
                }
            }
            meshes.flush(&mut backend);

            let expected = model.values().filter(|live| **live).count();
            let drawn: Vec<u32> = draws(&meshes).iter().map(|(h, _)| *h).collect();
            let drawn_set: BTreeSet<u32> = drawn.iter().copied().collect();
            prop_assert_eq!(drawn.len(), expected, "one draw per live id");
            prop_assert_eq!(drawn_set.len(), drawn.len(), "no handle drawn twice");
            prop_assert_eq!(backend.live_handles(), drawn_set, "no leaked or dangling handle");
            let freed: BTreeSet<u32> = backend.unregistered.iter().copied().collect();
            prop_assert_eq!(freed.len(), backend.unregistered.len(), "no handle freed twice");
            prop_assert!(freed.iter().all(|h| backend.registered.iter().any(|(r, _)| r == h)));
        }
    }
}
