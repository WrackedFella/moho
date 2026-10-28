//! Unloading a level returns the physics world, the world-mesh store and the backend to
//! where they were before it loaded.

use std::collections::BTreeSet;

use glam::Vec3;
use moho_level::{Level, LevelMesh, LoadedLevel};
use moho_physics::{HitTag, PhysicsWorld, RayQuery};
use moho_render_api::{InstanceGpu, WorldMesh, WorldMeshId};
use moho_renderer::{FrameError, RendererBackend, WorldMeshes};
use proptest::prelude::*;

const TAG: HitTag = HitTag(1);
const DT: f32 = 1.0 / 60.0;

/// Records register and unregister calls; handles start at 1 and are never reused.
#[derive(Default)]
struct RecordingBackend {
    next_handle: u32,
    registered: Vec<u32>,
    unregistered: Vec<u32>,
}

impl RecordingBackend {
    fn live_handles(&self) -> BTreeSet<u32> {
        let freed: BTreeSet<u32> = self.unregistered.iter().copied().collect();
        self.registered
            .iter()
            .copied()
            .filter(|h| !freed.contains(h))
            .collect()
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
        _material: &[u32],
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
        _camera: (glam::Mat4, glam::Mat4, Vec3),
    ) -> Result<(), FrameError> {
        Ok(())
    }
    fn enqueue_draw(&mut self, _mesh: u32, _instances: &[InstanceGpu]) {}
    fn submit_frame(&mut self) {}
    fn set_materials(&mut self, _materials: &[moho_renderer::MaterialGpu]) {}
    fn update_lighting(&mut self, _lighting: moho_renderer::LightingGpu) {}
    fn add_point_light(&mut self, _position: Vec3, _color: Vec3, _intensity: f32, _range: f32) -> u32 {
        0
    }
    fn remove_light(&mut self, _id: u32) -> bool {
        true
    }
    fn set_light_position(&mut self, _id: u32, _position: Vec3) {}
    fn set_light_enabled(&mut self, _id: u32, _enabled: bool) {}
    fn set_shadow_quality(&mut self, _quality: u8) {}
    fn set_ssao_quality(&mut self, _quality: u8) {}
}

/// The three counts a leak shows up in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Counts {
    colliders: usize,
    live_meshes: usize,
    backend_meshes: usize,
}

struct Stores {
    physics: PhysicsWorld,
    meshes: WorldMeshes,
    backend: RecordingBackend,
}

impl Stores {
    fn new() -> Self {
        Self {
            physics: PhysicsWorld::new(),
            meshes: WorldMeshes::new(),
            backend: RecordingBackend::default(),
        }
    }

    fn counts(&self) -> Counts {
        Counts {
            colliders: self.physics.collider_set.len(),
            live_meshes: self.meshes.live_len(),
            backend_meshes: self.backend.live_handles().len(),
        }
    }

    /// Register `level` in physics and queue it in the store, without flushing.
    fn install(&mut self, level: Level, first: u64) -> LoadedLevel {
        let loaded = LoadedLevel::new(level, WorldMeshId(first)).expect("ids fit in u64");
        for (index, level_mesh) in loaded.level().meshes.iter().enumerate() {
            let id = loaded.mesh_id(index).expect("index is within the level");
            self.physics.set_world_mesh(id, &level_mesh.mesh, TAG);
            self.meshes.upsert(id, level_mesh.mesh.clone(), 0);
        }
        loaded
    }

    /// Remove `loaded` from physics and queue its meshes' removal, without flushing.
    fn uninstall(&mut self, loaded: LoadedLevel) {
        let range = loaded.unload();
        self.physics.remove_world_meshes(range);
        self.meshes.remove_range(range);
    }

    fn flush(&mut self) {
        self.meshes.flush(&mut self.backend);
    }

    fn load(&mut self, level: Level, first: u64) -> LoadedLevel {
        let loaded = self.install(level, first);
        self.flush();
        loaded
    }

    fn unload(&mut self, loaded: LoadedLevel) {
        self.uninstall(loaded);
        self.flush();
    }
}

/// A square floor of side 20 at height `y`, facing up.
fn floor(y: f32) -> WorldMesh {
    WorldMesh::new(
        vec![
            [-10.0, y, -10.0],
            [10.0, y, -10.0],
            [10.0, y, 10.0],
            [-10.0, y, 10.0],
        ],
        vec![[0.0, 1.0, 0.0]; 4],
        vec![1.0; 4],
        vec![[0.0; 3]; 4],
        vec![1.0; 4],
        vec![0; 4],
        vec![0, 2, 1, 0, 3, 2],
    )
    .expect("valid floor mesh")
}

/// `meshes` floors, mesh `i` at height `3 i`, built in memory.
fn level(meshes: usize) -> Level {
    Level {
        meshes: (0..meshes)
            .map(|i| LevelMesh {
                mesh: floor(3.0 * i as f32),
                material: None,
            })
            .collect(),
        markers: Vec::new(),
    }
}

