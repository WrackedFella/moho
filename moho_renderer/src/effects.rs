//! Engine-drawn effects with an engine-owned life, fade, cap and unshadowed draw. A streak
//! is placed by handle: a game starts one, places it every frame and ends it. A flash or a
//! mark is fire-and-forget: spawned once, it draws and expires by itself.

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

/// How far a mark floats off its surface along the normal, so it does not z-fight.
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

    /// Where the streak draws this frame, if it was placed or is fading; consumes the
    /// placement.
    fn transform(&mut self, eye: Vec3) -> Option<Affine3A> {
        let placed = std::mem::take(&mut self.placed_this_frame);
        if !placed && self.ended_at.is_none() {
            return None;
        }
        let width = self.style.width * self.width_scale();
        streak_transform(self.head, self.heading?, self.style.length, width, eye)
    }

    fn finished_fading(&self) -> bool {
        self.ended_at
            .is_some_and(|ended_at| self.age.saturating_sub(ended_at) >= self.style.fade)
    }
}

/// How many built-in meshes there are: the variants of `MeshKind`.
const MESH_KINDS: usize = 3;

/// The built-in meshes, each uploaded once under its reserved id.
#[derive(Debug, Clone, Copy)]
enum MeshKind {
    Streak,
    Flash,
    Mark,
}

impl MeshKind {
    fn index(self) -> usize {
        self as usize
    }

    fn id(self) -> WorldMeshId {
        WorldMeshId(EFFECT_MESH_IDS_FROM + self as u64)
    }

    fn mesh(self) -> WorldMesh {
        match self {
            Self::Streak => streak_mesh(),
            Self::Flash => flash_mesh(),
            Self::Mark => mark_mesh(),
        }
    }
}

/// What a fixed-lifetime effect looks like beyond what both kinds share.
#[derive(Debug)]
enum Shape {
    /// Faces the eye, turned by `roll` about the facing axis.
    Flash { roll: f32 },
    /// Lies on a surface with unit `normal`; shrinks out over the last `fade`.
    Mark {
        normal: Vec3,
        roll: f32,
        fade: Duration,
    },
}

/// A flash or mark: spawned once, it expires at `lifetime` without being placed.
#[derive(Debug)]
struct Timed {
    shape: Shape,
    at: Vec3,
    size: f32,
    material: u32,
    lifetime: Duration,
    age: Duration,
}

impl Timed {
    /// The size factor in `0..=1`: a flash shrinks linearly over its life; a mark holds
    /// full size, then shrinks to zero over its last `fade`.
    fn scale(&self) -> f32 {
        let left = self.lifetime.saturating_sub(self.age).as_secs_f32();
        match self.shape {
            Shape::Flash { .. } => left / self.lifetime.as_secs_f32(),
            Shape::Mark { fade, .. } if fade.is_zero() => 1.0,
            Shape::Mark { fade, .. } => left / fade.as_secs_f32(),
        }
        .clamp(0.0, 1.0)
    }

    fn mesh_kind(&self) -> MeshKind {
        match self.shape {
            Shape::Flash { .. } => MeshKind::Flash,
            Shape::Mark { .. } => MeshKind::Mark,
        }
    }

    fn transform(&self, eye: Vec3) -> Option<Affine3A> {
        let (normal, roll, centre) = match self.shape {
            Shape::Flash { roll } => ((eye - self.at).try_normalize()?, roll, self.at),
            Shape::Mark { normal, roll, .. } => (normal, roll, self.at + normal * MARK_OFFSET),
        };
        let (sin, cos) = roll.sin_cos();
        let base_x = normal.any_orthonormal_vector();
        let base_y = normal.cross(base_x);
        let scale = self.size * self.scale();
        Some(Affine3A::from_cols(
            ((base_x * cos + base_y * sin) * scale).into(),
            ((base_y * cos - base_x * sin) * scale).into(),
            normal.into(),
            centre.into(),
        ))
    }
}

#[derive(Debug)]
enum Effect {
    Streak(Streak),
    Timed(Timed),
}

#[derive(Debug)]
pub struct Effects {
    store: BTreeMap<EffectHandle, Effect>,
    next_handle: u64,
    cap: usize,
    mesh_uploaded: [bool; MESH_KINDS],
}

impl Default for Effects {
    fn default() -> Self {
        Self {
            store: BTreeMap::new(),
            next_handle: 0,
            cap: CAPS[DEFAULT_QUALITY],
            mesh_uploaded: [false; MESH_KINDS],
        }
    }
}

impl Effects {
    fn next_handle(&mut self) -> EffectHandle {
        let handle = EffectHandle(self.next_handle);
        self.next_handle += 1;
        handle
    }

