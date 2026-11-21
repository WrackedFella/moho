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
    /// Tip-to-centre distance at spawn.
    pub size: f32,
    pub material: u32,
    pub lifetime: Duration,
}

/// A flat square lying on a surface: full size until `lifetime - fade`, then shrinking out.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MarkStyle {
    /// Side length at full size.
    pub size: f32,
    pub material: u32,
    pub lifetime: Duration,
    pub fade: Duration,
}

/// How far a mark floats off its surface, so it does not z-fight with it.
const MARK_OFFSET: f32 = 0.01;

/// Radius of a star's inner points, as a fraction of its tip radius.
const STAR_INNER_RADIUS: f32 = 0.4;

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

    /// Age by `dt`, starting the fade once past `max_age`.
    fn advance(&mut self, dt: Duration) {
        self.age += dt;
        if self.age >= self.style.max_age {
            self.ended_at.get_or_insert(self.style.max_age);
        }
    }

    /// The unshadowed placement for this frame, if the streak is placed or fading.
    fn drawn(&mut self, eye: Vec3) -> Option<Drawn> {
        let placed = std::mem::take(&mut self.placed_this_frame);
        if !placed && self.ended_at.is_none() {
            return None;
        }
        let width = self.style.width * self.width_scale();
        let transform = streak_transform(self.head, self.heading?, self.style.length, width, eye)?;
        Some(Drawn {
            slot: MeshSlot::Streak,
            transform,
            material: self.style.material,
        })
    }
}

/// The size factor at `age` of an effect that lasts `lifetime`: 1 until `lifetime - fade`,
/// then falling linearly to 0 at the lifetime. A fade longer than the lifetime is the whole
/// lifetime; a zero fade holds full size until the lifetime, then is gone.
fn fade_scale(age: Duration, lifetime: Duration, fade: Duration) -> f32 {
    if age >= lifetime {
        return 0.0;
    }
    let fade = fade.min(lifetime);
    // A zero fade always lands here (age < lifetime), so the division below is never by zero.
    if age <= lifetime.saturating_sub(fade) {
        return 1.0;
    }
    let left = lifetime.saturating_sub(age);
    (left.as_secs_f32() / fade.as_secs_f32()).clamp(0.0, 1.0)
}

#[derive(Debug)]
struct Flash {
    at: Vec3,
    roll: f32,
    style: FlashStyle,
    age: Duration,
}

impl Flash {
    fn drawn(&self, eye: Vec3) -> Option<Drawn> {
        let lifetime = self.style.lifetime;
        let size = self.style.size * fade_scale(self.age, lifetime, lifetime);
        Some(Drawn {
            slot: MeshSlot::Star,
            transform: flash_transform(self.at, self.roll, size, eye)?,
            material: self.style.material,
        })
    }
}

#[derive(Debug)]
struct Mark {
    at: Vec3,
    normal: Vec3,
    roll: f32,
    style: MarkStyle,
    age: Duration,
}

impl Mark {
    fn drawn(&self) -> Option<Drawn> {
        let scale = fade_scale(self.age, self.style.lifetime, self.style.fade);
        let size = self.style.size * scale;
        Some(Drawn {
            slot: MeshSlot::Quad,
            transform: mark_transform(self.at, self.normal, self.roll, size)?,
            material: self.style.material,
        })
    }
}

/// One live effect; the kinds share a handle space, the cap and the clock.
#[derive(Debug)]
enum Effect {
    Streak(Streak),
    Flash(Flash),
    Mark(Mark),
}

impl Effect {
    /// Flashes and marks end on their own at a known time, so they can make room for new ones.
    fn is_fixed_lifetime(&self) -> bool {
        !matches!(self, Self::Streak(_))
    }

