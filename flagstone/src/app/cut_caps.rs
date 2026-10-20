//! Keeps the cut-face caps of the ground in step with the view height.

use flagstone_voxel::{ChunkStore, VoxelGrid, VoxelMesh, cap_chunk_layers, cut_cap_mesh};
use glam::IVec3;
use moho_render_api::{WorldMesh, WorldMeshId};
use moho_renderer::WorldMeshes;
use std::collections::HashSet;

use super::world_geometry::chunk_mesh_id;

/// Which chunks carry a cap at the view height and which must be rebuilt.
#[derive(Debug, Default)]
pub struct CutCaps {
    cut: Option<f32>,
    /// The cut moved since the last `refresh`, which then re-picks the capped chunks.
    cut_moved: bool,
    capped: HashSet<IVec3>,
    dirty: HashSet<IVec3>,
}

impl CutCaps {
    /// The view height the caps follow; `None` when no cut is set.
    #[cfg(test)]
    pub fn cut(&self) -> Option<f32> {
        self.cut
    }

    /// Follow a new view height; `None` or a non-finite height (no cut, as the renderer
    /// reads it) removes every cap on the next `refresh`. Setting the height already
    /// followed changes nothing.
    pub fn set_cut(&mut self, cut: Option<f32>) {
        let cut = cut.filter(|cut| cut.is_finite());
        if cut != self.cut {
            self.cut = cut;
            self.cut_moved = true;
        }
    }

    /// Rebuild the cap of the chunk at `pos` on the next `refresh`.
    pub fn mark_dirty(&mut self, pos: IVec3) {
        self.dirty.insert(pos);
    }

    /// Drop the cap of the chunk at `pos` now.
    pub fn remove(&mut self, pos: IVec3, world_meshes: &mut WorldMeshes) {
        self.dirty.remove(&pos);
        self.uncap(pos, world_meshes);
    }

    fn uncap(&mut self, pos: IVec3, world_meshes: &mut WorldMeshes) {
        if self.capped.remove(&pos) {
            world_meshes.remove(cap_mesh_id(pos));
        }
    }

    /// Build the caps of dirty chunks and queue them with `world_meshes` under
    /// `cap_material`; an empty cap removes the chunk's cap.
    pub fn refresh(
        &mut self,
        grid: &VoxelGrid,
        chunks: &ChunkStore,
        world_meshes: &mut WorldMeshes,
        cap_material: u32,
    ) {
        let Some(cut) = self.cut else {
            for pos in self.capped.drain() {
                world_meshes.remove(cap_mesh_id(pos));
            }
            self.dirty.clear();
            self.cut_moved = false;
            return;
        };
        let layers = cap_chunk_layers(cut, grid.chunk_size());
        if std::mem::take(&mut self.cut_moved) {
            self.dirty.extend(self.capped.iter().copied());
            self.dirty.extend(
                chunks
                    .iter()
                    .map(flagstone_voxel::VoxelChunk::chunk_pos)
                    .filter(|pos| layers.contains(&pos.y)),
            );
        }
        for pos in std::mem::take(&mut self.dirty) {
            let chunk = chunks.get(pos).filter(|_| layers.contains(&pos.y));
            let mesh = chunk.map(|chunk| cut_cap_mesh(grid, pos, chunk.lod(), cut));
            match mesh.filter(|mesh| !mesh.indices.is_empty()) {
                Some(mesh) => {
                    world_meshes.upsert(cap_mesh_id(pos), cap_world_mesh(mesh), cap_material);
                    self.capped.insert(pos);
                }
                None => self.uncap(pos, world_meshes),
            }
        }
    }
}

/// The cap in the world-geometry contract's form.
fn cap_world_mesh(mesh: VoxelMesh) -> WorldMesh {
    WorldMesh::new(
        mesh.vertices,
        mesh.normals,
        mesh.ambient_occlusion,
        mesh.block_light_rgb,
        mesh.sky_exposed,
        mesh.geometry_type,
        mesh.indices,
    )
    .expect("cut_cap_mesh fills every vertex channel and indexes only its own vertices")
}

