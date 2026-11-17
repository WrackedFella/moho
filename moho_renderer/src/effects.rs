//! Engine-drawn effects. Streaks are placed by handle: a game starts one, places it every
//! frame and ends it. Flashes and marks are fixed-lifetime: a game spawns one and forgets
//! it. The engine owns life, fade, cap and unshadowed draw for all of them.

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

/// A flash: a star that faces the eye and shrinks linearly to nothing over `lifetime`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FlashStyle {
    pub size: f32,
    pub material: u32,
    pub lifetime: Duration,
}

/// A mark: a quad on a surface that keeps `size` until `fade` before `lifetime`, then
/// shrinks to nothing by `lifetime`. A zero `fade` removes it abruptly.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MarkStyle {
    pub size: f32,
    pub material: u32,
    pub lifetime: Duration,
    pub fade: Duration,
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

/// A muzzle flash: a camera-facing star that shrinks to nothing over its lifetime.
#[derive(Debug)]
struct Flash {
    style: FlashStyle,
    at: Vec3,
    roll: f32,
    age: Duration,
}

impl Flash {
    /// The size factor: 1 when new, falling linearly to 0 at the lifetime.
    fn scale(&self) -> f32 {
        if self.style.lifetime.is_zero() {
            return 0.0;
        }
        (1.0 - self.age.as_secs_f32() / self.style.lifetime.as_secs_f32()).clamp(0.0, 1.0)
    }
}

/// An impact mark: a quad on a surface that holds its size, then shrinks out over its fade.
#[derive(Debug)]
struct Mark {
    style: MarkStyle,
    at: Vec3,
    /// Unit surface normal.
    normal: Vec3,
    roll: f32,
    age: Duration,
}

impl Mark {
    /// The size factor: 1 until `lifetime - fade`, then linear to 0 at the lifetime.
    fn scale(&self) -> f32 {
        let remaining = self.style.lifetime.saturating_sub(self.age);
        if remaining >= self.style.fade {
            return 1.0;
        }
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

    /// Flashes and marks live a fixed time, so the cap may drop them; streaks end when told.
    fn is_fixed_lifetime(&self) -> bool {
        !matches!(self, Self::Streak(_))
    }
}

/// Mesh ids of the engine's effect meshes, offset from `EFFECT_MESH_IDS_FROM`.
const STREAK_MESH: usize = 0;
const FLASH_MESH: usize = 1;
const MARK_MESH: usize = 2;

/// How far a mark floats off its surface so it does not z-fight with it.
const MARK_OFFSET: f32 = 0.01;

#[derive(Debug)]
pub struct Effects {
    store: BTreeMap<EffectHandle, Effect>,
    next_handle: u64,
    cap: usize,
    /// Whether each effect mesh, indexed by its offset from `EFFECT_MESH_IDS_FROM`, is uploaded.
    mesh_uploaded: [bool; 3],
}

impl Default for Effects {
    fn default() -> Self {
        Self {
            store: BTreeMap::new(),
            next_handle: 0,
            cap: CAPS[DEFAULT_QUALITY],
            mesh_uploaded: [false; 3],
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

    /// Spawn a flash at `at`, turned by `roll` about the view axis. Refused for a
    /// non-finite `at` or `roll`; at the cap it replaces the oldest flash or mark, and is
    /// refused when only streaks are live.
    pub fn flash(&mut self, at: Vec3, roll: f32, style: FlashStyle) {
        if !at.is_finite() || !roll.is_finite() {
            return;
        }
        self.spawn_fixed(Effect::Flash(Flash {
            style,
            at,
            roll,
            age: Duration::ZERO,
        }));
    }

    /// Spawn a mark at `at` on a surface facing `normal`, turned by `roll` about it.
    /// Refused for a zero or non-finite `normal`, `at` or `roll`; at the cap it replaces the
    /// oldest flash or mark, and is refused when only streaks are live.
    pub fn mark(&mut self, at: Vec3, normal: Vec3, roll: f32, style: MarkStyle) {
        let Some(normal) = normal.try_normalize() else {
            return;
        };
        if !at.is_finite() || !roll.is_finite() {
            return;
        }
        self.spawn_fixed(Effect::Mark(Mark {
            style,
            at,
            normal,
            roll,
            age: Duration::ZERO,
        }));
    }

    fn issue_handle(&mut self) -> EffectHandle {
        let handle = EffectHandle(self.next_handle);
        self.next_handle += 1;
        handle
    }

    fn spawn_fixed(&mut self, effect: Effect) {
        if self.cap == 0 {
            return;
        }
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
                    let Some(transform) =
                        streak_transform(streak.head, heading, streak.style.length, width, eye)
                    else {
                        continue;
                    };
                    (STREAK_MESH, transform, streak.style.material)
                }
                Effect::Flash(flash) => {
                    let Some(transform) = flash_transform(flash, eye) else {
                        continue;
                    };
                    (FLASH_MESH, transform, flash.style.material)
                }
                Effect::Mark(mark) => {
                    let Some(transform) = mark_transform(mark) else {
                        continue;
                    };
                    (MARK_MESH, transform, mark.style.material)
                }
            };
            let id = WorldMeshId(EFFECT_MESH_IDS_FROM + kind as u64);
            if !std::mem::replace(&mut self.mesh_uploaded[kind], true) {
                let mesh = match kind {
                    STREAK_MESH => streak_mesh(),
                    FLASH_MESH => flash_mesh(),
                    _ => mark_mesh(),
                };
                meshes.upsert_movable(id, mesh);
            }
            meshes.place_unshadowed(id, transform, material);
        }
    }
}

