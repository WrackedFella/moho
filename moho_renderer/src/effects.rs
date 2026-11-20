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

/// A camera-facing star that shrinks linearly to nothing over its lifetime.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FlashStyle {
    pub size: f32,
    pub material: u32,
    pub lifetime: Duration,
}

/// A decal lying on a surface: full size until `lifetime - fade`, then shrinking to nothing.
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

/// 1 at `elapsed` zero falling linearly to 0 at `span`, and 0 for a zero span.
fn linear_fade(elapsed: Duration, span: Duration) -> f32 {
    if span.is_zero() {
        return 0.0;
    }
    (1.0 - elapsed.as_secs_f32() / span.as_secs_f32()).clamp(0.0, 1.0)
}

impl Streak {
    /// The width factor of the fade: 1 until ended, then falling linearly to 0.
    fn width_scale(&self) -> f32 {
        let Some(ended_at) = self.ended_at else {
            return 1.0;
        };
        linear_fade(self.age.saturating_sub(ended_at), self.style.fade)
    }

    fn finished_fading(&self) -> bool {
        self.ended_at
            .is_some_and(|ended_at| self.age.saturating_sub(ended_at) >= self.style.fade)
    }
}

#[derive(Debug, Clone, Copy)]
enum FixedKind {
    Flash { roll: f32 },
    Mark { normal: Vec3, roll: f32 },
}

/// A flash or mark: it ages on its own and is gone at its lifetime.
#[derive(Debug)]
struct Fixed {
    kind: FixedKind,
    at: Vec3,
    size: f32,
    material: u32,
    lifetime: Duration,
    /// The last stretch of the lifetime over which it shrinks to nothing.
    fade: Duration,
    age: Duration,
}

impl Fixed {
    /// The size factor: 1 until `lifetime - fade`, then falling linearly to 0.
    fn scale(&self) -> f32 {
        if self.age >= self.lifetime {
            return 0.0;
        }
        if self.fade.is_zero() {
            return 1.0;
        }
        let into_fade = self
            .age
            .saturating_sub(self.lifetime.saturating_sub(self.fade));
        linear_fade(into_fade, self.fade)
    }

    /// Where the unit mesh of this effect is drawn, or `None` if it has no valid basis.
    fn transform(&self, eye: Vec3) -> Option<Affine3A> {
        let size = self.size * self.scale();
        if size <= 0.0 {
            return None;
        }
        let (z, roll, centre) = match self.kind {
            FixedKind::Flash { roll } => ((eye - self.at).try_normalize()?, roll, self.at),
            FixedKind::Mark { normal, roll } => (normal, roll, self.at + normal * MARK_OFFSET),
        };
        let x = Quat::from_axis_angle(z, roll) * z.any_orthonormal_vector();
        let y = z.cross(x);
        Some(Affine3A::from_cols(
            (x * size).into(),
            (y * size).into(),
            z.into(),
            centre.into(),
        ))
    }

    /// Whether every input is usable: a finite position and roll, a mark's unit normal, a
    /// positive finite size and a non-zero lifetime.
    fn is_valid(&self) -> bool {
        let kind_valid = match self.kind {
            FixedKind::Flash { roll } => roll.is_finite(),
            FixedKind::Mark { normal, roll } => roll.is_finite() && normal.length_squared() > 0.0,
        };
        kind_valid
            && self.at.is_finite()
            && self.size.is_finite()
            && self.size > 0.0
            && !self.lifetime.is_zero()
    }

    fn mesh(&self) -> MeshSlot {
        match self.kind {
            FixedKind::Flash { .. } => MeshSlot::Star,
            FixedKind::Mark { .. } => MeshSlot::Quad,
        }
    }
}

/// The meshes effects draw, each in its own reserved id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum MeshSlot {
    Kite = 0,
    Star = 1,
    Quad = 2,
}

impl MeshSlot {
    fn id(self) -> WorldMeshId {
        WorldMeshId(EFFECT_MESH_IDS_FROM + self as u64)
    }
}

#[derive(Debug)]
enum Effect {
    Streak(Streak),
    Fixed(Fixed),
}

/// Streaks, flashes and marks under one cap, one handle counter and one draw path.
#[derive(Debug)]
pub struct Effects {
    live: BTreeMap<EffectHandle, Effect>,
    next_handle: u64,
    cap: usize,
    /// Whether each mesh is uploaded, indexed by `MeshSlot`.
    uploaded: [bool; 3],
}

impl Default for Effects {
    fn default() -> Self {
        Self {
            live: BTreeMap::new(),
            next_handle: 0,
            cap: CAPS[DEFAULT_QUALITY],
            uploaded: [false; 3],
        }
    }
}

