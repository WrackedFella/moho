//! Hands voxel chunk meshes to the renderer through the world-geometry contract.

use crate::App;
use crate::app::chunk_streamer::MIN_CHUNK_Y;
use crate::app::cut_caps::CutCaps;
use crate::app::event_loop::event_processor::{lod_for_chunk, lod_player_chunk};
use flagstone_voxel::{
    DIRT, GRASS, MaterialRegistry, MaterialType, VoxelChunk, chunks_meshed_from,
};
use glam::IVec3;
use moho_physics::{HitTag, PhysicsWorld};
use moho_render_api::{WorldMesh, WorldMeshError, WorldMeshId};
use moho_renderer::{MaterialTable, Scene};

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

/// Voxel material id to material-table index; ids it does not know map to the fallback.
#[derive(Debug, Clone)]
pub struct TerrainPalette {
    indices: Vec<u32>,
    fallback: u32,
}

impl TerrainPalette {
    /// Register every material of `registry` in `material_table` and remember where each
    /// landed; ids outside the registry map to `fallback`.
    pub fn new(
        registry: &MaterialRegistry,
        material_table: &mut MaterialTable,
        fallback: u32,
    ) -> Self {
        let indices = (0..registry.len() as u32)
            .map(|id| material_table.find_or_push(registry.material(id).expect("registered")))
            .collect();
        Self { indices, fallback }
    }

    /// The material-table index voxel material `voxel_material` draws with.
    pub fn index(&self, voxel_material: u32) -> u32 {
        self.indices
            .get(voxel_material as usize)
            .copied()
            .unwrap_or(self.fallback)
    }

    /// `mesh` with each vertex's voxel material in `voxel_materials` mapped to its table
    /// index; an empty slice (no per-vertex materials) leaves the mesh unchanged.
    ///
    /// # Errors
    /// `WorldMeshError` when the slice is non-empty and not one entry per vertex.
    pub fn apply(
        &self,
        mesh: WorldMesh,
        voxel_materials: &[u32],
    ) -> Result<WorldMesh, WorldMeshError> {
        if voxel_materials.is_empty() {
            return Ok(mesh);
        }
        mesh.with_materials(voxel_materials.iter().map(|&m| self.index(m)).collect())
    }
}

/// The chunk's mesh in the world-geometry contract's form, with each vertex's material
/// mapped through `palette` when the chunk carries per-vertex materials.
pub fn chunk_world_mesh(
    chunk: &VoxelChunk,
    palette: &TerrainPalette,
) -> Result<WorldMesh, WorldMeshError> {
    let mesh = WorldMesh::new(
        chunk.vertices().to_vec(),
        chunk.normals().to_vec(),
        chunk.ambient_occlusion().to_vec(),
        chunk.block_light_rgb().to_vec(),
        chunk.sky_exposed().to_vec(),
        chunk.geometry_type().to_vec(),
        chunk.indices().to_vec(),
    )?;
    palette.apply(mesh, chunk.materials())
}

