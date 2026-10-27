//! Renderer-side store of contract meshes keyed by [`WorldMeshId`].

use crate::RendererBackend;
use moho_render_api::{InstanceGpu, Rgba8Image, TextureId, WorldMesh, WorldMeshId};
use std::collections::BTreeMap;

/// How a mesh draws: once at identity, or only where placed.
#[derive(Debug, Clone, Copy)]
enum Kind {
    Static { material_idx: u32 },
    Movable,
}

/// A mesh waiting for the next `flush`; `None` is a queued remove.
type Pending = Option<(WorldMesh, Kind)>;

/// A mesh uploaded to the backend.
#[derive(Debug, Clone, Copy)]
struct Live {
    handle: u32,
    kind: Kind,
}

/// Queues world-mesh changes and applies them to a backend on `flush`.
///
/// Live meshes are kept in id order so the draw order is deterministic.
#[derive(Debug, Default)]
pub struct WorldMeshes {
    live: BTreeMap<WorldMeshId, Live>,
    pending: BTreeMap<WorldMeshId, Pending>,
    placements: BTreeMap<WorldMeshId, Vec<(glam::Affine3A, u32)>>,
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
            self.pending
                .insert(id, Some((mesh, Kind::Static { material_idx })));
        }
    }

    /// Queue adding or replacing a mesh that draws only where placed, so moving it
    /// never re-uploads. An empty mesh is a remove.
    pub fn upsert_movable(&mut self, id: WorldMeshId, mesh: WorldMesh) {
        if mesh.is_empty() {
            self.remove(id);
        } else {
            self.pending.insert(id, Some((mesh, Kind::Movable)));
        }
    }

    /// Append a placement of the mesh under `id` for this frame, drawn with `material`.
    ///
    /// Not checked against live ids here: an upsert queued the same frame is
    /// flushed before draws, and a placement on an id that is not live is skipped.
    pub fn place(&mut self, id: WorldMeshId, transform: glam::Affine3A, material: u32) {
        self.placements
            .entry(id)
            .or_default()
            .push((transform, material));
    }

    /// Queue adding or replacing the texture under `id`; replacing unregisters the old
    /// handle on `flush`.
    pub fn upsert_texture(&mut self, id: TextureId, image: Rgba8Image) {
        let _ = (id, image);
        panic!("WorldMeshes::upsert_texture is not implemented")
    }

    /// Queue removing the texture under `id`; placements naming it then draw white.
    pub fn remove_texture(&mut self, id: TextureId) {
        let _ = id;
        panic!("WorldMeshes::remove_texture is not implemented")
    }

    /// Like [`place`](Self::place), drawn with the texture under `texture`.
    pub fn place_textured(
        &mut self,
        id: WorldMeshId,
        transform: glam::Affine3A,
        material: u32,
        texture: TextureId,
    ) {
        let _ = (id, transform, material, texture);
        panic!("WorldMeshes::place_textured is not implemented")
    }

    /// Drop every placement; the renderer calls this each frame.
    pub fn clear_placements(&mut self) {
        self.placements.clear();
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
            let Some((mesh, kind)) = change else {
                continue;
            };
            let light_level: Vec<f32> = mesh
                .light_rgb()
                .iter()
                .map(|rgb| rgb[0].max(rgb[1]).max(rgb[2]))
                .collect();
            // An empty slice makes the backend defer every vertex to the instance material.
            let material: Vec<u32> = match (mesh.materials(), kind) {
                (Some(materials), _) => materials.to_vec(),
                (None, Kind::Static { material_idx }) => {
                    vec![material_idx; mesh.positions().len()]
                }
                (None, Kind::Movable) => Vec::new(),
            };
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
            self.live.insert(id, Live { handle, kind });
        }
    }

    /// Instances as `(backend handle, instance)`: each static mesh at identity, then
    /// every placement of a live id in id order and call order.
    pub fn draws(&self) -> impl Iterator<Item = (u32, InstanceGpu)> + '_ {
        // object_type 0: terrain, the fragment shader colours by normal
        let instance = |model: glam::Mat4, material: u32| InstanceGpu {
            model: model.to_cols_array_2d(),
            material,
            object_type: 0,
            padding: [0, 0],
        };
        let statics = self.live.values().filter_map(move |live| match live.kind {
            Kind::Static { material_idx } => {
                Some((live.handle, instance(glam::Mat4::IDENTITY, material_idx)))
            }
            Kind::Movable => None,
        });
        let placed = self.placements.iter().flat_map(move |(id, placements)| {
            let handle = self.live.get(id).map(|live| live.handle);
            placements.iter().filter_map(move |(transform, material)| {
                handle.map(|h| (h, instance(glam::Mat4::from(*transform), *material)))
            })
        });
        statics.chain(placed)
    }

    /// The same instances as [`draws`](Self::draws), as `(mesh handle, texture handle,
    /// instance)`. The texture handle is 0, the white default, for statics, for
    /// [`place`](Self::place) and for a texture id that is not live.
    pub fn textured_draws(&self) -> impl Iterator<Item = (u32, u32, InstanceGpu)> + '_ {
        std::iter::empty()
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
        /// Each `register_texture` call as `(handle, image)`; handles start at 100.
        textures: Vec<(u32, Rgba8Image)>,
        unregistered_textures: Vec<u32>,
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
        fn register_texture(&mut self, image: &Rgba8Image) -> u32 {
            let handle = 100 + u32::try_from(self.textures.len()).expect("few textures");
            self.textures.push((handle, image.clone()));
            handle
        }
        fn unregister_texture(&mut self, texture: u32) {
            self.unregistered_textures.push(texture);
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
        assert_eq!(
            (drawn[0].0, drawn[0].1.model),
            (c_handle, model_of(at(3.0)))
        );
    }

    #[test]
    fn placements_pair_with_their_own_mesh_across_ids() {
        let c = WorldMeshId(3);
        let mut meshes = WorldMeshes::new();
        let mut backend = RecordingBackend::default();
        meshes.upsert_movable(A, mesh(1));
        meshes.upsert_movable(B, mesh(2));
        meshes.upsert(c, mesh(3), 0);
        meshes.flush(&mut backend);
        let a_handle = backend.handle_with_index_count(3);
        let b_handle = backend.handle_with_index_count(6);
        let c_handle = backend.handle_with_index_count(9);

        meshes.place(B, at(10.0), 0);
        meshes.place(A, at(20.0), 0);
        meshes.place(B, at(11.0), 0);
        meshes.place(c, at(30.0), 0);

        let drawn: Vec<(u32, [[f32; 4]; 4])> =
            draws(&meshes).iter().map(|(h, i)| (*h, i.model)).collect();
        assert_eq!(
            drawn,
            vec![
                (c_handle, IDENTITY),
                (a_handle, model_of(at(20.0))),
                (b_handle, model_of(at(10.0))),
                (b_handle, model_of(at(11.0))),
                (c_handle, model_of(at(30.0))),
            ]
        );
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

    const SKIN: TextureId = TextureId(1);
    const FUR: TextureId = TextureId(2);

    /// A 1x1 image whose red channel tells textures apart.
    fn texture(red: u8) -> Rgba8Image {
        Rgba8Image::new([1, 1], vec![[red, 0, 0, 255]]).expect("one pixel")
    }

    fn textured(meshes: &WorldMeshes) -> Vec<(u32, u32, InstanceGpu)> {
        meshes.textured_draws().collect()
    }

    #[test]
    fn a_textured_placement_draws_with_its_texture_handle() {
        let mut meshes = WorldMeshes::new();
        let mut backend = RecordingBackend::default();
        meshes.upsert_movable(A, mesh(1));
        meshes.upsert_texture(SKIN, texture(10));
        assert!(
            backend.textures.is_empty(),
            "nothing is uploaded before flush"
        );
        meshes.flush(&mut backend);

        meshes.place_textured(A, at(2.0), 3, SKIN);

        let drawn = textured(&meshes);
        assert_eq!(backend.textures.len(), 1);
        let (handle, uploaded) = &backend.textures[0];
        assert_eq!(
            *uploaded,
            texture(10),
            "the image reaches the backend as given"
        );
        assert_eq!(drawn.len(), 1);
        assert_eq!(drawn[0].0, backend.handle_with_index_count(3));
        assert_eq!(drawn[0].1, *handle);
        assert_eq!(drawn[0].2.material, 3);
        assert_eq!(drawn[0].2.model, model_of(at(2.0)));
        let plain: Vec<(u32, [[f32; 4]; 4])> =
            draws(&meshes).iter().map(|(h, i)| (*h, i.model)).collect();
        assert_eq!(
            plain,
            vec![(drawn[0].0, drawn[0].2.model)],
            "draws() keeps its (handle, instance) items"
        );
    }

    #[test]
    fn placements_pick_their_own_texture() {
        let mut meshes = WorldMeshes::new();
        let mut backend = RecordingBackend::default();
        meshes.upsert_movable(A, mesh(1));
        meshes.upsert_texture(SKIN, texture(10));
        meshes.upsert_texture(FUR, texture(20));
        meshes.flush(&mut backend);
        let handle_of = |red: u8| {
            backend
                .textures
                .iter()
                .find(|(_, image)| *image == texture(red))
                .map(|(h, _)| *h)
                .expect("uploaded")
        };

        meshes.place_textured(A, at(1.0), 0, FUR);
        meshes.place_textured(A, at(2.0), 0, SKIN);
        meshes.place_textured(A, at(3.0), 0, FUR);

        let order: Vec<(u32, [[f32; 4]; 4])> = textured(&meshes)
            .iter()
            .map(|(_, t, i)| (*t, i.model))
            .collect();
        assert_eq!(
            order,
            vec![
                (handle_of(20), model_of(at(1.0))),
                (handle_of(10), model_of(at(2.0))),
                (handle_of(20), model_of(at(3.0))),
            ]
        );
    }

    #[test]
    fn a_texture_is_registered_once_however_many_placements_use_it() {
        let mut meshes = WorldMeshes::new();
        let mut backend = RecordingBackend::default();
        meshes.upsert_movable(A, mesh(1));
        meshes.upsert_movable(B, mesh(2));
        meshes.upsert_texture(SKIN, texture(10));
        meshes.flush(&mut backend);

        for frame in 0..3 {
            meshes.place_textured(A, at(1.0), 0, SKIN);
            meshes.place_textured(A, at(2.0), 0, SKIN);
            meshes.place_textured(B, at(3.0), 0, SKIN);
            meshes.flush(&mut backend);

            let handles: Vec<u32> = textured(&meshes).iter().map(|(_, t, _)| *t).collect();
            assert_eq!(handles, vec![backend.textures[0].0; 3], "frame {frame}");
            meshes.clear_placements();
        }

        assert_eq!(backend.textures.len(), 1);
        assert!(backend.unregistered_textures.is_empty());
    }

    #[test]
    fn place_and_a_not_live_texture_draw_with_the_white_handle() {
        let mut meshes = WorldMeshes::new();
        let mut backend = RecordingBackend::default();
        meshes.upsert_movable(A, mesh(1));
        meshes.upsert_texture(SKIN, texture(10));
        meshes.flush(&mut backend);

        meshes.place(A, at(1.0), 0);
        meshes.place_textured(A, at(2.0), 0, TextureId(99));
        meshes.place_textured(A, at(3.0), 0, SKIN);

        let drawn = textured(&meshes);
        let mesh_handle = backend.handle_with_index_count(3);
        let textures: Vec<(u32, u32)> = drawn.iter().map(|(m, t, _)| (*m, *t)).collect();
        assert_eq!(
            textures,
            vec![
                (mesh_handle, 0),
                (mesh_handle, 0),
                (mesh_handle, backend.textures[0].0),
            ],
            "the mesh still draws for a texture that is not live"
        );
    }

    #[test]
    fn a_static_mesh_draws_with_the_white_handle() {
        let mut meshes = WorldMeshes::new();
        let mut backend = RecordingBackend::default();
        meshes.upsert(A, mesh(1), 7);
        meshes.flush(&mut backend);

        let drawn = textured(&meshes);

        assert_eq!(drawn.len(), 1);
        assert_eq!((drawn[0].1, drawn[0].2.material), (0, 7));
    }

    #[test]
    fn replacing_a_texture_unregisters_the_old_handle() {
        let mut meshes = WorldMeshes::new();
        let mut backend = RecordingBackend::default();
        meshes.upsert_movable(A, mesh(1));
        meshes.upsert_texture(SKIN, texture(10));
        meshes.flush(&mut backend);
        let old = backend.textures[0].0;

        meshes.upsert_texture(SKIN, texture(20));
        meshes.flush(&mut backend);

        assert_eq!(backend.textures.len(), 2);
        assert_eq!(backend.textures[1].1, texture(20));
        let new = backend.textures[1].0;
        assert_ne!(new, old);
        assert_eq!(backend.unregistered_textures, vec![old]);
        meshes.place_textured(A, at(1.0), 0, SKIN);
        let handles: Vec<u32> = textured(&meshes).iter().map(|(_, t, _)| *t).collect();
        assert_eq!(handles, vec![new]);
    }

    #[test]
    fn removing_a_texture_unregisters_it() {
        let mut meshes = WorldMeshes::new();
        let mut backend = RecordingBackend::default();
        meshes.upsert_movable(A, mesh(1));
        meshes.upsert_texture(SKIN, texture(10));
        meshes.upsert_texture(FUR, texture(20));
        meshes.flush(&mut backend);
        let skin = backend.textures[0].0;
        let fur = backend.textures[1].0;

        meshes.remove_texture(SKIN);
        assert!(
            backend.unregistered_textures.is_empty(),
            "freed on flush, not before"
        );
        meshes.flush(&mut backend);

        assert_eq!(backend.unregistered_textures, vec![skin]);
        meshes.place_textured(A, at(1.0), 0, SKIN);
        meshes.place_textured(A, at(2.0), 0, FUR);
        let handles: Vec<u32> = textured(&meshes).iter().map(|(_, t, _)| *t).collect();
        assert_eq!(handles, vec![0, fur], "the removed texture draws white");
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
