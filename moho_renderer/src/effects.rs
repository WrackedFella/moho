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

/// A camera-facing four-point star that shrinks linearly to nothing over its lifetime.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FlashStyle {
    pub size: f32,
    pub material: u32,
    pub lifetime: Duration,
}

/// A quad lying on a surface: full size, then shrinking to nothing over the last `fade`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MarkStyle {
    pub size: f32,
    pub material: u32,
    pub lifetime: Duration,
    pub fade: Duration,
}

/// How far a mark floats off its surface along the normal, against z-fighting.
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

/// Whether a flash is a camera-facing star or a mark is a quad on a surface.
#[derive(Debug, Clone, Copy)]
enum FixedKind {
    Flash,
    Mark { normal: Vec3 },
}

/// A flash or mark: fixed lifetime, shrinking to nothing over its last `fade`.
#[derive(Debug)]
struct Fixed {
    kind: FixedKind,
    at: Vec3,
    roll: f32,
    size: f32,
    material: u32,
    lifetime: Duration,
    /// A flash fades over its whole lifetime.
    fade: Duration,
    age: Duration,
}

impl Fixed {
    /// The size factor: 1 until the last `fade` of the lifetime, then linear to 0.
    fn scale(&self) -> f32 {
        let remaining = self.lifetime.saturating_sub(self.age);
        if remaining.is_zero() {
            return 0.0;
        }
        if remaining >= self.fade {
            return 1.0;
        }
        remaining.as_secs_f32() / self.fade.as_secs_f32()
    }

    fn expired(&self) -> bool {
        self.age >= self.lifetime
    }
}

#[derive(Debug)]
enum Effect {
    Streak(Streak),
    Fixed(Fixed),
}

/// Which of the built-in meshes have been uploaded.
#[derive(Debug, Default)]
struct Uploaded {
    streak: bool,
    star: bool,
    quad: bool,
}

#[derive(Debug)]
pub struct Effects {
    live: BTreeMap<EffectHandle, Effect>,
    next_handle: u64,
    cap: usize,
    uploaded: Uploaded,
}

