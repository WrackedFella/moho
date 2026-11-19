//! Engine-drawn effects, placed by handle: a game starts one, places it every frame and
//! ends it; the engine owns its life, fade, cap and unshadowed draw.

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

/// A camera-facing star that shrinks linearly to nothing over its lifetime.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FlashStyle {
    /// Tip to tip across the star, in world units.
    pub size: f32,
    /// Material index the star draws with.
    pub material: u32,
    /// How long the flash lives; a zero lifetime spawns nothing.
    pub lifetime: Duration,
}

/// A flat square on a surface: full size until `lifetime - fade`, then shrinking to nothing.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MarkStyle {
    /// Side of the square, in world units.
    pub size: f32,
    /// Material index the square draws with.
    pub material: u32,
    /// How long the mark lives; a zero lifetime spawns nothing.
    pub lifetime: Duration,
    /// The closing part of the lifetime spent shrinking; clamped to the lifetime.
    pub fade: Duration,
}

/// How far a mark floats off its surface along the normal.
const MARK_OFFSET: f32 = 0.01;

/// Live-effect cap per effects quality, indexed like `QUALITY_NAMES`.
const CAPS: [usize; 5] = [0, 32, 64, 128, 256];

/// The highest quality level, ultra; higher values clamp to it.
const MAX_QUALITY: u32 = 4;

/// Quality used until a game sets one; matches the default `effects_quality` pref.
const DEFAULT_QUALITY: usize = 3;

/// Whether a flash or mark of this size can be seen: finite and above zero.
fn is_drawable_size(size: f32) -> bool {
    size.is_finite() && size > 0.0
}

/// Whether a flash or mark at `at` can be spawned: a finite position and roll, a drawable
/// size and a non-zero lifetime.
fn spawnable(at: Vec3, roll: f32, size: f32, lifetime: Duration) -> bool {
    at.is_finite() && roll.is_finite() && is_drawable_size(size) && !lifetime.is_zero()
}

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

impl Flash {
    fn scale(&self) -> f32 {
        (1.0 - self.age.as_secs_f32() / self.style.lifetime.as_secs_f32()).clamp(0.0, 1.0)
    }
}

#[derive(Debug)]
struct Mark {
    /// The centre, already lifted off the surface.
    at: Vec3,
    normal: Vec3,
    roll: f32,
    style: MarkStyle,
    age: Duration,
}

impl Mark {
    /// 1 until the fade starts, then falling linearly to 0 at the lifetime.
    fn scale(&self) -> f32 {
        let fade = self.style.fade.min(self.style.lifetime);
        let remaining = self.style.lifetime.saturating_sub(self.age);
        if fade.is_zero() || remaining >= fade {
            return 1.0;
        }
        (remaining.as_secs_f32() / fade.as_secs_f32()).clamp(0.0, 1.0)
    }
}

#[derive(Debug)]
enum Effect {
    Streak(Streak),
    Flash(Flash),
    Mark(Mark),
}

impl Effect {
    /// Flashes and marks end on their own and can make room for a new one; streaks
    /// belong to their game until it ends them.
    fn is_fixed_lifetime(&self) -> bool {
        !matches!(self, Self::Streak(_))
    }

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

    /// The mesh slot, transform and material to draw this frame, if any.
    fn drawn(&mut self, eye: Vec3) -> Option<(Slot, Affine3A, u32)> {
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
                Some((Slot::Streak, transform, streak.style.material))
            }
            Self::Flash(flash) => {
                let facing = (eye - flash.at).try_normalize().unwrap_or(Vec3::Z);
                let size = flash.style.size * flash.scale();
                Some((
                    Slot::Flash,
                    surface_transform(flash.at, facing, flash.roll, size),
                    flash.style.material,
                ))
            }
            Self::Mark(mark) => {
                let size = mark.style.size * mark.scale();
                Some((
                    Slot::Mark,
                    surface_transform(mark.at, mark.normal, mark.roll, size),
                    mark.style.material,
                ))
            }
        }
    }
}

/// The mesh an effect draws with; its discriminant is added to `EFFECT_MESH_IDS_FROM`.
#[derive(Debug, Clone, Copy)]
enum Slot {
    Streak,
    Flash,
    Mark,
}

/// How many `Slot` variants there are.
const SLOTS: usize = 3;

impl Slot {
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

#[derive(Debug)]
pub struct Effects {
    stored: BTreeMap<EffectHandle, Effect>,
    next_handle: u64,
    cap: usize,
    mesh_uploaded: [bool; SLOTS],
}

impl Default for Effects {
    fn default() -> Self {
        Self {
            stored: BTreeMap::new(),
            next_handle: 0,
            cap: CAPS[DEFAULT_QUALITY],
            mesh_uploaded: [false; SLOTS],
        }
    }
}

impl Effects {
    /// Start a streak. At the quality's cap the handle is inert: never stored, so placing
    /// or ending it does nothing.
    pub fn begin_streak(&mut self, style: StreakStyle) -> EffectHandle {
        self.store(Effect::Streak(Streak {
            style,
            head: Vec3::ZERO,
            heading: None,
            placed_this_frame: false,
            age: Duration::ZERO,
            ended_at: None,
        }))
    }