impl Effects {
    /// Start a streak. At the quality's cap the handle is inert: never stored, so placing
    /// or ending it does nothing.
    pub fn begin_streak(&mut self, style: StreakStyle) -> EffectHandle {
        let handle = self.issue_handle();
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

    fn issue_handle(&mut self) -> EffectHandle {
        let handle = EffectHandle(self.next_handle);
        self.next_handle += 1;
        handle
    }

    /// Spawn a flash at `at`: a four-point star facing the eye, turned by `roll` radians
    /// and shrinking linearly to nothing over the style's lifetime. Refused (nothing
    /// spawns) for a non-finite input, a non-positive size or a zero lifetime; at the cap
    /// it replaces the oldest flash or mark, or is refused when only streaks are live.
    pub fn flash(&mut self, at: Vec3, roll: f32, style: FlashStyle) {
        self.spawn_fixed(Fixed {
            kind: FixedKind::Flash { roll },
            at,
            size: style.size,
            material: style.material,
            lifetime: style.lifetime,
            fade: style.lifetime,
            age: Duration::ZERO,
        });
    }

    /// Spawn a mark at `at` lying on a surface with `normal`, turned by `roll` radians
    /// about it and lifted off the surface by `MARK_OFFSET`. Refused (nothing spawns)
    /// for a non-finite or zero normal or other input, a non-positive size or a zero
    /// lifetime; at the cap it replaces the oldest flash or mark, or is refused when
    /// only streaks are live.
    pub fn mark(&mut self, at: Vec3, normal: Vec3, roll: f32, style: MarkStyle) {
        self.spawn_fixed(Fixed {
            // Zero for a zero or non-finite normal, which `Fixed::is_valid` refuses.
            kind: FixedKind::Mark {
                normal: normal.normalize_or_zero(),
                roll,
            },
            at,
            size: style.size,
            material: style.material,
            lifetime: style.lifetime,
            fade: style.fade.min(style.lifetime),
            age: Duration::ZERO,
        });
    }

    fn spawn_fixed(&mut self, fixed: Fixed) {
        if self.cap == 0 || !fixed.is_valid() {
            return;
        }
        let over = (self.live.len() + 1).saturating_sub(self.cap);
        let evictable = self
            .live
            .values()
            .filter(|effect| matches!(effect, Effect::Fixed(_)))
            .take(over)
            .count();
        if evictable < over {
            return;
        }
        for _ in 0..over {
            let oldest = self
                .live
                .iter()
                .find_map(|(handle, effect)| matches!(effect, Effect::Fixed(_)).then_some(*handle));
            if let Some(handle) = oldest {
                self.live.remove(&handle);
            }
        }
        let handle = self.issue_handle();
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
    /// `max_age` and drop the finished ones.
    pub fn advance(&mut self, dt: Duration) {
        self.live.retain(|_, effect| match effect {
            Effect::Streak(streak) => {
                streak.age += dt;
                if streak.age >= streak.style.max_age {
                    streak.ended_at.get_or_insert(streak.style.max_age);
                }
                !streak.finished_fading()
            }
            Effect::Fixed(fixed) => {
                fixed.age += dt;
                fixed.age < fixed.lifetime
            }
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
    /// unshadowed, facing `eye`.
    pub(crate) fn draw(&mut self, meshes: &mut WorldMeshes, eye: Vec3) {
        for effect in self.live.values_mut() {
            let placement = match effect {
                Effect::Streak(streak) => {
                    let placed = std::mem::take(&mut streak.placed_this_frame);
                    if !placed && streak.ended_at.is_none() {
                        continue;
                    }
                    let Some(heading) = streak.heading else {
                        continue;
                    };
                    let width = streak.style.width * streak.width_scale();
                    streak_transform(streak.head, heading, streak.style.length, width, eye)
                        .map(|transform| (MeshSlot::Kite, transform, streak.style.material))
                }
                Effect::Fixed(fixed) => fixed
                    .transform(eye)
                    .map(|transform| (fixed.mesh(), transform, fixed.material)),
            };
            let Some((slot, transform, material)) = placement else {
                continue;
            };
            if !std::mem::replace(&mut self.uploaded[slot as usize], true) {
                meshes.upsert_movable(slot.id(), effect_mesh(slot));
            }
            meshes.place_unshadowed(slot.id(), transform, material);
        }
    }
}

/// A mesh facing +Z with flat channels, from positions and counter-clockwise indices.
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

fn effect_mesh(slot: MeshSlot) -> WorldMesh {
    match slot {
        // A kite from the tail at the origin to the head at x = 1.
        MeshSlot::Kite => flat_mesh(
            vec![
                [0.0, 0.0, 0.0],
                [0.8, -0.5, 0.0],
                [1.0, 0.0, 0.0],
                [0.8, 0.5, 0.0],
            ],
            vec![0, 1, 2, 0, 2, 3],
        ),
        // Four tips on the axes, half a unit out, alternating with four inner corners.
        MeshSlot::Star => flat_mesh(
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
        ),
        // A unit square centred on the origin.
        MeshSlot::Quad => flat_mesh(
            vec![
                [-0.5, -0.5, 0.0],
                [0.5, -0.5, 0.0],
                [0.5, 0.5, 0.0],
                [-0.5, 0.5, 0.0],
            ],
            vec![0, 1, 2, 0, 2, 3],
        ),
    }
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

        /// Like `frame`, with the backend mesh handle each instance was drawn on.
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

    fn assert_all_finite(instance: &InstanceGpu) {
        assert!(
            instance.model.iter().flatten().all(|v| v.is_finite()),
            "non-finite model {:?}",
            instance.model
        );
    }

    #[test]
    fn a_flash_shrinks_linearly_and_expires() {
        let mut rig = Rig::new();
        let at = Vec3::new(4.0, 1.0, -2.0);
        rig.effects.flash(at, 0.0, flash_style(0.4, ms(50)));

        let full = rig.frame(EYE);
        rig.effects.advance(ms(25));
        let half = rig.frame(EYE);
        rig.effects.advance(ms(25));
        let gone = rig.frame(EYE);

        assert_eq!(full.len(), 1);
        assert!((x_axis(&full[0]).length() - 0.4).abs() < EPS);
        assert!((y_axis(&full[0]).length() - 0.4).abs() < EPS);
        assert_near(translation(&full[0]), at);
        assert_eq!(full[0].material, 3);
        assert_eq!(half.len(), 1);
        assert!(
            (x_axis(&half[0]).length() - 0.2).abs() < EPS,
            "half size, got {}",
            x_axis(&half[0]).length()
        );
        assert!((y_axis(&half[0]).length() - 0.2).abs() < EPS);
        assert_near(translation(&half[0]), at);
        assert!(gone.is_empty());
        assert_eq!(rig.effects.live(), 0);
    }

    #[test]
    fn a_flash_faces_the_eye() {
        let mut rig = Rig::new();
        let at = Vec3::new(4.0, 1.0, -2.0);
        rig.effects.flash(at, 0.0, flash_style(0.4, ms(50)));
        rig.effects
            .flash(at, std::f32::consts::FRAC_PI_2, flash_style(0.4, ms(50)));
        rig.effects
            .flash(at, std::f32::consts::PI, flash_style(0.4, ms(50)));

        let drawn = rig.frame(EYE);

        assert_eq!(drawn.len(), 3);
        let to_eye = (EYE - at).normalize();
        for instance in &drawn {
            assert_all_finite(instance);
            assert_near(x_axis(instance).cross(y_axis(instance)).normalize(), to_eye);
            assert!(
                x_axis(instance).dot(to_eye).abs() < EPS,
                "X lies in the facing plane"
            );
            assert!(
                y_axis(instance).dot(to_eye).abs() < EPS,
                "Y lies in the facing plane"
            );
            assert!(x_axis(instance).dot(y_axis(instance)).abs() < EPS);
        }
        let (x0, x1, x2) = (
            x_axis(&drawn[0]).normalize(),
            x_axis(&drawn[1]).normalize(),
            x_axis(&drawn[2]).normalize(),
        );
        assert!(x0.dot(x1).abs() < EPS, "a quarter turn is perpendicular");
        assert!((x0.dot(x2) + 1.0).abs() < EPS, "a half turn is opposite");
    }

    #[test]
    fn a_flash_faces_a_different_eye_each_frame() {
        let mut rig = Rig::new();
        let at = Vec3::new(4.0, 1.0, -2.0);
        rig.effects.flash(at, 0.0, flash_style(0.4, ms(50)));
        let other_eye = Vec3::new(-3.0, 7.0, 5.0);

        rig.frame(EYE);
        let drawn = rig.frame(other_eye);

        assert_eq!(drawn.len(), 1);
        let normal = x_axis(&drawn[0]).cross(y_axis(&drawn[0])).normalize();
        assert_near(normal, (other_eye - at).normalize());
    }

    #[test]
    fn a_mark_lies_along_its_normal_offset_by_the_mark_offset() {
        let mut rig = Rig::new();
        let at = Vec3::new(4.0, 1.0, -2.0);
        let normal = Vec3::new(1.0, 2.0, -2.0).normalize();
        let secs = Duration::from_secs;
        rig.effects
            .mark(at, normal, 0.0, mark_style(2.0, secs(10), secs(1)));
        rig.effects
            .mark(at, normal, 1.0, mark_style(2.0, secs(10), secs(1)));

        let drawn = rig.frame(EYE);

        assert_eq!(MARK_OFFSET, 0.01);
        assert_eq!(drawn.len(), 2);
        let base = normal.any_orthonormal_vector();
        for (instance, roll) in drawn.iter().zip([0.0, 1.0]) {
            assert_near(translation(instance), at + normal * MARK_OFFSET);
            assert_near(z_axis(instance), normal);
            let x = glam::Quat::from_axis_angle(normal, roll) * base;
            assert_near(x_axis(instance), x * 2.0);
            assert_near(y_axis(instance), normal.cross(x) * 2.0);
            assert_eq!(instance.material, 5);
        }
    }

    #[test]
    fn a_mark_holds_full_size_then_shrinks_out_in_its_fade() {
        let mut rig = Rig::new();
        let secs = Duration::from_secs;
        rig.effects
            .mark(Vec3::X, Vec3::Y, 0.0, mark_style(2.0, secs(10), secs(1)));

        let young = rig.frame(EYE);
        rig.effects.advance(secs(9));
        let full = rig.frame(EYE);
        rig.effects.advance(ms(500));
        let half = rig.frame(EYE);
        rig.effects.advance(ms(500));
        let gone = rig.frame(EYE);

        assert_eq!(young.len(), 1);
        assert!((x_axis(&young[0]).length() - 2.0).abs() < EPS, "full at 0s");
        assert_eq!(full.len(), 1);
        assert!((x_axis(&full[0]).length() - 2.0).abs() < EPS, "full at 9s");
        assert_eq!(half.len(), 1);
        assert!(
            (x_axis(&half[0]).length() - 1.0).abs() < EPS,
            "half at 9.5s"
        );
        assert!((y_axis(&half[0]).length() - 1.0).abs() < EPS);
        assert!(gone.is_empty(), "gone at 10s");
        assert_eq!(rig.effects.live(), 0);
    }

    #[test]
    fn a_full_cap_drops_the_oldest_fixed_lifetime_effect() {
        let mut rig = Rig::new();
        rig.effects.set_quality(1);
        let secs = Duration::from_secs;
        for i in 0..32u8 {
            let at = Vec3::new(f32::from(i), 0.0, 0.0);
            rig.effects
                .mark(at, Vec3::Y, 0.0, mark_style(1.0, secs(10), secs(1)));
        }
        assert_eq!(rig.effects.live(), 32);

        rig.effects.mark(
            Vec3::new(100.0, 0.0, 0.0),
            Vec3::Y,
            0.0,
            mark_style(1.0, secs(10), secs(1)),
        );
        let drawn = rig.frame(EYE);

        assert_eq!(rig.effects.live(), 32);
        let mut xs: Vec<i32> = drawn
            .iter()
            .map(|i| translation(i).x.round() as i32)
            .collect();
        xs.sort_unstable();
        let mut expected: Vec<i32> = (1..32).collect();
        expected.push(100);
        assert_eq!(xs, expected, "the oldest (x = 0) is the one gone");
    }

    #[test]
    fn a_full_cap_drops_the_oldest_whichever_kind_it_and_the_newcomer_are() {
        fn spawn(rig: &mut Rig, flash: bool, x: f32) {
            let at = Vec3::new(x, 0.0, 0.0);
            let secs = Duration::from_secs;
            if flash {
                rig.effects.flash(at, 0.0, flash_style(1.0, secs(10)));
            } else {
                rig.effects
                    .mark(at, Vec3::Y, 0.0, mark_style(1.0, secs(10), secs(1)));
            }
        }

        for (old_is_flash, new_is_flash) in
            [(true, false), (false, true), (true, true), (false, false)]
        {
            let mut rig = Rig::new();
            rig.effects.set_quality(1);
            spawn(&mut rig, old_is_flash, -1.0);
            for i in 0..31u8 {
                spawn(&mut rig, !old_is_flash, f32::from(i));
            }
            assert_eq!(rig.effects.live(), 32);

            spawn(&mut rig, new_is_flash, 100.0);
            let drawn = rig.frame(EYE);

            let case = format!("old flash {old_is_flash}, new flash {new_is_flash}");
            assert_eq!(rig.effects.live(), 32, "{case}");
            assert_eq!(drawn.len(), 32, "{case}");
            assert!(
                drawn.iter().all(|i| translation(i).x > -0.5),
                "{case}: the oldest is gone"
            );
            assert!(
                drawn.iter().any(|i| translation(i).x > 50.0),
                "{case}: the newcomer draws"
            );
        }
    }

    #[test]
    fn a_full_cap_of_streaks_refuses_a_mark_and_keeps_every_streak() {
        let mut rig = Rig::new();
        rig.effects.set_quality(1);
        let handles: Vec<EffectHandle> = (0..32)
            .map(|_| rig.effects.begin_streak(style(2.0, 0.1)))
            .collect();
        for handle in &handles {
            rig.effects.place_streak(*handle, Vec3::X * 5.0, Vec3::X);
        }
        let secs = Duration::from_secs;

        rig.effects.mark(
            Vec3::new(100.0, 0.0, 0.0),
            Vec3::Y,
            0.0,
            mark_style(1.0, secs(10), secs(1)),
        );
        rig.effects
            .flash(Vec3::new(100.0, 0.0, 0.0), 0.0, flash_style(1.0, secs(10)));
        let drawn = rig.frame(EYE);

        assert_eq!(rig.effects.live(), 32);
        assert_eq!(drawn.len(), 32, "every streak still draws and nothing else");
        assert!(drawn.iter().all(|i| translation(i).x < 50.0));
    }

    #[test]
    fn quality_zero_refuses_flash_and_mark() {
        let mut rig = Rig::new();
        rig.effects.set_quality(0);
        let secs = Duration::from_secs;

        rig.effects.flash(Vec3::X, 0.0, flash_style(1.0, secs(1)));
        rig.effects
            .mark(Vec3::X, Vec3::Y, 0.0, mark_style(1.0, secs(1), ms(100)));

        assert_eq!(rig.effects.live(), 0);
        assert!(rig.frame(EYE).is_empty());
    }

    #[test]
    fn quality_zero_clears_live_flashes_and_marks() {
        let mut rig = Rig::new();
        let secs = Duration::from_secs;
        rig.effects.flash(Vec3::X, 0.0, flash_style(1.0, secs(1)));
        rig.effects
            .mark(Vec3::X, Vec3::Y, 0.0, mark_style(1.0, secs(1), ms(100)));
        assert_eq!(rig.effects.live(), 2);

        rig.effects.set_quality(0);

        assert_eq!(rig.effects.live(), 0);
        assert!(rig.frame(EYE).is_empty());
    }

    #[test]
    fn flashes_and_marks_count_as_live_and_share_handles_with_streaks() {
        let mut rig = Rig::new();
        let secs = Duration::from_secs;
        let before = rig.effects.begin_streak(style(2.0, 0.1));

        rig.effects.flash(Vec3::X, 0.0, flash_style(1.0, secs(1)));
        rig.effects
            .mark(Vec3::X, Vec3::Y, 0.0, mark_style(1.0, secs(1), ms(100)));
        let after = rig.effects.begin_streak(style(2.0, 0.1));
        rig.effects.place_streak(before, Vec3::X * 5.0, Vec3::X);
        rig.effects.place_streak(after, Vec3::X * 6.0, Vec3::X);

        assert_eq!(rig.effects.live(), 4);
        assert!(after > before);
        assert_eq!(
            rig.frame(EYE).len(),
            4,
            "both streaks, the flash and the mark"
        );
    }

    fn spawn_script(rig: &mut Rig) -> Vec<Vec<InstanceGpu>> {
        let secs = Duration::from_secs;
        rig.effects
            .flash(Vec3::new(1.0, 2.0, 3.0), 0.7, flash_style(0.5, ms(80)));
        rig.effects.mark(
            Vec3::new(-4.0, 0.0, 1.0),
            Vec3::new(0.0, 0.6, 0.8),
            2.1,
            mark_style(1.5, secs(2), ms(500)),
        );
        let mut frames = vec![rig.frame(EYE)];
        for _ in 0..4 {
            rig.effects.advance(ms(40));
            frames.push(rig.frame(EYE));
        }
        frames
    }

    #[test]
    fn the_same_spawns_draw_the_same_instances() {
        let first = spawn_script(&mut Rig::new());
        let second = spawn_script(&mut Rig::new());

        assert_eq!(first.len(), second.len());
        assert!(first.iter().any(|frame| frame.len() == 2), "both draw");
        for (a, b) in first.iter().zip(&second) {
            assert_eq!(a.len(), b.len());
            for (a, b) in a.iter().zip(b) {
                assert_eq!(a.model, b.model);
                assert_eq!(a.material, b.material);
            }
        }
    }

    #[test]
    fn the_star_mesh_is_eight_vertices_in_the_xy_plane_wound_toward_plus_z() {
        let mut rig = Rig::new();
        rig.effects.flash(Vec3::X, 0.0, flash_style(1.0, ms(50)));

        rig.frame(EYE);

        assert_eq!(rig.backend.registered.len(), 1);
        let (positions, indices) = &rig.backend.registered[0];
        assert_eq!(positions.len(), 8, "a four-point star");
        assert!(positions.iter().all(|p| p[2] == 0.0), "in the XY plane");
        let centroid = positions.iter().map(|p| Vec3::from(*p)).sum::<Vec3>() / 8.0;
        assert_near(centroid, Vec3::ZERO);
        for axis in 0..2 {
            let low = positions.iter().map(|p| p[axis]).fold(f32::MAX, f32::min);
            let high = positions.iter().map(|p| p[axis]).fold(f32::MIN, f32::max);
            assert!(
                (high - low - 1.0).abs() < EPS,
                "tip to tip is 1 along axis {axis}, got {}",
                high - low
            );
        }
        assert!(!indices.is_empty() && indices.len() % 3 == 0);
        for triangle in indices.chunks(3) {
            let [a, b, c] = [0, 1, 2].map(|i| Vec3::from(positions[triangle[i] as usize]));
            assert!(
                (b - a).cross(c - a).z > 0.0,
                "triangle {triangle:?} is counter-clockwise seen from +Z"
            );
        }
    }

    #[test]
    fn the_quad_mesh_is_a_unit_square_in_the_xy_plane_wound_toward_plus_z() {
        let mut rig = Rig::new();
        rig.effects
            .mark(Vec3::X, Vec3::Y, 0.0, mark_style(1.0, ms(500), ms(100)));

        rig.frame(EYE);

        assert_eq!(rig.backend.registered.len(), 1);
        let (positions, indices) = &rig.backend.registered[0];
        assert_eq!(positions.len(), 4);
        assert!(positions.iter().all(|p| p[2] == 0.0), "in the XY plane");
        assert_eq!(indices.len(), 6, "two triangles");
        for t in indices.chunks(3) {
            let [a, b, c] = [0, 1, 2].map(|i| Vec3::from(positions[t[i] as usize]));
            assert!(
                (b - a).cross(c - a).z > 0.0,
                "counter-clockwise seen from +Z"
            );
        }
        for corner in [(-0.5, -0.5), (0.5, -0.5), (0.5, 0.5), (-0.5, 0.5)] {
            assert!(
                positions
                    .iter()
                    .any(|p| p[0] == corner.0 && p[1] == corner.1),
                "missing corner {corner:?} in {positions:?}"
            );
        }
    }

    #[test]
    fn flash_and_mark_meshes_are_registered_once_each() {
        let mut rig = Rig::new();
        rig.effects.flash(Vec3::X, 0.0, flash_style(1.0, ms(500)));
        rig.effects
            .mark(Vec3::X, Vec3::Y, 0.0, mark_style(1.0, ms(500), ms(100)));

        for _ in 0..3 {
            rig.frame(EYE);
            rig.effects.advance(ms(10));
        }

        assert_eq!(rig.backend.registered.len(), 2);
    }

    #[test]
    fn a_full_cap_of_streaks_and_one_mark_drops_the_old_mark_for_a_new_one() {
        let mut rig = Rig::new();
        rig.effects.set_quality(1);
        let secs = Duration::from_secs;
        let handles: Vec<EffectHandle> = (0..31)
            .map(|_| rig.effects.begin_streak(style(2.0, 0.1)))
            .collect();
        rig.effects.mark(
            Vec3::new(-1.0, 0.0, 0.0),
            Vec3::Y,
            0.0,
            mark_style(1.0, secs(10), secs(1)),
        );
        for handle in &handles {
            rig.effects.place_streak(*handle, Vec3::X * 5.0, Vec3::X);
        }

        rig.effects.mark(
            Vec3::new(100.0, 0.0, 0.0),
            Vec3::Y,
            0.0,
            mark_style(1.0, secs(10), secs(1)),
        );
        let drawn = rig.frame(EYE);

        assert_eq!(rig.effects.live(), 32);
        assert_eq!(drawn.len(), 32, "31 streaks and the new mark");
        assert!(
            drawn.iter().all(|i| translation(i).x > -0.5),
            "the old mark is gone"
        );
        assert_eq!(
            drawn.iter().filter(|i| translation(i).x > 50.0).count(),
            1,
            "the new mark draws"
        );
    }

    #[test]
    fn a_streak_a_flash_and_a_mark_draw_on_three_distinct_meshes() {
        let mut rig = Rig::new();
        let secs = Duration::from_secs;
        let streak = rig.effects.begin_streak(style(2.0, 0.1));
        rig.effects.place_streak(streak, Vec3::X * 5.0, Vec3::X);
        rig.effects.flash(Vec3::X, 0.0, flash_style(1.0, secs(1)));
        rig.effects
            .mark(Vec3::X, Vec3::Y, 0.0, mark_style(1.0, secs(1), ms(100)));

        let drawn = rig.frame_with_meshes(EYE);

        assert_eq!(rig.backend.registered.len(), 3);
        assert_eq!(drawn.len(), 3);
        let mut handles: Vec<u32> = drawn.iter().map(|(handle, _)| *handle).collect();
        handles.sort_unstable();
        handles.dedup();
        assert_eq!(handles.len(), 3, "each kind has its own mesh");
    }

    #[test]
    fn a_non_finite_position_or_roll_spawns_nothing_and_costs_no_handle() {
        let bad = [f32::NAN, f32::INFINITY, f32::NEG_INFINITY];
        let secs = Duration::from_secs;
        let mut rig = Rig::new();

        for value in bad {
            let at = Vec3::new(value, 0.0, 0.0);
            rig.effects.flash(at, 0.0, flash_style(1.0, secs(1)));
            rig.effects.flash(Vec3::X, value, flash_style(1.0, secs(1)));
            rig.effects
                .mark(at, Vec3::Y, 0.0, mark_style(1.0, secs(1), ms(100)));
            rig.effects
                .mark(Vec3::X, Vec3::Y, value, mark_style(1.0, secs(1), ms(100)));
        }

        assert_eq!(rig.effects.live(), 0);
        assert!(rig.frame(EYE).is_empty());
        assert_eq!(
            rig.effects.begin_streak(style(2.0, 0.1)),
            EffectHandle(0),
            "refusals leave the counter alone"
        );
    }

    #[test]
    fn a_zero_or_nan_mark_normal_is_refused() {
        let mut rig = Rig::new();

        for normal in [
            Vec3::ZERO,
            Vec3::new(f32::NAN, 0.0, 1.0),
            Vec3::splat(f32::NAN),
        ] {
            rig.effects
                .mark(Vec3::X, normal, 0.0, mark_style(1.0, ms(500), ms(100)));
        }

        assert_eq!(rig.effects.live(), 0);
        assert!(rig.frame(EYE).is_empty());
    }

    #[test]
    fn a_non_unit_mark_normal_behaves_like_its_unit_direction() {
        let at = Vec3::new(1.0, 2.0, 3.0);
        let mut long = Rig::new();
        let mut unit = Rig::new();
        long.effects.mark(
            at,
            Vec3::new(0.0, 0.0, 5.0),
            0.4,
            mark_style(2.0, ms(500), ms(100)),
        );
        unit.effects
            .mark(at, Vec3::Z, 0.4, mark_style(2.0, ms(500), ms(100)));

        let long = long.frame(EYE);
        let unit = unit.frame(EYE);

        assert_eq!(long.len(), 1);
        assert_eq!(unit.len(), 1);
        assert_near(translation(&long[0]), at + Vec3::Z * MARK_OFFSET);
        assert!((z_axis(&long[0]).length() - 1.0).abs() < EPS);
        assert_near(x_axis(&long[0]), x_axis(&unit[0]));
        assert_near(y_axis(&long[0]), y_axis(&unit[0]));
        assert_near(z_axis(&long[0]), z_axis(&unit[0]));
    }

    #[test]
    fn a_non_positive_or_nan_size_is_refused_for_flash_and_mark() {
        let mut rig = Rig::new();

        for size in [0.0, -1.0, f32::NAN, f32::INFINITY] {
            rig.effects.flash(Vec3::X, 0.0, flash_style(size, ms(500)));
            rig.effects
                .mark(Vec3::X, Vec3::Y, 0.0, mark_style(size, ms(500), ms(100)));
        }

        assert_eq!(rig.effects.live(), 0);
        assert!(rig.frame(EYE).is_empty());
    }

    #[test]
    fn a_zero_lifetime_is_refused_for_flash_and_mark() {
        let mut rig = Rig::new();

        rig.effects
            .flash(Vec3::X, 0.0, flash_style(1.0, Duration::ZERO));
        rig.effects.mark(
            Vec3::X,
            Vec3::Y,
            0.0,
            mark_style(1.0, Duration::ZERO, Duration::ZERO),
        );

        assert_eq!(rig.effects.live(), 0);
        assert!(rig.frame(EYE).is_empty());
    }

    #[test]
    fn a_mark_fade_longer_than_its_lifetime_clamps_to_the_lifetime() {
        let mut rig = Rig::new();
        let secs = Duration::from_secs;
        rig.effects
            .mark(Vec3::X, Vec3::Y, 0.0, mark_style(2.0, secs(10), secs(20)));

        let start = rig.frame(EYE);
        rig.effects.advance(secs(1));
        let early = rig.frame(EYE);
        rig.effects.advance(secs(4));
        let half = rig.frame(EYE);
        rig.effects.advance(secs(5));
        let gone = rig.frame(EYE);

        assert_eq!(start.len(), 1);
        assert!((x_axis(&start[0]).length() - 2.0).abs() < EPS, "full at 0");
        assert_eq!(early.len(), 1);
        assert!((x_axis(&early[0]).length() - 1.8).abs() < EPS, "0.9 at 1s");
        assert_eq!(half.len(), 1);
        assert!((x_axis(&half[0]).length() - 1.0).abs() < EPS, "half at 5s");
        assert!(gone.is_empty());
        assert_eq!(rig.effects.live(), 0);
    }

    #[test]
    fn a_mark_with_no_fade_is_full_size_until_its_lifetime_then_gone() {
        let mut rig = Rig::new();
        let secs = Duration::from_secs;
        rig.effects.mark(
            Vec3::X,
            Vec3::Y,
            0.0,
            mark_style(2.0, secs(10), Duration::ZERO),
        );

        rig.effects.advance(ms(9_999));
        let last = rig.frame(EYE);
        rig.effects.advance(ms(1));
        let gone = rig.frame(EYE);

        assert_eq!(last.len(), 1);
        assert!((x_axis(&last[0]).length() - 2.0).abs() < EPS);
        assert!(gone.is_empty());
        assert_eq!(rig.effects.live(), 0);
    }

    #[test]
    fn a_flash_at_the_eye_draws_nothing_and_lives_to_its_lifetime() {
        let mut rig = Rig::new();
        let at = Vec3::new(4.0, 1.0, -2.0);
        rig.effects.flash(at, 0.3, flash_style(1.0, ms(50)));

        let at_eye = rig.frame(at);
        rig.effects.advance(ms(25));
        let still = rig.frame(at);
        let from_afar = rig.frame(EYE);
        rig.effects.advance(ms(25));

        assert!(at_eye.is_empty());
        assert!(still.is_empty());
        assert_eq!(from_afar.len(), 1, "it was live all along");
        assert_all_finite(&from_afar[0]);
        assert_eq!(rig.effects.live(), 0);
    }

    #[test]
    fn lowering_quality_over_the_cap_makes_a_new_mark_evict_enough_oldest_to_fit() {
        let mut rig = Rig::new();
        let secs = Duration::from_secs;
        for i in 0..40u8 {
            let at = Vec3::new(f32::from(i), 0.0, 0.0);
            rig.effects
                .mark(at, Vec3::Y, 0.0, mark_style(1.0, secs(10), secs(1)));
        }
        rig.effects.set_quality(1);
        assert_eq!(rig.effects.live(), 40, "kept though over the new cap");

        rig.effects.mark(
            Vec3::new(100.0, 0.0, 0.0),
            Vec3::Y,
            0.0,
            mark_style(1.0, secs(10), secs(1)),
        );
        let drawn = rig.frame(EYE);

        assert_eq!(rig.effects.live(), 32);
        let mut xs: Vec<i32> = drawn
            .iter()
            .map(|i| translation(i).x.round() as i32)
            .collect();
        xs.sort_unstable();
        let mut expected: Vec<i32> = (9..40).collect();
        expected.push(100);
        assert_eq!(xs, expected, "the nine oldest are gone, the newcomer is in");
    }

    #[test]
    fn roll_turns_flash_and_mark_the_same_right_handed_way() {
        let quarter = std::f32::consts::FRAC_PI_2;
        let at = Vec3::new(4.0, 1.0, -2.0);
        let normal = Vec3::new(1.0, 2.0, -2.0).normalize();
        let mut rig = Rig::new();
        rig.effects.flash(at, 0.0, flash_style(1.0, ms(500)));
        rig.effects.flash(at, quarter, flash_style(1.0, ms(500)));
        rig.effects
            .mark(at, normal, 0.0, mark_style(1.0, ms(500), ms(100)));
        rig.effects
            .mark(at, normal, quarter, mark_style(1.0, ms(500), ms(100)));

        let drawn = rig.frame(EYE);

        assert_eq!(drawn.len(), 4);
        // Draw order is by mesh (star, then quad), then spawn order.
        assert_near(x_axis(&drawn[1]), y_axis(&drawn[0]));
        assert_near(x_axis(&drawn[3]), y_axis(&drawn[2]));
    }

    #[test]
    fn a_mark_is_full_size_at_the_start_of_its_fade_and_smaller_a_millisecond_later() {
        let mut rig = Rig::new();
        let secs = Duration::from_secs;
        rig.effects
            .mark(Vec3::X, Vec3::Y, 0.0, mark_style(2.0, secs(10), secs(1)));

        rig.effects.advance(secs(9));
        let at_start = rig.frame(EYE);
        rig.effects.advance(ms(1));
        let after = rig.frame(EYE);

        assert!((x_axis(&at_start[0]).length() - 2.0).abs() < EPS);
        assert!(
            x_axis(&after[0]).length() < 2.0 - 1e-3,
            "got {}",
            x_axis(&after[0]).length()
        );
    }

    #[test]
    fn the_kite_star_and_quad_take_the_first_three_reserved_mesh_ids() {
        let mut rig = Rig::new();
        let streak = rig.effects.begin_streak(style(2.0, 0.1));
        rig.effects.place_streak(streak, Vec3::X * 5.0, Vec3::X);
        rig.effects.flash(Vec3::X, 0.0, flash_style(1.0, ms(500)));
        rig.effects
            .mark(Vec3::X, Vec3::Y, 0.0, mark_style(1.0, ms(500), ms(100)));
        rig.frame(EYE);
        assert_eq!(rig.meshes.live_len(), 3);

        for below in [1, 2] {
            rig.meshes.remove(WorldMeshId(EFFECT_MESH_IDS_FROM - below));
            rig.meshes.flush(&mut rig.backend);
            assert_eq!(rig.meshes.live_len(), 3, "nothing lives below the range");
        }
        for (offset, left) in [(0, 2), (1, 1), (2, 0)] {
            rig.meshes
                .remove(WorldMeshId(EFFECT_MESH_IDS_FROM + offset));
            rig.meshes.flush(&mut rig.backend);
            assert_eq!(rig.meshes.live_len(), left, "id FROM + {offset} was live");
        }
    }

    fn unit_normal() -> impl Strategy<Value = Vec3> {
        prop_oneof![
            Just(Vec3::X),
            Just(Vec3::Y),
            Just(Vec3::Z),
            Just(Vec3::NEG_X),
            Just(Vec3::NEG_Y),
            Just(Vec3::NEG_Z),
            (-1.0f32..1.0, -1.0f32..1.0, -1.0f32..1.0)
                .prop_filter_map("a direction", |(x, y, z)| Vec3::new(x, y, z)
                    .try_normalize()),
        ]
    }

    proptest! {
        #[test]
        fn a_mark_basis_is_finite_for_any_normal(
            normal in unit_normal(),
            roll in -20.0f32..20.0,
        ) {
            let mut rig = Rig::new();
            rig.effects.mark(
                Vec3::new(1.0, 2.0, 3.0),
                normal,
                roll,
                mark_style(1.5, ms(500), ms(100)),
            );

            let drawn = rig.frame(EYE);

            prop_assert_eq!(drawn.len(), 1);
            prop_assert!(drawn[0].model.iter().flatten().all(|v| v.is_finite()));
            prop_assert!((x_axis(&drawn[0]).length() - 1.5).abs() < 1e-3);
            prop_assert!((y_axis(&drawn[0]).length() - 1.5).abs() < 1e-3);
            prop_assert!((z_axis(&drawn[0]) - normal).length() < 1e-3);
            prop_assert!(x_axis(&drawn[0]).dot(normal).abs() < 1e-3);
            prop_assert!(y_axis(&drawn[0]).dot(normal).abs() < 1e-3);
            prop_assert!(x_axis(&drawn[0]).dot(y_axis(&drawn[0])).abs() < 1e-3);
        }
    }
}
