//! Engine-drawn effects: streaks are placed by handle (a game starts one, places it every
//! frame and ends it), flashes and marks are fire-and-forget; the engine owns life, fade,
//! cap and unshadowed draw.

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
    pub size: f32,
    pub material: u32,
    pub lifetime: Duration,
}

/// A flat square on a surface: full size until `lifetime - fade`, then shrinking out.
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

#[derive(Debug)]
struct Flash {
    at: Vec3,
    roll: f32,
    style: FlashStyle,
    age: Duration,
}

#[derive(Debug)]
struct Mark {
    /// Already lifted off the surface by `MARK_OFFSET`.
    at: Vec3,
    /// Unit surface normal.
    normal: Vec3,
    roll: f32,
    style: MarkStyle,
    age: Duration,
}

impl Mark {
    /// The size factor: 1 until `lifetime - fade`, then falling linearly to 0.
    fn scale(&self) -> f32 {
        let fade_start = self.style.lifetime.saturating_sub(self.style.fade);
        if self.age <= fade_start {
            return 1.0;
        }
        let span = self.style.lifetime.min(self.style.fade);
        remaining(self.style.lifetime.saturating_sub(self.age), span)
    }
}

/// `left / total` clamped to 0..=1, and 0 when `total` is zero.
fn remaining(left: Duration, total: Duration) -> f32 {
    if total.is_zero() {
        return 0.0;
    }
    (left.as_secs_f32() / total.as_secs_f32()).clamp(0.0, 1.0)
}

#[derive(Debug)]
enum Effect {
    Streak(Streak),
    Flash(Flash),
    Mark(Mark),
}

impl Effect {
    fn advance(&mut self, dt: Duration) {
        match self {
            Self::Streak(streak) => {
                streak.age += dt;
                if streak.age >= streak.style.max_age {
                    streak.ended_at.get_or_insert(streak.style.max_age);
                }
            }
            Self::Flash(flash) => flash.age += dt,
            Self::Mark(mark) => mark.age += dt,
        }
    }

    fn finished(&self) -> bool {
        match self {
            Self::Streak(streak) => streak.finished_fading(),
            Self::Flash(flash) => flash.age >= flash.style.lifetime,
            Self::Mark(mark) => mark.age >= mark.style.lifetime,
        }
    }

    /// Streaks live until their game ends them; flashes and marks have a fixed lifetime
    /// and may be evicted for a newer one.
    fn is_fixed_lifetime(&self) -> bool {
        !matches!(self, Self::Streak(_))
    }
}

/// Which built-in mesh an effect draws; the discriminant is its offset from
/// `EFFECT_MESH_IDS_FROM`.
#[derive(Debug, Clone, Copy)]
enum MeshKind {
    Streak,
    Flash,
    Mark,
}

impl MeshKind {
    const COUNT: usize = 3;

    fn id(self) -> WorldMeshId {
        WorldMeshId(EFFECT_MESH_IDS_FROM + self as u64)
    }

    fn build(self) -> WorldMesh {
        match self {
            Self::Streak => streak_mesh(),
            Self::Flash => star_mesh(),
            Self::Mark => quad_mesh(),
        }
    }
}

#[derive(Debug)]
pub struct Effects {
    store: BTreeMap<EffectHandle, Effect>,
    next_handle: u64,
    cap: usize,
    mesh_uploaded: [bool; MeshKind::COUNT],
}

