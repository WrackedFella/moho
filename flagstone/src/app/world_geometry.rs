//! Hands voxel chunk meshes to the renderer through the world-geometry contract.

use crate::App;
use crate::app::cut_caps::CutCaps;
use flagstone_voxel::{MaterialType, VoxelChunk};
use glam::IVec3;
use moho_physics::{HitTag, PhysicsWorld};
use moho_render_api::{WorldMesh, WorldMeshError, WorldMeshId};
use moho_renderer::Scene;

const AXIS_BITS: u32 = 21;
/// Half the 21-bit range: shifts a signed axis into `0..1 << AXIS_BITS`.
const AXIS_OFFSET: i32 = 1 << 20;

/// Stable renderer key for the chunk at `pos`: 21 bits per signed axis.
pub fn chunk_mesh_id(pos: IVec3) -> WorldMeshId {
    debug_assert!(
        pos.cmpge(IVec3::splat(-AXIS_OFFSET)).all() && pos.cmplt(IVec3::splat(AXIS_OFFSET)).all(),
        "chunk position {pos} does not fit 21 bits per axis"
    );
    let axis = |v: i32| u64::from((v + AXIS_OFFSET).cast_unsigned());
    WorldMeshId((axis(pos.x) << (2 * AXIS_BITS)) | (axis(pos.y) << AXIS_BITS) | axis(pos.z))
}

/// The chunk's mesh in the world-geometry contract's form.
pub fn chunk_world_mesh(chunk: &VoxelChunk) -> Result<WorldMesh, WorldMeshError> {
    WorldMesh::new(
        chunk.vertices().to_vec(),
        chunk.normals().to_vec(),
        chunk.ambient_occlusion().to_vec(),
        chunk.block_light_rgb().to_vec(),
        chunk.sky_exposed().to_vec(),
        chunk.geometry_type().to_vec(),
        chunk.indices().to_vec(),
    )
}

/// The strategy game's terrain tag; it has no ray consumer yet, so the value only satisfies
/// the registration signature.
const TERRAIN_TAG: HitTag = HitTag(0);

const GRASS: glam::Vec3 = glam::Vec3::new(0.3, 0.6, 0.3);
const DIRT: glam::Vec3 = glam::Vec3::new(0.6, 0.5, 0.4);

/// Register the terrain material once and return its table index; idempotent.
pub fn register_terrain_material(scene: &mut Scene) -> u32 {
    let terrain = MaterialType::VoxelTerrain {
        top_albedo: GRASS,
        side_albedo: DIRT,
    };
    scene.material_table.find_or_push(&terrain)
}

/// Register the cut-face material once and return its table index; idempotent. It is
/// the terrain's dirt colour on both faces, so a cap reads as the inside of the ground.
pub fn register_cut_face_material(scene: &mut Scene) -> u32 {
    let cut_face = MaterialType::VoxelTerrain {
        top_albedo: DIRT,
        side_albedo: DIRT,
    };
    scene.material_table.find_or_push(&cut_face)
}

/// Queue `chunk`'s mesh with the renderer, register its collider and rebuild its cut-face
/// cap on the next cap refresh. A refused mesh is logged; the chunk is neither drawn nor
/// solid.
fn upsert_chunk_mesh(
    scene: &mut Scene,
    physics: Option<&mut PhysicsWorld>,
    cut_caps: &mut CutCaps,
    material_idx: u32,
    chunk: &VoxelChunk,
) {
    let id = chunk_mesh_id(chunk.chunk_pos());
    match chunk_world_mesh(chunk) {
        Ok(mesh) => {
            if let Some(pw) = physics {
                pw.set_world_mesh(id, &mesh, TERRAIN_TAG);
            }
            scene.world_meshes_mut().upsert(id, mesh, material_idx);
        }
        Err(error) => {
            tracing::warn!(chunk = ?chunk.chunk_pos(), %error, "chunk mesh refused, not drawn");
            drop_chunk_mesh(scene, physics, id);
        }
    }
    cut_caps.mark_dirty(chunk.chunk_pos());
}