    /// Start a streak. At the quality's cap the handle is inert: never stored, so placing
    /// or ending it does nothing.
    pub fn begin_streak(&mut self, style: StreakStyle) -> EffectHandle {
        let handle = self.next_handle();
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

    /// Spawn a camera-facing star at `at`, turned by `roll`, shrinking over its lifetime.
    /// Refused for a zero lifetime, a non-finite input, or when the cap leaves no flash or
    /// mark to drop.
    pub fn flash(&mut self, at: Vec3, roll: f32, style: FlashStyle) {
        if !(at.is_finite() && roll.is_finite() && style.size.is_finite()) {
            return;
        }
        self.spawn(Timed {
            shape: Shape::Flash { roll },
            at,
            size: style.size,
            material: style.material,
            lifetime: style.lifetime,
            age: Duration::ZERO,
        });
    }

    /// Spawn a square lying on the surface at `at` with `normal`, turned by `roll` about
    /// it, full size until its last `style.fade` and then shrinking out. Refused like a
    /// flash, and for a zero or non-finite normal.
    pub fn mark(&mut self, at: Vec3, normal: Vec3, roll: f32, style: MarkStyle) {
        let Some(normal) = normal.try_normalize() else {
            return;
        };
        if !(at.is_finite() && roll.is_finite() && style.size.is_finite()) {
            return;
        }
        self.spawn(Timed {
            shape: Shape::Mark {
                normal,
                roll,
                fade: style.fade,
            },
            at,
            size: style.size,
            material: style.material,
            lifetime: style.lifetime,
            age: Duration::ZERO,
        });
    }

    /// Store a fixed-lifetime effect, dropping the oldest of them while at the cap.
    fn spawn(&mut self, effect: Timed) {
        if effect.lifetime.is_zero() || self.cap == 0 {
            return;
        }
        if self.store.len() < self.cap {
            let handle = self.next_handle();
            self.store.insert(handle, Effect::Timed(effect));
            return;
        }
        let evictable: Vec<EffectHandle> = self
            .store
            .iter()
            .filter_map(|(handle, e)| matches!(e, Effect::Timed(_)).then_some(*handle))
            .collect();
        let pinned = self.store.len() - evictable.len();
        if pinned >= self.cap {
            return;
        }
        let excess = (self.store.len() + 1).saturating_sub(self.cap);
        for handle in evictable.into_iter().take(excess) {
            self.store.remove(&handle);
        }
        let handle = self.next_handle();
        self.store.insert(handle, Effect::Timed(effect));
    }

    /// Start the fade; the streak then draws at its last placement until it is gone.
    pub fn end(&mut self, handle: EffectHandle) {
        if let Some(Effect::Streak(streak)) = self.store.get_mut(&handle) {
            streak.ended_at.get_or_insert(streak.age);
        }
    }

    /// Age every effect by the frame time, start the fade of any streak past its
    /// `max_age` and drop the finished ones and the flashes and marks past their lifetime.
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

    /// Place every streak placed this frame or fading and every live flash and mark,
    /// unshadowed, facing `eye`.
    pub(crate) fn draw(&mut self, meshes: &mut WorldMeshes, eye: Vec3) {
        for effect in self.store.values_mut() {
            let drawn = match effect {
                Effect::Streak(streak) => streak
                    .transform(eye)
                    .map(|t| (MeshKind::Streak, t, streak.style.material)),
                Effect::Timed(timed) => timed
                    .transform(eye)
                    .map(|t| (timed.mesh_kind(), t, timed.material)),
            };
            let Some((kind, transform, material)) = drawn else {
                continue;
            };
            if !std::mem::replace(&mut self.mesh_uploaded[kind.index()], true) {
                meshes.upsert_movable(kind.id(), kind.mesh());
            }
            meshes.place_unshadowed(kind.id(), transform, material);
        }
    }
}

/// A mesh facing +Z with unit normals and neutral lighting channels.
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

/// A four-point star of tip radius 0.5, tips on the axes, counter-clockwise seen from +Z.
fn flash_mesh() -> WorldMesh {
    const INNER: f32 = 0.15;
    let positions = (0..8u8)
        .map(|i| {
            let (sin, cos) = (f32::from(i) * std::f32::consts::FRAC_PI_4).sin_cos();
            let radius = if i % 2 == 0 { 0.5 } else { INNER };
            [cos * radius, sin * radius, 0.0]
        })
        .collect();
    let mut indices = vec![1, 3, 5, 1, 5, 7];
    for tip in [0, 2, 4, 6] {
        indices.extend([(tip + 7) % 8, tip, (tip + 1) % 8]);
    }
    flat_mesh(positions, indices)
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

    fn secs(n: u64) -> Duration {
        Duration::from_secs(n)
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

    /// The unit normal of the plane a flash or mark's X and Y span.
    fn plane_normal(instance: &InstanceGpu) -> Vec3 {
        x_axis(instance).cross(y_axis(instance)).normalize()
    }

    fn assert_close(actual: f32, expected: f32, what: &str) {
        assert!(
            (actual - expected).abs() < EPS,
            "{what}: expected {expected}, got {actual}"
        );
    }

    #[test]
    fn a_flash_shrinks_linearly_and_expires() {
        let mut rig = Rig::new();
        let at = Vec3::new(2.0, 1.0, -3.0);
        rig.effects.flash(at, 0.0, flash_style(0.4, ms(50)));

        let spawned = rig.frame(EYE);
        rig.effects.advance(ms(25));
        let half = rig.frame(EYE);
        rig.effects.advance(ms(25));
        let gone = rig.frame(EYE);

        assert_eq!(spawned.len(), 1, "drawn without being placed");
        assert_close(x_axis(&spawned[0]).length(), 0.4, "full size at spawn");
        assert_close(y_axis(&spawned[0]).length(), 0.4, "full size at spawn");
        assert_near(translation(&spawned[0]), at);
        assert_eq!(half.len(), 1);
        assert_close(x_axis(&half[0]).length(), 0.2, "half size at 25ms");
        assert_close(y_axis(&half[0]).length(), 0.2, "half size at 25ms");
        assert!(gone.is_empty(), "gone at its lifetime");
        assert_eq!(rig.effects.live(), 0);
    }

    #[test]
    fn a_flash_faces_the_eye() {
        let at = Vec3::new(1.0, 2.0, 3.0);
        let eyes = [Vec3::new(0.0, 0.0, 9.0), EYE, Vec3::new(-4.0, 7.0, -2.0)];
        for roll in [0.0, 0.7, -2.1] {
            let mut rig = Rig::new();
            rig.effects.flash(at, roll, flash_style(0.4, secs(1)));

            for eye in eyes {
                let drawn = rig.frame(eye);

                assert_eq!(drawn.len(), 1, "roll {roll}");
                let toward_eye = (eye - at).normalize();
                assert_near(plane_normal(&drawn[0]), toward_eye);
                assert!(x_axis(&drawn[0]).dot(y_axis(&drawn[0])).abs() < EPS);
            }
        }
    }

    #[test]
    fn a_flash_roll_turns_it_within_its_plane() {
        let at = Vec3::new(1.0, 2.0, 3.0);
        let roll = 0.5f32;
        let mut plain = Rig::new();
        plain.effects.flash(at, 0.0, flash_style(0.4, secs(1)));
        let mut rolled = Rig::new();
        rolled.effects.flash(at, roll, flash_style(0.4, secs(1)));

        let before = plain.frame(EYE);
        let after = rolled.frame(EYE);

        assert_eq!((before.len(), after.len()), (1, 1));
        let angle = x_axis(&before[0])
            .normalize()
            .dot(x_axis(&after[0]).normalize());
        assert_close(angle, roll.cos(), "cosine of the roll between the X axes");
        assert_near(plane_normal(&before[0]), plane_normal(&after[0]));
    }

    #[test]
    fn a_mark_lies_along_its_normal_offset_by_the_mark_offset() {
        let mut rig = Rig::new();
        let at = Vec3::new(3.0, 1.0, -2.0);
        let normal = Vec3::new(1.0, 2.0, 2.0).normalize();
        rig.effects
            .mark(at, normal, 0.3, mark_style(0.2, secs(10), secs(1)));

        let drawn = rig.frame(EYE);

        assert_eq!(drawn.len(), 1, "drawn without being placed");
        let mark = &drawn[0];
        assert_near(plane_normal(mark), normal);
        assert_near(translation(mark), at + normal * MARK_OFFSET);
        assert_close(x_axis(mark).length(), 0.2, "X spans the size");
        assert_close(y_axis(mark).length(), 0.2, "Y spans the size");
        assert!(x_axis(mark).dot(y_axis(mark)).abs() < EPS, "X is across Y");
        assert!(
            x_axis(mark).dot(normal).abs() < EPS,
            "X lies in the surface"
        );
        assert!(
            y_axis(mark).dot(normal).abs() < EPS,
            "Y lies in the surface"
        );
    }

    #[test]
    fn a_mark_roll_turns_it_about_its_normal() {
        let normal = Vec3::new(1.0, 2.0, 2.0).normalize();
        let roll = 0.6f32;
        let mut plain = Rig::new();
        plain
            .effects
            .mark(Vec3::ZERO, normal, 0.0, mark_style(1.0, secs(10), secs(1)));
        let mut rolled = Rig::new();
        rolled
            .effects
            .mark(Vec3::ZERO, normal, roll, mark_style(1.0, secs(10), secs(1)));

        let before = plain.frame(EYE);
        let after = rolled.frame(EYE);

        assert_eq!((before.len(), after.len()), (1, 1));
        assert_close(
            x_axis(&before[0]).dot(x_axis(&after[0])),
            roll.cos(),
            "cosine of the roll between the X axes",
        );
        assert_near(z_axis(&after[0]), normal);
    }

    #[test]
    fn a_mark_holds_full_size_then_shrinks_out_in_its_fade() {
        let mut rig = Rig::new();
        rig.effects.mark(
            Vec3::new(1.0, 0.0, 1.0),
            Vec3::Y,
            0.0,
            mark_style(0.2, secs(10), secs(1)),
        );

        rig.effects.advance(secs(9));
        let full = rig.frame(EYE);
        rig.effects.advance(ms(500));
        let half = rig.frame(EYE);
        rig.effects.advance(ms(500));
        let gone = rig.frame(EYE);

        assert_eq!(full.len(), 1);
        assert_close(x_axis(&full[0]).length(), 0.2, "full size at 9s");
        assert_close(y_axis(&full[0]).length(), 0.2, "full size at 9s");
        assert_eq!(half.len(), 1);
        assert_close(x_axis(&half[0]).length(), 0.1, "half size at 9.5s");
        assert_close(y_axis(&half[0]).length(), 0.1, "half size at 9.5s");
        assert!(gone.is_empty(), "gone at its lifetime");
        assert_eq!(rig.effects.live(), 0);
    }

    #[test]
    fn a_full_cap_drops_the_oldest_fixed_lifetime_effect() {
        let mut rig = Rig::new();
        rig.effects.set_quality(1);
        let style = mark_style(0.2, secs(10), secs(1));
        for i in 0..32 {
            let x = f32::from(u16::try_from(i).expect("small"));
            rig.effects
                .mark(Vec3::new(x + 1.0, 0.0, 0.0), Vec3::Y, 0.0, style);
        }
        assert_eq!(rig.effects.live(), 32);

        rig.effects
            .mark(Vec3::new(100.0, 0.0, 0.0), Vec3::Y, 0.0, style);

        assert_eq!(rig.effects.live(), 32);
        let xs: Vec<f32> = rig.frame(EYE).iter().map(|i| translation(i).x).collect();
        assert_eq!(xs.len(), 32);
        assert!(
            !xs.iter().any(|x| (x - 1.0).abs() < EPS),
            "the oldest mark is gone"
        );
        assert!(
            xs.iter().any(|x| (x - 2.0).abs() < EPS),
            "the next oldest stays"
        );
        assert!(
            xs.iter().any(|x| (x - 100.0).abs() < EPS),
            "the new one is in"
        );
    }

    #[test]
    fn a_full_cap_drops_a_flash_before_a_newer_mark() {
        let mut rig = Rig::new();
        rig.effects.set_quality(1);
        rig.effects
            .flash(Vec3::new(500.0, 0.0, 0.0), 0.0, flash_style(0.4, secs(10)));
        for i in 0..31 {
            let x = f32::from(u16::try_from(i).expect("small"));
            rig.effects.mark(
                Vec3::new(x + 1.0, 0.0, 0.0),
                Vec3::Y,
                0.0,
                mark_style(0.2, secs(10), secs(1)),
            );
        }
        assert_eq!(rig.effects.live(), 32);

        rig.effects.mark(
            Vec3::new(100.0, 0.0, 0.0),
            Vec3::Y,
            0.0,
            mark_style(0.2, secs(10), secs(1)),
        );

        assert_eq!(rig.effects.live(), 32);
        let xs: Vec<f32> = rig.frame(EYE).iter().map(|i| translation(i).x).collect();
        assert!(
            !xs.iter().any(|x| (x - 500.0).abs() < EPS),
            "the oldest effect, a flash, is gone"
        );
        assert!(
            xs.iter().any(|x| (x - 100.0).abs() < EPS),
            "the new one is in"
        );
    }

    #[test]
    fn a_full_cap_of_streaks_refuses_a_mark_and_keeps_every_streak() {
        let mut rig = Rig::new();
        rig.effects.set_quality(1);
        let mark_at = Vec3::new(100.0, 0.0, 0.0);
        let is_the_mark =
            |i: &InstanceGpu| (translation(i) - (mark_at + Vec3::Y * MARK_OFFSET)).length() < EPS;
        rig.effects
            .mark(mark_at, Vec3::Y, 0.0, mark_style(0.2, secs(10), secs(1)));
        assert!(
            rig.frame(EYE).iter().any(is_the_mark),
            "with a free slot the mark is accepted"
        );
        rig.effects.advance(secs(10));
        assert_eq!(rig.effects.live(), 0, "and it expires");
        let streaks: Vec<EffectHandle> = (0..32)
            .map(|_| rig.effects.begin_streak(style(2.0, 0.1)))
            .collect();
        for handle in &streaks {
            rig.effects
                .place_streak(*handle, Vec3::new(0.0, 5.0, 0.0), Vec3::X);
        }

        rig.effects
            .mark(mark_at, Vec3::Y, 0.0, mark_style(0.2, secs(10), secs(1)));

        assert_eq!(rig.effects.live(), 32, "the mark was refused");
        let drawn = rig.frame(EYE);
        assert_eq!(drawn.len(), 32, "every streak is kept");
        assert!(!drawn.iter().any(is_the_mark));
    }

    #[test]
    fn a_refused_spawn_evicts_nothing() {
        let mut rig = Rig::new();
        let streaks: Vec<EffectHandle> = (0..40)
            .map(|_| rig.effects.begin_streak(style(2.0, 0.1)))
            .collect();
        for handle in &streaks {
            rig.effects
                .place_streak(*handle, Vec3::new(0.0, 5.0, 0.0), Vec3::X);
        }
        let marks = [Vec3::new(100.0, 0.0, 0.0), Vec3::new(101.0, 0.0, 0.0)];
        for at in marks {
            rig.effects
                .mark(at, Vec3::Y, 0.0, mark_style(0.2, secs(10), secs(1)));
        }
        rig.effects.set_quality(1);
        assert_eq!(rig.effects.live(), 42, "kept though over the new cap");

        rig.effects.mark(
            Vec3::new(102.0, 0.0, 0.0),
            Vec3::Y,
            0.0,
            mark_style(0.2, secs(10), secs(1)),
        );

        assert_eq!(rig.effects.live(), 42, "the refused spawn evicted marks");
        let drawn = rig.frame(EYE);
        assert_eq!(drawn.len(), 42);
        for at in marks {
            let expected = at + Vec3::Y * MARK_OFFSET;
            assert!(
                drawn
                    .iter()
                    .any(|i| (translation(i) - expected).length() < EPS),
                "the mark at {at} is still drawn"
            );
        }
        assert!(
            !drawn.iter().any(|i| (translation(i).x - 102.0).abs() < EPS),
            "the refused mark is not drawn"
        );
    }

    #[test]
    fn a_full_cap_keeps_a_streak_and_drops_the_oldest_mark() {
        let mut rig = Rig::new();
        rig.effects.set_quality(1);
        let streak = rig.effects.begin_streak(style(2.0, 0.1));
        for i in 0..31 {
            let x = f32::from(u16::try_from(i).expect("small"));
            rig.effects.mark(
                Vec3::new(x + 1.0, 0.0, 0.0),
                Vec3::Y,
                0.0,
                mark_style(0.2, secs(10), secs(1)),
            );
        }
        assert_eq!(rig.effects.live(), 32);

        rig.effects.mark(
            Vec3::new(100.0, 0.0, 0.0),
            Vec3::Y,
            0.0,
            mark_style(0.2, secs(10), secs(1)),
        );
        rig.effects
            .place_streak(streak, Vec3::new(0.0, 50.0, 0.0), Vec3::X);

        assert_eq!(rig.effects.live(), 32);
        let drawn = rig.frame(EYE);
        let xs: Vec<f32> = drawn.iter().map(|i| translation(i).x).collect();
        assert_eq!(xs.len(), 32, "the streak is drawn among 31 marks");
        assert!(
            drawn
                .iter()
                .any(|i| (translation(i) - Vec3::new(-2.0, 50.0, 0.0)).length() < EPS),
            "the streak stays"
        );
        assert!(
            !xs.iter().any(|x| (x - 1.0).abs() < EPS),
            "the oldest mark is gone"
        );
        assert!(
            xs.iter().any(|x| (x - 2.0).abs() < EPS),
            "the next oldest stays"
        );
        assert!(
            xs.iter().any(|x| (x - 100.0).abs() < EPS),
            "the new one is in"
        );
    }

    #[test]
    fn quality_zero_clears_a_live_flash_and_mark() {
        let mut rig = Rig::new();
        rig.effects.flash(Vec3::X, 0.0, flash_style(0.4, secs(1)));
        rig.effects
            .mark(Vec3::Z, Vec3::Y, 0.0, mark_style(0.2, secs(10), secs(1)));
        assert_eq!(rig.effects.live(), 2);

        rig.effects.set_quality(0);

        assert_eq!(rig.effects.live(), 0);
        assert!(rig.frame(EYE).is_empty());
    }

    #[test]
    fn quality_zero_refuses_a_new_flash_and_mark() {
        let mut open = Rig::new();
        open.effects.flash(Vec3::X, 0.0, flash_style(0.4, secs(1)));
        open.effects
            .mark(Vec3::Z, Vec3::Y, 0.0, mark_style(0.2, secs(10), secs(1)));
        assert_eq!(
            open.frame(EYE).len(),
            2,
            "the same spawns draw when allowed"
        );
        let mut off = Rig::new();
        off.effects.set_quality(0);

        off.effects.flash(Vec3::X, 0.0, flash_style(0.4, secs(1)));
        off.effects
            .mark(Vec3::Z, Vec3::Y, 0.0, mark_style(0.2, secs(10), secs(1)));

        assert_eq!(off.effects.live(), 0);
        assert!(off.frame(EYE).is_empty());
    }

    #[test]
    fn the_same_spawns_draw_the_same_instances() {
        let run = || {
            let mut rig = Rig::new();
            rig.effects
                .flash(Vec3::new(1.0, 2.0, 3.0), 0.4, flash_style(0.4, ms(50)));
            rig.effects.mark(
                Vec3::new(-2.0, 0.0, 5.0),
                Vec3::new(0.0, 1.0, 1.0).normalize(),
                1.1,
                mark_style(0.2, secs(2), ms(500)),
            );
            let streak = rig.effects.begin_streak(style(2.0, 0.1));
            rig.effects
                .place_streak(streak, Vec3::new(4.0, 1.0, 0.0), Vec3::X);
            let mut frames = vec![rig.frame(EYE)];
            rig.effects.advance(ms(20));
            rig.effects
                .place_streak(streak, Vec3::new(5.0, 1.0, 0.0), Vec3::X);
            frames.push(rig.frame(EYE));
            rig.effects.advance(ms(1700));
            rig.effects
                .place_streak(streak, Vec3::new(6.0, 1.0, 0.0), Vec3::X);
            frames.push(rig.frame(EYE));
            frames
        };

        let first = run();
        let second = run();

        assert_eq!(first[0].len(), 3, "a flash, a mark and a streak");
        assert_eq!(first[1].len(), 3);
        assert_eq!(first[2].len(), 2, "the flash is gone; mark and streak stay");
        let mark_size = |instances: &[InstanceGpu]| {
            instances
                .iter()
                .map(|i| x_axis(i).length())
                .find(|len| (len - 0.112).abs() < EPS)
        };
        assert!(
            mark_size(&first[2]).is_some(),
            "the mark is at 0.2 * (2.0 - 1.72) / 0.5 = 0.112 at 1720ms"
        );
        assert_eq!(first.len(), second.len());
        for (a, b) in first.iter().flatten().zip(second.iter().flatten()) {
            assert_eq!(a.model, b.model);
            assert_eq!(a.material, b.material);
            assert_eq!(a.object_type, b.object_type);
        }
        assert_eq!(
            first.iter().map(Vec::len).collect::<Vec<_>>(),
            second.iter().map(Vec::len).collect::<Vec<_>>()
        );
    }

    #[test]
    fn the_streak_flash_and_mark_meshes_register_once_each_as_a_kite_a_star_and_a_unit_quad() {
        let mut rig = Rig::new();
        let streak = rig.effects.begin_streak(style(2.0, 0.1));
        for frame in 0u8..3 {
            let step = f32::from(frame);
            rig.effects
                .place_streak(streak, Vec3::new(4.0, 1.0, 0.0), Vec3::X);
            rig.effects
                .flash(Vec3::X * step, 0.0, flash_style(0.4, secs(1)));
            rig.effects.mark(
                Vec3::Z * step,
                Vec3::Y,
                0.0,
                mark_style(0.2, secs(10), secs(1)),
            );
            assert_eq!(
                rig.frame(EYE).len(),
                1 + 2 * (usize::from(frame) + 1),
                "the streak plus every flash and mark so far"
            );
        }

        assert_eq!(rig.backend.registered.len(), 3, "one upload per kind");
        let kite = streak_mesh();
        let others: Vec<&(Vec<[f32; 3]>, Vec<u32>)> = rig
            .backend
            .registered
            .iter()
            .filter(|(positions, _)| positions.as_slice() != kite.positions())
            .collect();
        assert_eq!(others.len(), 2, "the kite plus two more meshes");
        let star = others
            .iter()
            .find(|(positions, _)| positions.len() == 8)
            .expect("a four-point star has eight vertices");
        let quad = others
            .iter()
            .find(|(positions, _)| positions.len() == 4)
            .expect("a quad has four vertices");
        assert!(star.1.iter().all(|i| (*i as usize) < 8));
        assert_eq!(quad.1.len(), 6, "a quad is two triangles");
        for (name, (positions, indices)) in [("star", star), ("quad", quad)] {
            assert!(
                positions.iter().all(|p| p[2] == 0.0),
                "the {name} is in the XY plane"
            );
            assert!(!indices.is_empty() && indices.len() % 3 == 0);
            for triangle in indices.chunks(3) {
                let [a, b, c] = [0, 1, 2].map(|i| Vec3::from(positions[triangle[i] as usize]));
                assert!(
                    (b - a).cross(c - a).z > 0.0,
                    "the {name}'s triangle {triangle:?} is counter-clockwise seen from +Z"
                );
            }
            for axis in 0..2 {
                let lo = positions.iter().map(|p| p[axis]).fold(f32::MAX, f32::min);
                let hi = positions.iter().map(|p| p[axis]).fold(f32::MIN, f32::max);
                assert_close(
                    lo + hi,
                    0.0,
                    &format!("the {name} is centred on axis {axis}"),
                );
            }
        }
        for axis in 0..2 {
            let lo = quad.0.iter().map(|p| p[axis]).fold(f32::MAX, f32::min);
            let hi = quad.0.iter().map(|p| p[axis]).fold(f32::MIN, f32::max);
            assert_close(hi - lo, 1.0, "the quad is one unit across");
        }
        let tips = star
            .0
            .iter()
            .filter(|p| (Vec3::from(**p).length() - 0.5).abs() < EPS)
            .count();
        let reach = star
            .0
            .iter()
            .map(|p| Vec3::from(*p).length())
            .fold(0.0, f32::max);
        assert_close(reach, 0.5, "the star's tips reach radius 0.5");
        assert!(tips >= 4, "a four-point star has four tips, got {tips}");
    }

    #[test]
    fn a_flash_carries_its_material() {
        let mut rig = Rig::new();
        let style = FlashStyle {
            material: 3,
            ..flash_style(0.4, secs(1))
        };

        rig.effects.flash(Vec3::X, 0.0, style);
        let drawn = rig.frame(EYE);

        assert_eq!(drawn.len(), 1);
        assert_eq!(drawn[0].material, 3);
    }

    #[test]
    fn a_mark_carries_its_material() {
        let mut rig = Rig::new();
        let style = MarkStyle {
            material: 5,
            ..mark_style(0.2, secs(10), secs(1))
        };

        rig.effects.mark(Vec3::Z, Vec3::Y, 0.0, style);
        let drawn = rig.frame(EYE);

        assert_eq!(drawn.len(), 1);
        assert_eq!(drawn[0].material, 5);
    }

    #[test]
    fn a_flash_at_a_full_cap_of_marks_drops_the_oldest_mark() {
        let mut rig = Rig::new();
        rig.effects.set_quality(1);
        for i in 0..32 {
            let x = f32::from(u16::try_from(i).expect("small"));
            rig.effects.mark(
                Vec3::new(x + 1.0, 0.0, 0.0),
                Vec3::Y,
                0.0,
                mark_style(0.2, secs(10), secs(1)),
            );
        }
        assert_eq!(rig.effects.live(), 32);

        rig.effects
            .flash(Vec3::new(100.0, 0.0, 0.0), 0.0, flash_style(0.4, secs(10)));

        assert_eq!(rig.effects.live(), 32);
        let xs: Vec<f32> = rig.frame(EYE).iter().map(|i| translation(i).x).collect();
        assert_eq!(xs.len(), 32);
        assert!(
            !xs.iter().any(|x| (x - 1.0).abs() < EPS),
            "the oldest mark is gone"
        );
        assert!(
            xs.iter().any(|x| (x - 2.0).abs() < EPS),
            "the next oldest stays"
        );
        assert!(
            xs.iter().any(|x| (x - 100.0).abs() < EPS),
            "the new flash is in"
        );
    }

    #[test]
    fn a_zero_lifetime_flash_or_mark_is_never_drawn() {
        let mut rig = Rig::new();
        rig.effects
            .flash(Vec3::X, 0.0, flash_style(0.4, Duration::ZERO));
        rig.effects.mark(
            Vec3::Z,
            Vec3::Y,
            0.0,
            mark_style(0.2, Duration::ZERO, secs(1)),
        );
        rig.effects.mark(
            Vec3::Z,
            Vec3::Y,
            0.0,
            mark_style(0.2, Duration::ZERO, Duration::ZERO),
        );
        rig.effects
            .flash(Vec3::Y * 9.0, 0.0, flash_style(0.4, secs(1)));

        let drawn = rig.frame(EYE);
        rig.effects.advance(ms(1));
        let later = rig.frame(EYE);

        assert_eq!(drawn.len(), 1, "only the flash with a lifetime");
        assert_near(translation(&drawn[0]), Vec3::Y * 9.0);
        assert_eq!(later.len(), 1);
        assert!(
            drawn
                .iter()
                .chain(&later)
                .all(|i| { i.model.iter().flatten().all(|v| v.is_finite()) })
        );
    }

    #[test]
    fn a_fade_longer_than_the_lifetime_starts_shrunk_and_ends_at_the_lifetime() {
        let mut rig = Rig::new();
        rig.effects
            .mark(Vec3::X, Vec3::Y, 0.0, mark_style(0.2, secs(1), secs(5)));

        for (step, expected) in [0.04, 0.03, 0.02, 0.01].into_iter().enumerate() {
            let drawn = rig.frame(EYE);

            assert_eq!(drawn.len(), 1, "step {step}");
            assert_close(x_axis(&drawn[0]).length(), expected, "X at (1s - age) / 5s");
            assert_close(y_axis(&drawn[0]).length(), expected, "Y at (1s - age) / 5s");
            rig.effects.advance(ms(250));
        }

        assert!(rig.frame(EYE).is_empty(), "gone at its lifetime");
        assert_eq!(rig.effects.live(), 0);
    }

    #[test]
    fn spawning_after_the_quality_drops_below_the_live_count_evicts_down_to_the_cap() {
        let mut rig = Rig::new();
        for i in 0..40 {
            let x = f32::from(u16::try_from(i).expect("small"));
            rig.effects.mark(
                Vec3::new(x + 1.0, 0.0, 0.0),
                Vec3::Y,
                0.0,
                mark_style(0.2, secs(10), secs(1)),
            );
        }
        assert_eq!(rig.effects.live(), 40);
        rig.effects.set_quality(1);
        assert_eq!(rig.effects.live(), 40, "kept though over the new cap");

        rig.effects.mark(
            Vec3::X * 99.0,
            Vec3::Y,
            0.0,
            mark_style(0.2, secs(10), secs(1)),
        );
        rig.effects
            .flash(Vec3::X * 98.0, 0.0, flash_style(0.4, secs(1)));

        assert_eq!(rig.effects.live(), 32);
        let xs: Vec<f32> = rig.frame(EYE).iter().map(|i| translation(i).x).collect();
        assert_eq!(xs.len(), 32);
        assert!(
            xs.iter().any(|x| (x - 99.0).abs() < EPS),
            "the new mark is in"
        );
        assert!(
            xs.iter().any(|x| (x - 98.0).abs() < EPS),
            "the new flash is in"
        );
        for gone in 1i16..=10 {
            let gone = f32::from(gone);
            assert!(
                !xs.iter().any(|x| (x - gone).abs() < EPS),
                "the oldest marks are gone, but {gone} is drawn"
            );
        }
        assert!(
            xs.iter().any(|x| (x - 11.0).abs() < EPS),
            "the eleventh oldest stays"
        );
    }

    fn unit_vectors() -> impl Strategy<Value = Vec3> {
        prop_oneof![
            Just(Vec3::X),
            Just(Vec3::NEG_X),
            Just(Vec3::Y),
            Just(Vec3::NEG_Y),
            Just(Vec3::Z),
            Just(Vec3::NEG_Z),
            (-1.0f32..1.0, -1.0f32..1.0, -1.0f32..1.0).prop_filter_map(
                "a non-degenerate direction",
                |(x, y, z)| {
                    let raw = Vec3::new(x, y, z);
                    (raw.length() > 1e-2).then(|| raw.normalize())
                },
            ),
        ]
    }

    proptest! {
        #[test]
        fn a_mark_basis_is_finite_for_any_normal(
            normal in unit_vectors(),
            roll in -10.0f32..10.0,
        ) {
            let mut rig = Rig::new();
            rig.effects
                .mark(Vec3::ZERO, normal, roll, mark_style(1.0, secs(10), secs(1)));

            let drawn = rig.frame(EYE);

            prop_assert_eq!(drawn.len(), 1);
            let mark = &drawn[0];
            prop_assert!(mark.model.iter().flatten().all(|v| v.is_finite()));
            let (x, y, z) = (x_axis(mark), y_axis(mark), z_axis(mark));
            prop_assert!((x.length() - 1.0).abs() < 1e-3, "X is a unit vector");
            prop_assert!((y.length() - 1.0).abs() < 1e-3, "Y is a unit vector");
            prop_assert!((z - normal).length() < 1e-3, "Z is the normal");
            prop_assert!(x.dot(y).abs() < 1e-3);
            prop_assert!(x.dot(z).abs() < 1e-3);
            prop_assert!(y.dot(z).abs() < 1e-3);
            prop_assert!((x.cross(y) - z).length() < 1e-3, "right-handed about the normal");
        }
    }
}
