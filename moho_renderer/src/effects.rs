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

/// How far a mark floats off its surface along the normal, so a flat quad on a flat
/// surface does not z-fight.
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
struct Flash {
    at: Vec3,
    roll: f32,
    style: FlashStyle,
    age: Duration,
}

#[derive(Debug)]
struct Mark {
    at: Vec3,
    /// Unit normal; `None` for a zero or non-finite one, which draws nothing.
    normal: Option<Vec3>,
    roll: f32,
    style: MarkStyle,
    age: Duration,
}

impl Mark {
    /// The size factor: 1 until the last `fade` of the lifetime, then falling linearly to 0.
    fn size_scale(&self) -> f32 {
        let fade_from = self.style.lifetime.saturating_sub(self.style.fade);
        if self.age <= fade_from || self.style.fade.is_zero() {
            return 1.0;
        }
        let left = self.style.lifetime.saturating_sub(self.age);
        (left.as_secs_f32() / self.style.fade.as_secs_f32()).clamp(0.0, 1.0)
    }
}

#[derive(Debug)]
enum Entry {
    Streak(Streak),
    Flash(Flash),
    Mark(Mark),
}

#[derive(Debug)]
pub struct Effects {
    entries: BTreeMap<EffectHandle, Entry>,
    next_handle: u64,
    cap: usize,
    streak_uploaded: bool,
    flash_uploaded: bool,
    mark_uploaded: bool,
}

impl Default for Effects {
    fn default() -> Self {
        Self {
            entries: BTreeMap::new(),
            next_handle: 0,
            cap: CAPS[DEFAULT_QUALITY],
            streak_uploaded: false,
            flash_uploaded: false,
            mark_uploaded: false,
        }
    }
}

