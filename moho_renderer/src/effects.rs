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

/// How far a mark floats off the surface along its normal, to avoid z-fighting.
const MARK_OFFSET: f32 = 0.01;

/// Mesh ids of the built-in effect meshes, as offsets from `EFFECT_MESH_IDS_FROM`.
const KITE_SLOT: usize = 0;
const STAR_SLOT: usize = 1;
const QUAD_SLOT: usize = 2;
const MESH_SLOTS: usize = 3;

/// A muzzle flash: a camera-facing star that shrinks linearly to nothing over its lifetime.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FlashStyle {
    /// Extent across the star at spawn.
    pub size: f32,
    pub material: u32,
    /// How long it lives; it is gone once this has passed.
    pub lifetime: Duration,
}

/// An impact mark: a flat square on a surface that holds full size, then shrinks out.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MarkStyle {
    /// Side length at full size.
    pub size: f32,
    pub material: u32,
    /// How long it lives; it is gone once this has passed.
    pub lifetime: Duration,
    /// The last stretch of the lifetime over which it shrinks to nothing; clamped to the
    /// lifetime.
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

/// What a fixed-lifetime effect looks like and where it sits.
#[derive(Debug)]
enum Shape {
    /// Faces the eye; shrinks linearly over the whole lifetime.
    Flash,
    /// Lies along the unit `normal`; holds full size until the last `fade` of its lifetime.
    Mark { normal: Vec3, fade: Duration },
}

/// An effect that lives a fixed time from its spawn and needs no handle.
#[derive(Debug)]
struct Timed {
    shape: Shape,
    at: Vec3,
    roll: f32,
    size: f32,
    material: u32,
    lifetime: Duration,
    age: Duration,
}

impl Timed {
    /// The factor on `size`: 1 at full size, falling to 0 at the end of the lifetime.
    fn scale(&self) -> f32 {
        let remaining = self.lifetime.saturating_sub(self.age).as_secs_f32();
        match self.shape {
            Shape::Flash => remaining / self.lifetime.as_secs_f32(),
            Shape::Mark { fade, .. } => {
                let fade = fade.min(self.lifetime).as_secs_f32();
                if fade == 0.0 {
                    1.0
                } else {
                    (remaining / fade).min(1.0)
                }
            }
        }
    }

    /// Where it is drawn, or `None` when it has no usable orientation (a flash with the eye
    /// on it) or no size left.
    fn transform(&self, eye: Vec3) -> Option<Affine3A> {
        let scale = self.size * self.scale();
        if scale <= 0.0 {
            return None;
        }
        let (facing, centre) = match self.shape {
            Shape::Flash => ((eye - self.at).try_normalize()?, self.at),
            Shape::Mark { normal, .. } => (normal, self.at + normal * MARK_OFFSET),
        };
        let x = facing.any_orthonormal_vector();
        let y = facing.cross(x);
        let (sin, cos) = self.roll.sin_cos();
        Some(Affine3A::from_cols(
            ((x * cos + y * sin) * scale).into(),
            ((y * cos - x * sin) * scale).into(),
            facing.into(),
            centre.into(),
        ))
    }

    fn slot(&self) -> usize {
        match self.shape {
            Shape::Flash => STAR_SLOT,
            Shape::Mark { .. } => QUAD_SLOT,
        }
    }

    fn expired(&self) -> bool {
        self.age >= self.lifetime
    }
}

#[derive(Debug)]
enum Effect {
    /// Followed by a game through its handle.
    Streak(Streak),
    /// Spawned and forgotten; the oldest is dropped when the cap is full.
    Timed(Timed),
}

#[derive(Debug)]
pub struct Effects {
    live: BTreeMap<EffectHandle, Effect>,
    next_handle: u64,
    cap: usize,
    mesh_uploaded: [bool; MESH_SLOTS],
}

impl Default for Effects {
    fn default() -> Self {
        Self {
            live: BTreeMap::new(),
            next_handle: 0,
            cap: CAPS[DEFAULT_QUALITY],
            mesh_uploaded: [false; MESH_SLOTS],
        }
    }
}

