//! Engine-drawn effects. A streak is placed by handle: a game starts one, places it every
//! frame and ends it. A flash or mark is fire and forget: a game spawns it once. The engine
//! owns every effect's life, fade, cap and unshadowed draw.

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

/// A camera-facing star that shrinks linearly over its lifetime.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FlashStyle {
    /// Width tip to tip at spawn, in world units.
    pub size: f32,
    pub material: u32,
    pub lifetime: Duration,
}

/// A flat square lying on a surface that holds, then shrinks out over its fade.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MarkStyle {
    /// Side to side while held, in world units.
    pub size: f32,
    pub material: u32,
    pub lifetime: Duration,
    pub fade: Duration,
}

/// How far a mark is lifted off its surface so it does not z-fight with it.
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

    /// Where the streak draws this frame, facing `eye`; consumes the frame's placement.
    /// `None` when it was not placed and is not fading, has no heading yet, or is degenerate.
    fn frame_transform(&mut self, eye: Vec3) -> Option<Affine3A> {
        let placed = std::mem::take(&mut self.placed_this_frame);
        if !placed && self.ended_at.is_none() {
            return None;
        }
        let heading = self.heading?;
        let width = self.style.width * self.width_scale();
        streak_transform(self.head, heading, self.style.length, width, eye)
    }
}

/// A fixed-lifetime star, placed by the engine at spawn.
#[derive(Debug)]
struct Flash {
    style: FlashStyle,
    at: Vec3,
    roll: f32,
    age: Duration,
}

impl Flash {
    /// The size factor: 1 at spawn, falling linearly to 0 at `lifetime`.
    fn scale(&self) -> f32 {
        (1.0 - self.age.as_secs_f32() / self.style.lifetime.as_secs_f32()).clamp(0.0, 1.0)
    }
}

/// A fixed-lifetime quad lying on a surface; `normal` is a unit vector.
#[derive(Debug)]
struct Mark {
    style: MarkStyle,
    at: Vec3,
    normal: Vec3,
    roll: f32,
    age: Duration,
}

impl Mark {
    /// The size factor: 1 until `lifetime - fade`, then linear to 0 at `lifetime`.
    fn scale(&self) -> f32 {
        if self.style.fade.is_zero() {
            return 1.0;
        }
        let remaining = self.style.lifetime.saturating_sub(self.age);
        (remaining.as_secs_f32() / self.style.fade.as_secs_f32()).clamp(0.0, 1.0)
    }
}

#[derive(Debug)]
enum Effect {
    Streak(Streak),
    Flash(Flash),
    Mark(Mark),
}

impl Effect {
    /// Flashes and marks end by age alone, so they can make room at the cap.
    fn is_evictable(&self) -> bool {
        !matches!(self, Self::Streak(_))
    }
}

#[derive(Debug)]
pub struct Effects {
    store: BTreeMap<EffectHandle, Effect>,
    next_handle: u64,
    cap: usize,
    uploaded: [bool; EffectMesh::COUNT],
}

impl Default for Effects {
    fn default() -> Self {
        Self {
            store: BTreeMap::new(),
            next_handle: 0,
            cap: CAPS[DEFAULT_QUALITY],
            uploaded: [false; EffectMesh::COUNT],
        }
    }
}