/// Take the chunk at `pos` out of the renderer and physics, with its cap now.
fn remove_chunk_mesh(
    scene: &mut Scene,
    physics: Option<&mut PhysicsWorld>,
    cut_caps: &mut CutCaps,
    pos: IVec3,
) {
    cut_caps.remove(pos, scene.world_meshes_mut());
    drop_chunk_mesh(scene, physics, chunk_mesh_id(pos));
}

fn drop_chunk_mesh(scene: &mut Scene, physics: Option<&mut PhysicsWorld>, id: WorldMeshId) {
    scene.world_meshes_mut().remove(id);
    if let Some(pw) = physics {
        pw.remove_world_mesh(id);
    }
}

/// Put `chunk` in the store and hand its mesh to the renderer and physics,
/// replacing any previous chunk at that position. Its cut-face cap is rebuilt on the
/// next cap refresh.
pub fn insert_chunk(app: &mut App, chunk: VoxelChunk) -> Option<VoxelChunk> {
    upsert_chunk_mesh(
        &mut app.scene,
        app.physics.world.as_mut(),
        &mut app.cut_caps,
        app.terrain_material_idx,
        &chunk,
    );
    app.entities.chunks.insert(chunk)
}

/// Take the chunk at `pos` out of the store, the renderer and physics, with its cap.
pub fn remove_chunk(app: &mut App, pos: IVec3) -> Option<VoxelChunk> {
    remove_chunk_mesh(
        &mut app.scene,
        app.physics.world.as_mut(),
        &mut app.cut_caps,
        pos,
    );
    app.entities.chunks.remove(pos)
}

/// Hand every stored chunk's mesh to the renderer and physics; their cut-face caps
/// are rebuilt on the next cap refresh.
pub fn upsert_all_chunks(app: &mut App) {
    for chunk in app.entities.chunks.iter() {
        upsert_chunk_mesh(
            &mut app.scene,
            app.physics.world.as_mut(),
            &mut app.cut_caps,
            app.terrain_material_idx,
            chunk,
        );
    }
}

