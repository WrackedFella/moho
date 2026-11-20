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

/// How far a mark floats off its surface, so a flat quad does not z-fight a flat wall.
pub(crate) const MARK_OFFSET: f32 = 0.01;

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
pub struct Effects {
    streaks: BTreeMap<EffectHandle, Streak>,
    next_handle: u64,
    cap: usize,
    mesh_uploaded: bool,
}

impl Default for Effects {
    fn default() -> Self {
        Self {
            streaks: BTreeMap::new(),
            next_handle: 0,
            cap: CAPS[DEFAULT_QUALITY],
            mesh_uploaded: false,
        }
    }
}

impl Effects {
    /// Start a streak. At the quality's cap the handle is inert: never stored, so placing
    /// or ending it does nothing.
    pub fn begin_streak(&mut self, style: StreakStyle) -> EffectHandle {
        let handle = EffectHandle(self.next_handle);
        self.next_handle += 1;
        if self.streaks.len() < self.cap {
            self.streaks.insert(
                handle,
                Streak {
                    style,
                    head: Vec3::ZERO,
                    heading: None,
                    placed_this_frame: false,
                    age: Duration::ZERO,
                    ended_at: None,
                },
            );
        }
        handle
    }

    /// Spawn a camera-facing flash at `at`, rolled by `roll` radians.
    pub fn flash(&mut self, _at: Vec3, _roll: f32, _style: FlashStyle) {}

    /// Spawn a flat mark at `at` facing along `normal`, rolled by `roll` radians.
    pub fn mark(&mut self, at: Vec3, normal: Vec3, _roll: f32, _style: MarkStyle) {
        let _centre = at + normal * MARK_OFFSET;
    }

    /// Place the streak's head for this frame. A zero-length heading keeps the last one;
    /// a stale or unknown handle is ignored.
    pub fn place_streak(&mut self, handle: EffectHandle, head: Vec3, heading: Vec3) {
        let Some(streak) = self.streaks.get_mut(&handle) else {
            return;
        };
        streak.head = head;
        streak.heading = heading.try_normalize().or(streak.heading);
        streak.placed_this_frame = true;
    }

    /// Start the fade; the streak then draws at its last placement until it is gone.
    pub fn end(&mut self, handle: EffectHandle) {
        if let Some(streak) = self.streaks.get_mut(&handle) {
            streak.ended_at.get_or_insert(streak.age);
        }
    }

    /// Age every effect by the frame time, start the fade of any past its `max_age` and
    /// drop the finished ones.
    pub fn advance(&mut self, dt: Duration) {
        for streak in self.streaks.values_mut() {
            streak.age += dt;
            if streak.age >= streak.style.max_age {
                streak.ended_at.get_or_insert(streak.style.max_age);
            }
        }
        self.streaks.retain(|_, streak| !streak.finished_fading());
    }

    /// Cap new effects for `quality` (0 off to 4 ultra; higher clamps). Lowering keeps
    /// what is live; 0 clears it.
    pub fn set_quality(&mut self, quality: u32) {
        self.cap = CAPS
            [usize::try_from(quality.min(MAX_QUALITY)).expect("a quality of at most 4 fits usize")];
        if self.cap == 0 {
            self.streaks.clear();
        }
    }

    /// How many effects are live, fading ones included.
    pub fn live(&self) -> usize {
        self.streaks.len()
    }