    /// Spawn a flash at `at`, rolled by `roll` radians about the line to the eye.
    ///
    /// Refused, drawing nothing, when `at` or `roll` is not finite, the size is not finite
    /// and positive, or the lifetime is zero, and at the quality's cap when no older flash
    /// or mark can make room.
    pub fn flash(&mut self, at: Vec3, roll: f32, style: FlashStyle) {
        if !spawnable(at, roll, style.size, style.lifetime) {
            return;
        }
        self.store(Effect::Flash(Flash {
            at,
            roll,
            style,
            age: Duration::ZERO,
        }));
    }

    /// Spawn a mark at `at` lying on a surface with `normal`, rolled by `roll` radians.
    ///
    /// Refused, drawing nothing, when `at` or `roll` is not finite, the size is not finite
    /// and positive, `normal` is zero or not finite, or the lifetime is zero, and at the
    /// quality's cap when no older flash or mark can make room.
    pub fn mark(&mut self, at: Vec3, normal: Vec3, roll: f32, style: MarkStyle) {
        if !spawnable(at, roll, style.size, style.lifetime) {
            return;
        }
        let Some(normal) = normal.try_normalize() else {
            return;
        };
        self.store(Effect::Mark(Mark {
            at: at + normal * MARK_OFFSET,
            normal,
            roll,
            style,
            age: Duration::ZERO,
        }));
    }

    /// Issue the next handle and keep `effect` under it if the cap allows.
    fn store(&mut self, effect: Effect) -> EffectHandle {
        let handle = EffectHandle(self.next_handle);
        self.next_handle += 1;
        if self.make_room(effect.is_fixed_lifetime()) {
            self.stored.insert(handle, effect);
        }
        handle
    }

    /// Whether one more effect fits, dropping the oldest flash or mark first when
    /// `evict` and the store is exactly full. Streaks are never dropped.
    fn make_room(&mut self, evict: bool) -> bool {
        if self.stored.len() < self.cap {
            return true;
        }
        if !evict || self.stored.len() > self.cap {
            return false;
        }
        let oldest = self
            .stored
            .iter()
            .find(|(_, effect)| effect.is_fixed_lifetime())
            .map(|(handle, _)| *handle);
        if let Some(handle) = oldest {
            self.stored.remove(&handle);
            true
        } else {
            false
        }
    }

