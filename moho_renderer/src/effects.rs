//! Engine-drawn effects with one store, cap and unshadowed draw. A streak is placed by
//! handle: a game starts it, places it every frame and ends it. A flash or mark is
//! fire-and-forget: spawned once, it lives out a fixed lifetime on its own.

use crate::WorldMeshes;
use glam::{Affine3A, Quat, Vec3};
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

/// A camera-facing star that shrinks linearly to nothing over its lifetime.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FlashStyle {
    /// The star's tip-to-tip width.
    pub size: f32,
    pub material: u32,
    pub lifetime: Duration,
}

/// A flat square on a surface that holds full size, then shrinks out over `fade`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MarkStyle {
    /// The square's side.
    pub size: f32,
    pub material: u32,
    pub lifetime: Duration,
    pub fade: Duration,
}

/// How far a mark is lifted off its surface along the normal.
const MARK_OFFSET: f32 = 0.01;

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

#[derive(Debug)]
struct Flash {
    at: Vec3,
    roll: f32,
    style: FlashStyle,
    age: Duration,
}

#[derive(Debug)]
struct Mark {
    centre: Vec3,
    /// Unit X, unit Y and unit normal (Z) of the mark's basis.
    axes: [Vec3; 3],
    style: MarkStyle,
    age: Duration,
}

impl Mark {
    /// Full size until `lifetime - fade` (fade clamped to the lifetime), then linear to 0.
    /// A zero fade stays full; the mark is removed at its lifetime before it can drop.
    fn size_scale(&self) -> f32 {
        let lifetime = self.style.lifetime;
        let fade = self.style.fade.min(lifetime);
        if fade.is_zero() {
            return 1.0;
        }
        (lifetime.saturating_sub(self.age).as_secs_f32() / fade.as_secs_f32()).min(1.0)
    }
}

#[derive(Debug)]
enum Effect {
    Streak(Streak),
    Flash(Flash),
    Mark(Mark),
}

/// Mesh slots: an effect kind's mesh id is `EFFECT_MESH_IDS_FROM + slot`.
const STREAK_SLOT: usize = 0;
const FLASH_SLOT: usize = 1;
const MARK_SLOT: usize = 2;
const MESH_BUILDERS: [fn() -> WorldMesh; 3] = [streak_mesh, flash_mesh, mark_mesh];

impl Effect {
    /// Whether a full cap may drop this effect to make room: fixed-lifetime ones only.
    fn evictable(&self) -> bool {
        !matches!(self, Self::Streak(_))
    }

    fn advance(&mut self, dt: Duration) {
        match self {
            Self::Streak(streak) => {
                streak.age = streak.age.saturating_add(dt);
                if streak.age >= streak.style.max_age {
                    streak.ended_at.get_or_insert(streak.style.max_age);
                }
            }
            Self::Flash(flash) => flash.age = flash.age.saturating_add(dt),
            Self::Mark(mark) => mark.age = mark.age.saturating_add(dt),
        }
    }

    fn finished(&self) -> bool {
        match self {
            Self::Streak(streak) => streak.finished_fading(),
            Self::Flash(flash) => flash.age >= flash.style.lifetime,
            Self::Mark(mark) => mark.age >= mark.style.lifetime,
        }
    }

    /// The mesh slot, transform and material to draw this frame, if any.
    fn drawable(&mut self, eye: Vec3) -> Option<(usize, Affine3A, u32)> {
        match self {
            Self::Streak(streak) => {
                let placed = std::mem::take(&mut streak.placed_this_frame);
                if !placed && streak.ended_at.is_none() {
                    return None;
                }
                let width = streak.style.width * streak.width_scale();
                let transform = streak_transform(
                    streak.head,
                    streak.heading?,
                    streak.style.length,
                    width,
                    eye,
                )?;
                Some((STREAK_SLOT, transform, streak.style.material))
            }
            Self::Flash(flash) => {
                let remaining = 1.0 - flash.age.as_secs_f32() / flash.style.lifetime.as_secs_f32();
                let scale = flash.style.size * remaining;
                if !eye.is_finite() || scale <= 0.0 {
                    return None;
                }
                // Standing on the eye has no direction to face; any fixed one stays finite.
                let toward_eye = (eye - flash.at).try_normalize().unwrap_or(Vec3::Z);
                let (x, y) = rolled_basis(toward_eye, flash.roll);
                let transform = scaled_basis(x, y, toward_eye, scale, flash.at);
                Some((FLASH_SLOT, transform, flash.style.material))
            }
            Self::Mark(mark) => {
                let scale = mark.style.size * mark.size_scale();
                if scale <= 0.0 {
                    return None;
                }
                let [x, y, z] = mark.axes;
                let transform = scaled_basis(x, y, z, scale, mark.centre);
                Some((MARK_SLOT, transform, mark.style.material))
            }
        }
    }
}

/// Whether a flash or mark with these values can ever draw: a non-zero lifetime and a
/// finite `at`, `roll` and positive `size`.
fn can_spawn(at: Vec3, roll: f32, size: f32, lifetime: Duration) -> bool {
    !lifetime.is_zero() && at.is_finite() && roll.is_finite() && size.is_finite() && size > 0.0
}

/// A transform with X and Y scaled by `scale`, unit `z` and the given `origin`.
fn scaled_basis(x: Vec3, y: Vec3, z: Vec3, scale: f32, origin: Vec3) -> Affine3A {
    Affine3A::from_cols(
        (x * scale).into(),
        (y * scale).into(),
        z.into(),
        origin.into(),
    )
}

