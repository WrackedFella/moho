//! Engine-drawn effects, placed by handle: a game starts one, places it every frame and
//! ends it; the engine owns its life, fade, cap and unshadowed draw.

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

/// A camera-facing four-point star that shrinks linearly over its lifetime.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FlashStyle {
    pub size: f32,
    pub material: u32,
    pub lifetime: Duration,
}

/// A flat quad on a surface: full size until `lifetime - fade`, then shrinking to nothing.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MarkStyle {
    pub size: f32,
    pub material: u32,
    pub lifetime: Duration,
    pub fade: Duration,
}

/// How far a mark floats off its surface along the normal.
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

/// What a fixed-lifetime effect looks like and how it is oriented.
#[derive(Debug, Clone, Copy)]
enum Shape {
    /// Faces the eye, rolled about the eye direction.
    Flash { roll: f32 },
    /// Lies along the unit `normal`, rolled about it.
    Mark { normal: Vec3, roll: f32 },
}

/// A flash or mark: it shrinks over its last `fade` and is gone at `lifetime`.
#[derive(Debug)]
struct Fixed {
    shape: Shape,
    at: Vec3,
    size: f32,
    material: u32,
    lifetime: Duration,
    /// At most `lifetime`; the whole lifetime for a flash.
    fade: Duration,
    age: Duration,
}

impl Fixed {
    /// A new effect at age zero; `fade` is clamped to `lifetime`.
    fn new(
        shape: Shape,
        at: Vec3,
        size: f32,
        material: u32,
        lifetime: Duration,
        fade: Duration,
    ) -> Self {
        Self {
            shape,
            at,
            size,
            material,
            lifetime,
            fade: fade.min(lifetime),
            age: Duration::ZERO,
        }
    }

    /// Whether it can ever draw: finite placement, positive size and lifetime.
    fn is_drawable(&self) -> bool {
        let roll = match self.shape {
            Shape::Flash { roll } | Shape::Mark { roll, .. } => roll,
        };
        self.at.is_finite()
            && roll.is_finite()
            && self.size.is_finite()
            && self.size > 0.0
            && !self.lifetime.is_zero()
    }

    /// Where its unit mesh is drawn, or `None` if it has no facing toward `eye`.
    fn transform(&self, eye: Vec3) -> Option<Affine3A> {
        let (facing, roll, centre) = match self.shape {
            Shape::Flash { roll } => ((eye - self.at).try_normalize()?, roll, self.at),
            Shape::Mark { normal, roll } => (normal, roll, self.at + normal * MARK_OFFSET),
        };
        let x = Quat::from_axis_angle(facing, roll) * facing.any_orthonormal_vector();
        let y = facing.cross(x);
        let size = self.size * self.scale();
        Some(Affine3A::from_cols(
            (x * size).into(),
            (y * size).into(),
            facing.into(),
            centre.into(),
        ))
    }

    /// The size factor: 1 until `lifetime - fade`, then falling linearly to 0.
    fn scale(&self) -> f32 {
        let remaining = self.lifetime.saturating_sub(self.age);
        if remaining >= self.fade {
            return 1.0;
        }
        remaining.as_secs_f32() / self.fade.as_secs_f32()
    }
}

#[derive(Debug)]
enum Effect {
    Streak(Streak),
    Fixed(Fixed),
}

/// Mesh slots, offsets from `EFFECT_MESH_IDS_FROM`.
const STREAK_SLOT: usize = 0;
const FLASH_SLOT: usize = 1;
const MARK_SLOT: usize = 2;

/// The live effects: one store, one handle counter, one cap.
#[derive(Debug)]
pub struct Effects {
    store: BTreeMap<EffectHandle, Effect>,
    next_handle: u64,
    cap: usize,
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

    /// Spawn a flash at `at`, rolled by `roll` radians (counter-clockwise seen from the
    /// eye) about the eye direction. Refused for a non-finite or non-positive size or
    /// lifetime, and when the cap is full of streaks.
    pub fn flash(&mut self, at: Vec3, roll: f32, style: FlashStyle) {
        self.spawn_fixed(Fixed::new(
            Shape::Flash { roll },
            at,
            style.size,
            style.material,
            style.lifetime,
            style.lifetime,
        ));
    }

