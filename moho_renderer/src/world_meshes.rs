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

    // TEMPORARY STUB: no behaviour yet; replaced when the movable-mesh card is implemented.
    /// Queue adding or replacing a mesh that draws only where placed.
    pub fn upsert_movable(&mut self, _id: WorldMeshId, _mesh: WorldMesh) {}

    // TEMPORARY STUB: no behaviour yet.
    /// Append a placement of the mesh under `id` for this frame.
    pub fn place(&mut self, _id: WorldMeshId, _transform: glam::Affine3A, _material: u32) {}

    // TEMPORARY STUB: no behaviour yet.
    /// Drop every placement.
    pub fn clear_placements(&mut self) {}

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
            let material: Vec<u32> = mesh.materials().map_or_else(
                || vec![material_idx; mesh.positions().len()],
                <[u32]>::to_vec,
            );
            let handle = backend.register_indexed_mesh(
                mesh.positions(),
                mesh.normals(),
                mesh.ao(),
                mesh.surface(),
                &light_level,
                mesh.light_rgb(),
                mesh.sky_exposure(),
                &material,
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
        uploads: Vec<Upload>,
        /// The `material` argument of each `register_indexed_mesh` call.
        materials: Vec<Vec<u32>>,
    }

    /// The channel slices of one `register_indexed_mesh` call, named as the
    /// backend parameters are.
    #[derive(Debug, Default, PartialEq)]
    struct Upload {
        vertices: Vec<[f32; 3]>,
        normals: Vec<[f32; 3]>,
        ao: Vec<f32>,
        geometry_type: Vec<u32>,
        light_level: Vec<f32>,
        block_light_rgb: Vec<[f32; 3]>,
        sky_exposed: Vec<f32>,
        indices: Vec<u32>,
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
            vertices: &[[f32; 3]],
            normals: &[[f32; 3]],
            ao: &[f32],
            geometry_type: &[u32],
            light_level: &[f32],
            block_light_rgb: &[[f32; 3]],
            sky_exposed: &[f32],
            material: &[u32],
            indices: &[u32],
        ) -> u32 {
            self.materials.push(material.to_vec());
            self.uploads.push(Upload {
                vertices: vertices.to_vec(),
                normals: normals.to_vec(),
                ao: ao.to_vec(),
                geometry_type: geometry_type.to_vec(),
                light_level: light_level.to_vec(),
                block_light_rgb: block_light_rgb.to_vec(),
                sky_exposed: sky_exposed.to_vec(),
                indices: indices.to_vec(),
            });
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
    fn flush_uploads_each_channel_into_its_gpu_vertex_slot() {
        let mut meshes = WorldMeshes::new();
        let mut backend = RecordingBackend::default();
        let positions = vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]];
        let normals = vec![[0.0, 1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]];
        let light_rgb = vec![[0.2, 0.9, 0.5], [0.9, 0.1, 0.1], [0.3, 0.4, 0.6]];
        let world_mesh = WorldMesh::new(
            positions.clone(),
            normals.clone(),
            vec![0.25; 3],
            light_rgb.clone(),
            vec![0.75; 3],
            vec![3; 3],
            vec![0, 2, 1],
        )
        .expect("well-formed test mesh");
        meshes.upsert(A, world_mesh, 0);

        meshes.flush(&mut backend);

        assert_eq!(
            backend.uploads,
            vec![Upload {
                vertices: positions,
                normals,
                ao: vec![0.25; 3],
                geometry_type: vec![3; 3],
                light_level: vec![0.9, 0.9, 0.6],
                block_light_rgb: light_rgb,
                sky_exposed: vec![0.75; 3],
                indices: vec![0, 2, 1],
            }]
        );
    }

    #[test]
    fn mesh_without_materials_uploads_the_upsert_material_on_every_vertex() {
        let mut meshes = WorldMeshes::new();
        let mut backend = RecordingBackend::default();
        meshes.upsert(A, mesh(1), 7);

        meshes.flush(&mut backend);

        assert_eq!(backend.materials, vec![vec![7, 7, 7]]);
    }

    #[test]
    fn mesh_with_materials_uploads_them_unchanged() {
        let mut meshes = WorldMeshes::new();
        let mut backend = RecordingBackend::default();
        let with_materials = mesh(1)
            .with_materials(vec![2, 5, 9])
            .expect("one material per vertex");
        meshes.upsert(A, with_materials, 7);

        meshes.flush(&mut backend);

        assert_eq!(backend.materials, vec![vec![2, 5, 9]]);
        let drawn = draws(&meshes);
        assert_eq!(
            drawn[0].1.material, 7,
            "the upsert material stays the instance default"
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

    fn at(x: f32) -> glam::Affine3A {
        glam::Affine3A::from_translation(glam::Vec3::new(x, 0.0, 0.0))
    }

    fn model_of(transform: glam::Affine3A) -> [[f32; 4]; 4] {
        glam::Mat4::from(transform).to_cols_array_2d()
    }

    const IDENTITY: [[f32; 4]; 4] = [
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ];

    #[test]
    fn moving_a_placed_mesh_registers_it_once() {
        let mut meshes = WorldMeshes::new();
        let mut backend = RecordingBackend::default();
        meshes.upsert_movable(A, mesh(1));
        meshes.flush(&mut backend);

        for x in [1.0, 2.0, 3.0] {
            meshes.place(A, at(x), 0);
            meshes.flush(&mut backend);

            let drawn = draws(&meshes);
            assert_eq!(drawn.len(), 1, "one instance per frame at x = {x}");
            assert_eq!(drawn[0].0, backend.handle_with_index_count(3));
            assert_eq!(drawn[0].1.model, model_of(at(x)));
            meshes.clear_placements();
        }

        assert_eq!(backend.registered.len(), 1, "moving never re-uploads");
        assert!(backend.unregistered.is_empty());
    }

    #[test]
    fn placement_material_reaches_the_instance_and_vertices_defer() {
        let mut meshes = WorldMeshes::new();
        let mut backend = RecordingBackend::default();
        meshes.upsert_movable(A, mesh(1));
        meshes.flush(&mut backend);

        meshes.place(A, at(1.0), 3);
        meshes.place(A, at(2.0), 5);

        let materials: Vec<u32> = draws(&meshes).iter().map(|(_, i)| i.material).collect();
        assert_eq!(materials, vec![3, 5]);
        assert_eq!(
            backend.materials,
            vec![Vec::<u32>::new()],
            "an empty material slice makes the vertices defer to the instance material"
        );
    }

    #[test]
    fn movable_mesh_with_materials_channel_uploads_it() {
        let mut meshes = WorldMeshes::new();
        let mut backend = RecordingBackend::default();
        let with_materials = mesh(1)
            .with_materials(vec![2, 5, 9])
            .expect("one material per vertex");

        meshes.upsert_movable(A, with_materials);
        meshes.flush(&mut backend);

        assert_eq!(backend.materials, vec![vec![2, 5, 9]]);
    }

    #[test]
    fn unplaced_movable_mesh_draws_nothing_and_stays_registered() {
        let mut meshes = WorldMeshes::new();
        let mut backend = RecordingBackend::default();
        meshes.upsert_movable(A, mesh(1));
        meshes.flush(&mut backend);
        meshes.place(A, at(1.0), 0);
        assert_eq!(draws(&meshes).len(), 1, "drawn on the frame it is placed");

        meshes.clear_placements();
        meshes.flush(&mut backend);

        assert!(draws(&meshes).is_empty());
        assert_eq!(backend.live_handles().len(), 1, "still uploaded");
        assert!(backend.unregistered.is_empty());
    }

    #[test]
    fn movable_mesh_has_no_identity_draw() {
        let mut meshes = WorldMeshes::new();
        let mut backend = RecordingBackend::default();

        meshes.upsert_movable(A, mesh(1));
        meshes.flush(&mut backend);

        assert_eq!(backend.registered.len(), 1, "the movable mesh is uploaded");
        assert!(draws(&meshes).is_empty());
    }

    #[test]
    fn placing_a_static_mesh_adds_a_copy() {
        let mut meshes = WorldMeshes::new();
        let mut backend = RecordingBackend::default();
        meshes.upsert(A, mesh(1), 7);
        meshes.flush(&mut backend);

        meshes.place(A, at(4.0), 9);

        let drawn = draws(&meshes);
        let handle = backend.handle_with_index_count(3);
        assert_eq!(drawn.len(), 2);
        assert_eq!((drawn[0].0, drawn[0].1.model), (handle, IDENTITY));
        assert_eq!(drawn[0].1.material, 7);
        assert_eq!((drawn[1].0, drawn[1].1.model), (handle, model_of(at(4.0))));
        assert_eq!(drawn[1].1.material, 9);
    }

    #[test]
    fn placement_on_a_missing_or_removed_id_is_skipped() {
        let c = WorldMeshId(3);
        let never_upserted = WorldMeshId(9);
        let mut meshes = WorldMeshes::new();
        let mut backend = RecordingBackend::default();
        meshes.upsert_movable(A, mesh(1));
        meshes.upsert_movable(c, mesh(2));
        meshes.flush(&mut backend);

        meshes.place(never_upserted, at(1.0), 0);
        meshes.place(A, at(2.0), 0);
        meshes.remove(A);
        meshes.place(c, at(3.0), 0);
        meshes.flush(&mut backend);

        let drawn = draws(&meshes);
        assert_eq!(drawn.len(), 1, "only the live id's placement draws");
        let c_handle = backend.handle_with_index_count(6);
        assert_eq!((drawn[0].0, drawn[0].1.model), (c_handle, model_of(at(3.0))));
    }

    #[test]
    fn mesh_upserted_and_placed_in_one_frame_draws() {
        let mut meshes = WorldMeshes::new();
        let mut backend = RecordingBackend::default();

        meshes.upsert_movable(A, mesh(1));
        meshes.place(A, at(2.0), 4);
        meshes.flush(&mut backend);

        let drawn = draws(&meshes);
        assert_eq!(drawn.len(), 1);
        assert_eq!(drawn[0].1.model, model_of(at(2.0)));
        assert_eq!(drawn[0].1.material, 4);
    }

    #[test]
    fn upserting_with_the_other_kind_switches_it() {
        let mut meshes = WorldMeshes::new();
        let mut backend = RecordingBackend::default();
        meshes.upsert(A, mesh(1), 0);
        meshes.flush(&mut backend);
        let first = backend.handle_with_index_count(3);
        assert_eq!(draws(&meshes).len(), 1, "static draws at identity");

        meshes.upsert_movable(A, mesh(2));
        meshes.flush(&mut backend);

        assert!(draws(&meshes).is_empty(), "movable draws only where placed");
        assert_eq!(backend.unregistered, vec![first]);
        meshes.place(A, at(1.0), 0);
        let drawn = draws(&meshes);
        assert_eq!(drawn.len(), 1);
        assert_eq!(drawn[0].1.model, model_of(at(1.0)));

        meshes.clear_placements();
        meshes.upsert(A, mesh(3), 0);
        meshes.flush(&mut backend);

        let drawn = draws(&meshes);
        assert_eq!(drawn.len(), 1, "static again, with no placements");
        assert_eq!(drawn[0].1.model, IDENTITY);
        assert_eq!(backend.live_handles().len(), 1, "one live upload");
    }

    #[derive(Clone, Copy, Debug, PartialEq)]
    enum Kind {
        Static,
        Movable,
    }

    #[derive(Clone, Debug)]
    enum Op {
        Upsert(u64, bool),
        UpsertMovable(u64, bool),
        Place(u64),
        ClearPlacements,
        Remove(u64),
        Flush,
    }

    fn ops() -> impl Strategy<Value = Vec<Op>> {
        let op = prop_oneof![
            (0u64..4, any::<bool>()).prop_map(|(id, non_empty)| Op::Upsert(id, non_empty)),
            (0u64..4, any::<bool>()).prop_map(|(id, non_empty)| Op::UpsertMovable(id, non_empty)),
            (0u64..5).prop_map(Op::Place),
            Just(Op::ClearPlacements),
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
            let mut model: BTreeMap<u64, Kind> = BTreeMap::new();
            let mut placed: Vec<u64> = Vec::new();

            for op in ops {
                match op {
                    Op::Upsert(id, non_empty) => {
                        meshes.upsert(WorldMeshId(id), mesh(usize::from(non_empty)), 0);
                        if non_empty {
                            model.insert(id, Kind::Static);
                        } else {
                            model.remove(&id);
                        }
                    }
                    Op::UpsertMovable(id, non_empty) => {
                        meshes.upsert_movable(WorldMeshId(id), mesh(usize::from(non_empty)));
                        if non_empty {
                            model.insert(id, Kind::Movable);
                        } else {
                            model.remove(&id);
                        }
                    }
                    Op::Place(id) => {
                        meshes.place(WorldMeshId(id), at(1.0), 0);
                        placed.push(id);
                    }
                    Op::ClearPlacements => {
                        meshes.clear_placements();
                        placed.clear();
                    }
                    Op::Remove(id) => {
                        meshes.remove(WorldMeshId(id));
                        model.remove(&id);
                    }
                    Op::Flush => meshes.flush(&mut backend),
                }
            }
            meshes.flush(&mut backend);

            let statics = model.values().filter(|k| **k == Kind::Static).count();
            let placements = placed.iter().filter(|id| model.contains_key(id)).count();
            let drawn: Vec<u32> = draws(&meshes).iter().map(|(h, _)| *h).collect();
            let drawn_set: BTreeSet<u32> = drawn.iter().copied().collect();
            prop_assert_eq!(drawn.len(), statics + placements, "one instance per static id and per live placement");
            prop_assert!(drawn_set.is_subset(&backend.live_handles()), "only live handles are drawn");
            prop_assert_eq!(backend.live_handles().len(), model.len(), "one upload per live id, none leaked");
            let freed: BTreeSet<u32> = backend.unregistered.iter().copied().collect();
            prop_assert_eq!(freed.len(), backend.unregistered.len(), "no handle freed twice");
            prop_assert!(freed.iter().all(|h| backend.registered.iter().any(|(r, _)| r == h)));
        }
    }
}