/// An orthonormal pair `(x, y)` across the unit `normal`, turned by `roll` about it.
fn rolled_basis(normal: Vec3, roll: f32) -> (Vec3, Vec3) {
    let x = normal.any_orthonormal_vector();
    let y = normal.cross(x);
    let (sin, cos) = roll.sin_cos();
    (x * cos + y * sin, y * cos - x * sin)
}

/// The flash facing `eye`, or `None` when it has no size or the eye sits on it.
fn flash_transform(flash: &Flash, eye: Vec3) -> Option<Affine3A> {
    let size = flash.style.size * flash.scale();
    if !eye.is_finite() || !size.is_finite() || size <= 0.0 {
        return None;
    }
    let toward_eye = (eye - flash.at).try_normalize()?;
    let (x, y) = rolled_basis(toward_eye, flash.roll);
    Some(Affine3A::from_cols(
        (x * size).into(),
        (y * size).into(),
        toward_eye.into(),
        flash.at.into(),
    ))
}

/// The mark lying on its surface, or `None` when it has no size.
fn mark_transform(mark: &Mark) -> Option<Affine3A> {
    let size = mark.style.size * mark.scale();
    if !size.is_finite() || size <= 0.0 {
        return None;
    }
    let (x, y) = rolled_basis(mark.normal, mark.roll);
    Some(Affine3A::from_cols(
        (x * size).into(),
        (y * size).into(),
        mark.normal.into(),
        (mark.at + mark.normal * MARK_OFFSET).into(),
    ))
}

/// A mesh with +Z normals and neutral channels, one value per vertex.
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
    .expect("every channel has one value per vertex and the indices are in range")
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

/// A four-point star in the XY plane, points of radius 0.5 on the axes, counter-clockwise
/// seen from +Z. Even vertices are the points, odd ones the notches between them.
fn flash_mesh() -> WorldMesh {
    const NOTCH: f32 = 0.15;
    let positions = (0..8_u8)
        .map(|i| {
            let radius = if i % 2 == 0 { 0.5 } else { NOTCH };
            let angle = f32::from(i) * std::f32::consts::FRAC_PI_4;
            [radius * angle.cos(), radius * angle.sin(), 0.0]
        })
        .collect();
    // Each point is a triangle with its two notches; the notches form the centre square.
    let mut indices = Vec::new();
    for point in [0_u32, 2, 4, 6] {
        indices.extend([(point + 7) % 8, point, point + 1]);
    }
    indices.extend([1, 3, 5, 1, 5, 7]);
    flat_mesh(positions, indices)
}