    fn streak_mut(&mut self, handle: EffectHandle) -> Option<&mut Streak> {
        match self.stored.get_mut(&handle) {
            Some(Effect::Streak(streak)) => Some(streak),
            _ => None,
        }
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

    /// Start the fade; the streak then draws at its last placement until it is gone.
    pub fn end(&mut self, handle: EffectHandle) {
        if let Some(streak) = self.streak_mut(handle) {
            streak.ended_at.get_or_insert(streak.age);
        }
    }

    /// Age every effect by the frame time, start the fade of any streak past its
    /// `max_age` and drop the finished ones.
    pub fn advance(&mut self, dt: Duration) {
        for effect in self.stored.values_mut() {
            effect.advance(dt);
        }
        self.stored.retain(|_, effect| !effect.finished());
    }

    /// Cap new effects for `quality` (0 off to 4 ultra; higher clamps). Lowering keeps
    /// what is live; 0 clears it.
    pub fn set_quality(&mut self, quality: u32) {
        self.cap = CAPS
            [usize::try_from(quality.min(MAX_QUALITY)).expect("a quality of at most 4 fits usize")];
        if self.cap == 0 {
            self.stored.clear();
        }
    }

    /// How many effects are live, fading ones included.
    pub fn live(&self) -> usize {
        self.stored.len()
    }

    /// Place every flash and mark, and every streak placed this frame or fading,
    /// unshadowed; streaks and flashes face `eye`.
    pub(crate) fn draw(&mut self, meshes: &mut WorldMeshes, eye: Vec3) {
        for effect in self.stored.values_mut() {
            let Some((slot, transform, material)) = effect.drawn(eye) else {
                continue;
            };
            let id = slot.id();
            if !std::mem::replace(&mut self.mesh_uploaded[slot as usize], true) {
                meshes.upsert_movable(id, slot.mesh());
            }
            meshes.place_unshadowed(id, transform, material);
        }
    }
}

/// A mesh in the XY plane facing +Z, from its positions and counter-clockwise indices.
fn flat_mesh(positions: Vec<[f32; 2]>, indices: Vec<u32>) -> WorldMesh {
    let count = positions.len();
    WorldMesh::new(
        positions.into_iter().map(|[x, y]| [x, y, 0.0]).collect(),
        vec![[0.0, 0.0, 1.0]; count],
        vec![1.0; count],
        vec![[1.0; 3]; count],
        vec![1.0; count],
        vec![0; count],
        indices,
    )
    .expect("a flat mesh's channels all match its vertex count and its indices are in range")
}

/// A kite from the tail at the origin to the head at x = 1, counter-clockwise seen from +Z.
fn streak_mesh() -> WorldMesh {
    flat_mesh(
        vec![[0.0, 0.0], [0.8, -0.5], [1.0, 0.0], [0.8, 0.5]],
        vec![0, 1, 2, 0, 2, 3],
    )
}

/// A four-point star centred on the origin, tips at radius 0.5 on the axes alternating
/// with inner points on the diagonals, counter-clockwise seen from +Z.
fn flash_mesh() -> WorldMesh {
    flat_mesh(
        vec![
            [0.5, 0.0],
            [0.1, 0.1],
            [0.0, 0.5],
            [-0.1, 0.1],
            [-0.5, 0.0],
            [-0.1, -0.1],
            [0.0, -0.5],
            [0.1, -0.1],
        ],
        // The inner square, then a triangle on each of its sides out to a tip.
        vec![1, 3, 5, 1, 5, 7, 7, 0, 1, 1, 2, 3, 3, 4, 5, 5, 6, 7],
    )
}

/// A unit square centred on the origin, counter-clockwise seen from +Z.
fn mark_mesh() -> WorldMesh {
    flat_mesh(
        vec![[-0.5, -0.5], [0.5, -0.5], [0.5, 0.5], [-0.5, 0.5]],
        vec![0, 1, 2, 0, 2, 3],
    )
}

/// A transform with a unit Z along `facing`, X and Y rolled by `roll` radians about it
/// and scaled by `size`, centred on `at`. `facing` must be a unit vector.
fn surface_transform(at: Vec3, facing: Vec3, roll: f32, size: f32) -> Affine3A {
    let base_x = facing.any_orthonormal_vector();
    let base_y = facing.cross(base_x);
    let (sin, cos) = roll.sin_cos();
    let x = base_x * cos + base_y * sin;
    let y = base_y * cos - base_x * sin;
    Affine3A::from_cols(
        (x * size).into(),
        (y * size).into(),
        facing.into(),
        at.into(),
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
            self.frame_with_meshes(eye)
                .into_iter()
                .map(|(_, instance)| instance)
                .collect()
        }

        /// Like `frame`, with each instance's backend mesh handle (index into
        /// `backend.registered` plus one).
        fn frame_with_meshes(&mut self, eye: Vec3) -> Vec<(u32, InstanceGpu)> {
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
    fn each_slot_takes_the_next_reserved_mesh_id_in_order() {
        let ids = [Slot::Streak, Slot::Flash, Slot::Mark].map(Slot::id);

        assert_eq!(
            ids,
            [
                WorldMeshId(EFFECT_MESH_IDS_FROM),
                WorldMeshId(EFFECT_MESH_IDS_FROM + 1),
                WorldMeshId(EFFECT_MESH_IDS_FROM + 2),
            ]
        );
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

    fn flash_style(size: f32, lifetime_ms: u64) -> FlashStyle {
        FlashStyle {
            size,
            material: 0,
            lifetime: ms(lifetime_ms),
        }
    }

    fn mark_style(size: f32) -> MarkStyle {
        MarkStyle {
            size,
            material: 0,
            lifetime: Duration::from_secs(10),
            fade: Duration::from_secs(1),
        }
    }

    fn assert_triangles_ccw_toward_plus_z(positions: &[[f32; 3]], indices: &[u32]) {
        assert!(!indices.is_empty() && indices.len().is_multiple_of(3));
        for triangle in indices.chunks(3) {
            let [a, b, c] = [0, 1, 2].map(|i| Vec3::from(positions[triangle[i] as usize]));
            assert!(
                (b - a).cross(c - a).z > 0.0,
                "triangle {triangle:?} is counter-clockwise seen from +Z"
            );
        }
    }

    fn centroid(positions: &[[f32; 3]]) -> Vec3 {
        positions.iter().map(|p| Vec3::from(*p)).sum::<Vec3>() / positions.len() as f32
    }

    #[test]
    fn a_flash_shrinks_linearly_and_expires() {
        for lifetime_ms in [50, 200] {
            let mut rig = Rig::new();
            let at = Vec3::new(1.0, 2.0, 3.0);
            rig.effects.flash(at, 0.0, flash_style(0.4, lifetime_ms));

            let full = rig.frame(EYE);
            rig.effects.advance(ms(lifetime_ms / 2));
            let half = rig.frame(EYE);
            rig.effects.advance(ms(lifetime_ms / 2));
            let gone = rig.frame(EYE);

            assert_eq!(full.len(), 1, "a flash draws without being placed");
            assert_near(translation(&full[0]), at);
            assert!((x_axis(&full[0]).length() - 0.4).abs() < EPS);
            assert!((y_axis(&full[0]).length() - 0.4).abs() < EPS);
            assert_eq!(half.len(), 1, "lifetime {lifetime_ms} ms");
            assert_near(translation(&half[0]), at);
            assert!((x_axis(&half[0]).length() - 0.2).abs() < EPS);
            assert!((y_axis(&half[0]).length() - 0.2).abs() < EPS);
            assert!(gone.is_empty(), "gone at {lifetime_ms} ms");
            assert_eq!(rig.effects.live(), 0);
        }
    }

    #[test]
    fn a_flash_carries_its_material() {
        let mut rig = Rig::new();
        let style = FlashStyle {
            material: 5,
            ..flash_style(0.4, 50)
        };

        rig.effects.flash(Vec3::ZERO, 0.0, style);
        let drawn = rig.frame(EYE);

        assert_eq!(drawn.len(), 1);
        assert_eq!(drawn[0].material, 5);
    }

    #[test]
    fn a_flash_faces_the_eye() {
        let mut rig = Rig::new();
        let at = Vec3::new(1.0, 2.0, 3.0);
        rig.effects.flash(at, 0.0, flash_style(0.4, 50));

        for eye in [EYE, Vec3::new(-7.0, 1.0, 20.0)] {
            let drawn = rig.frame(eye);

            assert_eq!(drawn.len(), 1);
            let toward_eye = (eye - at).normalize();
            assert_near(z_axis(&drawn[0]).normalize(), toward_eye);
            assert!(x_axis(&drawn[0]).dot(toward_eye).abs() < EPS);
            assert!(y_axis(&drawn[0]).dot(toward_eye).abs() < EPS);
            assert!(x_axis(&drawn[0]).dot(y_axis(&drawn[0])).abs() < EPS);
            assert!(
                x_axis(&drawn[0]).cross(y_axis(&drawn[0])).dot(toward_eye) > 0.0,
                "wound toward the eye"
            );
        }
    }

    #[test]
    fn a_flash_roll_turns_it_about_the_line_to_the_eye() {
        let at = Vec3::new(1.0, 2.0, 3.0);
        let mut drawn = Vec::new();
        for roll in [0.0, std::f32::consts::FRAC_PI_2] {
            let mut rig = Rig::new();
            rig.effects.flash(at, roll, flash_style(0.4, 50));
            drawn.push(rig.frame(EYE)[0]);
        }

        let toward_eye = (EYE - at).normalize();
        assert!(
            x_axis(&drawn[0]).dot(x_axis(&drawn[1])).abs() < EPS,
            "a quarter turn"
        );
        assert!((x_axis(&drawn[1]).length() - 0.4).abs() < EPS);
        assert_near(z_axis(&drawn[1]), toward_eye);
    }

    #[test]
    fn the_flash_mesh_is_a_registered_once_eight_vertex_star_in_the_xy_plane() {
        let mut rig = Rig::new();
        rig.effects.flash(Vec3::X, 0.0, flash_style(0.4, 50));
        rig.frame(EYE);
        rig.effects.flash(Vec3::Y, 0.0, flash_style(0.4, 50));
        rig.frame(EYE);

        assert_eq!(rig.backend.registered.len(), 1, "uploaded once");
        let (positions, indices) = &rig.backend.registered[0];
        assert_eq!(positions.len(), 8, "a four-point star");
        assert!(positions.iter().all(|p| p[2] == 0.0), "in the XY plane");
        assert_near(centroid(positions), Vec3::ZERO);
        let mut ring: Vec<(f32, f32)> = positions
            .iter()
            .map(|p| (p[1].atan2(p[0]), Vec3::from(*p).length()))
            .collect();
        ring.sort_by(|a, b| a.0.total_cmp(&b.0));
        let is_tip = |radius: f32| (radius - 0.5).abs() < EPS;
        assert_eq!(
            ring.iter().filter(|(_, r)| is_tip(*r)).count(),
            4,
            "four tips at radius 0.5, tip to tip one unit"
        );
        for (i, (_, radius)) in ring.iter().enumerate() {
            let (_, next) = ring[(i + 1) % ring.len()];
            assert!(
                is_tip(*radius) != is_tip(next),
                "tips alternate with inner points"
            );
            if !is_tip(*radius) {
                assert!(
                    *radius > EPS && *radius < 0.5 - EPS,
                    "an inner point sits inside the tips, got {radius}"
                );
            }
        }
        assert!(indices.iter().all(|i| (*i as usize) < positions.len()));
        assert_triangles_ccw_toward_plus_z(positions, indices);
    }

    #[test]
    fn a_mark_lies_along_its_normal_offset_by_the_mark_offset() {
        let mut rig = Rig::new();
        let at = Vec3::new(4.0, 1.0, -2.0);
        let normal = Vec3::new(1.0, 2.0, 2.0).normalize();

        rig.effects.mark(at, normal, 0.7, mark_style(0.2));
        let drawn = rig.frame(EYE);

        assert_eq!(drawn.len(), 1, "a mark draws without being placed");
        assert_near(translation(&drawn[0]), at + normal * 0.01);
        assert_near(z_axis(&drawn[0]), normal);
        assert!((x_axis(&drawn[0]).length() - 0.2).abs() < EPS);
        assert!((y_axis(&drawn[0]).length() - 0.2).abs() < EPS);
        assert!(x_axis(&drawn[0]).dot(normal).abs() < EPS);
        assert!(y_axis(&drawn[0]).dot(normal).abs() < EPS);
        assert!(x_axis(&drawn[0]).dot(y_axis(&drawn[0])).abs() < EPS);
        assert!(
            x_axis(&drawn[0]).cross(y_axis(&drawn[0])).dot(normal) > 0.0,
            "faces along the normal"
        );
    }

    #[test]
    fn a_mark_normalises_its_normal_and_carries_its_material() {
        let mut rig = Rig::new();
        let style = MarkStyle {
            material: 3,
            ..mark_style(0.2)
        };

        rig.effects
            .mark(Vec3::ZERO, Vec3::new(0.0, 5.0, 0.0), 0.0, style);
        let drawn = rig.frame(EYE);

        assert_eq!(drawn.len(), 1);
        assert_near(translation(&drawn[0]), Vec3::new(0.0, 0.01, 0.0));
        assert_near(z_axis(&drawn[0]), Vec3::Y);
        assert_eq!(drawn[0].material, 3);
    }

    #[test]
    fn a_mark_roll_turns_it_about_its_normal() {
        let mut drawn = Vec::new();
        for roll in [0.0, std::f32::consts::FRAC_PI_2] {
            let mut rig = Rig::new();
            rig.effects.mark(Vec3::ZERO, Vec3::Y, roll, mark_style(0.2));
            drawn.push(rig.frame(EYE)[0]);
        }

        assert!(
            x_axis(&drawn[0]).dot(x_axis(&drawn[1])).abs() < EPS,
            "a quarter turn"
        );
        assert_near(translation(&drawn[0]), translation(&drawn[1]));
        assert_near(z_axis(&drawn[1]), Vec3::Y);
    }

    #[test]
    fn the_mark_mesh_is_a_registered_once_centred_unit_quad_wound_toward_plus_z() {
        let mut rig = Rig::new();
        rig.effects.mark(Vec3::X, Vec3::Y, 0.0, mark_style(0.2));
        rig.frame(EYE);
        rig.effects.mark(Vec3::Y, Vec3::Y, 0.0, mark_style(0.2));
        rig.frame(EYE);

        assert_eq!(rig.backend.registered.len(), 1, "uploaded once");
        let (positions, indices) = &rig.backend.registered[0];
        assert_eq!(positions.len(), 4);
        assert!(positions.iter().all(|p| p[2] == 0.0), "in the XY plane");
        assert_near(centroid(positions), Vec3::ZERO);
        for axis in 0..2 {
            let lo = positions.iter().map(|p| p[axis]).fold(f32::MAX, f32::min);
            let hi = positions.iter().map(|p| p[axis]).fold(f32::MIN, f32::max);
            assert!((hi - lo - 1.0).abs() < EPS, "one unit across axis {axis}");
        }
        assert_eq!(indices.len(), 6, "two triangles");
        assert_triangles_ccw_toward_plus_z(positions, indices);
    }

    #[test]
    fn streaks_flashes_and_marks_use_three_distinct_meshes() {
        let mut rig = Rig::new();
        let streak = rig.effects.begin_streak(style(2.0, 0.1));
        rig.effects.place_streak(streak, Vec3::X, Vec3::X);
        rig.effects.flash(Vec3::Y, 0.0, flash_style(0.4, 50));
        rig.effects.mark(Vec3::Z, Vec3::Y, 0.0, mark_style(0.2));

        let drawn = rig.frame_with_meshes(EYE);

        assert_eq!(drawn.len(), 3);
        assert_eq!(rig.backend.registered.len(), 3, "no mesh id is shared");
        let mesh_at = |p: Vec3| {
            let (handle, _) = drawn
                .iter()
                .find(|(_, d)| (translation(d) - p).length() < EPS)
                .unwrap_or_else(|| panic!("no instance at {p}"));
            (*handle, &rig.backend.registered[*handle as usize - 1].0)
        };
        let (streak_handle, streak_mesh_positions) = mesh_at(Vec3::new(-1.0, 0.0, 0.0));
        let (flash_handle, flash_mesh_positions) = mesh_at(Vec3::Y);
        let (mark_handle, mark_mesh_positions) = mesh_at(Vec3::new(0.0, 0.01, 1.0));
        assert!(streak_handle != flash_handle && flash_handle != mark_handle);
        assert_ne!(streak_handle, mark_handle);
        assert_eq!(
            streak_mesh_positions,
            &vec![
                [0.0, 0.0, 0.0],
                [0.8, -0.5, 0.0],
                [1.0, 0.0, 0.0],
                [0.8, 0.5, 0.0]
            ],
            "the streak draws its kite"
        );
        assert_eq!(flash_mesh_positions.len(), 8, "the flash draws the star");
        assert_eq!(mark_mesh_positions.len(), 4, "the mark draws the quad");
        assert_near(centroid(mark_mesh_positions), Vec3::ZERO);
    }

    #[test]
    fn a_mark_holds_full_size_then_shrinks_out_in_its_fade() {
        // (lifetime ms, fade ms): full when the fade starts, half midway, gone at the lifetime.
        for (lifetime_ms, fade_ms) in [(10_000u64, 1_000u64), (4_000, 2_000)] {
            let full_at = lifetime_ms - fade_ms;
            let half_at = lifetime_ms - fade_ms / 2;
            let mut rig = Rig::new();
            let style = MarkStyle {
                lifetime: ms(lifetime_ms),
                fade: ms(fade_ms),
                ..mark_style(0.2)
            };
            rig.effects.mark(Vec3::ZERO, Vec3::Y, 0.0, style);

            rig.effects.advance(ms(full_at));
            let full = rig.frame(EYE);
            rig.effects.advance(ms(half_at - full_at));
            let half = rig.frame(EYE);
            rig.effects.advance(ms(lifetime_ms - half_at));
            let gone = rig.frame(EYE);

            assert_eq!(full.len(), 1, "lifetime {lifetime_ms} ms");
            assert!(
                (x_axis(&full[0]).length() - 0.2).abs() < EPS,
                "full at {full_at} ms of {lifetime_ms}"
            );
            assert_eq!(half.len(), 1, "lifetime {lifetime_ms} ms");
            assert!(
                (x_axis(&half[0]).length() - 0.1).abs() < EPS,
                "half at {half_at} ms of {lifetime_ms}"
            );
            assert!((y_axis(&half[0]).length() - 0.1).abs() < EPS);
            assert_near(translation(&half[0]), translation(&full[0]));
            assert!(gone.is_empty(), "gone at {lifetime_ms} ms");
            assert_eq!(rig.effects.live(), 0);
        }
    }

    #[derive(Debug, Clone, Copy)]
    enum Kind {
        Flash,
        Mark,
    }

    /// Spawn a long-lived effect of `kind` at `at`; a mark lies on a +Z surface.
    fn spawn(effects: &mut Effects, kind: Kind, at: Vec3) {
        match kind {
            Kind::Flash => effects.flash(at, 0.0, flash_style(0.4, 500)),
            Kind::Mark => effects.mark(at, Vec3::Z, 0.0, mark_style(0.2)),
        }
    }

    /// Where an effect of `kind` spawned at `at` draws.
    fn drawn_position(kind: Kind, at: Vec3) -> Vec3 {
        match kind {
            Kind::Flash => at,
            Kind::Mark => at + Vec3::Z * 0.01,
        }
    }

    #[test]
    fn a_full_cap_drops_the_oldest_fixed_lifetime_effect() {
        let cases = [
            (Kind::Mark, Kind::Mark),
            (Kind::Flash, Kind::Mark),
            (Kind::Mark, Kind::Flash),
            (Kind::Flash, Kind::Flash),
        ];
        for (older, newer) in cases {
            let mut rig = Rig::new();
            rig.effects.set_quality(1);
            let streak = rig.effects.begin_streak(style(2.0, 0.1));
            let oldest = Vec3::new(10.0, 0.0, 0.0);
            spawn(&mut rig.effects, older, oldest);
            for i in 1..31 {
                spawn(
                    &mut rig.effects,
                    newer,
                    Vec3::new(10.0 + i as f32, 0.0, 0.0),
                );
            }
            assert_eq!(rig.effects.live(), 32, "{older:?} then {newer:?}");

            let newest = Vec3::new(100.0, 0.0, 0.0);
            spawn(&mut rig.effects, newer, newest);
            rig.effects
                .place_streak(streak, Vec3::new(0.0, 50.0, 0.0), Vec3::X);
            let drawn = rig.frame(EYE);

            assert_eq!(
                rig.effects.live(),
                32,
                "still at the cap: {older:?} then {newer:?}"
            );
            assert_eq!(drawn.len(), 32, "{older:?} then {newer:?}");
            let drawn_near = |p: Vec3| drawn.iter().any(|d| (translation(d) - p).length() < EPS);
            assert!(
                !drawn_near(drawn_position(older, oldest)),
                "the oldest {older:?} was dropped for a new {newer:?}"
            );
            assert!(
                drawn_near(drawn_position(newer, Vec3::new(11.0, 0.0, 0.0))),
                "the next oldest stays"
            );
            assert!(
                drawn_near(drawn_position(newer, newest)),
                "the new {newer:?} is live"
            );
            assert!(drawn_near(Vec3::new(-2.0, 50.0, 0.0)), "the streak stays");
        }
    }

    #[test]
    fn a_full_cap_of_streaks_refuses_a_mark_and_keeps_every_streak() {
        let mut rig = Rig::new();
        rig.effects.set_quality(1);
        let handles: Vec<EffectHandle> = (0..32)
            .map(|_| rig.effects.begin_streak(style(2.0, 0.1)))
            .collect();

        rig.effects
            .mark(Vec3::new(5.0, 5.0, 5.0), Vec3::Z, 0.0, mark_style(0.2));
        for handle in &handles {
            rig.effects.place_streak(*handle, Vec3::X, Vec3::X);
        }
        let drawn = rig.frame(EYE);

        assert_eq!(rig.effects.live(), 32);
        assert_eq!(drawn.len(), 32, "every streak stays and no mark joins");
        assert!(
            drawn
                .iter()
                .all(|d| (translation(d) - Vec3::new(5.0, 5.0, 5.01)).length() > 1.0)
        );
    }

    #[test]
    fn a_full_cap_of_streaks_refuses_a_flash_and_keeps_every_streak() {
        let mut rig = Rig::new();
        rig.effects.set_quality(1);
        let handles: Vec<EffectHandle> = (0..32)
            .map(|_| rig.effects.begin_streak(style(2.0, 0.1)))
            .collect();

        rig.effects
            .flash(Vec3::new(5.0, 5.0, 5.0), 0.0, flash_style(0.4, 50));
        for handle in &handles {
            rig.effects.place_streak(*handle, Vec3::X, Vec3::X);
        }
        let drawn = rig.frame(EYE);

        assert_eq!(rig.effects.live(), 32);
        assert_eq!(drawn.len(), 32);
        assert!(
            drawn
                .iter()
                .all(|d| (translation(d) - Vec3::new(5.0, 5.0, 5.0)).length() > 1.0),
            "no flash joins"
        );
        assert!(
            drawn
                .iter()
                .all(|d| (translation(d) - Vec3::new(-1.0, 0.0, 0.0)).length() < EPS),
            "every streak is drawn at its own position"
        );
    }

    #[test]
    fn quality_zero_refuses_flash_and_mark() {
        let mut rig = Rig::new();
        rig.effects.set_quality(0);

        rig.effects.flash(Vec3::X, 0.0, flash_style(0.4, 50));
        rig.effects.mark(Vec3::Y, Vec3::Y, 0.0, mark_style(0.2));

        assert_eq!(rig.effects.live(), 0);
        assert!(rig.frame(EYE).is_empty());
    }

    #[test]
    fn the_same_spawns_draw_the_same_instances() {
        let run = || {
            let mut rig = Rig::new();
            rig.effects
                .flash(Vec3::new(1.0, 2.0, 3.0), 0.3, flash_style(0.4, 50));
            rig.effects.mark(
                Vec3::new(4.0, 5.0, 6.0),
                Vec3::new(1.0, 1.0, 0.0),
                1.1,
                mark_style(0.2),
            );
            rig.effects.advance(ms(20));
            rig.frame(EYE)
                .iter()
                .map(|i| (i.model, i.material))
                .collect::<Vec<_>>()
        };

        let first = run();
        let second = run();

        assert_eq!(first.len(), 2);
        assert_eq!(first, second);
    }

    #[test]
    fn a_flash_aged_twenty_milliseconds_of_fifty_draws_at_sixty_percent() {
        let mut rig = Rig::new();
        rig.effects
            .flash(Vec3::new(1.0, 2.0, 3.0), 0.3, flash_style(0.4, 50));

        rig.effects.advance(ms(20));
        let drawn = rig.frame(EYE);

        assert_eq!(drawn.len(), 1);
        assert!((x_axis(&drawn[0]).length() - 0.24).abs() < EPS);
    }

    #[test]
    fn a_mark_with_a_zero_or_non_finite_normal_is_refused() {
        let normals = [
            Vec3::ZERO,
            Vec3::new(f32::NAN, 1.0, 0.0),
            Vec3::new(0.0, f32::INFINITY, 0.0),
            Vec3::new(0.0, 1.0, f32::NEG_INFINITY),
        ];
        for normal in normals {
            let mut rig = Rig::new();

            rig.effects.mark(Vec3::X, normal, 0.0, mark_style(0.2));

            assert_eq!(rig.effects.live(), 0, "normal {normal}");
            assert!(rig.frame(EYE).is_empty(), "normal {normal}");
        }
    }

    #[test]
    fn a_flash_or_mark_at_a_non_finite_point_is_refused() {
        let points = [
            Vec3::new(f32::NAN, 0.0, 0.0),
            Vec3::new(0.0, f32::INFINITY, 0.0),
            Vec3::splat(f32::NEG_INFINITY),
        ];
        for at in points {
            let mut rig = Rig::new();

            rig.effects.flash(at, 0.0, flash_style(0.4, 50));
            assert_eq!(rig.effects.live(), 0, "flash at {at}");
            rig.effects.mark(at, Vec3::Y, 0.0, mark_style(0.2));

            assert_eq!(rig.effects.live(), 0, "mark at {at}");
            assert!(rig.frame(EYE).is_empty(), "at {at}");
        }
    }

    fn any_normal() -> impl Strategy<Value = Vec3> {
        prop_oneof![
            Just(Vec3::X),
            Just(Vec3::NEG_X),
            Just(Vec3::Y),
            Just(Vec3::NEG_Y),
            Just(Vec3::Z),
            Just(Vec3::NEG_Z),
            (-1.0f32..1.0, -1.0f32..1.0, -1.0f32..1.0)
                .prop_map(|(x, y, z)| Vec3::new(x, y, z))
                .prop_filter("non-degenerate", |v| v.length() > 0.1),
        ]
    }

    proptest! {
        #[test]
        fn a_mark_basis_is_finite_for_any_normal(
            normal in any_normal(),
            roll in -7.0f32..7.0,
            size in 0.01f32..2.0,
        ) {
            let mut rig = Rig::new();

            let at = Vec3::new(1.0, 2.0, 3.0);

            rig.effects.mark(at, normal, roll, mark_style(size));
            let drawn = rig.frame(EYE);

            prop_assert_eq!(drawn.len(), 1);
            let model = drawn[0].model;
            prop_assert!(model.iter().flatten().all(|v| v.is_finite()));
            let unit = normal.normalize();
            prop_assert!((translation(&drawn[0]) - (at + unit * 0.01)).length() < 1e-3);
            prop_assert!(y_axis(&drawn[0]).dot(unit).abs() < 1e-3);
            prop_assert!((z_axis(&drawn[0]) - unit).length() < 1e-3);
            prop_assert!((x_axis(&drawn[0]).length() - size).abs() < 1e-3);
            prop_assert!((y_axis(&drawn[0]).length() - size).abs() < 1e-3);
            prop_assert!(x_axis(&drawn[0]).dot(y_axis(&drawn[0])).abs() < 1e-3);
            prop_assert!(x_axis(&drawn[0]).dot(unit).abs() < 1e-3);
            prop_assert!(x_axis(&drawn[0]).cross(y_axis(&drawn[0])).dot(unit) > 0.0);
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

    #[test]
    fn a_flash_or_mark_with_a_zero_or_negative_size_is_refused() {
        for size in [0.0, -0.4] {
            let mut rig = Rig::new();

            rig.effects.flash(Vec3::X, 0.0, flash_style(size, 50));
            assert_eq!(rig.effects.live(), 0, "flash of size {size}");
            rig.effects.mark(Vec3::Y, Vec3::Y, 0.0, mark_style(size));

            assert_eq!(rig.effects.live(), 0, "mark of size {size}");
            assert!(rig.frame(EYE).is_empty(), "size {size}");
        }
    }

    #[test]
    fn a_refused_zero_size_spawn_at_a_full_cap_evicts_nothing() {
        for (older, refused) in [(Kind::Mark, Kind::Mark), (Kind::Flash, Kind::Flash)] {
            let mut rig = Rig::new();
            rig.effects.set_quality(1);
            let oldest = Vec3::new(10.0, 0.0, 0.0);
            for i in 0..32 {
                spawn(
                    &mut rig.effects,
                    older,
                    Vec3::new(10.0 + i as f32, 0.0, 0.0),
                );
            }
            assert_eq!(rig.effects.live(), 32);

            match refused {
                Kind::Flash => {
                    rig.effects
                        .flash(Vec3::new(100.0, 0.0, 0.0), 0.0, flash_style(0.0, 500));
                }
                Kind::Mark => {
                    rig.effects
                        .mark(Vec3::new(100.0, 0.0, 0.0), Vec3::Z, 0.0, mark_style(0.0));
                }
            }
            let drawn = rig.frame(EYE);

            assert_eq!(rig.effects.live(), 32, "{refused:?} refused");
            assert_eq!(drawn.len(), 32, "{refused:?} refused");
            assert!(
                drawn
                    .iter()
                    .any(|d| (translation(d) - drawn_position(older, oldest)).length() < EPS),
                "the oldest {older:?} survives a refused {refused:?}"
            );
        }
    }

    #[test]
    fn a_mark_over_a_lowered_cap_is_refused_and_the_live_ones_stay() {
        let mut rig = Rig::new();
        for i in 0..20 {
            let at = Vec3::new(i as f32, 0.0, 0.0);
            spawn(&mut rig.effects, Kind::Flash, at);
            spawn(&mut rig.effects, Kind::Mark, at + Vec3::Y * 5.0);
        }
        assert_eq!(rig.effects.live(), 40);

        rig.effects.set_quality(1);
        rig.effects
            .mark(Vec3::new(100.0, 0.0, 0.0), Vec3::Z, 0.0, mark_style(0.2));
        let drawn = rig.frame(EYE);

        assert_eq!(rig.effects.live(), 40, "none dropped, none added");
        assert_eq!(drawn.len(), 40);
        assert!(
            drawn
                .iter()
                .all(|d| (translation(d) - Vec3::new(100.0, 0.0, 0.01)).length() > 1.0),
            "the new mark is not drawn"
        );
    }
}