/// The strategy game's terrain tag; it has no ray consumer yet, so the value only satisfies
/// the registration signature.
const TERRAIN_TAG: HitTag = HitTag(0);

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
    palette: &TerrainPalette,
    chunk: &VoxelChunk,
) {
    let id = chunk_mesh_id(chunk.chunk_pos());
    match chunk_world_mesh(chunk, palette) {
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
///
/// A chunk the grid stores also draws the faces it owes the empty chunks behind it; see
/// [`remesh_unstored_behind`].
pub fn insert_chunk(app: &mut App, chunk: VoxelChunk) -> Option<VoxelChunk> {
    let pos = chunk.chunk_pos();
    upsert_chunk_mesh(
        &mut app.scene,
        app.physics.world.as_mut(),
        &mut app.cut_caps,
        app.terrain_material_idx,
        &app.terrain_palette,
        &chunk,
    );
    let previous = app.entities.chunks.insert(chunk);
    if grid_stores(app, pos) {
        remesh_unstored_behind(app, pos);
    }
    previous
}

/// Take the chunk at `pos` out of the store, the renderer and physics, with its cap.
/// The empty chunks behind it are remeshed, so a face it owed them leaves with it.
pub fn remove_chunk(app: &mut App, pos: IVec3) -> Option<VoxelChunk> {
    remove_chunk_mesh(
        &mut app.scene,
        app.physics.world.as_mut(),
        &mut app.cut_caps,
        pos,
    );
    let removed = app.entities.chunks.remove(pos);
    remesh_unstored_behind(app, pos);
    removed
}

fn grid_stores(app: &App, pos: IVec3) -> bool {
    app.light_system
        .as_ref()
        .is_some_and(|light| light.grid().has_chunk(pos))
}

/// A chunk's first-layer faces are drawn by the chunks behind it, and the game meshes only
/// chunks the grid stores. So mesh each chunk behind `pos` that the grid does not store,
/// and not below the world floor: with geometry it is drawn and solid, without it leaves
/// the store, the renderer and physics.
fn remesh_unstored_behind(app: &mut App, pos: IVec3) {
    let Some(light) = app.light_system.as_ref() else {
        return;
    };
    let grid = light.grid();
    let player_chunk = lod_player_chunk(app.simulation.position());
    let behind: Vec<(IVec3, Option<VoxelChunk>)> =
        chunks_meshed_from(pos * grid.chunk_size(), grid.chunk_size())
            .filter(|&behind| behind != pos && behind.y >= MIN_CHUNK_Y && !grid.has_chunk(behind))
            .map(|behind| {
                let lod = lod_for_chunk(behind, player_chunk);
                let chunk = VoxelChunk::from_grid_lod(grid, behind, lod);
                (behind, chunk.has_geometry().then_some(chunk))
            })
            .collect();
    for (behind, chunk) in behind {
        if let Some(chunk) = chunk {
            // Unstored in the grid, so this does not recurse into the pass.
            insert_chunk(app, chunk);
        } else if app.entities.chunks.remove(behind).is_some() {
            remove_chunk_mesh(
                &mut app.scene,
                app.physics.world.as_mut(),
                &mut app.cut_caps,
                behind,
            );
        }
    }
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
            &app.terrain_palette,
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
        CaptureBackend, assert_ground_capped_at, cut_and_draw, drawn_caps, every_drawn_cap,
        ground_app, offset_palette, refresh_caps,
    };
    use crate::app::event_loop::event_processor::EventProcessor;
    use crate::app::event_loop::frame_processor::FrameProcessor;
    use crate::game_state::GameState;
    use flagstone_voxel::{CRYSTAL_MATERIAL_ID, WorldEvent};
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
        /// The `material` argument of each `register_indexed_mesh` call.
        pub(crate) materials: Vec<Vec<u32>>,
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
            material: &[u32],
            _indices: &[u32],
        ) -> u32 {
            self.next_handle += 1;
            self.materials.push(material.to_vec());
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
        fn set_shadow_quality(&mut self, quality: u8) -> u8 {
            quality
        }
        fn set_ssao_quality(&mut self, _quality: u8) {}
    }

    fn drawn_handles(app: &mut App) -> Vec<u32> {
        app.scene
            .world_meshes_mut()
            .draws()
            .map(|(h, _)| h)
            .collect()
    }

    fn palette() -> TerrainPalette {
        let mut scene = Scene::new();
        let fallback = register_terrain_material(&mut scene);
        TerrainPalette::new(
            &MaterialRegistry::new(),
            &mut scene.material_table,
            fallback,
        )
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

        let mesh = chunk_world_mesh(&chunk, &palette()).expect("well-formed chunk");

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
            chunk_world_mesh(&malformed, &palette()).is_err(),
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
        saved.generation.last_spec = Some(flagstone_game::scene_builders::WorldSpec::default());
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
        let loaded = app
            .entities
            .chunks
            .iter()
            .filter(|chunk| !chunk.is_empty())
            .count();
        assert_eq!(
            backend.registered.len(),
            1 + loaded,
            "every loaded chunk was uploaded, the saved one and the spawn area"
        );
        assert_eq!(
            drawn_handles(&mut app),
            backend.registered[1..].to_vec(),
            "only the loaded world is drawn"
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
            "saves",
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

        assert!(
            every_drawn_cap(&mut app, &backend).is_empty(),
            "no cap is drawn"
        );
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
        assert!(
            every_drawn_cap(&mut app, &backend).is_empty(),
            "caps removed"
        );

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
        pw.collider_set.len() - app.physics.pawns.len()
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
            "saves",
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

    /// Fills the blocks `min..=max` with grass-height smooth ground, in the grid only.
    fn fill_ground(app: &mut App, min: IVec3, max: IVec3) {
        app.light_system
            .as_mut()
            .expect("light system")
            .grid_mut()
            .mutator()
            .fill_region(min, max, 0);
    }

    /// Meshes the chunk at `pos` from the grid at full detail and hands it to the world.
    fn insert_grid_chunk(app: &mut App, pos: IVec3) {
        let grid = app.light_system.as_ref().expect("light system").grid();
        let chunk = VoxelChunk::from_grid_lod(grid, pos, 0);
        insert_chunk(app, chunk);
    }

    fn has_geometry_at(app: &App, pos: IVec3) -> bool {
        app.entities
            .chunks
            .get(pos)
            .is_some_and(VoxelChunk::has_geometry)
    }

    /// The chunk's vertices on the plane `axis = at`.
    fn vertices_on(app: &App, pos: IVec3, axis: usize, at: f32) -> usize {
        app.entities.chunks.get(pos).map_or(0, |chunk| {
            chunk
                .vertices()
                .iter()
                .filter(|v| (v[axis] - at).abs() < 1e-3)
                .count()
        })
    }

    #[test]
    fn drawing_ground_draws_its_face_in_the_empty_chunk_behind_it() {
        let mut app = App::headless();
        fill_ground(&mut app, IVec3::new(16, 0, 0), IVec3::new(31, 31, 31));
        assert!(!grid_stores(&app, IVec3::ZERO), "the chunk behind is empty");

        insert_grid_chunk(&mut app, IVec3::new(1, 0, 0));

        assert!(
            has_geometry_at(&app, IVec3::ZERO),
            "the empty chunk draws the ground's face"
        );
        let chunk = app.entities.chunks.get(IVec3::ZERO).expect("chunk stored");
        let on_seam = vertices_on(&app, IVec3::ZERO, 0, 16.0);
        assert_eq!(
            on_seam,
            chunk.vertices().len(),
            "every vertex lies on the seam"
        );
        assert!(
            collider_of(&app, IVec3::ZERO).is_some(),
            "the face is solid"
        );
        assert!(
            !grid_stores(&app, IVec3::ZERO),
            "the grid still holds no block there"
        );
    }

    #[test]
    fn a_remeshed_chunk_draws_its_face_in_the_empty_chunk_behind_it() {
        let mut app = App::headless();
        app.game_state = GameState::Playing;
        let ground = IVec3::new(1, 0, 0);
        fill_ground(&mut app, IVec3::new(16, 0, 0), IVec3::new(31, 15, 15));
        app.light_system
            .as_mut()
            .expect("light system")
            .grid_mut()
            .clear_chunk_modified(ground);
        let _ = app.world_event_rx.try_iter().count();
        app.event_bus.publish(WorldEvent::ChunkMeshDirty {
            chunk_pos: ground,
            terrain_dirty: true,
            structure_dirty: false,
        });

        EventProcessor::new().process_world_events(&mut app);

        assert!(has_geometry_at(&app, ground), "the ground is drawn");
        assert!(
            has_geometry_at(&app, IVec3::ZERO),
            "the empty chunk draws the ground's face"
        );
        assert!(
            vertices_on(&app, IVec3::ZERO, 0, 16.0) > 0,
            "the face sits on the seam"
        );
        assert!(
            collider_of(&app, IVec3::ZERO).is_some(),
            "the face is solid"
        );
    }

    #[test]
    fn removing_ground_takes_its_face_out_of_the_empty_chunk_behind_it() {
        let mut app = App::headless();
        let ground = IVec3::new(1, 0, 0);
        fill_ground(&mut app, IVec3::new(16, 0, 0), IVec3::new(31, 15, 15));
        insert_grid_chunk(&mut app, ground);
        assert!(
            has_geometry_at(&app, IVec3::ZERO),
            "the face is drawn first"
        );
        assert!(collider_of(&app, IVec3::ZERO).is_some(), "and solid");
        app.light_system
            .as_mut()
            .expect("light system")
            .grid_mut()
            .remove_chunk(ground);

        remove_chunk(&mut app, ground);

        assert!(
            app.entities.chunks.get(IVec3::ZERO).is_none(),
            "the empty chunk left the store"
        );
        assert!(
            collider_of(&app, IVec3::ZERO).is_none(),
            "its collider is gone"
        );
        let mut backend = RecordingBackend::default();
        app.scene.world_meshes_mut().flush(&mut backend);
        assert!(drawn_handles(&mut app).is_empty(), "nothing is drawn");
        assert_eq!(terrain_collider_count(&app), 0, "nothing is solid");
    }

    #[test]
    fn nothing_is_drawn_below_the_world_floor() {
        let mut app = App::headless();
        fill_ground(&mut app, IVec3::ZERO, IVec3::splat(15));

        insert_grid_chunk(&mut app, IVec3::ZERO);

        assert!(
            has_geometry_at(&app, IVec3::new(-1, 0, 0)),
            "the empty chunk beside the ground draws its face"
        );
        let floor = crate::app::chunk_streamer::MIN_CHUNK_Y;
        for x in -1..=0 {
            for z in -1..=0 {
                let below = IVec3::new(x, floor - 1, z);
                assert!(
                    app.entities.chunks.get(below).is_none(),
                    "{below} is below the floor"
                );
                assert!(collider_of(&app, below).is_none(), "{below} is solid");
            }
        }
    }

    #[test]
    fn an_empty_chunk_draws_the_faces_of_every_chunk_in_front_of_it() {
        let mut app = App::headless();
        fill_ground(&mut app, IVec3::new(16, 0, 0), IVec3::new(31, 31, 31));
        insert_grid_chunk(&mut app, IVec3::new(1, 0, 0));
        let before = app
            .entities
            .chunks
            .get(IVec3::ZERO)
            .map_or(0, |c| c.vertices().len());
        assert!(before > 0, "the face of the chunk in +x is drawn first");
        assert_eq!(
            vertices_on(&app, IVec3::ZERO, 0, 16.0),
            before,
            "only the +x face so far"
        );
        fill_ground(&mut app, IVec3::new(0, 0, 16), IVec3::new(15, 31, 31));

        insert_grid_chunk(&mut app, IVec3::new(0, 0, 1));

        assert!(!grid_stores(&app, IVec3::ZERO), "the chunk behind is empty");
        assert!(
            vertices_on(&app, IVec3::ZERO, 0, 16.0) > 0,
            "the face of the chunk in +x is kept"
        );
        assert!(
            vertices_on(&app, IVec3::ZERO, 2, 16.0) > 0,
            "the face of the chunk in +z is added"
        );
    }

    #[test]
    fn an_empty_chunk_with_nothing_left_to_draw_leaves_the_store() {
        let mut app = App::headless();
        let ground = IVec3::new(1, 0, 0);
        fill_ground(&mut app, IVec3::new(16, 0, 0), IVec3::new(17, 15, 15));
        insert_grid_chunk(&mut app, ground);
        assert!(
            has_geometry_at(&app, IVec3::ZERO),
            "the face is drawn first"
        );
        assert!(collider_of(&app, IVec3::ZERO).is_some(), "and solid");
        let grid = app.light_system.as_mut().expect("light system").grid_mut();
        for y in 0..16 {
            for z in 0..16 {
                assert!(
                    grid.mutator().remove(IVec3::new(16, y, z)),
                    "block was there"
                );
            }
        }

        insert_grid_chunk(&mut app, ground);

        assert!(
            grid_stores(&app, ground),
            "the ground's chunk still holds blocks"
        );
        assert!(
            app.entities.chunks.get(IVec3::ZERO).is_none(),
            "the empty chunk left the store"
        );
        assert!(
            collider_of(&app, IVec3::ZERO).is_none(),
            "its collider is gone"
        );
    }

    #[test]
    fn an_empty_chunk_at_negative_coordinates_draws_the_face_toward_the_ground() {
        let mut app = App::headless();
        fill_ground(&mut app, IVec3::ZERO, IVec3::splat(15));
        let behind = IVec3::new(-1, 0, 0);

        insert_grid_chunk(&mut app, IVec3::ZERO);

        assert!(!grid_stores(&app, behind), "the chunk behind is empty");
        assert!(has_geometry_at(&app, behind), "the face is drawn");
        assert!(
            vertices_on(&app, behind, 0, 0.0) > 0,
            "the face sits on the x = 0 seam"
        );
        assert!(collider_of(&app, behind).is_some(), "the face is solid");
        let floor = crate::app::chunk_streamer::MIN_CHUNK_Y;
        for below in [IVec3::new(-1, floor - 1, 0), IVec3::new(-1, floor - 1, -1)] {
            assert!(
                app.entities.chunks.get(below).is_none(),
                "{below} is below the floor"
            );
            assert!(collider_of(&app, below).is_none(), "{below} is solid");
        }
    }

    #[test]
    fn a_drawn_empty_chunk_that_gains_blocks_stays_a_normal_stored_chunk() {
        let mut app = App::headless();
        let behind = IVec3::ZERO;
        let ground = IVec3::new(1, 0, 0);
        fill_ground(&mut app, IVec3::new(16, 0, 0), IVec3::new(31, 31, 31));
        insert_grid_chunk(&mut app, ground);
        assert!(
            has_geometry_at(&app, behind),
            "drawn as an empty chunk first"
        );
        fill_ground(&mut app, IVec3::ZERO, IVec3::splat(15));
        insert_grid_chunk(&mut app, behind);
        assert!(grid_stores(&app, behind), "the chunk now holds blocks");
        let stored_vertices = app
            .entities
            .chunks
            .get(behind)
            .expect("stored")
            .vertices()
            .len();

        insert_grid_chunk(&mut app, ground);

        let at_behind = |app: &App| {
            app.entities
                .chunks
                .iter()
                .filter(|c| c.chunk_pos() == behind)
                .count()
        };
        assert_eq!(at_behind(&app), 1, "one chunk at the position");
        assert!(has_geometry_at(&app, behind), "still drawn");
        assert!(collider_of(&app, behind).is_some(), "still solid");
        assert_eq!(
            app.entities
                .chunks
                .get(behind)
                .expect("stored")
                .vertices()
                .len(),
            stored_vertices,
            "not replaced by an unstored-style remesh"
        );
        app.light_system
            .as_mut()
            .expect("light system")
            .grid_mut()
            .remove_chunk(ground);

        remove_chunk(&mut app, ground);

        assert_eq!(at_behind(&app), 1, "one chunk at the position");
        assert!(has_geometry_at(&app, behind), "still drawn");
        assert!(collider_of(&app, behind).is_some(), "still solid");
        assert_eq!(
            app.entities
                .chunks
                .get(behind)
                .expect("stored")
                .vertices()
                .len(),
            stored_vertices,
            "not replaced by an unstored-style remesh"
        );
    }

    #[test]
    fn two_grounds_removed_in_one_frame_leave_nothing_behind() {
        let mut app = App::headless();
        let first = IVec3::new(1, 0, 0);
        let second = IVec3::new(0, 0, 1);
        fill_ground(&mut app, IVec3::new(16, 0, 0), IVec3::new(31, 15, 15));
        fill_ground(&mut app, IVec3::new(0, 0, 16), IVec3::new(15, 15, 31));
        insert_grid_chunk(&mut app, first);
        insert_grid_chunk(&mut app, second);
        assert!(
            has_geometry_at(&app, IVec3::ZERO),
            "the face is drawn first"
        );
        for pos in [first, second] {
            app.light_system
                .as_mut()
                .expect("light system")
                .grid_mut()
                .remove_chunk(pos);
        }

        remove_chunk(&mut app, first);
        remove_chunk(&mut app, second);

        assert!(
            app.entities.chunks.get(IVec3::ZERO).is_none(),
            "the empty chunk left the store"
        );
        assert!(
            collider_of(&app, IVec3::ZERO).is_none(),
            "its collider is gone"
        );
        assert_eq!(app.entities.chunks.len(), 0, "no chunk is left");
        assert_eq!(terrain_collider_count(&app), 0, "nothing is solid");
        let mut backend = RecordingBackend::default();
        app.scene.world_meshes_mut().flush(&mut backend);
        assert!(drawn_handles(&mut app).is_empty(), "nothing is drawn");
    }

    /// Stone with soil on part of its top and an iron-ore block in it, meshed as a chunk.
    fn stone_soil_ore_chunk() -> VoxelChunk {
        let mut grid = flagstone_voxel::VoxelGrid::new(16);
        let mut mutator = grid.mutator();
        mutator.fill_region(IVec3::new(2, 0, 2), IVec3::new(9, 3, 9), 2);
        mutator.fill_region(IVec3::new(2, 4, 2), IVec3::new(5, 4, 9), 1);
        mutator.place(flagstone_voxel::BlockPos::new(8, 4, 8), 2, Some(1));
        VoxelChunk::from_grid_lod(&grid, IVec3::ZERO, 0)
    }

    #[test]
    fn chunk_mesh_vertices_map_to_their_block_materials_colours() {
        let mut app = App::headless();
        offset_palette(&mut app);
        let chunk = stone_soil_ore_chunk();
        let registry = flagstone_voxel::MaterialRegistry::new();

        let mesh = chunk_world_mesh(&chunk, &app.terrain_palette).expect("well-formed chunk");

        let drawn = mesh
            .materials()
            .expect("the chunk carries per-vertex materials");
        assert_eq!(drawn.len(), mesh.positions().len());
        let mut voxel_materials = HashSet::new();
        for (vertex, (&voxel, &idx)) in chunk.materials().iter().zip(drawn).enumerate() {
            let expected = registry.material(voxel).expect("registered").to_gpu();
            let stored = app.scene.material_table.as_slice()[idx as usize];
            assert_eq!(stored.albedo, expected.albedo, "vertex {vertex} albedo");
            assert_eq!(stored.params, expected.params, "vertex {vertex} params");
            voxel_materials.insert(voxel);
        }
        assert_eq!(
            voxel_materials,
            HashSet::from([1, 2, 3]),
            "soil, stone and ore all show"
        );
    }

    #[test]
    fn unknown_material_id_draws_in_the_terrain_material() {
        let app = App::headless();
        let mut grid = flagstone_voxel::VoxelGrid::new(16);
        let mut mutator = grid.mutator();
        mutator.fill_region(IVec3::new(2, 0, 2), IVec3::new(6, 2, 6), 2);
        mutator.place(flagstone_voxel::BlockPos::new(9, 1, 9), 50, None);
        mutator.place(flagstone_voxel::BlockPos::new(12, 1, 12), 100, None);
        let chunk = VoxelChunk::from_grid_lod(&grid, IVec3::ZERO, 0);

        let mesh = chunk_world_mesh(&chunk, &app.terrain_palette).expect("well-formed chunk");

        let drawn = mesh.materials().expect("per-vertex materials");
        for id in [50, 100, 5, u32::MAX] {
            assert_eq!(
                app.terrain_palette.index(id),
                app.terrain_material_idx,
                "unregistered id {id}"
            );
        }
        assert_ne!(
            app.terrain_palette.index(2),
            app.terrain_material_idx,
            "stone has its own entry"
        );
        let mut unknown = 0;
        for (&voxel, &idx) in chunk.materials().iter().zip(drawn) {
            if voxel == 50 || voxel == 100 {
                unknown += 1;
                assert_eq!(idx, app.terrain_material_idx, "voxel material {voxel}");
            }
        }
        assert!(unknown > 0, "the unregistered blocks are meshed");
    }

    #[test]
    fn terrain_palette_maps_each_registered_material_to_its_table_entry() {
        let mut scene = Scene::new();
        let registry = MaterialRegistry::new();
        let fallback = register_terrain_material(&mut scene);

        let palette = TerrainPalette::new(&registry, &mut scene.material_table, fallback);

        assert_eq!(registry.len(), 5);
        for id in 0..registry.len() as u32 {
            let expected = registry.material(id).expect("registered").to_gpu();
            let stored = scene.material_table.as_slice()[palette.index(id) as usize];
            assert_eq!(stored.albedo, expected.albedo, "material {id} albedo");
            assert_eq!(stored.params, expected.params, "material {id} params");
        }
        let distinct: HashSet<u32> = (0..5).map(|id| palette.index(id)).collect();
        assert_eq!(distinct.len(), 5, "five materials, five table entries");
    }

    #[test]
    fn terrain_palette_draws_the_crystal_as_an_emissive_entry() {
        let mut scene = Scene::new();
        let fallback = register_terrain_material(&mut scene);

        let palette = TerrainPalette::new(
            &MaterialRegistry::new(),
            &mut scene.material_table,
            fallback,
        );

        let stored = scene.material_table.as_slice()[palette.index(CRYSTAL_MATERIAL_ID) as usize];
        assert!(stored.params[3] > 0.0, "the crystal ignores scene lighting");
    }

    #[test]
    fn terrain_palette_reuses_the_terrain_and_cut_face_entries_for_grass_and_dirt() {
        let mut scene = Scene::new();
        let terrain = register_terrain_material(&mut scene);
        let cut_face = register_cut_face_material(&mut scene);

        let palette =
            TerrainPalette::new(&MaterialRegistry::new(), &mut scene.material_table, terrain);

        assert_eq!(palette.index(0), terrain, "grass");
        assert_eq!(palette.index(1), cut_face, "dirt");
        assert_eq!(
            scene.material_table.as_slice().len(),
            5,
            "stone, ore and crystal are new"
        );
    }

    #[test]
    fn chunk_from_scene_file_draws_without_materials() {
        let temp = tempfile::tempdir().expect("temp dir");
        let mut saved = App::headless();
        saved.generation.last_spec = Some(flagstone_game::scene_builders::WorldSpec::default());
        saved.entities.chunks.insert(chunk_at(IVec3::new(7, 0, 7)));
        crate::app::autosave::auto_save_on_shutdown(
            &mut saved,
            &moho_app::GameClock::default(),
            temp.path(),
            &[],
        )
        .expect("autosave");
        let mut app = App::headless();

        crate::app::scene_loader::load_scene(
            &mut app,
            &mut moho_app::GameClock::default(),
            &temp.path().join("scene.bin"),
        )
        .expect("load");

        let chunk = app
            .entities
            .chunks
            .get(IVec3::new(7, 0, 7))
            .expect("the saved chunk is loaded");
        assert!(
            chunk.materials().is_empty(),
            "a scene file stores no materials"
        );
        let mesh = chunk_world_mesh(chunk, &app.terrain_palette).expect("well-formed chunk");
        assert!(mesh.materials().is_none());
    }

    #[test]
    fn chunk_without_materials_uploads_the_terrain_material_on_every_vertex() {
        let mut app = App::headless();
        insert_chunk(&mut app, chunk_at(IVec3::new(1, 0, 1)));
        let mut backend = RecordingBackend::default();

        app.scene.world_meshes_mut().flush(&mut backend);

        assert_eq!(backend.materials, vec![vec![app.terrain_material_idx; 3]]);
    }

    #[test]
    fn chunk_with_materials_uploads_its_palette_indices() {
        let mut app = App::headless();
        offset_palette(&mut app);
        let chunk = stone_soil_ore_chunk();
        let expected: Vec<u32> = chunk
            .materials()
            .iter()
            .map(|&voxel| app.terrain_palette.index(voxel))
            .collect();
        insert_chunk(&mut app, chunk);
        let mut backend = RecordingBackend::default();

        app.scene.world_meshes_mut().flush(&mut backend);

        assert_eq!(backend.materials, vec![expected]);
        assert!(
            backend.materials[0]
                .iter()
                .any(|&idx| idx != app.terrain_material_idx),
            "not everything falls back to the terrain material"
        );
    }
}