impl Default for Effects {
    fn default() -> Self {
        Self {
            live: BTreeMap::new(),
            next_handle: 0,
            cap: CAPS[DEFAULT_QUALITY],
            uploaded: Uploaded::default(),
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

    /// Spawn a flash at `at`, rolled by `roll` about the view axis. Non-finite inputs are
    /// refused; see `spawn_fixed` for the cap.
    pub fn flash(&mut self, at: Vec3, roll: f32, style: FlashStyle) {
        self.spawn_fixed(Fixed {
            kind: FixedKind::Flash,
            at,
            roll,
            size: style.size,
            material: style.material,
            lifetime: style.lifetime,
            fade: style.lifetime,
            age: Duration::ZERO,
        });
    }

    /// Spawn a mark on a surface at `at` with the given `normal`, rolled by `roll` about it.
    /// A zero or non-finite normal and other non-finite inputs are refused. The fade is
    /// clamped to the lifetime.
    pub fn mark(&mut self, at: Vec3, normal: Vec3, roll: f32, style: MarkStyle) {
        let Some(normal) = normal.try_normalize() else {
            return;
        };
        self.spawn_fixed(Fixed {
            kind: FixedKind::Mark { normal },
            at,
            roll,
            size: style.size,
            material: style.material,
            lifetime: style.lifetime,
            fade: style.fade.min(style.lifetime),
            age: Duration::ZERO,
        });
    }

    /// Store a flash or mark. At the cap the oldest flashes and marks are evicted to make
    /// room; streaks are never evicted, so with none left to evict the spawn is refused.
    fn spawn_fixed(&mut self, fixed: Fixed) {
        if !(fixed.at.is_finite() && fixed.roll.is_finite() && fixed.size.is_finite())
            || self.cap == 0
        {
            return;
        }
        while self.live.len() >= self.cap {
            let oldest = self
                .live
                .iter()
                .find_map(|(handle, e)| matches!(e, Effect::Fixed(_)).then_some(*handle));
            let Some(oldest) = oldest else {
                return;
            };
            self.live.remove(&oldest);
        }
        let handle = self.next_handle();
        self.live.insert(handle, Effect::Fixed(fixed));
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

    /// Age every effect by the frame time, start the fade of any streak past its
    /// `max_age` and drop the finished and expired ones.
    pub fn advance(&mut self, dt: Duration) {
        for effect in self.live.values_mut() {
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
        self.live.retain(|_, effect| match effect {
            Effect::Streak(streak) => !streak.finished_fading(),
            Effect::Fixed(fixed) => !fixed.expired(),
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

    /// Place every streak placed this frame or fading, and every flash and mark,
    /// unshadowed; streaks and flashes face `eye`.
    pub(crate) fn draw(&mut self, meshes: &mut WorldMeshes, eye: Vec3) {
        for effect in self.live.values_mut() {
            match effect {
                Effect::Streak(streak) => {
                    let placed = std::mem::take(&mut streak.placed_this_frame);
                    if !placed && streak.ended_at.is_none() {
                        continue;
                    }
                    let Some(heading) = streak.heading else {
                        continue;
                    };
                    let width = streak.style.width * streak.width_scale();
                    if let Some(transform) =
                        streak_transform(streak.head, heading, streak.style.length, width, eye)
                    {
                        place(
                            meshes,
                            &mut self.uploaded.streak,
                            (STREAK_MESH_ID, streak_mesh),
                            transform,
                            streak.style.material,
                        );
                    }
                }
                Effect::Fixed(fixed) => {
                    let Some(transform) = fixed_transform(fixed, eye) else {
                        continue;
                    };
                    let (uploaded, id, mesh): (_, _, fn() -> WorldMesh) = match fixed.kind {
                        FixedKind::Flash => (&mut self.uploaded.star, STAR_MESH_ID, star_mesh),
                        FixedKind::Mark { .. } => {
                            (&mut self.uploaded.quad, QUAD_MESH_ID, quad_mesh)
                        }
                    };
                    place(meshes, uploaded, (id, mesh), transform, fixed.material);
                }
            }
        }
    }
}

const STREAK_MESH_ID: u64 = EFFECT_MESH_IDS_FROM;
const STAR_MESH_ID: u64 = EFFECT_MESH_IDS_FROM + 1;
const QUAD_MESH_ID: u64 = EFFECT_MESH_IDS_FROM + 2;

/// Upload `mesh` on first use, then place it unshadowed.
fn place(
    meshes: &mut WorldMeshes,
    uploaded: &mut bool,
    (id, mesh): (u64, fn() -> WorldMesh),
    transform: Affine3A,
    material: u32,
) {
    if !std::mem::replace(uploaded, true) {
        meshes.upsert_movable(WorldMeshId(id), mesh());
    }
    meshes.place_unshadowed(WorldMeshId(id), transform, material);
}

/// Two unit vectors across `axis`, turned by `roll` about it, with `x cross y == axis`.
fn rolled_basis(axis: Vec3, roll: f32) -> (Vec3, Vec3) {
    let x = axis.any_orthonormal_vector();
    let y = axis.cross(x);
    let (sin, cos) = roll.sin_cos();
    (x * cos + y * sin, y * cos - x * sin)
}

/// Where a flash or mark is drawn this frame; `None` while it is invisible or its eye
/// is unusable.
fn fixed_transform(fixed: &Fixed, eye: Vec3) -> Option<Affine3A> {
    let size = fixed.size * fixed.scale();
    if size <= 0.0 {
        return None;
    }
    let (axis, centre) = match fixed.kind {
        FixedKind::Flash => ((eye - fixed.at).try_normalize()?, fixed.at),
        FixedKind::Mark { normal } => (normal, fixed.at + normal * MARK_OFFSET),
    };
    let (x, y) = rolled_basis(axis, fixed.roll);
    Some(Affine3A::from_cols(
        (x * size).into(),
        (y * size).into(),
        axis.into(),
        centre.into(),
    ))
}

/// A four-point star in the XY plane, tips at radius 0.5 on the axes, counter-clockwise
/// seen from +Z.
fn star_mesh() -> WorldMesh {
    const INNER: f32 = 0.1;
    flat_mesh(
        vec![
            [0.5, 0.0, 0.0],
            [INNER, INNER, 0.0],
            [0.0, 0.5, 0.0],
            [-INNER, INNER, 0.0],
            [-0.5, 0.0, 0.0],
            [-INNER, -INNER, 0.0],
            [0.0, -0.5, 0.0],
            [INNER, -INNER, 0.0],
        ],
        vec![1, 3, 5, 1, 5, 7, 7, 0, 1, 1, 2, 3, 3, 4, 5, 5, 6, 7],
    )
}

/// A unit quad centred on the origin, counter-clockwise seen from +Z.
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

/// A mesh facing +Z with every per-vertex channel neutral, so it takes the material and
/// lighting of the instance alone.
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
                .map(|(_, i)| i)
                .collect()
        }

        /// One frame's draw as `(backend mesh handle, instance)`; handles are 1-based
        /// registration order.
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
    fn the_streak_star_and_quad_meshes_sit_at_the_first_three_reserved_ids() {
        for offset in 0..3 {
            let mut rig = Rig::new();
            let streak = rig.effects.begin_streak(style(2.0, 0.1));
            rig.effects.place_streak(streak, Vec3::X, Vec3::X);
            rig.effects.flash(Vec3::X, 0.0, flash_style(0.4, ms(500)));
            rig.effects.mark(
                Vec3::Y,
                Vec3::Y,
                0.0,
                mark_style(0.2, Duration::from_secs(10), ms(500)),
            );
            rig.frame(EYE);
            assert_eq!(rig.meshes.live_len(), 3);

            rig.meshes
                .remove(WorldMeshId(EFFECT_MESH_IDS_FROM + offset));
            rig.meshes.flush(&mut rig.backend);

            assert_eq!(
                rig.meshes.live_len(),
                2,
                "no mesh sits at reserved id +{offset}"
            );
        }
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
            material: 3,
            lifetime,
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

    fn all_finite(instance: &InstanceGpu) -> bool {
        instance.model.iter().flatten().all(|v| v.is_finite())
    }

    fn z_axis(instance: &InstanceGpu) -> Vec3 {
        let [x, y, z, _] = instance.model[2];
        Vec3::new(x, y, z)
    }

    /// Everything that distinguishes a drawn instance, so two frames can be compared.
    fn keys(drawn: &[InstanceGpu]) -> Vec<([[f32; 4]; 4], u32)> {
        drawn.iter().map(|i| (i.model, i.material)).collect()
    }

    fn any_drawn_near(drawn: &[InstanceGpu], at: Vec3) -> bool {
        drawn.iter().any(|i| (translation(i) - at).length() < 0.05)
    }

    fn assert_ccw_toward_plus_z(positions: &[[f32; 3]], indices: &[u32]) {
        assert_eq!(indices.len() % 3, 0);
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
        rig.effects.flash(at, 0.0, flash_style(0.4, ms(50)));

        let full = rig.frame(EYE);
        assert_eq!(full.len(), 1, "drawn without being placed");
        assert_near(translation(&full[0]), at);
        assert_eq!(full[0].material, 3);
        assert!((x_axis(&full[0]).length() - 0.4).abs() < EPS);
        assert!((y_axis(&full[0]).length() - 0.4).abs() < EPS);

        rig.effects.advance(ms(25));
        let half = rig.frame(EYE);
        assert_eq!(half.len(), 1);
        assert!((x_axis(&half[0]).length() - 0.2).abs() < EPS);
        assert!((y_axis(&half[0]).length() - 0.2).abs() < EPS);
        assert_eq!(rig.effects.live(), 1);

        rig.effects.advance(ms(25));
        assert_eq!(rig.effects.live(), 0);
        assert!(rig.frame(EYE).is_empty());
    }

    #[test]
    fn a_flash_faces_the_eye() {
        let at = Vec3::new(1.0, 2.0, 3.0);
        let eyes = [
            EYE,
            Vec3::new(-20.0, 5.0, 8.0),
            at + Vec3::Y * 7.0,
            at - Vec3::Y * 7.0,
            at + Vec3::Z * 4.0,
            at - Vec3::X * 4.0,
        ];
        for eye in eyes {
            for roll in [0.0, 0.7, 2.0, -1.3, 3.1] {
                let mut rig = Rig::new();
                rig.effects.flash(at, roll, flash_style(0.4, ms(50)));

                let drawn = rig.frame(eye);

                assert_eq!(drawn.len(), 1, "eye {eye} roll {roll}");
                let instance = &drawn[0];
                assert!(all_finite(instance), "eye {eye} roll {roll}");
                let (x, y) = (x_axis(instance), y_axis(instance));
                let view = (eye - at).normalize();
                assert!(x.cross(y).dot(view) > 0.0, "eye {eye} roll {roll}");
                assert!(x.dot(view).abs() < EPS && y.dot(view).abs() < EPS);
                assert!(x.dot(y).abs() < EPS);
                assert!((x.length() - 0.4).abs() < EPS);
                assert!((y.length() - 0.4).abs() < EPS);
            }
        }
    }

    #[test]
    fn a_flash_basis_follows_the_eye_of_each_frame_it_is_drawn_in() {
        let mut rig = Rig::new();
        let at = Vec3::new(1.0, 2.0, 3.0);
        rig.effects.flash(at, 0.3, flash_style(0.4, ms(500)));

        for eye in [EYE, at + Vec3::Z * 4.0] {
            let drawn = rig.frame(eye);

            assert_eq!(drawn.len(), 1, "eye {eye}");
            let (x, y) = (x_axis(&drawn[0]), y_axis(&drawn[0]));
            let view = (eye - at).normalize();
            assert!(
                x.cross(y).normalize().dot(view) > 1.0 - EPS,
                "x cross y faces the eye of this frame {eye}"
            );
            assert!(
                x.dot(view).abs() < EPS && y.dot(view).abs() < EPS,
                "eye {eye}"
            );
        }
    }

    #[test]
    fn a_flash_roll_turns_the_star_about_the_view_axis() {
        let at = Vec3::new(1.0, 2.0, 3.0);
        let spin = |roll: f32| {
            let mut rig = Rig::new();
            rig.effects.flash(at, roll, flash_style(1.0, ms(50)));
            let drawn = rig.frame(EYE);
            assert_eq!(drawn.len(), 1);
            x_axis(&drawn[0])
        };
        let base = spin(0.0);

        for roll in [0.4, 1.0, 2.5] {
            let turned = spin(roll);

            assert!(
                (base.dot(turned) - roll.cos()).abs() < EPS,
                "roll {roll} turns X by that angle"
            );
        }
    }

    #[test]
    fn a_mark_lies_along_its_normal_offset_by_the_mark_offset() {
        let mut rig = Rig::new();
        let at = Vec3::new(4.0, -1.0, 2.0);
        let normal = Vec3::new(1.0, 2.0, 2.0);
        rig.effects.mark(
            at,
            normal,
            0.9,
            mark_style(0.2, Duration::from_secs(10), ms(500)),
        );

        let drawn = rig.frame(EYE);

        assert_eq!(drawn.len(), 1);
        let instance = &drawn[0];
        let unit = normal.normalize();
        assert_eq!(instance.material, 5);
        assert_near(z_axis(instance), unit);
        assert!((x_axis(instance).length() - 0.2).abs() < EPS);
        assert!((y_axis(instance).length() - 0.2).abs() < EPS);
        assert!(x_axis(instance).dot(unit).abs() < EPS);
        assert!(y_axis(instance).dot(unit).abs() < EPS);
        assert!(x_axis(instance).dot(y_axis(instance)).abs() < EPS);
        assert_near(translation(instance), at + unit * MARK_OFFSET);
    }

    #[test]
    fn a_mark_roll_turns_its_basis_about_the_normal() {
        let at = Vec3::new(4.0, -1.0, 2.0);
        let basis = |roll: f32| {
            let mut rig = Rig::new();
            rig.effects.mark(
                at,
                Vec3::Y,
                roll,
                mark_style(1.0, Duration::from_secs(10), ms(500)),
            );
            let drawn = rig.frame(EYE);
            assert_eq!(drawn.len(), 1);
            (x_axis(&drawn[0]), y_axis(&drawn[0]))
        };
        let (x0, y0) = basis(0.0);

        let (x_quarter, _) = basis(std::f32::consts::FRAC_PI_2);
        let (x_third, _) = basis(1.0);

        assert!(x_quarter.dot(x0).abs() < EPS, "a quarter turn leaves X");
        assert!(x_quarter.dot(y0).abs() > 1.0 - EPS, "and lands on old Y");
        assert!((x_third.dot(x0) - 1.0f32.cos()).abs() < EPS);
    }

    #[test]
    fn a_mark_holds_full_size_then_shrinks_out_in_its_fade() {
        let mut rig = Rig::new();
        let at = Vec3::new(4.0, 0.0, 2.0);
        rig.effects.mark(
            at,
            Vec3::Y,
            0.0,
            mark_style(0.2, Duration::from_secs(10), Duration::from_secs(1)),
        );

        rig.effects.advance(Duration::from_secs(4));
        let early = rig.frame(EYE);
        assert_eq!(early.len(), 1);
        assert!(
            (x_axis(&early[0]).length() - 0.2).abs() < EPS,
            "not over size"
        );
        assert!(
            (y_axis(&early[0]).length() - 0.2).abs() < EPS,
            "not over size"
        );

        rig.effects.advance(Duration::from_secs(5));
        let full = rig.frame(EYE);
        assert_eq!(full.len(), 1);
        assert!((x_axis(&full[0]).length() - 0.2).abs() < EPS);
        assert!((y_axis(&full[0]).length() - 0.2).abs() < EPS);

        rig.effects.advance(ms(500));
        let half = rig.frame(EYE);
        assert_eq!(half.len(), 1);
        assert!((x_axis(&half[0]).length() - 0.1).abs() < EPS);
        assert!((y_axis(&half[0]).length() - 0.1).abs() < EPS);
        assert_near(translation(&half[0]), translation(&full[0]));

        rig.effects.advance(ms(500));
        assert_eq!(rig.effects.live(), 0);
        assert!(rig.frame(EYE).is_empty());
    }

    #[test]
    fn a_full_cap_drops_the_oldest_fixed_lifetime_effect() {
        let mut rig = Rig::new();
        rig.effects.set_quality(1);
        let long = Duration::from_secs(60);
        let oldest = Vec3::new(100.0, 0.0, 0.0);
        rig.effects
            .mark(oldest, Vec3::Y, 0.0, mark_style(0.2, long, ms(500)));
        for i in 1..32 {
            let at = Vec3::new(f32::from(u8::try_from(i).expect("small")), 0.0, 0.0);
            rig.effects
                .mark(at, Vec3::Y, 0.0, mark_style(0.2, long, ms(500)));
        }
        assert_eq!(rig.effects.live(), 32);
        let newest = Vec3::new(500.0, 0.0, 0.0);

        rig.effects
            .mark(newest, Vec3::Y, 0.0, mark_style(0.2, long, ms(500)));
        let drawn = rig.frame(EYE);

        assert_eq!(rig.effects.live(), 32);
        assert_eq!(drawn.len(), 32);
        assert!(!any_drawn_near(&drawn, oldest), "the oldest was dropped");
        assert!(any_drawn_near(&drawn, newest), "the newest is kept");
    }

    #[test]
    fn a_full_cap_of_streaks_refuses_a_mark_and_keeps_every_streak() {
        for kind in ["flash", "mark"] {
            let mut rig = Rig::new();
            rig.effects.set_quality(1);
            let handles: Vec<EffectHandle> = (0..32)
                .map(|_| rig.effects.begin_streak(style(2.0, 0.1)))
                .collect();
            let at = Vec3::new(0.0, 50.0, 0.0);

            match kind {
                "flash" => rig.effects.flash(at, 0.0, flash_style(0.4, ms(50))),
                _ => rig.effects.mark(
                    at,
                    Vec3::Y,
                    0.0,
                    mark_style(0.2, Duration::from_secs(10), ms(500)),
                ),
            }
            for handle in &handles {
                rig.effects
                    .place_streak(*handle, Vec3::new(10.0, 0.0, 0.0), Vec3::X);
            }
            let drawn = rig.frame(EYE);

            assert_eq!(rig.effects.live(), 32, "{kind}");
            assert_eq!(
                drawn.len(),
                32,
                "{kind}: every streak draws and the spawn does not"
            );
            assert!(!any_drawn_near(&drawn, at), "{kind}");
        }
    }

    #[test]
    fn a_spawn_above_the_cap_evicts_oldest_fixed_lifetime_effects_until_under_it() {
        let mut rig = Rig::new();
        let long = Duration::from_secs(60);
        let at = |i: u16| Vec3::new(f32::from(i) * 2.0, 0.0, 0.0);
        for i in 0..40 {
            rig.effects
                .mark(at(i), Vec3::Y, 0.0, mark_style(0.2, long, ms(500)));
        }
        rig.effects.set_quality(1);
        assert_eq!(rig.effects.live(), 40, "kept though over the new cap");

        rig.effects
            .mark(at(40), Vec3::Y, 0.0, mark_style(0.2, long, ms(500)));
        let drawn = rig.frame(EYE);

        assert_eq!(rig.effects.live(), 32, "nine evicted, one added");
        assert_eq!(drawn.len(), 32);
        for i in 0..9 {
            assert!(!any_drawn_near(&drawn, at(i)), "oldest {i} was evicted");
        }
        for i in 9..=40 {
            assert!(any_drawn_near(&drawn, at(i)), "{i} is kept");
        }
    }

    #[test]
    fn a_spawn_above_the_cap_with_nothing_to_evict_is_refused() {
        let mut rig = Rig::new();
        let handles: Vec<EffectHandle> = (0..40)
            .map(|_| rig.effects.begin_streak(style(2.0, 0.1)))
            .collect();
        rig.effects.set_quality(1);

        rig.effects
            .flash(Vec3::Y * 50.0, 0.0, flash_style(0.4, ms(50)));
        for handle in &handles {
            rig.effects
                .place_streak(*handle, Vec3::new(10.0, 0.0, 0.0), Vec3::X);
        }
        let drawn = rig.frame(EYE);

        assert_eq!(rig.effects.live(), 40);
        assert_eq!(drawn.len(), 40);
        assert!(!any_drawn_near(&drawn, Vec3::Y * 50.0));
    }

    #[test]
    fn a_mark_evicts_an_older_flash_at_the_cap() {
        let mut rig = Rig::new();
        rig.effects.set_quality(1);
        let long = Duration::from_secs(60);
        let flash_at = Vec3::new(100.0, 0.0, 0.0);
        rig.effects.flash(flash_at, 0.0, flash_style(0.4, long));
        for i in 1..32 {
            let at = Vec3::new(f32::from(u8::try_from(i).expect("small")), 0.0, 0.0);
            rig.effects
                .mark(at, Vec3::Y, 0.0, mark_style(0.2, long, ms(500)));
        }
        let newest = Vec3::new(500.0, 0.0, 0.0);

        rig.effects
            .mark(newest, Vec3::Y, 0.0, mark_style(0.2, long, ms(500)));
        let drawn = rig.frame(EYE);

        assert_eq!(rig.effects.live(), 32);
        assert!(!any_drawn_near(&drawn, flash_at), "the old flash went");
        assert!(any_drawn_near(&drawn, newest));
    }

    #[test]
    fn a_flash_evicts_an_older_mark_at_the_cap() {
        let mut rig = Rig::new();
        rig.effects.set_quality(1);
        let long = Duration::from_secs(60);
        let mark_at = Vec3::new(100.0, 0.0, 0.0);
        rig.effects
            .mark(mark_at, Vec3::Y, 0.0, mark_style(0.2, long, ms(500)));
        for i in 1..32 {
            let at = Vec3::new(f32::from(u8::try_from(i).expect("small")), 0.0, 0.0);
            rig.effects.flash(at, 0.0, flash_style(0.4, long));
        }
        let newest = Vec3::new(500.0, 0.0, 0.0);

        rig.effects.flash(newest, 0.0, flash_style(0.4, long));
        let drawn = rig.frame(EYE);

        assert_eq!(rig.effects.live(), 32);
        assert!(!any_drawn_near(&drawn, mark_at), "the old mark went");
        assert!(any_drawn_near(&drawn, newest));
    }

    #[test]
    fn a_spawn_at_the_cap_never_evicts_a_streak() {
        let mut rig = Rig::new();
        rig.effects.set_quality(1);
        let handles: Vec<EffectHandle> = (0..31)
            .map(|_| rig.effects.begin_streak(style(2.0, 0.1)))
            .collect();
        rig.effects
            .flash(Vec3::X * 100.0, 0.0, flash_style(0.4, ms(500)));

        rig.effects
            .flash(Vec3::X * 200.0, 0.0, flash_style(0.4, ms(500)));
        for handle in &handles {
            rig.effects
                .place_streak(*handle, Vec3::new(10.0, 0.0, 0.0), Vec3::X);
        }
        let drawn = rig.frame(EYE);

        assert_eq!(rig.effects.live(), 32);
        assert_eq!(drawn.len(), 32, "31 streaks and the newest flash");
        assert!(!any_drawn_near(&drawn, Vec3::X * 100.0));
        assert!(any_drawn_near(&drawn, Vec3::X * 200.0));
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
            mark_style(0.2, Duration::from_secs(10), ms(500)),
        );

        assert_eq!(rig.effects.live(), 0);
        assert!(rig.frame(EYE).is_empty());
    }

    #[test]
    fn a_zero_lifetime_flash_is_removed_at_once() {
        let mut rig = Rig::new();
        rig.effects
            .flash(Vec3::X, 0.0, flash_style(0.4, Duration::ZERO));
        rig.effects
            .flash(Vec3::Y * 5.0, 0.0, flash_style(0.4, ms(50)));

        let drawn = rig.frame(EYE);
        rig.effects.advance(ms(1));

        assert_eq!(drawn.len(), 1, "only the flash with a lifetime draws");
        assert_near(translation(&drawn[0]), Vec3::Y * 5.0);
        assert_eq!(rig.effects.live(), 1);
    }

    #[test]
    fn a_zero_lifetime_mark_is_removed_at_once() {
        let mut rig = Rig::new();
        rig.effects.mark(
            Vec3::X,
            Vec3::Y,
            0.0,
            mark_style(0.2, Duration::ZERO, Duration::ZERO),
        );
        rig.effects
            .flash(Vec3::Y * 5.0, 0.0, flash_style(0.4, ms(50)));

        let drawn = rig.frame(EYE);
        rig.effects.advance(ms(1));

        assert_eq!(drawn.len(), 1);
        assert_near(translation(&drawn[0]), Vec3::Y * 5.0);
        assert_eq!(rig.effects.live(), 1);
    }

    #[test]
    fn a_mark_fade_longer_than_its_lifetime_is_clamped_to_the_lifetime() {
        let mut rig = Rig::new();
        rig.effects.mark(
            Vec3::X,
            Vec3::Y,
            0.0,
            mark_style(0.2, Duration::from_secs(2), Duration::from_secs(9)),
        );

        let start = rig.frame(EYE);
        assert_eq!(start.len(), 1);
        assert!(all_finite(&start[0]));
        assert!(
            (x_axis(&start[0]).length() - 0.2).abs() < EPS,
            "not over size"
        );

        rig.effects.advance(Duration::from_secs(1));
        let middle = rig.frame(EYE);
        assert_eq!(middle.len(), 1);
        assert!((x_axis(&middle[0]).length() - 0.1).abs() < EPS);

        rig.effects.advance(Duration::from_secs(1));
        assert!(rig.frame(EYE).is_empty());
    }

    #[test]
    fn a_degenerate_normal_or_non_finite_input_draws_nothing() {
        type Spawn = fn(&mut Effects);
        let ok = mark_style(0.2, Duration::from_secs(10), ms(500));
        let cases: [(&str, Spawn); 9] = [
            ("mark with a zero normal", |e| {
                e.mark(
                    Vec3::X,
                    Vec3::ZERO,
                    0.0,
                    mark_style(0.2, Duration::from_secs(10), ms(500)),
                );
            }),
            ("mark with a NaN normal", |e| {
                e.mark(
                    Vec3::X,
                    Vec3::splat(f32::NAN),
                    0.0,
                    mark_style(0.2, Duration::from_secs(10), ms(500)),
                );
            }),
            ("mark with an infinite normal", |e| {
                e.mark(
                    Vec3::X,
                    Vec3::new(f32::INFINITY, 0.0, 0.0),
                    0.0,
                    mark_style(0.2, Duration::from_secs(10), ms(500)),
                );
            }),
            ("mark at a NaN position", |e| {
                e.mark(
                    Vec3::splat(f32::NAN),
                    Vec3::Y,
                    0.0,
                    mark_style(0.2, Duration::from_secs(10), ms(500)),
                );
            }),
            ("mark with a NaN roll", |e| {
                e.mark(
                    Vec3::X,
                    Vec3::Y,
                    f32::NAN,
                    mark_style(0.2, Duration::from_secs(10), ms(500)),
                );
            }),
            ("mark with a NaN size", |e| {
                e.mark(
                    Vec3::X,
                    Vec3::Y,
                    0.0,
                    mark_style(f32::NAN, Duration::from_secs(10), ms(500)),
                );
            }),
            ("flash at a NaN position", |e| {
                e.flash(Vec3::splat(f32::NAN), 0.0, flash_style(0.4, ms(50)));
            }),
            ("flash with a NaN roll", |e| {
                e.flash(Vec3::X, f32::NAN, flash_style(0.4, ms(50)));
            }),
            ("flash with a NaN size", |e| {
                e.flash(Vec3::X, 0.0, flash_style(f32::NAN, ms(50)));
            }),
        ];

        for (label, spawn) in cases {
            let mut rig = Rig::new();
            spawn(&mut rig.effects);

            let drawn = rig.frame(EYE);

            assert!(drawn.is_empty(), "{label} draws nothing");
        }
        let mut control = Rig::new();
        control.effects.mark(Vec3::Y * 9.0, Vec3::Y, 0.0, ok);
        let drawn = control.frame(EYE);
        assert_eq!(drawn.len(), 1, "a well-formed mark draws");
        assert!(all_finite(&drawn[0]));
        assert_near(
            translation(&drawn[0]),
            Vec3::Y * 9.0 + Vec3::Y * MARK_OFFSET,
        );
    }

    #[test]
    fn a_non_finite_eye_draws_no_flash() {
        let mut rig = Rig::new();
        rig.effects.flash(Vec3::X, 0.0, flash_style(0.4, ms(50)));
        rig.effects.mark(
            Vec3::Y,
            Vec3::Y,
            0.0,
            mark_style(0.2, Duration::from_secs(10), ms(500)),
        );

        let drawn = rig.frame(Vec3::splat(f32::NAN));

        assert!(drawn.iter().all(all_finite));
        assert_eq!(drawn.len(), 1, "the mark does not need the eye");
        assert_near(translation(&drawn[0]), Vec3::Y + Vec3::Y * MARK_OFFSET);
    }

    #[test]
    fn the_flash_and_mark_meshes_are_registered_once_each() {
        let mut rig = Rig::new();
        rig.effects.flash(Vec3::X, 0.0, flash_style(0.4, ms(500)));
        rig.effects.mark(
            Vec3::Y,
            Vec3::Y,
            0.0,
            mark_style(0.2, Duration::from_secs(10), ms(500)),
        );

        let mut drawn = Vec::new();
        for _ in 0..3 {
            drawn.extend(rig.frame_with_meshes(EYE));
            rig.effects.advance(ms(10));
            rig.effects.flash(Vec3::X, 0.5, flash_style(0.4, ms(500)));
        }

        assert_eq!(rig.backend.registered.len(), 2, "one star and one quad");
        let star_index = rig
            .backend
            .registered
            .iter()
            .position(|(p, _)| p.len() == 8)
            .expect("an eight-vertex star is registered");
        let quad_index = rig
            .backend
            .registered
            .iter()
            .position(|(p, _)| p.len() == 4)
            .expect("a four-vertex quad is registered");
        let star_handle = u32::try_from(star_index + 1).expect("few meshes");
        let quad_handle = u32::try_from(quad_index + 1).expect("few meshes");
        let flash_draws = drawn
            .iter()
            .filter(|(_, i)| (translation(i) - Vec3::X).length() < 0.05)
            .count();
        let mark_draws = drawn
            .iter()
            .filter(|(_, i)| (translation(i) - Vec3::Y).length() < 0.05)
            .count();
        assert!(flash_draws >= 1 && mark_draws >= 1, "both kinds drew");
        for (handle, instance) in &drawn {
            if (translation(instance) - Vec3::X).length() < 0.05 {
                assert_eq!(*handle, star_handle, "a flash draws with the star mesh");
            } else {
                assert_eq!(*handle, quad_handle, "a mark draws with the quad mesh");
            }
        }
        let star = &rig.backend.registered[star_index];
        let quad = &rig.backend.registered[quad_index];
        assert!(star.0.iter().all(|p| p[2] == 0.0), "the star is in XY");
        let centre = star
            .0
            .iter()
            .fold(Vec3::ZERO, |sum, p| sum + Vec3::from(*p))
            / 8.0;
        assert_near(centre, Vec3::ZERO);
        // Star shape: four tips at radius 0.5 on +-X and +-Y (so `size` is the full
        // tip-to-tip width), alternating with four inner vertices at one smaller radius
        // on the diagonals.
        let mut by_angle: Vec<Vec3> = star.0.iter().map(|p| Vec3::from(*p)).collect();
        by_angle.sort_by(|a, b| {
            a.y.atan2(a.x)
                .partial_cmp(&b.y.atan2(b.x))
                .expect("finite vertices")
        });
        let radii: Vec<f32> = by_angle.iter().map(|p| p.length()).collect();
        let inner = radii[0].min(radii[1]);
        assert!(
            inner > 0.05 && inner < 0.5 - 0.05,
            "inner radius {inner} is smaller"
        );
        for (k, p) in by_angle.iter().enumerate() {
            let tip = p.x.abs() < EPS || p.y.abs() < EPS;
            if tip {
                assert!((radii[k] - 0.5).abs() < EPS, "tip {p} is at radius 0.5");
            } else {
                assert!(
                    (radii[k] - inner).abs() < EPS,
                    "inner {p} shares one radius"
                );
                assert!((p.x.abs() - p.y.abs()).abs() < EPS, "inner {p} is diagonal");
            }
        }
        assert_eq!(
            by_angle
                .iter()
                .filter(|p| p.x.abs() < EPS || p.y.abs() < EPS)
                .count(),
            4,
            "four tips on the axes"
        );
        for k in 0..8 {
            let this_tip = by_angle[k].x.abs() < EPS || by_angle[k].y.abs() < EPS;
            let next = by_angle[(k + 1) % 8];
            let next_tip = next.x.abs() < EPS || next.y.abs() < EPS;
            assert_ne!(this_tip, next_tip, "tips and inner vertices alternate");
        }
        assert_ccw_toward_plus_z(&star.0, &star.1);
        assert!(star.1.iter().all(|i| (*i as usize) < 8));
        let mut corners: Vec<[f32; 3]> = quad.0.clone();
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
        assert_eq!(quad.1.len(), 6, "a quad is two triangles");
        assert_ccw_toward_plus_z(&quad.0, &quad.1);
    }

    #[test]
    fn a_streak_a_flash_and_a_mark_draw_together_each_with_its_own_mesh() {
        let mut rig = Rig::new();
        let handle = rig.effects.begin_streak(style(2.0, 0.1));
        rig.effects
            .place_streak(handle, Vec3::new(10.0, 0.0, 0.0), Vec3::X);
        rig.effects
            .flash(Vec3::new(0.0, 20.0, 0.0), 0.0, flash_style(0.4, ms(500)));
        rig.effects.mark(
            Vec3::new(0.0, 0.0, 30.0),
            Vec3::Y,
            0.0,
            mark_style(0.2, Duration::from_secs(10), ms(500)),
        );

        let drawn = rig.frame_with_meshes(EYE);

        assert_eq!(drawn.len(), 3);
        assert_eq!(rig.backend.registered.len(), 3, "kite, star and quad");
        let kite = vec![
            [0.0, 0.0, 0.0],
            [0.8, -0.5, 0.0],
            [1.0, 0.0, 0.0],
            [0.8, 0.5, 0.0],
        ];
        let kite_handle = rig
            .backend
            .registered
            .iter()
            .position(|(p, _)| *p == kite)
            .expect("the streak kite is registered intact")
            + 1;
        let star_handle = rig
            .backend
            .registered
            .iter()
            .position(|(p, _)| p.len() == 8)
            .expect("the star is registered")
            + 1;
        let quad_handle = rig
            .backend
            .registered
            .iter()
            .position(|(p, _)| p.len() == 4 && *p != kite)
            .expect("the quad is registered")
            + 1;
        let mut handles = [kite_handle, star_handle, quad_handle];
        handles.sort_unstable();
        assert_eq!(handles, [1, 2, 3], "three distinct meshes");
        let handle_near = |at: Vec3| {
            let hits: Vec<u32> = drawn
                .iter()
                .filter(|(_, i)| (translation(i) - at).length() < 0.05)
                .map(|(h, _)| *h)
                .collect();
            assert_eq!(hits.len(), 1, "one instance near {at}");
            usize::try_from(hits[0]).expect("small handle")
        };
        assert_eq!(handle_near(Vec3::new(8.0, 0.0, 0.0)), kite_handle);
        assert_eq!(handle_near(Vec3::new(0.0, 20.0, 0.0)), star_handle);
        assert_eq!(handle_near(Vec3::new(0.0, MARK_OFFSET, 30.0)), quad_handle);
    }

    #[test]
    fn quality_zero_clears_a_live_flash_and_mark() {
        let mut rig = Rig::new();
        rig.effects.flash(Vec3::X, 0.0, flash_style(0.4, ms(500)));
        rig.effects.mark(
            Vec3::Y,
            Vec3::Y,
            0.0,
            mark_style(0.2, Duration::from_secs(10), ms(500)),
        );
        assert_eq!(rig.effects.live(), 2, "spawned and live");

        rig.effects.set_quality(0);

        assert_eq!(rig.effects.live(), 0);
        assert!(rig.frame(EYE).is_empty());
    }

    #[test]
    fn the_same_spawns_draw_the_same_instances() {
        let run = || {
            let mut rig = Rig::new();
            let mut frames = Vec::new();
            for step in 0..6u8 {
                let t = f32::from(step);
                rig.effects
                    .flash(Vec3::new(t, 1.0, 2.0), t * 0.3, flash_style(0.4, ms(50)));
                rig.effects.mark(
                    Vec3::new(1.0, t, 2.0),
                    Vec3::new(t, 1.0, -1.0),
                    t * 0.7,
                    mark_style(0.2, ms(80), ms(40)),
                );
                rig.effects.advance(ms(20));
                frames.push(keys(&rig.frame(EYE)));
            }
            frames
        };

        let first = run();
        let second = run();

        assert!(first.iter().any(|f| !f.is_empty()), "something was drawn");
        assert_eq!(first, second);
    }

    fn normals() -> impl Strategy<Value = Vec3> {
        prop_oneof![
            Just(Vec3::X),
            Just(Vec3::NEG_X),
            Just(Vec3::Y),
            Just(Vec3::NEG_Y),
            Just(Vec3::Z),
            Just(Vec3::NEG_Z),
            (-5.0f32..5.0, -5.0f32..5.0, -5.0f32..5.0)
                .prop_map(|(x, y, z)| Vec3::new(x, y, z))
                .prop_filter("a usable normal", |n| n.length() > 0.1),
        ]
    }

    proptest! {
        #[test]
        fn a_mark_basis_is_finite_for_any_normal(
            normal in normals(),
            roll in -20.0f32..20.0,
            at in (-50.0f32..50.0, -50.0f32..50.0, -50.0f32..50.0),
        ) {
            let at = Vec3::new(at.0, at.1, at.2);
            let mut rig = Rig::new();
            rig.effects.mark(
                at,
                normal,
                roll,
                mark_style(0.2, Duration::from_secs(10), ms(500)),
            );

            let drawn = rig.frame(EYE);

            prop_assert_eq!(drawn.len(), 1);
            let instance = &drawn[0];
            prop_assert!(all_finite(instance));
            let unit = normal.normalize();
            prop_assert!((z_axis(instance).length() - 1.0).abs() < 1e-3, "Z is unit");
            prop_assert!((z_axis(instance) - unit).length() < 1e-3, "Z is the normal");
            prop_assert!(x_axis(instance).dot(unit).abs() < 1e-3);
            prop_assert!(y_axis(instance).dot(unit).abs() < 1e-3);
            prop_assert!(x_axis(instance).dot(y_axis(instance)).abs() < 1e-3);
            prop_assert!((x_axis(instance).length() - 0.2).abs() < 1e-3);
            prop_assert!((y_axis(instance).length() - 0.2).abs() < 1e-3);
            prop_assert!((translation(instance) - (at + unit * MARK_OFFSET)).length() < 1e-3);
        }
    }

    #[test]
    fn a_mark_with_zero_fade_is_full_size_until_its_lifetime_ends() {
        let mut rig = Rig::new();
        rig.effects.mark(
            Vec3::X,
            Vec3::Y,
            0.0,
            mark_style(0.2, Duration::from_secs(2), Duration::ZERO),
        );
        let start = rig.frame(EYE);
        assert_eq!(start.len(), 1);
        assert!(all_finite(&start[0]), "finite at spawn");
        assert!((x_axis(&start[0]).length() - 0.2).abs() < EPS);

        rig.effects.advance(ms(1999));
        let last = rig.frame(EYE);

        assert_eq!(last.len(), 1, "still drawn at 1.999s");
        assert!(all_finite(&last[0]), "finite at 1.999s");
        assert!((x_axis(&last[0]).length() - 0.2).abs() < EPS, "full size");
        assert!((y_axis(&last[0]).length() - 0.2).abs() < EPS, "full size");
        assert_eq!(rig.effects.live(), 1);

        rig.effects.advance(ms(1));

        assert_eq!(rig.effects.live(), 0, "gone at 2s");
        assert!(rig.frame(EYE).is_empty());
    }

    #[test]
    fn one_huge_advance_removes_flash_and_mark_and_frees_their_cap_slots() {
        let mut rig = Rig::new();
        rig.effects.set_quality(1);
        let hour = Duration::from_secs(3600);
        let survivor = Vec3::new(700.0, 0.0, 0.0);
        for i in 0..16u8 {
            let at = Vec3::new(f32::from(i), 0.0, 0.0);
            rig.effects.flash(at, 0.0, flash_style(0.4, ms(50)));
            rig.effects.mark(
                at + Vec3::Z * 3.0,
                Vec3::Y,
                0.0,
                mark_style(0.2, Duration::from_secs(10), ms(500)),
            );
        }
        rig.effects.mark(
            survivor,
            Vec3::Y,
            0.0,
            mark_style(0.2, hour * 2, Duration::from_secs(1)),
        );
        assert_eq!(rig.effects.live(), 32, "the cap is full");

        rig.effects.advance(hour);

        assert_eq!(rig.effects.live(), 1, "only the long-lived mark remains");
        for i in 0..31u8 {
            rig.effects.flash(
                Vec3::new(f32::from(i), 10.0, 0.0),
                0.0,
                flash_style(0.4, hour * 2),
            );
        }
        let drawn = rig.frame(EYE);
        assert_eq!(rig.effects.live(), 32);
        assert_eq!(drawn.len(), 32);
        assert!(
            any_drawn_near(&drawn, survivor + Vec3::Y * MARK_OFFSET),
            "refilling to the cap evicted nothing live"
        );
    }

    #[test]
    fn a_mark_with_a_tiny_normal_and_a_huge_roll_draws_finite_with_a_unit_z() {
        let mut rig = Rig::new();
        rig.effects.mark(
            Vec3::X,
            Vec3::new(1e-20, 0.0, 0.0),
            1e6,
            mark_style(0.2, Duration::from_secs(10), ms(500)),
        );

        let drawn = rig.frame(EYE);

        assert_eq!(drawn.len(), 1, "a tiny normal still has a direction");
        let instance = &drawn[0];
        assert!(all_finite(instance));
        assert!((z_axis(instance).length() - 1.0).abs() < EPS, "Z is unit");
        assert_near(z_axis(instance), Vec3::X);
        assert!(x_axis(instance).dot(Vec3::X).abs() < EPS);
        assert!(y_axis(instance).dot(Vec3::X).abs() < EPS);
        assert!((x_axis(instance).length() - 0.2).abs() < EPS);
    }

    #[test]
    fn a_flash_with_its_eye_almost_on_it_never_draws_non_finite_columns() {
        let at = Vec3::new(1.0, 2.0, 3.0);
        for offset in [
            Vec3::X * 1e-6,
            Vec3::new(0.0, -1e-6, 0.0),
            Vec3::new(1e-7, 1e-7, 1e-7),
        ] {
            let mut rig = Rig::new();
            rig.effects.flash(at, 0.4, flash_style(0.4, ms(50)));

            let drawn = rig.frame(at + offset);

            assert!(drawn.iter().all(all_finite), "offset {offset}");
            assert!(drawn.len() <= 1, "offset {offset}");
        }
    }

    #[test]
    fn eviction_at_the_cap_takes_fixed_effects_in_spawn_order() {
        let mut rig = Rig::new();
        rig.effects.set_quality(1);
        let long = Duration::from_secs(60);
        let at = |i: u16| Vec3::new(f32::from(i) * 2.0, 0.0, 0.0);
        for i in 0..32 {
            rig.effects
                .mark(at(i), Vec3::Y, 0.0, mark_style(0.2, long, ms(500)));
        }
        rig.effects
            .mark(at(100), Vec3::Y, 0.0, mark_style(0.2, long, ms(500)));

        rig.effects
            .mark(at(101), Vec3::Y, 0.0, mark_style(0.2, long, ms(500)));
        let drawn = rig.frame(EYE);

        assert_eq!(rig.effects.live(), 32);
        assert!(!any_drawn_near(&drawn, at(0)), "the oldest went first");
        assert!(
            !any_drawn_near(&drawn, at(1)),
            "the next oldest went second"
        );
        assert!(
            any_drawn_near(&drawn, at(100)),
            "the earlier newcomer stays"
        );
        assert!(any_drawn_near(&drawn, at(101)));
        for i in 2..32 {
            assert!(any_drawn_near(&drawn, at(i)), "{i} is kept");
        }
    }
}