/// Renderer key for the cap of the chunk at `pos`; never equals a chunk's key, which
/// uses only the low 63 bits.
pub fn cap_mesh_id(pos: IVec3) -> WorldMeshId {
    WorldMeshId(chunk_mesh_id(pos).0 | 1 << 63)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::App;
    use crate::app::event_loop::frame_processor::FrameProcessor;
    use crate::app::world_geometry::{insert_chunk, remove_chunk};
    use flagstone_voxel::VoxelChunk;
    use moho_renderer::RendererBackend;
    use proptest::prelude::*;
    use std::collections::BTreeMap;

    const LIMIT: i32 = 1 << 20;
    const AREA_TOLERANCE: f32 = 1e-3;
    /// Cap area of `ground_app`'s ground at a cut inside it: the blocks span x `0..32`
    /// and z `0..16`, a chunk owns cells from half a block in, and the walls sit half a
    /// block past the last block's centre. Marching cubes cuts the one convex corner
    /// inside the caps' chunks, at x 32 and z 16, by an eighth of a block.
    const GROUND_CAP_AREA: f32 = 31.5 * 15.5 - 0.125;
    /// What a 4x4 pit takes from that cap: each of its four air corners keeps an eighth
    /// of a block of solid.
    const PIT_AREA: f32 = 16.0 - 4.0 * 0.125;
    /// The cap of the chunk at x `1`, which holds the ground's convex corner.
    const FAR_CHUNK_CAP_AREA: f32 = 15.5 * 15.5 - 0.125;

    #[derive(Default)]
    pub(crate) struct CaptureBackend {
        next_handle: u32,
        live: BTreeMap<u32, (Vec<[f32; 3]>, Vec<u32>)>,
        pub(crate) unregistered: Vec<u32>,
    }

    impl RendererBackend for CaptureBackend {
        fn resize(&mut self, _width: u32, _height: u32) {}
        fn register_mesh(&mut self, _vertices: &[[f32; 3]]) -> u32 {
            unreachable!("world meshes upload indexed meshes")
        }
        fn register_indexed_mesh(
            &mut self,
            vertices: &[[f32; 3]],
            _normals: &[[f32; 3]],
            _ao: &[f32],
            _geometry_type: &[u32],
            _light_level: &[f32],
            _block_light_rgb: &[[f32; 3]],
            _sky_exposed: &[f32],
            indices: &[u32],
        ) -> u32 {
            self.next_handle += 1;
            self.live
                .insert(self.next_handle, (vertices.to_vec(), indices.to_vec()));
            self.next_handle
        }
        fn unregister_mesh(&mut self, mesh: u32) {
            self.live.remove(&mesh);
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

    /// A drawn mesh whose vertices all share one height: a cap.
    pub(crate) struct DrawnCap {
        pub(crate) handle: u32,
        pub(crate) material: u32,
        pub(crate) y: f32,
        pub(crate) vertices: Vec<[f32; 3]>,
        indices: Vec<u32>,
    }

    impl DrawnCap {
        pub(crate) fn area(&self) -> f32 {
            self.indices
                .as_chunks::<3>()
                .0
                .iter()
                .map(|t| {
                    let [a, b, c] = t.map(|i| self.vertices[i as usize]);
                    ((b[0] - a[0]) * (c[2] - a[2]) - (c[0] - a[0]) * (b[2] - a[2])).abs() / 2.0
                })
                .sum()
        }

        fn x_range(&self) -> [f32; 2] {
            self.vertices
                .iter()
                .fold([f32::MAX, f32::MIN], |[lo, hi], v| {
                    [lo.min(v[0]), hi.max(v[0])]
                })
        }
    }

    /// The flat meshes currently drawn, in draw order.
    pub(crate) fn drawn_caps(app: &mut App, backend: &CaptureBackend) -> Vec<DrawnCap> {
        app.scene
            .world_meshes_mut()
            .draws()
            .filter_map(|(handle, instance)| {
                let (vertices, indices) = backend.live.get(&handle)?;
                let y = vertices.first()?[1];
                vertices.iter().all(|v| v[1] == y).then(|| DrawnCap {
                    handle,
                    material: instance.material,
                    y,
                    vertices: vertices.clone(),
                    indices: indices.clone(),
                })
            })
            .collect()
    }

    /// Smooth ground over blocks x `0..32`, y `0..28`, z `0..16`, loaded as the four
    /// chunks of its two columns and two layers.
    pub(crate) fn ground_app() -> App {
        let mut app = App::headless();
        let grid = app.light_system.as_mut().expect("light system").grid_mut();
        grid.mutator()
            .fill_region(IVec3::ZERO, IVec3::new(31, 27, 15), 0);
        let chunks: Vec<VoxelChunk> = (0..2)
            .flat_map(|x| (0..2).map(move |y| IVec3::new(x, y, 0)))
            .map(|pos| VoxelChunk::from_grid_lod(grid, pos, 0))
            .collect();
        for chunk in chunks {
            insert_chunk(&mut app, chunk);
        }
        app
    }

    /// The frame's cap stage, minus the flush to a renderer.
    pub(crate) fn refresh_caps(app: &mut App) {
        FrameProcessor::new().update_cut_caps(app);
    }

    pub(crate) fn cut_and_draw(app: &mut App, backend: &mut CaptureBackend, cut: Option<f32>) {
        app.cut_caps.set_cut(cut);
        refresh_caps(app);
        app.scene.world_meshes_mut().flush(backend);
    }

    pub(crate) fn assert_ground_capped_at(app: &mut App, backend: &CaptureBackend, cut: f32) {
        let caps = drawn_caps(app, backend);

        assert_eq!(caps.len(), 2, "one cap per chunk column the cut crosses");
        for cap in &caps {
            assert_eq!(cap.y, cut, "cap height");
            assert_eq!(cap.material, app.cut_face_material_idx, "cap material");
        }
        let total: f32 = caps.iter().map(DrawnCap::area).sum();
        assert!(
            (total - GROUND_CAP_AREA).abs() < AREA_TOLERANCE,
            "caps cover {total}, not {GROUND_CAP_AREA}"
        );
    }

    #[test]
    fn setting_a_cut_caps_the_chunks_it_crosses() {
        let mut app = ground_app();
        let mut backend = CaptureBackend::default();
        app.scene.world_meshes_mut().flush(&mut backend);
        assert!(drawn_caps(&mut app, &backend).is_empty(), "no cut, no cap");

        cut_and_draw(&mut app, &mut backend, Some(18.0));

        assert_eq!(app.cut_caps.cut(), Some(18.0));
        assert_ground_capped_at(&mut app, &backend, 18.0);
        for cap in drawn_caps(&mut app, &backend) {
            let [lo, hi] = cap.x_range();
            assert!(
                (0.5..=16.5).contains(&lo) && (0.5..=16.5).contains(&hi)
                    || (16.5..=32.0).contains(&lo) && (16.5..=32.0).contains(&hi),
                "cap spans x {lo}..{hi}, across a chunk border"
            );
        }
    }

    #[test]
    fn clearing_the_cut_removes_every_cap() {
        let mut app = ground_app();
        let mut backend = CaptureBackend::default();
        app.scene.world_meshes_mut().flush(&mut backend);
        let terrain_draws = app.scene.world_meshes_mut().draws().count();
        cut_and_draw(&mut app, &mut backend, Some(18.0));
        let mut cap_handles: Vec<u32> = drawn_caps(&mut app, &backend)
            .iter()
            .map(|c| c.handle)
            .collect();
        cap_handles.sort_unstable();
        assert_eq!(cap_handles.len(), 2, "the caps exist before the cut clears");

        cut_and_draw(&mut app, &mut backend, None);

        assert_eq!(app.cut_caps.cut(), None);
        assert!(drawn_caps(&mut app, &backend).is_empty(), "no cap is drawn");
        assert_eq!(
            app.scene.world_meshes_mut().draws().count(),
            terrain_draws,
            "the terrain still draws"
        );
        let mut freed = backend.unregistered.clone();
        freed.sort_unstable();
        assert_eq!(freed, cap_handles, "the caps' meshes were freed");
    }

    #[test]
    fn reinserting_a_chunk_rebuilds_its_cap() {
        let mut app = ground_app();
        let mut backend = CaptureBackend::default();
        cut_and_draw(&mut app, &mut backend, Some(18.0));
        assert_ground_capped_at(&mut app, &backend, 18.0);
        let grid = app.light_system.as_mut().expect("light system").grid_mut();
        for x in 4..=7 {
            for y in 17..=19 {
                for z in 4..=7 {
                    assert!(
                        grid.mutator().remove(IVec3::new(x, y, z)),
                        "block was there"
                    );
                }
            }
        }
        let carved = VoxelChunk::from_grid_lod(grid, IVec3::new(0, 1, 0), 0);

        insert_chunk(&mut app, carved);
        refresh_caps(&mut app);
        app.scene.world_meshes_mut().flush(&mut backend);

        let caps = drawn_caps(&mut app, &backend);
        assert_eq!(caps.len(), 2, "both chunks are still capped");
        let total: f32 = caps.iter().map(DrawnCap::area).sum();
        assert!(
            (total - (GROUND_CAP_AREA - PIT_AREA)).abs() < AREA_TOLERANCE,
            "the 4x4 pit is open in the cap: {total}"
        );
    }

    #[test]
    fn removing_a_chunk_removes_its_cap() {
        let mut app = ground_app();
        let mut backend = CaptureBackend::default();
        cut_and_draw(&mut app, &mut backend, Some(18.0));
        let before = drawn_caps(&mut app, &backend).len();
        assert_eq!(before, 2, "both chunks are capped");

        remove_chunk(&mut app, IVec3::new(0, 1, 0));
        app.scene.world_meshes_mut().flush(&mut backend);
        let right_after = drawn_caps(&mut app, &backend);
        refresh_caps(&mut app);
        app.scene.world_meshes_mut().flush(&mut backend);
        let after_refresh = drawn_caps(&mut app, &backend);

        for (when, caps) in [
            ("on removal", right_after),
            ("after a refresh", after_refresh),
        ] {
            assert_eq!(caps.len(), 1, "one cap left {when}");
            assert!(
                (caps[0].area() - FAR_CHUNK_CAP_AREA).abs() < AREA_TOLERANCE,
                "the remaining cap is the other chunk's {when}"
            );
            assert!(caps[0].x_range()[0] >= 16.5, "the other chunk's cap {when}");
        }
    }

    #[test]
    fn moving_the_cut_replaces_caps() {
        let mut app = ground_app();
        let mut backend = CaptureBackend::default();

        // 16.25 is owned by the chunk layer below the border, whose cells reach 16.5.
        for cut in [18.0, 25.0, 10.0, 16.25, 18.0, 10.0] {
            cut_and_draw(&mut app, &mut backend, Some(cut));

            assert_ground_capped_at(&mut app, &backend, cut);
        }
        cut_and_draw(&mut app, &mut backend, Some(40.0));

        assert!(
            drawn_caps(&mut app, &backend).is_empty(),
            "no ground at 40, so no cap is left from the old cut"
        );
    }

    #[test]
    fn a_non_finite_cut_caps_nothing() {
        for cut in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            let mut app = ground_app();
            let mut backend = CaptureBackend::default();
            app.scene.world_meshes_mut().flush(&mut backend);

            cut_and_draw(&mut app, &mut backend, Some(cut));

            assert!(
                drawn_caps(&mut app, &backend).is_empty(),
                "a cut of {cut} leaves a cap"
            );
        }
    }

    #[test]
    fn the_frame_stage_caps_the_ground_under_the_cut_face_material() {
        let mut app = ground_app();
        let mut backend = CaptureBackend::default();
        app.cut_caps.set_cut(Some(18.0));

        FrameProcessor::new().update_cut_caps(&mut app);
        app.scene.world_meshes_mut().flush(&mut backend);

        assert_ground_capped_at(&mut app, &backend, 18.0);
        for cap in drawn_caps(&mut app, &backend) {
            assert_ne!(cap.material, app.terrain_material_idx, "not the grass");
        }
    }

    #[test]
    fn removing_a_dirty_chunk_leaves_no_cap() {
        let mut app = ground_app();
        let mut backend = CaptureBackend::default();
        cut_and_draw(&mut app, &mut backend, Some(18.0));
        assert_eq!(drawn_caps(&mut app, &backend).len(), 2, "both capped");
        let grid = app.light_system.as_mut().expect("light system").grid_mut();
        assert!(
            grid.mutator().remove(IVec3::new(5, 18, 5)),
            "block was there"
        );
        let edited = VoxelChunk::from_grid_lod(grid, IVec3::new(0, 1, 0), 0);
        insert_chunk(&mut app, edited);

        remove_chunk(&mut app, IVec3::new(0, 1, 0));
        refresh_caps(&mut app);
        app.scene.world_meshes_mut().flush(&mut backend);

        let caps = drawn_caps(&mut app, &backend);
        assert_eq!(caps.len(), 1, "only the other chunk is capped");
        assert!(caps[0].x_range()[0] >= 16.5, "the other chunk's cap");
    }

    #[test]
    fn a_coarse_chunk_is_capped_with_its_coarse_cells() {
        let mut app = App::headless();
        let grid = app.light_system.as_mut().expect("light system").grid_mut();
        grid.mutator()
            .fill_region(IVec3::ZERO, IVec3::new(15, 15, 15), 0);
        let coarse = VoxelChunk::from_grid_lod(grid, IVec3::ZERO, 1);
        insert_chunk(&mut app, coarse);
        let mut backend = CaptureBackend::default();

        cut_and_draw(&mut app, &mut backend, Some(7.0));

        let caps = drawn_caps(&mut app, &backend);
        assert_eq!(caps.len(), 1, "the chunk is capped");
        assert!(
            (caps[0].area() - 256.0).abs() < AREA_TOLERANCE,
            "coarse cells cover the whole 16x16 chunk, not {}",
            caps[0].area()
        );
    }

    proptest! {
        #[test]
        fn cap_ids_never_equal_chunk_ids(
            a in (-LIMIT..LIMIT, -LIMIT..LIMIT, -LIMIT..LIMIT),
            b in (-LIMIT..LIMIT, -LIMIT..LIMIT, -LIMIT..LIMIT),
            step in (-1..=1i32, -1..=1i32, -1..=1i32),
        ) {
            let a = IVec3::new(a.0, a.1, a.2);
            let b = IVec3::new(b.0, b.1, b.2);
            let near = (a + IVec3::new(step.0, step.1, step.2))
                .clamp(IVec3::splat(-LIMIT), IVec3::splat(LIMIT - 1));

            prop_assert_ne!(cap_mesh_id(a), chunk_mesh_id(b));
            prop_assert_ne!(cap_mesh_id(a), chunk_mesh_id(a));
            prop_assert_eq!(a == b, cap_mesh_id(a) == cap_mesh_id(b));
            prop_assert_eq!(a == near, cap_mesh_id(a) == cap_mesh_id(near));
            prop_assert_eq!(cap_mesh_id(a).0, chunk_mesh_id(a).0 | 1 << 63);
        }
    }
}