    /// Place every streak placed this frame or fading, unshadowed, facing `eye`.
    pub(crate) fn draw(&mut self, meshes: &mut WorldMeshes, eye: Vec3) {
        for streak in self.streaks.values_mut() {
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
                if !std::mem::replace(&mut self.mesh_uploaded, true) {
                    meshes.upsert_movable(WorldMeshId(EFFECT_MESH_IDS_FROM), streak_mesh());
                }
                meshes.place_unshadowed(
                    WorldMeshId(EFFECT_MESH_IDS_FROM),
                    transform,
                    streak.style.material,
                );
            }
        }
    }
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

    fn z_axis(instance: &InstanceGpu) -> Vec3 {
        let [x, y, z, _] = instance.model[2];
        Vec3::new(x, y, z)
    }

    fn is_finite(instance: &InstanceGpu) -> bool {
        instance.model.iter().flatten().all(|v| v.is_finite())
    }

    impl Rig {
        /// Like `frame`, with the backend mesh handle each instance draws.
        fn frame_with_meshes(&mut self, eye: Vec3) -> Vec<(u32, InstanceGpu)> {
            self.effects.draw(&mut self.meshes, eye);
            self.meshes.flush(&mut self.backend);
            let drawn = self.meshes.unshadowed_draws().collect();
            self.meshes.clear_placements();
            drawn
        }

        fn positions_of(&self, handle: u32) -> &[[f32; 3]] {
            &self.backend.registered[handle as usize - 1].0
        }
    }

    #[test]
    fn a_flash_shrinks_linearly_and_expires() {
        let mut rig = Rig::new();
        let at = Vec3::new(2.0, 1.0, -3.0);
        rig.effects.flash(at, 0.0, flash_style(0.4, ms(50)));

        let full = rig.frame(EYE);
        assert_eq!(full.len(), 1);
        assert_near(translation(&full[0]), at);
        assert!((x_axis(&full[0]).length() - 0.4).abs() < EPS);
        assert!((y_axis(&full[0]).length() - 0.4).abs() < EPS);

        rig.effects.advance(ms(25));
        let half = rig.frame(EYE);
        assert_eq!(half.len(), 1, "drawn without being placed");
        assert_near(translation(&half[0]), at);
        assert!((x_axis(&half[0]).length() - 0.2).abs() < EPS);
        assert!((y_axis(&half[0]).length() - 0.2).abs() < EPS);
        assert_eq!(rig.effects.live(), 1);

        rig.effects.advance(ms(25));
        assert!(rig.frame(EYE).is_empty());
        assert_eq!(rig.effects.live(), 0);
    }

    #[test]
    fn a_flash_faces_the_eye() {
        for roll in [0.0, 1.0, -2.5] {
            let mut rig = Rig::new();
            let at = Vec3::new(2.0, 1.0, -3.0);
            rig.effects.flash(at, roll, flash_style(0.4, ms(50)));

            let drawn = rig.frame(EYE);

            assert_eq!(drawn.len(), 1, "roll {roll}");
            let towards_eye = (EYE - at).normalize();
            let normal = x_axis(&drawn[0]).cross(y_axis(&drawn[0])).normalize();
            assert_near(normal, towards_eye);
            assert!(x_axis(&drawn[0]).dot(y_axis(&drawn[0])).abs() < EPS);
        }
    }

    #[test]
    fn a_flash_follows_the_eye_each_frame() {
        let mut rig = Rig::new();
        let at = Vec3::new(2.0, 1.0, -3.0);
        rig.effects.flash(at, 0.0, flash_style(0.4, ms(50)));
        rig.frame(EYE);

        let other = Vec3::new(-7.0, 2.0, 5.0);
        let drawn = rig.frame(other);

        assert_eq!(drawn.len(), 1);
        let normal = x_axis(&drawn[0]).cross(y_axis(&drawn[0])).normalize();
        assert_near(normal, (other - at).normalize());
    }

    #[test]
    fn a_flash_roll_turns_it_about_the_view_axis() {
        let at = Vec3::new(2.0, 1.0, -3.0);
        let mut rolled = Rig::new();
        rolled.effects.flash(at, 0.0, flash_style(0.4, ms(50)));
        let mut turned = Rig::new();
        turned
            .effects
            .flash(at, std::f32::consts::FRAC_PI_2, flash_style(0.4, ms(50)));

        let a = rolled.frame(EYE);
        let b = turned.frame(EYE);

        assert_eq!(a.len(), 1);
        assert_eq!(b.len(), 1);
        assert!(
            x_axis(&a[0])
                .normalize()
                .dot(x_axis(&b[0]).normalize())
                .abs()
                < 1e-3,
            "a quarter turn puts X where Y was"
        );
    }

    #[test]
    fn flash_and_mark_draw_the_next_two_reserved_meshes_a_star_and_a_unit_quad() {
        let mut rig = Rig::new();
        let streak = rig.effects.begin_streak(style(2.0, 0.1));
        rig.effects.place_streak(streak, Vec3::X * 5.0, Vec3::X);
        rig.effects
            .flash(Vec3::Y, 0.0, flash_style(0.4, Duration::from_secs(1)));
        rig.effects.mark(
            Vec3::Z,
            Vec3::Y,
            0.0,
            mark_style(0.2, Duration::from_secs(10), ms(500)),
        );

        let drawn = rig.frame_with_meshes(EYE);

        assert_eq!(drawn.len(), 3);
        let handles: std::collections::BTreeSet<u32> = drawn.iter().map(|(h, _)| *h).collect();
        assert_eq!(handles.len(), 3, "three distinct meshes");
        let counts: Vec<usize> = drawn
            .iter()
            .map(|(h, _)| rig.positions_of(*h).len())
            .collect();
        assert_eq!(
            counts,
            vec![4, 8, 4],
            "kite, star, quad in reserved-id order"
        );
        assert_eq!(rig.backend.registered.len(), 3);

        rig.frame_with_meshes(EYE);
        assert_eq!(rig.backend.registered.len(), 3, "each registers once");
    }

    #[test]
    fn the_star_mesh_is_flat_in_xy_and_wound_toward_plus_z() {
        let mut rig = Rig::new();
        rig.effects
            .flash(Vec3::Y, 0.0, flash_style(0.4, Duration::from_secs(1)));

        let drawn = rig.frame_with_meshes(EYE);

        assert_eq!(drawn.len(), 1);
        let (positions, indices) = &rig.backend.registered[drawn[0].0 as usize - 1];
        assert_eq!(positions.len(), 8, "four points, tip and notch each");
        assert!(positions.iter().all(|p| p[2] == 0.0), "in the XY plane");
        let centroid = positions.iter().map(|p| Vec3::from(*p)).sum::<Vec3>() / 8.0;
        assert_near(centroid, Vec3::ZERO);
        let mut by_angle: Vec<(f32, f32)> = positions
            .iter()
            .map(|p| (p[1].atan2(p[0]), p[0].hypot(p[1])))
            .collect();
        by_angle.sort_by(|a, b| a.0.total_cmp(&b.0));
        let radii: Vec<f32> = by_angle.iter().map(|(_, r)| *r).collect();
        let (tips, notches): (Vec<f32>, Vec<f32>) = if radii[0] > radii[1] {
            (
                radii.iter().step_by(2).copied().collect(),
                radii.iter().skip(1).step_by(2).copied().collect(),
            )
        } else {
            (
                radii.iter().skip(1).step_by(2).copied().collect(),
                radii.iter().step_by(2).copied().collect(),
            )
        };
        assert!(
            tips.iter().all(|r| (r - 0.5).abs() < EPS),
            "tips at 0.5: {radii:?}"
        );
        assert!(
            notches
                .iter()
                .all(|r| (r - notches[0]).abs() < EPS && *r > 0.0 && *r < 0.5 - EPS),
            "notches share one smaller radius, alternating with the tips: {radii:?}"
        );
        assert!(!indices.is_empty() && indices.len() % 3 == 0);
        for triangle in indices.chunks(3) {
            let [a, b, c] = [0, 1, 2].map(|i| Vec3::from(positions[triangle[i] as usize]));
            assert!((b - a).cross(c - a).z > 0.0, "{triangle:?} is CCW from +Z");
        }
    }

    #[test]
    fn the_quad_mesh_is_a_unit_square_wound_toward_plus_z() {
        let mut rig = Rig::new();
        rig.effects.mark(
            Vec3::Z,
            Vec3::Y,
            0.0,
            mark_style(0.2, Duration::from_secs(10), ms(500)),
        );

        let drawn = rig.frame_with_meshes(EYE);

        assert_eq!(drawn.len(), 1);
        let (positions, indices) = &rig.backend.registered[drawn[0].0 as usize - 1];
        assert_eq!(positions.len(), 4);
        assert_eq!(indices.len(), 6, "two triangles");
        assert!(positions.iter().all(|p| p[2] == 0.0));
        for axis in 0..2 {
            let min = positions.iter().map(|p| p[axis]).fold(f32::MAX, f32::min);
            let max = positions.iter().map(|p| p[axis]).fold(f32::MIN, f32::max);
            assert!((min + 0.5).abs() < EPS, "min on axis {axis} is -0.5");
            assert!((max - 0.5).abs() < EPS, "max on axis {axis} is 0.5");
        }
        let mut area = 0.0;
        for triangle in indices.chunks(3) {
            let [a, b, c] = [0, 1, 2].map(|i| Vec3::from(positions[triangle[i] as usize]));
            assert!((b - a).cross(c - a).z > 0.0, "{triangle:?} is CCW from +Z");
            area += (b - a).cross(c - a).z / 2.0;
        }
        assert!(
            (area - 1.0).abs() < EPS,
            "the triangles cover the unit square"
        );
    }

    #[test]
    fn flash_and_mark_carry_their_material() {
        let mut rig = Rig::new();
        rig.effects.flash(
            Vec3::Y,
            0.0,
            FlashStyle {
                material: 5,
                ..flash_style(0.4, Duration::from_secs(1))
            },
        );
        rig.effects.mark(
            Vec3::Z,
            Vec3::Y,
            0.0,
            MarkStyle {
                material: 9,
                ..mark_style(0.2, Duration::from_secs(10), ms(500))
            },
        );

        let mut materials: Vec<u32> = rig.frame(EYE).iter().map(|i| i.material).collect();
        materials.sort_unstable();

        assert_eq!(materials, vec![5, 9]);
    }

    #[test]
    fn a_mark_lies_along_its_normal_offset_by_the_mark_offset() {
        let mut rig = Rig::new();
        let at = Vec3::new(3.0, 2.0, 1.0);
        let normal = Vec3::new(0.0, 0.6, 0.8);
        rig.effects.mark(
            at,
            normal,
            0.7,
            mark_style(0.2, Duration::from_secs(10), ms(1000)),
        );

        let drawn = rig.frame(EYE);

        assert_eq!(drawn.len(), 1);
        assert_near(translation(&drawn[0]), at + normal * MARK_OFFSET);
        assert!(
            (translation(&drawn[0]) - at).dot(normal) > 0.0,
            "lifted off the surface, not sunk into it"
        );
        assert!((x_axis(&drawn[0]).length() - 0.2).abs() < EPS);
        assert!((y_axis(&drawn[0]).length() - 0.2).abs() < EPS);
        assert!(
            x_axis(&drawn[0]).dot(normal).abs() < EPS,
            "X lies in the surface"
        );
        assert!(
            y_axis(&drawn[0]).dot(normal).abs() < EPS,
            "Y lies in the surface"
        );
        assert!(x_axis(&drawn[0]).dot(y_axis(&drawn[0])).abs() < EPS);
        let facing = x_axis(&drawn[0]).cross(y_axis(&drawn[0])).normalize();
        assert_near(facing, normal);
    }

    #[test]
    fn a_mark_normalises_its_normal() {
        let mut rig = Rig::new();
        let at = Vec3::new(3.0, 2.0, 1.0);
        rig.effects.mark(
            at,
            Vec3::new(0.0, 5.0, 0.0),
            0.0,
            mark_style(0.2, Duration::from_secs(10), ms(1000)),
        );

        let drawn = rig.frame(EYE);

        assert_eq!(drawn.len(), 1);
        assert_near(translation(&drawn[0]), at + Vec3::Y * MARK_OFFSET);
        assert!((x_axis(&drawn[0]).length() - 0.2).abs() < EPS);
    }

    #[test]
    fn a_mark_roll_turns_it_in_the_surface_and_deterministically() {
        let normal = Vec3::new(0.0, 0.6, 0.8);
        let style = mark_style(0.2, Duration::from_secs(10), ms(1000));
        let mut flat = Rig::new();
        flat.effects.mark(Vec3::ZERO, normal, 0.0, style);
        let mut turned = Rig::new();
        turned
            .effects
            .mark(Vec3::ZERO, normal, std::f32::consts::FRAC_PI_2, style);

        let a = flat.frame(EYE);
        let b = turned.frame(EYE);

        assert_eq!(a.len(), 1);
        assert_eq!(b.len(), 1);
        assert!(
            x_axis(&a[0])
                .normalize()
                .dot(x_axis(&b[0]).normalize())
                .abs()
                < 1e-3,
            "a quarter turn puts X where Y was"
        );
        assert_near(z_axis(&a[0]).normalize(), z_axis(&b[0]).normalize());
    }

    #[test]
    fn a_mark_holds_full_size_then_shrinks_out_in_its_fade() {
        let mut rig = Rig::new();
        rig.effects.mark(
            Vec3::ZERO,
            Vec3::Y,
            0.0,
            mark_style(0.2, Duration::from_secs(10), Duration::from_secs(1)),
        );

        rig.effects.advance(Duration::from_secs(1));
        let early = rig.frame(EYE);
        assert_eq!(early.len(), 1);
        assert!(
            (x_axis(&early[0]).length() - 0.2).abs() < EPS,
            "full at 1 s"
        );

        rig.effects.advance(Duration::from_secs(8));
        let full = rig.frame(EYE);
        assert_eq!(full.len(), 1);
        assert!((x_axis(&full[0]).length() - 0.2).abs() < EPS, "full at 9 s");

        rig.effects.advance(ms(500));
        let half = rig.frame(EYE);
        assert_eq!(half.len(), 1, "drawn without being placed");
        assert!(
            (x_axis(&half[0]).length() - 0.1).abs() < EPS,
            "half at 9.5 s"
        );
        assert!((y_axis(&half[0]).length() - 0.1).abs() < EPS);
        assert_eq!(rig.effects.live(), 1);

        rig.effects.advance(ms(500));
        assert!(rig.frame(EYE).is_empty(), "gone at 10 s");
        assert_eq!(rig.effects.live(), 0);
    }

    #[test]
    fn a_full_cap_drops_the_oldest_fixed_lifetime_effect() {
        let mut rig = Rig::new();
        rig.effects.set_quality(1);
        let style = mark_style(0.2, Duration::from_secs(10), ms(500));
        for i in 0..32 {
            rig.effects.mark(Vec3::X * i as f32, Vec3::Y, 0.0, style);
        }
        assert_eq!(rig.effects.live(), 32);

        rig.effects.mark(Vec3::X * 100.0, Vec3::Y, 0.0, style);

        assert_eq!(rig.effects.live(), 32);
        let mut xs: Vec<i32> = rig
            .frame(EYE)
            .iter()
            .map(|i| translation(i).x.round() as i32)
            .collect();
        xs.sort_unstable();
        let expected: Vec<i32> = (1..32).chain([100]).collect();
        assert_eq!(xs, expected, "mark 0 is dropped, the rest and the new stay");
    }

    #[test]
    fn a_full_cap_evicts_the_lowest_handle_whether_flash_or_mark() {
        let mut rig = Rig::new();
        rig.effects.set_quality(1);
        let long = Duration::from_secs(10);
        rig.effects
            .flash(Vec3::X * -1.0, 0.0, flash_style(0.4, long));
        for i in 0..31 {
            rig.effects.mark(
                Vec3::X * i as f32,
                Vec3::Y,
                0.0,
                mark_style(0.2, long, ms(500)),
            );
        }
        assert_eq!(rig.effects.live(), 32);

        rig.effects.mark(
            Vec3::X * 100.0,
            Vec3::Y,
            0.0,
            mark_style(0.2, long, ms(500)),
        );

        assert_eq!(rig.effects.live(), 32);
        let xs: Vec<i32> = rig
            .frame(EYE)
            .iter()
            .map(|i| translation(i).x.round() as i32)
            .collect();
        assert!(!xs.contains(&-1), "the oldest, a flash, is gone");
        assert!(xs.contains(&0) && xs.contains(&100));
        assert_eq!(xs.len(), 32);
    }

    #[test]
    fn a_full_cap_drops_a_flash_for_a_new_flash_and_keeps_the_streak() {
        let mut rig = Rig::new();
        rig.effects.set_quality(1);
        let long = Duration::from_secs(10);
        let streak = rig.effects.begin_streak(style(2.0, 0.1));
        for i in 0..31 {
            rig.effects
                .flash(Vec3::X * i as f32, 0.0, flash_style(0.4, long));
        }
        assert_eq!(rig.effects.live(), 32);

        rig.effects
            .flash(Vec3::X * 100.0, 0.0, flash_style(0.4, long));

        assert_eq!(rig.effects.live(), 32);
        rig.effects
            .place_streak(streak, Vec3::new(5.0, 50.0, 0.0), Vec3::X);
        let drawn = rig.frame(EYE);
        assert_eq!(drawn.len(), 32, "the streak is still there");
        assert!(
            drawn.iter().any(|i| (translation(i).y - 50.0).abs() < EPS),
            "the streak survived"
        );
        assert!(
            !drawn
                .iter()
                .any(|i| (translation(i) - Vec3::ZERO).length() < EPS),
            "flash 0 was evicted"
        );
    }

    #[test]
    fn a_full_cap_of_streaks_refuses_a_mark_and_keeps_every_streak() {
        let mut rig = Rig::new();
        rig.effects.set_quality(1);
        let handles: Vec<EffectHandle> = (0..32)
            .map(|_| rig.effects.begin_streak(style(2.0, 0.1)))
            .collect();
        assert_eq!(rig.effects.live(), 32);

        rig.effects.mark(
            Vec3::X * 100.0,
            Vec3::Y,
            0.0,
            mark_style(0.2, Duration::from_secs(10), ms(500)),
        );
        rig.effects
            .flash(Vec3::X * 100.0, 0.0, flash_style(0.4, ms(50)));

        assert_eq!(rig.effects.live(), 32, "refused");
        for handle in &handles {
            rig.effects.place_streak(*handle, Vec3::X, Vec3::X);
        }
        let drawn = rig.frame(EYE);
        assert_eq!(drawn.len(), 32, "every streak, and nothing else");
        assert!(
            drawn.iter().all(|i| (x_axis(i).length() - 2.0).abs() < EPS),
            "each drawn instance is a streak, two long"
        );
        rig.effects.advance(ms(100));
        assert_eq!(rig.effects.live(), 32, "streaks stay");
    }

    #[test]
    fn a_freed_streak_slot_takes_the_next_mark() {
        let mut rig = Rig::new();
        rig.effects.set_quality(1);
        let handles: Vec<EffectHandle> = (0..32)
            .map(|_| rig.effects.begin_streak(style(2.0, 0.1)))
            .collect();
        rig.effects.end(handles[0]);
        rig.effects.advance(ms(100));
        assert_eq!(rig.effects.live(), 31);
        rig.effects.mark(
            Vec3::X * 100.0,
            Vec3::Y,
            0.0,
            mark_style(0.2, Duration::from_secs(10), ms(500)),
        );
        assert_eq!(rig.effects.live(), 32, "a freed slot takes the mark");
        assert_eq!(
            rig.frame(EYE).len(),
            1,
            "only the mark draws, unplaced streaks do not"
        );
    }

    #[test]
    fn a_lowered_quality_over_the_cap_evicts_one_for_one() {
        let mut rig = Rig::new();
        let long = Duration::from_secs(10);
        for i in 0..40 {
            rig.effects.mark(
                Vec3::X * i as f32,
                Vec3::Y,
                0.0,
                mark_style(0.2, long, ms(500)),
            );
        }
        rig.effects.set_quality(1);

        rig.effects.mark(
            Vec3::X * 100.0,
            Vec3::Y,
            0.0,
            mark_style(0.2, long, ms(500)),
        );

        assert_eq!(rig.effects.live(), 40, "one out, one in");
        let xs: Vec<i32> = rig
            .frame(EYE)
            .iter()
            .map(|i| translation(i).x.round() as i32)
            .collect();
        assert!(!xs.contains(&0) && xs.contains(&1) && xs.contains(&100));
    }

    #[test]
    fn expired_marks_free_room_below_the_cap() {
        let mut rig = Rig::new();
        rig.effects.set_quality(1);
        let short = mark_style(0.2, ms(100), ms(50));
        for i in 0..32 {
            rig.effects.mark(Vec3::X * i as f32, Vec3::Y, 0.0, short);
        }
        rig.effects.advance(ms(100));
        assert_eq!(rig.effects.live(), 0);

        rig.effects.mark(Vec3::X * 7.0, Vec3::Y, 0.0, short);

        assert_eq!(rig.effects.live(), 1);
        assert_eq!(rig.frame(EYE).len(), 1);
    }

    #[test]
    fn quality_zero_refuses_flash_and_mark() {
        let mut rig = Rig::new();
        rig.effects.set_quality(0);

        rig.effects
            .flash(Vec3::X, 0.0, flash_style(0.4, Duration::from_secs(1)));
        rig.effects.mark(
            Vec3::X,
            Vec3::Y,
            0.0,
            mark_style(0.2, Duration::from_secs(10), ms(500)),
        );

        assert_eq!(rig.effects.live(), 0);
        assert!(rig.frame(EYE).is_empty());
        rig.effects.set_quality(4);
        assert!(rig.frame(EYE).is_empty(), "refused spawns are not queued");
        assert_eq!(rig.effects.live(), 0);
    }

    #[test]
    fn quality_zero_clears_live_flashes_and_marks() {
        let mut rig = Rig::new();
        rig.effects
            .flash(Vec3::X, 0.0, flash_style(0.4, Duration::from_secs(1)));
        rig.effects.mark(
            Vec3::X,
            Vec3::Y,
            0.0,
            mark_style(0.2, Duration::from_secs(10), ms(500)),
        );
        assert_eq!(rig.effects.live(), 2);

        rig.effects.set_quality(0);

        assert_eq!(rig.effects.live(), 0);
        assert!(rig.frame(EYE).is_empty());
    }

    fn spawn_script(rig: &mut Rig) -> Vec<String> {
        let mut frames = Vec::new();
        rig.effects
            .flash(Vec3::new(1.0, 2.0, 3.0), 0.3, flash_style(0.4, ms(50)));
        rig.effects
            .flash(Vec3::new(-2.0, 0.5, 6.0), -1.1, flash_style(0.7, ms(80)));
        rig.effects.mark(
            Vec3::new(-4.0, 0.0, 2.0),
            Vec3::new(0.3, 0.9, -0.1),
            1.2,
            mark_style(0.2, Duration::from_secs(10), Duration::from_secs(1)),
        );
        rig.effects.mark(
            Vec3::new(5.0, 1.0, -2.0),
            Vec3::new(-1.0, 0.2, 0.4),
            -0.6,
            mark_style(0.35, Duration::from_secs(10), ms(500)),
        );
        for dt in [ms(10), ms(15), ms(9400), ms(300)] {
            rig.effects.advance(dt);
            frames.push(format!("{:?}", rig.frame(EYE)));
        }
        frames
    }

    #[test]
    fn the_same_spawns_draw_the_same_instances() {
        let mut first = Rig::new();
        let mut second = Rig::new();

        let a = spawn_script(&mut first);
        let b = spawn_script(&mut second);

        assert_eq!(a, b);
        assert!(
            a.iter().any(|frame| frame != "[]"),
            "the script draws something"
        );
    }

    #[test]
    fn degenerate_flash_styles_never_draw_non_finite_instances() {
        let mut rig = Rig::new();
        rig.effects
            .flash(Vec3::X, 0.0, flash_style(0.4, Duration::from_secs(1)));
        let controls = 1;
        for (size, lifetime) in [
            (0.4, Duration::ZERO),
            (0.0, ms(50)),
            (-1.0, ms(50)),
            (f32::NAN, ms(50)),
            (f32::INFINITY, ms(50)),
        ] {
            rig.effects
                .flash(Vec3::Y, f32::NAN, flash_style(size, lifetime));
        }

        let drawn = rig.frame(EYE);
        rig.effects.advance(ms(60));
        let later = rig.frame(EYE);

        assert!(drawn.len() >= controls, "the well-formed flash draws");
        assert!(drawn.iter().chain(&later).all(is_finite));
        assert_eq!(rig.effects.live(), 1, "only the control outlives the tick");
    }

    #[test]
    fn a_flash_with_the_eye_on_its_view_axis_is_finite_and_faces_the_eye() {
        let at = Vec3::new(2.0, 1.0, -3.0);
        for eye in [at + Vec3::Y * 5.0, at - Vec3::Y * 5.0, at] {
            let mut rig = Rig::new();
            rig.effects.flash(at, 0.4, flash_style(0.4, ms(50)));
            rig.effects
                .flash(at + Vec3::X * 20.0, 0.0, flash_style(0.4, ms(50)));

            let drawn = rig.frame(eye);

            assert!(!drawn.is_empty(), "the control flash draws, eye {eye}");
            assert!(drawn.iter().all(is_finite), "eye {eye}");
            if eye != at {
                let facing = drawn
                    .iter()
                    .find(|i| (translation(i) - at).length() < EPS)
                    .expect("the flash straight below or above the eye still draws");
                let normal = x_axis(facing).cross(y_axis(facing)).normalize();
                assert_near(normal, (eye - at).normalize());
            }
        }
    }

    #[test]
    fn degenerate_mark_styles_never_draw_non_finite_instances() {
        let mut rig = Rig::new();
        rig.effects.mark(
            Vec3::X,
            Vec3::Y,
            0.0,
            mark_style(0.2, Duration::from_secs(10), Duration::from_secs(1)),
        );
        let cases = [
            mark_style(0.2, Duration::ZERO, Duration::ZERO),
            mark_style(0.2, Duration::from_secs(1), Duration::from_secs(3)),
            mark_style(0.2, Duration::from_secs(1), Duration::ZERO),
            mark_style(0.0, Duration::from_secs(1), ms(100)),
            mark_style(-1.0, Duration::from_secs(1), ms(100)),
            mark_style(f32::NAN, Duration::from_secs(1), ms(100)),
        ];
        for case in cases {
            rig.effects.mark(Vec3::Y, Vec3::Y, 0.0, case);
        }
        rig.effects.mark(Vec3::Z, Vec3::ZERO, f32::NAN, cases[1]);

        let drawn = rig.frame(EYE);
        rig.effects.advance(ms(500));
        let mid = rig.frame(EYE);
        rig.effects.advance(Duration::from_secs(1));
        let later = rig.frame(EYE);

        assert!(!drawn.is_empty(), "the well-formed mark draws");
        assert!(drawn.iter().chain(&mid).chain(&later).all(is_finite));
        assert_eq!(
            rig.effects.live(),
            1,
            "only the control outlives the lifetimes"
        );
    }

    #[test]
    fn a_zero_fade_mark_is_full_size_until_its_lifetime_then_gone() {
        let mut rig = Rig::new();
        rig.effects.mark(
            Vec3::ZERO,
            Vec3::Y,
            0.0,
            mark_style(0.2, Duration::from_secs(2), Duration::ZERO),
        );

        rig.effects.advance(ms(1900));
        let drawn = rig.frame(EYE);
        assert_eq!(drawn.len(), 1);
        assert!((x_axis(&drawn[0]).length() - 0.2).abs() < EPS);

        rig.effects.advance(ms(100));
        assert!(rig.frame(EYE).is_empty());
        assert_eq!(rig.effects.live(), 0);
    }

    proptest! {
        #[test]
        fn a_mark_basis_is_finite_for_any_normal(
            random in (-1.0f32..1.0, -1.0f32..1.0, -1.0f32..1.0),
            axis in 0usize..6,
            use_axis in any::<bool>(),
            roll in -10.0f32..10.0,
        ) {
            let axes = [Vec3::X, Vec3::Y, Vec3::Z, -Vec3::X, -Vec3::Y, -Vec3::Z];
            let random = Vec3::new(random.0, random.1, random.2);
            let normal = if use_axis { axes[axis] } else { random };
            prop_assume!(normal.length() > 1e-2);
            let mut rig = Rig::new();

            rig.effects.mark(
                Vec3::new(1.0, 2.0, 3.0),
                normal,
                roll,
                mark_style(0.2, Duration::from_secs(10), Duration::from_secs(1)),
            );
            let drawn = rig.frame(EYE);

            prop_assert_eq!(drawn.len(), 1);
            prop_assert!(is_finite(&drawn[0]));
            let (x, y) = (x_axis(&drawn[0]), y_axis(&drawn[0]));
            prop_assert!((x.length() - 0.2).abs() < 1e-3);
            prop_assert!((y.length() - 0.2).abs() < 1e-3);
            prop_assert!(x.dot(y).abs() < 1e-3);
            let n = normal.normalize();
            prop_assert!(x.dot(n).abs() < 1e-3, "X lies in the surface");
            prop_assert!(y.dot(n).abs() < 1e-3, "Y lies in the surface");
            prop_assert!((x.cross(y).normalize() - n).length() < 1e-3, "faces along the normal");
        }
    }
}