impl Effects {
    /// Start a streak. At the quality's cap the handle is inert: never stored, so placing
    /// or ending it does nothing.
    pub fn begin_streak(&mut self, style: StreakStyle) -> EffectHandle {
        let handle = EffectHandle(self.next_handle);
        self.next_handle += 1;
        if self.live.len() < self.cap {
            self.live.insert(
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

    /// Spawn a muzzle flash at `at`, turned by `roll` radians about the line of sight. It
    /// needs no handle: it faces the eye, shrinks and expires on its own. Refused (nothing
    /// stored or dropped) for a non-finite `at` or `roll`, a size that is not finite and
    /// positive, or a zero lifetime.
    pub fn flash(&mut self, at: Vec3, roll: f32, style: FlashStyle) {
        self.spawn_timed(
            Shape::Flash,
            at,
            roll,
            style.size,
            style.material,
            style.lifetime,
        );
    }

    /// Spawn an impact mark at `at` lying on the surface with unit-or-not `normal`, turned
    /// by `roll` radians about it. It holds full size, shrinks over its last `fade` and
    /// expires on its own. Refused like [`flash`](Self::flash), and also for a zero or
    /// non-finite `normal`.
    pub fn mark(&mut self, at: Vec3, normal: Vec3, roll: f32, style: MarkStyle) {
        let Some(normal) = normal.try_normalize() else {
            return;
        };
        let shape = Shape::Mark {
            normal,
            fade: style.fade,
        };
        self.spawn_timed(shape, at, roll, style.size, style.material, style.lifetime);
    }

    fn spawn_timed(
        &mut self,
        shape: Shape,
        at: Vec3,
        roll: f32,
        size: f32,
        material: u32,
        lifetime: Duration,
    ) {
        let usable = at.is_finite()
            && roll.is_finite()
            && size.is_finite()
            && size > 0.0
            && !lifetime.is_zero();
        if !usable || !self.make_room() {
            return;
        }
        let handle = EffectHandle(self.next_handle);
        self.next_handle += 1;
        self.live.insert(
            handle,
            Effect::Timed(Timed {
                shape,
                at,
                roll,
                size,
                material,
                lifetime,
                age: Duration::ZERO,
            }),
        );
    }

    /// Whether one more effect fits, dropping the oldest fixed-lifetime one when the cap is
    /// full. Streaks are never dropped, so a cap full of them refuses.
    fn make_room(&mut self) -> bool {
        if self.live.len() < self.cap {
            return true;
        }
        let oldest = self
            .live
            .iter()
            .find_map(|(handle, effect)| matches!(effect, Effect::Timed(_)).then_some(*handle));
        oldest.is_some_and(|handle| self.live.remove(&handle).is_some())
    }

    /// Place the streak's head for this frame. A zero-length heading keeps the last one;
    /// a stale or unknown handle is ignored.
    pub fn place_streak(&mut self, handle: EffectHandle, head: Vec3, heading: Vec3) {
        let Some(Effect::Streak(streak)) = self.live.get_mut(&handle) else {
            return;
        };
        streak.head = head;
        streak.heading = heading.try_normalize().or(streak.heading);
        streak.placed_this_frame = true;
    }

    /// Start the fade; the streak then draws at its last placement until it is gone.
    pub fn end(&mut self, handle: EffectHandle) {
        if let Some(Effect::Streak(streak)) = self.live.get_mut(&handle) {
            streak.ended_at.get_or_insert(streak.age);
        }
    }

    /// Age every effect by the frame time, start the fade of any past its `max_age` and
    /// drop the finished ones.
    pub fn advance(&mut self, dt: Duration) {
        for effect in self.live.values_mut() {
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
        self.live.retain(|_, effect| match effect {
            Effect::Streak(streak) => !streak.finished_fading(),
            Effect::Timed(timed) => !timed.expired(),
        });
    }

    /// Cap new effects for `quality` (0 off to 4 ultra; higher clamps). Lowering keeps
    /// what is live; 0 clears it.
    pub fn set_quality(&mut self, quality: u32) {
        self.cap = CAPS
            [usize::try_from(quality.min(MAX_QUALITY)).expect("a quality of at most 4 fits usize")];
        if self.cap == 0 {
            self.live.clear();
        }
    }

    /// How many effects are live, fading ones included.
    pub fn live(&self) -> usize {
        self.live.len()
    }

    /// Place every streak placed this frame or fading, and every flash and mark, unshadowed;
    /// streaks and flashes face `eye`.
    pub(crate) fn draw(&mut self, meshes: &mut WorldMeshes, eye: Vec3) {
        for effect in self.live.values_mut() {
            let (slot, material, transform) = match effect {
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
                    (KITE_SLOT, streak.style.material, transform)
                }
                Effect::Timed(timed) => (timed.slot(), timed.material, timed.transform(eye)),
            };
            let Some(transform) = transform else {
                continue;
            };
            let id = WorldMeshId(EFFECT_MESH_IDS_FROM + u64::try_from(slot).expect("a slot fits"));
            if !std::mem::replace(&mut self.mesh_uploaded[slot], true) {
                meshes.upsert_movable(id, built_in_mesh(slot));
            }
            meshes.place_unshadowed(id, transform, material);
        }
    }
}

/// The built-in mesh for a slot: the streak kite, the flash star or the mark quad.
fn built_in_mesh(slot: usize) -> WorldMesh {
    match slot {
        KITE_SLOT => streak_mesh(),
        STAR_SLOT => star_mesh(),
        _ => quad_mesh(),
    }
}

/// Eight vertices alternating between the outer radius 0.5 on the axes and an inner 0.1
/// on the diagonals, in the XY plane, counter-clockwise seen from +Z.
fn star_mesh() -> WorldMesh {
    const OUTER: f32 = 0.5;
    const INNER: f32 = 0.1;
    let positions: Vec<[f32; 3]> = (0..8)
        .map(|i: u8| {
            let angle = std::f32::consts::FRAC_PI_4 * f32::from(i);
            let radius = if i.is_multiple_of(2) { OUTER } else { INNER };
            [radius * angle.cos(), radius * angle.sin(), 0.0]
        })
        .collect();
    // The inner diamond (vertices 1, 3, 5, 7) as two triangles, then one triangle per tip.
    let indices = vec![1, 3, 5, 1, 5, 7, 7, 0, 1, 1, 2, 3, 3, 4, 5, 5, 6, 7];
    WorldMesh::new(
        positions,
        vec![[0.0, 0.0, 1.0]; 8],
        vec![1.0; 8],
        vec![[1.0; 3]; 8],
        vec![1.0; 8],
        vec![0; 8],
        indices,
    )
    .expect("the star's channels are all eight vertices long and its indices are in range")
}

/// A unit square centred on the origin in the XY plane, counter-clockwise seen from +Z.
fn quad_mesh() -> WorldMesh {
    WorldMesh::new(
        vec![
            [-0.5, -0.5, 0.0],
            [0.5, -0.5, 0.0],
            [0.5, 0.5, 0.0],
            [-0.5, 0.5, 0.0],
        ],
        vec![[0.0, 0.0, 1.0]; 4],
        vec![1.0; 4],
        vec![[1.0; 3]; 4],
        vec![1.0; 4],
        vec![0; 4],
        vec![0, 1, 2, 0, 2, 3],
    )
    .expect("the quad's channels are all four vertices long and its indices are in range")
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

    // Convention: an effect's "size" is the length of the model matrix's X column and of its
    // Y column (both unit-extent meshes: the star's outer radius and the quad's half-side are
    // 0.5), so the drawn extent equals size and "half size" is half that column length.
    fn z_axis(instance: &InstanceGpu) -> Vec3 {
        let [x, y, z, _] = instance.model[2];
        Vec3::new(x, y, z)
    }

    fn flash_style(size: f32, lifetime_ms: u64) -> FlashStyle {
        FlashStyle {
            size,
            material: 3,
            lifetime: ms(lifetime_ms),
        }
    }

    fn mark_style(size: f32, lifetime: Duration, fade: Duration) -> MarkStyle {
        MarkStyle {
            size,
            material: 5,
            lifetime,
            fade,
        }
    }

    fn secs(n: f32) -> Duration {
        Duration::from_secs_f32(n)
    }

    fn assert_size(instance: &InstanceGpu, size: f32) {
        for (name, axis) in [("X", x_axis(instance)), ("Y", y_axis(instance))] {
            assert!(
                (axis.length() - size).abs() < EPS,
                "{name} column should be {size} long, got {}",
                axis.length()
            );
        }
    }

    #[test]
    fn a_flash_shrinks_linearly_and_expires() {
        let mut rig = Rig::new();
        let at = Vec3::new(1.0, 2.0, 3.0);
        rig.effects.flash(at, 0.0, flash_style(0.4, 50));

        let full = rig.frame(EYE);

        assert_eq!(rig.effects.live(), 1);
        assert_eq!(full.len(), 1, "drawn without any placement");
        assert_size(&full[0], 0.4);
        assert_near(translation(&full[0]), at);
        assert_eq!(full[0].material, 3);

        rig.effects.advance(ms(25));
        let half = rig.frame(EYE);

        assert_eq!(half.len(), 1);
        assert_size(&half[0], 0.2);
        assert_near(translation(&half[0]), at);

        rig.effects.advance(ms(25));

        assert!(rig.frame(EYE).is_empty());
        assert_eq!(rig.effects.live(), 0);
    }

    #[test]
    fn a_flash_faces_the_eye() {
        let at = Vec3::new(1.0, 2.0, 3.0);
        let eyes = [
            EYE,
            Vec3::new(-20.0, 4.0, 3.0),
            at + Vec3::Y * 10.0,
            at - Vec3::Y * 10.0,
            at + Vec3::Z * 10.0,
        ];
        for eye in eyes {
            for roll in [0.0, 1.0, -2.5] {
                let mut rig = Rig::new();
                rig.effects.flash(at, roll, flash_style(0.4, 50));

                let drawn = rig.frame(eye);

                assert_eq!(drawn.len(), 1);
                let (x, y) = (x_axis(&drawn[0]), y_axis(&drawn[0]));
                assert!(x.dot(y).abs() < EPS, "X and Y are perpendicular");
                assert_size(&drawn[0], 0.4);
                assert!(
                    x.cross(y).normalize().dot((eye - at).normalize()) > 1.0 - EPS,
                    "faces away from {eye}, roll {roll}"
                );
            }
        }
    }

    #[test]
    fn a_flash_follows_the_eye_it_is_drawn_from_each_frame() {
        let at = Vec3::new(1.0, 2.0, 3.0);
        let mut rig = Rig::new();
        rig.effects.flash(at, 0.4, flash_style(0.4, 50));

        for (frame, eye) in [EYE, Vec3::new(-20.0, 4.0, 3.0), at + Vec3::Y * 10.0]
            .into_iter()
            .enumerate()
        {
            let drawn = rig.frame(eye);

            assert_eq!(drawn.len(), 1, "frame {frame}");
            let normal = x_axis(&drawn[0]).cross(y_axis(&drawn[0])).normalize();
            assert!(
                normal.dot((eye - at).normalize()) > 1.0 - EPS,
                "frame {frame} should face {eye}"
            );
        }
    }

    #[test]
    fn a_flash_with_the_eye_at_or_beside_it_is_finite_or_skipped() {
        let at = Vec3::new(1.0, 2.0, 3.0);
        for eye in [at, at + Vec3::splat(1e-6), at + Vec3::X * 1e-9] {
            let mut rig = Rig::new();
            rig.effects.flash(at, 0.3, flash_style(0.4, 50));

            let drawn = rig.frame(eye);

            assert!(drawn.len() <= 1);
            assert!(
                drawn
                    .iter()
                    .all(|i| i.model.iter().flatten().all(|v| v.is_finite())),
                "eye {eye}"
            );
        }
    }

    #[test]
    fn a_flash_with_the_eye_exactly_at_it_still_counts_as_live() {
        let at = Vec3::new(1.0, 2.0, 3.0);
        let mut rig = Rig::new();
        rig.effects.flash(at, 0.3, flash_style(0.4, 50));

        rig.frame(at);

        assert_eq!(rig.effects.live(), 1);
        assert_eq!(rig.frame(EYE).len(), 1, "drawn again from a usable eye");
    }

    type RefusedCase = (&'static str, Box<dyn Fn(&mut Effects)>);

    fn refused_flash_and_mark_cases() -> Vec<RefusedCase> {
        let long = Duration::from_secs(10);
        let fade = Duration::from_secs(1);
        let at = Vec3::new(1.0, 2.0, 3.0);
        let mut cases: Vec<RefusedCase> = Vec::new();
        for (name, normal) in [
            ("zero normal", Vec3::ZERO),
            ("nan normal", Vec3::new(f32::NAN, 0.0, 1.0)),
            ("infinite normal", Vec3::new(f32::INFINITY, 0.0, 0.0)),
        ] {
            cases.push((
                name,
                Box::new(move |e| e.mark(at, normal, 0.0, mark_style(0.2, long, fade))),
            ));
        }
        for (name, bad) in [
            ("nan at", Vec3::new(f32::NAN, 0.0, 0.0)),
            ("infinite at", Vec3::new(0.0, f32::INFINITY, 0.0)),
        ] {
            cases.push((
                name,
                Box::new(move |e| e.flash(bad, 0.0, flash_style(0.4, 50))),
            ));
            cases.push((
                name,
                Box::new(move |e| e.mark(bad, Vec3::Y, 0.0, mark_style(0.2, long, fade))),
            ));
        }
        for (name, size) in [
            ("zero size", 0.0),
            ("negative size", -0.4),
            ("nan size", f32::NAN),
            ("infinite size", f32::INFINITY),
        ] {
            cases.push((
                name,
                Box::new(move |e| e.flash(at, 0.0, flash_style(size, 50))),
            ));
            cases.push((
                name,
                Box::new(move |e| e.mark(at, Vec3::Y, 0.0, mark_style(size, long, fade))),
            ));
        }
        cases.push((
            "zero flash lifetime",
            Box::new(move |e| e.flash(at, 0.0, flash_style(0.4, 0))),
        ));
        cases.push((
            "zero mark lifetime",
            Box::new(move |e| {
                e.mark(
                    at,
                    Vec3::Y,
                    0.0,
                    mark_style(0.2, Duration::ZERO, Duration::ZERO),
                );
            }),
        ));
        cases
    }

    #[test]
    fn a_flash_or_mark_with_unusable_input_is_refused_and_draws_nothing() {
        for (name, spawn) in refused_flash_and_mark_cases() {
            let mut rig = Rig::new();
            let kept = rig.effects.begin_streak(style(2.0, 0.1));

            spawn(&mut rig.effects);

            assert_eq!(rig.effects.live(), 1, "{name}: not stored");
            assert!(rig.frame(EYE).is_empty(), "{name}: draws nothing");
            rig.effects.place_streak(kept, Vec3::X * 5.0, Vec3::X);
            assert_eq!(rig.frame(EYE).len(), 1, "{name}: only the streak draws");
        }
    }

    #[test]
    fn a_refused_spawn_does_not_evict_at_a_full_cap() {
        for (name, spawn) in refused_flash_and_mark_cases() {
            let mut rig = Rig::new();
            rig.effects.set_quality(1);
            let marks = mark_style(0.2, Duration::from_secs(10), Duration::from_secs(1));
            for i in 0..32u8 {
                rig.effects
                    .mark(Vec3::new(f32::from(i), 0.0, 0.0), Vec3::Y, 0.0, marks);
            }

            spawn(&mut rig.effects);

            assert_eq!(rig.effects.live(), 32, "{name}");
            let xs: Vec<i32> = rig
                .frame(EYE)
                .iter()
                .map(|i| translation(i).x.round() as i32)
                .collect();
            assert!(xs.contains(&0), "{name}: the oldest stays");
        }
    }

    #[test]
    fn a_mark_fade_longer_than_its_lifetime_shrinks_from_spawn() {
        let mut rig = Rig::new();
        rig.effects.mark(
            Vec3::X,
            Vec3::Y,
            0.0,
            mark_style(0.2, Duration::from_secs(4), Duration::from_secs(8)),
        );

        let spawned = rig.frame(EYE);
        rig.effects.advance(Duration::from_secs(2));
        let half = rig.frame(EYE);
        rig.effects.advance(Duration::from_secs(2));
        let gone = rig.frame(EYE);

        assert_eq!(spawned.len(), 1);
        assert_size(&spawned[0], 0.2);
        assert_eq!(half.len(), 1);
        assert_size(&half[0], 0.1);
        assert!(gone.is_empty());
        assert_eq!(rig.effects.live(), 0);
    }

    #[test]
    fn a_mark_with_no_fade_holds_full_size_then_is_gone_at_its_lifetime() {
        let mut rig = Rig::new();
        rig.effects.mark(
            Vec3::X,
            Vec3::Y,
            0.0,
            mark_style(0.2, Duration::from_secs(4), Duration::ZERO),
        );

        rig.effects.advance(secs(3.99));
        let late = rig.frame(EYE);
        rig.effects.advance(secs(0.01));
        let gone = rig.frame(EYE);

        assert_eq!(late.len(), 1);
        assert_size(&late[0], 0.2);
        assert!(late[0].model.iter().flatten().all(|v| v.is_finite()));
        assert!(gone.is_empty());
        assert_eq!(rig.effects.live(), 0);
    }

    #[test]
    fn a_flash_roll_turns_it_about_the_line_of_sight() {
        let at = Vec3::new(1.0, 2.0, 3.0);
        let mut plain = Rig::new();
        plain.effects.flash(at, 0.0, flash_style(0.4, 50));
        let mut rolled = Rig::new();
        rolled
            .effects
            .flash(at, std::f32::consts::FRAC_PI_2, flash_style(0.4, 50));

        let before = plain.frame(EYE);
        let after = rolled.frame(EYE);

        assert_eq!(before.len(), 1);
        assert_eq!(after.len(), 1);
        assert_near(x_axis(&after[0]), y_axis(&before[0]));
        assert_near(y_axis(&after[0]), -x_axis(&before[0]));
    }

    #[test]
    fn a_mark_lies_along_its_normal_offset_by_the_mark_offset() {
        let mut rig = Rig::new();
        let at = Vec3::new(4.0, 1.0, 2.0);
        let normal = Vec3::new(1.0, 2.0, 2.0) / 3.0;
        rig.effects.mark(
            at,
            normal,
            0.0,
            mark_style(0.2, Duration::from_secs(10), Duration::from_secs(1)),
        );

        let drawn = rig.frame(EYE);
        let other_eye = rig.frame(Vec3::new(-30.0, 8.0, -4.0));

        let lift = (translation(&drawn[0]) - at).dot(normal);
        assert!(
            lift > 0.0 && lift < 0.05,
            "centre is lifted off the surface by a small margin, got {lift}"
        );
        assert_eq!(rig.effects.live(), 1);
        assert_eq!(drawn.len(), 1);
        assert_size(&drawn[0], 0.2);
        assert_near(translation(&drawn[0]), at + normal * MARK_OFFSET);
        assert_near(z_axis(&drawn[0]).normalize(), normal);
        assert!(
            x_axis(&drawn[0]).dot(normal).abs() < EPS,
            "X is in the surface"
        );
        assert!(
            y_axis(&drawn[0]).dot(normal).abs() < EPS,
            "Y is in the surface"
        );
        assert!(x_axis(&drawn[0]).dot(y_axis(&drawn[0])).abs() < EPS);
        assert!(
            x_axis(&drawn[0]).cross(y_axis(&drawn[0])).dot(normal) > 0.0,
            "the front face looks along the normal"
        );
        assert_eq!(drawn[0].material, 5);
        assert_eq!(
            other_eye[0].model, drawn[0].model,
            "a mark does not turn to the eye"
        );
    }

    #[test]
    fn the_mark_quad_is_a_unit_square_wound_toward_plus_z() {
        let mut rig = Rig::new();
        rig.effects.mark(
            Vec3::X,
            Vec3::Y,
            0.0,
            mark_style(0.2, Duration::from_secs(10), Duration::from_secs(1)),
        );

        rig.frame(EYE);

        assert_eq!(rig.backend.registered.len(), 1);
        let (positions, indices) = &rig.backend.registered[0];
        assert_quad(positions, indices);
    }

    fn assert_quad(positions: &[[f32; 3]], indices: &[u32]) {
        assert_eq!(positions.len(), 4, "a unit quad");
        assert!(positions.iter().all(|p| p[2] == 0.0));
        assert!(
            positions
                .iter()
                .all(|p| (p[0].abs() - 0.5).abs() < EPS && (p[1].abs() - 0.5).abs() < EPS),
            "a quad of side 1 centred on the origin"
        );
        assert_eq!(indices.len(), 6);
        for triangle in indices.chunks(3) {
            let [a, b, c] = [0, 1, 2].map(|i| Vec3::from(positions[triangle[i] as usize]));
            assert!((b - a).cross(c - a).z > 0.0, "counter-clockwise from +Z");
        }
    }

    fn assert_star(positions: &[[f32; 3]]) {
        assert_eq!(positions.len(), 8, "a four-point star is eight vertices");
        assert!(positions.iter().all(|p| p[2] == 0.0), "in the XY plane");
        let outer = positions
            .iter()
            .map(|p| Vec3::from(*p).length())
            .fold(0.0, f32::max);
        assert!((outer - 0.5).abs() < EPS, "outer radius 0.5, got {outer}");
    }

    #[test]
    fn the_flash_star_is_an_eight_vertex_star_of_outer_radius_one_half() {
        let mut rig = Rig::new();
        rig.effects.flash(Vec3::X, 0.0, flash_style(0.4, 50));

        rig.frame(EYE);

        assert_eq!(rig.backend.registered.len(), 1);
        assert_star(&rig.backend.registered[0].0);
    }

    #[test]
    fn each_effect_kind_uploads_its_mesh_once_under_the_next_reserved_id() {
        let mut rig = Rig::new();
        // The meshes are registered and listed in mesh-id order, so a backend handle of 1, 2
        // and 3 stands for the ids EFFECT_MESH_IDS_FROM, +1 and +2; the spawn order is
        // scrambled so only the ids decide it. Materials tell the kinds apart.
        rig.effects.mark(
            Vec3::Z,
            Vec3::Y,
            0.0,
            mark_style(0.2, Duration::from_secs(10), Duration::from_secs(1)),
        );
        rig.effects.flash(Vec3::Y, 0.0, flash_style(0.4, 5000));
        let handle = rig.effects.begin_streak(StreakStyle::new(2.0, 0.1, 0));

        for _ in 0..2 {
            rig.effects
                .place_streak(handle, Vec3::new(5.0, 0.0, 0.0), Vec3::X);
            rig.effects.draw(&mut rig.meshes, EYE);
            rig.meshes.flush(&mut rig.backend);
            let drawn: Vec<(u32, u32)> = rig
                .meshes
                .unshadowed_draws()
                .map(|(mesh, instance)| (mesh, instance.material))
                .collect();
            rig.meshes.clear_placements();
            assert_eq!(drawn, vec![(1, 0), (2, 3), (3, 5)], "kite, star, quad");
        }

        assert_eq!(rig.backend.registered.len(), 3, "exactly three uploads");
        let kite = &rig.backend.registered[0].0;
        assert_eq!(kite.len(), 4);
        assert!((kite[2][0] - 1.0).abs() < EPS, "the kite's head at x = 1");
        assert_star(&rig.backend.registered[1].0);
        assert_quad(&rig.backend.registered[2].0, &rig.backend.registered[2].1);
    }

    #[test]
    fn a_mark_roll_turns_it_about_its_normal() {
        let at = Vec3::new(4.0, 1.0, 2.0);
        let normal = Vec3::new(1.0, 2.0, 2.0) / 3.0;
        let style = mark_style(0.2, Duration::from_secs(10), Duration::from_secs(1));
        let mut plain = Rig::new();
        plain.effects.mark(at, normal, 0.0, style);
        let mut rolled = Rig::new();
        rolled
            .effects
            .mark(at, normal, std::f32::consts::FRAC_PI_2, style);

        let before = plain.frame(EYE);
        let after = rolled.frame(EYE);

        assert_eq!(before.len(), 1);
        assert_eq!(after.len(), 1);
        assert_near(x_axis(&after[0]), y_axis(&before[0]));
        assert_near(y_axis(&after[0]), -x_axis(&before[0]));
        assert_near(translation(&after[0]), translation(&before[0]));
    }

    #[test]
    fn a_mark_holds_full_size_then_shrinks_out_in_its_fade() {
        let mut rig = Rig::new();
        rig.effects.mark(
            Vec3::new(4.0, 1.0, 2.0),
            Vec3::Y,
            0.0,
            mark_style(0.2, Duration::from_secs(10), Duration::from_secs(1)),
        );

        rig.effects.advance(Duration::from_secs(5));
        let early = rig.frame(EYE);
        rig.effects.advance(Duration::from_secs(4));
        let full = rig.frame(EYE);
        rig.effects.advance(secs(0.5));
        let half = rig.frame(EYE);
        rig.effects.advance(secs(0.5));
        let gone = rig.frame(EYE);

        assert_eq!(early.len(), 1);
        assert_size(&early[0], 0.2);
        assert_eq!(full.len(), 1);
        assert_size(&full[0], 0.2);
        assert_eq!(half.len(), 1);
        assert_size(&half[0], 0.1);
        assert!(gone.is_empty());
        assert_eq!(rig.effects.live(), 0);
    }

    #[test]
    fn a_full_cap_drops_the_oldest_fixed_lifetime_effect() {
        let mut rig = Rig::new();
        rig.effects.set_quality(1);
        let style = mark_style(0.2, Duration::from_secs(10), Duration::from_secs(1));
        for i in 0..32 {
            let x = f32::from(u8::try_from(i).expect("small"));
            rig.effects
                .mark(Vec3::new(x, 0.0, 0.0), Vec3::Y, 0.0, style);
        }
        assert_eq!(rig.effects.live(), 32);

        rig.effects
            .mark(Vec3::new(100.0, 0.0, 0.0), Vec3::Y, 0.0, style);

        assert_eq!(rig.effects.live(), 32);
        let xs: Vec<i32> = rig
            .frame(EYE)
            .iter()
            .map(|i| translation(i).x.round() as i32)
            .collect();
        assert_eq!(xs.len(), 32);
        assert!(!xs.contains(&0), "the oldest is dropped");
        assert!(xs.contains(&1), "the next oldest stays");
        assert!(xs.contains(&100), "the new one is live");
    }

    #[test]
    fn a_full_cap_evicts_a_mark_or_flash_before_any_streak() {
        let mut rig = Rig::new();
        rig.effects.set_quality(1);
        let streak = rig.effects.begin_streak(style(2.0, 0.1));
        let marks = mark_style(0.2, Duration::from_secs(10), Duration::from_secs(1));
        rig.effects
            .flash(Vec3::new(1.0, 1.0, 0.0), 0.0, flash_style(0.4, 5000));
        for i in 2..32 {
            let x = f32::from(u8::try_from(i).expect("small"));
            rig.effects
                .mark(Vec3::new(x, 1.0, 0.0), Vec3::Z, 0.0, marks);
        }
        assert_eq!(rig.effects.live(), 32);

        rig.effects
            .mark(Vec3::new(100.0, 1.0, 0.0), Vec3::Z, 0.0, marks);
        rig.effects
            .place_streak(streak, Vec3::new(50.0, 0.0, 0.0), Vec3::X);

        assert_eq!(rig.effects.live(), 32);
        let xs: Vec<i32> = rig
            .frame(EYE)
            .iter()
            .map(|i| translation(i).x.round() as i32)
            .collect();
        assert_eq!(xs.len(), 32);
        assert!(xs.contains(&48), "the streak, the lowest handle, stays");
        assert!(!xs.contains(&1), "the oldest flash is dropped instead");
        assert!(xs.contains(&2));
        assert!(xs.contains(&100));
    }

    #[test]
    fn a_full_cap_of_streaks_refuses_a_mark_and_keeps_every_streak() {
        let mut rig = Rig::new();
        rig.effects.set_quality(1);
        let handles: Vec<EffectHandle> = (0..32)
            .map(|_| rig.effects.begin_streak(style(2.0, 0.1)))
            .collect();

        rig.effects.mark(
            Vec3::new(9.0, 9.0, 9.0),
            Vec3::Y,
            0.0,
            mark_style(0.2, Duration::from_secs(10), Duration::from_secs(1)),
        );
        rig.effects
            .flash(Vec3::new(9.0, 9.0, 9.0), 0.0, flash_style(0.4, 50));

        assert_eq!(rig.effects.live(), 32);
        for handle in &handles {
            rig.effects.place_streak(*handle, Vec3::X * 5.0, Vec3::X);
        }
        let drawn = rig.frame(EYE);
        assert_eq!(drawn.len(), 32, "every streak stays and nothing else draws");
        for instance in &drawn {
            assert!((x_axis(instance).length() - 2.0).abs() < EPS, "a streak");
        }
    }

    #[test]
    fn quality_zero_refuses_flash_and_mark() {
        let mut rig = Rig::new();
        rig.effects.set_quality(0);

        rig.effects.flash(Vec3::X, 0.0, flash_style(0.4, 50));
        rig.effects.mark(
            Vec3::Y,
            Vec3::Y,
            0.0,
            mark_style(0.2, Duration::from_secs(10), Duration::from_secs(1)),
        );

        assert_eq!(rig.effects.live(), 0);
        assert!(rig.frame(EYE).is_empty());
        rig.effects.set_quality(4);
        assert_eq!(rig.effects.live(), 0, "nothing was queued for later");
        assert!(rig.frame(EYE).is_empty());
    }

    #[test]
    fn quality_zero_clears_a_live_flash_and_mark() {
        let mut rig = Rig::new();
        rig.effects.flash(Vec3::X, 0.0, flash_style(0.4, 50));
        rig.effects.mark(
            Vec3::Y,
            Vec3::Y,
            0.0,
            mark_style(0.2, Duration::from_secs(10), Duration::from_secs(1)),
        );
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
                .flash(Vec3::new(1.0, 2.0, 3.0), 0.7, flash_style(0.4, 50));
            rig.effects.mark(
                Vec3::new(4.0, 5.0, 6.0),
                Vec3::new(0.0, 1.0, 1.0).normalize(),
                1.3,
                mark_style(0.2, Duration::from_secs(10), Duration::from_secs(1)),
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

    fn unit_normal() -> impl Strategy<Value = Vec3> {
        prop_oneof![
            (0usize..6)
                .prop_map(|i| { [Vec3::X, Vec3::Y, Vec3::Z, -Vec3::X, -Vec3::Y, -Vec3::Z][i] }),
            (0.0f32..std::f32::consts::TAU, -1.5f32..1.5).prop_map(|(yaw, pitch)| {
                Vec3::new(
                    pitch.cos() * yaw.cos(),
                    pitch.sin(),
                    pitch.cos() * yaw.sin(),
                )
            }),
        ]
    }

    proptest! {
        #[test]
        fn a_mark_basis_is_finite_for_any_normal(
            normal in unit_normal(),
            roll in -10.0f32..10.0,
            size in 0.01f32..2.0,
        ) {
            let at = Vec3::new(1.0, 2.0, 3.0);
            let mut rig = Rig::new();
            rig.effects.mark(
                at,
                normal,
                roll,
                mark_style(size, Duration::from_secs(10), Duration::from_secs(1)),
            );

            let drawn = rig.frame(EYE);

            prop_assert_eq!(drawn.len(), 1);
            prop_assert!(drawn[0].model.iter().flatten().all(|v| v.is_finite()));
            let (x, y) = (x_axis(&drawn[0]), y_axis(&drawn[0]));
            prop_assert!((x.length() - size).abs() < 1e-3);
            prop_assert!((y.length() - size).abs() < 1e-3);
            prop_assert!(x.dot(normal).abs() < 1e-3);
            prop_assert!(y.dot(normal).abs() < 1e-3);
            prop_assert!(x.dot(y).abs() < 1e-3);
            prop_assert!(x.cross(y).dot(normal) > 0.0);
            prop_assert!((translation(&drawn[0]) - (at + normal * MARK_OFFSET)).length() < 1e-3);
        }
    }

    fn xs_of(drawn: &[InstanceGpu]) -> Vec<i32> {
        drawn
            .iter()
            .map(|i| translation(i).x.round() as i32)
            .collect()
    }

    #[test]
    fn lowering_quality_below_the_live_count_drops_exactly_the_oldest_on_the_next_spawn() {
        let mut rig = Rig::new();
        let marks = mark_style(0.2, Duration::from_secs(10), Duration::from_secs(1));
        for i in 0..40u8 {
            rig.effects
                .mark(Vec3::new(f32::from(i), 0.0, 0.0), Vec3::Y, 0.0, marks);
        }
        rig.effects.set_quality(1);
        assert_eq!(rig.effects.live(), 40);

        rig.effects
            .flash(Vec3::new(100.0, 0.0, 0.0), 0.0, flash_style(0.4, 5000));

        assert_eq!(rig.effects.live(), 40, "one in, exactly one out");
        let xs = xs_of(&rig.frame(EYE));
        assert_eq!(xs.len(), 40);
        assert!(!xs.contains(&0), "the oldest mark no longer draws");
        assert!(xs.contains(&1), "the next oldest stays");
        assert!(xs.contains(&100));
    }

    #[test]
    fn a_flash_one_nanosecond_from_expiry_draws_finite_or_not_at_all_then_is_gone() {
        let mut rig = Rig::new();
        rig.effects.flash(Vec3::X, 0.0, flash_style(0.4, 50));

        rig.effects.advance(ms(49) + Duration::from_nanos(999_999));
        let nearly = rig.frame(EYE);

        assert!(nearly.len() <= 1);
        for instance in &nearly {
            assert!(instance.model.iter().flatten().all(|v| v.is_finite()));
            assert!(x_axis(instance).length() < 1e-6, "about size * 2e-8");
        }
        assert_eq!(rig.effects.live(), 1, "still live");

        rig.effects.advance(Duration::from_nanos(1));

        assert!(rig.frame(EYE).is_empty());
        assert_eq!(rig.effects.live(), 0);
    }

    #[test]
    fn many_small_advances_expire_a_flash_at_the_same_sum_as_one() {
        let mut rig = Rig::new();
        rig.effects.flash(Vec3::X, 0.0, flash_style(0.4, 50));

        for step in 1..=4 {
            rig.effects.advance(ms(10));
            assert_eq!(rig.effects.live(), 1, "step {step}");
            assert_eq!(rig.frame(EYE).len(), 1, "step {step}");
        }
        rig.effects.advance(ms(10));

        assert_eq!(rig.effects.live(), 0);
        assert!(rig.frame(EYE).is_empty());
    }

    #[test]
    fn many_large_advances_expire_a_mark_at_the_same_sum_as_one() {
        let mut rig = Rig::new();
        rig.effects.mark(
            Vec3::X,
            Vec3::Y,
            0.0,
            mark_style(0.2, Duration::from_secs(10), Duration::from_secs(1)),
        );

        for step in 1..=3 {
            rig.effects.advance(Duration::from_secs(3));
            let drawn = rig.frame(EYE);
            assert_eq!(drawn.len(), 1, "step {step}");
            assert_size(&drawn[0], 0.2);
        }
        rig.effects.advance(Duration::from_secs(1));

        assert_eq!(rig.effects.live(), 0);
        assert!(rig.frame(EYE).is_empty());
    }

    #[test]
    fn a_non_unit_mark_normal_faces_unit_and_lifts_by_exactly_the_mark_offset() {
        let mut rig = Rig::new();
        let at = Vec3::new(4.0, 1.0, 2.0);
        rig.effects.mark(
            at,
            Vec3::new(0.0, 0.0, 5.0),
            0.0,
            mark_style(0.2, Duration::from_secs(10), Duration::from_secs(1)),
        );

        let drawn = rig.frame(EYE);

        assert_eq!(drawn.len(), 1);
        assert_near(z_axis(&drawn[0]), Vec3::Z);
        assert_near(translation(&drawn[0]), at + Vec3::Z * MARK_OFFSET);
        assert_size(&drawn[0], 0.2);
    }

    #[test]
    fn a_full_cap_of_streaks_freed_by_one_takes_a_mark_then_a_flash_evicts_the_mark() {
        let mut rig = Rig::new();
        rig.effects.set_quality(1);
        let handles: Vec<EffectHandle> = (0..32)
            .map(|_| rig.effects.begin_streak(style(2.0, 0.1)))
            .collect();
        rig.effects.end(handles[5]);
        rig.effects.advance(ms(60));
        assert_eq!(rig.effects.live(), 31);

        rig.effects.mark(
            Vec3::new(200.0, 0.0, 0.0),
            Vec3::Y,
            0.0,
            mark_style(0.2, Duration::from_secs(10), Duration::from_secs(1)),
        );
        assert_eq!(rig.effects.live(), 32);
        rig.effects
            .flash(Vec3::new(100.0, 0.0, 0.0), 0.0, flash_style(0.4, 5000));

        assert_eq!(rig.effects.live(), 32);
        for (i, handle) in handles.iter().enumerate() {
            if i != 5 {
                rig.effects.place_streak(*handle, Vec3::X * 5.0, Vec3::X);
            }
        }
        let xs = xs_of(&rig.frame(EYE));
        assert_eq!(xs.len(), 32);
        assert_eq!(xs.iter().filter(|x| **x == 3).count(), 31, "every streak");
        assert!(xs.contains(&100), "the flash took the mark's place");
        assert!(!xs.contains(&200), "the mark was dropped");
    }
}