/// A unit square centred on the origin in z = 0, counter-clockwise seen from +Z.
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
        /// Each handle passed to `unregister_mesh`.
        unregistered: Vec<u32>,
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
        fn unregister_mesh(&mut self, mesh: u32) {
            self.unregistered.push(mesh);
        }
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

    fn flash_style(size: f32, material: u32, lifetime_ms: u64) -> FlashStyle {
        FlashStyle {
            size,
            material,
            lifetime: ms(lifetime_ms),
        }
    }

    fn mark_style(size: f32, material: u32) -> MarkStyle {
        MarkStyle {
            size,
            material,
            lifetime: Duration::from_secs(10),
            fade: Duration::from_secs(1),
        }
    }

    /// The instance drawn at `position`, if any.
    fn found_at(drawn: &[InstanceGpu], position: Vec3) -> Option<&InstanceGpu> {
        drawn
            .iter()
            .find(|i| (translation(i) - position).length() < EPS)
    }

    #[test]
    fn a_flash_shrinks_linearly_and_expires() {
        // (size, lifetime in ms, [(advance in ms before the frame, expected axis length)]).
        type Row = (f32, u64, Vec<(u64, Option<f32>)>);
        let rows: [Row; 2] = [
            (0.4, 50, vec![(0, Some(0.4)), (25, Some(0.2)), (25, None)]),
            (
                1.0,
                200,
                vec![
                    (0, Some(1.0)),
                    (50, Some(0.75)),
                    (50, Some(0.5)),
                    (100, None),
                ],
            ),
        ];
        for (size, lifetime, steps) in rows {
            let mut rig = Rig::new();
            rig.effects.flash(
                Vec3::new(1.0, 2.0, 3.0),
                0.0,
                flash_style(size, 0, lifetime),
            );

            for (advance, expected) in steps {
                rig.effects.advance(ms(advance));
                let drawn = rig.frame(EYE);

                if let Some(length) = expected {
                    assert_eq!(drawn.len(), 1, "size {size} after {advance} ms");
                    assert!((x_axis(&drawn[0]).length() - length).abs() < EPS);
                    assert!((y_axis(&drawn[0]).length() - length).abs() < EPS);
                } else {
                    assert!(drawn.is_empty(), "size {size} is gone");
                    assert_eq!(rig.effects.live(), 0);
                }
            }
        }
    }

    /// The flash at `at` faces `eye`: X and Y are orthogonal, `size` long, and their normal
    /// points from `at` to the eye.
    fn assert_flash_faces(instance: &InstanceGpu, at: Vec3, eye: Vec3, size: f32) {
        let (x, y) = (x_axis(instance), y_axis(instance));
        assert!((x.length() - size).abs() < 1e-3, "|X| = {}", x.length());
        assert!((y.length() - size).abs() < 1e-3, "|Y| = {}", y.length());
        assert!(x.dot(y).abs() < 1e-3, "X and Y are orthogonal");
        assert_near(translation(instance), at);
        let facing = x.cross(y).normalize();
        let toward = (eye - at).normalize();
        assert!(
            (facing - toward).length() < 1e-3,
            "X x Y is {facing}, the eye is along {toward}"
        );
    }

    #[test]
    fn a_flash_faces_the_eye() {
        let at = Vec3::new(1.0, 2.0, 3.0);
        let eyes = [
            at + Vec3::new(5.0, 0.0, 0.0),
            at + Vec3::new(-5.0, 1.0, 0.0),
            at + Vec3::new(0.0, 7.0, 0.5),
            at + Vec3::new(0.0, -7.0, 0.5),
            at + Vec3::new(0.0, 7.0, 0.0),
            at + Vec3::new(0.0, -7.0, 0.0),
            at + Vec3::new(0.0, 1.0, 9.0),
            at + Vec3::new(0.0, 1.0, -9.0),
            EYE,
        ];
        for size in [1.0, 0.3] {
            for eye in eyes {
                let mut rig = Rig::new();
                rig.effects.flash(at, 0.0, flash_style(size, 0, 50));

                let drawn = rig.frame(eye);

                assert_eq!(drawn.len(), 1);
                assert_flash_faces(&drawn[0], at, eye, size);
            }
        }
    }

    proptest! {
        #[test]
        fn a_flash_faces_any_eye_direction(
            offset in (-20.0f32..20.0, -20.0f32..20.0, -20.0f32..20.0),
            roll in 0.0f32..std::f32::consts::TAU,
            size in 0.05f32..2.0,
        ) {
            let at = Vec3::new(1.0, 2.0, 3.0);
            let offset = Vec3::new(offset.0, offset.1, offset.2);
            prop_assume!(offset.length() > 1.0);
            let mut rig = Rig::new();
            rig.effects.flash(at, roll, flash_style(size, 0, 50));

            let drawn = rig.frame(at + offset);

            prop_assert_eq!(drawn.len(), 1);
            assert_flash_faces(&drawn[0], at, at + offset, size);
        }
    }

    #[test]
    fn a_flash_roll_turns_it_in_its_plane_while_it_still_faces_the_eye() {
        let at = Vec3::new(1.0, 2.0, 3.0);
        let mut unrolled = Rig::new();
        unrolled.effects.flash(at, 0.0, flash_style(1.0, 0, 50));
        let mut rolled = Rig::new();
        rolled
            .effects
            .flash(at, std::f32::consts::FRAC_PI_2, flash_style(1.0, 0, 50));

        let before = unrolled.frame(EYE);
        let after = rolled.frame(EYE);

        assert_eq!(before.len(), 1);
        assert_eq!(after.len(), 1);
        assert!(
            x_axis(&before[0]).dot(x_axis(&after[0])).abs() < EPS,
            "a quarter turn puts X where it was perpendicular"
        );
        assert_flash_faces(&after[0], at, EYE, 1.0);
        assert_flash_faces(&before[0], at, EYE, 1.0);
    }

    #[test]
    fn a_mark_lies_along_its_normal_offset_by_the_mark_offset() {
        let at = Vec3::new(4.0, 0.0, -2.0);
        let oblique = Vec3::new(1.0, 2.0, 2.0) / 3.0;
        let rows = [
            (Vec3::Y, Vec3::new(4.0, 0.01, -2.0)),
            (
                oblique,
                Vec3::new(4.0 + 0.01 / 3.0, 0.02 / 3.0, -2.0 + 0.02 / 3.0),
            ),
        ];
        for (normal, expected) in rows {
            let mut rig = Rig::new();
            rig.effects.mark(at, normal, 0.0, mark_style(0.2, 0));

            let drawn = rig.frame(EYE);

            assert_eq!(drawn.len(), 1);
            let (x, y, z) = (x_axis(&drawn[0]), y_axis(&drawn[0]), z_axis(&drawn[0]));
            assert_near(z, normal);
            assert!((x.length() - 0.2).abs() < EPS);
            assert!((y.length() - 0.2).abs() < EPS);
            assert!(x.dot(normal).abs() < EPS);
            assert!(y.dot(normal).abs() < EPS);
            assert!(x.dot(y).abs() < EPS);
            assert_near(translation(&drawn[0]), expected);
        }
    }

    #[test]
    fn a_mark_holds_full_size_then_shrinks_out_in_its_fade() {
        // (lifetime in ms, fade in ms, [(advance in ms, expected axis length)]) at size 0.2.
        type Row = (u64, u64, Vec<(u64, Option<f32>)>);
        let rows: [Row; 2] = [
            (
                10_000,
                1_000,
                vec![(9_000, Some(0.2)), (500, Some(0.1)), (500, None)],
            ),
            (
                4_000,
                2_000,
                vec![
                    (1_000, Some(0.2)),
                    (1_000, Some(0.2)),
                    (1_000, Some(0.1)),
                    (1_000, None),
                ],
            ),
        ];
        for (lifetime, fade, steps) in rows {
            let mut rig = Rig::new();
            rig.effects.mark(
                Vec3::ZERO,
                Vec3::Y,
                0.0,
                MarkStyle {
                    size: 0.2,
                    material: 0,
                    lifetime: ms(lifetime),
                    fade: ms(fade),
                },
            );

            for (advance, expected) in steps {
                rig.effects.advance(ms(advance));
                let drawn = rig.frame(EYE);

                if let Some(length) = expected {
                    assert_eq!(drawn.len(), 1, "lifetime {lifetime} fade {fade}");
                    assert!((x_axis(&drawn[0]).length() - length).abs() < EPS);
                    assert!((y_axis(&drawn[0]).length() - length).abs() < EPS);
                } else {
                    assert!(drawn.is_empty(), "lifetime {lifetime} is over");
                    assert_eq!(rig.effects.live(), 0);
                }
            }
        }
    }

    #[test]
    fn a_mark_roll_turns_it_a_quarter_about_its_normal() {
        let at = Vec3::new(4.0, 0.0, -2.0);
        for normal in [Vec3::Y, Vec3::new(1.0, 2.0, 2.0) / 3.0] {
            let mut unrolled = Rig::new();
            unrolled.effects.mark(at, normal, 0.0, mark_style(1.0, 0));
            let mut rolled = Rig::new();
            rolled
                .effects
                .mark(at, normal, std::f32::consts::FRAC_PI_2, mark_style(1.0, 0));

            let before = unrolled.frame(EYE);
            let after = rolled.frame(EYE);

            assert_eq!(before.len(), 1);
            assert_eq!(after.len(), 1);
            assert!(
                x_axis(&before[0]).dot(x_axis(&after[0])).abs() < EPS,
                "a quarter turn makes X perpendicular to the unrolled X"
            );
            assert_near(z_axis(&after[0]), normal);
            assert_near(translation(&after[0]), translation(&before[0]));
        }
    }

    #[test]
    fn a_mark_normalises_its_normal() {
        let mut rig = Rig::new();
        let at = Vec3::new(4.0, 0.0, -2.0);

        rig.effects
            .mark(at, Vec3::new(0.0, 0.0, 3.0), 0.0, mark_style(0.2, 0));
        let drawn = rig.frame(EYE);

        assert_eq!(drawn.len(), 1);
        assert_near(z_axis(&drawn[0]), Vec3::Z);
        assert_near(translation(&drawn[0]), Vec3::new(4.0, 0.0, -1.99));
        assert!((x_axis(&drawn[0]).length() - 0.2).abs() < EPS);
        assert!((y_axis(&drawn[0]).length() - 0.2).abs() < EPS);
    }

    #[test]
    fn a_mark_with_a_zero_or_non_finite_normal_is_refused() {
        let bad = [
            Vec3::ZERO,
            Vec3::new(f32::NAN, 0.0, 1.0),
            Vec3::new(0.0, f32::INFINITY, 0.0),
            Vec3::splat(f32::NEG_INFINITY),
        ];
        for normal in bad {
            let mut rig = Rig::new();

            rig.effects.mark(Vec3::X, normal, 0.0, mark_style(0.2, 0));

            assert_eq!(rig.effects.live(), 0, "normal {normal}");
            assert!(rig.frame(EYE).is_empty());
        }
    }

    #[test]
    fn a_full_cap_drops_the_oldest_fixed_lifetime_effect() {
        let mut rig = Rig::new();
        rig.effects.set_quality(1);
        let position = |i: u32| Vec3::new(i as f32 * 10.0, 0.0, 0.0);
        for i in 0..32 {
            rig.effects
                .mark(position(i), Vec3::Y, 0.0, mark_style(0.2, 0));
        }
        assert_eq!(rig.effects.live(), 32);

        rig.effects
            .mark(position(32), Vec3::Y, 0.0, mark_style(0.2, 0));
        let drawn = rig.frame(EYE);

        assert_eq!(rig.effects.live(), 32);
        assert_eq!(drawn.len(), 32);
        assert!(found_at(&drawn, position(0) + Vec3::Y * MARK_OFFSET).is_none());
        assert!(found_at(&drawn, position(1) + Vec3::Y * MARK_OFFSET).is_some());
        assert!(found_at(&drawn, position(32) + Vec3::Y * MARK_OFFSET).is_some());
    }

    #[test]
    fn a_full_cap_evicts_a_flash_for_a_mark_oldest_first() {
        let mut rig = Rig::new();
        rig.effects.set_quality(1);
        let position = |i: u32| Vec3::new(i as f32 * 10.0, 0.0, 0.0);
        rig.effects
            .flash(position(0), 0.0, flash_style(0.4, 0, 5000));
        for i in 1..32 {
            rig.effects
                .mark(position(i), Vec3::Y, 0.0, mark_style(0.2, 0));
        }

        rig.effects
            .mark(position(32), Vec3::Y, 0.0, mark_style(0.2, 0));
        let drawn = rig.frame(EYE);

        assert_eq!(rig.effects.live(), 32);
        assert!(found_at(&drawn, position(0)).is_none(), "the flash went");
        assert!(found_at(&drawn, position(1) + Vec3::Y * MARK_OFFSET).is_some());
    }

    #[test]
    fn a_full_cap_of_streaks_refuses_a_mark_and_keeps_every_streak() {
        for spawn_a_flash in [false, true] {
            let mut rig = Rig::new();
            rig.effects.set_quality(1);
            let head = |i: u32| Vec3::new(i as f32 * 10.0, 1.0, 0.0);
            let handles: Vec<EffectHandle> = (0..32)
                .map(|_| rig.effects.begin_streak(style(2.0, 0.1)))
                .collect();
            for (i, handle) in (0..).zip(&handles) {
                rig.effects.place_streak(*handle, head(i), Vec3::X);
            }

            let at = Vec3::new(500.0, 0.0, 0.0);
            if spawn_a_flash {
                rig.effects.flash(at, 0.0, flash_style(0.4, 0, 5000));
            } else {
                rig.effects.mark(at, Vec3::Y, 0.0, mark_style(0.2, 0));
            }
            let drawn = rig.frame(EYE);

            assert_eq!(rig.effects.live(), 32, "flash: {spawn_a_flash}");
            assert_eq!(drawn.len(), 32, "every streak and no new effect");
            for i in 0..32 {
                assert!(
                    found_at(&drawn, head(i) - Vec3::X * 2.0).is_some(),
                    "streak {i} still draws (flash: {spawn_a_flash})"
                );
            }
        }
    }

    #[test]
    fn a_full_cap_with_the_oldest_a_streak_drops_the_oldest_mark_not_the_streak() {
        let mut rig = Rig::new();
        rig.effects.set_quality(1);
        let position = |i: u32| Vec3::new(i as f32 * 10.0, 0.0, 0.0);
        let streak = rig.effects.begin_streak(style(2.0, 0.1));
        let head = Vec3::new(-100.0, 1.0, 0.0);
        rig.effects.place_streak(streak, head, Vec3::X);
        for i in 1..32 {
            rig.effects
                .mark(position(i), Vec3::Y, 0.0, mark_style(0.2, 0));
        }
        assert_eq!(rig.effects.live(), 32);

        rig.effects
            .mark(position(32), Vec3::Y, 0.0, mark_style(0.2, 0));
        let drawn = rig.frame(EYE);

        assert_eq!(rig.effects.live(), 32);
        assert_eq!(drawn.len(), 32);
        assert!(
            found_at(&drawn, head - Vec3::X * 2.0).is_some(),
            "the streak stays"
        );
        assert!(found_at(&drawn, position(1) + Vec3::Y * 0.01).is_none());
        assert!(found_at(&drawn, position(2) + Vec3::Y * 0.01).is_some());
        assert!(found_at(&drawn, position(32) + Vec3::Y * 0.01).is_some());
    }

    #[test]
    fn a_streak_begun_at_a_full_cap_of_fixed_lifetime_effects_is_inert_and_evicts_nothing() {
        let mut rig = Rig::new();
        rig.effects.set_quality(1);
        let position = |i: u32| Vec3::new(i as f32 * 10.0, 0.0, 0.0);
        for i in 0..16 {
            rig.effects
                .flash(position(i), 0.0, flash_style(0.4, 0, 5000));
        }
        for i in 16..32 {
            rig.effects
                .mark(position(i), Vec3::Y, 0.0, mark_style(0.2, 0));
        }

        let handle = rig.effects.begin_streak(style(2.0, 0.1));
        let head = Vec3::new(-100.0, 1.0, 0.0);
        rig.effects.place_streak(handle, head, Vec3::X);
        rig.effects.end(handle);
        let drawn = rig.frame(EYE);

        assert_eq!(rig.effects.live(), 32);
        assert_eq!(drawn.len(), 32, "nothing evicted, no streak drawn");
        assert!(found_at(&drawn, head - Vec3::X * 2.0).is_none());
        for i in 0..16 {
            assert!(found_at(&drawn, position(i)).is_some(), "flash {i}");
        }
        for i in 16..32 {
            assert!(
                found_at(&drawn, position(i) + Vec3::Y * 0.01).is_some(),
                "mark {i}"
            );
        }
    }

    #[test]
    fn quality_zero_refuses_flash_and_mark() {
        let mut rig = Rig::new();
        rig.effects.set_quality(0);

        rig.effects.flash(Vec3::X, 0.0, flash_style(0.4, 0, 50));
        rig.effects.mark(Vec3::Y, Vec3::Y, 0.0, mark_style(0.2, 0));

        assert_eq!(rig.effects.live(), 0);
        assert!(rig.frame(EYE).is_empty());
    }

    #[test]
    fn quality_zero_clears_live_flashes_and_marks() {
        let mut rig = Rig::new();
        rig.effects.flash(Vec3::X, 0.0, flash_style(0.4, 0, 50));
        rig.effects.mark(Vec3::Y, Vec3::Y, 0.0, mark_style(0.2, 0));
        assert_eq!(rig.effects.live(), 2);

        rig.effects.set_quality(0);

        assert_eq!(rig.effects.live(), 0);
        assert!(rig.frame(EYE).is_empty());
    }

    #[test]
    fn a_flash_or_mark_with_a_non_finite_position_or_roll_is_refused() {
        let finite_at = Vec3::new(1.0, 2.0, 3.0);
        let bad_positions = [
            Vec3::new(f32::NAN, 0.0, 0.0),
            Vec3::new(0.0, f32::INFINITY, 0.0),
            Vec3::new(0.0, 0.0, f32::NEG_INFINITY),
        ];
        let bad_rolls = [f32::NAN, f32::INFINITY, f32::NEG_INFINITY];
        for as_flash in [true, false] {
            let spawn = |rig: &mut Rig, at: Vec3, roll: f32| {
                if as_flash {
                    rig.effects.flash(at, roll, flash_style(0.4, 0, 5000));
                } else {
                    rig.effects.mark(at, Vec3::Y, roll, mark_style(0.2, 0));
                }
            };
            for at in bad_positions {
                let mut rig = Rig::new();

                spawn(&mut rig, at, 0.5);

                assert_eq!(rig.effects.live(), 0, "flash: {as_flash}, at {at}");
                assert!(rig.frame(EYE).is_empty());
            }
            for roll in bad_rolls {
                let mut rig = Rig::new();

                spawn(&mut rig, finite_at, roll);

                assert_eq!(rig.effects.live(), 0, "flash: {as_flash}, roll {roll}");
                assert!(rig.frame(EYE).is_empty());
            }
        }
    }

    #[test]
    fn a_flash_or_mark_with_an_unusable_size_draws_nothing() {
        for size in [f32::NAN, f32::INFINITY, 0.0, -0.4] {
            let mut flash = Rig::new();
            flash
                .effects
                .flash(Vec3::new(1.0, 2.0, 3.0), 0.0, flash_style(size, 0, 5000));
            let mut mark = Rig::new();
            mark.effects
                .mark(Vec3::new(1.0, 2.0, 3.0), Vec3::Y, 0.0, mark_style(size, 0));

            assert!(flash.frame(EYE).is_empty(), "flash of size {size}");
            assert!(mark.frame(EYE).is_empty(), "mark of size {size}");
        }
    }

    #[test]
    fn a_flash_seen_from_a_non_finite_eye_draws_nothing() {
        let eyes = [
            Vec3::new(f32::NAN, 0.0, 0.0),
            Vec3::new(0.0, f32::INFINITY, 0.0),
            Vec3::splat(f32::NEG_INFINITY),
        ];
        for eye in eyes {
            let mut rig = Rig::new();
            rig.effects
                .flash(Vec3::new(1.0, 2.0, 3.0), 0.0, flash_style(0.4, 0, 5000));

            let drawn = rig.frame(eye);

            assert!(
                drawn
                    .iter()
                    .all(|i| i.model.iter().flatten().all(|v| v.is_finite())),
                "no non-finite matrix reaches the GPU, eye {eye}"
            );
            assert!(drawn.is_empty(), "eye {eye}");
        }
    }

    fn scripted_frames() -> Vec<Vec<([[f32; 4]; 4], u32)>> {
        let mut rig = Rig::new();
        rig.effects
            .flash(Vec3::new(1.0, 2.0, 3.0), 0.7, flash_style(0.4, 3, 50));
        rig.effects
            .mark(Vec3::new(4.0, 0.0, 1.0), Vec3::X, 1.2, mark_style(0.3, 5));
        let mut frames = Vec::new();
        for _ in 0..3 {
            let drawn = rig.frame(EYE);
            frames.push(drawn.iter().map(|i| (i.model, i.material)).collect());
            rig.effects.advance(ms(20));
        }
        frames
    }

    #[test]
    fn the_same_spawns_draw_the_same_instances() {
        let first = scripted_frames();
        let second = scripted_frames();

        assert_eq!(first, second);
        assert_eq!(first[0].len(), 2, "both effects drew");
    }

    #[test]
    fn the_flash_and_mark_meshes_are_registered_once() {
        let mut rig = Rig::new();

        for i in 0..5 {
            let at = Vec3::new(i as f32, 0.0, 0.0);
            rig.effects.flash(at, 0.0, flash_style(0.4, 0, 5000));
            rig.effects.mark(at, Vec3::Y, 0.0, mark_style(0.2, 0));
            rig.frame(EYE);
        }

        let mut vertex_counts: Vec<usize> = rig
            .backend
            .registered
            .iter()
            .map(|(positions, _)| positions.len())
            .collect();
        vertex_counts.sort_unstable();
        assert_eq!(
            vertex_counts,
            vec![4, 8],
            "one quad and one star, once each"
        );
        let (star_positions, star_indices) = star_geometry(&rig);
        assert_eq!(star_positions.len(), 8);
        assert!(
            star_positions.iter().all(|p| p[2] == 0.0),
            "in the XY plane"
        );
        let centroid = star_positions.iter().map(|p| Vec3::from(*p)).sum::<Vec3>() / 8.0;
        assert_near(centroid, Vec3::ZERO);
        let radii: Vec<f32> = star_positions
            .iter()
            .map(|p| Vec3::from(*p).length())
            .collect();
        let outer = radii.iter().filter(|r| (**r - 0.5).abs() < EPS).count();
        assert_eq!(outer, 4, "four points at radius 0.5");
        assert!(
            radii.iter().all(|r| *r > EPS && *r < 0.5 + EPS),
            "no vertex beyond the points, none at the centre: {radii:?}"
        );
        assert_ccw_from_plus_z(&star_positions, &star_indices);
        let (quad_positions, quad_indices) = quad_geometry(&rig);
        let mut corners = quad_positions.clone();
        corners.sort_by(|a, b| a.partial_cmp(b).expect("finite corners"));
        assert_eq!(
            corners,
            vec![
                [-0.5, -0.5, 0.0],
                [-0.5, 0.5, 0.0],
                [0.5, -0.5, 0.0],
                [0.5, 0.5, 0.0]
            ],
            "a centred unit square in z = 0"
        );
        assert_ccw_from_plus_z(&quad_positions, &quad_indices);
    }

    type Geometry = (Vec<[f32; 3]>, Vec<u32>);

    /// The one registered mesh with eight vertices.
    fn star_geometry(rig: &Rig) -> Geometry {
        let found: Vec<&Geometry> = rig
            .backend
            .registered
            .iter()
            .filter(|(p, _)| p.len() == 8)
            .collect();
        assert_eq!(found.len(), 1, "exactly one star");
        found[0].clone()
    }

    /// The one registered mesh with four vertices and none at x = 1 (the kite's head).
    fn quad_geometry(rig: &Rig) -> Geometry {
        let found: Vec<&Geometry> = rig
            .backend
            .registered
            .iter()
            .filter(|(p, _)| p.len() == 4 && !p.contains(&[1.0, 0.0, 0.0]))
            .collect();
        assert_eq!(found.len(), 1, "exactly one quad");
        found[0].clone()
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

    impl Rig {
        /// Like `frame`, with the backend mesh handle each instance draws.
        fn frame_with_handles(&mut self, eye: Vec3) -> Vec<(u32, InstanceGpu)> {
            self.effects.draw(&mut self.meshes, eye);
            self.meshes.flush(&mut self.backend);
            let drawn = self.meshes.unshadowed_draws().collect();
            self.meshes.clear_placements();
            drawn
        }
    }

    fn sentinel() -> WorldMesh {
        WorldMesh::new(
            vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            vec![[0.0, 0.0, 1.0]; 3],
            vec![1.0; 3],
            vec![[1.0; 3]; 3],
            vec![1.0; 3],
            vec![0; 3],
            vec![0, 1, 2],
        )
        .expect("the sentinel's channels are all three vertices long")
    }

    #[test]
    fn each_effect_kind_draws_its_own_reserved_mesh_id() {
        // Sentinels sit on the first four reserved ids, so the backend handle an effect
        // replaces names the id it uploaded under: handle n belongs to id EFFECT_MESH_IDS_FROM + n - 1.
        for (offset, name) in [(0u32, "streak"), (1, "flash"), (2, "mark")] {
            let mut rig = Rig::new();
            for k in 0..4 {
                rig.meshes
                    .upsert_movable(WorldMeshId(EFFECT_MESH_IDS_FROM + k), sentinel());
            }
            rig.meshes.flush(&mut rig.backend);
            assert_eq!(rig.backend.registered.len(), 4);
            match offset {
                0 => {
                    let handle = rig.effects.begin_streak(style(2.0, 0.1));
                    rig.effects.place_streak(handle, Vec3::X * 5.0, Vec3::X);
                }
                1 => rig.effects.flash(Vec3::X, 0.0, flash_style(0.4, 0, 50)),
                _ => rig.effects.mark(Vec3::X, Vec3::Y, 0.0, mark_style(0.2, 0)),
            }

            let drawn = rig.frame_with_handles(EYE);

            assert_eq!(drawn.len(), 1, "{name}");
            assert_eq!(
                rig.backend.unregistered,
                vec![offset + 1],
                "the {name} replaces the mesh on id EFFECT_MESH_IDS_FROM + {offset}"
            );
            assert_eq!(rig.backend.registered.len(), 5);
            assert_eq!(drawn[0].0, 5, "it draws the mesh it uploaded");
            let (positions, _) = &rig.backend.registered[4];
            let expected_vertices = if offset == 1 { 8 } else { 4 };
            assert_eq!(positions.len(), expected_vertices, "{name} geometry");
            let is_kite = positions.contains(&[1.0, 0.0, 0.0]);
            assert_eq!(is_kite, offset == 0, "only the streak is the kite");
        }
    }

    #[test]
    fn streak_flash_and_mark_draw_three_distinct_ids_in_id_order() {
        let mut rig = Rig::new();
        let handle = rig.effects.begin_streak(style(2.0, 0.1));
        rig.effects.place_streak(handle, Vec3::X * 5.0, Vec3::X);
        rig.effects
            .flash(Vec3::X * 10.0, 0.0, flash_style(0.4, 0, 50));
        rig.effects
            .mark(Vec3::X * 20.0, Vec3::Y, 0.0, mark_style(0.2, 0));

        let drawn = rig.frame_with_handles(EYE);

        let handles: Vec<u32> = drawn.iter().map(|(h, _)| *h).collect();
        let vertex_counts: Vec<usize> = handles
            .iter()
            .map(|h| rig.backend.registered[*h as usize - 1].0.len())
            .collect();
        assert_eq!(handles.len(), 3);
        assert!(handles[0] != handles[1] && handles[1] != handles[2] && handles[0] != handles[2]);
        assert_eq!(
            vertex_counts,
            vec![4, 8, 4],
            "draws come in id order: streak, flash (+1), mark (+2)"
        );
    }

    #[test]
    fn a_streak_flash_and_mark_all_draw_in_one_frame_with_their_materials() {
        let mut rig = Rig::new();
        let handle = rig.effects.begin_streak(StreakStyle::new(2.0, 0.1, 11));
        rig.effects
            .place_streak(handle, Vec3::new(0.0, 5.0, 0.0), Vec3::X);
        rig.effects
            .flash(Vec3::new(10.0, 0.0, 0.0), 0.0, flash_style(0.4, 22, 50));
        rig.effects
            .mark(Vec3::new(20.0, 0.0, 0.0), Vec3::Y, 0.0, mark_style(0.2, 33));

        let drawn = rig.frame(EYE);

        let mut materials: Vec<u32> = drawn.iter().map(|i| i.material).collect();
        materials.sort_unstable();
        assert_eq!(materials, vec![11, 22, 33]);
        assert_eq!(rig.effects.live(), 3);
        assert_eq!(rig.backend.registered.len(), 3, "three distinct meshes");
    }

    /// A unit normal: the six axes or an arbitrary direction.
    fn normals() -> impl Strategy<Value = Vec3> {
        prop_oneof![
            Just(Vec3::X),
            Just(Vec3::NEG_X),
            Just(Vec3::Y),
            Just(Vec3::NEG_Y),
            Just(Vec3::Z),
            Just(Vec3::NEG_Z),
            (-1.0f32..1.0, -1.0f32..1.0, -1.0f32..1.0)
                .prop_filter_map("a non-zero normal", |(x, y, z)| {
                    Vec3::new(x, y, z).try_normalize()
                }),
        ]
    }

    proptest! {
        #[test]
        fn a_mark_basis_is_finite_for_any_normal(
            normal in normals(),
            scale in 0.1f32..5.0,
            roll in 0.0f32..std::f32::consts::TAU,
            size in 0.01f32..2.0,
        ) {
            let at = Vec3::new(1.0, 2.0, 3.0);
            let mut rig = Rig::new();
            rig.effects.mark(at, normal * scale, roll, mark_style(size, 0));

            let drawn = rig.frame(EYE);

            prop_assert_eq!(drawn.len(), 1);
            prop_assert!(
                (translation(&drawn[0]) - (at + normal * 0.01)).length() < 1e-4,
                "centred a hundredth off the surface"
            );
            prop_assert!(drawn[0].model.iter().flatten().all(|v| v.is_finite()));
            let (x, y, z) = (x_axis(&drawn[0]), y_axis(&drawn[0]), z_axis(&drawn[0]));
            prop_assert!((x.length() - size).abs() < 1e-3);
            prop_assert!((y.length() - size).abs() < 1e-3);
            prop_assert!(x.dot(y).abs() < 1e-3, "X and Y are orthogonal");
            prop_assert!(x.dot(normal).abs() < 1e-3, "X lies in the surface");
            prop_assert!(y.dot(normal).abs() < 1e-3, "Y lies in the surface");
            prop_assert!((z - normal).length() < 1e-3, "Z is the normal");
        }
    }
}