/// `v` as a unit vector, or `None` if it is zero or non-finite. Scaling by the largest
/// component first keeps a long (or tiny) vector from overflowing (or underflowing).
fn unit_direction(v: Vec3) -> Option<Vec3> {
    if !v.is_finite() {
        return None;
    }
    (v / v.abs().max_element()).try_normalize()
}

/// X and Y across a unit `normal`, turned by `roll` about it, with X x Y along it.
fn rolled_basis(normal: Vec3, roll: f32) -> (Vec3, Vec3) {
    let x = Quat::from_axis_angle(normal, roll) * normal.any_orthonormal_vector();
    (x, normal.cross(x))
}

#[derive(Debug)]
pub struct Effects {
    store: BTreeMap<EffectHandle, Effect>,
    next_handle: u64,
    cap: usize,
    mesh_uploaded: [bool; MESH_BUILDERS.len()],
}

impl Default for Effects {
    fn default() -> Self {
        Self {
            store: BTreeMap::new(),
            next_handle: 0,
            cap: CAPS[DEFAULT_QUALITY],
            mesh_uploaded: [false; MESH_BUILDERS.len()],
        }
    }
}

impl Effects {
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

    fn issue_handle(&mut self) -> EffectHandle {
        let handle = EffectHandle(self.next_handle);
        self.next_handle += 1;
        handle
    }

    fn streak_mut(&mut self, handle: EffectHandle) -> Option<&mut Streak> {
        match self.store.get_mut(&handle) {
            Some(Effect::Streak(streak)) => Some(streak),
            _ => None,
        }
    }