    /// Spawn a mark at `at` lying along `normal`, rolled by `roll` radians about it.
    /// Refused for a zero or non-finite normal, and under the conditions of [`Self::flash`].
    pub fn mark(&mut self, at: Vec3, normal: Vec3, roll: f32, style: MarkStyle) {
        let Some(normal) = normal.try_normalize() else {
            return;
        };
        self.spawn_fixed(Fixed::new(
            Shape::Mark { normal, roll },
            at,
            style.size,
            style.material,
            style.lifetime,
            style.fade,
        ));
    }

    /// Store a fixed-lifetime effect, dropping the oldest such effect when the cap is full.
    fn spawn_fixed(&mut self, fixed: Fixed) {
        if !fixed.is_drawable() || self.cap == 0 {
            return;
        }
        let streaks = self
            .store
            .values()
            .filter(|e| matches!(e, Effect::Streak(_)))
            .count();
        if streaks >= self.cap {
            return;
        }
        while self.store.len() >= self.cap {
            let oldest = self
                .store
                .iter()
                .find_map(|(handle, e)| matches!(e, Effect::Fixed(_)).then_some(*handle));
            let Some(oldest) = oldest else {
                return;
            };
            self.store.remove(&oldest);
        }
        let handle = EffectHandle(self.next_handle);
        self.next_handle += 1;
        self.store.insert(handle, Effect::Fixed(fixed));
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
                Effect::Fixed(fixed) => fixed.age += dt,
            }
        }
        self.store.retain(|_, effect| match effect {
            Effect::Streak(streak) => !streak.finished_fading(),
            Effect::Fixed(fixed) => fixed.age < fixed.lifetime,
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
            let (slot, transform, material) = match effect {
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
                    (STREAK_SLOT, transform, streak.style.material)
                }
                Effect::Fixed(fixed) => {
                    let slot = match fixed.shape {
                        Shape::Flash { .. } => FLASH_SLOT,
                        Shape::Mark { .. } => MARK_SLOT,
                    };
                    (slot, fixed.transform(eye), fixed.material)
                }
            };
            let Some(transform) = transform else {
                continue;
            };
            let id = WorldMeshId(EFFECT_MESH_IDS_FROM + slot as u64);
            if !std::mem::replace(&mut self.mesh_uploaded[slot], true) {
                let mesh = match slot {
                    STREAK_SLOT => streak_mesh(),
                    FLASH_SLOT => star_mesh(),
                    _ => quad_mesh(),
                };
                meshes.upsert_movable(id, mesh);
            }
            meshes.place_unshadowed(id, transform, material);
        }
    }
}

/// A flat mesh in the XY plane facing +Z; every vertex has the same lighting inputs.
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
    .expect("every channel has one entry per position and the indices are in range")
}

/// A four-point star with its tips at distance 0.5 on the axes, counter-clockwise from +Z.
fn star_mesh() -> WorldMesh {
    const WAIST: f32 = 0.15;
    flat_mesh(
        vec![
            [0.5, 0.0, 0.0],
            [WAIST, WAIST, 0.0],
            [0.0, 0.5, 0.0],
            [-WAIST, WAIST, 0.0],
            [-0.5, 0.0, 0.0],
            [-WAIST, -WAIST, 0.0],
            [0.0, -0.5, 0.0],
            [WAIST, -WAIST, 0.0],
        ],
        vec![7, 0, 1, 1, 2, 3, 3, 4, 5, 5, 6, 7, 1, 3, 5, 1, 5, 7],
    )
}

