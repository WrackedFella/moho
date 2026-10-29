//! Renderer-side store of contract meshes keyed by [`WorldMeshId`].

use crate::RendererBackend;
use crate::textures::WHITE_TEXTURE;
use moho_render_api::{InstanceGpu, Rgba8Image, TextureId, WorldMesh, WorldMeshId, WorldMeshRange};
use std::collections::{BTreeMap, BTreeSet};

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
    /// For a skinned mesh, the shortest palette that covers its joints.
    palette_len: Option<usize>,
}

/// One placement: where, in which material, and with which texture (`None` is white).
#[derive(Debug, Clone, Copy)]
struct Placement {
    transform: glam::Affine3A,
    material: u32,
    texture: Option<TextureId>,
}

#[derive(Debug)]
struct SkinnedPlacement {
    id: WorldMeshId,
    placement: Placement,
    palette: Vec<glam::Affine3A>,
}

/// An instance of a mesh at `model`; object type 0 is terrain, so the fragment shader
/// colours by normal.
fn instance(model: glam::Mat4, material: u32) -> InstanceGpu {
    InstanceGpu {
        model: model.to_cols_array_2d(),
        material,
        object_type: 0,
        padding: [0, 0],
    }
}

/// Queues world-mesh and texture changes and applies them to a backend on `flush`.
///
/// Live meshes are kept in id order so the draw order is deterministic.
#[derive(Debug, Default)]
pub struct WorldMeshes {
    live: BTreeMap<WorldMeshId, Live>,
    pending: BTreeMap<WorldMeshId, Pending>,
    placements: BTreeMap<WorldMeshId, Vec<Placement>>,
    /// Skinned placements in call order.
    skinned: Vec<SkinnedPlacement>,
    /// Mesh ids already warned about, so a bad skinned placement warns once, not per frame;
    /// an upsert or remove of the id forgets it.
    warned: BTreeSet<WorldMeshId>,
    /// Backend handle of each uploaded texture.
    live_textures: BTreeMap<TextureId, u32>,
    /// A texture waiting for the next `flush`; `None` is a queued remove.
    pending_textures: BTreeMap<TextureId, Option<Rgba8Image>>,
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
        self.placements.entry(id).or_default().push(Placement {
            transform,
            material,
            texture: None,
        });
    }

    /// Append a placement of the skinned mesh under `id` for this frame, posed by `palette`
    /// (one model-space matrix per joint, as `Model::joint_palette` gives it; copied).
    ///
    /// Drawn through [`skinned_draws`](Self::skinned_draws). On `flush`, a palette shorter
    /// than the mesh's highest joint plus one drops the placement with a warning.
    pub fn place_skinned(
        &mut self,
        id: WorldMeshId,
        transform: glam::Affine3A,
        material: u32,
        texture: Option<TextureId>,
        palette: &[glam::Affine3A],
    ) {
        self.skinned.push(SkinnedPlacement {
            id,
            placement: Placement {
                transform,
                material,
                texture,
            },
            palette: palette.to_vec(),
        });
    }

    /// Queue adding or replacing the texture under `id`; replacing unregisters the old
    /// handle on `flush`.
    pub fn upsert_texture(&mut self, id: TextureId, image: Rgba8Image) {
        self.pending_textures.insert(id, Some(image));
    }

    /// Queue removing the texture under `id`; placements naming it then draw white.
    pub fn remove_texture(&mut self, id: TextureId) {
        self.pending_textures.insert(id, None);
    }

    /// Like [`place`](Self::place), drawn with the texture under `texture`. A texture id
    /// that is not live when drawn draws white.
    pub fn place_textured(
        &mut self,
        id: WorldMeshId,
        transform: glam::Affine3A,
        material: u32,
        texture: TextureId,
    ) {
        self.placements.entry(id).or_default().push(Placement {
            transform,
            material,
            texture: Some(texture),
        });
    }

    /// Append a placement of the mesh under `id` for this frame that draws without casting a
    /// shadow, listed by [`unshadowed_draws`](Self::unshadowed_draws) and by no other draw list.
    pub fn place_unshadowed(
        &mut self,
        _id: WorldMeshId,
        _transform: glam::Affine3A,
        _material: u32,
    ) {
        todo!("unshadowed placements")
    }

    /// Unshadowed placements of live ids as `(backend handle, instance)`, in id order and
    /// call order.
    pub fn unshadowed_draws(&self) -> impl Iterator<Item = (u32, InstanceGpu)> + '_ {
        std::iter::empty()
    }

    /// Drop every placement; the renderer calls this each frame.
    pub fn clear_placements(&mut self) {
        self.placements.clear();
        self.skinned.clear();
    }

    /// Queue removing the mesh under `id`.
    pub fn remove(&mut self, id: WorldMeshId) {
        self.pending.insert(id, None);
    }

    /// Queue removing the mesh under every id in `range`.
    pub fn remove_range(&mut self, range: WorldMeshRange) {
        for id in range.iter() {
            self.remove(id);
        }
    }

    /// How many meshes are registered with the backend, as of the last `flush`.
    pub fn live_len(&self) -> usize {
        self.live.len()
    }

    /// Apply queued changes: register new meshes, skins and textures, unregister replaced or
    /// removed ones, then drop skinned placements whose palette is too short for their mesh
    /// (one warning per mesh id).
    pub fn flush(&mut self, backend: &mut dyn RendererBackend) {
        for (id, change) in std::mem::take(&mut self.pending_textures) {
            if let Some(old) = self.live_textures.remove(&id) {
                backend.unregister_texture(old);
            }
            if let Some(image) = change {
                let handle = backend.register_texture(&image);
                self.live_textures.insert(id, handle);
            }
        }
        for (id, change) in std::mem::take(&mut self.pending) {
            self.warned.remove(&id);
            if let Some(old) = self.live.remove(&id) {
                backend.unregister_mesh(old.handle);
            }
            let Some((mesh, kind)) = change else {
                continue;
            };
            // An empty slice makes the backend defer every vertex to the instance material.
            let material: Vec<u32> = match (mesh.materials(), kind) {
                (Some(materials), _) => materials.to_vec(),
                (None, Kind::Static { material_idx }) => {
                    vec![material_idx; mesh.positions().len()]
                }
                (None, Kind::Movable) => Vec::new(),
            };
            let handle = backend.register_world_mesh(&mesh, &material);
            let palette_len = mesh.skin().map(|(joints, weights)| {
                backend.register_skin(handle, joints, weights);
                crate::skinning::palette_len(joints)
            });
            self.live.insert(
                id,
                Live {
                    handle,
                    kind,
                    palette_len,
                },
            );
        }
        self.skinned.retain(|skinned| {
            let Some(live) = self.live.get(&skinned.id) else {
                return true;
            };
            let Some(needed) = live.palette_len else {
                if self.warned.insert(skinned.id) {
                    tracing::warn!(
                        mesh_id = skinned.id.0,
                        "skinned placement of a mesh with no skin; placement not drawn"
                    );
                }
                return false;
            };
            if skinned.palette.len() >= needed {
                return true;
            }
            if self.warned.insert(skinned.id) {
                tracing::warn!(
                    mesh_id = skinned.id.0,
                    joints = skinned.palette.len(),
                    needed,
                    "palette shorter than the mesh's joints; placement not drawn"
                );
            }
            false
        });
    }

    /// Instances as `(backend handle, instance)`: each static mesh at identity, then
    /// every placement of a live id in id order and call order.
    pub fn draws(&self) -> impl Iterator<Item = (u32, InstanceGpu)> + '_ {
        self.textured_draws()
            .map(|(mesh, _texture, instance)| (mesh, instance))
    }

    /// The same instances as [`draws`](Self::draws), as `(mesh handle, texture handle,
    /// instance)`. The texture handle is 0, the white default, for statics, for
    /// [`place`](Self::place) and for a texture id that is not live.
    pub fn textured_draws(&self) -> impl Iterator<Item = (u32, u32, InstanceGpu)> + '_ {
        let statics = self.live.values().filter_map(move |live| match live.kind {
            Kind::Static { material_idx } => Some((
                live.handle,
                WHITE_TEXTURE,
                instance(glam::Mat4::IDENTITY, material_idx),
            )),
            Kind::Movable => None,
        });
        let placed = self.placements.iter().flat_map(move |(id, placements)| {
            let handle = self.live.get(id).map(|live| live.handle);
            handle.into_iter().flat_map(move |handle| {
                placements.iter().map(move |placement| {
                    let texture = self.texture_handle(placement.texture);
                    let model = glam::Mat4::from(placement.transform);
                    (handle, texture, instance(model, placement.material))
                })
            })
        });
        statics.chain(placed)
    }

    /// Skinned placements of live skinned meshes in call order, as `(mesh handle, texture
    /// handle, instance, palette)`. The instance's model is the placement's root; the
    /// texture handle is 0, the white default, for [`None`] and for a texture id that is
    /// not live.
    pub fn skinned_draws(
        &self,
    ) -> impl Iterator<Item = (u32, u32, InstanceGpu, &[glam::Affine3A])> + '_ {
        self.skinned.iter().filter_map(move |skinned| {
            let live = self.live.get(&skinned.id)?;
            live.palette_len?;
            let placement = &skinned.placement;
            let model = glam::Mat4::from(placement.transform);
            Some((
                live.handle,
                self.texture_handle(placement.texture),
                instance(model, placement.material),
                skinned.palette.as_slice(),
            ))
        })
    }

    fn texture_handle(&self, texture: Option<TextureId>) -> u32 {
        texture
            .and_then(|t| self.live_textures.get(&t).copied())
            .unwrap_or(WHITE_TEXTURE)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use std::collections::{BTreeMap, BTreeSet};

    /// One `register_skin` call: mesh handle, joints, weights.
    type SkinCall = (u32, Vec<[u16; 4]>, Vec<[f32; 4]>);

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
        /// Each `register_skin` call as `(mesh handle, joints, weights)`.
        skins: Vec<SkinCall>,
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

        /// The handle `image` was registered under, whatever the upload order.
        fn texture_handle(&self, image: &Rgba8Image) -> u32 {
            let Some((handle, _)) = self.textures.iter().find(|(_, i)| i == image) else {
                panic!("no texture registered with image {image:?}");
            };
            *handle
        }
    }

    /// A backend that takes world meshes through `register_world_mesh` and records the
    /// UVs each one carried; any other upload route fails the test.
    #[derive(Default)]
    struct UvBackend {
        uvs: Vec<Option<Vec<[f32; 2]>>>,
        materials: Vec<Vec<u32>>,
    }

    impl RendererBackend for UvBackend {
        fn resize(&mut self, _width: u32, _height: u32) {}
        fn register_mesh(&mut self, _vertices: &[[f32; 3]]) -> u32 {
            unreachable!("world meshes upload through register_world_mesh")
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
            _material: &[u32],
            _indices: &[u32],
        ) -> u32 {
            unreachable!("flush must hand the whole mesh to register_world_mesh to carry its UVs")
        }
        fn register_world_mesh(&mut self, mesh: &WorldMesh, material: &[u32]) -> u32 {
            self.uvs.push(mesh.uvs().map(<[_]>::to_vec));
            self.materials.push(material.to_vec());
            11
        }
        fn unregister_mesh(&mut self, _mesh: u32) {}
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
        fn register_skin(&mut self, mesh: u32, joints: &[[u16; 4]], weights: &[[f32; 4]]) {
            self.skins.push((mesh, joints.to_vec(), weights.to_vec()));
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

    #[test]
    fn remove_range_unregisters_exactly_the_range() {
        let mut meshes = WorldMeshes::new();
        let mut backend = RecordingBackend::default();
        for id in 1..=5 {
            meshes.upsert(WorldMeshId(id), mesh(id as usize), 0);
        }
        meshes.flush(&mut backend);
        let handle = |id: usize| backend.handle_with_index_count(3 * id);
        let (outside, inside) = (
            BTreeSet::from([handle(1), handle(5)]),
            BTreeSet::from([handle(2), handle(3), handle(4)]),
        );

        meshes.remove_range(WorldMeshRange::new(WorldMeshId(2), 3).expect("fits"));
        assert!(
            backend.unregistered.is_empty(),
            "freed on flush, not before"
        );
        meshes.flush(&mut backend);

        let freed: BTreeSet<u32> = backend.unregistered.iter().copied().collect();
        assert_eq!(freed, inside);
        assert_eq!(backend.unregistered.len(), 3, "each handle freed once");
        assert_eq!(backend.live_handles(), outside);
        let drawn: BTreeSet<u32> = draws(&meshes).iter().map(|(h, _)| *h).collect();
        assert_eq!(drawn, outside, "only ids outside the range still draw");
    }

    #[test]
    fn live_len_counts_flushed_meshes_only() {
        let mut meshes = WorldMeshes::new();
        let mut backend = RecordingBackend::default();
        assert_eq!(meshes.live_len(), 0);

        meshes.upsert(A, mesh(1), 0);
        meshes.upsert_movable(B, mesh(2));
        assert_eq!(meshes.live_len(), 0, "queued, not yet registered");
        meshes.flush(&mut backend);
        assert_eq!(meshes.live_len(), 2, "static and movable both count");

        meshes.remove(A);
        assert_eq!(meshes.live_len(), 2, "a queued remove is not applied yet");
        meshes.flush(&mut backend);
        assert_eq!(meshes.live_len(), 1);
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
    fn unshadowed_placements_are_listed_apart_from_ordinary_draws() {
        let mut meshes = WorldMeshes::new();
        let mut backend = RecordingBackend::default();
        meshes.upsert_movable(A, mesh(1));
        meshes.flush(&mut backend);

        meshes.place(A, at(1.0), 3);
        meshes.place_unshadowed(A, at(2.0), 5);

        let handle = backend.handle_with_index_count(3);
        let ordinary = draws(&meshes);
        assert_eq!(
            ordinary.len(),
            1,
            "the unshadowed placement is not in draws"
        );
        assert_eq!(ordinary[0].1.model, model_of(at(1.0)));
        assert_eq!(meshes.textured_draws().count(), 1);
        let unshadowed: Vec<(u32, InstanceGpu)> = meshes.unshadowed_draws().collect();
        assert_eq!(unshadowed.len(), 1);
        assert_eq!(unshadowed[0].0, handle);
        assert_eq!(unshadowed[0].1.model, model_of(at(2.0)));
        assert_eq!(unshadowed[0].1.material, 5);
    }

    #[test]
    fn unshadowed_placements_keep_id_order_then_call_order() {
        let mut meshes = WorldMeshes::new();
        let mut backend = RecordingBackend::default();
        meshes.upsert_movable(A, mesh(1));
        meshes.upsert_movable(B, mesh(2));
        meshes.flush(&mut backend);

        meshes.place_unshadowed(B, at(1.0), 0);
        meshes.place_unshadowed(A, at(2.0), 0);
        meshes.place_unshadowed(A, at(3.0), 0);

        let drawn: Vec<(u32, [[f32; 4]; 4])> = meshes
            .unshadowed_draws()
            .map(|(handle, instance)| (handle, instance.model))
            .collect();
        let (a, b) = (
            backend.handle_with_index_count(3),
            backend.handle_with_index_count(6),
        );
        assert_eq!(
            drawn,
            vec![
                (a, model_of(at(2.0))),
                (a, model_of(at(3.0))),
                (b, model_of(at(1.0)))
            ]
        );
    }

    #[test]
    fn an_unshadowed_placement_on_a_missing_id_is_skipped() {
        let mut meshes = WorldMeshes::new();
        let mut backend = RecordingBackend::default();
        meshes.upsert_movable(A, mesh(1));
        meshes.flush(&mut backend);

        meshes.place_unshadowed(B, at(1.0), 0);

        assert_eq!(meshes.unshadowed_draws().count(), 0);
    }

    #[test]
    fn clear_placements_clears_unshadowed_placements() {
        let mut meshes = WorldMeshes::new();
        let mut backend = RecordingBackend::default();
        meshes.upsert_movable(A, mesh(1));
        meshes.flush(&mut backend);
        meshes.place_unshadowed(A, at(1.0), 0);
        assert_eq!(meshes.unshadowed_draws().count(), 1);

        meshes.clear_placements();

        assert_eq!(meshes.unshadowed_draws().count(), 0);
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
        let handle_of = |red: u8| backend.texture_handle(&texture(red));

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
            assert_eq!(
                handles,
                vec![backend.texture_handle(&texture(10)); 3],
                "frame {frame}"
            );
            meshes.clear_placements();
        }

        assert_eq!(backend.textures.len(), 1);
        assert!(backend.unregistered_textures.is_empty());
    }

    #[test]
    fn flush_registers_a_mesh_with_uvs_through_register_world_mesh() {
        let uvs = vec![[0.25, 0.75], [1.0, 0.5], [0.0, 1.0]];
        let textured_mesh = mesh(1).with_uvs(uvs.clone()).expect("one uv per vertex");
        let mut meshes = WorldMeshes::new();
        let mut backend = UvBackend::default();
        meshes.upsert(A, textured_mesh, 7);
        meshes.upsert_movable(B, mesh(2));

        meshes.flush(&mut backend);

        assert_eq!(
            backend.uvs,
            vec![Some(uvs), None],
            "the mesh's UVs reach the backend; a mesh without UVs passes none"
        );
        assert_eq!(
            backend.materials,
            vec![vec![7, 7, 7], Vec::<u32>::new()],
            "the per-vertex material channel is resolved before the hand-off"
        );
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
                (mesh_handle, backend.texture_handle(&texture(10))),
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
        let old = backend.texture_handle(&texture(10));

        meshes.upsert_texture(SKIN, texture(20));
        meshes.flush(&mut backend);

        assert_eq!(backend.textures.len(), 2);
        let new = backend.texture_handle(&texture(20));
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
        let skin = backend.texture_handle(&texture(10));
        let fur = backend.texture_handle(&texture(20));

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

    /// A three-vertex mesh whose second vertex is bound to `highest_joint`.
    fn skinned_mesh(highest_joint: u16) -> WorldMesh {
        mesh(1)
            .with_skin(
                vec![[0, 0, 0, 0], [highest_joint, 0, 0, 0], [0, 0, 0, 0]],
                vec![[1.0, 0.0, 0.0, 0.0]; 3],
            )
            .expect("one joint set and weight set per vertex")
    }

    /// `count` joints, joint `j` translated by `(j, lift, 0)`.
    fn palette(count: u16, lift: f32) -> Vec<glam::Affine3A> {
        (0..count)
            .map(|j| glam::Affine3A::from_translation(glam::Vec3::new(f32::from(j), lift, 0.0)))
            .collect()
    }

    type SkinnedDraw = (u32, u32, InstanceGpu, Vec<glam::Affine3A>);

    fn skinned(meshes: &WorldMeshes) -> Vec<SkinnedDraw> {
        meshes
            .skinned_draws()
            .map(|(mesh, texture, instance, palette)| (mesh, texture, instance, palette.to_vec()))
            .collect()
    }

    /// Runs `run` and returns its result with what `tracing` wrote meanwhile.
    fn logged<T>(run: impl FnOnce() -> T) -> (T, String) {
        use std::io::Write;
        use std::sync::{Arc, Mutex};

        struct Sink(Arc<Mutex<Vec<u8>>>);
        impl Write for Sink {
            fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
                self.0.lock().expect("log sink").extend_from_slice(buf);
                Ok(buf.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }

        let buffer = Arc::new(Mutex::new(Vec::new()));
        let writer = Arc::clone(&buffer);
        let subscriber = tracing_subscriber::fmt()
            .with_writer(move || Sink(Arc::clone(&writer)))
            .with_ansi(false)
            .finish();
        let result = tracing::subscriber::with_default(subscriber, run);
        let bytes = buffer.lock().expect("log sink").clone();
        (
            result,
            String::from_utf8(bytes).expect("log output is utf-8"),
        )
    }

    #[test]
    fn a_skinned_placement_yields_its_palette_in_order() {
        let mut meshes = WorldMeshes::new();
        let mut backend = RecordingBackend::default();
        meshes.upsert_movable(A, skinned_mesh(2));
        meshes.upsert_texture(SKIN, texture(10));
        meshes.flush(&mut backend);
        let turned = vec![
            at(1.0),
            glam::Affine3A::from_rotation_z(std::f32::consts::FRAC_PI_2),
            at(-3.0),
        ];

        meshes.place_skinned(A, at(4.0), 6, Some(SKIN), &turned);

        let drawn = skinned(&meshes);
        assert_eq!(drawn.len(), 1);
        let (handle, texture_handle, instance, yielded) = &drawn[0];
        assert_eq!(*handle, backend.handle_with_index_count(3));
        assert_eq!(*texture_handle, backend.texture_handle(&texture(10)));
        assert_eq!(
            instance.model,
            model_of(at(4.0)),
            "the root is the instance model"
        );
        assert_eq!(instance.material, 6);
        assert_eq!(*yielded, turned, "joint order is the palette order");
        assert!(
            draws(&meshes).is_empty(),
            "a skinned placement is not a plain draw"
        );
        assert!(textured(&meshes).is_empty());
    }

    #[test]
    fn a_skinned_placement_without_a_texture_draws_white() {
        let mut meshes = WorldMeshes::new();
        let mut backend = RecordingBackend::default();
        meshes.upsert_movable(A, skinned_mesh(1));
        meshes.flush(&mut backend);

        meshes.place_skinned(A, at(1.0), 0, None, &palette(2, 0.0));

        let drawn = skinned(&meshes);
        assert_eq!(drawn.len(), 1);
        assert_eq!(drawn[0].1, 0, "the white default texture");
    }

    #[test]
    fn two_skinned_placements_of_one_mesh_register_it_and_its_skin_once() {
        let mut meshes = WorldMeshes::new();
        let mut backend = RecordingBackend::default();
        meshes.upsert_movable(A, skinned_mesh(2));
        let (near, far) = (palette(3, 0.0), palette(3, 7.0));

        for frame in 0..2 {
            meshes.place_skinned(A, at(1.0), 0, None, &near);
            meshes.place_skinned(A, at(2.0), 0, None, &far);
            meshes.flush(&mut backend);

            let drawn = skinned(&meshes);
            let handle = backend.handle_with_index_count(3);
            assert_eq!(drawn.len(), 2, "frame {frame}");
            assert_eq!((drawn[0].0, drawn[1].0), (handle, handle), "frame {frame}");
            assert_eq!(drawn[0].2.model, model_of(at(1.0)));
            assert_eq!(drawn[1].2.model, model_of(at(2.0)));
            assert_eq!((&drawn[0].3, &drawn[1].3), (&near, &far), "frame {frame}");
            meshes.clear_placements();
        }

        assert_eq!(backend.registered.len(), 1, "the mesh uploads once");
        assert_eq!(
            backend.skins,
            vec![(
                backend.handle_with_index_count(3),
                vec![[0, 0, 0, 0], [2, 0, 0, 0], [0, 0, 0, 0]],
                vec![[1.0, 0.0, 0.0, 0.0]; 3],
            )],
            "the skin is registered once, against the mesh handle"
        );
    }

    #[test]
    fn a_mesh_without_a_skin_registers_no_skin() {
        let mut meshes = WorldMeshes::new();
        let mut backend = RecordingBackend::default();
        meshes.upsert_movable(A, mesh(1));
        meshes.upsert(B, mesh(2), 0);

        meshes.flush(&mut backend);

        assert!(backend.skins.is_empty());
    }

    #[test]
    fn a_palette_shorter_than_the_meshs_joints_is_not_drawn() {
        let hero = WorldMeshId(4242);
        let mut meshes = WorldMeshes::new();
        let mut backend = RecordingBackend::default();
        meshes.upsert_movable(hero, skinned_mesh(3));
        meshes.upsert_movable(B, skinned_mesh(1));
        meshes.flush(&mut backend);
        let full = palette(4, 0.0);
        let other = palette(2, 0.0);

        meshes.place_skinned(hero, at(1.0), 0, None, &palette(3, 0.0));
        meshes.place_skinned(hero, at(2.0), 0, None, &full);
        meshes.place_skinned(hero, at(3.0), 0, None, &palette(0, 0.0));
        meshes.place_skinned(B, at(4.0), 0, None, &other);
        meshes.place(B, at(5.0), 0);
        let ((), log) = logged(|| meshes.flush(&mut backend));

        let xs: Vec<f32> = skinned(&meshes).iter().map(|d| d.2.model[3][0]).collect();
        assert_eq!(
            xs,
            vec![2.0, 4.0],
            "only the placements with enough joints draw"
        );
        assert_eq!(draws(&meshes).len(), 1, "plain placements are unaffected");
        assert_eq!(
            log.matches("WARN").count(),
            1,
            "one warning per mesh id, however many placements are short: {log}"
        );
        assert!(log.contains("4242"), "the warning names the mesh id: {log}");
    }

    #[test]
    fn replacing_a_skinned_mesh_with_an_unskinned_one_stops_its_skinned_draws() {
        let mut meshes = WorldMeshes::new();
        let mut backend = RecordingBackend::default();
        meshes.upsert_movable(A, skinned_mesh(1));
        meshes.flush(&mut backend);
        meshes.place_skinned(A, at(1.0), 0, None, &palette(2, 0.0));
        assert_eq!(skinned(&meshes).len(), 1);
        let old = backend.handle_with_index_count(3);
        meshes.clear_placements();

        meshes.upsert_movable(A, mesh(2));
        meshes.flush(&mut backend);
        meshes.place_skinned(A, at(1.0), 0, None, &palette(2, 0.0));
        meshes.flush(&mut backend);

        assert!(skinned(&meshes).is_empty());
        assert_eq!(backend.unregistered, vec![old]);
    }

    #[test]
    fn a_removed_skinned_mesh_draws_nothing() {
        let mut meshes = WorldMeshes::new();
        let mut backend = RecordingBackend::default();
        meshes.upsert_movable(A, skinned_mesh(1));
        meshes.flush(&mut backend);

        meshes.remove(A);
        meshes.flush(&mut backend);
        meshes.place_skinned(A, at(1.0), 0, None, &palette(2, 0.0));
        meshes.flush(&mut backend);

        assert!(skinned(&meshes).is_empty());
    }

    #[test]
    fn a_short_palette_placed_in_the_upload_frame_is_dropped_at_that_flush() {
        let hero = WorldMeshId(4242);
        let mut meshes = WorldMeshes::new();
        let mut backend = RecordingBackend::default();

        meshes.upsert_movable(hero, skinned_mesh(3));
        meshes.place_skinned(hero, at(1.0), 0, None, &palette(3, 0.0));
        let ((), log) = logged(|| meshes.flush(&mut backend));

        assert!(skinned(&meshes).is_empty());
        assert_eq!(log.matches("WARN").count(), 1, "{log}");
        assert!(log.contains("4242"), "the warning names the mesh id: {log}");
    }

    #[test]
    fn a_short_palette_warns_once_per_mesh_not_every_frame() {
        let hero = WorldMeshId(4242);
        let mut meshes = WorldMeshes::new();
        let mut backend = RecordingBackend::default();
        meshes.upsert_movable(hero, skinned_mesh(3));
        let short = palette(3, 0.0);

        let (frames, log) = logged(|| {
            let mut frames = Vec::new();
            for _ in 0..3 {
                meshes.place_skinned(hero, at(1.0), 0, None, &short);
                meshes.flush(&mut backend);
                frames.push(skinned(&meshes).len());
                meshes.clear_placements();
            }
            frames
        });

        assert_eq!(frames, vec![0, 0, 0], "a short palette never draws");
        assert_eq!(
            log.matches("WARN").count(),
            1,
            "one warning per mesh, not per frame: {log}"
        );
        assert!(log.contains("4242"), "the warning names the mesh id: {log}");

        meshes.upsert_movable(hero, skinned_mesh(3));
        let ((), again) = logged(|| {
            meshes.flush(&mut backend);
            meshes.place_skinned(hero, at(1.0), 0, None, &short);
            meshes.flush(&mut backend);
        });

        assert!(skinned(&meshes).is_empty());
        assert_eq!(
            again.matches("WARN").count(),
            1,
            "a replaced mesh warns afresh: {again}"
        );
        assert!(again.contains("4242"), "{again}");
    }

    #[test]
    fn a_skinned_placement_of_an_unskinned_mesh_warns_once() {
        let odd = WorldMeshId(7373);
        let mut meshes = WorldMeshes::new();
        let mut backend = RecordingBackend::default();
        meshes.upsert_movable(odd, mesh(1));
        meshes.flush(&mut backend);

        let (frames, log) = logged(|| {
            let mut frames = Vec::new();
            for _ in 0..2 {
                meshes.place_skinned(odd, at(1.0), 0, None, &palette(2, 0.0));
                meshes.flush(&mut backend);
                frames.push(skinned(&meshes).len());
                meshes.clear_placements();
            }
            frames
        });

        assert_eq!(frames, vec![0, 0], "an unskinned mesh has no skinned draw");
        assert_eq!(
            log.matches("WARN").count(),
            1,
            "one warning in total: {log}"
        );
        assert!(log.contains("7373"), "the warning names the mesh id: {log}");
    }

    #[test]
    fn replacing_a_skinned_mesh_registers_the_new_skin() {
        let mut meshes = WorldMeshes::new();
        let mut backend = RecordingBackend::default();
        meshes.upsert_movable(A, skinned_mesh(1));
        meshes.flush(&mut backend);
        meshes.upsert_movable(A, skinned_mesh(3));
        meshes.flush(&mut backend);

        meshes.place_skinned(A, at(1.0), 0, None, &palette(2, 0.0));
        let ((), log) = logged(|| meshes.flush(&mut backend));

        let handles: Vec<u32> = backend.registered.iter().map(|(h, _)| *h).collect();
        assert_eq!(backend.skins.len(), 2);
        assert_eq!(backend.skins[0].0, handles[0]);
        assert_eq!(backend.skins[0].1[1], [1, 0, 0, 0]);
        assert_eq!(backend.skins[1].0, handles[1]);
        assert_eq!(backend.skins[1].1[1], [3, 0, 0, 0]);
        assert!(skinned(&meshes).is_empty(), "2 joints < 4 needed");
        assert_eq!(log.matches("WARN").count(), 1, "{log}");
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
        RemoveRange(u64, u32),
        Flush,
    }

    fn ops() -> impl Strategy<Value = Vec<Op>> {
        let op = prop_oneof![
            (0u64..4, any::<bool>()).prop_map(|(id, non_empty)| Op::Upsert(id, non_empty)),
            (0u64..4, any::<bool>()).prop_map(|(id, non_empty)| Op::UpsertMovable(id, non_empty)),
            (0u64..5).prop_map(Op::Place),
            Just(Op::ClearPlacements),
            (0u64..4).prop_map(Op::Remove),
            (0u64..5, 0u32..5).prop_map(|(first, len)| Op::RemoveRange(first, len)),
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
                    Op::RemoveRange(first, len) => {
                        let range = WorldMeshRange::new(WorldMeshId(first), len).expect("fits");
                        meshes.remove_range(range);
                        model.retain(|id, _| !(first..first + u64::from(len)).contains(id));
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
            prop_assert_eq!(meshes.live_len(), model.len(), "the store counts what the backend holds");
            let freed: BTreeSet<u32> = backend.unregistered.iter().copied().collect();
            prop_assert_eq!(freed.len(), backend.unregistered.len(), "no handle freed twice");
            prop_assert!(freed.iter().all(|h| backend.registered.iter().any(|(r, _)| r == h)));
        }
    }
}
