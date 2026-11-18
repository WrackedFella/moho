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

/// How far a mark floats off its surface.
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

    pub fn flash(&mut self, _at: Vec3, _roll: f32, _style: FlashStyle) {}

    pub fn mark(&mut self, _at: Vec3, _normal: Vec3, _roll: f32, _style: MarkStyle) {}

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

    fn comparable(drawn: &[InstanceGpu]) -> Vec<([[f32; 4]; 4], u32)> {
        drawn.iter().map(|i| (i.model, i.material)).collect()
    }

    fn assert_finite(instance: &InstanceGpu) {
        assert!(
            instance.model.iter().flatten().all(|v| v.is_finite()),
            "non-finite model {:?}",
            instance.model
        );
    }

    #[test]
    fn the_flash_mesh_is_a_four_point_star_wound_toward_plus_z() {
        let mut rig = Rig::new();
        rig.effects.flash(Vec3::ZERO, 0.0, flash_style(0.4, ms(50)));

        let drawn = rig.frame(EYE);
        let again = rig.frame(EYE);

        assert_eq!(drawn.len(), 1);
        assert_eq!(again.len(), 1, "drawn every frame while live");
        assert_eq!(rig.backend.registered.len(), 1, "uploaded once");
        let (positions, indices) = &rig.backend.registered[0];
        assert_eq!(positions.len(), 8, "four points and four notches");
        let mut by_angle: Vec<Vec3> = positions.iter().map(|p| Vec3::from(*p)).collect();
        by_angle.sort_by(|a, b| a.y.atan2(a.x).partial_cmp(&b.y.atan2(b.x)).expect("finite"));
        let radii: Vec<f32> = by_angle.iter().map(|p| p.length()).collect();
        for pair in radii.chunks(2) {
            let (tip, notch) = if pair[0] > pair[1] {
                (pair[0], pair[1])
            } else {
                (pair[1], pair[0])
            };
            assert!((tip - 0.5).abs() < EPS, "tips at radius 0.5, got {radii:?}");
            assert!(
                notch > 0.0 && notch < 0.5 - EPS,
                "notches sit inside the tips, got {radii:?}"
            );
        }
        for around in 0..8 {
            let (a, b) = (radii[around], radii[(around + 1) % 8]);
            assert!(
                ((a - 0.5).abs() < EPS) != ((b - 0.5).abs() < EPS),
                "tips and notches alternate around the star, got {radii:?}"
            );
        }
        let centroid = by_angle.iter().sum::<Vec3>() / 8.0;
        assert_near(centroid, Vec3::ZERO);
        assert!(!indices.is_empty() && indices.len() % 3 == 0);
        for triangle in indices.chunks(3) {
            let [a, b, c] = [0, 1, 2].map(|i| Vec3::from(positions[triangle[i] as usize]));
            assert!(
                (b - a).cross(c - a).z > 0.0,
                "triangle {triangle:?} is counter-clockwise seen from +Z"
            );
        }
        assert!(positions.iter().all(|p| p[2] == 0.0), "in the XY plane");
    }

    #[test]
    fn the_mark_mesh_is_a_unit_quad_centred_on_the_origin_wound_toward_plus_z() {
        let mut rig = Rig::new();
        rig.effects
            .mark(Vec3::ZERO, Vec3::Y, 0.0, mark_style(0.2, ms(50), ms(10)));

        let drawn = rig.frame(EYE);
        let again = rig.frame(EYE);

        assert_eq!(drawn.len(), 1);
        assert_eq!(again.len(), 1, "drawn every frame while live");
        assert_eq!(rig.backend.registered.len(), 1, "uploaded once");
        let (positions, indices) = &rig.backend.registered[0];
        let mut corners = positions.clone();
        corners.sort_by(|a, b| a.partial_cmp(b).expect("finite"));
        corners.dedup();
        assert_eq!(
            corners,
            vec![
                [-0.5, -0.5, 0.0],
                [-0.5, 0.5, 0.0],
                [0.5, -0.5, 0.0],
                [0.5, 0.5, 0.0]
            ]
        );
        assert_eq!(indices.len(), 6, "a quad is two triangles");
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
        let at = Vec3::new(2.0, 1.0, -3.0);
        rig.effects.flash(
            at,
            0.0,
            FlashStyle {
                material: 5,
                ..flash_style(0.4, ms(50))
            },
        );

        let full = rig.frame(EYE);
        rig.effects.advance(ms(25));
        let half = rig.frame(EYE);
        rig.effects.advance(ms(25));
        let gone = rig.frame(EYE);

        assert_eq!(full.len(), 1, "drawn with no placement call");
        assert_near(translation(&full[0]), at);
        assert_eq!(full[0].material, 5);
        assert!((x_axis(&full[0]).length() - 0.4).abs() < EPS);
        assert!((y_axis(&full[0]).length() - 0.4).abs() < EPS);
        assert_eq!(half.len(), 1);
        assert_near(translation(&half[0]), at);
        assert!((x_axis(&half[0]).length() - 0.2).abs() < EPS);
        assert!((y_axis(&half[0]).length() - 0.2).abs() < EPS);
        assert!(gone.is_empty());
        assert_eq!(rig.effects.live(), 0);
    }

    #[test]
    fn the_star_is_as_wide_tip_to_tip_as_its_size() {
        let mut rig = Rig::new();
        rig.effects.flash(Vec3::ZERO, 0.0, flash_style(0.4, ms(50)));

        let drawn = rig.frame(EYE);
        let (positions, _) = &rig.backend.registered[0];

        let widest = positions
            .iter()
            .map(|p| (x_axis(&drawn[0]) * p[0] + y_axis(&drawn[0]) * p[1]).length())
            .fold(0.0, f32::max);
        assert!((widest * 2.0 - 0.4).abs() < EPS, "tip to tip is the size");
    }

    #[test]
    fn a_flash_faces_the_eye() {
        let mut rig = Rig::new();
        let at = Vec3::new(1.0, 2.0, 3.0);
        rig.effects
            .flash(at, 0.0, flash_style(0.4, Duration::from_secs(5)));

        let first = rig.frame(EYE);
        let other_eye = Vec3::new(-7.0, 1.0, 4.0);
        let second = rig.frame(other_eye);

        for (drawn, eye) in [(first, EYE), (second, other_eye)] {
            assert_eq!(drawn.len(), 1);
            let to_eye = (eye - at).normalize();
            let (x, y, z) = (x_axis(&drawn[0]), y_axis(&drawn[0]), z_axis(&drawn[0]));
            assert!(x.dot(to_eye).abs() < EPS, "X is across the view");
            assert!(y.dot(to_eye).abs() < EPS, "Y is across the view");
            assert!(x.dot(y).abs() < EPS);
            assert_near(z.normalize(), to_eye);
            assert!(x.cross(y).dot(to_eye) > 0.0, "X x Y faces the eye");
        }
    }

    #[test]
    fn a_flash_seen_from_straight_above_or_below_still_faces_the_eye() {
        let at = Vec3::new(1.0, 2.0, 3.0);
        for eye in [at + Vec3::Y * 5.0, at - Vec3::Y * 5.0] {
            let mut rig = Rig::new();
            rig.effects.flash(at, 0.4, flash_style(0.4, ms(50)));

            let drawn = rig.frame(eye);

            assert_eq!(drawn.len(), 1, "eye {eye}");
            assert_finite(&drawn[0]);
            let to_eye = (eye - at).normalize();
            assert_near(z_axis(&drawn[0]).normalize(), to_eye);
            assert!(x_axis(&drawn[0]).dot(to_eye).abs() < EPS);
            assert!((x_axis(&drawn[0]).length() - 0.4).abs() < EPS);
            assert!(
                x_axis(&drawn[0]).cross(y_axis(&drawn[0])).dot(to_eye) > 0.0,
                "X x Y faces the eye"
            );
        }
    }

    #[test]
    fn a_flash_roll_turns_its_basis_about_the_view_axis() {
        let at = Vec3::new(1.0, 2.0, 3.0);
        let mut unrolled = Rig::new();
        unrolled.effects.flash(at, 0.0, flash_style(0.4, ms(50)));
        let mut rolled = Rig::new();
        rolled
            .effects
            .flash(at, std::f32::consts::FRAC_PI_2, flash_style(0.4, ms(50)));

        let base = unrolled.frame(EYE);
        let turned = rolled.frame(EYE);

        assert_near(x_axis(&turned[0]), y_axis(&base[0]));
        assert_near(y_axis(&turned[0]), -x_axis(&base[0]));
        assert_near(z_axis(&turned[0]), z_axis(&base[0]));
    }

    #[test]
    fn a_mark_lies_along_its_normal_offset_by_the_mark_offset() {
        let mut rig = Rig::new();
        let at = Vec3::new(4.0, 0.0, -2.0);
        let normal = Vec3::new(0.0, 3.0, 0.0);
        rig.effects.mark(
            at,
            normal,
            0.0,
            MarkStyle {
                material: 3,
                ..mark_style(0.2, ms(500), ms(100))
            },
        );

        let drawn = rig.frame(EYE);

        assert_eq!(drawn.len(), 1);
        assert_eq!(drawn[0].material, 3);
        assert_near(translation(&drawn[0]), at + Vec3::Y * 0.01);
        assert_near(z_axis(&drawn[0]), Vec3::Y);
        assert!((x_axis(&drawn[0]).length() - 0.2).abs() < EPS);
        assert!((y_axis(&drawn[0]).length() - 0.2).abs() < EPS);
        assert!(x_axis(&drawn[0]).dot(Vec3::Y).abs() < EPS);
        assert!(y_axis(&drawn[0]).dot(Vec3::Y).abs() < EPS);
        assert!(
            x_axis(&drawn[0]).cross(y_axis(&drawn[0])).dot(Vec3::Y) > 0.0,
            "X x Y points along the normal"
        );
    }

    #[test]
    fn a_mark_roll_turns_its_basis_about_the_normal() {
        let at = Vec3::new(1.0, 1.0, 1.0);
        let normal = Vec3::new(1.0, 2.0, -2.0).normalize();
        let mut unrolled = Rig::new();
        unrolled
            .effects
            .mark(at, normal, 0.0, mark_style(0.2, ms(500), ms(100)));
        let mut rolled = Rig::new();
        rolled.effects.mark(
            at,
            normal,
            std::f32::consts::FRAC_PI_2,
            mark_style(0.2, ms(500), ms(100)),
        );

        let base = unrolled.frame(EYE);
        let turned = rolled.frame(EYE);

        assert_near(x_axis(&base[0]), normal.any_orthonormal_vector() * 0.2);
        assert_near(x_axis(&turned[0]), y_axis(&base[0]));
        assert_near(y_axis(&turned[0]), -x_axis(&base[0]));
        assert_near(z_axis(&turned[0]), normal);
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

        rig.effects.advance(Duration::from_secs(9));
        let full = rig.frame(EYE);
        rig.effects.advance(ms(500));
        let half = rig.frame(EYE);
        rig.effects.advance(ms(500));
        let gone = rig.frame(EYE);

        assert_eq!(full.len(), 1);
        assert!((x_axis(&full[0]).length() - 0.2).abs() < EPS, "full at 9 s");
        assert_eq!(half.len(), 1);
        assert!(
            (x_axis(&half[0]).length() - 0.1).abs() < EPS,
            "half at 9.5 s"
        );
        assert!((y_axis(&half[0]).length() - 0.1).abs() < EPS);
        assert!(gone.is_empty(), "gone at 10 s");
        assert_eq!(rig.effects.live(), 0);
    }

    #[test]
    fn a_mark_with_no_fade_holds_full_size_until_its_lifetime_then_vanishes() {
        let mut rig = Rig::new();
        rig.effects.mark(
            Vec3::ZERO,
            Vec3::Y,
            0.0,
            mark_style(0.2, ms(100), Duration::ZERO),
        );

        rig.effects.advance(ms(99));
        let before = rig.frame(EYE);
        rig.effects.advance(ms(1));
        let after = rig.frame(EYE);

        assert_eq!(before.len(), 1);
        assert!((x_axis(&before[0]).length() - 0.2).abs() < EPS);
        assert!(after.is_empty());
        assert_eq!(rig.effects.live(), 0);
    }

    fn mark_at_x(rig: &mut Rig, x: f32) {
        rig.effects.mark(
            Vec3::new(x, 0.0, 0.0),
            Vec3::Y,
            0.0,
            mark_style(0.2, Duration::from_secs(10), ms(100)),
        );
    }

    fn drawn_xs(drawn: &[InstanceGpu]) -> Vec<f32> {
        drawn.iter().map(|i| translation(i).x.round()).collect()
    }

    fn flash_at_x(rig: &mut Rig, x: f32) {
        rig.effects.flash(
            Vec3::new(x, 0.0, 0.0),
            0.0,
            flash_style(0.4, Duration::from_secs(10)),
        );
    }

    #[test]
    fn a_full_cap_drops_the_oldest_fixed_lifetime_effect() {
        let spawners: [(&str, fn(&mut Rig, f32)); 2] = [("mark", mark_at_x), ("flash", flash_at_x)];
        for (kind, spawn) in spawners {
            let mut rig = Rig::new();
            rig.effects.set_quality(1);
            for x in 0..32 {
                spawn(&mut rig, x as f32 * 10.0);
            }
            assert_eq!(rig.effects.live(), 32, "{kind}");

            spawn(&mut rig, 320.0);
            let drawn = rig.frame(EYE);

            assert_eq!(rig.effects.live(), 32, "{kind}");
            assert_eq!(drawn.len(), 32, "{kind}");
            let xs = drawn_xs(&drawn);
            assert!(!xs.contains(&0.0), "the oldest {kind} is dropped");
            assert!(xs.contains(&10.0), "the next oldest {kind} stays");
            assert!(xs.contains(&320.0), "the new {kind} is kept");
        }
    }

    #[test]
    fn a_full_cap_drops_an_older_flash_before_a_newer_mark() {
        let mut rig = Rig::new();
        rig.effects.set_quality(1);
        rig.effects.flash(
            Vec3::new(500.0, 0.0, 0.0),
            0.0,
            flash_style(0.4, Duration::from_secs(10)),
        );
        for x in 1..32 {
            mark_at_x(&mut rig, x as f32 * 10.0);
        }
        assert_eq!(rig.effects.live(), 32);

        mark_at_x(&mut rig, 320.0);
        let drawn = rig.frame(EYE);

        assert_eq!(rig.effects.live(), 32);
        let xs = drawn_xs(&drawn);
        assert!(!xs.contains(&500.0), "the oldest, a flash, is dropped");
        assert!(xs.contains(&10.0));
        assert!(xs.contains(&320.0));
    }

    #[test]
    fn a_full_cap_never_evicts_a_streak_while_a_fixed_lifetime_effect_exists() {
        let mut rig = Rig::new();
        rig.effects.set_quality(1);
        let streaks: Vec<EffectHandle> = (0..30)
            .map(|_| rig.effects.begin_streak(style(2.0, 0.1)))
            .collect();
        mark_at_x(&mut rig, 100.0);
        mark_at_x(&mut rig, 200.0);
        assert_eq!(rig.effects.live(), 32);

        mark_at_x(&mut rig, 300.0);
        for handle in &streaks {
            rig.effects
                .place_streak(*handle, Vec3::new(0.0, 7.0, 0.0), Vec3::X);
        }
        let drawn = rig.frame(EYE);

        assert_eq!(rig.effects.live(), 32);
        assert_eq!(drawn.len(), 32, "all 30 streaks and 2 marks");
        let xs = drawn_xs(&drawn);
        assert!(!xs.contains(&100.0), "the oldest mark is dropped");
        assert!(xs.contains(&200.0) && xs.contains(&300.0));
    }

    #[test]
    fn a_full_cap_of_streaks_refuses_a_mark_and_keeps_every_streak() {
        let mut rig = Rig::new();
        rig.effects.set_quality(1);
        let streaks: Vec<EffectHandle> = (0..32)
            .map(|_| rig.effects.begin_streak(style(2.0, 0.1)))
            .collect();

        mark_at_x(&mut rig, 100.0);
        rig.effects.flash(
            Vec3::new(200.0, 0.0, 0.0),
            0.0,
            flash_style(0.4, Duration::from_secs(10)),
        );
        for handle in &streaks {
            rig.effects
                .place_streak(*handle, Vec3::new(0.0, 7.0, 0.0), Vec3::X);
        }
        let drawn = rig.frame(EYE);

        assert_eq!(rig.effects.live(), 32);
        assert_eq!(drawn.len(), 32, "every streak stays and nothing else draws");
        assert!(drawn.iter().all(|i| translation(i).y.round() != 0.0));
    }

    #[test]
    fn a_slot_freed_by_a_finished_streak_takes_the_next_mark() {
        let mut rig = Rig::new();
        rig.effects.set_quality(1);
        let streaks: Vec<EffectHandle> = (0..32)
            .map(|_| {
                rig.effects.begin_streak(StreakStyle {
                    fade: ms(50),
                    ..style(2.0, 0.1)
                })
            })
            .collect();
        rig.effects.end(streaks[0]);
        rig.effects.advance(ms(60));
        assert_eq!(rig.effects.live(), 31, "the ended streak finished fading");

        mark_at_x(&mut rig, 100.0);

        assert_eq!(rig.effects.live(), 32, "the freed slot takes the mark");
    }

    #[test]
    fn quality_zero_refuses_flash_and_mark() {
        let mut rig = Rig::new();
        rig.effects.set_quality(0);

        rig.effects.flash(Vec3::ZERO, 0.0, flash_style(0.4, ms(50)));
        mark_at_x(&mut rig, 1.0);

        assert_eq!(rig.effects.live(), 0);
        assert!(rig.frame(EYE).is_empty());
        rig.effects.set_quality(4);
        rig.effects.flash(Vec3::ZERO, 0.0, flash_style(0.4, ms(50)));
        assert_eq!(rig.effects.live(), 1, "the same spawn works above zero");
    }

    #[test]
    fn quality_zero_clears_live_flashes_and_marks() {
        let mut rig = Rig::new();
        rig.effects
            .flash(Vec3::ZERO, 0.0, flash_style(0.4, Duration::from_secs(5)));
        mark_at_x(&mut rig, 1.0);
        assert_eq!(rig.effects.live(), 2);

        rig.effects.set_quality(0);

        assert_eq!(rig.effects.live(), 0);
        assert!(rig.frame(EYE).is_empty());
    }

    #[test]
    fn a_streak_a_flash_and_a_mark_draw_on_their_own_reserved_meshes() {
        let mut rig = Rig::new();
        let streak = rig.effects.begin_streak(style(2.0, 0.1));
        rig.effects.place_streak(streak, Vec3::X * 5.0, Vec3::X);
        rig.effects.flash(Vec3::Y, 0.0, flash_style(0.4, ms(50)));
        rig.effects
            .mark(Vec3::Z, Vec3::Y, 0.0, mark_style(0.2, ms(50), ms(10)));

        let drawn = rig.frame_with_handles(EYE);

        assert_eq!(drawn.len(), 3);
        assert_eq!(rig.backend.registered.len(), 3, "three distinct meshes");
        let vertices = |(handle, _): &(u32, InstanceGpu)| {
            rig.backend.registered[handle.checked_sub(1).expect("handles are 1-based") as usize]
                .0
                .len()
        };
        assert_eq!(drawn.iter().map(vertices).collect::<Vec<_>>(), [4, 8, 4]);
        assert_eq!(rig.meshes.live_len(), 3);
        for offset in 0..3 {
            let before = rig.meshes.live_len();
            rig.meshes
                .remove(WorldMeshId(EFFECT_MESH_IDS_FROM + offset));
            assert_eq!(
                rig.meshes.live_len(),
                before - 1,
                "mesh id EFFECT_MESH_IDS_FROM + {offset} is live"
            );
        }
    }

    #[test]
    fn invalid_flash_and_mark_inputs_spawn_nothing() {
        let good_flash = flash_style(0.4, ms(50));
        let good_mark = mark_style(0.2, ms(50), ms(10));
        let nan = f32::NAN;
        let bad_flashes: [(&str, Vec3, f32, FlashStyle); 8] = [
            ("zero size", Vec3::ZERO, 0.0, flash_style(0.0, ms(50))),
            ("negative size", Vec3::ZERO, 0.0, flash_style(-0.4, ms(50))),
            ("NaN size", Vec3::ZERO, 0.0, flash_style(nan, ms(50))),
            (
                "infinite size",
                Vec3::ZERO,
                0.0,
                flash_style(f32::INFINITY, ms(50)),
            ),
            (
                "zero lifetime",
                Vec3::ZERO,
                0.0,
                flash_style(0.4, Duration::ZERO),
            ),
            ("NaN position", Vec3::new(nan, 0.0, 0.0), 0.0, good_flash),
            (
                "infinite position",
                Vec3::new(0.0, f32::INFINITY, 0.0),
                0.0,
                good_flash,
            ),
            ("NaN roll", Vec3::ZERO, nan, good_flash),
        ];
        for (name, at, roll, flash_style) in bad_flashes {
            let mut rig = Rig::new();
            rig.effects.flash(at, roll, flash_style);
            assert_eq!(rig.effects.live(), 0, "flash with {name}");
            assert!(rig.frame(EYE).is_empty(), "flash with {name}");
            rig.effects.flash(Vec3::ZERO, 0.0, good_flash);
            assert_eq!(rig.effects.live(), 1, "a valid flash spawns after {name}");
        }

        let bad_marks: [(&str, Vec3, Vec3, f32, MarkStyle); 12] = [
            (
                "zero size",
                Vec3::ZERO,
                Vec3::Y,
                0.0,
                mark_style(0.0, ms(50), ms(10)),
            ),
            (
                "negative size",
                Vec3::ZERO,
                Vec3::Y,
                0.0,
                mark_style(-0.2, ms(50), ms(10)),
            ),
            (
                "NaN size",
                Vec3::ZERO,
                Vec3::Y,
                0.0,
                mark_style(nan, ms(50), ms(10)),
            ),
            (
                "infinite size",
                Vec3::ZERO,
                Vec3::Y,
                0.0,
                mark_style(f32::INFINITY, ms(50), ms(10)),
            ),
            (
                "zero lifetime",
                Vec3::ZERO,
                Vec3::Y,
                0.0,
                mark_style(0.2, Duration::ZERO, Duration::ZERO),
            ),
            (
                "NaN position",
                Vec3::new(nan, 0.0, 0.0),
                Vec3::Y,
                0.0,
                good_mark,
            ),
            (
                "infinite position",
                Vec3::new(0.0, f32::INFINITY, 0.0),
                Vec3::Y,
                0.0,
                good_mark,
            ),
            ("NaN roll", Vec3::ZERO, Vec3::Y, nan, good_mark),
            ("zero normal", Vec3::ZERO, Vec3::ZERO, 0.0, good_mark),
            (
                "NaN normal",
                Vec3::ZERO,
                Vec3::new(nan, 1.0, 0.0),
                0.0,
                good_mark,
            ),
            (
                "infinite normal",
                Vec3::ZERO,
                Vec3::new(f32::INFINITY, 0.0, 0.0),
                0.0,
                good_mark,
            ),
            (
                "infinite roll",
                Vec3::ZERO,
                Vec3::Y,
                f32::INFINITY,
                good_mark,
            ),
        ];
        for (name, at, normal, roll, mark_style) in bad_marks {
            let mut rig = Rig::new();
            rig.effects.mark(at, normal, roll, mark_style);
            assert_eq!(rig.effects.live(), 0, "mark with {name}");
            assert!(rig.frame(EYE).is_empty(), "mark with {name}");
            rig.effects.mark(Vec3::ZERO, Vec3::Y, 0.0, good_mark);
            assert_eq!(rig.effects.live(), 1, "a valid mark spawns after {name}");
        }
    }

    #[test]
    fn the_same_spawns_draw_the_same_instances() {
        let run = || {
            let mut rig = Rig::new();
            rig.effects
                .flash(Vec3::new(1.0, 2.0, 3.0), 0.7, flash_style(0.4, ms(50)));
            rig.effects.mark(
                Vec3::new(-4.0, 0.5, 2.0),
                Vec3::new(0.3, 1.0, -0.2),
                1.9,
                mark_style(0.2, ms(500), ms(100)),
            );
            rig.effects.advance(ms(20));
            comparable(&rig.frame(EYE))
        };

        let first = run();
        let second = run();

        assert_eq!(first.len(), 2);
        assert_eq!(first, second);
    }

    #[test]
    fn a_mark_basis_is_finite_for_axis_aligned_normals() {
        for normal in [
            Vec3::X,
            Vec3::Y,
            Vec3::Z,
            Vec3::NEG_X,
            Vec3::NEG_Y,
            Vec3::NEG_Z,
        ] {
            let mut rig = Rig::new();
            rig.effects
                .mark(Vec3::ZERO, normal, 0.3, mark_style(0.2, ms(500), ms(100)));

            let drawn = rig.frame(EYE);

            assert_eq!(drawn.len(), 1, "normal {normal}");
            assert_finite(&drawn[0]);
            assert_near(z_axis(&drawn[0]), normal);
        }
    }

    proptest! {
        #[test]
        fn a_mark_basis_is_finite_for_any_normal(
            normal in (-1.0f32..1.0, -1.0f32..1.0, -1.0f32..1.0),
            roll in -7.0f32..7.0,
        ) {
            let normal = Vec3::new(normal.0, normal.1, normal.2);
            prop_assume!(normal.length() > 1e-2);
            let mut rig = Rig::new();

            rig.effects
                .mark(Vec3::ZERO, normal, roll, mark_style(0.2, ms(500), ms(100)));
            let drawn = rig.frame(EYE);

            prop_assert_eq!(drawn.len(), 1);
            prop_assert!(drawn[0].model.iter().flatten().all(|v| v.is_finite()));
            prop_assert!((z_axis(&drawn[0]) - normal.normalize()).length() < 1e-3);
            let n = normal.normalize();
            let (x, y) = (x_axis(&drawn[0]), y_axis(&drawn[0]));
            prop_assert!((x.length() - 0.2).abs() < 1e-3);
            prop_assert!((y.length() - 0.2).abs() < 1e-3);
            prop_assert!(x.dot(n).abs() < 1e-3);
            prop_assert!(y.dot(n).abs() < 1e-3);
            prop_assert!(x.dot(y).abs() < 1e-3);
            prop_assert!(x.cross(y).dot(n) > 0.0);
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