const BASELINE: Counts = Counts {
    colliders: 0,
    live_meshes: 0,
    backend_meshes: 0,
};

#[test]
fn load_unload_load_unload_returns_every_count_to_baseline() {
    let mut stores = Stores::new();
    assert_eq!(stores.counts(), BASELINE);

    let a = stores.load(level(5), 1000);
    stores.unload(a);
    assert_eq!(stores.counts(), BASELINE, "after unloading A");
    let b = stores.load(level(2), 1000);
    stores.unload(b);

    assert_eq!(stores.counts(), BASELINE, "after unloading B");
    assert_eq!(
        stores.backend.registered.len(),
        7,
        "both levels were uploaded, so the baseline is not vacuous"
    );
    assert_eq!(stores.backend.unregistered.len(), 7);
}

#[test]
fn a_b_a_repeats_and_leaves_no_stale_tail() {
    let (a_len, b_len) = (5, 2);
    let mut stores = Stores::new();
    let tail_ids = || (1000 + b_len as u64..1000 + a_len as u64).map(WorldMeshId);

    let a = stores.load(level(a_len), 1000);
    stores.unload(a);
    assert_eq!(stores.counts(), BASELINE, "after A");
    let b = stores.load(level(b_len), 1000);
    assert_eq!(
        stores.counts(),
        Counts {
            colliders: b_len,
            live_meshes: b_len,
            backend_meshes: b_len
        },
        "B alone is loaded"
    );
    for id in tail_ids() {
        assert!(
            stores.physics.world_mesh_collider(id).is_none(),
            "{id:?} of A's tail has no collider"
        );
    }
    stores.unload(b);
    assert_eq!(stores.counts(), BASELINE, "after B");
    let a_again = stores.load(level(a_len), 1000);
    stores.unload(a_again);

    assert_eq!(stores.counts(), BASELINE, "after A again");
    for id in (1000..1000 + a_len as u64).map(WorldMeshId) {
        assert!(stores.physics.world_mesh_collider(id).is_none(), "{id:?}");
    }
}

#[test]
fn a_loaded_level_is_counted_by_all_three_stores() {
    let mut stores = Stores::new();

    let loaded = stores.load(level(4), 50);

    assert_eq!(
        stores.counts(),
        Counts {
            colliders: 4,
            live_meshes: 4,
            backend_meshes: 4
        }
    );
    assert_eq!(loaded.ids().len(), 4);
}

#[test]
fn an_unloaded_level_is_not_hit_after_the_next_step() {
    let mut stores = Stores::new();
    let down = RayQuery {
        origin: Vec3::new(0.0, 100.0, 0.0),
        direction: Vec3::NEG_Y,
        max_distance: 200.0,
        ignore: None,
    };
    let loaded = stores.load(level(3), 10);
    stores.physics.step(DT);
    let hit = stores
        .physics
        .cast_ray(&down)
        .expect("the ray hits A's highest floor");
    assert!((hit.distance - 94.0).abs() < 1e-3, "{hit:?}");

    stores.unload(loaded);
    stores.physics.step(DT);

    assert_eq!(stores.physics.cast_ray(&down), None);
}

#[test]
fn unload_and_install_in_one_frame_leaves_only_the_new_level() {
    let (a_len, b_len) = (5, 2);
    let mut stores = Stores::new();
    let a = stores.load(level(a_len), 1000);
    let a_handles = stores.backend.live_handles();
    assert_eq!(a_handles.len(), a_len);

    stores.uninstall(a);
    let b = stores.install(level(b_len), 1000);
    stores.flush();

    assert_eq!(
        stores.counts(),
        Counts {
            colliders: b_len,
            live_meshes: b_len,
            backend_meshes: b_len
        }
    );
    let now = stores.backend.live_handles();
    assert!(now.is_disjoint(&a_handles), "none of A's uploads survive");
    let freed: BTreeSet<u32> = stores.backend.unregistered.iter().copied().collect();
    assert!(a_handles.is_subset(&freed), "A's backend meshes are unregistered");
    for id in b.ids().iter() {
        assert!(stores.physics.world_mesh_collider(id).is_some(), "{id:?}");
    }
}

/// Level sizes: A is the largest, B smaller, C empty.
const SIZES: [usize; 3] = [5, 2, 0];
const FIRSTS: [u64; 3] = [0, 3, 1000];

proptest! {
    #[test]
    fn any_sequence_of_switches_returns_to_baseline(
        steps in prop::collection::vec((0usize..3, 0usize..3, any::<bool>()), 0..12)
    ) {
        let mut stores = Stores::new();

        for (which, first, flush_between) in steps {
            let loaded = stores.install(level(SIZES[which]), FIRSTS[first]);
            if flush_between {
                stores.flush();
            }
            stores.unload(loaded);
        }

        prop_assert_eq!(stores.counts(), BASELINE);
    }
}
