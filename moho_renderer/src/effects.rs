//! Engine-drawn effects: streaks are placed by handle (a game starts one, places it every
//! frame and ends it), while flashes and marks are fire-and-forget with a fixed lifetime.
//! The engine owns their life, fade, shared cap and unshadowed draw.

use crate::WorldMeshes;
use glam::{Affine3A, Quat, Vec3};
use moho_render_api::{WorldMesh, WorldMeshId};
use std::collections::{BTreeMap, BTreeSet};
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

/// How far a mark is lifted off its surface along the normal.
pub const MARK_OFFSET: f32 = 0.01;

/// A camera-facing star that shrinks to nothing over its lifetime; `size` is tip to tip.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FlashStyle {
    pub size: f32,
    pub material: u32,
    pub lifetime: Duration,
}

impl FlashStyle {
    pub fn new(size: f32, material: u32, lifetime: Duration) -> Self {
        Self {
            size,
            material,
            lifetime,
        }
    }
}

/// A square decal on a surface: full size until `lifetime - fade`, then shrinking to zero at
/// `lifetime`. A `fade` longer than the lifetime is clamped to it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MarkStyle {
    pub size: f32,
    pub material: u32,
    pub lifetime: Duration,
    pub fade: Duration,
}

impl MarkStyle {
    pub fn new(size: f32, material: u32, lifetime: Duration, fade: Duration) -> Self {
        Self {
            size,
            material,
            lifetime,
            fade,
        }
    }
}

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

/// What a fixed-lifetime effect looks like and where it faces.
#[derive(Debug)]
enum Shape {
    /// Faces the eye at draw time, rolled about the line of sight.
    Flash { roll: f32 },
    /// Lies on a surface: unit `x` in the surface, unit `normal` out of it.
    Mark { x: Vec3, normal: Vec3 },
}

/// A flash or mark: it lives for `lifetime`, full size until `lifetime - fade`.
#[derive(Debug)]
struct Timed {
    shape: Shape,
    at: Vec3,
    size: f32,
    material: u32,
    lifetime: Duration,
    /// Never longer than `lifetime`.
    fade: Duration,
    age: Duration,
}

impl Timed {
    /// The size factor: 1 until the fade starts, then falling linearly to 0.
    fn scale(&self) -> f32 {
        if self.fade.is_zero() {
            return 1.0;
        }
        let left = self.lifetime.saturating_sub(self.age);
        (left.as_secs_f32() / self.fade.as_secs_f32()).clamp(0.0, 1.0)
    }

    /// `None` when a flash cannot face an eye at its own position or a non-finite one.
    fn transform(&self, eye: Vec3) -> Option<Affine3A> {
        let size = self.size * self.scale();
        let (x, y, z) = match self.shape {
            Shape::Flash { roll } => {
                let facing = (eye - self.at).try_normalize()?;
                let x = rolled_perpendicular(facing, roll);
                (x, facing.cross(x), facing)
            }
            Shape::Mark { x, normal } => (x, normal.cross(x), normal),
        };
        Some(Affine3A::from_cols(
            (x * size).into(),
            (y * size).into(),
            z.into(),
            self.at.into(),
        ))
    }

    fn mesh(&self) -> MeshKind {
        match self.shape {
            Shape::Flash { .. } => MeshKind::Flash,
            Shape::Mark { .. } => MeshKind::Mark,
        }
    }
}

#[derive(Debug)]
enum Effect {
    Streak(Streak),
    Timed(Timed),
}

/// The engine's effect meshes, in reserved-id order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum MeshKind {
    Streak,
    Flash,
    Mark,
}

impl MeshKind {
    fn id(self) -> WorldMeshId {
        let offset = match self {
            Self::Streak => 0,
            Self::Flash => 1,
            Self::Mark => 2,
        };
        WorldMeshId(EFFECT_MESH_IDS_FROM + offset)
    }

    fn build(self) -> WorldMesh {
        match self {
            Self::Streak => streak_mesh(),
            Self::Flash => flash_mesh(),
            Self::Mark => mark_mesh(),
        }
    }
}

#[derive(Debug)]
pub struct Effects {
    store: BTreeMap<EffectHandle, Effect>,
    next_handle: u64,
    cap: usize,
    uploaded: BTreeSet<MeshKind>,
}

impl Default for Effects {
    fn default() -> Self {
        Self {
            store: BTreeMap::new(),
            next_handle: 0,
            cap: CAPS[DEFAULT_QUALITY],
            uploaded: BTreeSet::new(),
        }
    }
}

impl Effects {
    fn issue_handle(&mut self) -> EffectHandle {
        let handle = EffectHandle(self.next_handle);
        self.next_handle += 1;
        handle
    }

    /// Start a streak. At the quality's cap the handle is inert: never stored, so placing
    /// or ending it does nothing.
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

    /// Spawn a flash at `at`, turned by `roll` about the line of sight. A zero lifetime, a
    /// non-finite or non-positive size or a non-finite `at` or `roll` is refused.
    pub fn flash(&mut self, at: Vec3, roll: f32, style: FlashStyle) {
        if !at.is_finite() || !roll.is_finite() {
            return;
        }
        self.spawn_timed(Timed {
            shape: Shape::Flash { roll },
            at,
            size: style.size,
            material: style.material,
            lifetime: style.lifetime,
            fade: style.lifetime,
            age: Duration::ZERO,
        });
    }