    fn advance(&mut self, dt: Duration) {
        match self {
            Self::Streak(streak) => streak.advance(dt),
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

    fn drawn(&mut self, eye: Vec3) -> Option<Drawn> {
        match self {
            Self::Streak(streak) => streak.drawn(eye),
            Self::Flash(flash) => flash.drawn(eye),
            Self::Mark(mark) => mark.drawn(),
        }
    }
}

/// The built-in meshes effects draw, each registered once under the id after the last.
#[derive(Debug, Clone, Copy)]
enum MeshSlot {
    Streak,
    Star,
    Quad,
}

impl MeshSlot {
    const COUNT: usize = 3;

    fn id(self) -> WorldMeshId {
        WorldMeshId(EFFECT_MESH_IDS_FROM + self as u64)
    }

    fn mesh(self) -> WorldMesh {
        match self {
            Self::Streak => streak_mesh(),
            Self::Star => star_mesh(),
            Self::Quad => quad_mesh(),
        }
    }
}

/// One effect's placement for the frame.
struct Drawn {
    slot: MeshSlot,
    transform: Affine3A,
    material: u32,
}

#[derive(Debug)]
pub struct Effects {
    store: BTreeMap<EffectHandle, Effect>,
    next_handle: u64,
    cap: usize,
    uploaded: [bool; MeshSlot::COUNT],
}

impl Default for Effects {
    fn default() -> Self {
        Self {
            store: BTreeMap::new(),
            next_handle: 0,
            cap: CAPS[DEFAULT_QUALITY],
            uploaded: [false; MeshSlot::COUNT],
        }
    }
}

impl Effects {
    /// Start a streak. At the quality's cap the handle is inert: never stored, so placing
    /// or ending it does nothing.
    pub fn begin_streak(&mut self, style: StreakStyle) -> EffectHandle {
        let handle = EffectHandle(self.next_handle);
        self.next_handle += 1;
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

    /// Spawn a camera-facing flash at `at`, rolled by `roll` about the line of sight, that
    /// shrinks to nothing over its lifetime. A zero lifetime spawns nothing. At the cap the
    /// oldest flash or mark makes room; with none to drop the flash is refused.
    pub fn flash(&mut self, at: Vec3, roll: f32, style: FlashStyle) {
        if style.lifetime.is_zero() || !is_placeable(&[at], roll, style.size) {
            return;
        }
        self.spawn(Effect::Flash(Flash {
            at,
            roll,
            style,
            age: Duration::ZERO,
        }));
    }

    /// Spawn a flat mark at `at` lying on a surface with `normal`, rolled by `roll` about
    /// it. A zero lifetime spawns nothing; the cap applies as for [`Effects::flash`].
    pub fn mark(&mut self, at: Vec3, normal: Vec3, roll: f32, style: MarkStyle) {
        let Some(normal) = normal.try_normalize() else {
            return;
        };
        if style.lifetime.is_zero() || !is_placeable(&[at], roll, style.size) {
            return;
        }
        self.spawn(Effect::Mark(Mark {
            at,
            normal,
            roll,
            style,
            age: Duration::ZERO,
        }));
    }

    /// Store a fixed-lifetime effect, first dropping the one oldest such effect when at or
    /// over the cap; streaks are never dropped, so with none to drop it is refused.
    fn spawn(&mut self, effect: Effect) {
        if self.store.len() >= self.cap {
            let oldest = self
                .store
                .iter()
                .find_map(|(handle, live)| live.is_fixed_lifetime().then_some(*handle));
            let Some(oldest) = oldest else {
                return;
            };
            self.store.remove(&oldest);
        }
        let handle = EffectHandle(self.next_handle);
        self.next_handle += 1;
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

    /// Age every effect by the frame time and drop the finished ones; a streak past its
    /// `max_age` starts its fade first.
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

    /// Place every effect that draws this frame, unshadowed: streaks placed or fading,
    /// flashes and marks always, each facing `eye` where it faces the camera.
    pub(crate) fn draw(&mut self, meshes: &mut WorldMeshes, eye: Vec3) {
        for effect in self.store.values_mut() {
            let Some(drawn) = effect.drawn(eye) else {
                continue;
            };
            if !std::mem::replace(&mut self.uploaded[drawn.slot as usize], true) {
                meshes.upsert_movable(drawn.slot.id(), drawn.slot.mesh());
            }
            meshes.place_unshadowed(drawn.slot.id(), drawn.transform, drawn.material);
        }
    }
}

/// A flat, unlit mesh in the XY plane facing +Z; `indices` must wind counter-clockwise.
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

/// A four-point star about the origin: tips of radius 1 on the axes at the even vertices,
/// inner points between them at the odd ones, counter-clockwise seen from +Z.
fn star_mesh() -> WorldMesh {
    let positions = (0..8_u8)
        .map(|i| {
            let angle = f32::from(i) * std::f32::consts::FRAC_PI_4;
            let radius = if i % 2 == 0 { 1.0 } else { STAR_INNER_RADIUS };
            [radius * angle.cos(), radius * angle.sin(), 0.0]
        })
        .collect();
    // The square of inner points, then one triangle on each tip.
    let mut indices = vec![1, 3, 5, 1, 5, 7];
    for tip in [0, 2, 4, 6] {
        indices.extend([(tip + 7) % 8, tip, (tip + 1) % 8]);
    }
    flat_mesh(positions, indices)
}

/// A unit square about the origin, counter-clockwise seen from +Z.
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

/// A frame at `pos` facing the unit `z`: X and Y scaled by `size` and turned by `roll`
/// about `z`.
fn oriented(z: Vec3, roll: f32, size: f32, pos: Vec3) -> Affine3A {
    let x = z.any_orthonormal_vector();
    let y = z.cross(x);
    let (sin, cos) = roll.sin_cos();
    Affine3A::from_cols(
        ((x * cos + y * sin) * size).into(),
        ((y * cos - x * sin) * size).into(),
        z.into(),
        pos.into(),
    )
}

/// Whether a flash or mark with these points, `roll` and `size` can ever be drawn: all
/// finite and `size` positive.
fn is_placeable(points: &[Vec3], roll: f32, size: f32) -> bool {
    points.iter().all(|point| point.is_finite())
        && roll.is_finite()
        && size.is_finite()
        && size > 0.0
}

/// Where a flash of `size` at `at` is drawn: facing `eye`, rolled by `roll`. `None` for
/// non-finite input or no size left.
fn flash_transform(at: Vec3, roll: f32, size: f32, eye: Vec3) -> Option<Affine3A> {
    if !is_placeable(&[at, eye], roll, size) {
        return None;
    }
    let z = (eye - at).try_normalize().unwrap_or(Vec3::Z);
    Some(oriented(z, roll, size, at))
}

/// Where a mark of `size` on a surface with unit `normal` is drawn: lifted off it by
/// `MARK_OFFSET` and rolled by `roll` about it. `None` for non-finite input or no size left.
fn mark_transform(at: Vec3, normal: Vec3, roll: f32, size: f32) -> Option<Affine3A> {
    if !is_placeable(&[at], roll, size) {
        return None;
    }
    Some(oriented(normal, roll, size, at + normal * MARK_OFFSET))
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

    fn z_axis(instance: &InstanceGpu) -> Vec3 {
        let [x, y, z, _] = instance.model[2];
        Vec3::new(x, y, z)
    }

    /// One frame's unshadowed draws with the backend mesh handle each was drawn with.
    fn frame_with_meshes(rig: &mut Rig, eye: Vec3) -> Vec<(u32, InstanceGpu)> {
        rig.effects.draw(&mut rig.meshes, eye);
        rig.meshes.flush(&mut rig.backend);
        let drawn = rig.meshes.unshadowed_draws().collect();
        rig.meshes.clear_placements();
        drawn
    }

    /// The drawn instance's matrix and material, comparable across runs.
    fn signature(instance: &InstanceGpu) -> ([[f32; 4]; 4], u32) {
        (instance.model, instance.material)
    }

    #[test]
    fn a_flash_shrinks_linearly_and_expires() {
        let mut rig = Rig::new();
        let at = Vec3::new(2.0, 1.0, -3.0);
        rig.effects.flash(at, 0.0, flash_style(0.4, ms(50)));

        let full = rig.frame(EYE);
        rig.effects.advance(ms(25));
        let half = rig.frame(EYE);

        assert_eq!(full.len(), 1);
        assert_near(translation(&full[0]), at);
        assert!((x_axis(&full[0]).length() - 0.4).abs() < EPS);
        assert!((y_axis(&full[0]).length() - 0.4).abs() < EPS);
        assert_eq!(rig.effects.live(), 1);
        assert_eq!(half.len(), 1);
        assert_near(translation(&half[0]), at);
        assert!(
            (x_axis(&half[0]).length() - 0.2).abs() < EPS,
            "half size, got {}",
            x_axis(&half[0]).length()
        );
        assert!((y_axis(&half[0]).length() - 0.2).abs() < EPS);

        rig.effects.advance(ms(25));
        assert!(rig.frame(EYE).is_empty());
        assert_eq!(rig.effects.live(), 0);
    }

    #[test]
    fn a_flash_carries_its_material() {
        let mut rig = Rig::new();
        let style = FlashStyle {
            material: 9,
            ..flash_style(0.4, ms(50))
        };

        rig.effects.flash(Vec3::ZERO, 0.0, style);
        let drawn = rig.frame(EYE);

        assert_eq!(drawn.len(), 1);
        assert_eq!(drawn[0].material, 9);
    }

    #[test]
    fn a_flash_faces_the_eye() {
        let at = Vec3::new(1.0, 2.0, 3.0);
        for eye in [EYE, Vec3::new(-20.0, 5.0, 0.0), at + Vec3::Y * 4.0] {
            let mut rig = Rig::new();
            rig.effects.flash(at, 0.0, flash_style(0.4, ms(50)));

            let drawn = rig.frame(eye);

            assert_eq!(drawn.len(), 1);
            let (x, y, z) = (x_axis(&drawn[0]), y_axis(&drawn[0]), z_axis(&drawn[0]));
            let to_eye = (eye - at).normalize();
            assert!(
                x.cross(y).normalize().dot(to_eye) > 1.0 - EPS,
                "X x Y points at the eye"
            );
            assert!(x.dot(y).abs() < EPS, "square on");
            assert!(x.dot(to_eye).abs() < EPS && y.dot(to_eye).abs() < EPS);
            assert!(z.normalize().dot(to_eye) > 1.0 - EPS, "Z points at the eye");
        }
    }

    #[test]
    fn a_flash_roll_turns_it_about_the_line_of_sight() {
        let at = Vec3::new(1.0, 2.0, 3.0);
        let roll = 0.7_f32;
        let mut plain = Rig::new();
        plain.effects.flash(at, 0.0, flash_style(0.4, ms(50)));
        let mut rolled = Rig::new();
        rolled.effects.flash(at, roll, flash_style(0.4, ms(50)));

        let plain = plain.frame(EYE);
        let rolled = rolled.frame(EYE);

        let a = x_axis(&plain[0]).normalize();
        let b = x_axis(&rolled[0]).normalize();
        assert!(
            (a.dot(b) - roll.cos()).abs() < EPS,
            "X turned by the roll, dot {}",
            a.dot(b)
        );
        let to_eye = (EYE - at).normalize();
        assert!(b.dot(to_eye).abs() < EPS, "still across the line of sight");
        assert!((x_axis(&rolled[0]).length() - 0.4).abs() < EPS);
    }

    #[test]
    fn a_flash_with_zero_lifetime_never_draws() {
        let mut rig = Rig::new();

        rig.effects
            .flash(Vec3::X, 0.0, flash_style(0.4, Duration::ZERO));

        assert!(rig.frame(EYE).is_empty());
        rig.effects.advance(ms(1));
        assert_eq!(rig.effects.live(), 0);
    }

    #[test]
    fn the_flash_star_is_registered_once_as_four_points_wound_toward_plus_z() {
        let mut rig = Rig::new();

        for x in [1.0, 2.0, 3.0] {
            rig.effects
                .flash(Vec3::X * x, 0.0, flash_style(0.4, ms(50)));
        }
        let drawn = frame_with_meshes(&mut rig, EYE);
        let again = frame_with_meshes(&mut rig, EYE);

        assert_eq!(drawn.len(), 3);
        assert_eq!(again.len(), 3);
        assert_eq!(
            rig.backend.registered.len(),
            1,
            "one star for every flash, across frames"
        );
        let (positions, indices) = &rig.backend.registered[0];
        assert_eq!(positions.len(), 8);
        let points: Vec<Vec3> = positions.iter().map(|p| Vec3::from(*p)).collect();
        assert_near(points.iter().copied().sum::<Vec3>() / 8.0, Vec3::ZERO);
        let radii: Vec<f32> = points.iter().map(|p| p.length()).collect();
        let outer = radii.iter().copied().fold(f32::MIN, f32::max);
        let inner = radii.iter().copied().fold(f32::MAX, f32::min);
        assert!(
            inner > 0.0 && inner < outer,
            "tips stand out from the waist"
        );
        let tips_first = (radii[0] - outer).abs() < EPS;
        for (i, radius) in radii.iter().enumerate() {
            let is_tip = (i % 2 == 0) == tips_first;
            let expected = if is_tip { outer } else { inner };
            assert!(
                (radius - expected).abs() < EPS,
                "vertex {i} alternates tip and inner, radius {radius}"
            );
        }
        assert_eq!(indices.len() % 3, 0);
        assert!(!indices.is_empty());
        for triangle in indices.chunks(3) {
            let [a, b, c] = [0, 1, 2].map(|i| Vec3::from(positions[triangle[i] as usize]));
            assert!(
                (b - a).cross(c - a).z > 0.0,
                "triangle {triangle:?} is counter-clockwise seen from +Z"
            );
            assert!(a.z == 0.0 && b.z == 0.0 && c.z == 0.0, "in the XY plane");
        }
    }

    #[test]
    fn a_mark_lies_along_its_normal_offset_by_the_mark_offset() {
        let at = Vec3::new(4.0, 0.0, -2.0);
        let normals = [
            Vec3::Y,
            Vec3::new(1.0, 1.0, 0.0).normalize(),
            Vec3::new(0.0, -0.6, 0.8),
        ];
        for normal in normals {
            let mut rig = Rig::new();
            rig.effects.mark(
                at,
                normal,
                0.0,
                mark_style(0.2, Duration::from_secs(10), Duration::from_secs(1)),
            );

            let drawn = rig.frame(EYE);

            assert_eq!(drawn.len(), 1);
            let (x, y) = (x_axis(&drawn[0]), y_axis(&drawn[0]));
            assert_near(translation(&drawn[0]), at + normal * 0.01);
            assert!((x.length() - 0.2).abs() < EPS, "0.2 across");
            assert!((y.length() - 0.2).abs() < EPS, "0.2 across");
            assert!(x.dot(y).abs() < EPS, "a square");
            assert!(x.dot(normal).abs() < EPS && y.dot(normal).abs() < EPS);
            assert!(
                x.cross(y).normalize().dot(normal) > 1.0 - EPS,
                "faces along the normal"
            );
            assert_near(z_axis(&drawn[0]), normal);
        }
    }

    #[test]
    fn a_mark_roll_turns_it_about_its_normal() {
        let normal = Vec3::new(0.0, 0.6, 0.8);
        let roll = 0.9_f32;
        let style = mark_style(0.2, Duration::from_secs(10), Duration::from_secs(1));
        let mut plain = Rig::new();
        plain.effects.mark(Vec3::ZERO, normal, 0.0, style);
        let mut rolled = Rig::new();
        rolled.effects.mark(Vec3::ZERO, normal, roll, style);

        let plain = plain.frame(EYE);
        let rolled = rolled.frame(EYE);

        let a = x_axis(&plain[0]).normalize();
        let b = x_axis(&rolled[0]).normalize();
        assert!((a.dot(b) - roll.cos()).abs() < EPS, "dot {}", a.dot(b));
        assert!(b.dot(normal).abs() < EPS, "still on the surface");
        assert!(a.dot(normal).abs() < EPS);
        assert_near(translation(&plain[0]), translation(&rolled[0]));
    }

    #[test]
    fn the_mark_quad_is_registered_once_as_a_unit_square_wound_toward_plus_z() {
        let mut rig = Rig::new();

        for x in [1.0, 2.0] {
            rig.effects.mark(
                Vec3::X * x,
                Vec3::Y,
                0.0,
                mark_style(0.2, Duration::from_secs(10), Duration::from_secs(1)),
            );
        }
        rig.effects.flash(Vec3::ZERO, 0.0, flash_style(0.4, ms(50)));
        let drawn = frame_with_meshes(&mut rig, EYE);
        let again = frame_with_meshes(&mut rig, EYE);

        assert_eq!(drawn.len(), 3);
        assert_eq!(again.len(), 3);
        assert_eq!(
            rig.backend.registered.len(),
            2,
            "one star and one quad, however many are live"
        );
        let quad = rig
            .backend
            .registered
            .iter()
            .find(|(positions, _)| positions.len() == 4)
            .expect("a four-vertex quad was registered");
        let (positions, indices) = quad;
        let mut area = 0.0;
        for triangle in indices.chunks(3) {
            let [a, b, c] = [0, 1, 2].map(|i| Vec3::from(positions[triangle[i] as usize]));
            let normal = (b - a).cross(c - a);
            assert!(normal.z > 0.0, "counter-clockwise seen from +Z");
            area += normal.z / 2.0;
        }
        assert!((area - 1.0).abs() < EPS, "a unit square, area {area}");
        let centre = positions.iter().map(|p| Vec3::from(*p)).sum::<Vec3>() / 4.0;
        assert_near(centre, Vec3::ZERO);
        for (sx, sy) in [(-0.5, -0.5), (-0.5, 0.5), (0.5, -0.5), (0.5, 0.5)] {
            assert!(
                positions
                    .iter()
                    .any(|p| (Vec3::from(*p) - Vec3::new(sx, sy, 0.0)).length() < EPS),
                "corner ({sx}, {sy}) is missing from {positions:?}"
            );
        }
    }

    #[test]
    fn the_flash_star_and_mark_quad_use_their_own_meshes_apart_from_the_streak() {
        let mut rig = Rig::new();
        let streak = rig.effects.begin_streak(style(2.0, 0.1));
        rig.effects.place_streak(streak, Vec3::X * 5.0, Vec3::X);
        rig.effects.flash(Vec3::Y, 0.0, flash_style(0.4, ms(50)));
        rig.effects.mark(
            Vec3::Z,
            Vec3::Y,
            0.0,
            mark_style(0.2, Duration::from_secs(10), Duration::from_secs(1)),
        );

        let drawn = frame_with_meshes(&mut rig, EYE);

        assert_eq!(drawn.len(), 3);
        let mut handles: Vec<u32> = drawn.iter().map(|(handle, _)| *handle).collect();
        handles.sort_unstable();
        handles.dedup();
        assert_eq!(handles.len(), 3, "a mesh each");
        assert_eq!(rig.backend.registered.len(), 3);
    }

    #[test]
    fn the_flash_star_and_mark_quad_hold_the_two_ids_after_the_streak_mesh() {
        let materials_drawn = |rig: &mut Rig| {
            rig.effects
                .place_streak(EffectHandle(0), Vec3::X * 5.0, Vec3::X);
            let mut materials: Vec<u32> = frame_with_meshes(rig, EYE)
                .iter()
                .map(|(_, instance)| instance.material)
                .collect();
            materials.sort_unstable();
            materials
        };
        let mut rig = Rig::new();
        rig.effects.begin_streak(StreakStyle::new(2.0, 0.1, 3));
        rig.effects.flash(
            Vec3::Y,
            0.0,
            FlashStyle {
                material: 1,
                ..flash_style(0.4, Duration::from_secs(10))
            },
        );
        rig.effects.mark(
            Vec3::Z,
            Vec3::Y,
            0.0,
            MarkStyle {
                material: 2,
                ..long_mark()
            },
        );
        assert_eq!(materials_drawn(&mut rig), vec![1, 2, 3]);

        rig.meshes.remove(WorldMeshId(EFFECT_MESH_IDS_FROM + 1));
        assert_eq!(materials_drawn(&mut rig), vec![2, 3], "the star is +1");

        rig.meshes.remove(WorldMeshId(EFFECT_MESH_IDS_FROM + 2));
        assert_eq!(materials_drawn(&mut rig), vec![3], "the quad is +2");
    }

    #[test]
    fn a_mark_holds_full_size_then_shrinks_out_in_its_fade() {
        let mut rig = Rig::new();
        let at = Vec3::new(1.0, 0.0, 1.0);
        rig.effects.mark(
            at,
            Vec3::Y,
            0.0,
            mark_style(0.2, Duration::from_secs(10), Duration::from_secs(1)),
        );

        rig.effects.advance(Duration::from_secs(9));
        let held = rig.frame(EYE);
        rig.effects.advance(secs(0.5));
        let half = rig.frame(EYE);

        assert_eq!(held.len(), 1);
        assert!((x_axis(&held[0]).length() - 0.2).abs() < EPS, "full size");
        assert!((y_axis(&held[0]).length() - 0.2).abs() < EPS);
        assert_eq!(half.len(), 1);
        assert!(
            (x_axis(&half[0]).length() - 0.1).abs() < EPS,
            "half size, got {}",
            x_axis(&half[0]).length()
        );
        assert!((y_axis(&half[0]).length() - 0.1).abs() < EPS);
        assert_near(translation(&half[0]), translation(&held[0]));
        assert_eq!(rig.effects.live(), 1);

        rig.effects.advance(secs(0.5));
        assert!(rig.frame(EYE).is_empty());
        assert_eq!(rig.effects.live(), 0);
    }

    #[test]
    fn a_mark_with_zero_fade_stays_full_size_until_its_lifetime_then_is_gone() {
        let mut rig = Rig::new();
        rig.effects.mark(
            Vec3::ZERO,
            Vec3::Y,
            0.0,
            mark_style(0.2, Duration::from_secs(2), Duration::ZERO),
        );

        rig.effects.advance(ms(1900));
        let held = rig.frame(EYE);
        rig.effects.advance(ms(100));

        assert_eq!(held.len(), 1);
        assert!((x_axis(&held[0]).length() - 0.2).abs() < EPS);
        assert!(rig.frame(EYE).is_empty());
        assert_eq!(rig.effects.live(), 0);
    }

    #[test]
    fn a_mark_with_zero_lifetime_never_draws() {
        let mut rig = Rig::new();

        rig.effects.mark(
            Vec3::ZERO,
            Vec3::Y,
            0.0,
            mark_style(0.2, Duration::ZERO, Duration::ZERO),
        );

        assert!(rig.frame(EYE).is_empty());
        rig.effects.advance(ms(1));
        assert_eq!(rig.effects.live(), 0);
    }

    fn long_mark() -> MarkStyle {
        mark_style(0.2, Duration::from_secs(10), Duration::from_secs(1))
    }

    #[test]
    fn a_full_cap_drops_the_oldest_fixed_lifetime_effect() {
        let mut rig = Rig::new();
        rig.effects.set_quality(1);
        for i in 0..32_u8 {
            rig.effects
                .mark(Vec3::new(f32::from(i), 0.0, 0.0), Vec3::Y, 0.0, long_mark());
        }
        assert_eq!(rig.effects.live(), 32);

        rig.effects
            .mark(Vec3::new(100.0, 0.0, 0.0), Vec3::Y, 0.0, long_mark());

        assert_eq!(rig.effects.live(), 32);
        let xs: Vec<f32> = rig
            .frame(EYE)
            .iter()
            .map(|instance| translation(instance).x)
            .collect();
        assert_eq!(xs.len(), 32);
        assert!(
            !xs.iter().any(|x| x.abs() < 0.5),
            "the oldest (x = 0) is dropped"
        );
        assert!(
            xs.iter().any(|x| (x - 1.0).abs() < 0.5),
            "the next oldest stays"
        );
        assert!(
            xs.iter().any(|x| (x - 100.0).abs() < 0.5),
            "the new one is live"
        );
    }

    #[test]
    fn a_full_cap_of_flashes_and_marks_drops_the_oldest_of_either_kind() {
        let mut rig = Rig::new();
        rig.effects.set_quality(1);
        rig.effects.flash(
            Vec3::new(0.0, 5.0, 0.0),
            0.0,
            flash_style(0.4, Duration::from_secs(10)),
        );
        for i in 1..32_u8 {
            rig.effects
                .mark(Vec3::new(f32::from(i), 0.0, 0.0), Vec3::Y, 0.0, long_mark());
        }

        rig.effects
            .mark(Vec3::new(100.0, 0.0, 0.0), Vec3::Y, 0.0, long_mark());

        assert_eq!(rig.effects.live(), 32);
        let drawn = rig.frame(EYE);
        assert_eq!(drawn.len(), 32);
        assert!(
            !drawn.iter().any(|i| (translation(i).y - 5.0).abs() < 0.5),
            "the oldest, a flash, is dropped"
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
            .mark(Vec3::new(100.0, 0.0, 0.0), Vec3::Y, 0.0, long_mark());

        assert_eq!(rig.effects.live(), 32);
        for handle in &handles {
            rig.effects.place_streak(*handle, Vec3::X, Vec3::X);
        }
        let drawn = rig.frame(EYE);
        assert_eq!(drawn.len(), 32, "only the streaks draw");
        assert!(
            !drawn.iter().any(|i| (translation(i).x - 100.0).abs() < 0.5),
            "the mark is refused"
        );
    }

    #[test]
    fn a_full_cap_of_streaks_refuses_a_flash() {
        let mut rig = Rig::new();
        rig.effects.set_quality(1);
        for _ in 0..32 {
            rig.effects.begin_streak(style(2.0, 0.1));
        }

        rig.effects
            .flash(Vec3::Y * 7.0, 0.0, flash_style(0.4, ms(50)));

        assert_eq!(rig.effects.live(), 32);
        assert!(
            rig.frame(EYE).is_empty(),
            "no streak was placed, the flash was refused"
        );
    }

    #[test]
    fn a_full_cap_never_evicts_a_streak_even_when_it_is_the_oldest() {
        let mut rig = Rig::new();
        rig.effects.set_quality(1);
        let streak = rig.effects.begin_streak(style(2.0, 0.1));
        for i in 0..31_u8 {
            rig.effects
                .mark(Vec3::new(f32::from(i), 0.0, 0.0), Vec3::Y, 0.0, long_mark());
        }
        assert_eq!(rig.effects.live(), 32);

        rig.effects
            .mark(Vec3::new(100.0, 0.0, 0.0), Vec3::Y, 0.0, long_mark());

        assert_eq!(rig.effects.live(), 32);
        rig.effects
            .place_streak(streak, Vec3::new(0.0, 50.0, 0.0), Vec3::X);
        let drawn = rig.frame(EYE);
        assert_eq!(drawn.len(), 32);
        assert!(
            drawn.iter().any(|i| (head_of(i).y - 50.0).abs() < 0.5),
            "the streak survived and still draws"
        );
        assert!(
            !drawn.iter().any(|i| translation(i).x.abs() < 0.5
                && (translation(i).y - 0.01).abs() < 0.5
                && (translation(i).z).abs() < 0.5
                && x_axis(i).length() < 0.25
                && x_axis(i).length() > 0.15),
            "the oldest mark (x = 0) was the one dropped"
        );
    }

    #[test]
    fn quality_zero_refuses_flash_and_mark() {
        let mut rig = Rig::new();
        rig.effects.set_quality(0);

        rig.effects.flash(Vec3::X, 0.0, flash_style(0.4, ms(50)));
        rig.effects.mark(Vec3::Y, Vec3::Y, 0.0, long_mark());

        assert_eq!(rig.effects.live(), 0);
        assert!(rig.frame(EYE).is_empty());
        rig.effects.advance(ms(10));
        assert_eq!(rig.effects.live(), 0);
    }

    #[test]
    fn quality_zero_clears_live_flashes_and_marks() {
        let mut rig = Rig::new();
        rig.effects.flash(Vec3::X, 0.0, flash_style(0.4, ms(50)));
        rig.effects.mark(Vec3::Y, Vec3::Y, 0.0, long_mark());
        assert_eq!(rig.effects.live(), 2);

        rig.effects.set_quality(0);

        assert_eq!(rig.effects.live(), 0);
        assert!(rig.frame(EYE).is_empty());
    }

    #[test]
    fn a_full_cap_drops_one_oldest_fixed_lifetime_effect_when_over_the_cap() {
        let mut rig = Rig::new();
        for i in 0..40_u8 {
            rig.effects
                .mark(Vec3::new(f32::from(i), 0.0, 0.0), Vec3::Y, 0.0, long_mark());
        }
        rig.effects.set_quality(1);
        assert_eq!(rig.effects.live(), 40, "kept though over the new cap");

        rig.effects
            .mark(Vec3::new(100.0, 0.0, 0.0), Vec3::Y, 0.0, long_mark());

        assert_eq!(rig.effects.live(), 40, "one out, one in");
        let xs: Vec<f32> = rig
            .frame(EYE)
            .iter()
            .map(|instance| translation(instance).x)
            .collect();
        assert_eq!(xs.len(), 40);
        assert!(!xs.iter().any(|x| x.abs() < 0.5), "the oldest is gone");
        assert!(xs.iter().any(|x| (x - 1.0).abs() < 0.5), "the next stays");
        assert!(xs.iter().any(|x| (x - 39.0).abs() < 0.5));
        assert!(
            xs.iter().any(|x| (x - 100.0).abs() < 0.5),
            "the new one is in"
        );
    }

    #[test]
    fn a_flash_at_a_full_cap_drops_the_oldest_fixed_lifetime_effect() {
        let mut rig = Rig::new();
        rig.effects.set_quality(1);
        for i in 0..32_u8 {
            rig.effects
                .mark(Vec3::new(f32::from(i), 0.0, 0.0), Vec3::Y, 0.0, long_mark());
        }

        rig.effects
            .flash(Vec3::new(0.0, 5.0, 0.0), 0.0, flash_style(0.4, ms(50)));

        assert_eq!(rig.effects.live(), 32);
        let drawn = rig.frame(EYE);
        assert_eq!(drawn.len(), 32);
        assert!(
            drawn.iter().any(|i| (translation(i).y - 5.0).abs() < 0.1),
            "the flash is live"
        );
        assert!(
            !drawn
                .iter()
                .any(|i| translation(i).x.abs() < 0.1 && (translation(i).y - 0.01).abs() < 0.1),
            "the oldest mark (x = 0) is dropped"
        );
    }

    #[test]
    fn a_mark_carries_its_own_material() {
        let mut rig = Rig::new();
        let style = MarkStyle {
            material: 5,
            ..long_mark()
        };
        let flash = FlashStyle {
            material: 9,
            ..flash_style(0.4, ms(50))
        };

        rig.effects.mark(Vec3::X, Vec3::Y, 0.0, style);
        rig.effects.flash(Vec3::Z, 0.0, flash);
        let mut materials: Vec<u32> = rig.frame(EYE).iter().map(|i| i.material).collect();

        materials.sort_unstable();
        assert_eq!(materials, vec![5, 9]);
    }

    #[test]
    fn the_same_spawns_draw_the_same_instances() {
        let run = || {
            let mut rig = Rig::new();
            rig.effects
                .flash(Vec3::new(1.0, 2.0, 3.0), 0.3, flash_style(0.4, ms(50)));
            rig.effects.mark(
                Vec3::new(-1.0, 0.0, 2.0),
                Vec3::new(0.0, 0.6, 0.8),
                1.1,
                long_mark(),
            );
            let mut frames = vec![rig.frame(EYE)];
            for _ in 0..3 {
                rig.effects.advance(ms(20));
                frames.push(rig.frame(EYE));
            }
            frames
                .iter()
                .map(|frame| frame.iter().map(signature).collect::<Vec<_>>())
                .collect::<Vec<_>>()
        };

        let first = run();
        let second = run();

        assert_eq!(first, second);
        assert_eq!(first[0].len(), 2, "both drew on the first frame");
    }

    proptest! {
        #[test]
        fn a_mark_basis_is_finite_for_any_normal(
            axis in 0usize..3,
            negative in any::<bool>(),
            free in (-1.0f32..1.0, -1.0f32..1.0, -1.0f32..1.0),
            use_axis in any::<bool>(),
            roll in -10.0f32..10.0,
            at in (-50.0f32..50.0, -50.0f32..50.0, -50.0f32..50.0),
        ) {
            let normal = if use_axis {
                let sign = if negative { -1.0 } else { 1.0 };
                [Vec3::X, Vec3::Y, Vec3::Z][axis] * sign
            } else {
                prop_assume!(Vec3::new(free.0, free.1, free.2).length() > 0.05);
                Vec3::new(free.0, free.1, free.2).normalize()
            };
            let mut rig = Rig::new();

            rig.effects.mark(Vec3::new(at.0, at.1, at.2), normal, roll, long_mark());
            let drawn = rig.frame(EYE);

            prop_assert_eq!(drawn.len(), 1);
            prop_assert!(
                drawn[0].model.iter().all(|c| c.iter().all(|v| v.is_finite())),
                "non-finite matrix {:?}",
                drawn[0].model
            );
            let (x, y, z) = (x_axis(&drawn[0]), y_axis(&drawn[0]), z_axis(&drawn[0]));
            prop_assert!((z - normal).length() < 1e-3, "Z is the normal, got {z}");
            prop_assert!((x.length() - 0.2).abs() < 1e-3);
            prop_assert!((y.length() - 0.2).abs() < 1e-3);
            prop_assert!(x.dot(normal).abs() < 1e-3);
            prop_assert!(y.dot(normal).abs() < 1e-3);
            prop_assert!(x.cross(y).normalize().dot(normal) > 1.0 - 1e-3);
        }
    }

    /// 32 long marks at x = 0..32 under the quality-1 cap.
    fn rig_full_of_marks() -> Rig {
        let mut rig = Rig::new();
        rig.effects.set_quality(1);
        for i in 0..32_u8 {
            rig.effects
                .mark(Vec3::new(f32::from(i), 0.0, 0.0), Vec3::Y, 0.0, long_mark());
        }
        rig
    }

    /// 32 long flashes at x = 0..32 under the quality-1 cap.
    fn rig_full_of_flashes() -> Rig {
        let mut rig = Rig::new();
        rig.effects.set_quality(1);
        for i in 0..32_u8 {
            rig.effects.flash(
                Vec3::new(f32::from(i), 0.0, 0.0),
                0.0,
                flash_style(0.4, Duration::from_secs(10)),
            );
        }
        rig
    }

    fn assert_nothing_evicted(rig: &mut Rig) {
        assert_eq!(rig.effects.live(), 32, "nothing evicted, nothing added");
        let drawn = rig.frame(EYE);
        assert_eq!(drawn.len(), 32);
        assert!(
            drawn.iter().any(|i| translation(i).x.abs() < 0.1),
            "the oldest (x = 0) is still live"
        );
    }

    #[test]
    fn a_mark_that_could_never_draw_is_refused_without_evicting() {
        let nan = f32::NAN;
        let bad: [(Vec3, Vec3, f32, f32); 8] = [
            (Vec3::X * 100.0, Vec3::ZERO, 0.0, 0.2),
            (Vec3::new(nan, 0.0, 0.0), Vec3::Y, 0.0, 0.2),
            (Vec3::X * 100.0, Vec3::new(nan, 1.0, 0.0), 0.0, 0.2),
            (Vec3::X * 100.0, Vec3::Y, nan, 0.2),
            (Vec3::X * 100.0, Vec3::Y, f32::INFINITY, 0.2),
            (Vec3::X * 100.0, Vec3::Y, 0.0, 0.0),
            (Vec3::X * 100.0, Vec3::Y, 0.0, -0.2),
            (Vec3::X * 100.0, Vec3::Y, 0.0, nan),
        ];
        for (at, normal, roll, size) in bad {
            let mut rig = rig_full_of_marks();
            let style = MarkStyle {
                size,
                ..long_mark()
            };

            rig.effects.mark(at, normal, roll, style);

            assert_nothing_evicted(&mut rig);
        }
    }

    #[test]
    fn a_flash_that_could_never_draw_is_refused_without_evicting() {
        let nan = f32::NAN;
        let bad: [(Vec3, f32, f32); 6] = [
            (Vec3::new(nan, 0.0, 0.0), 0.0, 0.4),
            (Vec3::X * 100.0, nan, 0.4),
            (Vec3::X * 100.0, f32::INFINITY, 0.4),
            (Vec3::X * 100.0, 0.0, 0.0),
            (Vec3::X * 100.0, 0.0, -0.4),
            (Vec3::X * 100.0, 0.0, nan),
        ];
        for (at, roll, size) in bad {
            let mut rig = rig_full_of_flashes();
            let style = FlashStyle {
                size,
                ..flash_style(0.4, Duration::from_secs(10))
            };

            rig.effects.flash(at, roll, style);

            assert_nothing_evicted(&mut rig);
        }
    }

    #[test]
    fn a_bad_flash_or_mark_is_refused_with_room_to_spare() {
        let mut rig = Rig::new();

        rig.effects.mark(Vec3::X, Vec3::ZERO, 0.0, long_mark());
        rig.effects
            .flash(Vec3::splat(f32::NAN), 0.0, flash_style(0.4, ms(50)));

        assert_eq!(rig.effects.live(), 0);
    }

    #[test]
    fn a_mark_fade_longer_than_its_lifetime_shrinks_over_the_whole_lifetime() {
        let mut rig = Rig::new();
        rig.effects.mark(
            Vec3::ZERO,
            Vec3::Y,
            0.0,
            mark_style(0.2, Duration::from_secs(1), Duration::from_secs(5)),
        );

        let start = rig.frame(EYE);
        rig.effects.advance(ms(500));
        let half = rig.frame(EYE);

        assert_eq!(start.len(), 1);
        assert!((x_axis(&start[0]).length() - 0.2).abs() < EPS, "full at 0");
        assert_eq!(half.len(), 1);
        assert!(
            (x_axis(&half[0]).length() - 0.1).abs() < EPS,
            "half at half the lifetime, got {}",
            x_axis(&half[0]).length()
        );
        assert_finite_columns(&half[0]);
        rig.effects.advance(ms(500));
        assert!(rig.frame(EYE).is_empty());
        assert_eq!(rig.effects.live(), 0);
    }

    fn assert_finite_columns(instance: &InstanceGpu) {
        assert!(
            instance.model.iter().flatten().all(|v| v.is_finite()),
            "non-finite matrix {:?}",
            instance.model
        );
    }

    #[test]
    fn over_the_cap_with_only_streaks_a_flash_or_mark_is_refused() {
        let mut rig = Rig::new();
        let handles: Vec<EffectHandle> = (0..40)
            .map(|_| rig.effects.begin_streak(style(2.0, 0.1)))
            .collect();
        rig.effects.set_quality(1);

        rig.effects.mark(Vec3::X * 100.0, Vec3::Y, 0.0, long_mark());
        rig.effects
            .flash(Vec3::X * 100.0, 0.0, flash_style(0.4, ms(50)));

        assert_eq!(rig.effects.live(), 40);
        for handle in &handles {
            rig.effects.place_streak(*handle, Vec3::X, Vec3::X);
        }
        assert_eq!(rig.frame(EYE).len(), 40, "every streak stays, nothing else");
    }
}