/// A unit square centred on the origin, counter-clockwise from +Z.
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
    use glam::Quat;
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

    fn z_axis(instance: &InstanceGpu) -> Vec3 {
        let [x, y, z, _] = instance.model[2];
        Vec3::new(x, y, z)
    }

    fn flash_style(size: f32, lifetime: Duration) -> FlashStyle {
        FlashStyle {
            size,
            material: 5,
            lifetime,
        }
    }

    fn mark_style(size: f32) -> MarkStyle {
        MarkStyle {
            size,
            material: 6,
            lifetime: Duration::from_secs(10),
            fade: Duration::from_secs(1),
        }
    }

    /// One frame's draw with the backend mesh handle each instance uses.
    fn frame_meshes(rig: &mut Rig, eye: Vec3) -> Vec<(u32, InstanceGpu)> {
        rig.effects.draw(&mut rig.meshes, eye);
        rig.meshes.flush(&mut rig.backend);
        let drawn = rig.meshes.unshadowed_draws().collect();
        rig.meshes.clear_placements();
        drawn
    }

    /// The registered mesh's vertices carried through the instance's transform.
    fn world_points(rig: &Rig, mesh: u32, instance: &InstanceGpu) -> Vec<Vec3> {
        let index = usize::try_from(mesh).expect("a small handle") - 1;
        rig.backend.registered[index]
            .0
            .iter()
            .map(|p| {
                x_axis(instance) * p[0]
                    + y_axis(instance) * p[1]
                    + z_axis(instance) * p[2]
                    + translation(instance)
            })
            .collect()
    }

    fn centre(points: &[Vec3]) -> Vec3 {
        let (lo, hi) = bounds(points, |p| p);
        (lo + hi) * 0.5
    }

    fn bounds(points: &[Vec3], f: impl Fn(Vec3) -> Vec3) -> (Vec3, Vec3) {
        let mut lo = Vec3::splat(f32::MAX);
        let mut hi = Vec3::splat(f32::MIN);
        for p in points {
            lo = lo.min(f(*p));
            hi = hi.max(f(*p));
        }
        (lo, hi)
    }

    fn extent_along(points: &[Vec3], dir: Vec3) -> f32 {
        let dots: Vec<f32> = points.iter().map(|p| p.dot(dir)).collect();
        let max = dots.iter().copied().fold(f32::MIN, f32::max);
        let min = dots.iter().copied().fold(f32::MAX, f32::min);
        max - min
    }

    /// Extents of the first drawn mark along `u` and along `normal x u`.
    fn mark_extent(rig: &mut Rig, normal: Vec3) -> Option<(f32, f32)> {
        let drawn = frame_meshes(rig, EYE);
        let (mesh, instance) = drawn.first()?;
        let points = world_points(rig, *mesh, instance);
        let u = normal.any_orthonormal_vector();
        Some((
            extent_along(&points, u),
            extent_along(&points, normal.cross(u)),
        ))
    }

    #[test]
    fn a_flash_shrinks_linearly_and_expires() {
        let mut rig = Rig::new();
        rig.effects
            .flash(Vec3::new(1.0, 2.0, 3.0), 0.0, flash_style(0.4, ms(50)));

        let full = flash_extents(&mut rig, Vec3::new(1.0, 2.0, 3.0));
        rig.effects.advance(ms(25));
        let half = flash_extents(&mut rig, Vec3::new(1.0, 2.0, 3.0));
        rig.effects.advance(ms(25));
        let gone = flash_extents(&mut rig, Vec3::new(1.0, 2.0, 3.0));

        for (label, got, size) in [("spawn", full, 0.4), ("25ms", half, 0.2)] {
            let (material, across_x, across_y, along_eye) =
                got.unwrap_or_else(|| panic!("drawn at {label}"));
            assert_eq!(material, 5);
            assert!(
                (across_x - size).abs() < EPS,
                "{label}: x extent {across_x}"
            );
            assert!(
                (across_y - size).abs() < EPS,
                "{label}: y extent {across_y}"
            );
            assert!(along_eye < EPS, "{label}: the star is flat");
        }
        assert_eq!(gone, None);
        assert_eq!(rig.effects.live(), 0);
    }

    /// `(material, extent along X, extent along Y, extent along the eye direction)` of the
    /// first drawn instance's world-space star.
    fn flash_extents(rig: &mut Rig, at: Vec3) -> Option<(u32, f32, f32, f32)> {
        let drawn = frame_meshes(rig, EYE);
        let (mesh, instance) = drawn.first()?;
        let points = world_points(rig, *mesh, instance);
        Some((
            instance.material,
            extent_along(&points, x_axis(instance).normalize()),
            extent_along(&points, y_axis(instance).normalize()),
            extent_along(&points, (EYE - at).normalize()),
        ))
    }

    #[test]
    fn a_flash_faces_the_eye() {
        let at = Vec3::new(1.0, 2.0, 3.0);
        let mut rig = Rig::new();
        rig.effects.flash(at, 0.0, flash_style(0.4, ms(50)));
        let mut rolled = Rig::new();
        rolled
            .effects
            .flash(at, std::f32::consts::FRAC_PI_2, flash_style(0.4, ms(50)));

        let plain = rig.frame(EYE);
        let turned = rolled.frame(EYE);

        assert_eq!(plain.len(), 1);
        assert_eq!(turned.len(), 1);
        let (x, y) = (x_axis(&plain[0]), y_axis(&plain[0]));
        assert!(x.dot(y).abs() < EPS, "in-plane axes are perpendicular");
        assert!(x.cross(y).dot(EYE - at) > 0.0, "X x Y points to the eye");
        assert_near(translation(&plain[0]), at);
        assert_near(x_axis(&turned[0]), y);
        assert_near(y_axis(&turned[0]), -x);
        assert_near(
            z_axis(&turned[0]).normalize(),
            z_axis(&plain[0]).normalize(),
        );
        assert_near(z_axis(&plain[0]).normalize(), (EYE - at).normalize());

        let other_eye = Vec3::new(-8.0, -5.0, 11.0);
        let mut moved = Rig::new();
        moved.effects.flash(at, 0.0, flash_style(0.4, ms(50)));
        let seen = moved.frame(other_eye);
        assert_eq!(seen.len(), 1);
        assert_near(z_axis(&seen[0]).normalize(), (other_eye - at).normalize());
    }

    #[test]
    fn a_mark_lies_along_its_normal_offset_by_the_mark_offset() {
        let at = Vec3::new(4.0, 1.0, -2.0);
        let normal = Vec3::new(1.0, 2.0, 2.0).normalize();
        let roll = 0.7;
        let mut rig = Rig::new();
        rig.effects.mark(at, normal, roll, mark_style(0.2));

        let drawn = frame_meshes(&mut rig, EYE);

        assert_eq!(drawn.len(), 1);
        let (mesh, instance) = &drawn[0];
        assert_eq!(instance.material, 6);
        assert_near(z_axis(instance).normalize(), normal);
        let points = world_points(&rig, *mesh, instance);
        assert_near(centre(&points), at + normal * MARK_OFFSET);
        let u = Quat::from_axis_angle(normal, roll) * normal.any_orthonormal_vector();
        let v = normal.cross(u);
        assert!((extent_along(&points, u) - 0.2).abs() < EPS);
        assert!((extent_along(&points, v) - 0.2).abs() < EPS);
        assert!(extent_along(&points, normal) < EPS, "the quad is flat");
        assert!(
            (x_axis(instance).normalize().dot(u) - 1.0).abs() < EPS,
            "X is the rolled orthonormal vector"
        );
    }

    #[test]
    fn a_mark_holds_full_size_then_shrinks_out_in_its_fade() {
        let normal = Vec3::Y;
        let mut rig = Rig::new();
        rig.effects.mark(Vec3::ZERO, normal, 0.0, mark_style(0.2));

        let young = mark_extent(&mut rig, normal);
        rig.effects.advance(Duration::from_secs(9));
        let full = mark_extent(&mut rig, normal);
        rig.effects.advance(ms(500));
        let half = mark_extent(&mut rig, normal);
        rig.effects.advance(ms(500));
        let gone = mark_extent(&mut rig, normal);

        for (label, got, size) in [
            ("spawn", young, 0.2),
            ("9s", full, 0.2),
            ("9.5s", half, 0.1),
        ] {
            let (along_u, along_v) = got.unwrap_or_else(|| panic!("drawn at {label}"));
            assert!((along_u - size).abs() < EPS, "{label}: u extent {along_u}");
            assert!((along_v - size).abs() < EPS, "{label}: v extent {along_v}");
        }
        assert_eq!(gone, None);
        assert_eq!(rig.effects.live(), 0);
    }

    #[test]
    fn a_full_cap_drops_the_oldest_fixed_lifetime_effect() {
        let mut rig = Rig::new();
        rig.effects.set_quality(1);
        for i in 0..32u8 {
            let at = Vec3::new(f32::from(i), 0.0, 0.0);
            rig.effects.mark(at, Vec3::Y, 0.0, mark_style(0.2));
        }
        assert_eq!(rig.effects.live(), 32);

        rig.effects
            .flash(Vec3::new(100.0, 0.0, 0.0), 0.0, flash_style(0.4, ms(50)));

        assert_eq!(rig.effects.live(), 32);
        let drawn = frame_meshes(&mut rig, EYE);
        assert_eq!(drawn.len(), 32);
        let mut xs: Vec<f32> = drawn.iter().map(|(_, i)| translation(i).x).collect();
        xs.sort_by(f32::total_cmp);
        assert!(xs[0] >= 0.5, "the oldest (x = 0) was dropped, got {xs:?}");
        assert!(xs[31] > 99.0, "the new flash is admitted");
    }

    #[test]
    fn a_full_cap_of_streaks_refuses_a_mark_and_keeps_every_streak() {
        let mut rig = Rig::new();
        rig.effects.set_quality(1);
        let handles: Vec<EffectHandle> = (0..32)
            .map(|_| rig.effects.begin_streak(style(2.0, 0.1)))
            .collect();

        rig.effects.mark(Vec3::ZERO, Vec3::Y, 0.0, mark_style(0.2));
        rig.effects.flash(Vec3::ZERO, 0.0, flash_style(0.4, ms(50)));

        assert_eq!(rig.effects.live(), 32);
        for (i, handle) in (0u8..).zip(&handles) {
            let head = Vec3::new(f32::from(i) * 10.0, 0.0, 0.0);
            rig.effects.place_streak(*handle, head, Vec3::X);
        }
        let drawn = frame_meshes(&mut rig, EYE);
        assert_eq!(drawn.len(), 32, "every streak stays");
        let streak_mesh_handle = drawn[0].0;
        assert!(
            drawn.iter().all(|(mesh, _)| *mesh == streak_mesh_handle),
            "only streaks draw"
        );
        let mut heads: Vec<f32> = drawn.iter().map(|(_, i)| head_of(i).x).collect();
        heads.sort_by(f32::total_cmp);
        let expected: Vec<f32> = (0u8..32).map(|i| f32::from(i) * 10.0).collect();
        assert_eq!(heads, expected, "every streak head is present");
        assert_eq!(rig.backend.registered.len(), 1, "only the streak mesh");
        assert_eq!(rig.backend.registered[0].0.len(), 4, "and it is the kite");
    }

    #[test]
    fn a_refused_mark_is_never_queued_for_when_streaks_end() {
        let mut rig = Rig::new();
        rig.effects.set_quality(1);
        let handles: Vec<EffectHandle> = (0..32)
            .map(|_| rig.effects.begin_streak(style(2.0, 0.1)))
            .collect();
        rig.effects.mark(Vec3::ZERO, Vec3::Y, 0.0, mark_style(0.2));
        for handle in &handles {
            rig.effects.end(*handle);
        }

        rig.effects.advance(Duration::from_secs(1));

        assert_eq!(rig.effects.live(), 0, "the refused mark never queued");
        rig.effects.mark(Vec3::ZERO, Vec3::Y, 0.0, mark_style(0.2));
        assert_eq!(rig.effects.live(), 1, "room again, so a mark is admitted");
        assert_eq!(frame_meshes(&mut rig, EYE).len(), 1);
    }

    /// Each case breaks exactly one input: `(label, at, roll, size, lifetime)`.
    fn single_bad_inputs() -> Vec<(&'static str, Vec3, f32, f32, Duration)> {
        let at = Vec3::new(1.0, 2.0, 3.0);
        let life = Duration::from_secs(10);
        vec![
            ("NaN at", Vec3::new(f32::NAN, 2.0, 3.0), 0.3, 0.2, life),
            (
                "infinite at",
                Vec3::new(1.0, f32::INFINITY, 3.0),
                0.3,
                0.2,
                life,
            ),
            ("NaN roll", at, f32::NAN, 0.2, life),
            ("infinite roll", at, f32::INFINITY, 0.2, life),
            ("NaN size", at, 0.3, f32::NAN, life),
            ("infinite size", at, 0.3, f32::INFINITY, life),
            ("zero size", at, 0.3, 0.0, life),
            ("negative size", at, 0.3, -0.2, life),
            ("zero lifetime", at, 0.3, 0.2, Duration::ZERO),
        ]
    }

    #[test]
    fn a_flash_with_any_single_bad_input_is_refused() {
        for (label, at, roll, size, lifetime) in single_bad_inputs() {
            let mut rig = Rig::new();
            let mut flash = flash_style(size, lifetime);
            flash.lifetime = lifetime;

            rig.effects.flash(at, roll, flash);

            assert_eq!(rig.effects.live(), 0, "{label}");
            assert!(frame_meshes(&mut rig, EYE).is_empty(), "{label}");
        }
        let mut control = Rig::new();
        control
            .effects
            .flash(Vec3::new(1.0, 2.0, 3.0), 0.3, flash_style(0.2, ms(50)));
        assert_eq!(control.effects.live(), 1, "the valid control is admitted");
    }

    #[test]
    fn a_mark_with_any_single_bad_input_is_refused() {
        for (label, at, roll, size, lifetime) in single_bad_inputs() {
            let mut rig = Rig::new();
            let style = MarkStyle {
                size,
                material: 6,
                lifetime,
                fade: Duration::ZERO,
            };

            rig.effects.mark(at, Vec3::Y, roll, style);

            assert_eq!(rig.effects.live(), 0, "{label}");
            assert!(frame_meshes(&mut rig, EYE).is_empty(), "{label}");
        }
        let mut control = Rig::new();
        control
            .effects
            .mark(Vec3::new(1.0, 2.0, 3.0), Vec3::Y, 0.3, mark_style(0.2));
        assert_eq!(control.effects.live(), 1, "the valid control is admitted");
    }

    #[test]
    fn a_refused_spawn_evicts_nothing() {
        let mut rig = Rig::new();
        for _ in 0..40 {
            rig.effects.begin_streak(style(2.0, 0.1));
        }
        for i in 0..3u8 {
            let at = Vec3::new(f32::from(i), 0.0, 5.0);
            rig.effects.mark(at, Vec3::Y, 0.0, mark_style(0.2));
        }
        assert_eq!(rig.effects.live(), 43);
        rig.effects.set_quality(1);

        rig.effects
            .mark(Vec3::new(100.0, 0.0, 5.0), Vec3::Y, 0.0, mark_style(0.2));

        assert_eq!(rig.effects.live(), 43, "refused, and nothing was evicted");
        let drawn = frame_meshes(&mut rig, EYE);
        let mut xs: Vec<f32> = drawn.iter().map(|(_, i)| translation(i).x).collect();
        xs.sort_by(f32::total_cmp);
        assert_eq!(xs.len(), 3, "the three marks still draw");
        assert!(xs[0] < 0.5 && xs[2] < 3.0, "the original marks, got {xs:?}");
    }

    #[test]
    fn a_full_mixed_cap_drops_the_oldest_mark_and_keeps_the_streak() {
        let mut rig = Rig::new();
        rig.effects.set_quality(1);
        let streak = rig.effects.begin_streak(style(2.0, 0.1));
        rig.effects
            .place_streak(streak, Vec3::new(500.0, 0.0, 0.0), Vec3::X);
        for i in 0..31u8 {
            let at = Vec3::new(f32::from(i), 0.0, 5.0);
            rig.effects.mark(at, Vec3::Y, 0.0, mark_style(0.2));
        }
        assert_eq!(rig.effects.live(), 32);

        rig.effects
            .mark(Vec3::new(100.0, 0.0, 5.0), Vec3::Y, 0.0, mark_style(0.2));

        assert_eq!(rig.effects.live(), 32);
        let drawn = frame_meshes(&mut rig, EYE);
        assert_eq!(drawn.len(), 32);
        assert!(
            drawn
                .iter()
                .any(|(_, i)| (head_of(i).x - 500.0).abs() < EPS),
            "the streak still draws"
        );
        let mut mark_xs: Vec<f32> = drawn
            .iter()
            .map(|(_, i)| translation(i))
            .filter(|t| (t.z - 5.0).abs() < 0.3)
            .map(|t| t.x)
            .collect();
        mark_xs.sort_by(f32::total_cmp);
        assert_eq!(mark_xs.len(), 31);
        assert!(
            mark_xs[0] > 0.5,
            "the first mark (x = 0) was dropped: {mark_xs:?}"
        );
        assert!(mark_xs[30] > 99.0, "the new mark is admitted");
    }

    #[test]
    fn quality_zero_refuses_flash_and_mark() {
        let mut rig = Rig::new();
        rig.effects.set_quality(0);

        rig.effects.flash(Vec3::ZERO, 0.0, flash_style(0.4, ms(50)));
        rig.effects.mark(Vec3::ZERO, Vec3::Y, 0.0, mark_style(0.2));

        assert_eq!(rig.effects.live(), 0);
        assert!(rig.frame(EYE).is_empty());
        rig.effects.set_quality(4);
        assert!(rig.frame(EYE).is_empty(), "nothing was queued while off");
        rig.effects.mark(Vec3::ZERO, Vec3::Y, 0.0, mark_style(0.2));
        assert_eq!(rig.frame(EYE).len(), 1, "spawning works again at quality 4");
    }

    #[test]
    fn set_quality_zero_clears_flashes_and_marks() {
        let mut rig = Rig::new();
        rig.effects.flash(Vec3::ZERO, 0.0, flash_style(0.4, ms(50)));
        rig.effects.mark(Vec3::ZERO, Vec3::Y, 0.0, mark_style(0.2));
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
                .flash(Vec3::new(1.0, 2.0, 3.0), 0.3, flash_style(0.4, ms(50)));
            rig.effects
                .mark(Vec3::new(4.0, 0.0, 1.0), Vec3::Y, 1.1, mark_style(0.2));
            rig.effects.advance(ms(20));
            rig.frame(EYE)
        };

        let first = run();
        let second = run();

        assert_eq!(first.len(), 2);
        assert_eq!(first.len(), second.len());
        for (a, b) in first.iter().zip(&second) {
            assert_eq!(a.model, b.model);
            assert_eq!(a.material, b.material);
        }
    }

    #[test]
    fn flash_and_mark_upload_their_meshes_once_each() {
        let mut rig = Rig::new();
        for frame in 0..3u8 {
            let at = Vec3::splat(f32::from(frame));
            rig.effects.flash(at, 0.0, flash_style(0.4, ms(500)));
            rig.effects.mark(at, Vec3::Y, 0.0, mark_style(0.2));
            frame_meshes(&mut rig, EYE);
        }

        let mut vertex_counts: Vec<usize> =
            rig.backend.registered.iter().map(|m| m.0.len()).collect();
        vertex_counts.sort_unstable();
        assert_eq!(
            vertex_counts,
            vec![4, 8],
            "a unit quad and an 8-vertex star"
        );
        for (positions, indices) in &rig.backend.registered {
            assert!(
                positions.iter().all(|p| p[2] == 0.0),
                "the mesh lies in the XY plane"
            );
            assert!(!indices.is_empty() && indices.len() % 3 == 0);
            for triangle in indices.chunks(3) {
                let [a, b, c] = [0, 1, 2].map(|i| Vec3::from(positions[triangle[i] as usize]));
                assert!(
                    (b - a).cross(c - a).z > 0.0,
                    "triangle {triangle:?} of a {}-vertex mesh is counter-clockwise seen from +Z",
                    positions.len()
                );
            }
        }
    }

    #[test]
    fn effect_meshes_stay_inside_the_reserved_id_range() {
        let mut rig = Rig::new();
        for offset in [1, 2] {
            let id = WorldMeshId(EFFECT_MESH_IDS_FROM - offset);
            rig.meshes.upsert(id, streak_mesh(), 0);
        }
        let streak = rig.effects.begin_streak(style(2.0, 0.1));
        rig.effects.place_streak(streak, Vec3::X * 5.0, Vec3::X);
        rig.effects.flash(Vec3::Z, 0.0, flash_style(0.4, ms(50)));
        rig.effects.mark(Vec3::Y, Vec3::Y, 0.0, mark_style(0.2));

        frame_meshes(&mut rig, EYE);

        assert_eq!(
            rig.meshes.live_len(),
            5,
            "two game meshes and three effect meshes, none replacing another"
        );
    }

    #[test]
    fn flashes_marks_and_streaks_coexist_in_one_frame() {
        let mut rig = Rig::new();
        let streak = rig.effects.begin_streak(style(2.0, 0.1));
        rig.effects.place_streak(streak, Vec3::X * 5.0, Vec3::X);
        rig.effects.flash(Vec3::Z, 0.0, flash_style(0.4, ms(50)));
        rig.effects.mark(Vec3::Y, Vec3::Y, 0.0, mark_style(0.2));

        let drawn = frame_meshes(&mut rig, EYE);

        assert_eq!(rig.effects.live(), 3);
        assert_eq!(drawn.len(), 3);
        let meshes: std::collections::BTreeSet<u32> = drawn.iter().map(|d| d.0).collect();
        assert_eq!(meshes.len(), 3, "each kind draws its own mesh");
        assert_eq!(rig.backend.registered.len(), 3);
    }

    proptest! {
        #[test]
        fn a_mark_basis_is_finite_for_any_normal(
            pick in 0usize..7,
            arbitrary in (-1.0f32..1.0, -1.0f32..1.0, -1.0f32..1.0),
            roll in -10.0f32..10.0,
        ) {
            let axes = [Vec3::X, Vec3::NEG_X, Vec3::Y, Vec3::NEG_Y, Vec3::Z, Vec3::NEG_Z];
            let normal = if let Some(axis) = axes.get(pick) {
                *axis
            } else {
                let v = Vec3::new(arbitrary.0, arbitrary.1, arbitrary.2);
                prop_assume!(v.length() > 0.1);
                v.normalize()
            };
            let mut rig = Rig::new();
            rig.effects.mark(Vec3::ZERO, normal, roll, mark_style(1.0));

            let drawn = frame_meshes(&mut rig, EYE);

            prop_assert_eq!(drawn.len(), 1);
            let (x, y, z) = (x_axis(&drawn[0].1), y_axis(&drawn[0].1), z_axis(&drawn[0].1));
            for axis in [x, y, z, translation(&drawn[0].1)] {
                prop_assert!(axis.is_finite());
            }
            prop_assert!((x.length() - 1.0).abs() < 1e-3);
            prop_assert!((y.length() - 1.0).abs() < 1e-3);
            prop_assert!((z.length() - 1.0).abs() < 1e-3);
            prop_assert!(x.dot(y).abs() < 1e-3);
            prop_assert!(x.dot(z).abs() < 1e-3);
            prop_assert!(y.dot(z).abs() < 1e-3);
            prop_assert!((z - normal).length() < 1e-3);
            prop_assert!((x.cross(y) - z).length() < 1e-3, "right-handed");
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