impl Default for Effects {
    fn default() -> Self {
        Self {
            store: BTreeMap::new(),
            next_handle: 0,
            cap: CAPS[DEFAULT_QUALITY],
            mesh_uploaded: [false; MeshKind::COUNT],
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

    /// Store a fixed-lifetime effect, first dropping the oldest one if the cap is reached.
    /// With nothing evictable (only streaks, or a cap of 0) the effect is refused.
    fn spawn_fixed(&mut self, effect: Effect) {
        if self.store.len() >= self.cap {
            let oldest = self
                .store
                .iter()
                .find(|(_, live)| live.is_fixed_lifetime())
                .map(|(handle, _)| *handle);
            let Some(oldest) = oldest else {
                return;
            };
            self.store.remove(&oldest);
        }
        let handle = self.issue_handle();
        self.store.insert(handle, effect);
    }

    /// Spawn a flash at `at`, rolled by `roll` radians about the view axis. At the cap the
    /// oldest flash or mark is dropped; with none to drop the flash is refused.
    pub fn flash(&mut self, at: Vec3, roll: f32, style: FlashStyle) {
        self.spawn_fixed(Effect::Flash(Flash {
            at,
            roll,
            style,
            age: Duration::ZERO,
        }));
    }

    /// Spawn a mark at `at` lying on a surface with `normal`, rolled by `roll` radians.
    /// A zero or non-finite normal refuses the mark; at the cap it evicts like a flash.
    pub fn mark(&mut self, at: Vec3, normal: Vec3, roll: f32, style: MarkStyle) {
        let Some(normal) = normal.try_normalize() else {
            return;
        };
        self.spawn_fixed(Effect::Mark(Mark {
            at: at + normal * MARK_OFFSET,
            normal,
            roll,
            style,
            age: Duration::ZERO,
        }));
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
            effect.advance(dt);
        }
        self.store.retain(|_, effect| !effect.finished());
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

    /// Place every streak placed this frame or fading, and every live flash and mark,
    /// unshadowed, facing `eye`.
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
                Effect::Flash(flash) => {
                    let size = flash.style.size
                        * remaining(
                            flash.style.lifetime.saturating_sub(flash.age),
                            flash.style.lifetime,
                        );
                    let transform = flash_transform(flash.at, flash.roll, size, eye);
                    (MeshKind::Flash, transform, flash.style.material)
                }
                Effect::Mark(mark) => {
                    let transform = facing_transform(
                        mark.at,
                        mark.normal,
                        mark.roll,
                        mark.style.size * mark.scale(),
                    );
                    (MeshKind::Mark, transform, mark.style.material)
                }
            };
            let Some(transform) = transform else {
                continue;
            };
            if !std::mem::replace(&mut self.mesh_uploaded[kind as usize], true) {
                meshes.upsert_movable(kind.id(), kind.build());
            }
            meshes.place_unshadowed(kind.id(), transform, material);
        }
    }
}