impl Effects {
    /// Start a streak. At the quality's cap the handle is inert: never stored, so placing
    /// or ending it does nothing.
    pub fn begin_streak(&mut self, style: StreakStyle) -> EffectHandle {
        let handle = EffectHandle(self.next_handle);
        self.next_handle += 1;
        if self.entries.len() < self.cap {
            self.entries.insert(
                handle,
                Entry::Streak(Streak {
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

    /// Spawn a camera-facing flash at `at`, rolled by `roll` radians. It shrinks to
    /// nothing over its lifetime. At the cap the oldest flash or mark makes room; with
    /// none to drop, the spawn is refused.
    pub fn flash(&mut self, at: Vec3, roll: f32, style: FlashStyle) {
        self.spawn_timed(Entry::Flash(Flash {
            at,
            roll,
            style,
            age: Duration::ZERO,
        }));
    }

    /// Spawn a mark at `at` lying along `normal`, rolled by `roll` radians. It holds its
    /// size until the last `fade` of its lifetime, then shrinks out. A zero or non-finite
    /// `normal` draws nothing. Capped like [`Effects::flash`].
    pub fn mark(&mut self, at: Vec3, normal: Vec3, roll: f32, style: MarkStyle) {
        self.spawn_timed(Entry::Mark(Mark {
            at,
            normal: normal.try_normalize(),
            roll,
            style,
            age: Duration::ZERO,
        }));
    }

    fn spawn_timed(&mut self, entry: Entry) {
        let handle = EffectHandle(self.next_handle);
        self.next_handle += 1;
        if self.entries.len() >= self.cap {
            let oldest = self
                .entries
                .iter()
                .find(|(_, e)| !matches!(e, Entry::Streak(_)))
                .map(|(h, _)| *h);
            let Some(oldest) = oldest else {
                return;
            };
            self.entries.remove(&oldest);
        }
        self.entries.insert(handle, entry);
    }

    /// Place the streak's head for this frame. A zero-length heading keeps the last one;
    /// a stale or unknown handle is ignored.
    pub fn place_streak(&mut self, handle: EffectHandle, head: Vec3, heading: Vec3) {
        let Some(Entry::Streak(streak)) = self.entries.get_mut(&handle) else {
            return;
        };
        streak.head = head;
        streak.heading = heading.try_normalize().or(streak.heading);
        streak.placed_this_frame = true;
    }

    /// Start the fade; the streak then draws at its last placement until it is gone.
    pub fn end(&mut self, handle: EffectHandle) {
        if let Some(Entry::Streak(streak)) = self.entries.get_mut(&handle) {
            streak.ended_at.get_or_insert(streak.age);
        }
    }

    /// Age every effect by the frame time, start the fade of any streak past its
    /// `max_age` and drop the finished ones.
    pub fn advance(&mut self, dt: Duration) {
        for entry in self.entries.values_mut() {
            match entry {
                Entry::Streak(streak) => {
                    streak.age += dt;
                    if streak.age >= streak.style.max_age {
                        streak.ended_at.get_or_insert(streak.style.max_age);
                    }
                }
                Entry::Flash(flash) => flash.age += dt,
                Entry::Mark(mark) => mark.age += dt,
            }
        }
        self.entries.retain(|_, entry| match entry {
            Entry::Streak(streak) => !streak.finished_fading(),
            Entry::Flash(flash) => flash.age < flash.style.lifetime,
            Entry::Mark(mark) => mark.age < mark.style.lifetime,
        });
    }

    /// Cap new effects for `quality` (0 off to 4 ultra; higher clamps). Lowering keeps
    /// what is live; 0 clears it.
    pub fn set_quality(&mut self, quality: u32) {
        self.cap = CAPS
            [usize::try_from(quality.min(MAX_QUALITY)).expect("a quality of at most 4 fits usize")];
        if self.cap == 0 {
            self.entries.clear();
        }
    }

    /// How many effects are live, fading ones included.
    pub fn live(&self) -> usize {
        self.entries.len()
    }

    /// Place every streak placed this frame or fading, and every flash and mark,
    /// unshadowed, facing `eye` where they face it.
    pub(crate) fn draw(&mut self, meshes: &mut WorldMeshes, eye: Vec3) {
        for entry in self.entries.values_mut() {
            let (id, uploaded, build, transform, material) = match entry {
                Entry::Streak(streak) => {
                    let placed = std::mem::take(&mut streak.placed_this_frame);
                    if !placed && streak.ended_at.is_none() {
                        continue;
                    }
                    let Some(heading) = streak.heading else {
                        continue;
                    };
                    let width = streak.style.width * streak.width_scale();
                    (
                        EFFECT_MESH_IDS_FROM,
                        &mut self.streak_uploaded,
                        streak_mesh as fn() -> WorldMesh,
                        streak_transform(streak.head, heading, streak.style.length, width, eye),
                        streak.style.material,
                    )
                }
                Entry::Flash(flash) => (
                    EFFECT_MESH_IDS_FROM + 1,
                    &mut self.flash_uploaded,
                    flash_mesh as fn() -> WorldMesh,
                    flash_transform(flash, eye),
                    flash.style.material,
                ),
                Entry::Mark(mark) => (
                    EFFECT_MESH_IDS_FROM + 2,
                    &mut self.mark_uploaded,
                    mark_mesh as fn() -> WorldMesh,
                    mark_transform(mark),
                    mark.style.material,
                ),
            };
            let Some(transform) = transform else {
                continue;
            };
            if !std::mem::replace(uploaded, true) {
                meshes.upsert_movable(WorldMeshId(id), build());
            }
            meshes.place_unshadowed(WorldMeshId(id), transform, material);
        }
    }
}

/// A four-point star in the XY plane, tips (radius 1) on the axes and inner points between
/// them, counter-clockwise seen from +Z.
fn flash_mesh() -> WorldMesh {
    const INNER: f32 = 0.3;
    let positions: Vec<[f32; 3]> = (0..8u8)
        .map(|i| {
            let angle = f32::from(i) * std::f32::consts::FRAC_PI_4;
            let radius = if i % 2 == 0 { 1.0 } else { INNER };
            [radius * angle.cos(), radius * angle.sin(), 0.0]
        })
        .collect();
    let mut indices = Vec::new();
    for tip in [0u32, 2, 4, 6] {
        indices.extend([(tip + 7) % 8, tip, tip + 1]);
    }
    indices.extend([1, 3, 5, 1, 5, 7]);
    flat_mesh(positions, indices)
}

/// A centred unit square in the XY plane, counter-clockwise seen from +Z.
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

/// The flash facing `eye`, rolled counter-clockwise about its facing axis and shrunk by age.
fn flash_transform(flash: &Flash, eye: Vec3) -> Option<Affine3A> {
    let lifetime = flash.style.lifetime.as_secs_f32();
    if lifetime <= 0.0 || !flash.at.is_finite() || !eye.is_finite() {
        return None;
    }
    let size = flash.style.size * (1.0 - flash.age.as_secs_f32() / lifetime).clamp(0.0, 1.0);
    if !size.is_finite() || size <= 0.0 {
        return None;
    }
    let facing = (eye - flash.at).try_normalize()?;
    Some(rolled_basis(facing, flash.roll, size, flash.at))
}

/// The mark lying along its normal, lifted off the surface and shrunk through its fade.
fn mark_transform(mark: &Mark) -> Option<Affine3A> {
    let normal = mark.normal?;
    let size = mark.style.size * mark.size_scale();
    if !size.is_finite() || size <= 0.0 || !mark.at.is_finite() {
        return None;
    }
    Some(rolled_basis(
        normal,
        mark.roll,
        size,
        mark.at + normal * MARK_OFFSET,
    ))
}

/// A right-handed basis with Z along `z`, X and Y scaled by `size` and turned `roll`
/// radians counter-clockwise about Z.
fn rolled_basis(z: Vec3, roll: f32, size: f32, origin: Vec3) -> Affine3A {
    let x0 = z.any_orthonormal_vector();
    let y0 = z.cross(x0);
    let (sin, cos) = roll.sin_cos();
    let x = x0 * cos + y0 * sin;
    let y = y0 * cos - x0 * sin;
    Affine3A::from_cols(
        (x * size).into(),
        (y * size).into(),
        z.into(),
        origin.into(),
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

    #[test]
    fn a_flash_shrinks_linearly_and_expires() {
        let mut rig = Rig::new();
        let at = Vec3::new(2.0, 1.0, -3.0);
        rig.effects.flash(at, 0.0, flash_style(0.4, ms(50)));

        let full = rig.frame(EYE);

        assert_eq!(full.len(), 1);
        assert_near(translation(&full[0]), at);
        assert_eq!(full[0].material, 3);
        assert!((x_axis(&full[0]).length() - 0.4).abs() < EPS);
        assert!((y_axis(&full[0]).length() - 0.4).abs() < EPS);
        assert_eq!(rig.effects.live(), 1);

        rig.effects.advance(ms(25));
        let half = rig.frame(EYE);

        assert_eq!(half.len(), 1, "a flash draws without being placed");
        assert_near(translation(&half[0]), at);
        assert!((x_axis(&half[0]).length() - 0.2).abs() < EPS);
        assert!((y_axis(&half[0]).length() - 0.2).abs() < EPS);

        rig.effects.advance(ms(25));

        assert!(rig.frame(EYE).is_empty());
        assert_eq!(rig.effects.live(), 0);
    }

    #[test]
    fn a_flash_draws_the_star_mesh_registered_once() {
        let mut rig = Rig::new();
        rig.effects.flash(Vec3::X, 0.0, flash_style(0.4, ms(50)));
        rig.frame(EYE);
        rig.effects.flash(Vec3::Y, 0.0, flash_style(0.4, ms(50)));
        rig.frame(EYE);

        assert_eq!(rig.backend.registered.len(), 1, "spawning never re-uploads");
        let (positions, indices) = &rig.backend.registered[0];
        assert_eq!(positions.len(), 8, "a four-point star");
        assert!(positions.iter().all(|p| p[2] == 0.0), "in the XY plane");
        assert!(!indices.is_empty(), "the star has triangles");
        assert_eq!(indices.len() % 3, 0);
        let used: std::collections::BTreeSet<u32> = indices.iter().copied().collect();
        assert_eq!(used, (0..8).collect(), "every vertex is used");
        let radius = |i: usize| Vec3::from(positions[i]).length();
        let (even, odd) = (radius(0), radius(1));
        assert!((even - odd).abs() > 0.05, "tips and inner points differ");
        for i in 0..8 {
            let expected = if i % 2 == 0 { even } else { odd };
            assert!((radius(i) - expected).abs() < EPS, "vertices alternate");
        }
        for triangle in indices.chunks(3) {
            let [a, b, c] = [0, 1, 2].map(|i| Vec3::from(positions[triangle[i] as usize]));
            assert!((b - a).cross(c - a).z > 0.0, "counter-clockwise from +Z");
        }
    }

    #[test]
    fn a_flash_faces_the_eye() {
        let at = Vec3::new(2.0, 1.0, -3.0);
        let eyes = [EYE, Vec3::new(-20.0, 4.0, 7.0), at + Vec3::Y * 10.0];
        for eye in eyes {
            for roll in [0.0, 1.0] {
                let mut rig = Rig::new();
                rig.effects.flash(at, roll, flash_style(0.4, ms(50)));

                let drawn = rig.frame(eye);

                assert_eq!(drawn.len(), 1);
                let (x, y) = (x_axis(&drawn[0]), y_axis(&drawn[0]));
                assert!(x.cross(y).dot(eye - at) > 0.0, "X x Y points away");
                assert!(x.dot(y).abs() < EPS, "a square basis");
                assert!((x.length() - y.length()).abs() < EPS);
                assert_near(z_axis(&drawn[0]), x.cross(y).normalize());
                assert_near(x.cross(y).normalize(), (eye - at).normalize());
            }
        }
    }

    #[test]
    fn a_flash_faces_the_eye_of_the_frame_it_is_drawn_in() {
        let at = Vec3::new(2.0, 1.0, -3.0);
        let mut rig = Rig::new();
        rig.effects.flash(at, 0.4, flash_style(0.4, ms(500)));
        let other_eye = Vec3::new(-20.0, 4.0, 7.0);

        let first = rig.frame(EYE);
        let second = rig.frame(other_eye);

        assert_eq!(first.len(), 1);
        assert_eq!(second.len(), 1);
        for (drawn, eye) in [(&first[0], EYE), (&second[0], other_eye)] {
            let normal = x_axis(drawn).cross(y_axis(drawn)).normalize();
            assert_near(normal, (eye - at).normalize());
        }
    }

    #[test]
    fn a_flash_roll_turns_the_star_in_its_plane() {
        let at = Vec3::new(2.0, 1.0, -3.0);
        let mut flat_rig = Rig::new();
        flat_rig.effects.flash(at, 0.0, flash_style(1.0, ms(50)));
        let mut turned_rig = Rig::new();
        turned_rig
            .effects
            .flash(at, std::f32::consts::FRAC_PI_2, flash_style(1.0, ms(50)));

        let (flat, turned) = (flat_rig.frame(EYE), turned_rig.frame(EYE));

        assert_eq!(flat.len(), 1);
        assert_eq!(turned.len(), 1);
        assert!(
            x_axis(&flat[0]).dot(x_axis(&turned[0])).abs() < EPS,
            "a quarter turn is perpendicular"
        );
        assert_near(
            x_axis(&flat[0]).cross(x_axis(&turned[0])).normalize(),
            z_axis(&flat[0]),
        );
    }

    #[test]
    fn a_mark_lies_along_its_normal_offset_by_the_mark_offset() {
        let at = Vec3::new(4.0, 2.0, 1.0);
        let normal = Vec3::new(0.0, 0.6, 0.8);
        let mut rig = Rig::new();
        rig.effects.mark(
            at,
            normal,
            0.7,
            mark_style(0.2, Duration::from_secs(10), ms(500)),
        );

        let drawn = rig.frame(EYE);

        assert_eq!(drawn.len(), 1);
        let d = &drawn[0];
        assert_eq!(d.material, 5);
        const { assert!(MARK_OFFSET > 0.0) };
        assert!((translation(d) - at).dot(normal) > 0.0, "lifted off");
        assert_near(translation(d), at + normal * MARK_OFFSET);
        assert_near(z_axis(d), normal);
        assert!((x_axis(d).length() - 0.2).abs() < EPS, "0.2 across");
        assert!((y_axis(d).length() - 0.2).abs() < EPS, "0.2 across");
        assert!(x_axis(d).dot(normal).abs() < EPS, "flat on the surface");
        assert!(y_axis(d).dot(normal).abs() < EPS, "flat on the surface");
        assert!(x_axis(d).dot(y_axis(d)).abs() < EPS);
        assert_near(x_axis(d).cross(y_axis(d)).normalize(), normal);
    }

    #[test]
    fn a_mark_roll_turns_it_about_its_normal() {
        let normal = Vec3::new(0.0, 0.6, 0.8);
        let style = mark_style(1.0, Duration::from_secs(10), ms(500));
        let mut flat = Rig::new();
        flat.effects.mark(Vec3::ZERO, normal, 0.0, style);
        let mut turned = Rig::new();
        turned
            .effects
            .mark(Vec3::ZERO, normal, std::f32::consts::FRAC_PI_2, style);

        let (flat, turned) = (flat.frame(EYE), turned.frame(EYE));

        assert_eq!(flat.len(), 1);
        assert_eq!(turned.len(), 1);
        assert_near(x_axis(&turned[0]), y_axis(&flat[0]));
        assert_near(y_axis(&turned[0]), -x_axis(&flat[0]));
    }

    #[test]
    fn a_mark_draws_the_unit_quad_mesh_registered_once() {
        let mut rig = Rig::new();
        let style = mark_style(0.2, Duration::from_secs(10), ms(500));
        rig.effects.mark(Vec3::X, Vec3::Y, 0.0, style);
        rig.frame(EYE);
        rig.effects.mark(Vec3::Z, Vec3::Y, 0.0, style);
        rig.frame(EYE);

        assert_eq!(rig.backend.registered.len(), 1, "spawning never re-uploads");
        let (positions, indices) = &rig.backend.registered[0];
        assert_eq!(positions.len(), 4);
        let mut corners: Vec<[f32; 3]> = positions.clone();
        corners.sort_by(|a, b| a.partial_cmp(b).expect("finite"));
        assert_eq!(
            corners,
            vec![
                [-0.5, -0.5, 0.0],
                [-0.5, 0.5, 0.0],
                [0.5, -0.5, 0.0],
                [0.5, 0.5, 0.0]
            ],
            "a centred unit square in the XY plane"
        );
        assert_eq!(indices.len(), 6);
        for triangle in indices.chunks(3) {
            let [a, b, c] = [0, 1, 2].map(|i| Vec3::from(positions[triangle[i] as usize]));
            assert!((b - a).cross(c - a).z > 0.0, "counter-clockwise from +Z");
        }
    }

    #[test]
    fn a_mark_holds_full_size_then_shrinks_out_in_its_fade() {
        let mut rig = Rig::new();
        let at = Vec3::new(4.0, 2.0, 1.0);
        rig.effects.mark(
            at,
            Vec3::Y,
            0.0,
            mark_style(0.2, Duration::from_secs(10), Duration::from_secs(1)),
        );

        rig.effects.advance(Duration::from_secs(9));
        let full = rig.frame(EYE);

        assert_eq!(full.len(), 1);
        assert!((x_axis(&full[0]).length() - 0.2).abs() < EPS, "full size");
        assert!((y_axis(&full[0]).length() - 0.2).abs() < EPS);

        rig.effects.advance(ms(500));
        let half = rig.frame(EYE);

        assert_eq!(half.len(), 1);
        assert_near(translation(&half[0]), translation(&full[0]));
        assert!((x_axis(&half[0]).length() - 0.1).abs() < EPS, "half size");
        assert!((y_axis(&half[0]).length() - 0.1).abs() < EPS);

        rig.effects.advance(ms(500));

        assert!(rig.frame(EYE).is_empty());
        assert_eq!(rig.effects.live(), 0);
    }

    #[test]
    fn a_full_cap_drops_the_oldest_fixed_lifetime_effect() {
        let mut rig = Rig::new();
        rig.effects.set_quality(1);
        let style = mark_style(0.2, Duration::from_secs(10), ms(500));
        for i in 0..32u8 {
            rig.effects
                .mark(Vec3::new(f32::from(i), 0.0, 0.0), Vec3::Z, 0.0, style);
        }
        assert_eq!(rig.effects.live(), 32);

        rig.effects
            .mark(Vec3::new(100.0, 0.0, 0.0), Vec3::Z, 0.0, style);

        assert_eq!(rig.effects.live(), 32);
        let mut xs: Vec<f32> = rig.frame(EYE).iter().map(|i| translation(i).x).collect();
        xs.sort_by(|a, b| a.partial_cmp(b).expect("finite"));
        assert_eq!(xs.len(), 32);
        assert!(xs[0] > 0.5, "the oldest, at x = 0, no longer draws");
        assert!((xs[0] - 1.0).abs() < EPS, "the next oldest stays");
        assert!((xs[31] - 100.0).abs() < EPS, "the new one draws");
    }

    #[test]
    fn a_full_cap_drops_a_flash_or_mark_for_a_new_flash() {
        let mut rig = Rig::new();
        rig.effects.set_quality(1);
        rig.effects
            .flash(Vec3::new(-7.0, 0.0, 0.0), 0.0, flash_style(0.4, ms(500)));
        let style = mark_style(0.2, Duration::from_secs(10), ms(500));
        for i in 1..32u8 {
            rig.effects
                .mark(Vec3::new(f32::from(i), 0.0, 0.0), Vec3::Z, 0.0, style);
        }

        rig.effects
            .flash(Vec3::new(50.0, 0.0, 0.0), 0.0, flash_style(0.4, ms(500)));

        assert_eq!(rig.effects.live(), 32);
        let xs: Vec<f32> = rig.frame(EYE).iter().map(|i| translation(i).x).collect();
        assert!(!xs.iter().any(|x| (x + 7.0).abs() < EPS), "oldest dropped");
        assert!(xs.iter().any(|x| (x - 50.0).abs() < EPS), "new one drawn");
    }

    #[test]
    fn a_full_cap_of_streaks_refuses_a_mark_and_keeps_every_streak() {
        let mut rig = Rig::new();
        rig.effects.set_quality(1);
        let handles: Vec<EffectHandle> = (0..32)
            .map(|_| rig.effects.begin_streak(style(2.0, 0.1)))
            .collect();

        rig.effects.mark(
            Vec3::new(100.0, 0.0, 0.0),
            Vec3::Z,
            0.0,
            mark_style(0.2, Duration::from_secs(10), ms(500)),
        );
        rig.effects
            .flash(Vec3::new(100.0, 0.0, 0.0), 0.0, flash_style(0.4, ms(50)));

        assert_eq!(rig.effects.live(), 32);
        for handle in &handles {
            rig.effects.place_streak(*handle, Vec3::X * 5.0, Vec3::X);
        }
        let drawn = rig.frame(EYE);
        assert_eq!(drawn.len(), 32, "every streak draws, and no mark");
        assert!(
            drawn.iter().all(|i| (translation(i).x - 3.0).abs() < EPS),
            "only streaks (tail at x = 3) are drawn"
        );
    }

    #[test]
    fn quality_zero_refuses_flash_and_mark() {
        let mut rig = Rig::new();
        rig.effects.set_quality(0);

        rig.effects.flash(Vec3::X, 0.0, flash_style(0.4, ms(50)));
        rig.effects.mark(
            Vec3::Y,
            Vec3::Z,
            0.0,
            mark_style(0.2, Duration::from_secs(10), ms(500)),
        );

        assert_eq!(rig.effects.live(), 0);
        assert!(rig.frame(EYE).is_empty());
        rig.effects.set_quality(4);
        assert_eq!(rig.effects.live(), 0, "nothing was stored");
        assert!(rig.frame(EYE).is_empty());

        rig.effects.flash(Vec3::X, 0.0, flash_style(0.4, ms(50)));
        rig.effects.mark(
            Vec3::Y,
            Vec3::Z,
            0.0,
            mark_style(0.2, Duration::from_secs(10), ms(500)),
        );
        assert_eq!(rig.frame(EYE).len(), 2, "at quality 4 both draw");
    }

    #[test]
    fn the_same_spawns_draw_the_same_instances() {
        let run = || {
            let mut rig = Rig::new();
            rig.effects
                .flash(Vec3::new(1.0, 2.0, 3.0), 0.3, flash_style(0.4, ms(50)));
            rig.effects.mark(
                Vec3::new(4.0, 5.0, 6.0),
                Vec3::new(0.0, 1.0, 1.0),
                1.1,
                mark_style(0.2, Duration::from_secs(10), ms(500)),
            );
            let mut frames = vec![rig.frame(EYE)];
            rig.effects.advance(ms(20));
            frames.push(rig.frame(EYE));
            frames
                .into_iter()
                .map(|f| f.iter().map(|i| (i.model, i.material)).collect::<Vec<_>>())
                .collect::<Vec<_>>()
        };

        let (first, second) = (run(), run());

        assert_eq!(first[0].len(), 2, "both effects draw");
        assert_eq!(first, second);
    }

    fn normals() -> impl Strategy<Value = Vec3> {
        prop_oneof![
            Just(Vec3::X),
            Just(Vec3::Y),
            Just(Vec3::Z),
            Just(-Vec3::X),
            Just(-Vec3::Y),
            Just(-Vec3::Z),
            Just(Vec3::new(0.0, 0.0, 3.0)),
            (-1.0f32..1.0, -1.0f32..1.0, -1.0f32..1.0).prop_map(|(x, y, z)| Vec3::new(x, y, z)),
        ]
    }

    proptest! {
        #[test]
        fn a_mark_basis_is_finite_for_any_normal(
            normal in normals(),
            roll in -7.0f32..7.0,
            size in 0.05f32..2.0,
        ) {
            prop_assume!(normal.length() > 1e-2);
            let mut rig = Rig::new();
            rig.effects.mark(
                Vec3::new(1.0, 2.0, 3.0),
                normal,
                roll,
                mark_style(size, Duration::from_secs(10), ms(500)),
            );

            let drawn = rig.frame(EYE);

            prop_assert_eq!(drawn.len(), 1);
            let d = &drawn[0];
            prop_assert!(d.model.iter().flatten().all(|v| v.is_finite()));
            let unit = normal.normalize();
            let (x, y, z) = (x_axis(d) / size, y_axis(d) / size, z_axis(d));
            prop_assert!((x.length() - 1.0).abs() < 1e-3);
            prop_assert!((y.length() - 1.0).abs() < 1e-3);
            prop_assert!((z.length() - 1.0).abs() < 1e-3);
            prop_assert!(x.dot(y).abs() < 1e-3);
            prop_assert!(x.dot(z).abs() < 1e-3);
            prop_assert!(y.dot(z).abs() < 1e-3);
            prop_assert!((z - unit).length() < 1e-3, "Z is the normal");
            prop_assert!(x.cross(y).dot(z) > 0.999, "right-handed, X x Y along Z");
        }
    }

    /// One frame's unshadowed draws as `(backend mesh handle, instance)`.
    fn frame_by_mesh(rig: &mut Rig, eye: Vec3) -> Vec<(u32, InstanceGpu)> {
        rig.effects.draw(&mut rig.meshes, eye);
        rig.meshes.flush(&mut rig.backend);
        let drawn = rig.meshes.unshadowed_draws().collect();
        rig.meshes.clear_placements();
        drawn
    }

    #[test]
    fn a_mixed_cap_drops_the_oldest_mark_and_keeps_the_streak() {
        let mut rig = Rig::new();
        rig.effects.set_quality(1);
        let streak = rig.effects.begin_streak(style(2.0, 0.1));
        let marks = mark_style(0.2, Duration::from_secs(10), ms(500));
        for i in 1..32u8 {
            rig.effects
                .mark(Vec3::new(f32::from(i), 0.0, 0.0), Vec3::Z, 0.0, marks);
        }
        assert_eq!(rig.effects.live(), 32);

        rig.effects
            .mark(Vec3::new(100.0, 0.0, 0.0), Vec3::Z, 0.0, marks);

        assert_eq!(rig.effects.live(), 32);
        rig.effects
            .place_streak(streak, Vec3::new(500.0, 0.0, 0.0), Vec3::X);
        let xs: Vec<f32> = rig.frame(EYE).iter().map(|i| translation(i).x).collect();
        assert_eq!(xs.len(), 32);
        assert!(xs.iter().any(|x| (x - 498.0).abs() < EPS), "streak stays");
        assert!(
            !xs.iter().any(|x| (x - 1.0).abs() < EPS),
            "oldest mark gone"
        );
        assert!(xs.iter().any(|x| (x - 2.0).abs() < EPS), "next mark stays");
        assert!(xs.iter().any(|x| (x - 100.0).abs() < EPS), "new mark draws");
    }

    #[test]
    fn a_streak_a_flash_and_a_mark_draw_three_meshes_registered_once_each() {
        let mut rig = Rig::new();
        let streak = rig.effects.begin_streak(style(2.0, 0.1));
        rig.effects.flash(Vec3::X, 0.0, flash_style(0.4, ms(50)));
        rig.effects.mark(
            Vec3::Y,
            Vec3::Z,
            0.0,
            mark_style(0.2, Duration::from_secs(10), ms(500)),
        );

        for _ in 0..2 {
            rig.effects.place_streak(streak, Vec3::Z * 9.0, Vec3::X);
            let drawn = frame_by_mesh(&mut rig, EYE);
            assert_eq!(drawn.len(), 3);
            let handles: std::collections::BTreeSet<u32> = drawn.iter().map(|d| d.0).collect();
            assert_eq!(handles.len(), 3, "each effect kind has its own mesh");
        }

        assert_eq!(rig.backend.registered.len(), 3, "each uploaded once");
        let sizes: std::collections::BTreeSet<usize> =
            rig.backend.registered.iter().map(|m| m.0.len()).collect();
        assert_eq!(sizes, [4, 8].into(), "kite and quad have 4, star has 8");
    }

    #[test]
    fn quality_zero_after_spawning_clears_flashes_and_marks() {
        let mut rig = Rig::new();
        rig.effects.flash(Vec3::X, 0.0, flash_style(0.4, ms(50)));
        rig.effects.mark(
            Vec3::Y,
            Vec3::Z,
            0.0,
            mark_style(0.2, Duration::from_secs(10), ms(500)),
        );
        assert_eq!(rig.effects.live(), 2);

        rig.effects.set_quality(0);

        assert_eq!(rig.effects.live(), 0);
        assert!(rig.frame(EYE).is_empty());
    }

    #[test]
    fn a_mark_offsets_along_the_unit_normal() {
        let at = Vec3::new(4.0, 2.0, 1.0);
        let mut rig = Rig::new();
        rig.effects.mark(
            at,
            Vec3::new(0.0, 0.0, 3.0),
            0.0,
            mark_style(0.2, Duration::from_secs(10), ms(500)),
        );

        let drawn = rig.frame(EYE);

        assert_eq!(drawn.len(), 1);
        assert_near(translation(&drawn[0]), at + Vec3::Z * MARK_OFFSET);
        assert_near(z_axis(&drawn[0]), Vec3::Z);
    }

    #[test]
    fn a_mark_with_a_zero_or_non_finite_normal_draws_nothing() {
        let normals = [
            Vec3::ZERO,
            Vec3::new(f32::NAN, 0.0, 1.0),
            Vec3::new(0.0, f32::INFINITY, 0.0),
        ];
        for normal in normals {
            let mut rig = Rig::new();

            rig.effects.mark(
                Vec3::X,
                normal,
                0.0,
                mark_style(0.2, Duration::from_secs(10), ms(500)),
            );

            assert!(rig.frame(EYE).is_empty(), "normal {normal}");
        }
        let mut rig = Rig::new();
        rig.effects.mark(
            Vec3::X,
            Vec3::Z,
            0.0,
            mark_style(0.2, Duration::from_secs(10), ms(500)),
        );
        assert_eq!(rig.frame(EYE).len(), 1, "a valid normal still draws");
    }

    #[test]
    fn a_fade_longer_than_the_lifetime_does_not_panic_and_ends_at_the_lifetime() {
        let mut rig = Rig::new();
        rig.effects.mark(
            Vec3::X,
            Vec3::Z,
            0.0,
            mark_style(0.2, Duration::from_secs(1), Duration::from_secs(5)),
        );
        assert_eq!(rig.frame(EYE).len(), 1, "drawn at spawn");

        rig.effects.advance(ms(500));
        let drawn = rig.frame(EYE);
        assert!(drawn.iter().all(|i| x_axis(i).length().is_finite()));

        rig.effects.advance(ms(500));

        assert!(rig.frame(EYE).is_empty());
        assert_eq!(rig.effects.live(), 0);
    }

    #[test]
    fn a_non_finite_roll_draws_nothing() {
        let mut rig = Rig::new();
        rig.effects
            .flash(Vec3::X, 0.5, flash_style(0.3, Duration::from_secs(10)));
        rig.effects.mark(
            Vec3::X,
            Vec3::Z,
            0.5,
            mark_style(0.2, Duration::from_secs(10), ms(500)),
        );
        assert_eq!(rig.frame(EYE).len(), 2, "a finite roll draws");

        for roll in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            let mut rig = Rig::new();
            rig.effects
                .flash(Vec3::X, roll, flash_style(0.3, Duration::from_secs(10)));
            assert!(rig.frame(EYE).is_empty(), "flash roll {roll}");

            let mut rig = Rig::new();
            rig.effects.mark(
                Vec3::X,
                Vec3::Z,
                roll,
                mark_style(0.2, Duration::from_secs(10), ms(500)),
            );
            assert!(rig.frame(EYE).is_empty(), "mark roll {roll}");
        }
    }
}