impl Effects {
    /// Start a streak, evicting the oldest flashes and marks if the cap needs the room
    /// (see `evict_for_one`). When streaks alone fill the cap the handle is inert: never
    /// stored, so placing or ending it does nothing.
    pub fn begin_streak(&mut self, style: StreakStyle) -> EffectHandle {
        let handle = self.issue_handle();
        if self.evict_for_one() {
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

    /// Spawn a camera-facing star at `at`, rolled by `roll` about the facing axis. It
    /// shrinks linearly and is gone after its lifetime. Nothing is spawned for a zero
    /// lifetime, a non-positive or non-finite size or a non-finite input, nor when the cap
    /// leaves no room (see `evict_for_one`).
    pub fn flash(&mut self, at: Vec3, roll: f32, style: FlashStyle) {
        if !spawnable(at, roll, style.size, style.lifetime) {
            return;
        }
        self.spawn(Effect::Flash(Flash {
            style,
            at,
            roll,
            age: Duration::ZERO,
        }));
    }

    /// Spawn a quad lying on a surface at `at` with the given `normal` (normalised here),
    /// rolled by `roll` about it. Nothing is spawned for a zero or non-finite normal, a
    /// zero lifetime, a non-positive or non-finite size, a non-finite input, or when the cap
    /// leaves no room (see `evict_for_one`).
    pub fn mark(&mut self, at: Vec3, normal: Vec3, roll: f32, style: MarkStyle) {
        let Some(normal) = normal.try_normalize() else {
            return;
        };
        if !spawnable(at, roll, style.size, style.lifetime) {
            return;
        }
        self.spawn(Effect::Mark(Mark {
            style,
            at,
            normal,
            roll,
            age: Duration::ZERO,
        }));
    }

    fn spawn(&mut self, effect: Effect) {
        if !self.evict_for_one() {
            return;
        }
        let handle = self.issue_handle();
        self.store.insert(handle, effect);
    }

    fn issue_handle(&mut self) -> EffectHandle {
        let handle = EffectHandle(self.next_handle);
        self.next_handle += 1;
        handle
    }

    /// Make room for one more effect by dropping the oldest flashes and marks. If that
    /// cannot bring the live count below the cap, drops nothing and returns `false`.
    fn evict_for_one(&mut self) -> bool {
        let Some(over) = (self.store.len() + 1).checked_sub(self.cap) else {
            return true;
        };
        if self.store.values().filter(|e| e.is_evictable()).count() < over {
            return false;
        }
        let mut left = over;
        self.store.retain(|_, effect| {
            let evict = left > 0 && effect.is_evictable();
            left -= usize::from(evict);
            !evict
        });
        true
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

    /// Place every streak placed this frame or fading, and every flash and mark,
    /// unshadowed, facing `eye` where they face it.
    pub(crate) fn draw(&mut self, meshes: &mut WorldMeshes, eye: Vec3) {
        for effect in self.store.values_mut() {
            let drawn = match effect {
                Effect::Streak(streak) => streak
                    .frame_transform(eye)
                    .map(|transform| (EffectMesh::Streak, transform, streak.style.material)),
                Effect::Flash(flash) => flash_transform(flash, eye)
                    .map(|transform| (EffectMesh::Flash, transform, flash.style.material)),
                Effect::Mark(mark) => {
                    Some((EffectMesh::Mark, mark_transform(mark), mark.style.material))
                }
            };
            if let Some((mesh, transform, material)) = drawn {
                place(meshes, &mut self.uploaded, mesh, transform, material);
            }
        }
    }
}

/// Whether a flash or mark can be spawned: finite place and roll, a positive finite size
/// and a non-zero lifetime.
fn spawnable(at: Vec3, roll: f32, size: f32, lifetime: Duration) -> bool {
    at.is_finite() && roll.is_finite() && size.is_finite() && size > 0.0 && !lifetime.is_zero()
}

/// The built-in meshes effects draw with, one reserved id each.
#[derive(Debug, Clone, Copy)]
enum EffectMesh {
    Streak,
    Flash,
    Mark,
}

impl EffectMesh {
    const ALL: [Self; 3] = [Self::Streak, Self::Flash, Self::Mark];
    const COUNT: usize = Self::ALL.len();

    fn index(self) -> usize {
        self as usize
    }

    fn id(self) -> WorldMeshId {
        WorldMeshId(EFFECT_MESH_IDS_FROM + self as u64)
    }

    fn build(self) -> WorldMesh {
        match self {
            Self::Streak => streak_mesh(),
            Self::Flash => flash_mesh(),
            Self::Mark => mark_mesh(),
        }
    }
}

/// Place `transform` on `mesh`, uploading it on first use.
fn place(
    meshes: &mut WorldMeshes,
    uploaded: &mut [bool; EffectMesh::COUNT],
    mesh: EffectMesh,
    transform: Affine3A,
    material: u32,
) {
    if !std::mem::replace(&mut uploaded[mesh.index()], true) {
        meshes.upsert_movable(mesh.id(), mesh.build());
    }
    meshes.place_unshadowed(mesh.id(), transform, material);
}

/// A flat mesh facing +Z: every channel but the positions is the same for each vertex.
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
    .expect("every channel is as long as the positions and the indices are in range")
}

/// A four-point star in the XY plane, tips at 0.5 on the axes (one unit tip to tip),
/// counter-clockwise seen from +Z. Vertices 0..4 are the tips, 4..8 the notches between them.
fn flash_mesh() -> WorldMesh {
    const NOTCH: f32 = 0.09;
    flat_mesh(
        vec![
            [0.5, 0.0, 0.0],
            [0.0, 0.5, 0.0],
            [-0.5, 0.0, 0.0],
            [0.0, -0.5, 0.0],
            [NOTCH, NOTCH, 0.0],
            [-NOTCH, NOTCH, 0.0],
            [-NOTCH, -NOTCH, 0.0],
            [NOTCH, -NOTCH, 0.0],
        ],
        vec![
            4, 5, 6, 4, 6, 7, // the middle
            7, 0, 4, 4, 1, 5, 5, 2, 6, 6, 3, 7, // a tip between its two notches
        ],
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

/// A square of `size` centred on `centre`, facing along the unit `z`, with the unit `x`
/// perpendicular to it turned by `roll` about `z` (positive toward Y); X and Y are
/// right-handed.
fn rolled_square(z: Vec3, x: Vec3, roll: f32, size: f32, centre: Vec3) -> Affine3A {
    let y = z.cross(x);
    let (sin, cos) = roll.sin_cos();
    let rolled_x = x * cos + y * sin;
    Affine3A::from_cols(
        (rolled_x * size).into(),
        (z.cross(rolled_x) * size).into(),
        z.into(),
        centre.into(),
    )
}

/// Where a flash is drawn: its plane faces `eye`, falling back to +Z when the eye sits on it.
fn flash_transform(flash: &Flash, eye: Vec3) -> Option<Affine3A> {
    if !eye.is_finite() {
        return None;
    }
    let to_eye = eye - flash.at;
    let z = if to_eye.length() < 1e-4 {
        Vec3::Z
    } else {
        to_eye.normalize()
    };
    // X from world up keeps the star steady as the eye moves; straight above or below
    // there is no "up" to use.
    let across = Vec3::Y.cross(z);
    let x = if across.length() < 1e-4 {
        z.any_orthonormal_vector()
    } else {
        across.normalize()
    };
    Some(rolled_square(
        z,
        x,
        flash.roll,
        flash.style.size * flash.scale(),
        flash.at,
    ))
}

/// Where a mark is drawn: lying along its normal, lifted off the surface by `MARK_OFFSET`.
fn mark_transform(mark: &Mark) -> Affine3A {
    rolled_square(
        mark.normal,
        mark.normal.any_orthonormal_vector(),
        mark.roll,
        mark.style.size * mark.scale(),
        mark.at + mark.normal * MARK_OFFSET,
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
            self.frame_with_handles(eye)
                .into_iter()
                .map(|(_, instance)| instance)
                .collect()
        }

        /// Like `frame`, with each instance's backend mesh handle (1-based registration order).
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

    fn assert_len(axis: Vec3, expected: f32, what: &str) {
        assert!(
            (axis.length() - expected).abs() < EPS,
            "{what}: expected length {expected}, got {}",
            axis.length()
        );
    }

    fn assert_ccw_from_plus_z(positions: &[[f32; 3]], indices: &[u32]) {
        assert!(!indices.is_empty() && indices.len().is_multiple_of(3));
        for triangle in indices.chunks(3) {
            let [a, b, c] = [0, 1, 2].map(|i| Vec3::from(positions[triangle[i] as usize]));
            assert!(
                (b - a).cross(c - a).z > 0.0,
                "triangle {triangle:?} is counter-clockwise seen from +Z"
            );
        }
    }

    fn assert_orthogonal(a: Vec3, b: Vec3, what: &str) {
        assert!(
            a.dot(b).abs() < EPS,
            "{what}: {a} and {b} are not orthogonal"
        );
    }

    #[test]
    fn a_flash_shrinks_linearly_and_expires() {
        let mut rig = Rig::new();
        let at = Vec3::new(4.0, 1.0, -2.0);
        rig.effects.flash(
            at,
            0.0,
            FlashStyle {
                material: 7,
                ..flash_style(0.4, ms(50))
            },
        );

        let full = rig.frame(EYE);

        assert_eq!(rig.effects.live(), 1);
        assert_eq!(full.len(), 1, "a flash draws without being placed");
        assert_near(translation(&full[0]), at);
        assert_len(x_axis(&full[0]), 0.4, "x at spawn");
        assert_len(y_axis(&full[0]), 0.4, "y at spawn");
        assert_eq!(full[0].material, 7);

        rig.effects.advance(ms(25));
        let half = rig.frame(EYE);

        assert_eq!(half.len(), 1);
        assert_near(translation(&half[0]), at);
        assert_len(x_axis(&half[0]), 0.2, "x at half life");
        assert_len(y_axis(&half[0]), 0.2, "y at half life");
        assert_eq!(rig.effects.live(), 1);

        rig.effects.advance(ms(25));

        assert_eq!(rig.effects.live(), 0);
        assert!(rig.frame(EYE).is_empty());
    }

    #[test]
    fn a_flash_faces_the_eye() {
        let at = Vec3::new(4.0, 1.0, -2.0);
        for eye in [EYE, Vec3::new(-7.0, 2.0, 5.0), at + Vec3::Y * 10.0] {
            let mut rig = Rig::new();
            rig.effects.flash(at, 0.0, flash_style(0.4, ms(50)));

            let drawn = rig.frame(eye);

            assert_eq!(drawn.len(), 1, "eye {eye}");
            let (x, y, z) = (x_axis(&drawn[0]), y_axis(&drawn[0]), z_axis(&drawn[0]));
            let toward = (eye - at).normalize();
            assert!(x.cross(y).dot(toward) > 0.0, "X x Y points away from {eye}");
            assert_near(z, toward);
            assert_len(x, 0.4, "x");
            assert_len(y, 0.4, "y");
            assert_orthogonal(x, y, "x, y");
            assert_orthogonal(x, z, "x, z");
            assert_orthogonal(y, z, "y, z");
        }
    }

    #[test]
    fn the_flash_mesh_is_registered_once_as_a_flat_eight_vertex_star_wound_toward_plus_z() {
        let mut rig = Rig::new();
        rig.effects.flash(Vec3::ZERO, 0.0, flash_style(0.4, ms(50)));

        rig.frame(EYE);
        rig.effects.advance(ms(10));
        rig.frame(EYE);

        assert_eq!(rig.backend.registered.len(), 1, "uploaded once");
        let (positions, indices) = &rig.backend.registered[0];
        assert_eq!(positions.len(), 8, "a four-point star is eight vertices");
        assert!(
            positions.iter().all(|p| p[2] == 0.0),
            "the star is flat in XY"
        );
        assert_ccw_from_plus_z(positions, indices);
    }

    #[test]
    fn the_mark_mesh_is_registered_once_as_a_unit_quad_wound_toward_plus_z() {
        let mut rig = Rig::new();
        rig.effects.mark(
            Vec3::ZERO,
            Vec3::Y,
            0.0,
            mark_style(0.2, Duration::from_secs(10), ms(1000)),
        );

        rig.frame(EYE);
        rig.effects.advance(ms(10));
        rig.frame(EYE);

        assert_eq!(rig.backend.registered.len(), 1, "uploaded once");
        let (positions, indices) = &rig.backend.registered[0];
        assert_eq!(positions.len(), 4, "a quad");
        for p in positions {
            assert_eq!(p[2], 0.0);
            assert_eq!(p[0].abs(), 0.5);
            assert_eq!(p[1].abs(), 0.5);
        }
        assert_ccw_from_plus_z(positions, indices);
    }

    #[test]
    fn a_flash_roll_turns_it_within_its_plane() {
        let at = Vec3::new(4.0, 1.0, -2.0);
        let roll = 0.5;
        let mut plain = Rig::new();
        let mut rolled = Rig::new();
        plain.effects.flash(at, 0.0, flash_style(0.4, ms(50)));
        rolled.effects.flash(at, roll, flash_style(0.4, ms(50)));

        let before = plain.frame(EYE);
        let after = rolled.frame(EYE);

        assert_eq!((before.len(), after.len()), (1, 1), "both flashes draw");

        assert_near(z_axis(&after[0]), z_axis(&before[0]));
        assert_len(x_axis(&after[0]), 0.4, "rolled x");
        let turned = x_axis(&before[0])
            .normalize()
            .dot(x_axis(&after[0]).normalize());
        assert!(
            (turned - roll.cos()).abs() < EPS,
            "X turned by the roll, cos was {turned}"
        );
    }

    #[test]
    fn a_mark_lies_along_its_normal_offset_by_the_mark_offset() {
        let at = Vec3::new(3.0, 0.0, 5.0);
        for normal in [Vec3::Y, Vec3::new(1.0, 2.0, 2.0).normalize()] {
            let mut rig = Rig::new();
            rig.effects.mark(
                at,
                normal,
                0.3,
                mark_style(0.2, Duration::from_secs(10), ms(1000)),
            );

            let drawn = rig.frame(EYE);

            assert_eq!(drawn.len(), 1, "normal {normal}");
            assert_eq!(rig.effects.live(), 1);
            assert_near(translation(&drawn[0]), at + normal * MARK_OFFSET);
            let (x, y, z) = (x_axis(&drawn[0]), y_axis(&drawn[0]), z_axis(&drawn[0]));
            assert_near(z, normal);
            assert_len(x, 0.2, "x");
            assert_len(y, 0.2, "y");
            assert_orthogonal(x, y, "x, y");
            assert_orthogonal(x, normal, "x, normal");
            assert_orthogonal(y, normal, "y, normal");
            assert!(
                x.cross(y).dot(normal) > 0.0,
                "the front faces along the normal"
            );
        }
    }

    #[test]
    fn a_mark_carries_its_material() {
        let mut rig = Rig::new();
        rig.effects.mark(
            Vec3::X,
            Vec3::Y,
            0.0,
            MarkStyle {
                material: 5,
                ..mark_style(0.2, Duration::from_secs(10), ms(1000))
            },
        );

        let drawn = rig.frame(EYE);

        assert_eq!(drawn.len(), 1);
        assert_eq!(drawn[0].material, 5);
    }

    #[test]
    fn a_streak_started_at_a_cap_full_of_marks_evicts_the_oldest_mark() {
        let mut rig = Rig::new();
        rig.effects.set_quality(1);
        let life = Duration::from_secs(10);
        for i in 0..32 {
            let x = 10.0 * i as f32;
            rig.effects.mark(
                Vec3::new(x, 0.0, 0.0),
                Vec3::Y,
                0.0,
                mark_style(0.2, life, ms(1000)),
            );
        }
        assert_eq!(rig.effects.live(), 32);

        let streak = rig.effects.begin_streak(style(2.0, 0.1));
        rig.effects
            .place_streak(streak, Vec3::new(0.0, 0.0, 500.0), Vec3::X);
        let drawn = rig.frame(EYE);

        assert_eq!(rig.effects.live(), 32, "a mark made room");
        assert_eq!(drawn.len(), 32);
        assert!(
            drawn.iter().any(|i| (translation(i).z - 500.0).abs() < 1.0),
            "the streak is live, not inert"
        );
        let mut mark_xs: Vec<f32> = drawn
            .iter()
            .filter(|i| translation(i).z.abs() < 1.0)
            .map(|i| translation(i).x)
            .collect();
        mark_xs.sort_by(f32::total_cmp);
        let expected: Vec<f32> = (1..32).map(|i| 10.0 * i as f32).collect();
        assert_eq!(mark_xs, expected, "the oldest mark went, the other 31 stay");
    }

    #[test]
    fn the_flash_star_spans_its_size_tip_to_tip() {
        let mut rig = Rig::new();
        rig.effects.flash(Vec3::ZERO, 0.0, flash_style(0.4, ms(50)));

        rig.frame(EYE);

        assert_eq!(rig.backend.registered.len(), 1);
        let (positions, _) = &rig.backend.registered[0];
        let max_x = positions.iter().map(|p| p[0].abs()).fold(0.0, f32::max);
        let max_y = positions.iter().map(|p| p[1].abs()).fold(0.0, f32::max);
        assert!((max_x - 0.5).abs() < EPS, "tips on +-X at 0.5, got {max_x}");
        assert!((max_y - 0.5).abs() < EPS, "tips on +-Y at 0.5, got {max_y}");
    }

    #[test]
    fn a_flash_does_not_spin_as_the_eye_crosses_an_axis_plane() {
        let x_for = |eye: Vec3| {
            let mut rig = Rig::new();
            rig.effects.flash(Vec3::ZERO, 0.0, flash_style(0.4, ms(50)));
            let drawn = rig.frame(eye);
            assert_eq!(drawn.len(), 1, "eye {eye}");
            x_axis(&drawn[0]).normalize()
        };

        let above = x_for(Vec3::new(5.0, 1.0, 0.01));
        let below = x_for(Vec3::new(5.0, 1.0, -0.01));

        assert!(
            above.dot(below) > 0.99,
            "X jumped from {above} to {below} across the z = 0 plane"
        );
    }

    #[test]
    fn a_positive_roll_turns_x_toward_y_about_the_facing_axis() {
        let roll = 0.5;
        let life = Duration::from_secs(10);
        let draw_pair = |spawn: &dyn Fn(&mut Effects, f32)| {
            let mut plain = Rig::new();
            let mut rolled = Rig::new();
            spawn(&mut plain.effects, 0.0);
            spawn(&mut rolled.effects, roll);
            let before = plain.frame(EYE);
            let after = rolled.frame(EYE);
            assert_eq!((before.len(), after.len()), (1, 1), "both draw");
            (before[0], after[0])
        };
        let mut pairs = Vec::new();
        for normal in [Vec3::Y, Vec3::new(1.0, 2.0, 2.0).normalize()] {
            pairs.push(draw_pair(&|effects, roll| {
                effects.mark(Vec3::X, normal, roll, mark_style(0.2, life, ms(1000)));
            }));
        }
        pairs.push(draw_pair(&|effects, roll| {
            effects.flash(Vec3::X, roll, flash_style(0.4, life));
        }));

        for (before, after) in pairs {
            let z = z_axis(&before);
            let x0 = x_axis(&before).normalize();
            let x1 = x_axis(&after).normalize();
            assert!((x0.dot(x1) - roll.cos()).abs() < EPS, "cos of the roll");
            let toward_y = x1.dot(z.cross(x0));
            assert!(
                (toward_y - roll.sin()).abs() < EPS,
                "a positive roll turns X toward Y, sin was {toward_y}"
            );
        }
    }

    #[test]
    fn a_mark_roll_turns_it_about_its_normal() {
        let roll = 0.5;
        for normal in [Vec3::Y, Vec3::new(1.0, 2.0, 2.0).normalize()] {
            let mut plain = Rig::new();
            let mut rolled = Rig::new();
            let life = Duration::from_secs(10);
            plain
                .effects
                .mark(Vec3::X, normal, 0.0, mark_style(0.2, life, ms(1000)));
            rolled
                .effects
                .mark(Vec3::X, normal, roll, mark_style(0.2, life, ms(1000)));

            let before = plain.frame(EYE);
            let after = rolled.frame(EYE);

            assert_eq!((before.len(), after.len()), (1, 1), "normal {normal}");
            assert_near(z_axis(&after[0]), z_axis(&before[0]));
            let turned = x_axis(&before[0])
                .normalize()
                .dot(x_axis(&after[0]).normalize());
            assert!(
                (turned - roll.cos()).abs() < EPS,
                "normal {normal}: X turned by the roll, cos was {turned}"
            );
        }
    }

    #[test]
    fn a_mark_holds_full_size_then_shrinks_out_in_its_fade() {
        let mut rig = Rig::new();
        let at = Vec3::new(3.0, 0.0, 5.0);
        rig.effects.mark(
            at,
            Vec3::Y,
            0.0,
            mark_style(0.2, Duration::from_secs(10), ms(1000)),
        );

        rig.effects.advance(Duration::from_secs(9));
        let full = rig.frame(EYE);

        assert_eq!(full.len(), 1, "a mark draws without being placed");
        assert_len(x_axis(&full[0]), 0.2, "x at 9 s");
        assert_len(y_axis(&full[0]), 0.2, "y at 9 s");

        rig.effects.advance(ms(500));
        let half = rig.frame(EYE);

        assert_eq!(half.len(), 1);
        assert_len(x_axis(&half[0]), 0.1, "x at 9.5 s");
        assert_len(y_axis(&half[0]), 0.1, "y at 9.5 s");
        assert_eq!(rig.effects.live(), 1);

        rig.effects.advance(ms(500));

        assert_eq!(rig.effects.live(), 0);
        assert!(rig.frame(EYE).is_empty());
    }

    #[test]
    fn a_full_cap_drops_the_oldest_fixed_lifetime_effect() {
        let mut rig = Rig::new();
        rig.effects.set_quality(1);
        let style = mark_style(0.2, Duration::from_secs(10), ms(1000));
        for i in 0..32 {
            let x = 10.0 * i as f32;
            rig.effects
                .mark(Vec3::new(x, 0.0, 0.0), Vec3::Y, 0.0, style);
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
            "the first mark is gone: {xs:?}"
        );
        assert!(
            xs.iter().any(|x| (*x - 1000.0).abs() < 1.0),
            "the new mark drew"
        );
    }

    #[test]
    fn eviction_drops_the_oldest_flash_or_mark_and_skips_older_streaks() {
        let mut rig = Rig::new();
        rig.effects.set_quality(1);
        let streak = rig.effects.begin_streak(style(2.0, 0.1));
        rig.effects.flash(
            Vec3::new(0.0, 50.0, 0.0),
            0.0,
            flash_style(0.4, Duration::from_secs(5)),
        );
        let mark = mark_style(0.2, Duration::from_secs(10), ms(1000));
        for i in 1..31 {
            let x = 10.0 * i as f32;
            rig.effects.mark(Vec3::new(x, 0.0, 0.0), Vec3::Y, 0.0, mark);
        }
        assert_eq!(rig.effects.live(), 32);

        rig.effects
            .mark(Vec3::new(1000.0, 0.0, 0.0), Vec3::Y, 0.0, mark);
        rig.effects
            .place_streak(streak, Vec3::new(0.0, 0.0, 200.0), Vec3::X);
        let drawn = rig.frame(EYE);

        assert_eq!(rig.effects.live(), 32);
        assert_eq!(drawn.len(), 32, "the streak, 30 marks and the new one");
        assert!(
            drawn.iter().all(|i| (translation(i).y - 50.0).abs() > 1.0),
            "the oldest flash was dropped"
        );
        assert!(
            drawn.iter().any(|i| (translation(i).z - 200.0).abs() < 1.0),
            "the older streak stayed"
        );
    }

    #[test]
    fn a_full_cap_of_streaks_refuses_a_mark_and_keeps_every_streak() {
        let mut rig = Rig::new();
        rig.effects.set_quality(1);
        let handles: Vec<EffectHandle> = (0..32)
            .map(|_| rig.effects.begin_streak(style(2.0, 0.1)))
            .collect();

        rig.effects.mark(
            Vec3::new(100.0, 100.0, 100.0),
            Vec3::Y,
            0.0,
            mark_style(0.2, Duration::from_secs(10), ms(1000)),
        );
        rig.effects.flash(
            Vec3::new(100.0, 100.0, 100.0),
            0.0,
            flash_style(0.4, ms(50)),
        );
        for handle in &handles {
            rig.effects
                .place_streak(*handle, Vec3::new(0.0, 1.0, 0.0), Vec3::X);
        }
        let drawn = rig.frame(EYE);

        assert_eq!(rig.effects.live(), 32);
        assert_eq!(drawn.len(), 32, "every streak draws and no mark does");
        assert!(
            drawn.iter().all(|i| translation(i).x < 50.0),
            "no mark or flash drew"
        );
    }

    #[test]
    fn quality_zero_refuses_flash_and_mark() {
        let mut rig = Rig::new();
        rig.effects.set_quality(0);

        rig.effects.flash(Vec3::X, 0.0, flash_style(0.4, ms(50)));
        rig.effects.mark(
            Vec3::X,
            Vec3::Y,
            0.0,
            mark_style(0.2, Duration::from_secs(10), ms(1000)),
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
                Vec3::new(0.0, 1.0, 1.0).normalize(),
                1.1,
                mark_style(0.2, Duration::from_secs(10), ms(1000)),
            );
            let mut frames = vec![rig.frame(EYE)];
            for step in [ms(20), ms(20), ms(5000), ms(4500)] {
                rig.effects.advance(step);
                frames.push(rig.frame(EYE));
            }
            frames
                .iter()
                .map(|f| f.iter().map(|i| (i.model, i.material)).collect::<Vec<_>>())
                .collect::<Vec<_>>()
        };

        let first = run();
        let second = run();

        assert_eq!(first, second);
        assert_eq!(
            first[0].len(),
            2,
            "both drew, so the comparison is not vacuous"
        );
    }

    fn assert_basis_finite_and_square(normal: Vec3, roll: f32) {
        let mut rig = Rig::new();
        let at = Vec3::new(1.0, 2.0, 3.0);
        rig.effects.mark(
            at,
            normal,
            roll,
            mark_style(0.2, Duration::from_secs(10), ms(1000)),
        );

        let drawn = rig.frame(EYE);

        assert_eq!(drawn.len(), 1, "normal {normal}, roll {roll}");
        let (x, y, z) = (x_axis(&drawn[0]), y_axis(&drawn[0]), z_axis(&drawn[0]));
        for column in drawn[0].model {
            assert!(
                column.iter().all(|v| v.is_finite()),
                "normal {normal}: {column:?}"
            );
        }
        assert_near(z, normal);
        assert_len(x, 0.2, "x");
        assert_len(y, 0.2, "y");
        assert_orthogonal(x, y, "x, y");
        assert_orthogonal(x, normal, "x, normal");
        assert_orthogonal(y, normal, "y, normal");
        assert!(
            x.cross(y).dot(normal) > 0.0,
            "normal {normal}: the front faces along the normal"
        );
        assert_near(translation(&drawn[0]), at + normal * MARK_OFFSET);
    }

    #[test]
    fn a_mark_basis_is_finite_along_each_axis() {
        for normal in [Vec3::X, Vec3::Y, Vec3::Z, -Vec3::X, -Vec3::Y, -Vec3::Z] {
            for roll in [0.0, 1.0, -2.5] {
                assert_basis_finite_and_square(normal, roll);
            }
        }
    }

    proptest! {
        #[test]
        fn a_mark_basis_is_finite_for_any_normal(
            normal in (-1.0f32..1.0, -1.0f32..1.0, -1.0f32..1.0),
            roll in -7.0f32..7.0,
        ) {
            let normal = Vec3::new(normal.0, normal.1, normal.2);
            prop_assume!(normal.length() > 0.1);

            assert_basis_finite_and_square(normal.normalize(), roll);
        }
    }

    #[test]
    fn streak_flash_and_mark_draw_with_three_distinct_meshes() {
        let mut rig = Rig::new();
        let streak = rig.effects.begin_streak(StreakStyle::new(2.0, 0.1, 1));
        rig.effects.flash(
            Vec3::new(0.0, 5.0, 0.0),
            0.0,
            FlashStyle {
                material: 2,
                ..flash_style(0.4, Duration::from_secs(5))
            },
        );
        rig.effects.mark(
            Vec3::new(0.0, 0.0, 5.0),
            Vec3::Y,
            0.0,
            MarkStyle {
                material: 3,
                ..mark_style(0.2, Duration::from_secs(10), ms(1000))
            },
        );

        let mut frames = Vec::new();
        for _ in 0..2 {
            rig.effects
                .place_streak(streak, Vec3::new(5.0, 0.0, 0.0), Vec3::X);
            frames.push(rig.frame_with_handles(EYE));
            rig.effects.advance(ms(1));
        }

        for drawn in &frames {
            assert_eq!(drawn.len(), 3, "a streak, a flash and a mark draw");
        }
        assert_eq!(rig.backend.registered.len(), 3, "each mesh uploaded once");
        let handle_of = |frame: &Vec<(u32, InstanceGpu)>, material: u32| {
            frame
                .iter()
                .find(|(_, instance)| instance.material == material)
                .map(|(handle, _)| *handle)
                .expect("an instance of that material drew")
        };
        let handles = [1, 2, 3].map(|material| handle_of(&frames[0], material));
        assert_eq!(
            handles
                .iter()
                .collect::<std::collections::BTreeSet<_>>()
                .len(),
            3,
            "three distinct backend meshes: {handles:?}"
        );
        assert_eq!(
            handles,
            [1, 2, 3].map(|material| handle_of(&frames[1], material)),
            "the same meshes draw on the second frame"
        );
        let mesh = |handle: u32| &rig.backend.registered[handle as usize - 1].0;
        let [kite, star, quad] = handles.map(mesh);
        assert_eq!(kite.len(), 4);
        assert!(kite.contains(&[1.0, 0.0, 0.0]), "the streak is the kite");
        assert_eq!(star.len(), 8, "the flash is the star");
        assert_eq!(quad.len(), 4, "the mark is the quad");
        assert!(quad.iter().all(|p| p[0].abs() == 0.5 && p[1].abs() == 0.5));
    }

    #[test]
    fn a_flash_then_a_streak_both_stay_live() {
        let mut rig = Rig::new();
        rig.effects.flash(
            Vec3::new(0.0, 5.0, 0.0),
            0.0,
            flash_style(0.4, Duration::from_secs(5)),
        );

        let streak = rig.effects.begin_streak(style(2.0, 0.1));
        rig.effects
            .place_streak(streak, Vec3::new(5.0, 0.0, 0.0), Vec3::X);
        let drawn = rig.frame(EYE);

        assert_eq!(rig.effects.live(), 2);
        assert_eq!(drawn.len(), 2, "the flash and the streak both draw");
    }

    #[test]
    fn quality_zero_clears_live_flashes_and_marks() {
        let mut rig = Rig::new();
        rig.effects.flash(Vec3::X, 0.0, flash_style(0.4, ms(50)));
        rig.effects.mark(
            Vec3::X,
            Vec3::Y,
            0.0,
            mark_style(0.2, Duration::from_secs(10), ms(1000)),
        );
        assert_eq!(rig.effects.live(), 2, "both were accepted at quality 4");

        rig.effects.set_quality(0);

        assert_eq!(rig.effects.live(), 0);
        assert!(rig.frame(EYE).is_empty());
    }

    #[test]
    fn a_zero_lifetime_flash_never_draws() {
        let mut rig = Rig::new();
        rig.effects.flash(
            Vec3::new(0.0, 5.0, 0.0),
            0.0,
            flash_style(0.4, Duration::ZERO),
        );
        rig.effects
            .flash(Vec3::new(0.0, 6.0, 0.0), 0.0, flash_style(0.4, ms(50)));

        let drawn = rig.frame(EYE);

        assert_eq!(rig.effects.live(), 1, "only the flash with a lifetime");
        assert_eq!(drawn.len(), 1);
        assert_near(translation(&drawn[0]), Vec3::new(0.0, 6.0, 0.0));
    }

    fn assert_finite(instance: &InstanceGpu, what: &str) {
        for column in instance.model {
            assert!(column.iter().all(|v| v.is_finite()), "{what}: {column:?}");
        }
    }

    #[test]
    fn a_zero_fade_mark_holds_full_size_until_its_lifetime() {
        let life = Duration::from_secs(1);

        let mut abrupt = Rig::new();
        abrupt
            .effects
            .mark(Vec3::X, Vec3::Y, 0.0, mark_style(0.2, life, Duration::ZERO));
        abrupt.effects.advance(ms(999));
        let held = abrupt.frame(EYE);
        assert_eq!(held.len(), 1, "a zero fade holds until the lifetime");
        assert_finite(&held[0], "zero fade");
        assert_len(x_axis(&held[0]), 0.2, "zero fade holds full size");
        abrupt.effects.advance(ms(1));
        assert_eq!(abrupt.effects.live(), 0);
        assert!(abrupt.frame(EYE).is_empty());
    }

    #[test]
    fn a_fade_longer_than_the_lifetime_scales_by_remaining_life_over_fade() {
        let life = Duration::from_secs(1);

        let mut overlong = Rig::new();
        overlong.effects.mark(
            Vec3::X,
            Vec3::Y,
            0.0,
            mark_style(0.2, life, Duration::from_secs(2)),
        );
        let start = overlong.frame(EYE);
        assert_eq!(start.len(), 1);
        assert_finite(&start[0], "overlong fade at spawn");
        assert_len(
            x_axis(&start[0]),
            0.1,
            "shrinking from spawn at the fade's rate",
        );
        overlong.effects.advance(ms(500));
        let later = overlong.frame(EYE);
        assert_eq!(later.len(), 1);
        assert_finite(&later[0], "overlong fade later");
        assert_len(x_axis(&later[0]), 0.05, "(lifetime - age) / fade");
        overlong.effects.advance(ms(500));
        assert_eq!(overlong.effects.live(), 0);
        assert!(overlong.frame(EYE).is_empty());
    }

    #[test]
    fn a_flash_with_the_eye_at_its_centre_is_finite() {
        let at = Vec3::new(1.0, 2.0, 3.0);
        for eye in [at, at + Vec3::new(0.0, 0.0, 1e-6)] {
            let mut rig = Rig::new();
            rig.effects.flash(at, 0.3, flash_style(0.4, ms(50)));

            let drawn = rig.frame(eye);

            assert_eq!(drawn.len(), 1, "eye {eye:?}");
            assert_finite(&drawn[0], "flash with the eye at its centre");
            assert_near(translation(&drawn[0]), at);
            assert_len(x_axis(&drawn[0]), 0.4, "x");
            assert_len(y_axis(&drawn[0]), 0.4, "y");
            assert_orthogonal(x_axis(&drawn[0]), y_axis(&drawn[0]), "x, y");
        }
    }

    #[test]
    fn a_mark_normalises_its_normal_and_refuses_a_zero_one() {
        let at = Vec3::new(3.0, 0.0, 5.0);
        let unit = Vec3::new(1.0, 2.0, 2.0).normalize();
        let style = mark_style(0.2, Duration::from_secs(10), ms(1000));
        let mut long = Rig::new();
        let mut short = Rig::new();
        long.effects.mark(at, unit * 5.0, 0.4, style);
        short.effects.mark(at, unit, 0.4, style);

        let scaled = long.frame(EYE);
        let exact = short.frame(EYE);

        assert_eq!((scaled.len(), exact.len()), (1, 1));
        assert_near(translation(&scaled[0]), translation(&exact[0]));
        assert_near(x_axis(&scaled[0]), x_axis(&exact[0]));
        assert_near(y_axis(&scaled[0]), y_axis(&exact[0]));
        assert_near(z_axis(&scaled[0]), unit);
        assert_near(translation(&scaled[0]), at + unit * MARK_OFFSET);

        for bad in [
            Vec3::ZERO,
            Vec3::new(f32::NAN, 0.0, 1.0),
            Vec3::new(0.0, f32::INFINITY, 0.0),
        ] {
            long.effects.mark(at, bad, 0.0, style);
            assert_eq!(long.effects.live(), 1, "normal {bad:?} spawns nothing");
        }
        assert_eq!(long.frame(EYE).len(), 1);
    }

    #[test]
    fn a_spawn_over_a_lowered_cap_evicts_down_to_the_cap() {
        let mut rig = Rig::new();
        let style = mark_style(0.2, Duration::from_secs(10), ms(1000));
        for i in 0..40 {
            let x = 10.0 * i as f32;
            rig.effects
                .mark(Vec3::new(x, 0.0, 0.0), Vec3::Y, 0.0, style);
        }
        assert_eq!(rig.effects.live(), 40);
        rig.effects.set_quality(1);
        assert_eq!(rig.effects.live(), 40, "lowering keeps what is live");

        rig.effects
            .mark(Vec3::new(1000.0, 0.0, 0.0), Vec3::Y, 0.0, style);
        let drawn = rig.frame(EYE);

        assert_eq!(rig.effects.live(), 32, "the new effect included");
        assert_eq!(drawn.len(), 32);
        let xs: Vec<f32> = drawn.iter().map(|i| translation(i).x).collect();
        assert!(
            xs.iter().all(|x| *x > 85.0),
            "the nine oldest marks are gone: {xs:?}"
        );
        assert!(
            xs.iter().any(|x| (*x - 1000.0).abs() < 1.0),
            "the newest drew"
        );
    }

    fn sentinel_mesh(k: f32) -> WorldMesh {
        WorldMesh::new(
            vec![[k, 0.0, 0.0], [k + 1.0, 0.0, 0.0], [k, 1.0, 0.0]],
            vec![[0.0, 0.0, 1.0]; 3],
            vec![1.0; 3],
            vec![[1.0; 3]; 3],
            vec![1.0; 3],
            vec![0; 3],
            vec![0, 1, 2],
        )
        .expect("the sentinel's channels are all three vertices long and its indices are in range")
    }

    #[test]
    fn effects_never_overwrite_game_meshes_below_the_reserved_ids() {
        let mut rig = Rig::new();
        let ids = [0, 1, EFFECT_MESH_IDS_FROM - 1];
        let sentinels = [
            (ids[0], 10.0, 100),
            (ids[1], 20.0, 101),
            (ids[2], 30.0, 102),
        ];
        for (id, k, _) in sentinels {
            rig.meshes.upsert_movable(WorldMeshId(id), sentinel_mesh(k));
        }
        let streak = rig.effects.begin_streak(StreakStyle::new(2.0, 0.1, 1));
        rig.effects.flash(
            Vec3::new(0.0, 5.0, 0.0),
            0.0,
            FlashStyle {
                material: 2,
                ..flash_style(0.4, Duration::from_secs(5))
            },
        );
        rig.effects.mark(
            Vec3::new(0.0, 0.0, 5.0),
            Vec3::Y,
            0.0,
            MarkStyle {
                material: 3,
                ..mark_style(0.2, Duration::from_secs(10), ms(1000))
            },
        );

        for _ in 0..2 {
            for (id, _, material) in sentinels {
                rig.meshes
                    .place_unshadowed(WorldMeshId(id), Affine3A::IDENTITY, material);
            }
            rig.effects
                .place_streak(streak, Vec3::new(5.0, 0.0, 0.0), Vec3::X);
            let drawn = rig.frame_with_handles(EYE);

            assert_eq!(drawn.len(), 6, "three game meshes and three effects draw");
            let handle_of = |material: u32| {
                drawn
                    .iter()
                    .find(|(_, instance)| instance.material == material)
                    .map(|(handle, _)| *handle)
                    .expect("an instance of that material drew")
            };
            for (_, k, material) in sentinels {
                let (positions, _) = &rig.backend.registered[handle_of(material) as usize - 1];
                assert_eq!(
                    positions,
                    &vec![[k, 0.0, 0.0], [k + 1.0, 0.0, 0.0], [k, 1.0, 0.0]],
                    "game mesh {material} keeps its geometry"
                );
            }
            let effect_handles: std::collections::BTreeSet<u32> =
                [1, 2, 3].map(handle_of).into_iter().collect();
            let game_handles: std::collections::BTreeSet<u32> =
                [100, 101, 102].map(handle_of).into_iter().collect();
            assert_eq!(effect_handles.len(), 3);
            assert!(effect_handles.is_disjoint(&game_handles));
            rig.effects.advance(ms(1));
        }

        assert_eq!(
            rig.backend.registered.len(),
            6,
            "the effect meshes are additional, uploaded once"
        );
    }

    #[test]
    fn a_flash_or_mark_with_a_non_positive_or_non_finite_size_spawns_nothing() {
        let bad_sizes = [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, 0.0, -0.4];
        let life = Duration::from_secs(10);

        let mut empty = Rig::new();
        for size in bad_sizes {
            empty
                .effects
                .flash(Vec3::X, 0.0, flash_style(size, Duration::from_secs(5)));
            empty
                .effects
                .mark(Vec3::X, Vec3::Y, 0.0, mark_style(size, life, ms(1000)));
            assert_eq!(empty.effects.live(), 0, "size {size} spawns nothing");
        }
        assert!(empty.frame(EYE).is_empty());

        let mut full = Rig::new();
        full.effects.set_quality(1);
        for i in 0..32 {
            let x = 10.0 * i as f32;
            full.effects.mark(
                Vec3::new(x, 0.0, 0.0),
                Vec3::Y,
                0.0,
                mark_style(0.2, life, ms(1000)),
            );
        }
        for size in bad_sizes {
            full.effects.mark(
                Vec3::new(1000.0, 0.0, 0.0),
                Vec3::Y,
                0.0,
                mark_style(size, life, ms(1000)),
            );
            full.effects.flash(
                Vec3::new(1000.0, 0.0, 0.0),
                0.0,
                flash_style(size, Duration::from_secs(5)),
            );
        }
        let drawn = full.frame(EYE);

        assert_eq!(full.effects.live(), 32, "bad sizes evict nothing");
        assert_eq!(drawn.len(), 32, "all 32 originals still draw");
        let mut xs: Vec<f32> = drawn.iter().map(|i| translation(i).x).collect();
        xs.sort_by(f32::total_cmp);
        let expected: Vec<f32> = (0..32).map(|i| 10.0 * i as f32).collect();
        assert_eq!(xs, expected, "the originals, none replaced");
    }

    #[test]
    fn a_spawn_that_eviction_cannot_make_room_for_evicts_nothing() {
        let mut rig = Rig::new();
        let style = mark_style(0.2, Duration::from_secs(10), ms(1000));
        for _ in 0..35 {
            rig.effects.begin_streak(self::style(2.0, 0.1));
        }
        for i in 0..5 {
            let x = 10.0 * i as f32;
            rig.effects
                .mark(Vec3::new(x, 0.0, 0.0), Vec3::Y, 0.0, style);
        }
        assert_eq!(rig.effects.live(), 40);
        rig.effects.set_quality(1);

        rig.effects
            .mark(Vec3::new(1000.0, 0.0, 0.0), Vec3::Y, 0.0, style);
        let drawn = rig.frame(EYE);

        assert_eq!(rig.effects.live(), 40, "refused, and nothing evicted");
        assert_eq!(drawn.len(), 5, "the five old marks still draw");
        assert!(
            drawn.iter().all(|i| translation(i).x < 100.0),
            "the new mark did not"
        );
    }

    #[derive(Debug, Clone)]
    enum Op {
        Begin,
        End(usize),
        Advance(u64),
        SetQuality(u32),
        Flash,
        Mark,
    }

    fn ops() -> impl Strategy<Value = Vec<Op>> {
        prop::collection::vec(
            prop_oneof![
                4 => Just(Op::Begin),
                2 => (0usize..64).prop_map(Op::End),
                2 => (0u64..200).prop_map(Op::Advance),
                1 => (0u32..6).prop_map(Op::SetQuality),
                3 => Just(Op::Flash),
                3 => Just(Op::Mark),
            ],
            0..80,
        )
    }

    proptest! {
        #[test]
        fn random_ops_keep_handles_unique_spawns_capped_and_streaks_unevicted(ops in ops()) {
            let mut rig = Rig::new();
            let mut quality = 4u32;
            let mut clock = 0u64;
            let mut issued: Vec<EffectHandle> = Vec::new();
            // Streaks neither ended nor past their max_age: each must still draw when placed.
            let mut protected: Vec<(EffectHandle, u64)> = Vec::new();

            for op in ops {
                match op {
                    Op::Begin => {
                        let before = rig.effects.live();
                        let handle = rig.effects.begin_streak(StreakStyle {
                            fade: ms(50),
                            max_age: ms(300),
                            ..style(2.0, 0.1)
                        });
                        prop_assert!(!issued.contains(&handle), "{handle:?} issued twice");
                        if rig.effects.live() > before {
                            protected.push((handle, clock));
                        }
                        issued.push(handle);
                    }
                    Op::End(index) => {
                        if !protected.is_empty() {
                            let (handle, _) = protected.remove(index % protected.len());
                            rig.effects.end(handle);
                        }
                    }
                    Op::Advance(millis) => {
                        rig.effects.advance(ms(millis));
                        clock += millis;
                        protected.retain(|(_, born)| clock - born < 300);
                    }
                    Op::SetQuality(level) => {
                        rig.effects.set_quality(level);
                        quality = level;
                        if level == 0 {
                            protected.clear();
                        }
                    }
                    Op::Flash | Op::Mark => {
                        let before = rig.effects.live();
                        if matches!(op, Op::Flash) {
                            rig.effects.flash(Vec3::X, 0.0, flash_style(0.4, ms(200)));
                        } else {
                            rig.effects.mark(Vec3::X, Vec3::Y, 0.0, mark_style(0.2, ms(300), ms(50)));
                        }
                        let cap = CAPS[usize::try_from(quality.min(MAX_QUALITY)).expect("small")];
                        if before < cap {
                            prop_assert_eq!(rig.effects.live(), before + 1, "room, so it spawns");
                        } else {
                            prop_assert!(rig.effects.live() <= before, "no room, so none is added");
                        }
                    }
                }

                for (slot, (handle, _)) in protected.iter().enumerate() {
                    let z = 1000.0 + 10.0 * slot as f32;
                    rig.effects.place_streak(*handle, Vec3::new(0.0, 0.0, z), Vec3::X);
                }
                let drawn = rig.frame(EYE);
                for slot in 0..protected.len() {
                    let z = 1000.0 + 10.0 * slot as f32;
                    prop_assert!(
                        drawn.iter().any(|i| (translation(i).z - z).abs() < 1.0),
                        "streak {slot} left the store before its end or max_age"
                    );
                }
            }
        }
    }

    fn all_finite(instance: &InstanceGpu) -> bool {
        instance.model.iter().flatten().all(|v| v.is_finite())
    }

    #[test]
    fn a_huge_roll_still_draws_a_finite_size_correct_flash_and_mark() {
        let mut rig = Rig::new();
        rig.effects
            .flash(Vec3::ZERO, 1e30, flash_style(0.4, Duration::from_secs(1)));
        let flash = rig.frame(EYE);
        rig.effects.advance(Duration::from_secs(2));
        rig.effects.mark(
            Vec3::ZERO,
            Vec3::Y,
            1e30,
            mark_style(0.2, Duration::from_secs(10), ms(0)),
        );
        let mark = rig.frame(EYE);

        for (drawn, size, what) in [(&flash, 0.4, "flash"), (&mark, 0.2, "mark")] {
            assert_eq!(drawn.len(), 1, "{what} draws one instance");
            assert!(all_finite(&drawn[0]), "{what} columns are finite");
            assert_len(x_axis(&drawn[0]), size, what);
            assert_len(y_axis(&drawn[0]), size, what);
            assert_orthogonal(x_axis(&drawn[0]), y_axis(&drawn[0]), what);
        }
    }

    #[test]
    fn a_flash_whose_eye_is_not_finite_skips_the_frame_and_draws_again_after() {
        let mut rig = Rig::new();
        rig.effects
            .flash(Vec3::ZERO, 0.0, flash_style(0.4, Duration::from_secs(1)));

        let blind = rig.frame(Vec3::splat(f32::NAN));
        let live_after_blind = rig.effects.live();
        let next = rig.frame(EYE);

        assert!(blind.is_empty(), "a NaN eye draws nothing");
        assert_eq!(live_after_blind, 1);
        assert_eq!(next.len(), 1);
        assert!(all_finite(&next[0]));
    }

    #[test]
    fn an_effect_spawned_after_time_has_passed_starts_at_full_size() {
        let mut rig = Rig::new();
        rig.effects.advance(Duration::from_secs(5));

        rig.effects.flash(Vec3::ZERO, 0.0, flash_style(0.4, ms(50)));
        rig.effects.mark(
            Vec3::new(3.0, 0.0, 0.0),
            Vec3::Y,
            0.0,
            mark_style(0.2, Duration::from_secs(10), Duration::from_secs(1)),
        );
        let drawn = rig.frame(EYE);

        assert_eq!(drawn.len(), 2);
        let mut lengths: Vec<f32> = drawn.iter().map(|i| x_axis(i).length()).collect();
        lengths.sort_by(f32::total_cmp);
        assert!(
            (lengths[0] - 0.2).abs() < EPS,
            "mark full size: {lengths:?}"
        );
        assert!(
            (lengths[1] - 0.4).abs() < EPS,
            "flash full size: {lengths:?}"
        );
    }

    #[test]
    fn a_streak_over_a_lowered_cap_evicts_flashes_down_to_the_cap() {
        let mut rig = Rig::new();
        for i in 0..40 {
            rig.effects.flash(
                Vec3::new(i as f32 * 10.0, 0.0, 0.0),
                0.0,
                flash_style(0.4, Duration::from_secs(10)),
            );
        }
        assert_eq!(rig.effects.live(), 40);

        rig.effects.set_quality(1);
        let handle = rig.effects.begin_streak(style(2.0, 0.1));
        rig.effects
            .place_streak(handle, Vec3::new(0.0, 500.0, 0.0), Vec3::X);
        let drawn = rig.frame(EYE);

        assert_eq!(rig.effects.live(), 32);
        assert_eq!(drawn.len(), 32);
        assert!(
            drawn.iter().any(|i| (translation(i).y - 500.0).abs() < 1.0),
            "the streak draws"
        );
        let flash_xs: Vec<f32> = drawn
            .iter()
            .map(translation)
            .filter(|t| t.y.abs() < 1.0)
            .map(|t| t.x)
            .collect();
        assert_eq!(flash_xs.len(), 31);
        for i in 0..9 {
            assert!(
                !flash_xs.iter().any(|x| (x - i as f32 * 10.0).abs() < 1.0),
                "flash {i} is among the 9 oldest and is gone"
            );
        }
        for i in 9..40 {
            assert!(
                flash_xs.iter().any(|x| (x - i as f32 * 10.0).abs() < 1.0),
                "flash {i} survives"
            );
        }
    }

    #[test]
    fn a_mark_with_a_tiny_normal_is_unit_or_refused_never_nan() {
        let mut rig = Rig::new();

        rig.effects.mark(
            Vec3::ZERO,
            Vec3::new(0.0, 1e-30, 0.0),
            0.0,
            mark_style(0.2, Duration::from_secs(10), ms(0)),
        );
        let drawn = rig.frame(EYE);

        if rig.effects.live() == 0 {
            assert!(drawn.is_empty());
        } else {
            assert_eq!(drawn.len(), 1);
            assert!(all_finite(&drawn[0]), "columns are finite");
            assert_len(z_axis(&drawn[0]), 1.0, "z");
        }
    }

    #[test]
    fn advancing_by_zero_changes_nothing() {
        let mut rig = Rig::new();
        rig.effects
            .flash(Vec3::ZERO, 0.3, flash_style(0.4, Duration::from_secs(1)));
        rig.effects.mark(
            Vec3::new(3.0, 0.0, 0.0),
            Vec3::Y,
            0.5,
            mark_style(0.2, Duration::from_secs(10), Duration::from_secs(1)),
        );
        let before = rig.frame(EYE);

        rig.effects.advance(Duration::ZERO);
        let after = rig.frame(EYE);

        assert_eq!(before.len(), 2);
        assert_eq!(before.len(), after.len());
        for (b, a) in before.iter().zip(&after) {
            assert_eq!(b.model, a.model);
            assert_eq!(b.material, a.material);
        }
        assert_eq!(rig.effects.live(), 2);
    }
}