/// A mesh lying in z = 0 facing +Z, with unit normals, AO and light.
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
    .expect("every channel has one entry per position and the indices are in range")
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
/// seen from +Z. Even vertices are tips, odd ones the inner corners between them.
fn star_mesh() -> WorldMesh {
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

/// A unit square centred on the origin, counter-clockwise seen from +Z.
fn quad_mesh() -> WorldMesh {
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

/// A square of side `size` centred on `at` with unit normal `z`, turned by `roll` about it.
fn facing_transform(at: Vec3, z: Vec3, roll: f32, size: f32) -> Option<Affine3A> {
    let finite = at.is_finite() && z.is_finite() && roll.is_finite() && size.is_finite();
    if !finite || size <= 0.0 {
        return None;
    }
    let x = Quat::from_axis_angle(z, roll) * z.any_orthonormal_vector();
    let y = z.cross(x);
    Some(Affine3A::from_cols(
        (x * size).into(),
        (y * size).into(),
        z.into(),
        at.into(),
    ))
}

/// Where a flash of tip-to-tip `size` at `at` is drawn so that it faces `eye`.
fn flash_transform(at: Vec3, roll: f32, size: f32, eye: Vec3) -> Option<Affine3A> {
    if !eye.is_finite() {
        return None;
    }
    let toward_eye = (eye - at).try_normalize().unwrap_or(Vec3::Z);
    facing_transform(at, toward_eye, roll, size)
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
            self.frame_with_handles(eye)
                .into_iter()
                .map(|(_, instance)| instance)
                .collect()
        }

        /// One frame's draw as `(backend mesh handle, instance)`.
        fn frame_with_handles(&mut self, eye: Vec3) -> Vec<(u32, InstanceGpu)> {
            self.effects.draw(&mut self.meshes, eye);
            self.meshes.flush(&mut self.backend);
            let drawn = self.meshes.unshadowed_draws().collect();
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

    const FLASH_MATERIAL: u32 = 11;
    const MARK_MATERIAL: u32 = 22;

    fn flash_style(size: f32, lifetime: Duration) -> FlashStyle {
        FlashStyle {
            size,
            material: FLASH_MATERIAL,
            lifetime,
        }
    }

    fn mark_style(size: f32, lifetime: Duration, fade: Duration) -> MarkStyle {
        MarkStyle {
            size,
            material: MARK_MATERIAL,
            lifetime,
            fade,
        }
    }

    fn all_finite(drawn: &[InstanceGpu]) -> bool {
        drawn
            .iter()
            .all(|i| i.model.iter().flatten().all(|v| v.is_finite()))
    }

    fn secs(n: f32) -> Duration {
        Duration::from_secs_f32(n)
    }

    #[test]
    fn a_flash_shrinks_linearly_and_expires() {
        let mut rig = Rig::new();
        let at = Vec3::new(2.0, 3.0, 4.0);
        rig.effects.flash(at, 0.0, flash_style(0.4, ms(50)));

        let full = rig.frame(EYE);
        assert_eq!(full.len(), 1);
        assert_near(translation(&full[0]), at);
        assert_eq!(full[0].material, FLASH_MATERIAL);
        assert!((x_axis(&full[0]).length() - 0.4).abs() < EPS);
        assert!((y_axis(&full[0]).length() - 0.4).abs() < EPS);

        rig.effects.advance(ms(25));
        let half = rig.frame(EYE);
        assert_eq!(half.len(), 1);
        assert_near(translation(&half[0]), at);
        assert_eq!(half[0].material, FLASH_MATERIAL);
        assert!((x_axis(&half[0]).length() - 0.2).abs() < EPS);
        assert!((y_axis(&half[0]).length() - 0.2).abs() < EPS);
        assert_eq!(rig.effects.live(), 1);

        rig.effects.advance(ms(25));
        assert_eq!(rig.effects.live(), 0);
        assert!(rig.frame(EYE).is_empty());
    }

    #[test]
    fn a_flash_faces_the_eye() {
        let mut rig = Rig::new();
        let at = Vec3::new(1.0, 1.0, 1.0);
        rig.effects.flash(at, 0.0, flash_style(0.4, secs(5.0)));

        for eye in [EYE, Vec3::new(-5.0, 2.0, 0.5), Vec3::new(0.0, 30.0, 0.1)] {
            let drawn = rig.frame(eye);

            assert_eq!(drawn.len(), 1);
            let toward_eye = (eye - at).normalize();
            let normal = x_axis(&drawn[0]).cross(y_axis(&drawn[0])).normalize();
            assert!(
                (normal - toward_eye).length() < EPS,
                "eye {eye}: flash normal {normal}, expected {toward_eye}"
            );
            assert!(x_axis(&drawn[0]).dot(y_axis(&drawn[0])).abs() < EPS);
        }
    }

    #[test]
    fn a_flash_roll_turns_it_about_the_view_axis() {
        let at = Vec3::new(1.0, 1.0, 1.0);
        let toward_eye = (EYE - at).normalize();
        let [flat, rolled] = [0.0, std::f32::consts::FRAC_PI_2].map(|roll| {
            let mut rig = Rig::new();
            rig.effects.flash(at, roll, flash_style(0.4, ms(50)));
            let drawn = rig.frame(EYE);
            assert_eq!(drawn.len(), 1);
            drawn[0]
        });

        for instance in [&flat, &rolled] {
            let normal = x_axis(instance).cross(y_axis(instance)).normalize();
            assert!(
                (normal - toward_eye).length() < EPS,
                "still faces the eye, got {normal}"
            );
        }
        let rolled_x = x_axis(&rolled).normalize();
        let flat_y = y_axis(&flat).normalize();
        assert!(
            (rolled_x - flat_y).length() < EPS || (rolled_x + flat_y).length() < EPS,
            "a quarter turn puts X where Y was, got {rolled_x} against {flat_y}"
        );
    }

    #[test]
    fn a_mark_lies_along_its_normal_offset_by_the_mark_offset() {
        let at = Vec3::new(5.0, 1.0, -2.0);
        let normal = Vec3::new(0.0, 0.6, 0.8);
        let mut rig = Rig::new();
        rig.effects
            .mark(at, normal, 0.3, mark_style(0.2, secs(10.0), secs(1.0)));

        let drawn = rig.frame(EYE);

        assert_eq!(drawn.len(), 1);
        assert_eq!(MARK_OFFSET, 0.01);
        assert_near(translation(&drawn[0]), at + normal * MARK_OFFSET);
        assert_eq!(drawn[0].material, MARK_MATERIAL);
        let (x, y) = (x_axis(&drawn[0]), y_axis(&drawn[0]));
        assert!((x.length() - 0.2).abs() < EPS, "0.2 across in X");
        assert!((y.length() - 0.2).abs() < EPS, "0.2 across in Y");
        assert!(x.dot(y).abs() < EPS, "a square, not a rhombus");
        assert!(x.dot(normal).abs() < EPS, "X lies in the surface");
        assert!(y.dot(normal).abs() < EPS, "Y lies in the surface");
        assert!(x.cross(y).dot(normal) > 0.0, "faces along the normal");
    }

    #[test]
    fn a_mark_roll_turns_it_about_its_normal() {
        let axes = [0.0, std::f32::consts::FRAC_PI_2].map(|roll| {
            let mut rig = Rig::new();
            rig.effects.mark(
                Vec3::ZERO,
                Vec3::Y,
                roll,
                mark_style(0.2, secs(10.0), secs(1.0)),
            );
            let drawn = rig.frame(EYE);
            assert_eq!(drawn.len(), 1);
            x_axis(&drawn[0]).normalize()
        });

        assert!(axes[0].dot(axes[1]).abs() < EPS, "a quarter turn apart");
    }

    #[test]
    fn a_mark_holds_full_size_then_shrinks_out_in_its_fade() {
        let mut rig = Rig::new();
        rig.effects.mark(
            Vec3::ZERO,
            Vec3::Y,
            0.0,
            mark_style(0.2, secs(10.0), secs(1.0)),
        );

        rig.effects.advance(secs(9.0));
        let full = rig.frame(EYE);
        assert_eq!(full.len(), 1);
        assert!((x_axis(&full[0]).length() - 0.2).abs() < EPS, "full size");

        rig.effects.advance(secs(0.5));
        let half = rig.frame(EYE);
        assert_eq!(half.len(), 1);
        assert!((x_axis(&half[0]).length() - 0.1).abs() < EPS, "half size");
        assert!((y_axis(&half[0]).length() - 0.1).abs() < EPS);
        assert_eq!(rig.effects.live(), 1);

        rig.effects.advance(secs(0.5));
        assert_eq!(rig.effects.live(), 0);
        assert!(rig.frame(EYE).is_empty());
    }

    #[test]
    fn a_full_cap_drops_the_oldest_fixed_lifetime_effect() {
        let mut rig = Rig::new();
        rig.effects.set_quality(1);
        let spawn = |rig: &mut Rig, x: f32| {
            rig.effects.mark(
                Vec3::new(x, 0.0, 0.0),
                Vec3::Y,
                0.0,
                mark_style(0.2, secs(10.0), secs(1.0)),
            );
        };
        for i in 0..32 {
            spawn(&mut rig, i as f32);
        }
        assert_eq!(rig.effects.live(), 32);

        spawn(&mut rig, 100.0);

        assert_eq!(rig.effects.live(), 32);
        let mut xs: Vec<f32> = rig
            .frame(EYE)
            .iter()
            .map(|i| translation(i).x.round())
            .collect();
        xs.sort_by(f32::total_cmp);
        let expected: Vec<f32> = (1..32).map(|i| i as f32).chain([100.0]).collect();
        assert_eq!(xs, expected, "the oldest (x = 0) went, the newest stayed");
    }

    #[test]
    fn a_full_cap_evicts_the_oldest_flash_or_mark_but_never_a_streak() {
        let mut rig = Rig::new();
        rig.effects.set_quality(1);
        let streaks: Vec<EffectHandle> = (0..31)
            .map(|_| rig.effects.begin_streak(style(2.0, 0.1)))
            .collect();
        rig.effects
            .flash(Vec3::new(500.0, 0.0, 0.0), 0.0, flash_style(0.4, secs(5.0)));
        assert_eq!(rig.effects.live(), 32);

        rig.effects.mark(
            Vec3::new(600.0, 0.0, 0.0),
            Vec3::Y,
            0.0,
            mark_style(0.2, secs(10.0), secs(1.0)),
        );

        assert_eq!(rig.effects.live(), 32);
        for (i, handle) in streaks.iter().enumerate() {
            rig.effects
                .place_streak(*handle, Vec3::new(i as f32, 0.0, 0.0), Vec3::X);
        }
        let drawn = rig.frame(EYE);
        assert_eq!(drawn.len(), 32, "31 streaks and the new mark");
        assert!(drawn.iter().all(|i| (translation(i).x - 500.0).abs() > 1.0));
        assert!(drawn.iter().any(|i| (translation(i).x - 600.0).abs() < 1.0));
    }

    #[test]
    fn a_full_cap_of_streaks_refuses_a_mark_and_keeps_every_streak() {
        let mut rig = Rig::new();
        rig.effects.set_quality(1);
        let streaks: Vec<EffectHandle> = (0..32)
            .map(|_| rig.effects.begin_streak(style(2.0, 0.1)))
            .collect();

        rig.effects.mark(
            Vec3::new(900.0, 0.0, 0.0),
            Vec3::Y,
            0.0,
            mark_style(0.2, secs(10.0), secs(1.0)),
        );
        rig.effects
            .flash(Vec3::new(900.0, 0.0, 0.0), 0.0, flash_style(0.4, ms(50)));

        assert_eq!(rig.effects.live(), 32);
        for handle in &streaks {
            rig.effects.place_streak(*handle, Vec3::X, Vec3::X);
        }
        let drawn = rig.frame(EYE);
        assert_eq!(drawn.len(), 32, "every streak and nothing else");
        assert!(drawn.iter().all(|i| translation(i).x < 800.0));
    }

    #[test]
    fn quality_zero_refuses_flash_and_mark() {
        let mut rig = Rig::new();
        rig.effects.set_quality(0);

        rig.effects.flash(Vec3::ZERO, 0.0, flash_style(0.4, ms(50)));
        rig.effects.mark(
            Vec3::ZERO,
            Vec3::Y,
            0.0,
            mark_style(0.2, secs(10.0), secs(1.0)),
        );

        assert_eq!(rig.effects.live(), 0);
        assert!(rig.frame(EYE).is_empty());
    }

    #[test]
    fn the_same_spawns_draw_the_same_instances() {
        let run = || {
            let mut rig = Rig::new();
            rig.effects
                .flash(Vec3::new(1.0, 2.0, 3.0), 0.7, flash_style(0.4, ms(50)));
            rig.effects.mark(
                Vec3::new(4.0, 0.0, 1.0),
                Vec3::new(0.0, 0.6, 0.8),
                1.3,
                mark_style(0.2, secs(10.0), secs(1.0)),
            );
            rig.effects.advance(ms(20));
            let drawn = rig.frame(EYE);
            assert_eq!(drawn.len(), 2);
            drawn
                .iter()
                .map(|i| (i.model, i.material))
                .collect::<Vec<_>>()
        };

        assert_eq!(run(), run());
    }

    #[test]
    fn flash_and_mark_meshes_register_once_each_as_a_star_and_a_quad() {
        let mut rig = Rig::new();
        for i in 0..3 {
            let x = i as f32;
            rig.effects
                .flash(Vec3::new(x, 0.0, 0.0), 0.0, flash_style(0.4, secs(5.0)));
            rig.effects.mark(
                Vec3::new(x, 1.0, 0.0),
                Vec3::Y,
                0.0,
                mark_style(0.2, secs(10.0), secs(1.0)),
            );
            rig.frame(EYE);
        }

        let mut meshes = rig.backend.registered.clone();
        meshes.sort_by_key(|(positions, _)| positions.len());
        assert_eq!(meshes.len(), 2, "one upload per shape, never re-uploaded");
        let [(quad, quad_indices), (star, star_indices)] = &meshes[..] else {
            unreachable!("two meshes were asserted above");
        };
        assert_eq!(quad.len(), 4, "the quad has four vertices");
        assert_eq!(quad_indices.len(), 6, "the quad is two triangles");
        assert_eq!(star.len(), 8, "the star has eight vertices");
        assert_eq!(star_indices.len() % 3, 0);
        assert!(!star_indices.is_empty());
        assert_flat_and_ccw(quad, quad_indices);
        assert_flat_and_ccw(star, star_indices);

        let mut corners = quad.clone();
        corners.sort_by(|a, b| a.partial_cmp(b).expect("finite corners"));
        assert_eq!(
            corners,
            vec![
                [-0.5, -0.5, 0.0],
                [-0.5, 0.5, 0.0],
                [0.5, -0.5, 0.0],
                [0.5, 0.5, 0.0]
            ],
            "a unit quad centred on the origin"
        );

        let points: Vec<Vec3> = star.iter().copied().map(Vec3::from).collect();
        let tips = points
            .iter()
            .filter(|p| (p.length() - 0.5).abs() < EPS)
            .count();
        assert_eq!(tips, 4, "four outer points at radius 0.5");
        assert!(
            points
                .iter()
                .all(|p| p.length() > EPS && p.length() < 0.5 + EPS),
            "inner points sit between the centre and the tips"
        );
        for p in &points {
            assert!(
                points.iter().any(|q| (*q + *p).length() < EPS),
                "{p} has no opposite point, so the star is not centred on the origin"
            );
        }
        let widest = points
            .iter()
            .flat_map(|p| points.iter().map(move |q| (*p - *q).length()))
            .fold(0.0f32, f32::max);
        assert!((widest - 1.0).abs() < EPS, "tip to tip is the unit size");
    }

    fn assert_flat_and_ccw(positions: &[[f32; 3]], indices: &[u32]) {
        assert!(positions.iter().all(|p| p[2] == 0.0), "lies in z = 0");
        for triangle in indices.chunks(3) {
            let [a, b, c] = [0, 1, 2].map(|i| Vec3::from(positions[triangle[i] as usize]));
            assert!(
                (b - a).cross(c - a).z > 0.0,
                "triangle {triangle:?} is counter-clockwise seen from +Z"
            );
        }
    }

    #[test]
    fn a_streak_a_flash_and_a_mark_in_one_frame_use_three_distinct_meshes() {
        let mut rig = Rig::new();
        let streak = rig.effects.begin_streak(StreakStyle::new(2.0, 0.1, 3));
        rig.effects
            .place_streak(streak, Vec3::new(10.0, 0.0, 0.0), Vec3::X);
        rig.effects
            .flash(Vec3::new(0.0, 1.0, 0.0), 0.0, flash_style(0.4, secs(5.0)));
        rig.effects.mark(
            Vec3::new(0.0, 0.0, 1.0),
            Vec3::Y,
            0.0,
            mark_style(0.2, secs(10.0), secs(1.0)),
        );

        let drawn = rig.frame_with_handles(EYE);

        assert_eq!(drawn.len(), 3);
        let handle_for = |material: u32| {
            let found: Vec<u32> = drawn
                .iter()
                .filter(|(_, i)| i.material == material)
                .map(|(h, _)| *h)
                .collect();
            assert_eq!(found.len(), 1, "one draw with material {material}");
            found[0]
        };
        let [kite, star, quad] = [3, FLASH_MATERIAL, MARK_MATERIAL].map(handle_for);
        assert!(
            kite != star && star != quad && kite != quad,
            "distinct meshes"
        );
        let positions_of = |handle: u32| {
            let index = usize::try_from(handle).expect("a small handle") - 1;
            rig.backend.registered[index].0.clone()
        };
        assert_eq!(positions_of(kite).len(), 4);
        assert!(
            positions_of(kite).contains(&[1.0, 0.0, 0.0]),
            "the streak draws the kite"
        );
        assert_eq!(positions_of(star).len(), 8, "the flash draws the star");
        let quad_positions = positions_of(quad);
        assert_eq!(quad_positions.len(), 4);
        assert!(
            quad_positions.iter().all(|p| p[0].abs() == 0.5),
            "the mark draws the quad"
        );
    }

    #[test]
    fn a_mark_with_no_fade_is_a_hard_cut_at_its_lifetime() {
        let mut rig = Rig::new();
        rig.effects.mark(
            Vec3::ZERO,
            Vec3::Y,
            0.0,
            mark_style(0.2, secs(10.0), Duration::ZERO),
        );

        let at_birth = rig.frame(EYE);
        rig.effects.advance(secs(9.0));
        let late = rig.frame(EYE);
        rig.effects.advance(secs(0.99));
        let last = rig.frame(EYE);

        for drawn in [&at_birth, &late, &last] {
            assert_eq!(drawn.len(), 1);
            assert!(all_finite(drawn));
            assert!((x_axis(&drawn[0]).length() - 0.2).abs() < EPS, "full size");
            assert!((y_axis(&drawn[0]).length() - 0.2).abs() < EPS, "full size");
        }
        rig.effects.advance(secs(0.01));
        assert_eq!(rig.effects.live(), 0);
        assert!(rig.frame(EYE).is_empty());
    }

    #[test]
    fn a_mark_fade_longer_than_its_lifetime_shrinks_from_birth() {
        let mut rig = Rig::new();
        rig.effects.mark(
            Vec3::ZERO,
            Vec3::Y,
            0.0,
            mark_style(0.2, secs(1.0), secs(4.0)),
        );

        let born = rig.frame(EYE);
        rig.effects.advance(secs(0.25));
        let quarter = rig.frame(EYE);
        rig.effects.advance(secs(0.5));
        let three_quarters = rig.frame(EYE);

        for (drawn, size) in [(&born, 0.2), (&quarter, 0.15), (&three_quarters, 0.05)] {
            assert_eq!(drawn.len(), 1);
            assert!(all_finite(drawn));
            assert!(
                (x_axis(&drawn[0]).length() - size).abs() < EPS,
                "expected {size}, got {}",
                x_axis(&drawn[0]).length()
            );
        }
        rig.effects.advance(secs(0.25));
        assert_eq!(rig.effects.live(), 0);
        assert!(rig.frame(EYE).is_empty());
    }

    #[test]
    fn a_mark_normalises_its_normal() {
        let at = Vec3::new(5.0, 1.0, -2.0);
        let mut rig = Rig::new();
        rig.effects.mark(
            at,
            Vec3::new(0.0, 3.0, 4.0),
            0.0,
            mark_style(0.2, secs(10.0), secs(1.0)),
        );

        let drawn = rig.frame(EYE);

        assert_eq!(drawn.len(), 1);
        let unit = Vec3::new(0.0, 0.6, 0.8);
        assert_near(translation(&drawn[0]), at + unit * MARK_OFFSET);
        let (x, y) = (x_axis(&drawn[0]), y_axis(&drawn[0]));
        assert!((x.length() - 0.2).abs() < EPS, "axes are size long");
        assert!((y.length() - 0.2).abs() < EPS, "axes are size long");
        assert!(x.dot(unit).abs() < EPS);
        assert!(y.dot(unit).abs() < EPS);
        assert!(x.cross(y).dot(unit) > 0.0);
    }

    #[test]
    fn a_zero_or_non_finite_normal_refuses_the_mark_and_evicts_nothing() {
        let bad = [
            Vec3::ZERO,
            Vec3::new(f32::NAN, 1.0, 0.0),
            Vec3::new(0.0, f32::INFINITY, 0.0),
        ];
        for normal in bad {
            let mut rig = Rig::new();
            rig.effects.set_quality(1);
            for i in 0..32 {
                rig.effects.mark(
                    Vec3::new(i as f32, 0.0, 0.0),
                    Vec3::Y,
                    0.0,
                    mark_style(0.2, secs(10.0), secs(1.0)),
                );
            }

            rig.effects.mark(
                Vec3::new(100.0, 0.0, 0.0),
                normal,
                0.0,
                mark_style(0.2, secs(10.0), secs(1.0)),
            );

            assert_eq!(rig.effects.live(), 32, "normal {normal}");
            let drawn = rig.frame(EYE);
            assert_eq!(drawn.len(), 32, "normal {normal}");
            assert!(all_finite(&drawn));
            assert!(
                drawn.iter().any(|i| translation(i).x.abs() < 0.5),
                "the oldest was not evicted, normal {normal}"
            );
            assert!(drawn.iter().all(|i| translation(i).x < 50.0));
        }
    }

    #[test]
    fn a_zero_normal_mark_is_not_stored_when_there_is_room() {
        let mut rig = Rig::new();

        rig.effects.mark(
            Vec3::ONE,
            Vec3::ZERO,
            0.0,
            mark_style(0.2, secs(10.0), secs(1.0)),
        );

        assert_eq!(rig.effects.live(), 0);
        assert!(rig.frame(EYE).is_empty());
    }

    #[test]
    fn a_non_finite_position_draws_nothing_and_does_not_panic() {
        let mut rig = Rig::new();
        let bad = Vec3::new(f32::NAN, 0.0, 0.0);

        rig.effects.flash(bad, 0.0, flash_style(0.4, secs(5.0)));
        rig.effects
            .mark(bad, Vec3::Y, 0.0, mark_style(0.2, secs(10.0), secs(1.0)));

        assert!(rig.frame(EYE).is_empty());
    }

    fn unit_normals() -> impl Strategy<Value = Vec3> {
        prop_oneof![
            Just(Vec3::X),
            Just(Vec3::Y),
            Just(Vec3::Z),
            Just(Vec3::NEG_X),
            Just(Vec3::NEG_Y),
            Just(Vec3::NEG_Z),
            (-1.0f32..1.0, -1.0f32..1.0, -1.0f32..1.0)
                .prop_filter_map("a direction needs length", |(x, y, z)| {
                    Vec3::new(x, y, z).try_normalize()
                }),
        ]
    }

    proptest! {
        #[test]
        fn a_mark_basis_is_finite_for_any_normal(
            normal in unit_normals(),
            roll in -7.0f32..7.0,
        ) {
            let mut rig = Rig::new();
            rig.effects
                .mark(Vec3::ONE, normal, roll, mark_style(0.2, secs(10.0), secs(1.0)));

            let drawn = rig.frame(EYE);

            prop_assert_eq!(drawn.len(), 1);
            prop_assert!(drawn[0].model.iter().flatten().all(|v| v.is_finite()));
            let (x, y) = (x_axis(&drawn[0]), y_axis(&drawn[0]));
            prop_assert!((x.length() - 0.2).abs() < 1e-3);
            prop_assert!((y.length() - 0.2).abs() < 1e-3);
            prop_assert!(x.dot(normal).abs() < 1e-3);
            prop_assert!(y.dot(normal).abs() < 1e-3);
            prop_assert!(x.cross(y).dot(normal) > 0.0);
        }
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