/// Call before the store is cleared: mesh ids are derived from its chunks.
pub fn remove_all_chunk_meshes(app: &mut App) {
    for chunk in app.entities.chunks.iter() {
        remove_chunk_mesh(
            &mut app.scene,
            app.physics.world.as_mut(),
            &mut app.cut_caps,
            chunk.chunk_pos(),
        );
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::App;
    use crate::app::cut_caps::cap_mesh_id;
    use crate::app::cut_caps::tests::{
        CaptureBackend, assert_ground_capped_at, cut_and_draw, drawn_caps, ground_app, refresh_caps,
    };
    use crate::app::event_loop::event_processor::EventProcessor;
    use crate::app::event_loop::frame_processor::FrameProcessor;
    use crate::game_state::GameState;
    use flagstone_voxel::WorldEvent;
    use moho_render_api::RenderMaterial;
    use moho_renderer::RendererBackend;
    use proptest::prelude::*;
    use std::collections::HashSet;

    const LIMIT: i32 = 1 << 20;

    proptest! {
        #[test]
        fn chunk_mesh_id_is_unique_within_range(
            a in (-LIMIT..LIMIT, -LIMIT..LIMIT, -LIMIT..LIMIT),
            b in (-LIMIT..LIMIT, -LIMIT..LIMIT, -LIMIT..LIMIT),
            step in (-1..=1i32, -1..=1i32, -1..=1i32),
        ) {
            let a = IVec3::new(a.0, a.1, a.2);
            let b = IVec3::new(b.0, b.1, b.2);
            let near = (a + IVec3::new(step.0, step.1, step.2)).clamp(IVec3::splat(-LIMIT), IVec3::splat(LIMIT - 1));

            prop_assert_eq!(a == b, chunk_mesh_id(a) == chunk_mesh_id(b));
            prop_assert_eq!(a == near, chunk_mesh_id(a) == chunk_mesh_id(near));
        }
    }

    #[test]
    fn chunk_mesh_id_is_unique_across_axis_extremes() {
        let corners = [-LIMIT, -1, 0, 1, LIMIT - 1];
        let mut seen = HashSet::new();

        for x in corners {
            for y in corners {
                for z in corners {
                    assert!(
                        seen.insert(chunk_mesh_id(IVec3::new(x, y, z))),
                        "id collision at ({x}, {y}, {z})"
                    );
                }
            }
        }
    }

    /// Records register and unregister calls; handles are never reused.
    #[derive(Default)]
    pub(crate) struct RecordingBackend {
        next_handle: u32,
        pub(crate) registered: Vec<u32>,
        pub(crate) unregistered: Vec<u32>,
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
            _indices: &[u32],
        ) -> u32 {
            self.next_handle += 1;
            self.registered.push(self.next_handle);
            self.next_handle
        }
        fn unregister_mesh(&mut self, mesh: u32) {
            self.unregistered.push(mesh);
        }
        fn begin_frame(
            &mut self,
            _camera: (glam::Mat4, glam::Mat4, glam::Vec3),
        ) -> Result<(), moho_renderer::FrameError> {
            Ok(())
        }
        fn enqueue_draw(&mut self, _mesh: u32, _instances: &[moho_renderer::InstanceGpu]) {}
        fn submit_frame(&mut self) {}
        fn set_materials(&mut self, _materials: &[moho_renderer::MaterialGpu]) {}
        fn update_lighting(&mut self, _lighting: moho_renderer::LightingGpu) {}
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

    fn drawn_handles(app: &mut App) -> Vec<u32> {
        app.scene
            .world_meshes_mut()
            .draws()
            .map(|(h, _)| h)
            .collect()
    }

    /// A chunk with a distinct value in every channel, so a swapped pair of
    /// same-typed channels shows up.
    pub(crate) fn chunk_at(pos: IVec3) -> VoxelChunk {
        VoxelChunk::new(
            pos,
            vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]],
            vec![[0.0, 1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]],
            vec![0.25, 0.3, 0.35],
            vec![3, 4, 5],
            vec![0.6, 0.6, 0.6],
            vec![[0.2, 0.9, 0.5], [0.1, 0.1, 0.1], [0.4, 0.3, 0.2]],
            vec![0.75, 0.8, 0.85],
            vec![0, 1, 2],
            1,
        )
    }

    #[test]
    fn chunk_world_mesh_carries_every_channel() {
        let chunk = chunk_at(IVec3::new(1, 2, 3));

        let mesh = chunk_world_mesh(&chunk).expect("well-formed chunk");

        assert_eq!(mesh.positions(), chunk.vertices());
        assert_eq!(mesh.normals(), chunk.normals());
        assert_eq!(mesh.ao(), [0.25, 0.3, 0.35]);
        assert_eq!(mesh.ao(), chunk.ambient_occlusion());
        assert_eq!(mesh.light_rgb(), chunk.block_light_rgb());
        assert_eq!(mesh.sky_exposure(), [0.75, 0.8, 0.85]);
        assert_eq!(mesh.sky_exposure(), chunk.sky_exposed());
        assert_eq!(mesh.surface(), chunk.geometry_type());
        assert_eq!(mesh.indices(), chunk.indices());
    }

    #[test]
    fn register_terrain_material_returns_the_terrain_entry_and_is_idempotent() {
        let mut scene = Scene::new();
        scene
            .material_table
            .find_or_push(&MaterialType::Lambertian {
                albedo: glam::Vec3::new(0.9, 0.1, 0.1),
            });
        scene.material_table.find_or_push(&MaterialType::Metal {
            albedo: glam::Vec3::new(0.2, 0.2, 0.9),
            fuzz: 0.3,
        });

        let first = register_terrain_material(&mut scene);
        let len_after_first = scene.material_table.as_slice().len();
        let second = register_terrain_material(&mut scene);

        assert_eq!(first, 2, "appended after the two existing materials");
        assert_eq!(second, first);
        assert_eq!(len_after_first, 3);
        assert_eq!(scene.material_table.as_slice().len(), 3, "no growth");
        let expected = MaterialType::VoxelTerrain {
            top_albedo: glam::Vec3::new(0.3, 0.6, 0.3),
            side_albedo: glam::Vec3::new(0.6, 0.5, 0.4),
        }
        .to_gpu();
        let stored = scene.material_table.as_slice()[first as usize];
        assert_eq!(stored.albedo, expected.albedo);
        assert_eq!(stored.params, expected.params);
    }

    fn malformed_chunk_at(pos: IVec3) -> VoxelChunk {
        VoxelChunk::new(
            pos,
            vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]],
            vec![[0.0, 1.0, 0.0]],
            vec![0.25, 0.3, 0.35],
            vec![3, 4, 5],
            vec![0.6, 0.6, 0.6],
            vec![[0.2, 0.9, 0.5], [0.1, 0.1, 0.1], [0.4, 0.3, 0.2]],
            vec![0.75, 0.8, 0.85],
            vec![0, 1, 2],
            1,
        )
    }

    #[test]
    fn refused_chunk_mesh_drops_its_collider() {
        let mut app = App::headless();
        let pos = IVec3::new(2, 0, 2);
        insert_chunk(&mut app, chunk_at(pos));
        assert!(collider_of(&app, pos).is_some(), "collider before refusal");

        insert_chunk(&mut app, malformed_chunk_at(pos));

        assert!(
            collider_of(&app, pos).is_none(),
            "no collider after refusal"
        );
        assert_eq!(terrain_collider_count(&app), 0, "no terrain collider left");
    }

    #[test]
    fn refused_chunk_mesh_stops_drawing_the_previous_one() {
        let mut app = App::headless();
        let pos = IVec3::new(2, 0, 2);
        insert_chunk(&mut app, chunk_at(pos));
        let mut backend = RecordingBackend::default();
        app.scene.world_meshes_mut().flush(&mut backend);
        let old = backend.registered[0];
        assert_eq!(
            drawn_handles(&mut app),
            vec![old],
            "drawn before the refusal"
        );
        let malformed = malformed_chunk_at(pos);
        assert!(
            chunk_world_mesh(&malformed).is_err(),
            "the mesh is malformed"
        );

        insert_chunk(&mut app, malformed);
        app.scene.world_meshes_mut().flush(&mut backend);

        assert!(drawn_handles(&mut app).is_empty(), "nothing is drawn");
        assert_eq!(backend.unregistered, vec![old], "the old mesh was freed");
        assert_eq!(backend.registered.len(), 1, "no replacement was uploaded");
    }

    #[test]
    fn loading_a_scene_stops_drawing_the_previous_worlds_chunks() {
        let temp = tempfile::tempdir().expect("temp dir");
        let mut saved = App::headless();
        saved.entities.chunks.insert(chunk_at(IVec3::new(7, 0, 7)));
        crate::app::autosave::auto_save_on_shutdown(
            &mut saved,
            &moho_app::GameClock::default(),
            temp.path(),
            &[],
        )
        .expect("autosave");
        let mut app = App::headless();
        insert_chunk(&mut app, chunk_at(IVec3::new(1, 0, 1)));
        let mut backend = RecordingBackend::default();
        app.scene.world_meshes_mut().flush(&mut backend);
        let old = backend.registered[0];
        assert_eq!(drawn_handles(&mut app), vec![old], "drawn before the load");

        crate::app::scene_loader::load_scene(
            &mut app,
            &mut moho_app::GameClock::default(),
            &temp.path().join("scene.bin"),
        )
        .expect("load");
        app.scene.world_meshes_mut().flush(&mut backend);

        assert_eq!(backend.unregistered, vec![old], "the old chunk was freed");
        assert_eq!(backend.registered.len(), 2, "the saved chunk was uploaded");
        assert_eq!(
            drawn_handles(&mut app),
            vec![backend.registered[1]],
            "only the saved chunk is drawn"
        );
    }

    #[test]
    fn remeshing_a_chunk_to_empty_frees_its_mesh() {
        let mut app = App::headless();
        app.game_state = GameState::Playing;
        let chunk = IVec3::new(10, 4, 10);
        let block = flagstone_voxel::BlockPos::new(165, 70, 165);
        app.light_system
            .as_mut()
            .expect("App starts with a light system")
            .grid_mut()
            .mutator()
            .place(block, 1, None);
        let _ = app.world_event_rx.try_iter().count();
        let dirty = WorldEvent::ChunkMeshDirty {
            chunk_pos: chunk,
            terrain_dirty: true,
            structure_dirty: false,
        };
        app.event_bus.publish(dirty.clone());
        EventProcessor::new().process_world_events(&mut app);
        let mut backend = RecordingBackend::default();
        app.scene.world_meshes_mut().flush(&mut backend);
        assert_eq!(backend.registered.len(), 1, "the chunk is uploaded");
        assert_eq!(drawn_handles(&mut app), backend.registered);

        assert!(
            app.light_system
                .as_mut()
                .expect("light system")
                .grid_mut()
                .mutator()
                .remove(block),
            "the block was there to remove"
        );
        let _ = app.world_event_rx.try_iter().count();
        app.event_bus.publish(dirty);
        EventProcessor::new().process_world_events(&mut app);
        app.scene.world_meshes_mut().flush(&mut backend);

        assert!(
            app.entities
                .chunks
                .get(chunk)
                .is_none_or(|c| !c.has_geometry()),
            "the chunk no longer has geometry"
        );
        assert!(drawn_handles(&mut app).is_empty(), "nothing is drawn");
        assert_eq!(
            backend.unregistered, backend.registered,
            "its mesh was freed"
        );
    }

    #[test]
    fn evicted_chunk_is_not_drawn() {
        let mut app = App::headless();
        app.game_state = GameState::Playing;
        app.simulation
            .set_position_yaw_pitch(glam::Vec3::new(8.0, 80.0, 8.0), 0.0, 0.0);
        app.chunk_streamer = Some(crate::app::chunk_streamer::ChunkStreamer::new(
            flagstone_game::scene_builders::TerrainConfig::default(),
            flagstone_voxel::StreamingConfig {
                load_radius_chunks: 0,
                unload_radius_chunks: 1,
                chunks_per_frame: 1,
            },
            "headless-test-evicted-chunk-not-drawn",
        ));
        let far = IVec3::new(10, 4, 10);
        let grid = app
            .light_system
            .as_mut()
            .expect("App starts with a light system")
            .grid_mut();
        grid.mutator()
            .place(flagstone_voxel::BlockPos::new(165, 70, 165), 1, None);
        // Unmodified, so eviction doesn't write a chunk file to the working directory.
        grid.clear_chunk_modified(far);
        let _ = app.world_event_rx.try_iter().count();
        app.event_bus.publish(WorldEvent::ChunkMeshDirty {
            chunk_pos: far,
            terrain_dirty: true,
            structure_dirty: false,
        });
        EventProcessor::new().process_world_events(&mut app);
        assert!(
            app.entities
                .chunks
                .get(far)
                .is_some_and(VoxelChunk::has_geometry),
            "the placed block meshes into a chunk with geometry"
        );
        let mut backend = RecordingBackend::default();
        app.scene.world_meshes_mut().flush(&mut backend);
        assert_eq!(backend.registered.len(), 1, "the chunk's mesh is uploaded");
        let handle = backend.registered[0];
        assert_eq!(
            app.scene
                .world_meshes_mut()
                .draws()
                .map(|(h, _)| h)
                .collect::<Vec<_>>(),
            vec![handle],
            "the chunk is drawn while loaded"
        );

        FrameProcessor::new().update_chunk_streaming(&mut app);
        app.scene.world_meshes_mut().flush(&mut backend);

        assert!(app.entities.chunks.get(far).is_none(), "chunk was evicted");
        assert_eq!(
            app.scene.world_meshes_mut().draws().count(),
            0,
            "nothing is drawn for the evicted chunk"
        );
        assert_eq!(backend.unregistered, vec![handle], "its GPU mesh was freed");
    }

    #[test]
    fn caps_use_the_cut_face_material() {
        let mut app = ground_app();
        let mut backend = CaptureBackend::default();
        app.cut_caps.set_cut(Some(18.0));
        refresh_caps(&mut app);
        app.scene.world_meshes_mut().flush(&mut backend);

        let caps = drawn_caps(&mut app, &backend);

        assert!(!caps.is_empty(), "the cut through the ground is capped");
        let dirt = glam::Vec3::new(0.6, 0.5, 0.4);
        let expected = MaterialType::VoxelTerrain {
            top_albedo: dirt,
            side_albedo: dirt,
        }
        .to_gpu();
        for cap in caps {
            let stored = app.scene.material_table.as_slice()[cap.material as usize];
            assert_ne!(
                cap.material, app.terrain_material_idx,
                "not the grass terrain"
            );
            assert_eq!(stored.albedo, expected.albedo, "cut-face albedo");
            assert_eq!(stored.params, expected.params, "cut-face params");
        }
    }

    #[test]
    fn caps_register_no_collider() {
        let mut app = ground_app();
        let colliders_before = terrain_collider_count(&app);
        assert!(colliders_before > 0, "the ground has colliders");
        let mut backend = CaptureBackend::default();

        app.cut_caps.set_cut(Some(18.0));
        refresh_caps(&mut app);
        app.scene.world_meshes_mut().flush(&mut backend);

        assert!(
            !drawn_caps(&mut app, &backend).is_empty(),
            "the cut through the ground is capped"
        );
        let physics = app.physics.world.as_ref().expect("physics world");
        for chunk in app.entities.chunks.iter() {
            assert!(
                physics
                    .world_mesh_collider(cap_mesh_id(chunk.chunk_pos()))
                    .is_none(),
                "cap of {} has a collider",
                chunk.chunk_pos()
            );
        }
        assert_eq!(
            terrain_collider_count(&app),
            colliders_before,
            "no new collider"
        );
    }

    #[test]
    fn removing_every_chunk_mesh_removes_the_caps() {
        let mut app = ground_app();
        let mut backend = CaptureBackend::default();
        cut_and_draw(&mut app, &mut backend, Some(18.0));
        assert_ground_capped_at(&mut app, &backend, 18.0);

        remove_all_chunk_meshes(&mut app);
        app.scene.world_meshes_mut().flush(&mut backend);

        assert!(drawn_caps(&mut app, &backend).is_empty(), "no cap is drawn");
        assert_eq!(
            app.scene.world_meshes_mut().draws().count(),
            0,
            "nothing is drawn"
        );
    }

    #[test]
    fn upserting_every_chunk_rebuilds_the_caps() {
        let mut app = ground_app();
        let mut backend = CaptureBackend::default();
        cut_and_draw(&mut app, &mut backend, Some(18.0));
        remove_all_chunk_meshes(&mut app);
        app.scene.world_meshes_mut().flush(&mut backend);
        assert!(drawn_caps(&mut app, &backend).is_empty(), "caps removed");

        upsert_all_chunks(&mut app);
        refresh_caps(&mut app);
        app.scene.world_meshes_mut().flush(&mut backend);

        assert_ground_capped_at(&mut app, &backend, 18.0);
    }

    fn collider_of(app: &App, pos: IVec3) -> Option<moho_physics::ColliderHandle> {
        app.physics
            .world
            .as_ref()
            .expect("physics world")
            .world_mesh_collider(chunk_mesh_id(pos))
    }

    pub(crate) fn terrain_collider_count(app: &App) -> usize {
        let pw = app.physics.world.as_ref().expect("physics world");
        pw.collider_set.len() - usize::from(app.physics.player.is_some())
    }

    /// Places `block`, then drains stale events and meshes `chunk` from the dirty event.
    /// The chunk's modified flag is cleared so streaming treats it as unmodified.
    fn remesh_with_block(app: &mut App, block: flagstone_voxel::BlockPos, chunk: IVec3) {
        let grid = app.light_system.as_mut().expect("light system").grid_mut();
        grid.mutator().place(block, 1, None);
        grid.clear_chunk_modified(chunk);
        let _ = app.world_event_rx.try_iter().count();
        app.event_bus.publish(WorldEvent::ChunkMeshDirty {
            chunk_pos: chunk,
            terrain_dirty: true,
            structure_dirty: false,
        });
        EventProcessor::new().process_world_events(app);
    }

    #[test]
    fn remesh_updates_draw_and_collider_together() {
        let mut app = App::headless();
        app.game_state = GameState::Playing;
        let chunk = IVec3::new(10, 4, 10);
        remesh_with_block(
            &mut app,
            flagstone_voxel::BlockPos::new(165, 70, 165),
            chunk,
        );
        let mut backend = RecordingBackend::default();
        app.scene.world_meshes_mut().flush(&mut backend);
        let first = collider_of(&app, chunk).expect("collider after first mesh");
        assert_eq!(drawn_handles(&mut app), backend.registered);
        let first_triangles = app
            .entities
            .chunks
            .get(chunk)
            .expect("chunk")
            .indices()
            .len()
            / 3;

        remesh_with_block(
            &mut app,
            flagstone_voxel::BlockPos::new(166, 70, 165),
            chunk,
        );
        app.scene.world_meshes_mut().flush(&mut backend);

        let second = collider_of(&app, chunk).expect("collider after remesh");
        let second_triangles = app
            .entities
            .chunks
            .get(chunk)
            .expect("chunk")
            .indices()
            .len()
            / 3;
        let collider_triangles = app
            .physics
            .world
            .as_ref()
            .expect("physics world")
            .collider_set[second]
            .shape()
            .as_trimesh()
            .expect("terrain collider is a trimesh")
            .num_triangles();
        assert_ne!(first, second, "the collider was replaced");
        assert_ne!(
            first_triangles, second_triangles,
            "the remesh changed the geometry"
        );
        assert_eq!(
            collider_triangles, second_triangles,
            "the collider matches the new mesh"
        );
        assert_eq!(backend.registered.len(), 2);
        assert_eq!(drawn_handles(&mut app), vec![backend.registered[1]]);
        assert_eq!(terrain_collider_count(&app), 1, "the old collider is gone");
    }

    #[test]
    fn eviction_frees_draw_and_collider() {
        let mut app = App::headless();
        app.game_state = GameState::Playing;
        app.simulation
            .set_position_yaw_pitch(glam::Vec3::new(8.0, 80.0, 8.0), 0.0, 0.0);
        app.chunk_streamer = Some(crate::app::chunk_streamer::ChunkStreamer::new(
            flagstone_game::scene_builders::TerrainConfig::default(),
            flagstone_voxel::StreamingConfig {
                load_radius_chunks: 0,
                unload_radius_chunks: 1,
                chunks_per_frame: 1,
            },
            "headless-test-eviction-frees-draw-and-collider",
        ));
        let far = IVec3::new(10, 4, 10);
        remesh_with_block(&mut app, flagstone_voxel::BlockPos::new(165, 70, 165), far);
        let mut backend = RecordingBackend::default();
        app.scene.world_meshes_mut().flush(&mut backend);
        assert!(collider_of(&app, far).is_some(), "collider while loaded");
        assert_eq!(terrain_collider_count(&app), 1);
        assert_eq!(drawn_handles(&mut app), backend.registered);

        FrameProcessor::new().update_chunk_streaming(&mut app);
        app.scene.world_meshes_mut().flush(&mut backend);

        assert!(app.entities.chunks.get(far).is_none(), "chunk was evicted");
        assert!(drawn_handles(&mut app).is_empty(), "nothing is drawn");
        assert!(collider_of(&app, far).is_none(), "its collider was freed");
        assert_eq!(terrain_collider_count(&app), 0);
    }
}