    /// Store a fixed-lifetime effect. At the cap the oldest flash or mark makes room; with
    /// none to drop (only streaks live) the spawn is refused.
    fn spawn_fixed(&mut self, effect: Effect) {
        if self.store.len() >= self.cap {
            let oldest = self
                .store
                .iter()
                .find_map(|(handle, effect)| effect.evictable().then_some(*handle));
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
        let Some(streak) = self.streak_mut(handle) else {
            return;
        };
        streak.head = head;
        streak.heading = heading.try_normalize().or(streak.heading);
        streak.placed_this_frame = true;
    }

    /// Spawn a flash at `at`, rolled by `roll` radians about the view axis. It faces the
    /// eye and shrinks to nothing over its lifetime. One that could never draw (see
    /// `can_spawn`) is refused; at the cap it evicts the oldest flash or mark, else it is
    /// refused.
    pub fn flash(&mut self, at: Vec3, roll: f32, style: FlashStyle) {
        if !can_spawn(at, roll, style.size, style.lifetime) {
            return;
        }
        self.spawn_fixed(Effect::Flash(Flash {
            at,
            roll,
            style,
            age: Duration::ZERO,
        }));
    }

    /// Spawn a mark at `at` on a surface with `normal`, rolled by `roll` radians about it.
    /// A zero or non-finite `normal`, or one that could never draw (see `can_spawn`), is
    /// refused; at the cap it evicts the oldest flash or mark, else it is refused.
    pub fn mark(&mut self, at: Vec3, normal: Vec3, roll: f32, style: MarkStyle) {
        let Some(normal) = unit_direction(normal) else {
            return;
        };
        if !can_spawn(at, roll, style.size, style.lifetime) {
            return;
        }
        let (x, y) = rolled_basis(normal, roll);
        self.spawn_fixed(Effect::Mark(Mark {
            centre: at + normal * MARK_OFFSET,
            axes: [x, y, normal],
            style,
            age: Duration::ZERO,
        }));
    }

    /// Start the fade; the streak then draws at its last placement until it is gone.
    pub fn end(&mut self, handle: EffectHandle) {
        if let Some(streak) = self.streak_mut(handle) {
            streak.ended_at.get_or_insert(streak.age);
        }
    }

    /// Age every effect by the frame time, start the fade of any streak past its
    /// `max_age` and drop the finished ones.
    pub fn advance(&mut self, dt: Duration) {
        self.store.retain(|_, effect| {
            effect.advance(dt);
            !effect.finished()
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

    /// Place every drawable effect unshadowed: streaks placed this frame or fading, and
    /// every live flash and mark. Streaks and flashes face `eye`.
    pub(crate) fn draw(&mut self, meshes: &mut WorldMeshes, eye: Vec3) {
        for effect in self.store.values_mut() {
            let Some((slot, transform, material)) = effect.drawable(eye) else {
                continue;
            };
            let id = WorldMeshId(EFFECT_MESH_IDS_FROM + slot as u64);
            if !std::mem::replace(&mut self.mesh_uploaded[slot], true) {
                meshes.upsert_movable(id, MESH_BUILDERS[slot]());
            }
            meshes.place_unshadowed(id, transform, material);
        }
    }
}

/// A flat mesh facing +Z, every vertex normal and channel at its neutral value.
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

/// A four-point star one unit tip to tip (tips at 0.5 on the axes), counter-clockwise
/// seen from +Z.
fn flash_mesh() -> WorldMesh {
    flat_mesh(
        vec![
            [0.5, 0.0, 0.0],
            [0.125, 0.125, 0.0],
            [0.0, 0.5, 0.0],
            [-0.125, 0.125, 0.0],
            [-0.5, 0.0, 0.0],
            [-0.125, -0.125, 0.0],
            [0.0, -0.5, 0.0],
            [0.125, -0.125, 0.0],
        ],
        vec![7, 0, 1, 1, 2, 3, 3, 4, 5, 5, 6, 7, 1, 3, 5, 1, 5, 7],
    )
}

/// A unit square centred on the origin, counter-clockwise seen from +Z.
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

        /// One frame's draw as `(backend mesh handle, instance)`, in mesh-id order.
        fn frame_with_handles(&mut self, eye: Vec3) -> Vec<(u32, InstanceGpu)> {
            self.effects.draw(&mut self.meshes, eye);
            self.meshes.flush(&mut self.backend);
            let drawn = self.meshes.unshadowed_draws().collect();
            self.meshes.clear_placements();
            drawn
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

    fn z_axis(instance: &InstanceGpu) -> Vec3 {
        let [x, y, z, _] = instance.model[2];
        Vec3::new(x, y, z)
    }

    fn flash_style(size: f32, lifetime: Duration) -> FlashStyle {
        FlashStyle {
            size,
            material: 0,
            lifetime,
        }
    }

    fn mark_style(size: f32, lifetime: Duration, fade: Duration) -> MarkStyle {
        MarkStyle {
            size,
            material: 0,
            lifetime,
            fade,
        }
    }

    fn secs(n: f32) -> Duration {
        Duration::from_secs_f32(n)
    }

    /// Every triangle of the mesh is counter-clockwise seen from +Z, and the mesh lies in z = 0.
    fn assert_flat_and_wound_toward_plus_z(positions: &[[f32; 3]], indices: &[u32]) {
        assert!(!indices.is_empty());
        assert_eq!(indices.len() % 3, 0, "whole triangles");
        assert!(positions.iter().all(|p| p[2] == 0.0), "in the XY plane");
        for triangle in indices.chunks(3) {
            let [a, b, c] = [0, 1, 2].map(|i| Vec3::from(positions[triangle[i] as usize]));
            assert!(
                (b - a).cross(c - a).z > 0.0,
                "triangle {triangle:?} is counter-clockwise seen from +Z"
            );
        }
    }

    #[test]
    fn a_flash_shrinks_linearly_and_expires() {
        let mut rig = Rig::new();
        let at = Vec3::new(1.0, 2.0, 3.0);

        let style = FlashStyle {
            material: 9,
            ..flash_style(0.4, ms(50))
        };
        rig.effects.flash(at, 0.0, style);
        let fresh = rig.frame(EYE);

        assert_eq!(fresh.len(), 1);
        assert_eq!(fresh[0].material, 9);
        assert_near(translation(&fresh[0]), at);
        assert!((x_axis(&fresh[0]).length() - 0.4).abs() < EPS);
        assert!((y_axis(&fresh[0]).length() - 0.4).abs() < EPS);
        assert_eq!(rig.backend.registered.len(), 1);
        let (positions, indices) = &rig.backend.registered[0];
        assert_eq!(positions.len(), 8, "a four-point star");
        assert_flat_and_wound_toward_plus_z(positions, indices);
        for axis in 0..2 {
            let low = positions.iter().map(|p| p[axis]).fold(f32::MAX, f32::min);
            let high = positions.iter().map(|p| p[axis]).fold(f32::MIN, f32::max);
            assert_eq!((low, high), (-0.5, 0.5), "tip to tip is 1, axis {axis}");
        }

        rig.effects.advance(ms(25));
        let half = rig.frame(EYE);

        assert_eq!(half.len(), 1, "a flash draws every frame without a place");
        assert_near(translation(&half[0]), at);
        assert!((x_axis(&half[0]).length() - 0.2).abs() < EPS);
        assert!((y_axis(&half[0]).length() - 0.2).abs() < EPS);
        assert_eq!(rig.effects.live(), 1);

        rig.effects.advance(ms(25));

        assert!(rig.frame(EYE).is_empty());
        assert_eq!(rig.effects.live(), 0);
    }

    #[test]
    fn a_flash_faces_the_eye() {
        let at = Vec3::new(1.0, 2.0, 3.0);
        let size = 0.4;
        let eyes = [
            EYE,
            at + Vec3::Y * 7.0,
            at - Vec3::Y * 7.0,
            at + Vec3::X * 7.0,
        ];
        for eye in eyes {
            let mut rig = Rig::new();
            rig.effects.flash(at, 0.0, flash_style(size, ms(50)));
            rig.effects
                .flash(at, std::f32::consts::FRAC_PI_2, flash_style(size, ms(50)));

            let drawn = rig.frame(eye);

            assert_eq!(drawn.len(), 2, "eye {eye}");
            let toward_eye = (eye - at).normalize();
            for instance in &drawn {
                let (x, y, z) = (x_axis(instance), y_axis(instance), z_axis(instance));
                assert_near(z, toward_eye);
                assert!((x.length() - size).abs() < EPS, "eye {eye}");
                assert!((y.length() - size).abs() < EPS, "eye {eye}");
                assert!(x.dot(y).abs() < EPS, "X and Y are orthogonal");
                assert!(x.dot(toward_eye).abs() < EPS, "X is across the view");
                assert!(y.dot(toward_eye).abs() < EPS, "Y is across the view");
                assert!(x.cross(y).dot(toward_eye) > 0.0, "X x Y points at the eye");
            }
            let (unrolled, rolled) = (x_axis(&drawn[0]), x_axis(&drawn[1]));
            assert!(
                unrolled.dot(rolled).abs() < EPS,
                "a quarter turn of roll turns X a quarter turn, eye {eye}"
            );
            assert!(
                (unrolled.cross(rolled).dot(toward_eye).abs() - size * size).abs() < EPS,
                "and the turn is about the view axis, eye {eye}"
            );
        }

        let mut rig = Rig::new();
        rig.effects.flash(at, 0.0, flash_style(size, ms(50)));
        let first = rig.frame(EYE);
        let moved_eye = at + Vec3::Y * 7.0;
        let second = rig.frame(moved_eye);
        assert_near(z_axis(&first[0]), (EYE - at).normalize());
        assert_eq!(second.len(), 1);
        assert_near(z_axis(&second[0]), Vec3::Y);
    }

    #[test]
    fn a_mark_lies_along_its_normal_offset_by_the_mark_offset() {
        let mut rig = Rig::new();
        let at = Vec3::new(4.0, 5.0, 6.0);
        let normal = Vec3::new(1.0, 2.0, 2.0);
        let unit = normal.normalize();
        let style = MarkStyle {
            material: 5,
            ..mark_style(0.2, Duration::from_secs(10), Duration::from_secs(1))
        };

        rig.effects.mark(at, normal, 0.7, style);
        let drawn = rig.frame(EYE);
        let elsewhere = {
            rig.effects.advance(ms(1));
            rig.frame(Vec3::new(-30.0, 8.0, 1.0))
        };

        assert_eq!(drawn.len(), 1);
        let (x, y, z) = (x_axis(&drawn[0]), y_axis(&drawn[0]), z_axis(&drawn[0]));
        assert_near(z, unit);
        assert!(x.dot(unit).abs() < EPS, "X lies in the surface");
        assert!(y.dot(unit).abs() < EPS, "Y lies in the surface");
        assert!(x.dot(y).abs() < EPS);
        assert!((x.length() - 0.2).abs() < EPS, "0.2 across");
        assert!((y.length() - 0.2).abs() < EPS, "0.2 across");
        assert!(x.cross(y).dot(unit) > 0.0, "faces along the normal");
        assert_near(translation(&drawn[0]), at + unit * MARK_OFFSET);
        assert_eq!(drawn[0].material, 5);
        assert_eq!(elsewhere.len(), 1);
        assert_eq!(
            format!("{:?}", elsewhere[0].model),
            format!("{:?}", drawn[0].model),
            "a mark does not follow the eye"
        );
        assert_eq!(rig.backend.registered.len(), 1);
        let (positions, indices) = &rig.backend.registered[0];
        assert_eq!(positions.len(), 4, "a unit square");
        assert_flat_and_wound_toward_plus_z(positions, indices);
        for axis in 0..2 {
            let low = positions.iter().map(|p| p[axis]).fold(f32::MAX, f32::min);
            let high = positions.iter().map(|p| p[axis]).fold(f32::MIN, f32::max);
            assert_eq!((low, high), (-0.5, 0.5), "side 1, centred, axis {axis}");
        }

        let mut rolled_rig = Rig::new();
        rolled_rig
            .effects
            .mark(at, normal, 0.7 + std::f32::consts::FRAC_PI_2, style);
        let rolled = rolled_rig.frame(EYE);
        assert!(
            x_axis(&rolled[0]).dot(x).abs() < EPS,
            "a quarter turn of roll turns X a quarter turn"
        );
    }

    #[test]
    fn a_mark_holds_full_size_then_shrinks_out_in_its_fade() {
        let mut rig = Rig::new();
        let at = Vec3::new(2.0, 0.0, 2.0);
        rig.effects.mark(
            at,
            Vec3::Y,
            0.0,
            mark_style(0.2, Duration::from_secs(10), Duration::from_secs(1)),
        );

        rig.effects.advance(Duration::from_secs(9));
        let full = rig.frame(EYE);

        assert_eq!(full.len(), 1);
        assert!((x_axis(&full[0]).length() - 0.2).abs() < EPS, "full size");
        assert!((y_axis(&full[0]).length() - 0.2).abs() < EPS, "full size");

        rig.effects.advance(secs(0.5));
        let half = rig.frame(EYE);

        assert_eq!(half.len(), 1);
        assert!((x_axis(&half[0]).length() - 0.1).abs() < EPS, "half size");
        assert!((y_axis(&half[0]).length() - 0.1).abs() < EPS, "half size");
        assert_near(translation(&half[0]), at + Vec3::Y * MARK_OFFSET);
        assert_eq!(rig.effects.live(), 1);

        rig.effects.advance(secs(0.5));

        assert!(rig.frame(EYE).is_empty());
        assert_eq!(rig.effects.live(), 0);
    }

    #[test]
    fn a_full_cap_drops_the_oldest_fixed_lifetime_effect() {
        let mut rig = Rig::new();
        rig.effects.set_quality(1);
        let style = mark_style(0.2, Duration::from_secs(10), Duration::from_secs(1));
        for i in 0..32 {
            rig.effects
                .mark(Vec3::new(i as f32, 0.0, 0.0), Vec3::Y, 0.0, style);
        }
        assert_eq!(rig.effects.live(), 32);

        rig.effects
            .mark(Vec3::new(32.0, 0.0, 0.0), Vec3::Y, 0.0, style);
        let drawn = rig.frame(EYE);

        assert_eq!(rig.effects.live(), 32);
        let mut xs: Vec<i32> = drawn
            .iter()
            .map(|d| translation(d).x.round() as i32)
            .collect();
        xs.sort_unstable();
        assert_eq!(
            xs,
            (1..=32).collect::<Vec<i32>>(),
            "the first mark is dropped"
        );

        rig.effects
            .flash(Vec3::new(100.0, 0.0, 0.0), 0.0, flash_style(0.4, ms(50)));
        let drawn = rig.frame(EYE);

        assert_eq!(rig.effects.live(), 32, "a flash also drops the oldest mark");
        let mut xs: Vec<i32> = drawn
            .iter()
            .map(|d| translation(d).x.round() as i32)
            .collect();
        xs.sort_unstable();
        let mut expected: Vec<i32> = (2..=32).collect();
        expected.push(100);
        assert_eq!(xs, expected);
    }

    #[test]
    fn a_full_cap_of_streaks_refuses_a_mark_and_keeps_every_streak() {
        let mut rig = Rig::new();
        rig.effects.set_quality(1);
        let handles: Vec<EffectHandle> = (0..32)
            .map(|_| rig.effects.begin_streak(style(2.0, 0.1)))
            .collect();

        rig.effects.mark(
            Vec3::new(500.0, 0.0, 0.0),
            Vec3::Y,
            0.0,
            mark_style(0.2, Duration::from_secs(10), Duration::from_secs(1)),
        );
        rig.effects
            .flash(Vec3::new(500.0, 0.0, 0.0), 0.0, flash_style(0.4, ms(50)));
        for (i, handle) in handles.iter().enumerate() {
            rig.effects
                .place_streak(*handle, Vec3::new(10.0 + i as f32, 0.0, 0.0), Vec3::X);
        }
        let drawn = rig.frame(EYE);

        assert_eq!(rig.effects.live(), 32);
        assert_eq!(drawn.len(), 32, "every streak stays and nothing else draws");
        assert!(drawn.iter().all(|d| translation(d).x < 100.0));

        rig.effects.set_quality(2);
        rig.effects.mark(
            Vec3::new(500.0, 0.0, 0.0),
            Vec3::Y,
            0.0,
            mark_style(0.2, Duration::from_secs(10), Duration::from_secs(1)),
        );
        let drawn = rig.frame(EYE);

        assert_eq!(rig.effects.live(), 33, "with room, the mark is accepted");
        assert!(
            drawn.iter().any(|d| (translation(d).x - 500.0).abs() < EPS),
            "and it draws"
        );
    }

    #[test]
    fn quality_zero_refuses_flash_and_mark() {
        let mut rig = Rig::new();
        let long = mark_style(0.2, Duration::from_secs(10), Duration::from_secs(1));
        rig.effects.set_quality(1);
        rig.effects.flash(Vec3::X, 0.0, flash_style(0.4, ms(50)));
        rig.effects.mark(Vec3::Z, Vec3::Y, 0.0, long);
        assert_eq!(rig.effects.live(), 2, "quality 1 accepts both");

        rig.effects.set_quality(0);

        assert_eq!(rig.effects.live(), 0);
        assert!(rig.frame(EYE).is_empty());

        rig.effects.flash(Vec3::X, 0.0, flash_style(0.4, ms(50)));
        rig.effects.mark(Vec3::Z, Vec3::Y, 0.0, long);

        assert_eq!(rig.effects.live(), 0);
        assert!(rig.frame(EYE).is_empty());
    }

    #[test]
    fn the_same_spawns_draw_the_same_instances() {
        let run = || {
            let mut rig = Rig::new();
            rig.effects
                .flash(Vec3::new(1.0, 2.0, 3.0), 0.3, flash_style(0.4, ms(50)));
            rig.effects.mark(
                Vec3::new(4.0, 5.0, 6.0),
                Vec3::new(0.0, 1.0, 1.0),
                1.1,
                mark_style(0.2, Duration::from_secs(10), Duration::from_secs(1)),
            );
            let mut frames = vec![format!("{:?}", rig.frame(EYE))];
            for step in [ms(20), ms(20), Duration::from_secs(9)] {
                rig.effects.advance(step);
                frames.push(format!("{:?}", rig.frame(EYE)));
            }
            frames
        };

        let (first, second) = (run(), run());

        assert_eq!(first, second);
        assert_ne!(first[0], first[3], "the effects did change over time");

        let mut rig = Rig::new();
        let at = Vec3::new(40.0, 5.0, 6.0);
        let normal = Vec3::new(0.0, 1.0, 1.0);
        let twin = mark_style(0.2, Duration::from_secs(10), Duration::from_secs(1));
        rig.effects.mark(at, normal, 1.1, twin);
        for i in 0..5 {
            let other = Vec3::new(i as f32, 0.0, 0.0);
            rig.effects.flash(other, 0.2, flash_style(0.3, ms(50)));
            rig.effects.mark(other + Vec3::Z, Vec3::X, 0.4, twin);
        }
        rig.effects.mark(at, normal, 1.1, twin);
        let drawn = rig.frame(EYE);

        let twins: Vec<&InstanceGpu> = drawn
            .iter()
            .filter(|d| translation(d).y > 5.0 && translation(d).x > 30.0)
            .collect();
        assert_eq!(twins.len(), 2, "both twins drew");
        assert_eq!(
            format!("{:?}", twins[0].model),
            format!("{:?}", twins[1].model),
            "the same mark draws the same whatever was spawned before it"
        );
    }

    fn assert_mark_basis_is_finite(normal: Vec3, roll: f32) {
        let mut rig = Rig::new();
        rig.effects.mark(
            Vec3::ZERO,
            normal,
            roll,
            mark_style(0.5, Duration::from_secs(10), Duration::from_secs(1)),
        );

        let drawn = rig.frame(EYE);

        assert_eq!(drawn.len(), 1, "normal {normal}");
        assert!(
            drawn[0].model.iter().flatten().all(|v| v.is_finite()),
            "normal {normal}, roll {roll}: {:?}",
            drawn[0].model
        );
        let unit = normal.normalize();
        let (x, y, z) = (x_axis(&drawn[0]), y_axis(&drawn[0]), z_axis(&drawn[0]));
        assert!(
            (z - unit).length() < 1e-3,
            "Z is the unit normal, normal {normal}"
        );
        assert!(x.dot(unit).abs() < 1e-3 && y.dot(unit).abs() < 1e-3);
        assert!((x.length() - 0.5).abs() < 1e-3 && (y.length() - 0.5).abs() < 1e-3);
        assert!(
            x.dot(y).abs() < 1e-3,
            "X and Y are orthogonal, normal {normal}"
        );
        assert!(
            x.cross(y).dot(unit) > 0.0,
            "faces along the normal, normal {normal}"
        );
    }

    #[test]
    fn a_mark_basis_is_finite_for_any_normal() {
        for normal in [Vec3::X, Vec3::Y, Vec3::Z, -Vec3::X, -Vec3::Y, -Vec3::Z] {
            assert_mark_basis_is_finite(normal, 0.0);
            assert_mark_basis_is_finite(normal, 2.5);
        }

        proptest!(|(
            n in (-10.0f32..10.0, -10.0f32..10.0, -10.0f32..10.0),
            roll in -7.0f32..7.0,
        )| {
            let normal = Vec3::new(n.0, n.1, n.2);
            prop_assume!(normal.length() > 1e-2);
            assert_mark_basis_is_finite(normal, roll);
        });
    }

    fn drawn_xs(drawn: &[InstanceGpu]) -> Vec<i32> {
        let mut xs: Vec<i32> = drawn
            .iter()
            .map(|d| translation(d).x.round() as i32)
            .collect();
        xs.sort_unstable();
        xs
    }

    fn long_mark() -> MarkStyle {
        mark_style(0.2, Duration::from_secs(10), Duration::from_secs(1))
    }

    /// A rig at quality 1 holding 32 marks at x = 0..=31.
    fn rig_full_of_marks() -> Rig {
        let mut rig = Rig::new();
        rig.effects.set_quality(1);
        for i in 0..32 {
            rig.effects
                .mark(Vec3::new(i as f32, 0.0, 0.0), Vec3::Y, 0.0, long_mark());
        }
        assert_eq!(rig.effects.live(), 32);
        rig
    }

    #[test]
    fn each_effect_kind_draws_its_own_reserved_mesh() {
        let mut rig = Rig::new();
        let streak = rig.effects.begin_streak(StreakStyle::new(2.0, 0.1, 1));
        rig.effects
            .place_streak(streak, Vec3::new(10.0, 0.0, 0.0), Vec3::X);
        let flash = FlashStyle {
            material: 2,
            ..flash_style(0.4, ms(50))
        };
        rig.effects.flash(Vec3::new(0.0, 5.0, 0.0), 0.0, flash);
        let mark = MarkStyle {
            material: 3,
            ..long_mark()
        };
        rig.effects
            .mark(Vec3::new(0.0, 0.0, 5.0), Vec3::Y, 0.0, mark);

        let drawn = rig.frame_with_handles(EYE);

        assert_eq!(rig.backend.registered.len(), 3, "one mesh per kind");
        assert_eq!(drawn.len(), 3);
        let materials: Vec<u32> = drawn.iter().map(|(_, i)| i.material).collect();
        assert_eq!(
            materials,
            vec![1, 2, 3],
            "streak, flash, mark in reserved id order"
        );
        let handles: Vec<u32> = drawn.iter().map(|(h, _)| *h).collect();
        assert_eq!(handles, vec![1, 2, 3], "three distinct meshes");
        let (kite, _) = &rig.backend.registered[(handles[0] - 1) as usize];
        assert_eq!(
            kite,
            &vec![
                [0.0, 0.0, 0.0],
                [0.8, -0.5, 0.0],
                [1.0, 0.0, 0.0],
                [0.8, 0.5, 0.0]
            ],
            "the streak draws the kite"
        );
        let (star, _) = &rig.backend.registered[(handles[1] - 1) as usize];
        assert_eq!(star.len(), 8, "the flash draws the star");
        let (quad, _) = &rig.backend.registered[(handles[2] - 1) as usize];
        assert_eq!(quad.len(), 4);
        assert!(
            quad.iter().all(|p| p[0].abs() == 0.5 && p[1].abs() == 0.5),
            "the mark draws the unit square"
        );

        rig.effects
            .place_streak(streak, Vec3::new(11.0, 0.0, 0.0), Vec3::X);
        let again = rig.frame_with_handles(EYE);

        assert_eq!(again.len(), 3);
        assert_eq!(rig.backend.registered.len(), 3, "nothing new is registered");
    }

    #[test]
    fn eviction_skips_an_older_streak_for_the_oldest_mark() {
        let mut rig = Rig::new();
        rig.effects.set_quality(1);
        let streak = rig.effects.begin_streak(style(2.0, 0.1));
        for i in 1..32 {
            rig.effects
                .mark(Vec3::new(i as f32, 0.0, 0.0), Vec3::Y, 0.0, long_mark());
        }
        assert_eq!(rig.effects.live(), 32);

        rig.effects
            .mark(Vec3::new(100.0, 0.0, 0.0), Vec3::Y, 0.0, long_mark());
        rig.effects
            .place_streak(streak, Vec3::new(500.0, 0.0, 0.0), Vec3::X);
        let drawn = rig.frame(EYE);

        assert_eq!(rig.effects.live(), 32);
        let mut expected: Vec<i32> = (2..32).collect();
        expected.extend([100, 498]);
        assert_eq!(drawn_xs(&drawn), expected, "mark 1 dropped, streak kept");
    }

    #[test]
    fn a_streak_begun_at_a_full_cap_of_marks_is_inert_and_evicts_nothing() {
        let mut rig = rig_full_of_marks();

        let handle = rig.effects.begin_streak(style(2.0, 0.1));
        rig.effects
            .place_streak(handle, Vec3::new(700.0, 0.0, 0.0), Vec3::X);
        rig.effects.end(handle);
        let drawn = rig.frame(EYE);

        assert_eq!(rig.effects.live(), 32);
        assert_eq!(drawn_xs(&drawn), (0..32).collect::<Vec<i32>>());
    }

    #[test]
    fn a_zero_fade_mark_holds_full_size_until_its_lifetime() {
        let at = Vec3::new(3.0, 0.0, 0.0);

        let mut abrupt = Rig::new();
        abrupt
            .effects
            .mark(at, Vec3::Y, 0.0, mark_style(0.2, secs(2.0), Duration::ZERO));
        abrupt.effects.advance(ms(1900));
        let drawn = abrupt.frame(EYE);
        assert_eq!(drawn.len(), 1);
        assert!(
            (x_axis(&drawn[0]).length() - 0.2).abs() < EPS,
            "full at 1.9 s"
        );
        abrupt.effects.advance(ms(100));
        assert!(abrupt.frame(EYE).is_empty(), "gone at 2 s");
        assert_eq!(abrupt.effects.live(), 0);
    }

    #[test]
    fn a_fade_longer_than_the_lifetime_clamps_to_it() {
        let at = Vec3::new(3.0, 0.0, 0.0);

        let mut clamped = Rig::new();
        clamped
            .effects
            .mark(at, Vec3::Y, 0.0, mark_style(0.2, secs(2.0), secs(5.0)));
        clamped.effects.advance(ms(1000));
        let drawn = clamped.frame(EYE);
        assert_eq!(drawn.len(), 1);
        assert!(drawn[0].model.iter().flatten().all(|v| v.is_finite()));
        assert!(
            (x_axis(&drawn[0]).length() - 0.1).abs() < EPS,
            "half at 1 s"
        );
        clamped.effects.advance(ms(1000));
        assert!(clamped.frame(EYE).is_empty(), "gone at 2 s");
    }

    #[test]
    fn a_zero_lifetime_flash_or_mark_is_refused_and_evicts_nothing() {
        let at = Vec3::new(3.0, 0.0, 0.0);

        let mut rig = rig_full_of_marks();
        rig.effects.flash(at, 0.0, flash_style(0.4, Duration::ZERO));
        rig.effects.mark(
            at,
            Vec3::Y,
            0.0,
            mark_style(0.2, Duration::ZERO, Duration::ZERO),
        );
        let drawn = rig.frame(EYE);
        assert_eq!(rig.effects.live(), 32, "refused, and nothing evicted");
        assert_eq!(drawn_xs(&drawn), (0..32).collect::<Vec<i32>>());

        let mut empty = Rig::new();
        empty
            .effects
            .flash(at, 0.0, flash_style(0.4, Duration::ZERO));
        empty.effects.mark(
            at,
            Vec3::Y,
            0.0,
            mark_style(0.2, Duration::ZERO, Duration::ZERO),
        );
        assert_eq!(empty.effects.live(), 0);
        assert!(empty.frame(EYE).is_empty());
    }

    #[test]
    fn degenerate_geometry_is_refused_or_finite() {
        let mut rig = rig_full_of_marks();
        let at = Vec3::new(3.0, 0.0, 0.0);

        rig.effects.mark(at, Vec3::ZERO, 0.0, long_mark());
        rig.effects
            .mark(at, Vec3::new(f32::NAN, 1.0, 0.0), 0.0, long_mark());
        rig.effects
            .mark(at, Vec3::new(f32::INFINITY, 0.0, 0.0), 0.0, long_mark());
        rig.effects
            .mark(Vec3::new(f32::NAN, 0.0, 0.0), Vec3::Y, 0.0, long_mark());
        rig.effects.mark(
            Vec3::new(0.0, f32::INFINITY, 0.0),
            Vec3::Y,
            0.0,
            long_mark(),
        );
        rig.effects
            .flash(Vec3::new(f32::NAN, 0.0, 0.0), 0.0, flash_style(0.4, ms(50)));
        let drawn = rig.frame(EYE);

        assert_eq!(rig.effects.live(), 32, "refused, and nothing evicted");
        assert_eq!(drawn_xs(&drawn), (0..32).collect::<Vec<i32>>());

        let mut at_eye = Rig::new();
        at_eye.effects.flash(EYE, 0.0, flash_style(0.4, ms(50)));
        let drawn = at_eye.frame(EYE);
        assert_eq!(drawn.len(), 1);
        assert!(drawn[0].model.iter().flatten().all(|v| v.is_finite()));
        assert!((x_axis(&drawn[0]).length() - 0.4).abs() < EPS);
        assert!((y_axis(&drawn[0]).length() - 0.4).abs() < EPS);
    }

    #[test]
    fn a_non_finite_roll_is_refused_and_evicts_nothing() {
        let mut rig = rig_full_of_marks();
        let at = Vec3::new(500.0, 0.0, 0.0);

        for roll in [f32::NAN, f32::INFINITY] {
            rig.effects.flash(at, roll, flash_style(0.4, ms(50)));
        }
        for roll in [f32::NAN, f32::NEG_INFINITY] {
            rig.effects.mark(at, Vec3::Y, roll, long_mark());
        }
        let drawn = rig.frame(EYE);

        assert_eq!(rig.effects.live(), 32);
        assert_eq!(drawn_xs(&drawn), (0..32).collect::<Vec<i32>>());
        assert!(
            drawn
                .iter()
                .all(|d| d.model.iter().flatten().all(|v| v.is_finite()))
        );
    }

    #[test]
    fn a_size_that_cannot_draw_is_refused_and_evicts_nothing() {
        let mut rig = rig_full_of_marks();
        let at = Vec3::new(500.0, 0.0, 0.0);

        for size in [0.0, -0.3, f32::NAN, f32::INFINITY] {
            rig.effects.flash(at, 0.0, flash_style(size, ms(50)));
            rig.effects.mark(
                at,
                Vec3::Y,
                0.0,
                mark_style(size, Duration::from_secs(10), Duration::from_secs(1)),
            );
        }
        let drawn = rig.frame(EYE);

        assert_eq!(rig.effects.live(), 32);
        assert_eq!(drawn_xs(&drawn), (0..32).collect::<Vec<i32>>());
    }

    #[test]
    fn spawning_over_a_lowered_cap_replaces_the_oldest_and_grows_nothing() {
        let mut rig = Rig::new();
        for i in 0..40 {
            rig.effects
                .mark(Vec3::new(i as f32, 0.0, 0.0), Vec3::Y, 0.0, long_mark());
        }
        rig.effects.set_quality(1);

        rig.effects
            .mark(Vec3::new(100.0, 0.0, 0.0), Vec3::Y, 0.0, long_mark());
        let drawn = rig.frame(EYE);

        assert_eq!(rig.effects.live(), 40);
        let mut expected: Vec<i32> = (1..40).collect();
        expected.push(100);
        assert_eq!(drawn_xs(&drawn), expected);
    }

    #[test]
    fn a_long_normal_is_still_a_direction() {
        for (normal, expected) in [
            (Vec3::new(1e30, 0.0, 0.0), Vec3::X),
            (Vec3::new(0.0, -3e38, 0.0), -Vec3::Y),
        ] {
            let mut rig = Rig::new();

            rig.effects.mark(Vec3::ZERO, normal, 0.0, long_mark());
            let drawn = rig.frame(EYE);

            assert_eq!(drawn.len(), 1, "normal {normal}");
            assert!(drawn[0].model.iter().flatten().all(|v| v.is_finite()));
            assert_near(z_axis(&drawn[0]), expected);
        }
    }

    #[test]
    fn a_flash_seen_from_a_non_finite_eye_draws_nothing() {
        let mut rig = Rig::new();
        rig.effects
            .flash(Vec3::new(1.0, 2.0, 3.0), 0.0, flash_style(0.4, ms(50)));

        for eye in [Vec3::splat(f32::NAN), Vec3::new(f32::INFINITY, 0.0, 0.0)] {
            assert!(rig.frame(eye).is_empty(), "eye {eye}");
        }
        let drawn = rig.frame(EYE);

        assert_eq!(rig.effects.live(), 1, "still live");
        assert_eq!(drawn.len(), 1, "and it draws once the eye is sane");
        assert!(drawn[0].model.iter().flatten().all(|v| v.is_finite()));
    }

    #[test]
    fn a_huge_advance_expires_everything_without_overflow() {
        let mut rig = Rig::new();
        rig.effects
            .flash(Vec3::new(1.0, 0.0, 0.0), 0.0, flash_style(0.4, ms(50)));
        rig.effects
            .mark(Vec3::new(2.0, 0.0, 0.0), Vec3::Y, 0.0, long_mark());
        let streak = rig.effects.begin_streak(style(2.0, 0.1));
        rig.effects
            .place_streak(streak, Vec3::new(10.0, 0.0, 0.0), Vec3::X);
        rig.effects.advance(ms(10));
        assert_eq!(rig.effects.live(), 3);

        rig.effects.advance(Duration::MAX);

        assert_eq!(rig.effects.live(), 0);
        assert!(rig.frame(EYE).is_empty());
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
}
