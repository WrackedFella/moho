//! Engine-drawn effects with one store, cap and unshadowed draw. A streak is placed by
//! handle: a game starts it, places it every frame and ends it. A flash or a mark is fired
//! and forgotten: it has no handle and the engine ages it out over its fixed lifetime.

use crate::WorldMeshes;
use glam::{Affine3A, Vec3};
use moho_render_api::{WorldMesh, WorldMeshId};
use std::collections::BTreeMap;
use std::time::Duration;

/// First mesh id reserved for engine effects; games keep their own ids below it.
pub const EFFECT_MESH_IDS_FROM: u64 = u64::MAX - 63;

/// Engine-issued, monotonic and never reused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EffectHandle(u64);

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StreakStyle {
    pub length: f32,
    pub width: f32,
    pub material: u32,
    pub fade: Duration,
    pub max_age: Duration,
}

impl StreakStyle {
    pub fn new(length: f32, width: f32, material: u32) -> Self {
        Self {
            length,
            width,
            material,
            fade: Duration::from_millis(50),
            max_age: Duration::from_secs(10),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FlashStyle {
    pub size: f32,
    pub material: u32,
    pub lifetime: Duration,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MarkStyle {
    pub size: f32,
    pub material: u32,
    pub lifetime: Duration,
    pub fade: Duration,
}

/// How far a mark is lifted off its surface along the normal.
pub const MARK_OFFSET: f32 = 0.01;

/// Live-effect cap per effects quality, indexed like `QUALITY_NAMES`.
const CAPS: [usize; 5] = [0, 32, 64, 128, 256];

/// The highest quality level, ultra; higher values clamp to it.
const MAX_QUALITY: u32 = 4;

/// Quality used until a game sets one; matches the default `effects_quality` pref.
const DEFAULT_QUALITY: usize = 3;

#[derive(Debug)]
struct Streak {
    style: StreakStyle,
    head: Vec3,
    /// Unit heading; `None` until a non-zero one is placed.
    heading: Option<Vec3>,
    placed_this_frame: bool,
    age: Duration,
    ended_at: Option<Duration>,
}

impl Streak {
    /// The width factor of the fade: 1 until ended, then falling linearly to 0.
    fn width_scale(&self) -> f32 {
        let Some(ended_at) = self.ended_at else {
            return 1.0;
        };
        if self.style.fade.is_zero() {
            return 0.0;
        }
        let since_end = self.age.saturating_sub(ended_at);
        (1.0 - since_end.as_secs_f32() / self.style.fade.as_secs_f32()).clamp(0.0, 1.0)
    }

    fn finished_fading(&self) -> bool {
        self.ended_at
            .is_some_and(|ended_at| self.age.saturating_sub(ended_at) >= self.style.fade)
    }
}

/// A camera-facing flash that shrinks to nothing over its lifetime.
#[derive(Debug)]
struct Flash {
    at: Vec3,
    roll: f32,
    style: FlashStyle,
    age: Duration,
}

/// A mark lying on a surface, full size until its fade, then shrinking to nothing.
#[derive(Debug)]
struct Mark {
    at: Vec3,
    /// Unit surface normal.
    normal: Vec3,
    roll: f32,
    style: MarkStyle,
    age: Duration,
}

impl Mark {
    /// The size factor: 1 until the last `fade` of the lifetime, then falling linearly to 0.
    fn size_scale(&self) -> f32 {
        let remaining = self.style.lifetime.saturating_sub(self.age);
        if remaining >= self.style.fade {
            return 1.0;
        }
        remaining.as_secs_f32() / self.style.fade.as_secs_f32()
    }
}

#[derive(Debug)]
enum Effect {
    Streak(Streak),
    Flash(Flash),
    Mark(Mark),
}

/// Each effect mesh's offset from `EFFECT_MESH_IDS_FROM`, also its slot in
/// `Effects::uploaded`.
const STREAK_MESH: u64 = 0;
const FLASH_MESH: u64 = 1;
const MARK_MESH: u64 = 2;

#[derive(Debug)]
pub struct Effects {
    store: BTreeMap<EffectHandle, Effect>,
    next_handle: u64,
    cap: usize,
    uploaded: [bool; 3],
}

impl Default for Effects {
    fn default() -> Self {
        Self {
            store: BTreeMap::new(),
            next_handle: 0,
            cap: CAPS[DEFAULT_QUALITY],
            uploaded: [false; 3],
        }
    }
}

impl Effects {
    /// Start a streak. At the quality's cap the handle is inert: never stored, so placing
    /// or ending it does nothing. A streak is never evicted.
    pub fn begin_streak(&mut self, style: StreakStyle) -> EffectHandle {
        let handle = self.issue_handle();
        if self.store.len() < self.cap {
            self.store.insert(
                handle,
                Effect::Streak(Streak {
                    style,
                    head: Vec3::ZERO,
                    heading: None,
                    placed_this_frame: false,
                    age: Duration::ZERO,
                    ended_at: None,
                }),
            );
        }
        handle
    }

    /// Spawn a flash at `at`, facing the eye and turned by `roll` about the line to it. It
    /// shrinks linearly and is gone at `style.lifetime`. Refused for a non-finite `at` or
    /// `roll`, at quality 0, and when the cap is full of streaks.
    pub fn flash(&mut self, at: Vec3, roll: f32, style: FlashStyle) {
        if at.is_finite() && roll.is_finite() {
            self.spawn_fixed(Effect::Flash(Flash {
                at,
                roll,
                style,
                age: Duration::ZERO,
            }));
        }
    }

    /// Spawn a mark at `at` lying on the surface with `normal` (any non-zero length), turned
    /// by `roll` about it and lifted by `MARK_OFFSET`. It is full size until the last
    /// `style.fade` of `style.lifetime`, then shrinks away. Refused for a zero or non-finite
    /// `normal`, a non-finite `at` or `roll`, at quality 0, and when the cap is full of
    /// streaks.
    pub fn mark(&mut self, at: Vec3, normal: Vec3, roll: f32, style: MarkStyle) {
        let Some(normal) = normal.try_normalize() else {
            return;
        };
        if at.is_finite() && roll.is_finite() {
            self.spawn_fixed(Effect::Mark(Mark {
                at,
                normal,
                roll,
                style,
                age: Duration::ZERO,
            }));
        }
    }

    fn issue_handle(&mut self) -> EffectHandle {
        let handle = EffectHandle(self.next_handle);
        self.next_handle += 1;
        handle
    }

    /// Store a fixed-lifetime effect; at the cap the lowest-handle one makes room, and with
    /// none to evict (or at quality 0) the spawn is refused.
    fn spawn_fixed(&mut self, effect: Effect) {
        if self.cap == 0 {
            return;
        }
        if self.store.len() >= self.cap {
            let oldest = self
                .store
                .iter()
                .find(|(_, e)| !matches!(e, Effect::Streak(_)))
                .map(|(handle, _)| *handle);
            let Some(oldest) = oldest else {
                return;
            };
            self.store.remove(&oldest);
        }
        let handle = self.issue_handle();
        self.store.insert(handle, effect);
    }

    /// Place the streak's head for this frame. A zero-length heading keeps the last one;
    /// a stale or unknown handle is ignored.
    pub fn place_streak(&mut self, handle: EffectHandle, head: Vec3, heading: Vec3) {
        let Some(Effect::Streak(streak)) = self.store.get_mut(&handle) else {
            return;
        };
        streak.head = head;
        streak.heading = heading.try_normalize().or(streak.heading);
        streak.placed_this_frame = true;
    }

    /// Start the fade; the streak then draws at its last placement until it is gone.
    pub fn end(&mut self, handle: EffectHandle) {
        if let Some(Effect::Streak(streak)) = self.store.get_mut(&handle) {
            streak.ended_at.get_or_insert(streak.age);
        }
    }

    /// Age every effect by the frame time, start the fade of any streak past its `max_age`
    /// and drop the finished and expired ones.
    pub fn advance(&mut self, dt: Duration) {
        for effect in self.store.values_mut() {
            match effect {
                Effect::Streak(streak) => {
                    streak.age += dt;
                    if streak.age >= streak.style.max_age {
                        streak.ended_at.get_or_insert(streak.style.max_age);
                    }
                }
                Effect::Flash(flash) => flash.age += dt,
                Effect::Mark(mark) => mark.age += dt,
            }
        }
        self.store.retain(|_, effect| match effect {
            Effect::Streak(streak) => !streak.finished_fading(),
            Effect::Flash(flash) => flash.age < flash.style.lifetime,
            Effect::Mark(mark) => mark.age < mark.style.lifetime,
        });
    }

    /// Cap new effects for `quality` (0 off to 4 ultra; higher clamps). Lowering keeps
    /// what is live; 0 clears it.
    pub fn set_quality(&mut self, quality: u32) {
        self.cap = CAPS
            [usize::try_from(quality.min(MAX_QUALITY)).expect("a quality of at most 4 fits usize")];
        if self.cap == 0 {
            self.store.clear();
        }
    }

    /// How many effects are live, fading ones included.
    pub fn live(&self) -> usize {
        self.store.len()
    }

    /// Place, unshadowed, every streak placed this frame or fading (facing `eye`) and every
    /// live flash (facing `eye`) and mark.
    pub(crate) fn draw(&mut self, meshes: &mut WorldMeshes, eye: Vec3) {
        for effect in self.store.values_mut() {
            let (mesh, build, transform, material) = match effect {
                Effect::Streak(streak) => {
                    let placed = std::mem::take(&mut streak.placed_this_frame);
                    if !placed && streak.ended_at.is_none() {
                        continue;
                    }
                    let Some(heading) = streak.heading else {
                        continue;
                    };
                    let width = streak.style.width * streak.width_scale();
                    let transform =
                        streak_transform(streak.head, heading, streak.style.length, width, eye);
                    (
                        STREAK_MESH,
                        streak_mesh as fn() -> WorldMesh,
                        transform,
                        streak.style.material,
                    )
                }
                Effect::Flash(flash) => (
                    FLASH_MESH,
                    flash_mesh as fn() -> WorldMesh,
                    flash_transform(flash, eye),
                    flash.style.material,
                ),
                Effect::Mark(mark) => (
                    MARK_MESH,
                    mark_mesh as fn() -> WorldMesh,
                    mark_transform(mark),
                    mark.style.material,
                ),
            };
            let Some(transform) = transform else {
                continue;
            };
            let id = WorldMeshId(EFFECT_MESH_IDS_FROM + mesh);
            let slot = usize::try_from(mesh).expect("a mesh offset of at most 2 fits usize");
            if !std::mem::replace(&mut self.uploaded[slot], true) {
                meshes.upsert_movable(id, build());
            }
            meshes.place_unshadowed(id, transform, material);
        }
    }
}

/// A mesh from flat XY positions facing +Z, with up normals and neutral channels.
fn flat_mesh(positions: Vec<[f32; 3]>, indices: Vec<u32>) -> WorldMesh {
    let n = positions.len();
    WorldMesh::new(
        positions,
        vec![[0.0, 0.0, 1.0]; n],
        vec![1.0; n],
        vec![[1.0; 3]; n],
        vec![1.0; n],
        vec![0; n],
        indices,
    )
    .expect("every channel has one entry per vertex and the indices are in range")
}

/// A four-point star in the XY plane, counter-clockwise seen from +Z: tips at radius 0.5 on
/// the axes alternating with inner points at (+-0.1, +-0.1).
fn flash_mesh() -> WorldMesh {
    flat_mesh(
        vec![
            [0.5, 0.0, 0.0],
            [0.1, 0.1, 0.0],
            [0.0, 0.5, 0.0],
            [-0.1, 0.1, 0.0],
            [-0.5, 0.0, 0.0],
            [-0.1, -0.1, 0.0],
            [0.0, -0.5, 0.0],
            [0.1, -0.1, 0.0],
        ],
        vec![7, 0, 1, 1, 2, 3, 3, 4, 5, 5, 6, 7, 1, 3, 5, 1, 5, 7],
    )
}

/// A unit quad with corners at +-0.5 in XY, counter-clockwise seen from +Z.
fn mark_mesh() -> WorldMesh {
    flat_mesh(
        vec![
            [-0.5, -0.5, 0.0],
            [0.5, -0.5, 0.0],
            [0.5, 0.5, 0.0],
            [-0.5, 0.5, 0.0],
        ],
        vec![0, 1, 2, 0, 2, 3],
    )
}

/// Axes `x` and `y` in the plane across `z` (a unit vector), turned by `roll` about it.
fn rolled_basis(z: Vec3, roll: f32) -> (Vec3, Vec3) {
    let x0 = z.any_orthonormal_vector();
    let y0 = z.cross(x0);
    let (sin, cos) = roll.sin_cos();
    (x0 * cos + y0 * sin, y0 * cos - x0 * sin)
}

/// The flash star scaled by its remaining life, facing `eye`; `None` when spent or when the
/// eye is not finite.
fn flash_transform(flash: &Flash, eye: Vec3) -> Option<Affine3A> {
    let lifetime = flash.style.lifetime.as_secs_f32();
    if !eye.is_finite() || lifetime <= 0.0 || !is_drawable_size(flash.style.size) {
        return None;
    }
    let scale = flash.style.size * (1.0 - flash.age.as_secs_f32() / lifetime);
    let z = (eye - flash.at).try_normalize().unwrap_or(Vec3::Z);
    let (x, y) = rolled_basis(z, flash.roll);
    Some(Affine3A::from_cols(
        (x * scale).into(),
        (y * scale).into(),
        z.into(),
        flash.at.into(),
    ))
}

/// A size that can be drawn: finite and positive.
fn is_drawable_size(size: f32) -> bool {
    size.is_finite() && size > 0.0
}

/// The mark quad on its surface, lifted by `MARK_OFFSET` and scaled by its fade; `None` when
/// spent or when its size cannot be drawn.
fn mark_transform(mark: &Mark) -> Option<Affine3A> {
    if mark.age >= mark.style.lifetime || !is_drawable_size(mark.style.size) {
        return None;
    }
    let scale = mark.style.size * mark.size_scale();
    let (x, y) = rolled_basis(mark.normal, mark.roll);
    Some(Affine3A::from_cols(
        (x * scale).into(),
        (y * scale).into(),
        mark.normal.into(),
        (mark.at + mark.normal * MARK_OFFSET).into(),
    ))
}

/// A kite from the tail at the origin to the head at x = 1, counter-clockwise seen from +Z.
fn streak_mesh() -> WorldMesh {
    WorldMesh::new(
        vec![
            [0.0, 0.0, 0.0],
            [0.8, -0.5, 0.0],
            [1.0, 0.0, 0.0],
            [0.8, 0.5, 0.0],
        ],
        vec![[0.0, 0.0, 1.0]; 4],
        vec![1.0; 4],
        vec![[1.0; 3]; 4],
        vec![1.0; 4],
        vec![0; 4],
        vec![0, 1, 2, 0, 2, 3],
    )
    .expect("the kite's channels are all four vertices long and its indices are in range")
}

/// Where a streak from `head - dir * length` to `head` is drawn so that it faces `eye`.
pub(crate) fn streak_transform(
    head: Vec3,
    dir: Vec3,
    length: f32,
    width: f32,
    eye: Vec3,
) -> Option<Affine3A> {
    let finite = head.is_finite() && dir.is_finite() && eye.is_finite() && length.is_finite();
    if !finite || !width.is_finite() || width <= 0.0 || length <= 0.0 {
        return None;
    }
    let tail = head - dir * length;
    // Any point on the streak's axis gives the same cross product with `dir`.
    let across = (eye - head).cross(dir);
    let y = if across.length() < 1e-4 {
        dir.any_orthonormal_vector()
    } else {
        across.normalize()
    };
    Some(Affine3A::from_cols(
        (dir * length).into(),
        (y * width).into(),
        dir.cross(y).into(),
        tail.into(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{InstanceGpu, RendererBackend};
    use moho_render_api::WorldMesh;
    use proptest::prelude::*;

    const EPS: f32 = 1e-4;

    /// Records the world meshes it is asked to register; every other call is inert.
    #[derive(Default)]
    struct MeshLog {
        /// Each `register_world_mesh` call as `(positions, indices)`.
        registered: Vec<(Vec<[f32; 3]>, Vec<u32>)>,
    }

    impl RendererBackend for MeshLog {
        fn resize(&mut self, _width: u32, _height: u32) {}
        fn register_mesh(&mut self, _vertices: &[[f32; 3]]) -> u32 {
            unreachable!("effects upload world meshes through register_world_mesh")
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
            unreachable!("effects upload world meshes through register_world_mesh")
        }
        fn register_world_mesh(&mut self, mesh: &WorldMesh, _material: &[u32]) -> u32 {
            self.registered
                .push((mesh.positions().to_vec(), mesh.indices().to_vec()));
            u32::try_from(self.registered.len()).expect("few meshes")
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
        fn set_shadow_quality(&mut self, quality: u8) -> u8 {
            quality
        }
        fn set_ssao_quality(&mut self, _quality: u8) {}
    }

    /// Effects at Ultra quality (cap 256), with the world meshes and backend they draw into.
    struct Rig {
        effects: Effects,
        meshes: WorldMeshes,
        backend: MeshLog,
    }

    impl Rig {
        fn new() -> Self {
            let mut effects = Effects::default();
            effects.set_quality(4);
            Self {
                effects,
                meshes: WorldMeshes::new(),
                backend: MeshLog::default(),
            }
        }

        /// One frame's draw: the unshadowed instances the effects queued.
        fn frame(&mut self, eye: Vec3) -> Vec<InstanceGpu> {
            self.effects.draw(&mut self.meshes, eye);
            self.meshes.flush(&mut self.backend);
            let drawn = self.meshes.unshadowed_draws().map(|(_, i)| i).collect();
            assert_eq!(
                self.meshes.draws().count(),
                0,
                "effects never use ordinary placements"
            );
            self.meshes.clear_placements();
            drawn
        }
    }

    const EYE: Vec3 = Vec3::new(9.0, 6.0, 3.0);

    fn style(length: f32, width: f32) -> StreakStyle {
        StreakStyle::new(length, width, 0)
    }

    fn translation(instance: &InstanceGpu) -> Vec3 {
        let [x, y, z, _] = instance.model[3];
        Vec3::new(x, y, z)
    }

    fn x_axis(instance: &InstanceGpu) -> Vec3 {
        let [x, y, z, _] = instance.model[0];
        Vec3::new(x, y, z)
    }

    fn y_axis(instance: &InstanceGpu) -> Vec3 {
        let [x, y, z, _] = instance.model[1];
        Vec3::new(x, y, z)
    }

    fn head_of(instance: &InstanceGpu) -> Vec3 {
        translation(instance) + x_axis(instance)
    }

    fn assert_near(actual: Vec3, expected: Vec3) {
        assert!(
            (actual - expected).length() < EPS,
            "expected {expected}, got {actual}"
        );
    }

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[test]
    fn a_streak_spans_tail_to_head_along_the_heading() {
        let mut rig = Rig::new();
        let handle = rig.effects.begin_streak(style(2.0, 0.1));

        rig.effects
            .place_streak(handle, Vec3::new(10.0, 1.0, 0.0), Vec3::X);
        let drawn = rig.frame(EYE);

        assert_eq!(drawn.len(), 1);
        assert_near(translation(&drawn[0]), Vec3::new(8.0, 1.0, 0.0));
        assert_near(head_of(&drawn[0]), Vec3::new(10.0, 1.0, 0.0));
    }

    #[test]
    fn a_streak_normalises_its_heading_and_carries_its_material() {
        let mut rig = Rig::new();
        let handle = rig.effects.begin_streak(StreakStyle::new(2.0, 0.1, 7));

        rig.effects
            .place_streak(handle, Vec3::new(0.0, 0.0, 10.0), Vec3::new(0.0, 0.0, 5.0));
        let drawn = rig.frame(EYE);

        assert_eq!(drawn.len(), 1);
        assert_near(translation(&drawn[0]), Vec3::new(0.0, 0.0, 8.0));
        assert_eq!(drawn[0].material, 7);
    }

    #[test]
    fn the_streak_mesh_is_registered_once_as_a_kite_wound_toward_plus_z() {
        let mut rig = Rig::new();
        let handle = rig.effects.begin_streak(style(2.0, 0.1));

        for x in [1.0, 2.0, 3.0] {
            rig.effects
                .place_streak(handle, Vec3::new(x, 0.0, 0.0), Vec3::X);
            rig.frame(EYE);
        }

        assert_eq!(rig.backend.registered.len(), 1, "moving never re-uploads");
        let (positions, indices) = &rig.backend.registered[0];
        assert_eq!(
            positions,
            &vec![
                [0.0, 0.0, 0.0],
                [0.8, -0.5, 0.0],
                [1.0, 0.0, 0.0],
                [0.8, 0.5, 0.0]
            ]
        );
        assert_eq!(indices.len(), 6, "a kite is two triangles");
        for triangle in indices.chunks(3) {
            let [a, b, c] = [0, 1, 2].map(|i| Vec3::from(positions[triangle[i] as usize]));
            assert!(
                (b - a).cross(c - a).z > 0.0,
                "triangle {triangle:?} is counter-clockwise seen from +Z"
            );
        }
    }

    #[test]
    fn a_bounce_is_the_next_placement_with_a_new_heading() {
        let mut rig = Rig::new();
        let handle = rig.effects.begin_streak(style(2.0, 0.1));
        let head = Vec3::new(4.0, 1.0, 4.0);
        rig.effects.place_streak(handle, head, Vec3::X);
        rig.frame(EYE);

        rig.effects.place_streak(handle, head, Vec3::Z);
        let drawn = rig.frame(EYE);

        assert_eq!(drawn.len(), 1);
        assert_near(translation(&drawn[0]), head - Vec3::Z * 2.0);
        assert_near(head_of(&drawn[0]), head);
    }

    #[test]
    fn a_zero_heading_is_ignored_and_keeps_the_last_one() {
        let mut rig = Rig::new();
        let handle = rig.effects.begin_streak(style(2.0, 0.1));
        let head = Vec3::new(4.0, 1.0, 4.0);
        rig.effects.place_streak(handle, head, Vec3::Z);
        rig.frame(EYE);

        let moved = head + Vec3::X;
        rig.effects.place_streak(handle, moved, Vec3::ZERO);
        let drawn = rig.frame(EYE);

        assert_eq!(drawn.len(), 1);
        assert_near(translation(&drawn[0]), moved - Vec3::Z * 2.0);
        assert_near(head_of(&drawn[0]), moved);
    }

    #[test]
    fn a_zero_heading_on_the_first_placement_draws_nothing() {
        let mut rig = Rig::new();
        let handle = rig.effects.begin_streak(style(2.0, 0.1));

        rig.effects
            .place_streak(handle, Vec3::new(4.0, 1.0, 4.0), Vec3::ZERO);

        assert!(rig.frame(EYE).is_empty());
        assert_eq!(rig.effects.live(), 1, "still live");
    }

    #[test]
    fn an_unplaced_streak_draws_nothing_and_stays_live() {
        let mut rig = Rig::new();
        let handle = rig.effects.begin_streak(style(2.0, 0.1));
        rig.effects.place_streak(handle, Vec3::X * 5.0, Vec3::X);
        assert_eq!(rig.frame(EYE).len(), 1, "drawn on the frame it is placed");

        let drawn = rig.frame(EYE);

        assert!(drawn.is_empty());
        assert_eq!(rig.effects.live(), 1);
        rig.effects.place_streak(handle, Vec3::X * 6.0, Vec3::X);
        assert_eq!(rig.frame(EYE).len(), 1, "placing again draws again");
    }

    #[test]
    fn ending_fades_width_linearly_then_removes_the_handle() {
        let mut rig = Rig::new();
        let mut fading = style(2.0, 0.2);
        fading.fade = ms(60);
        let handle = rig.effects.begin_streak(fading);
        let head = Vec3::new(10.0, 1.0, 0.0);
        rig.effects.place_streak(handle, head, Vec3::X);
        let full = rig.frame(EYE);
        rig.effects.advance(ms(100));
        rig.effects.end(handle);

        rig.effects.advance(ms(30));
        let half = rig.frame(EYE);

        assert_eq!(half.len(), 1, "a fading streak draws without being placed");
        assert_near(translation(&half[0]), translation(&full[0]));
        assert_near(x_axis(&half[0]), x_axis(&full[0]));
        assert!(
            (y_axis(&half[0]).length() - 0.1).abs() < EPS,
            "half the width, got {}",
            y_axis(&half[0]).length()
        );
        assert_eq!(rig.effects.live(), 1);

        rig.effects.advance(ms(30));
        assert!(rig.frame(EYE).is_empty());
        assert_eq!(rig.effects.live(), 0);
        rig.effects.place_streak(handle, head, Vec3::X);
        assert!(rig.frame(EYE).is_empty(), "the handle is gone");
    }

    #[test]
    fn a_zero_fade_removes_at_the_end() {
        let mut rig = Rig::new();
        let mut abrupt = style(2.0, 0.2);
        abrupt.fade = Duration::ZERO;
        let handle = rig.effects.begin_streak(abrupt);
        rig.effects.place_streak(handle, Vec3::X * 5.0, Vec3::X);
        assert_eq!(rig.frame(EYE).len(), 1);

        rig.effects.end(handle);
        rig.effects.advance(ms(1));

        assert!(rig.frame(EYE).is_empty());
        assert_eq!(rig.effects.live(), 0);
    }

    #[test]
    fn max_age_ends_a_streak_that_was_never_ended() {
        let mut rig = Rig::new();
        let mut forgotten = style(2.0, 0.2);
        forgotten.max_age = Duration::from_secs(2);
        forgotten.fade = ms(60);
        let handle = rig.effects.begin_streak(forgotten);
        let head = Vec3::new(3.0, 1.0, 0.0);
        rig.effects.place_streak(handle, head, Vec3::X);

        rig.effects.advance(Duration::from_secs(1));
        let young = rig.frame(EYE);
        assert_eq!(young.len(), 1);
        assert!(
            (y_axis(&young[0]).length() - 0.2).abs() < EPS,
            "not fading yet"
        );
        assert_eq!(rig.effects.live(), 1);

        rig.effects.advance(Duration::from_secs(1));
        rig.effects.advance(ms(30));
        let fading = rig.frame(EYE);
        assert_eq!(fading.len(), 1, "fades like an ended one without a place");
        assert!((y_axis(&fading[0]).length() - 0.1).abs() < EPS);

        rig.effects.advance(ms(30));
        assert!(rig.frame(EYE).is_empty());
        assert_eq!(rig.effects.live(), 0);
    }

    #[test]
    fn stale_and_unknown_handles_are_ignored() {
        let mut rig = Rig::new();
        let finished = rig.effects.begin_streak(style(2.0, 0.2));
        rig.effects.end(finished);
        rig.effects.advance(Duration::from_secs(1));
        assert_eq!(rig.effects.live(), 0);
        let bystander = rig.effects.begin_streak(style(2.0, 0.2));
        let unknown = EffectHandle(u64::MAX / 2);

        for handle in [finished, unknown] {
            rig.effects.place_streak(handle, Vec3::X, Vec3::X);
            rig.effects.end(handle);
        }
        let drawn = rig.frame(EYE);

        assert!(drawn.is_empty());
        assert_eq!(rig.effects.live(), 1, "only the bystander is live");
        rig.effects.place_streak(bystander, Vec3::X, Vec3::X);
        assert_eq!(rig.frame(EYE).len(), 1, "and it was not ended");
    }

    #[test]
    fn quality_caps_new_streaks_and_returns_inert_handles() {
        let rows = [
            (0, 3, 0),
            (1, 40, 32),
            (2, 100, 64),
            (3, 200, 128),
            (4, 300, 256),
        ];
        for (quality, asked, live) in rows {
            let mut rig = Rig::new();
            rig.effects.set_quality(quality);

            let handles: Vec<EffectHandle> = (0..asked)
                .map(|_| rig.effects.begin_streak(style(2.0, 0.1)))
                .collect();

            assert_eq!(rig.effects.live(), live, "quality {quality}");
            for handle in &handles {
                rig.effects.place_streak(*handle, Vec3::X, Vec3::X);
            }
            assert_eq!(rig.frame(EYE).len(), live, "inert handles draw nothing");
            let (kept, inert) = handles.split_at(live);
            let distinct: std::collections::BTreeSet<_> = handles.iter().collect();
            assert_eq!(distinct.len(), handles.len(), "every handle is distinct");
            assert_eq!(kept.len(), live);
            for handle in inert {
                rig.effects.end(*handle);
            }
            rig.effects.advance(ms(1));
            assert_eq!(
                rig.effects.live(),
                live,
                "ending inert handles does nothing"
            );
        }
    }

    #[test]
    fn streak_transform_rejects_a_zero_or_negative_length() {
        for length in [0.0, -2.0] {
            let drawn = streak_transform(Vec3::X, Vec3::X, length, 0.1, Vec3::new(0.0, 0.0, 5.0));
            assert!(drawn.is_none(), "length {length}");
        }
    }

    #[test]
    fn the_reserved_mesh_ids_are_the_top_sixty_four() {
        assert_eq!(EFFECT_MESH_IDS_FROM, 0xFFFF_FFFF_FFFF_FFC0);
        assert_eq!(u64::MAX - EFFECT_MESH_IDS_FROM + 1, 64);
    }

    #[test]
    fn quality_above_four_clamps_to_ultra() {
        for quality in [5, 9, u32::MAX] {
            let mut rig = Rig::new();
            rig.effects.set_quality(1);
            rig.effects.set_quality(quality);

            for _ in 0..300 {
                rig.effects.begin_streak(style(2.0, 0.1));
            }

            assert_eq!(rig.effects.live(), 256, "quality {quality}");
        }
    }

    #[test]
    fn quality_zero_clears_live_effects() {
        let mut rig = Rig::new();
        let handles: Vec<EffectHandle> = (0..3)
            .map(|_| rig.effects.begin_streak(style(2.0, 0.1)))
            .collect();
        for handle in &handles {
            rig.effects.place_streak(*handle, Vec3::X, Vec3::X);
        }

        rig.effects.set_quality(0);

        assert_eq!(rig.effects.live(), 0);
        assert!(rig.frame(EYE).is_empty());
    }

    #[test]
    fn lowering_quality_keeps_live_effects() {
        let mut rig = Rig::new();
        let handles: Vec<EffectHandle> = (0..40)
            .map(|_| rig.effects.begin_streak(style(2.0, 0.1)))
            .collect();

        rig.effects.set_quality(1);

        assert_eq!(rig.effects.live(), 40, "kept though over the new cap");
        rig.effects.begin_streak(style(2.0, 0.1));
        assert_eq!(rig.effects.live(), 40, "but no new one starts");
        for handle in &handles {
            rig.effects.place_streak(*handle, Vec3::X, Vec3::X);
        }
        assert_eq!(rig.frame(EYE).len(), 40);
    }

    fn assert_faces_eye(transform: Affine3A, eye: Vec3) {
        let x = Vec3::from(transform.matrix3.x_axis);
        let y = Vec3::from(transform.matrix3.y_axis);
        let midpoint = Vec3::from(transform.translation) + x * 0.5;
        assert!(
            x.cross(y).dot(eye - midpoint) > 0.0,
            "X x Y points away from the eye"
        );
    }

    proptest! {
        #[test]
        fn streak_transform_faces_the_eye_with_ccw_winding(
            head in (-50.0f32..50.0, -50.0f32..50.0, -50.0f32..50.0),
            yaw in 0.0f32..std::f32::consts::TAU,
            pitch in -1.5f32..1.5,
            eye in (-50.0f32..50.0, -50.0f32..50.0, -50.0f32..50.0),
            length in 0.1f32..10.0,
            width in 0.01f32..2.0,
        ) {
            let head = Vec3::new(head.0, head.1, head.2);
            let eye = Vec3::new(eye.0, eye.1, eye.2);
            let dir = Vec3::new(
                pitch.cos() * yaw.cos(),
                pitch.sin(),
                pitch.cos() * yaw.sin(),
            );
            let midpoint = head - dir * length * 0.5;
            prop_assume!((eye - midpoint).cross(dir).length() > 1e-2);

            let transform = streak_transform(head, dir, length, width, eye)
                .expect("finite inputs and a positive width");

            let tail = Vec3::from(transform.translation);
            let x = Vec3::from(transform.matrix3.x_axis);
            let y = Vec3::from(transform.matrix3.y_axis);
            prop_assert!((tail - (head - dir * length)).length() < 1e-3);
            prop_assert!((x - dir * length).length() < 1e-3);
            prop_assert!((y.length() - width).abs() < 1e-3);
            prop_assert!(y.dot(dir).abs() < 1e-3, "the width is across the heading");
            assert_faces_eye(transform, eye);
            let z = Vec3::from(transform.matrix3.z_axis);
            prop_assert!((z.length() - 1.0).abs() < 1e-3, "Z is a unit vector");
            let normal = x.cross(y).normalize();
            prop_assert!((z - normal).length() < 1e-3, "Z is along X x Y");
            prop_assert!(
                (z - dir.cross(y.normalize())).length() < 1e-3,
                "Z is dir x unit Y"
            );
        }
    }

    #[test]
    fn streak_transform_looking_down_the_streak_is_finite() {
        let head = Vec3::new(1.0, 2.0, 3.0);
        let dir = Vec3::new(0.0, 0.6, 0.8);
        for eye in [head + dir * 20.0, head - dir * 20.0, head + dir * 0.5] {
            let transform = streak_transform(head, dir, 2.0, 0.3, eye)
                .expect("looking down the streak is still drawable");

            let columns = [
                transform.matrix3.x_axis,
                transform.matrix3.y_axis,
                transform.matrix3.z_axis,
                transform.translation,
            ];
            assert!(
                columns
                    .iter()
                    .all(|c| c.to_array().iter().all(|v| v.is_finite()))
            );
            let y = Vec3::from(transform.matrix3.y_axis);
            assert!((y.length() - 0.3).abs() < EPS);
            assert!(y.dot(dir).abs() < EPS, "still across the heading");
        }
    }

    #[test]
    fn streak_transform_rejects_non_finite_inputs_and_non_positive_width() {
        let head = Vec3::new(1.0, 2.0, 3.0);

        assert!(streak_transform(head, Vec3::X, 2.0, 0.0, EYE).is_none());
        assert!(streak_transform(head, Vec3::X, 2.0, -0.1, EYE).is_none());
        assert!(streak_transform(head, Vec3::X, 2.0, f32::NAN, EYE).is_none());
        assert!(streak_transform(Vec3::new(f32::NAN, 0.0, 0.0), Vec3::X, 2.0, 0.1, EYE).is_none());
        assert!(streak_transform(head, Vec3::X, f32::INFINITY, 0.1, EYE).is_none());
        assert!(streak_transform(head, Vec3::X, 2.0, 0.1, Vec3::splat(f32::NAN)).is_none());
    }

    #[derive(Debug, Clone)]
    enum Op {
        Begin,
        End(usize),
        Advance(u64),
        SetQuality(u32),
    }

    fn ops() -> impl Strategy<Value = Vec<Op>> {
        prop::collection::vec(
            prop_oneof![
                4 => Just(Op::Begin),
                2 => (0usize..64).prop_map(Op::End),
                2 => (0u64..200).prop_map(Op::Advance),
                1 => (0u32..6).prop_map(Op::SetQuality),
            ],
            0..80,
        )
    }

    proptest! {
        #[test]
        fn handles_are_never_reused(ops in ops()) {
            let mut effects = Effects::default();
            effects.set_quality(4);
            let mut issued: Vec<EffectHandle> = Vec::new();
            let mut live_handles: Vec<EffectHandle> = Vec::new();

            for op in ops {
                match op {
                    Op::Begin => {
                        let before = effects.live();
                        let handle = effects.begin_streak(StreakStyle {
                            fade: ms(50),
                            max_age: ms(300),
                            ..style(2.0, 0.1)
                        });
                        prop_assert!(!issued.contains(&handle), "{handle:?} issued twice");
                        if effects.live() > before {
                            live_handles.push(handle);
                        }
                        issued.push(handle);
                    }
                    Op::End(index) => {
                        if let Some(handle) = live_handles.get(index % live_handles.len().max(1)) {
                            effects.end(*handle);
                        }
                    }
                    Op::Advance(millis) => effects.advance(ms(millis)),
                    Op::SetQuality(quality) => effects.set_quality(quality),
                }
            }
        }
    }

    fn z_axis(instance: &InstanceGpu) -> Vec3 {
        let [x, y, z, _] = instance.model[2];
        Vec3::new(x, y, z)
    }

    fn flash_style(size: f32, lifetime: Duration) -> FlashStyle {
        FlashStyle {
            size,
            material: 3,
            lifetime,
        }
    }

    fn mark_style(size: f32, lifetime: Duration, fade: Duration) -> MarkStyle {
        MarkStyle {
            size,
            material: 9,
            lifetime,
            fade,
        }
    }

    #[test]
    fn a_flash_shrinks_linearly_and_expires() {
        let mut rig = Rig::new();
        let at = Vec3::new(1.0, 2.0, 3.0);
        rig.effects.flash(at, 0.0, flash_style(0.4, ms(50)));

        let full = rig.frame(EYE);
        rig.effects.advance(ms(25));
        let half = rig.frame(EYE);
        rig.effects.advance(ms(25));
        let gone = rig.frame(EYE);

        assert_eq!(full.len(), 1);
        assert!((x_axis(&full[0]).length() - 0.4).abs() < EPS);
        assert_eq!(full[0].material, 3);
        assert_near(translation(&full[0]), at);
        assert_eq!(half.len(), 1);
        assert!(
            (x_axis(&half[0]).length() - 0.2).abs() < EPS,
            "half size, got {}",
            x_axis(&half[0]).length()
        );
        assert!((y_axis(&half[0]).length() - 0.2).abs() < EPS);
        assert!(gone.is_empty());
        assert_eq!(rig.effects.live(), 0);
    }

    #[test]
    fn a_flash_faces_the_eye() {
        let at = Vec3::new(1.0, 2.0, 3.0);
        let eyes = [
            EYE,
            Vec3::new(-20.0, 5.0, 0.0),
            Vec3::new(1.0, 30.0, 3.5),
            Vec3::new(1.0, -30.0, 3.0),
        ];
        for eye in eyes {
            let mut rig = Rig::new();
            rig.effects.flash(at, 0.7, flash_style(0.4, ms(50)));

            let drawn = rig.frame(eye);

            assert_eq!(drawn.len(), 1, "eye {eye}");
            let facing = x_axis(&drawn[0]).cross(y_axis(&drawn[0]));
            assert!(
                facing.normalize().dot((eye - at).normalize()) > 0.99,
                "eye {eye}: faces {facing}"
            );
        }
    }

    #[test]
    fn a_mark_lies_along_its_normal_offset_by_the_mark_offset() {
        let mut rig = Rig::new();
        let at = Vec3::new(4.0, 0.0, -2.0);
        let normal = Vec3::new(0.0, 0.6, 0.8);
        rig.effects.mark(
            at,
            normal,
            0.3,
            mark_style(0.2, Duration::from_secs(10), ms(1000)),
        );

        let drawn = rig.frame(EYE);

        assert_eq!(drawn.len(), 1);
        let mark = &drawn[0];
        assert_eq!(mark.material, 9);
        assert_near(translation(mark), at + normal * MARK_OFFSET);
        assert!((x_axis(mark).length() - 0.2).abs() < EPS);
        assert!((y_axis(mark).length() - 0.2).abs() < EPS);
        assert!(x_axis(mark).dot(y_axis(mark)).abs() < EPS, "a square");
        assert!(x_axis(mark).dot(normal).abs() < EPS, "flat on the surface");
        assert!(y_axis(mark).dot(normal).abs() < EPS, "flat on the surface");
        assert_near(z_axis(mark).normalize(), normal);
        assert_eq!(MARK_OFFSET, 0.01);
    }

    #[test]
    fn a_mark_holds_full_size_then_shrinks_out_in_its_fade() {
        let mut rig = Rig::new();
        let style = mark_style(0.2, Duration::from_secs(10), Duration::from_secs(1));
        rig.effects.mark(Vec3::ZERO, Vec3::Y, 0.0, style);

        rig.effects.advance(Duration::from_secs(9));
        let full = rig.frame(EYE);
        rig.effects.advance(ms(500));
        let half = rig.frame(EYE);
        rig.effects.advance(ms(500));
        let gone = rig.frame(EYE);

        assert_eq!(full.len(), 1);
        assert!((x_axis(&full[0]).length() - 0.2).abs() < EPS, "full at 9 s");
        assert_eq!(half.len(), 1);
        assert!(
            (x_axis(&half[0]).length() - 0.1).abs() < EPS,
            "half at 9.5 s, got {}",
            x_axis(&half[0]).length()
        );
        assert!((y_axis(&half[0]).length() - 0.1).abs() < EPS);
        assert!(gone.is_empty());
        assert_eq!(rig.effects.live(), 0);
    }

    #[test]
    fn a_full_cap_drops_the_oldest_fixed_lifetime_effect() {
        let mut rig = Rig::new();
        rig.effects.set_quality(1);
        let style = mark_style(0.2, Duration::from_secs(10), ms(1000));
        for i in 0..32u8 {
            rig.effects.mark(
                Vec3::new(f32::from(i) * 10.0, 0.0, 0.0),
                Vec3::Y,
                0.0,
                style,
            );
        }
        assert_eq!(rig.effects.live(), 32);

        rig.effects
            .mark(Vec3::new(1000.0, 0.0, 0.0), Vec3::Y, 0.0, style);
        let drawn = rig.frame(EYE);

        assert_eq!(rig.effects.live(), 32);
        assert_eq!(drawn.len(), 32);
        let xs: Vec<f32> = drawn.iter().map(|i| translation(i).x).collect();
        assert!(
            xs.iter().all(|x| *x > 5.0),
            "the first mark (x = 0) is gone"
        );
        assert!(
            xs.iter().any(|x| (*x - 10.0).abs() < EPS),
            "the second stays"
        );
        assert!(
            xs.iter().any(|x| (*x - 1000.0).abs() < EPS),
            "the new one is in"
        );
    }

    #[test]
    fn a_full_cap_of_streaks_refuses_a_mark_and_keeps_every_streak() {
        let mut control = Rig::new();
        control.effects.set_quality(1);
        for _ in 0..31 {
            control.effects.begin_streak(style(2.0, 0.1));
        }
        control.effects.mark(
            Vec3::new(5.0, 0.0, 0.0),
            Vec3::Y,
            0.0,
            mark_style(0.2, Duration::from_secs(10), ms(1000)),
        );
        assert_eq!(control.effects.live(), 32, "a free slot admits a mark");
        assert_eq!(control.frame(EYE).len(), 1);

        let mut rig = Rig::new();
        rig.effects.set_quality(1);
        let handles: Vec<EffectHandle> = (0..32)
            .map(|_| rig.effects.begin_streak(style(2.0, 0.1)))
            .collect();
        assert_eq!(rig.effects.live(), 32);

        rig.effects.mark(
            Vec3::new(5.0, 0.0, 0.0),
            Vec3::Y,
            0.0,
            mark_style(0.2, Duration::from_secs(10), ms(1000)),
        );
        rig.effects
            .flash(Vec3::new(5.0, 0.0, 0.0), 0.0, flash_style(0.4, ms(50)));
        for handle in &handles {
            rig.effects.place_streak(*handle, Vec3::X, Vec3::X);
        }
        let drawn = rig.frame(EYE);

        assert_eq!(rig.effects.live(), 32);
        assert_eq!(drawn.len(), 32, "every streak still draws");
        assert!(
            drawn.iter().all(|i| i.material == 0),
            "neither the mark nor the flash was admitted"
        );
    }

    #[test]
    fn quality_zero_refuses_flash_and_mark() {
        let mut rig = Rig::new();
        let mark = mark_style(0.2, Duration::from_secs(10), ms(1000));
        rig.effects.flash(Vec3::X, 0.0, flash_style(0.4, ms(50)));
        rig.effects.mark(Vec3::X, Vec3::Y, 0.0, mark);
        assert_eq!(rig.effects.live(), 2, "accepted while effects are on");
        rig.effects.set_quality(0);

        rig.effects.flash(Vec3::X, 0.0, flash_style(0.4, ms(50)));
        rig.effects.mark(Vec3::X, Vec3::Y, 0.0, mark);

        assert_eq!(rig.effects.live(), 0);
        assert!(rig.frame(EYE).is_empty());
        rig.effects.set_quality(4);
        assert!(rig.frame(EYE).is_empty(), "refused spawns do not return");
        assert_eq!(rig.effects.live(), 0);
    }

    #[test]
    fn the_same_spawns_draw_the_same_instances() {
        let run = || {
            let mut rig = Rig::new();
            rig.effects
                .flash(Vec3::new(1.0, 2.0, 3.0), 0.5, flash_style(0.4, ms(50)));
            rig.effects.mark(
                Vec3::new(-4.0, 0.0, 2.0),
                Vec3::new(0.0, 1.0, 1.0),
                1.2,
                mark_style(0.2, Duration::from_secs(10), ms(1000)),
            );
            let mut frames = vec![format!("{:?}", rig.frame(EYE))];
            for step in [20, 20, 5000, 4600] {
                rig.effects.advance(ms(step));
                frames.push(format!("{:?}", rig.frame(EYE)));
            }
            frames
        };

        let first = run();
        let second = run();

        assert_eq!(first, second);
        assert_ne!(first[0], first[1], "the effects change as they age");
        assert_ne!(first[0], "[]", "something was drawn to compare");
    }

    fn any_normal() -> impl Strategy<Value = Vec3> {
        prop_oneof![
            Just(Vec3::X),
            Just(Vec3::Y),
            Just(Vec3::Z),
            Just(Vec3::NEG_X),
            Just(Vec3::NEG_Y),
            Just(Vec3::NEG_Z),
            (1.0e-3f32..1.0e3, 0usize..3, any::<bool>()).prop_map(|(len, axis, negative)| {
                let sign = if negative { -len } else { len };
                let mut v = Vec3::ZERO;
                v[axis] = sign;
                v
            }),
            ((-100.0f32..100.0, -100.0f32..100.0, -100.0f32..100.0)
                .prop_filter("not near zero", |(x, y, z)| {
                    Vec3::new(*x, *y, *z).length() > 1e-2
                }))
            .prop_map(|(x, y, z)| Vec3::new(x, y, z)),
        ]
    }

    proptest! {
        #[test]
        fn a_mark_basis_is_finite_for_any_normal(
            normal in any_normal(),
            roll in -10.0f32..10.0,
        ) {
            let mut rig = Rig::new();
            rig.effects.mark(
                Vec3::new(1.0, 2.0, 3.0),
                normal,
                roll,
                mark_style(0.2, Duration::from_secs(10), ms(1000)),
            );

            let drawn = rig.frame(EYE);

            prop_assert_eq!(drawn.len(), 1);
            let model = drawn[0].model;
            prop_assert!(model.iter().flatten().all(|v| v.is_finite()), "{model:?}");
            let (x, y, z) = (x_axis(&drawn[0]), y_axis(&drawn[0]), z_axis(&drawn[0]));
            let unit = normal.normalize();
            prop_assert!(
                (translation(&drawn[0]) - (Vec3::new(1.0, 2.0, 3.0) + unit * MARK_OFFSET)).length() < 1e-3,
                "lifted along the unit normal"
            );
            prop_assert!(y.dot(unit).abs() < 1e-3, "Y lies in the surface");
            prop_assert!((x.length() - 0.2).abs() < 1e-3);
            prop_assert!((y.length() - 0.2).abs() < 1e-3);
            prop_assert!(x.dot(y).abs() < 1e-3, "X and Y are perpendicular");
            prop_assert!(x.dot(normal.normalize()).abs() < 1e-3, "X lies in the surface");
            prop_assert!(z.normalize().dot(normal.normalize()) > 0.999, "Z is the normal");
        }
    }

    #[test]
    fn a_mark_with_a_zero_or_non_finite_normal_is_refused() {
        let style = mark_style(0.2, Duration::from_secs(10), ms(1000));
        let at = Vec3::new(1.0, 2.0, 3.0);
        let mut rig = Rig::new();
        rig.effects.mark(at, Vec3::Y, 0.0, style);
        assert_eq!(rig.effects.live(), 1, "a valid mark is accepted");

        let bad_normals = [
            Vec3::ZERO,
            Vec3::new(f32::NAN, 0.0, 1.0),
            Vec3::new(0.0, f32::INFINITY, 0.0),
            Vec3::new(0.0, 0.0, f32::NEG_INFINITY),
            Vec3::splat(f32::NAN),
        ];
        for normal in bad_normals {
            rig.effects.mark(at, normal, 0.0, style);
            assert_eq!(rig.effects.live(), 1, "normal {normal} is refused");
        }
        for bad_at in [
            Vec3::new(f32::NAN, 0.0, 0.0),
            Vec3::new(0.0, f32::INFINITY, 0.0),
        ] {
            rig.effects.mark(bad_at, Vec3::Y, 0.0, style);
            assert_eq!(rig.effects.live(), 1, "position {bad_at} is refused");
        }
        for roll in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            rig.effects.mark(at, Vec3::Y, roll, style);
            assert_eq!(rig.effects.live(), 1, "roll {roll} is refused");
        }
        let drawn = rig.frame(EYE);

        assert_eq!(drawn.len(), 1, "only the valid mark draws");
        assert_near(translation(&drawn[0]), at + Vec3::Y * MARK_OFFSET);
    }

    /// The registered mesh each drawn instance uses, with its instance.
    fn frame_meshes(rig: &mut Rig, eye: Vec3) -> Vec<(u32, InstanceGpu)> {
        rig.effects.draw(&mut rig.meshes, eye);
        rig.meshes.flush(&mut rig.backend);
        let drawn = rig.meshes.unshadowed_draws().collect();
        rig.meshes.clear_placements();
        drawn
    }

    /// The backend handle the world meshes hold for `id`, found by placing a probe on it.
    fn handle_of(rig: &mut Rig, id: u64) -> Option<u32> {
        rig.meshes
            .place_unshadowed(WorldMeshId(id), Affine3A::IDENTITY, 0);
        let handle = rig.meshes.unshadowed_draws().map(|(h, _)| h).next();
        rig.meshes.clear_placements();
        handle
    }

    fn upward_triangles(positions: &[[f32; 3]], indices: &[u32]) -> bool {
        indices.len().is_multiple_of(3)
            && !indices.is_empty()
            && indices.chunks(3).all(|t| {
                let [a, b, c] = [0, 1, 2].map(|i| Vec3::from(positions[t[i] as usize]));
                (b - a).cross(c - a).z > 0.0
            })
    }

    #[test]
    fn a_flash_draws_a_four_point_star_registered_once_under_the_second_effect_id() {
        let mut rig = Rig::new();
        for x in [0.0, 1.0, 2.0] {
            rig.effects
                .flash(Vec3::new(x, 0.0, 0.0), 0.0, flash_style(0.4, ms(50)));
        }

        let drawn = frame_meshes(&mut rig, EYE);
        frame_meshes(&mut rig, EYE);

        assert_eq!(drawn.len(), 3);
        assert_eq!(rig.backend.registered.len(), 1, "one shared flash mesh");
        let handle = handle_of(&mut rig, EFFECT_MESH_IDS_FROM + 1)
            .expect("the flash mesh lives under the second effect id");
        assert!(drawn.iter().all(|(h, _)| *h == handle));
        let (positions, indices) = &rig.backend.registered[handle as usize - 1];
        assert_eq!(positions.len(), 8);
        assert!(positions.iter().all(|p| p[2] == 0.0), "flat in XY");
        let mut radii: Vec<f32> = positions.iter().map(|p| Vec3::from(*p).length()).collect();
        radii.sort_by(f32::total_cmp);
        assert!(radii[0] > 0.0);
        assert!(radii[3] < radii[4] - 1e-3, "four inner points, four outer");
        assert!(
            (radii[0] - radii[3]).abs() < EPS,
            "inner points equidistant"
        );
        assert!((radii[4] - radii[7]).abs() < EPS, "tips equidistant");
        let tips: Vec<Vec3> = positions
            .iter()
            .map(|p| Vec3::from(*p))
            .filter(|p| p.length() > radii[3] + 1e-3)
            .collect();
        for (i, tip) in tips.iter().enumerate() {
            let perpendicular = tips
                .iter()
                .enumerate()
                .filter(|(j, other)| *j != i && tip.dot(**other).abs() < EPS)
                .count();
            assert_eq!(perpendicular, 2, "tips sit 90 degrees apart");
        }
        assert!(indices.iter().all(|i| (*i as usize) < positions.len()));
        assert!(
            upward_triangles(positions, indices),
            "wound toward +Z: {indices:?}"
        );
    }

    #[test]
    fn a_mark_draws_a_unit_quad_registered_once_under_the_third_effect_id() {
        let mut rig = Rig::new();
        let style = mark_style(0.2, Duration::from_secs(10), ms(1000));
        for x in [0.0, 1.0, 2.0] {
            rig.effects
                .mark(Vec3::new(x, 0.0, 0.0), Vec3::Y, 0.0, style);
        }

        let drawn = frame_meshes(&mut rig, EYE);
        frame_meshes(&mut rig, EYE);

        assert_eq!(drawn.len(), 3);
        assert_eq!(rig.backend.registered.len(), 1, "one shared mark mesh");
        let handle = handle_of(&mut rig, EFFECT_MESH_IDS_FROM + 2)
            .expect("the mark mesh lives under the third effect id");
        assert!(drawn.iter().all(|(h, _)| *h == handle));
        let (positions, indices) = &rig.backend.registered[handle as usize - 1];
        assert_eq!(positions.len(), 4);
        let mut corners: Vec<[i32; 3]> = positions
            .iter()
            .map(|p| p.map(|v| (v * 2.0).round() as i32))
            .collect();
        corners.sort_unstable();
        assert_eq!(
            corners,
            vec![[-1, -1, 0], [-1, 1, 0], [1, -1, 0], [1, 1, 0]],
            "corners at plus and minus a half in XY"
        );
        assert!(
            positions
                .iter()
                .flatten()
                .all(|v| v.abs() == 0.5 || *v == 0.0)
        );
        assert_eq!(indices.len(), 6, "a quad is two triangles");
        assert!(upward_triangles(positions, indices), "wound toward +Z");
    }

    #[test]
    fn effects_use_distinct_meshes() {
        let mut rig = Rig::new();
        let handle = rig.effects.begin_streak(style(2.0, 0.1));
        rig.effects.place_streak(handle, Vec3::X, Vec3::X);
        rig.effects.flash(Vec3::X, 0.0, flash_style(0.4, ms(50)));
        rig.effects.mark(
            Vec3::X,
            Vec3::Y,
            0.0,
            mark_style(0.2, Duration::from_secs(10), ms(1000)),
        );

        let drawn = frame_meshes(&mut rig, EYE);

        assert_eq!(drawn.len(), 3);
        assert_eq!(rig.backend.registered.len(), 3);
        let ids: Vec<Option<u32>> = (0..3)
            .map(|k| handle_of(&mut rig, EFFECT_MESH_IDS_FROM + k))
            .collect();
        assert!(ids.iter().all(Option::is_some));
        assert_ne!(ids[0], ids[1]);
        assert_ne!(ids[1], ids[2]);
        assert_ne!(ids[0], ids[2]);
    }

    #[test]
    fn a_full_cap_of_mixed_effects_drops_the_oldest_fixed_lifetime_one_and_keeps_the_streak() {
        let mark = mark_style(0.2, Duration::from_secs(10), ms(1000));
        for spawn_flash in [true, false] {
            let mut rig = Rig::new();
            rig.effects.set_quality(1);
            let streak = rig.effects.begin_streak(style(2.0, 0.1));
            rig.effects.flash(
                Vec3::new(0.0, 0.0, 0.0),
                0.0,
                flash_style(0.4, Duration::from_secs(10)),
            );
            for i in 1..31u8 {
                rig.effects
                    .mark(Vec3::new(f32::from(i) * 10.0, 0.0, 0.0), Vec3::Y, 0.0, mark);
            }
            assert_eq!(rig.effects.live(), 32);
            let newcomer = Vec3::new(1000.0, 0.0, 0.0);

            if spawn_flash {
                rig.effects
                    .flash(newcomer, 0.0, flash_style(0.4, Duration::from_secs(10)));
            } else {
                rig.effects.mark(newcomer, Vec3::Y, 0.0, mark);
            }
            rig.effects
                .place_streak(streak, Vec3::new(500.0, 0.0, 0.0), Vec3::X);
            let drawn = rig.frame(EYE);

            assert_eq!(rig.effects.live(), 32, "flash spawn: {spawn_flash}");
            assert_eq!(drawn.len(), 32, "flash spawn: {spawn_flash}");
            assert!(
                drawn
                    .iter()
                    .any(|i| i.material == 0 && (translation(i).x - 498.0).abs() < EPS),
                "the streak is kept (flash spawn: {spawn_flash})"
            );
            assert!(
                !drawn
                    .iter()
                    .any(|i| i.material == 3 && translation(i).x.abs() < EPS),
                "the oldest fixed-lifetime effect, a flash, is gone (flash spawn: {spawn_flash})"
            );
            assert!(
                drawn
                    .iter()
                    .any(|i| (translation(i).x - 10.0).abs() < EPS && i.material == 9),
                "the next oldest stays"
            );
            let new_material = if spawn_flash { 3 } else { 9 };
            assert!(
                drawn
                    .iter()
                    .any(|i| i.material == new_material && (translation(i).x - 1000.0).abs() < 0.1),
                "the newcomer is in"
            );
        }
    }

    #[test]
    fn roll_turns_a_mark_about_its_normal() {
        let style = mark_style(0.2, Duration::from_secs(10), ms(1000));
        let mut first = Rig::new();
        first.effects.mark(Vec3::ZERO, Vec3::Z, 0.0, style);
        let mut second = Rig::new();
        second
            .effects
            .mark(Vec3::ZERO, Vec3::Z, std::f32::consts::FRAC_PI_2, style);

        let before = first.frame(EYE);
        let after = second.frame(EYE);

        assert_eq!((before.len(), after.len()), (1, 1));
        let turned = x_axis(&after[0]);
        let original_y = y_axis(&before[0]);
        assert!(
            (turned - original_y).length() < EPS || (turned + original_y).length() < EPS,
            "a quarter turn sends X to Y: {turned} vs {original_y}"
        );
        assert!(
            (turned - x_axis(&before[0])).length() > 0.1,
            "it actually turned"
        );
    }

    #[test]
    fn roll_turns_a_flash_about_the_axis_to_the_eye() {
        let at = Vec3::new(1.0, 2.0, 3.0);
        let mut first = Rig::new();
        first.effects.flash(at, 0.0, flash_style(0.4, ms(50)));
        let mut second = Rig::new();
        second
            .effects
            .flash(at, std::f32::consts::FRAC_PI_2, flash_style(0.4, ms(50)));

        let before = first.frame(EYE);
        let after = second.frame(EYE);

        assert_eq!((before.len(), after.len()), (1, 1));
        let turned = x_axis(&after[0]);
        let original_y = y_axis(&before[0]);
        assert!(
            (turned - original_y).length() < EPS || (turned + original_y).length() < EPS,
            "a quarter turn sends X to Y: {turned} vs {original_y}"
        );
        assert!(
            (turned - x_axis(&before[0])).length() > 0.1,
            "it actually turned"
        );
        let axis_before = x_axis(&before[0]).cross(y_axis(&before[0])).normalize();
        let axis_after = x_axis(&after[0]).cross(y_axis(&after[0])).normalize();
        assert_near(axis_after, axis_before);
        assert!(
            axis_after.dot((EYE - at).normalize()) > 0.99,
            "still faces the eye"
        );
    }

    fn is_finite_instance(instance: &InstanceGpu) -> bool {
        instance.model.iter().flatten().all(|v| v.is_finite())
    }

    #[test]
    fn a_zero_lifetime_flash_draws_nothing_before_any_advance() {
        let mut rig = Rig::new();
        rig.effects
            .flash(Vec3::X, 0.0, flash_style(0.4, Duration::ZERO));

        let drawn = rig.frame(EYE);

        assert!(drawn.is_empty(), "gone at its lifetime: {drawn:?}");
    }

    #[test]
    fn a_zero_lifetime_mark_draws_nothing_before_any_advance() {
        for fade in [Duration::ZERO, Duration::from_secs(1)] {
            let mut rig = Rig::new();
            rig.effects
                .mark(Vec3::X, Vec3::Y, 0.0, mark_style(0.2, Duration::ZERO, fade));

            let drawn = rig.frame(EYE);

            assert!(drawn.is_empty(), "fade {fade:?} drew {drawn:?}");
        }
    }

    #[test]
    fn a_zero_lifetime_mark_is_not_live_after_the_first_advance() {
        for fade in [Duration::ZERO, Duration::from_secs(1)] {
            let mut rig = Rig::new();
            rig.effects
                .mark(Vec3::X, Vec3::Y, 0.0, mark_style(0.2, Duration::ZERO, fade));

            rig.effects.advance(ms(1));

            assert_eq!(rig.effects.live(), 0, "fade {fade:?}");
            assert!(rig.frame(EYE).is_empty(), "fade {fade:?}");
        }
    }

    #[test]
    fn a_flash_with_a_bad_size_draws_nothing_finite_wrong() {
        for size in [f32::NAN, f32::INFINITY, 0.0, -0.4, f32::NEG_INFINITY] {
            let mut rig = Rig::new();
            rig.effects.flash(Vec3::X, 0.0, flash_style(size, ms(50)));

            let drawn = rig.frame(EYE);

            assert!(
                drawn.iter().all(is_finite_instance),
                "size {size} drew a non-finite instance: {drawn:?}"
            );
            if size.is_nan() || size <= 0.0 {
                assert!(drawn.is_empty(), "size {size} drew {drawn:?}");
            }
        }
    }

    #[test]
    fn a_mark_with_a_bad_size_draws_nothing_finite_wrong() {
        for size in [f32::NAN, f32::INFINITY, 0.0, -0.2, f32::NEG_INFINITY] {
            let mut rig = Rig::new();
            rig.effects.mark(
                Vec3::X,
                Vec3::Y,
                0.0,
                mark_style(size, Duration::from_secs(10), ms(1000)),
            );

            let drawn = rig.frame(EYE);

            assert!(
                drawn.iter().all(is_finite_instance),
                "size {size} drew a non-finite instance: {drawn:?}"
            );
            if size.is_nan() || size <= 0.0 {
                assert!(drawn.is_empty(), "size {size} drew {drawn:?}");
            }
        }
    }

    #[test]
    fn a_mark_whose_fade_exceeds_its_lifetime_only_shrinks_and_is_gone_at_its_lifetime() {
        let mut rig = Rig::new();
        let size = 0.2;
        rig.effects.mark(
            Vec3::X,
            Vec3::Y,
            0.0,
            mark_style(size, Duration::from_secs(1), Duration::from_secs(4)),
        );

        let mut previous = f32::INFINITY;
        for step in 0..10 {
            let drawn = rig.frame(EYE);

            assert_eq!(drawn.len(), 1, "alive at step {step}");
            assert!(is_finite_instance(&drawn[0]), "step {step}");
            let scale = x_axis(&drawn[0]).length();
            assert!(scale <= size + EPS, "step {step}: {scale} above size");
            assert!(scale <= previous + EPS, "step {step}: grew to {scale}");
            previous = scale;
            rig.effects.advance(ms(100));
        }

        assert!(rig.frame(EYE).is_empty(), "gone at 1 s");
        assert_eq!(rig.effects.live(), 0);
    }

    #[test]
    fn lowering_quality_then_spawning_a_flash_never_grows_live_and_drops_the_oldest() {
        let mut rig = Rig::new();
        let mark = mark_style(0.2, Duration::from_secs(10), ms(1000));
        for i in 0..100u8 {
            rig.effects
                .mark(Vec3::new(f32::from(i) * 10.0, 0.0, 0.0), Vec3::Y, 0.0, mark);
        }
        assert_eq!(rig.effects.live(), 100);
        rig.effects.set_quality(1);
        let before = rig.effects.live();

        rig.effects
            .flash(Vec3::new(5000.0, 0.0, 0.0), 0.0, flash_style(0.4, ms(50)));

        assert!(rig.effects.live() <= before, "live grew past {before}");
        assert_eq!(rig.effects.live(), 100);
    }
}