    /// Spawn a mark on a surface at `at` facing along `normal`, turned by `roll` in the
    /// surface and lifted by [`MARK_OFFSET`]. A zero or non-finite `normal`, a non-finite
    /// `at` or `roll`, a non-finite or non-positive size or a zero lifetime is refused.
    pub fn mark(&mut self, at: Vec3, normal: Vec3, roll: f32, style: MarkStyle) {
        let Some(normal) = normal.try_normalize() else {
            return;
        };
        if !at.is_finite() || !roll.is_finite() {
            return;
        }
        let x = rolled_perpendicular(normal, roll);
        self.spawn_timed(Timed {
            shape: Shape::Mark { x, normal },
            at: at + normal * MARK_OFFSET,
            size: style.size,
            material: style.material,
            lifetime: style.lifetime,
            fade: style.fade.min(style.lifetime),
            age: Duration::ZERO,
        });
    }

    /// Store `timed` unless it is degenerate or the cap is full of streaks. At the cap the
    /// oldest fixed-lifetime effect makes room.
    fn spawn_timed(&mut self, timed: Timed) {
        if timed.lifetime.is_zero() || !timed.size.is_finite() || timed.size <= 0.0 {
            return;
        }
        if self.store.len() >= self.cap {
            let oldest = self
                .store
                .iter()
                .find_map(|(handle, effect)| matches!(effect, Effect::Timed(_)).then_some(*handle));
            let Some(oldest) = oldest else {
                return;
            };
            self.store.remove(&oldest);
        }
        let handle = self.issue_handle();
        self.store.insert(handle, Effect::Timed(timed));
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
    /// and drop the finished ones.
    pub fn advance(&mut self, dt: Duration) {
        for effect in self.store.values_mut() {
            match effect {
                Effect::Streak(streak) => {
                    streak.age += dt;
                    if streak.age >= streak.style.max_age {
                        streak.ended_at.get_or_insert(streak.style.max_age);
                    }
                }
                Effect::Timed(timed) => timed.age += dt,
            }
        }
        self.store.retain(|_, effect| match effect {
            Effect::Streak(streak) => !streak.finished_fading(),
            Effect::Timed(timed) => timed.age < timed.lifetime,
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

    /// Place every streak placed this frame or fading, and every flash and mark,
    /// unshadowed; streaks and flashes face `eye`.
    pub(crate) fn draw(&mut self, meshes: &mut WorldMeshes, eye: Vec3) {
        for effect in self.store.values_mut() {
            let (kind, transform, material) = match effect {
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
                    (MeshKind::Streak, transform, streak.style.material)
                }
                Effect::Timed(timed) => (timed.mesh(), timed.transform(eye), timed.material),
            };
            let Some(transform) = transform else {
                continue;
            };
            if self.uploaded.insert(kind) {
                meshes.upsert_movable(kind.id(), kind.build());
            }
            meshes.place_unshadowed(kind.id(), transform, material);
        }
    }
}

/// A unit vector perpendicular to the unit `axis`, turned by `roll` radians about it.
fn rolled_perpendicular(axis: Vec3, roll: f32) -> Vec3 {
    Quat::from_axis_angle(axis, roll) * axis.any_orthonormal_vector()
}

/// A mesh flat in the XY plane with +Z normals and neutral lighting channels.
fn flat_mesh(positions: Vec<[f32; 3]>, indices: Vec<u32>) -> WorldMesh {
    let count = positions.len();
    WorldMesh::new(
        positions,
        vec![[0.0, 0.0, 1.0]; count],
        vec![1.0; count],
        vec![[1.0; 3]; count],
        vec![1.0; count],
        vec![0; count],
        indices,
    )
    .expect("every channel has one entry per vertex and the indices are in range")
}

/// A kite from the tail at the origin to the head at x = 1, counter-clockwise seen from +Z.
fn streak_mesh() -> WorldMesh {
    flat_mesh(
        vec![
            [0.0, 0.0, 0.0],
            [0.8, -0.5, 0.0],
            [1.0, 0.0, 0.0],
            [0.8, 0.5, 0.0],
        ],
        vec![0, 1, 2, 0, 2, 3],
    )
}

/// A four-point star centred on the origin, tips at radius 0.5 on the axes, counter-clockwise
/// seen from +Z; even vertices are the tips.
fn flash_mesh() -> WorldMesh {
    flat_mesh(
        vec![
            [0.5, 0.0, 0.0],
            [0.15, 0.15, 0.0],
            [0.0, 0.5, 0.0],
            [-0.15, 0.15, 0.0],
            [-0.5, 0.0, 0.0],
            [-0.15, -0.15, 0.0],
            [0.0, -0.5, 0.0],
            [0.15, -0.15, 0.0],
        ],
        vec![
            7, 0, 1, 1, 2, 3, 3, 4, 5, 5, 6, 7, // the tips
            1, 3, 5, 1, 5, 7, // the inner square
        ],
    )
}

/// A unit quad centred on the origin, counter-clockwise seen from +Z.
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

    fn flash_style(size: f32, material: u32) -> FlashStyle {
        FlashStyle::new(size, material, ms(50))
    }

    fn mark_style(size: f32, material: u32) -> MarkStyle {
        MarkStyle::new(
            size,
            material,
            Duration::from_secs(10),
            Duration::from_secs(1),
        )
    }

    fn assert_finite(instance: &InstanceGpu) {
        assert!(
            instance.model.iter().flatten().all(|v| v.is_finite()),
            "non-finite model {:?}",
            instance.model
        );
    }

    #[test]
    fn a_flash_shrinks_linearly_and_expires() {
        let mut rig = Rig::new();
        let at = Vec3::new(2.0, 3.0, 4.0);
        rig.effects.flash(at, 0.0, flash_style(0.4, 5));

        let full = rig.frame(EYE);
        assert_eq!(full.len(), 1);
        assert_eq!(rig.effects.live(), 1);
        assert_near(translation(&full[0]), at);
        assert!((x_axis(&full[0]).length() - 0.4).abs() < EPS);
        assert!((y_axis(&full[0]).length() - 0.4).abs() < EPS);
        assert_eq!(full[0].material, 5, "the style's material is carried");

        rig.effects.advance(ms(25));
        let half = rig.frame(EYE);
        assert_eq!(half.len(), 1, "drawn without being placed again");
        assert_near(translation(&half[0]), at);
        assert!((x_axis(&half[0]).length() - 0.2).abs() < EPS);
        assert!((y_axis(&half[0]).length() - 0.2).abs() < EPS);

        rig.effects.advance(ms(25));
        assert!(rig.frame(EYE).is_empty());
        assert_eq!(rig.effects.live(), 0);
    }

    #[test]
    fn a_flash_faces_the_eye() {
        for roll in [0.0, 0.7, std::f32::consts::FRAC_PI_2] {
            let mut rig = Rig::new();
            let at = Vec3::new(2.0, 3.0, 4.0);
            rig.effects.flash(at, roll, flash_style(0.4, 0));

            let drawn = rig.frame(EYE);

            assert_eq!(drawn.len(), 1);
            let (x, y) = (x_axis(&drawn[0]), y_axis(&drawn[0]));
            assert!(x.dot(y).abs() < EPS, "roll {roll}: axes are perpendicular");
            assert!(
                x.cross(y).dot(EYE - at) > 0.0,
                "roll {roll}: X x Y does not point toward the eye"
            );
            assert!(
                x.dot(EYE - at).abs() < EPS && y.dot(EYE - at).abs() < EPS,
                "roll {roll}: the star lies across the line of sight"
            );
        }
    }

    #[test]
    fn a_flash_roll_turns_it_about_its_facing() {
        let at = Vec3::new(2.0, 3.0, 4.0);
        let mut flat = Rig::new();
        flat.effects.flash(at, 0.0, flash_style(0.4, 0));
        let mut turned = Rig::new();
        turned
            .effects
            .flash(at, std::f32::consts::FRAC_PI_2, flash_style(0.4, 0));

        let a = flat.frame(EYE)[0];
        let b = turned.frame(EYE)[0];

        assert!((x_axis(&a).length() - x_axis(&b).length()).abs() < EPS);
        assert!(
            x_axis(&a).normalize().dot(x_axis(&b).normalize()).abs() < EPS,
            "a quarter turn of roll turns the star a quarter turn"
        );
        assert!(
            x_axis(&a)
                .cross(y_axis(&a))
                .normalize()
                .dot(x_axis(&b).cross(y_axis(&b)).normalize())
                > 0.999,
            "roll does not change the facing"
        );
    }

    #[test]
    fn a_flash_follows_the_eye_while_it_lives() {
        let at = Vec3::new(2.0, 3.0, 4.0);
        let second_eye = Vec3::new(2.0, 30.0, 4.5);
        let mut rig = Rig::new();
        rig.effects.flash(at, 0.0, flash_style(0.4, 0));
        let first = rig.frame(EYE)[0];
        rig.effects.advance(ms(1));

        let later = rig.frame(second_eye);

        assert_eq!(later.len(), 1, "still live");
        let (x, y) = (x_axis(&later[0]).normalize(), y_axis(&later[0]).normalize());
        let toward = (second_eye - at).normalize();
        assert!(
            x.dot(toward).abs() < EPS,
            "X is across the new line of sight"
        );
        assert!(
            y.dot(toward).abs() < EPS,
            "Y is across the new line of sight"
        );
        assert!(x.dot(y).abs() < EPS);
        assert!(
            x.cross(y).dot(toward) > 0.999,
            "faces the second eye, not the first"
        );
        let first_facing = x_axis(&first).cross(y_axis(&first)).normalize();
        assert!(
            first_facing.dot(x.cross(y)) < 0.999,
            "the facing changed between frames"
        );
    }

    #[test]
    fn the_flash_mesh_is_a_four_point_star_in_the_xy_plane() {
        let mut rig = Rig::new();
        rig.effects.flash(Vec3::ZERO, 0.0, flash_style(0.4, 0));

        rig.frame(EYE);
        rig.frame(EYE);

        assert_eq!(rig.backend.registered.len(), 1, "uploaded once");
        assert_eq!(rig.meshes.live_len(), 1);
        let (positions, indices) = &rig.backend.registered[0];
        assert_eq!(positions.len(), 8);
        assert!(positions.iter().all(|p| p[2] == 0.0), "flat in XY");
        let radii: Vec<f32> = positions.iter().map(|p| Vec3::from(*p).length()).collect();
        let tips = radii.iter().filter(|r| (**r - 0.5).abs() < EPS).count();
        let inner = radii.iter().filter(|r| **r < 0.3).count();
        assert_eq!(tips, 4, "four tips at radius 0.5, got {radii:?}");
        assert_eq!(inner, 4, "four inner vertices inside 0.3, got {radii:?}");
        let centre: Vec3 = positions.iter().map(|p| Vec3::from(*p)).sum::<Vec3>() / 8.0;
        assert!(centre.length() < EPS, "centred on the origin");
        assert!(!indices.is_empty() && indices.len() % 3 == 0);
        for triangle in indices.chunks(3) {
            let [a, b, c] = [0, 1, 2].map(|i| Vec3::from(positions[triangle[i] as usize]));
            assert!((b - a).cross(c - a).z > 0.0, "counter-clockwise from +Z");
        }
    }

    #[test]
    fn a_mark_lies_along_its_normal_offset_by_the_mark_offset() {
        let mut rig = Rig::new();
        let at = Vec3::new(5.0, 1.0, -2.0);
        let normal = Vec3::new(1.0, 2.0, 2.0).normalize();
        rig.effects.mark(at, normal, 0.3, mark_style(0.2, 6));

        let drawn = rig.frame(EYE);

        assert_eq!(drawn.len(), 1);
        let d = &drawn[0];
        assert_near(translation(d), at + normal * MARK_OFFSET);
        assert!((x_axis(d).length() - 0.2).abs() < EPS);
        assert!((y_axis(d).length() - 0.2).abs() < EPS);
        assert_near(z_axis(d), normal);
        assert!(x_axis(d).dot(normal).abs() < EPS, "X lies in the surface");
        assert!(y_axis(d).dot(normal).abs() < EPS, "Y lies in the surface");
        assert!(x_axis(d).dot(y_axis(d)).abs() < EPS);
        assert!(
            x_axis(d).cross(y_axis(d)).dot(normal) > 0.0,
            "faces along the normal"
        );
        assert_eq!(d.material, 6);
    }

    #[test]
    fn a_mark_does_not_follow_the_eye() {
        let at = Vec3::new(5.0, 1.0, -2.0);
        let mut near = Rig::new();
        near.effects.mark(at, Vec3::Y, 0.3, mark_style(0.2, 0));
        let mut far = Rig::new();
        far.effects.mark(at, Vec3::Y, 0.3, mark_style(0.2, 0));

        let a = near.frame(EYE)[0];
        let b = far.frame(Vec3::new(-40.0, 2.0, 17.0))[0];

        assert_eq!(a.model, b.model);
    }

    #[test]
    fn a_mark_roll_turns_it_in_the_surface() {
        let at = Vec3::new(5.0, 1.0, -2.0);
        let mut flat = Rig::new();
        flat.effects.mark(at, Vec3::Y, 0.0, mark_style(0.2, 0));
        let mut rolled = Rig::new();
        rolled
            .effects
            .mark(at, Vec3::Y, std::f32::consts::FRAC_PI_2, mark_style(0.2, 0));

        let a = flat.frame(EYE)[0];
        let b = rolled.frame(EYE)[0];

        assert_near(z_axis(&a), z_axis(&b));
        assert_near(translation(&a), translation(&b));
        assert!(x_axis(&a).dot(x_axis(&b)).abs() < EPS, "a quarter turn");
    }

    #[test]
    fn the_mark_mesh_is_a_unit_quad_in_the_xy_plane() {
        let mut rig = Rig::new();
        rig.effects
            .mark(Vec3::ZERO, Vec3::Y, 0.0, mark_style(0.2, 0));

        rig.frame(EYE);

        assert_eq!(rig.backend.registered.len(), 1);
        let (positions, indices) = &rig.backend.registered[0];
        assert_eq!(positions.len(), 4);
        assert_eq!(indices.len(), 6, "two triangles");
        assert!(positions.iter().all(|p| p[2] == 0.0));
        for axis in 0..2 {
            let lo = positions.iter().map(|p| p[axis]).fold(f32::MAX, f32::min);
            let hi = positions.iter().map(|p| p[axis]).fold(f32::MIN, f32::max);
            assert!((hi - lo - 1.0).abs() < EPS, "axis {axis} spans 1");
            assert!((hi + lo).abs() < EPS, "axis {axis} is centred");
        }
        for triangle in indices.chunks(3) {
            let [a, b, c] = [0, 1, 2].map(|i| Vec3::from(positions[triangle[i] as usize]));
            assert!((b - a).cross(c - a).z > 0.0, "counter-clockwise from +Z");
        }
    }

    #[test]
    fn flash_and_mark_use_distinct_meshes_outside_the_streak_mesh() {
        let mut rig = Rig::new();
        let handle = rig.effects.begin_streak(style(2.0, 0.1));
        rig.effects.place_streak(handle, Vec3::X, Vec3::X);
        rig.effects.flash(Vec3::ZERO, 0.0, flash_style(0.4, 1));
        rig.effects
            .mark(Vec3::ZERO, Vec3::Y, 0.0, mark_style(0.2, 2));

        let drawn = rig.frame(EYE);

        assert_eq!(drawn.len(), 3);
        assert_eq!(rig.backend.registered.len(), 3, "one mesh per kind");
        assert_eq!(rig.meshes.live_len(), 3);
        for offset in 0..3 {
            rig.meshes
                .remove(WorldMeshId(EFFECT_MESH_IDS_FROM + offset));
        }
        rig.meshes.flush(&mut rig.backend);
        assert_eq!(
            rig.meshes.live_len(),
            0,
            "the streak, flash and mark meshes are the first three reserved ids"
        );
        let sizes: std::collections::BTreeSet<usize> = rig
            .backend
            .registered
            .iter()
            .map(|(positions, _)| positions.len())
            .collect();
        assert_eq!(sizes, [4, 8].into(), "kite and quad share a count");
    }

    #[test]
    fn a_mark_holds_full_size_then_shrinks_out_in_its_fade() {
        let mut rig = Rig::new();
        let at = Vec3::new(1.0, 1.0, 1.0);
        rig.effects.mark(at, Vec3::Y, 0.0, mark_style(0.2, 0));

        rig.effects.advance(Duration::from_secs(9));
        let full = rig.frame(EYE);
        assert_eq!(full.len(), 1);
        assert!((x_axis(&full[0]).length() - 0.2).abs() < EPS, "full at 9 s");

        rig.effects.advance(ms(500));
        let half = rig.frame(EYE);
        assert_eq!(half.len(), 1);
        assert!(
            (x_axis(&half[0]).length() - 0.1).abs() < EPS,
            "half at 9.5 s"
        );
        assert!((y_axis(&half[0]).length() - 0.1).abs() < EPS);
        assert_near(translation(&half[0]), translation(&full[0]));
        assert_eq!(rig.effects.live(), 1);

        rig.effects.advance(ms(500));
        assert!(rig.frame(EYE).is_empty(), "gone at 10 s");
        assert_eq!(rig.effects.live(), 0);
    }

    #[test]
    fn a_mark_and_a_streak_never_share_a_handle() {
        let mut rig = Rig::new();
        rig.effects
            .mark(Vec3::new(3.0, 0.0, 0.0), Vec3::Y, 0.0, mark_style(0.2, 6));
        let handle = rig.effects.begin_streak(style(2.0, 0.1));

        rig.effects
            .place_streak(handle, Vec3::new(0.0, 5.0, 0.0), Vec3::X);
        let drawn = rig.frame(EYE);

        assert_eq!(rig.effects.live(), 2);
        assert_eq!(drawn.len(), 2, "the placed streak and the mark both draw");
        assert_eq!(drawn.iter().filter(|d| d.material == 0).count(), 1);
        assert_eq!(drawn.iter().filter(|d| d.material == 6).count(), 1);
    }

    #[test]
    fn degenerate_flash_and_mark_spawns_are_refused() {
        let nan = f32::NAN;
        let inf = f32::INFINITY;
        let flashes = [
            ("zero lifetime", FlashStyle::new(0.4, 0, Duration::ZERO)),
            ("zero size", flash_style(0.0, 0)),
            ("negative size", flash_style(-0.4, 0)),
            ("NaN size", flash_style(nan, 0)),
            ("infinite size", flash_style(inf, 0)),
        ];
        for (case, flash) in flashes {
            let mut rig = Rig::new();

            rig.effects.flash(Vec3::ZERO, 0.0, flash);

            assert_eq!(rig.effects.live(), 0, "flash with {case}");
            assert!(rig.frame(EYE).is_empty(), "flash with {case}");
        }
        let zero_lifetime = MarkStyle::new(0.2, 0, Duration::ZERO, Duration::ZERO);
        let marks = [
            ("zero normal", Vec3::ZERO, Vec3::ZERO, mark_style(0.2, 0)),
            (
                "NaN normal",
                Vec3::ZERO,
                Vec3::splat(nan),
                mark_style(0.2, 0),
            ),
            (
                "infinite normal",
                Vec3::ZERO,
                Vec3::splat(inf),
                mark_style(0.2, 0),
            ),
            (
                "NaN position",
                Vec3::new(nan, 0.0, 0.0),
                Vec3::Y,
                mark_style(0.2, 0),
            ),
            (
                "infinite position",
                Vec3::new(inf, 0.0, 0.0),
                Vec3::Y,
                mark_style(0.2, 0),
            ),
            ("zero size", Vec3::ZERO, Vec3::Y, mark_style(0.0, 0)),
            ("negative size", Vec3::ZERO, Vec3::Y, mark_style(-0.2, 0)),
            ("NaN size", Vec3::ZERO, Vec3::Y, mark_style(nan, 0)),
            ("infinite size", Vec3::ZERO, Vec3::Y, mark_style(inf, 0)),
            ("zero lifetime", Vec3::ZERO, Vec3::Y, zero_lifetime),
        ];
        for (case, at, normal, mark) in marks {
            let mut rig = Rig::new();

            rig.effects.mark(at, normal, 0.0, mark);

            assert_eq!(rig.effects.live(), 0, "mark with {case}");
            assert!(rig.frame(EYE).is_empty(), "mark with {case}");
        }
    }

    #[test]
    fn a_mark_fade_is_clamped_to_its_lifetime_and_a_zero_fade_is_abrupt() {
        let mut clamped = Rig::new();
        let long_fade = MarkStyle::new(0.2, 0, Duration::from_secs(1), Duration::from_secs(5));
        clamped.effects.mark(Vec3::ZERO, Vec3::Y, 0.0, long_fade);

        let spawn = clamped.frame(EYE);
        clamped.effects.advance(ms(500));
        let half = clamped.frame(EYE);

        assert_eq!(spawn.len(), 1);
        assert!(
            (x_axis(&spawn[0]).length() - 0.2).abs() < EPS,
            "full at spawn"
        );
        assert_eq!(half.len(), 1);
        assert!(
            (x_axis(&half[0]).length() - 0.1).abs() < EPS,
            "half at half the lifetime, got {}",
            x_axis(&half[0]).length()
        );
        clamped.effects.advance(ms(500));
        assert!(clamped.frame(EYE).is_empty(), "gone at its lifetime");
        assert_eq!(clamped.effects.live(), 0);

        let mut abrupt = Rig::new();
        let no_fade = MarkStyle::new(0.2, 0, Duration::from_secs(1), Duration::ZERO);
        abrupt.effects.mark(Vec3::ZERO, Vec3::Y, 0.0, no_fade);

        abrupt.effects.advance(ms(999));
        let late = abrupt.frame(EYE);

        assert_eq!(late.len(), 1);
        assert!(
            (x_axis(&late[0]).length() - 0.2).abs() < EPS,
            "full size until its lifetime"
        );
        abrupt.effects.advance(ms(1));
        assert!(abrupt.frame(EYE).is_empty(), "gone at its lifetime");
        assert_eq!(abrupt.effects.live(), 0);
    }

    #[test]
    fn a_full_cap_drops_the_oldest_fixed_lifetime_effect() {
        let mut rig = Rig::new();
        rig.effects.set_quality(1);
        for i in 0..32 {
            rig.effects.mark(
                Vec3::new(i as f32, 0.0, 0.0),
                Vec3::Y,
                0.0,
                mark_style(0.2, 0),
            );
        }
        assert_eq!(rig.effects.live(), 32);

        rig.effects
            .mark(Vec3::new(100.0, 0.0, 0.0), Vec3::Y, 0.0, mark_style(0.2, 0));

        assert_eq!(rig.effects.live(), 32);
        let xs: Vec<f32> = rig.frame(EYE).iter().map(|d| translation(d).x).collect();
        assert_eq!(xs.len(), 32);
        assert!(xs.iter().all(|x| *x > 0.5), "the oldest, at x = 0, is gone");
        assert!(
            xs.iter().any(|x| (*x - 1.0).abs() < EPS),
            "the second stays"
        );
        assert!(
            xs.iter().any(|x| (*x - 100.0).abs() < EPS),
            "the new one is in"
        );
    }

    #[test]
    fn a_full_cap_evicts_an_old_flash_before_a_newer_mark() {
        let mut rig = Rig::new();
        rig.effects.set_quality(1);
        rig.effects
            .flash(Vec3::new(-7.0, 0.0, 0.0), 0.0, flash_style(0.4, 3));
        for i in 0..31 {
            rig.effects.mark(
                Vec3::new(i as f32, 0.0, 0.0),
                Vec3::Y,
                0.0,
                mark_style(0.2, 0),
            );
        }

        rig.effects
            .mark(Vec3::new(100.0, 0.0, 0.0), Vec3::Y, 0.0, mark_style(0.2, 0));

        assert_eq!(rig.effects.live(), 32);
        let drawn = rig.frame(EYE);
        assert_eq!(drawn.len(), 32);
        assert!(drawn.iter().all(|d| d.material == 0), "the flash is gone");
    }

    #[test]
    fn a_full_cap_drops_the_oldest_when_a_flash_arrives() {
        let mut rig = Rig::new();
        rig.effects.set_quality(1);
        for i in 0..32 {
            rig.effects.mark(
                Vec3::new(i as f32, 0.0, 0.0),
                Vec3::Y,
                0.0,
                mark_style(0.2, 0),
            );
        }

        rig.effects
            .flash(Vec3::new(100.0, 0.0, 0.0), 0.0, flash_style(0.4, 3));

        assert_eq!(rig.effects.live(), 32);
        let drawn = rig.frame(EYE);
        assert_eq!(drawn.len(), 32);
        assert_eq!(drawn.iter().filter(|d| d.material == 3).count(), 1);
        assert!(
            drawn
                .iter()
                .all(|d| translation(d).x > 0.5 || d.material == 3)
        );
    }

    #[test]
    fn a_full_cap_of_streaks_refuses_a_mark_and_keeps_every_streak() {
        let mut rig = Rig::new();
        rig.effects.set_quality(1);
        let handles: Vec<EffectHandle> = (0..32)
            .map(|_| rig.effects.begin_streak(style(2.0, 0.1)))
            .collect();

        rig.effects
            .mark(Vec3::ZERO, Vec3::Y, 0.0, mark_style(0.2, 9));
        rig.effects.flash(Vec3::ZERO, 0.0, flash_style(0.4, 9));

        assert_eq!(rig.effects.live(), 32);
        for handle in &handles {
            rig.effects.place_streak(*handle, Vec3::X, Vec3::X);
        }
        let drawn = rig.frame(EYE);
        assert_eq!(drawn.len(), 32, "every streak still draws");
        assert!(drawn.iter().all(|d| d.material == 0), "no mark or flash");
    }

    #[test]
    fn a_mark_evicts_the_oldest_mark_but_never_an_older_streak() {
        let mut rig = Rig::new();
        rig.effects.set_quality(1);
        let streak = rig.effects.begin_streak(style(2.0, 0.1));
        for i in 0..31 {
            rig.effects.mark(
                Vec3::new(i as f32, 0.0, 0.0),
                Vec3::Y,
                0.0,
                mark_style(0.2, 4),
            );
        }

        rig.effects
            .mark(Vec3::new(100.0, 0.0, 0.0), Vec3::Y, 0.0, mark_style(0.2, 4));

        assert_eq!(rig.effects.live(), 32);
        rig.effects
            .place_streak(streak, Vec3::new(-50.0, 0.0, 0.0), Vec3::X);
        let drawn = rig.frame(EYE);
        assert_eq!(drawn.len(), 32, "the streak and 31 marks");
        assert_eq!(drawn.iter().filter(|d| d.material == 0).count(), 1);
        let mark_xs: Vec<f32> = drawn
            .iter()
            .filter(|d| d.material == 4)
            .map(|d| translation(d).x)
            .collect();
        assert!(mark_xs.iter().all(|x| *x > 0.5), "the oldest mark is gone");
    }

    #[test]
    fn quality_zero_refuses_flash_and_mark() {
        let mut rig = Rig::new();
        rig.effects.set_quality(0);

        rig.effects.flash(Vec3::ZERO, 0.0, flash_style(0.4, 1));
        rig.effects
            .mark(Vec3::ZERO, Vec3::Y, 0.0, mark_style(0.2, 2));

        assert_eq!(rig.effects.live(), 0);
        assert!(rig.frame(EYE).is_empty());
        assert!(rig.backend.registered.is_empty(), "nothing uploaded");
    }

    #[test]
    fn quality_zero_clears_a_live_flash_and_mark() {
        let mut rig = Rig::new();
        rig.effects.flash(Vec3::ZERO, 0.0, flash_style(0.4, 1));
        rig.effects
            .mark(Vec3::ZERO, Vec3::Y, 0.0, mark_style(0.2, 2));
        assert_eq!(rig.effects.live(), 2);

        rig.effects.set_quality(0);

        assert_eq!(rig.effects.live(), 0);
        assert!(rig.frame(EYE).is_empty());
    }

    #[test]
    fn the_same_spawns_draw_the_same_instances() {
        let run = || {
            let mut rig = Rig::new();
            rig.effects
                .flash(Vec3::new(1.0, 2.0, 3.0), 0.4, flash_style(0.4, 1));
            rig.effects.mark(
                Vec3::new(4.0, 5.0, 6.0),
                Vec3::new(0.0, 0.6, 0.8),
                1.1,
                mark_style(0.2, 2),
            );
            rig.effects.advance(ms(20));
            let mut drawn: Vec<([[f32; 4]; 4], u32)> = rig
                .frame(EYE)
                .iter()
                .map(|d| (d.model, d.material))
                .collect();
            drawn.sort_by_key(|(_, material)| *material);
            drawn
        };

        let first = run();
        let second = run();

        assert_eq!(first.len(), 2);
        assert_eq!(first, second);
    }

    fn assert_mark_basis(normal: Vec3, roll: f32) {
        let mut rig = Rig::new();
        let at = Vec3::new(1.0, -2.0, 3.0);
        rig.effects.mark(at, normal, roll, mark_style(0.5, 0));

        let drawn = rig.frame(EYE);

        assert_eq!(drawn.len(), 1, "normal {normal}");
        let d = &drawn[0];
        assert_finite(d);
        assert_near(translation(d), at + normal * MARK_OFFSET);
        assert!((x_axis(d).length() - 0.5).abs() < 1e-3, "normal {normal}");
        assert!((y_axis(d).length() - 0.5).abs() < 1e-3, "normal {normal}");
        assert!(x_axis(d).dot(normal).abs() < 1e-3, "normal {normal}");
        assert!(y_axis(d).dot(normal).abs() < 1e-3, "normal {normal}");
        assert!(x_axis(d).dot(y_axis(d)).abs() < 1e-3, "normal {normal}");
        assert!(
            x_axis(d).cross(y_axis(d)).dot(normal) > 0.0,
            "normal {normal}"
        );
        assert!((z_axis(d) - normal).length() < 1e-3, "normal {normal}");
    }

    #[test]
    fn a_mark_basis_is_finite_for_a_normal_along_each_axis() {
        for normal in [Vec3::X, Vec3::Y, Vec3::Z, -Vec3::X, -Vec3::Y, -Vec3::Z] {
            for roll in [0.0, 1.0, -2.5] {
                assert_mark_basis(normal, roll);
            }
        }
    }

    proptest! {
        #[test]
        fn a_mark_basis_is_finite_for_any_normal(
            yaw in 0.0f32..std::f32::consts::TAU,
            pitch in -1.57f32..1.57,
            roll in -7.0f32..7.0,
        ) {
            let normal = Vec3::new(
                pitch.cos() * yaw.cos(),
                pitch.sin(),
                pitch.cos() * yaw.sin(),
            );

            assert_mark_basis(normal, roll);
        }
    }

    #[test]
    fn a_flash_at_the_eye_draws_nothing_without_hiding_another_flash() {
        let mut rig = Rig::new();
        rig.effects.flash(EYE, 0.0, flash_style(0.4, 1));
        let elsewhere = Vec3::new(2.0, 3.0, 4.0);
        rig.effects.flash(elsewhere, 0.0, flash_style(0.4, 2));

        let drawn = rig.frame(EYE);

        assert_eq!(rig.effects.live(), 2, "the coincident flash stays live");
        assert_eq!(drawn.len(), 1);
        assert_eq!(drawn[0].material, 2);
        assert_near(translation(&drawn[0]), elsewhere);
    }

    #[test]
    fn a_lowered_cap_below_the_live_count_evicts_exactly_the_oldest_mark() {
        let mut rig = Rig::new();
        for i in 0..40 {
            rig.effects.mark(
                Vec3::new(i as f32, 0.0, 0.0),
                Vec3::Y,
                0.0,
                mark_style(0.2, 0),
            );
        }
        rig.effects.set_quality(1);

        rig.effects
            .mark(Vec3::new(100.0, 0.0, 0.0), Vec3::Y, 0.0, mark_style(0.2, 0));

        assert_eq!(rig.effects.live(), 40, "neither grows nor shrinks");
        let mut xs: Vec<f32> = rig.frame(EYE).iter().map(|d| translation(d).x).collect();
        xs.sort_by(f32::total_cmp);
        assert_eq!(xs.len(), 40);
        assert!(xs[0] > 0.5 && xs[0] < 1.5, "only x = 0 is gone, got {xs:?}");
        assert!((xs[39] - 100.0).abs() < EPS);
    }

    #[test]
    fn a_mark_with_a_huge_roll_keeps_an_orthogonal_sized_basis() {
        for normal in [Vec3::Y, Vec3::new(1.0, 2.0, 2.0).normalize()] {
            let mut rig = Rig::new();
            rig.effects
                .mark(Vec3::ZERO, normal, 1.0e6, mark_style(0.5, 0));

            let drawn = rig.frame(EYE);

            assert_eq!(drawn.len(), 1);
            let d = &drawn[0];
            assert!(
                (x_axis(d).length() - 0.5).abs() < 1e-3,
                "{}",
                x_axis(d).length()
            );
            assert!(
                (y_axis(d).length() - 0.5).abs() < 1e-3,
                "{}",
                y_axis(d).length()
            );
            assert!(x_axis(d).dot(normal).abs() < 1e-3);
            assert!(y_axis(d).dot(normal).abs() < 1e-3);
            assert!(x_axis(d).dot(y_axis(d)).abs() < 1e-3);
        }
    }

    #[test]
    fn a_tiny_but_non_zero_normal_is_accepted_and_faces_along_it() {
        let mut rig = Rig::new();
        rig.effects.mark(
            Vec3::ZERO,
            Vec3::new(1.0e-10, 0.0, 0.0),
            0.0,
            mark_style(0.5, 0),
        );

        let drawn = rig.frame(EYE);

        assert_eq!(drawn.len(), 1);
        assert_finite(&drawn[0]);
        assert_near(z_axis(&drawn[0]), Vec3::X);
        assert!((z_axis(&drawn[0]).length() - 1.0).abs() < 1e-3);
    }

    #[test]
    fn huge_finite_sizes_and_positions_never_draw_a_non_finite_instance() {
        let mut rig = Rig::new();
        rig.effects
            .flash(Vec3::new(1.0, 2.0, 3.0), 0.0, flash_style(1.0e30, 1));
        rig.effects.mark(
            Vec3::new(1.0, 2.0, 3.0),
            Vec3::Y,
            0.0,
            mark_style(1.0e30, 2),
        );
        rig.effects
            .mark(Vec3::splat(1.0e30), Vec3::Y, 0.0, mark_style(0.5, 3));
        rig.effects
            .flash(Vec3::splat(1.0e30), 0.0, flash_style(0.5, 4));

        let drawn = rig.frame(EYE);

        for d in &drawn {
            assert_finite(d);
        }
    }

    #[test]
    fn handles_keep_increasing_across_a_streak_that_faded_away_before_a_mark() {
        let mut rig = Rig::new();
        let first = rig.effects.begin_streak(style(2.0, 0.1));
        rig.effects.end(first);
        rig.effects.advance(Duration::from_secs(1));
        assert_eq!(rig.effects.live(), 0);

        rig.effects
            .mark(Vec3::ZERO, Vec3::Y, 0.0, mark_style(0.2, 6));
        let mark_handle = *rig.effects.store.keys().next().expect("the mark is live");
        let second = rig.effects.begin_streak(style(2.0, 0.1));

        assert!(first < mark_handle && mark_handle < second);
        rig.effects.advance(ms(1));
        assert_eq!(rig.effects.live(), 2);
        rig.effects.advance(Duration::from_secs(10));
        assert_eq!(rig.effects.live(), 1, "only the streak is left");
    }
}
