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

/// A flash: a camera-facing star that shrinks linearly to nothing over its lifetime.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FlashStyle {
    /// Tip-to-tip size at birth.
    pub size: f32,
    pub material: u32,
    /// A zero lifetime is refused at spawn.
    pub lifetime: Duration,
}

impl FlashStyle {
    pub fn new(size: f32, material: u32, lifetime: Duration) -> Self {
        Self {
            size,
            material,
            lifetime,
        }
    }
}

/// A mark: a square lying on a surface that holds full size, then shrinks out over its
/// last `fade`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MarkStyle {
    /// Side length at full size.
    pub size: f32,
    pub material: u32,
    /// A zero lifetime is refused at spawn.
    pub lifetime: Duration,
    /// The shrink at the end of the lifetime; longer than the lifetime clamps to it, zero
    /// is a hard cut.
    pub fade: Duration,
}

impl MarkStyle {
    pub fn new(size: f32, material: u32, lifetime: Duration, fade: Duration) -> Self {
        Self {
            size,
            material,
            lifetime,
            fade,
        }
    }
}

/// How far a mark is lifted off its surface along the normal.
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

/// A flash or mark: fixed at spawn, aged by `advance` and gone at its lifetime.
#[derive(Debug)]
struct Timed<S> {
    at: Vec3,
    roll: f32,
    style: S,
    age: Duration,
}

#[derive(Debug)]
enum Effect {
    Streak(Streak),
    Flash(Timed<FlashStyle>),
    /// The mark's state and its surface normal.
    Mark(Timed<MarkStyle>, Vec3),
}

/// Which built-in mesh an effect draws with; the index into `MESH_BUILDERS`, and the
/// offset from `EFFECT_MESH_IDS_FROM`.
const STREAK_MESH: usize = 0;
const FLASH_MESH: usize = 1;
const MARK_MESH: usize = 2;

const MESH_BUILDERS: [fn() -> WorldMesh; 3] = [streak_mesh, flash_mesh, mark_mesh];

/// One upload flag per built-in mesh.
type MeshUploaded = [bool; MESH_BUILDERS.len()];

impl Effect {
    /// Streaks end when their owner says so; flashes and marks have a fixed lifetime and
    /// are the only ones a full cap may evict.
    fn is_fixed_lifetime(&self) -> bool {
        !matches!(self, Self::Streak(_))
    }

    fn age(&mut self, dt: Duration) {
        match self {
            Self::Streak(streak) => {
                streak.age += dt;
                if streak.age >= streak.style.max_age {
                    streak.ended_at.get_or_insert(streak.style.max_age);
                }
            }
            Self::Flash(flash) => flash.age += dt,
            Self::Mark(mark, _) => mark.age += dt,
        }
    }

    fn finished(&self) -> bool {
        match self {
            Self::Streak(streak) => streak.finished_fading(),
            Self::Flash(flash) => flash.age >= flash.style.lifetime,
            Self::Mark(mark, _) => mark.age >= mark.style.lifetime,
        }
    }

    /// The mesh slot, transform and material to draw this frame, if it draws at all.
    fn drawing(&mut self, eye: Vec3) -> Option<(usize, Affine3A, u32)> {
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
                Some((STREAK_MESH, transform, streak.style.material))
            }
            Self::Flash(flash) => {
                let remaining = flash.style.lifetime.saturating_sub(flash.age);
                let scale = remaining.as_secs_f32() / flash.style.lifetime.as_secs_f32();
                let facing = (eye - flash.at).try_normalize()?;
                let transform =
                    rolled_frame(flash.at, facing, flash.roll, flash.style.size * scale)?;
                Some((FLASH_MESH, transform, flash.style.material))
            }
            Self::Mark(mark, normal) => {
                let lifetime = mark.style.lifetime;
                let fade = mark.style.fade.min(lifetime);
                let remaining = lifetime.saturating_sub(mark.age);
                let scale = if remaining >= fade {
                    1.0
                } else {
                    remaining.as_secs_f32() / fade.as_secs_f32()
                };
                let normal = normal.try_normalize()?;
                let transform = rolled_frame(
                    mark.at + normal * MARK_OFFSET,
                    normal,
                    mark.roll,
                    mark.style.size * scale,
                )?;
                Some((MARK_MESH, transform, mark.style.material))
            }
        }
    }
}

/// Streaks, flashes and marks in one store: one handle counter, one cap, one `advance`.
#[derive(Debug)]
pub struct Effects {
    store: BTreeMap<EffectHandle, Effect>,
    next_handle: u64,
    cap: usize,
    mesh_uploaded: MeshUploaded,
}

impl Default for Effects {
    fn default() -> Self {
        Self {
            store: BTreeMap::new(),
            next_handle: 0,
            cap: CAPS[DEFAULT_QUALITY],
            mesh_uploaded: [false; MESH_BUILDERS.len()],
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

    /// Store a flash or mark. At the cap the oldest flash or mark makes room; with none to
    /// drop (all streaks) the spawn is refused, as it is at quality 0 or with no lifetime.
    fn spawn_timed(&mut self, effect: Effect, lifetime: Duration) {
        if self.cap == 0 || lifetime.is_zero() {
            return;
        }
        if self.store.len() >= self.cap {
            let oldest = self
                .store
                .iter()
                .find_map(|(handle, effect)| effect.is_fixed_lifetime().then_some(*handle));
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

    /// Spawn a flash at `at`, rolled by `roll` about the line to the eye. It draws and
    /// shrinks until its lifetime ends; no handle, since nothing else is needed of it.
    /// Refused as described on the cap; see `set_quality`.
    pub fn flash(&mut self, at: Vec3, roll: f32, style: FlashStyle) {
        let flash = Timed {
            at,
            roll,
            style,
            age: Duration::ZERO,
        };
        self.spawn_timed(Effect::Flash(flash), style.lifetime);
    }

    /// Spawn a mark lying on a surface at `at` with unit `normal`, rolled by `roll` about
    /// the normal and lifted by `MARK_OFFSET`. It holds full size, then fades out.
    pub fn mark(&mut self, at: Vec3, normal: Vec3, roll: f32, style: MarkStyle) {
        let mark = Timed {
            at,
            roll,
            style,
            age: Duration::ZERO,
        };
        self.spawn_timed(Effect::Mark(mark, normal), style.lifetime);
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
            effect.age(dt);
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
            let Some((slot, transform, material)) = effect.drawing(eye) else {
                continue;
            };
            let id = WorldMeshId(EFFECT_MESH_IDS_FROM + slot as u64);
            if !std::mem::replace(&mut self.mesh_uploaded[slot], true) {
                meshes.upsert_movable(id, MESH_BUILDERS[slot]());
            }
            meshes.place_unshadowed(id, transform, material);
        }
    }
}

/// A frame with Z along the unit `z`, X rotated by `roll` about it and Y completing it,
/// scaled by `size` in X and Y, centred on `centre`. `None` for non-finite inputs or a
/// size that is not positive.
fn rolled_frame(centre: Vec3, z: Vec3, roll: f32, size: f32) -> Option<Affine3A> {
    if !(centre.is_finite() && roll.is_finite() && size.is_finite()) || size <= 0.0 {
        return None;
    }
    let x = Quat::from_axis_angle(z, roll) * z.any_orthonormal_vector();
    let y = z.cross(x);
    Some(Affine3A::from_cols(
        (x * size).into(),
        (y * size).into(),
        z.into(),
        centre.into(),
    ))
}

/// A four-point star centred on the origin, tips at radius 0.5, counter-clockwise seen
/// from +Z. Vertices alternate tip and inner corner.
fn flash_mesh() -> WorldMesh {
    const INNER: f32 = 0.1;
    WorldMesh::new(
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
        vec![[0.0, 0.0, 1.0]; 8],
        vec![1.0; 8],
        vec![[1.0; 3]; 8],
        vec![1.0; 8],
        vec![0; 8],
        vec![
            7, 0, 1, 1, 2, 3, 3, 4, 5, 5, 6, 7, // the four points
            1, 3, 5, 1, 5, 7, // the inner square
        ],
    )
    .expect("the star's channels are all eight vertices long and its indices are in range")
}

/// A unit square centred on the origin, counter-clockwise seen from +Z.
fn mark_mesh() -> WorldMesh {
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

    fn flash_style(size: f32, lifetime_ms: u64) -> FlashStyle {
        FlashStyle::new(size, 0, ms(lifetime_ms))
    }

    fn mark_style(size: f32) -> MarkStyle {
        MarkStyle::new(size, 0, Duration::from_secs(10), Duration::from_secs(1))
    }

    fn z_axis(instance: &InstanceGpu) -> Vec3 {
        let [x, y, z, _] = instance.model[2];
        Vec3::new(x, y, z)
    }

    fn columns_finite(instance: &InstanceGpu) -> bool {
        instance.model.iter().flatten().all(|v| v.is_finite())
    }

    /// The backend handle and instance of every unshadowed draw this frame.
    fn frame_by_mesh(rig: &mut Rig) -> Vec<(u32, InstanceGpu)> {
        rig.effects.draw(&mut rig.meshes, EYE);
        rig.meshes.flush(&mut rig.backend);
        let drawn = rig.meshes.unshadowed_draws().collect();
        rig.meshes.clear_placements();
        drawn
    }

    fn model_and_material(instances: &[InstanceGpu]) -> Vec<([[f32; 4]; 4], u32)> {
        instances.iter().map(|i| (i.model, i.material)).collect()
    }

    /// Where the mark's quad lies for `normal` and `roll`: its X and Y axes, unit length.
    fn mark_basis(normal: Vec3, roll: f32) -> (Vec3, Vec3) {
        let x = glam::Quat::from_axis_angle(normal, roll) * normal.any_orthonormal_vector();
        (x, normal.cross(x))
    }

    /// X coordinates of the marks drawn at y near 0.01, ascending.
    fn mark_xs(drawn: &[InstanceGpu]) -> Vec<i32> {
        let mut xs: Vec<i32> = drawn
            .iter()
            .filter(|i| translation(i).y < 1.0)
            .map(|i| translation(i).x.round() as i32)
            .collect();
        xs.sort_unstable();
        xs
    }

    #[test]
    fn a_flash_shrinks_linearly_and_expires() {
        let mut rig = Rig::new();
        let at = Vec3::new(3.0, 2.0, -1.0);

        rig.effects.flash(at, 0.0, flash_style(0.4, 50));
        let fresh = rig.frame(EYE);

        assert_eq!(fresh.len(), 1);
        assert_near(translation(&fresh[0]), at);
        assert!((x_axis(&fresh[0]).length() - 0.4).abs() < EPS);
        assert!((y_axis(&fresh[0]).length() - 0.4).abs() < EPS);

        rig.effects.advance(ms(10));
        let fifth = rig.frame(EYE);

        assert_eq!(fifth.len(), 1);
        assert!(
            (x_axis(&fifth[0]).length() - 0.32).abs() < EPS,
            "a linear shrink is 0.32 at 10 ms, got {}",
            x_axis(&fifth[0]).length()
        );
        assert!((y_axis(&fifth[0]).length() - 0.32).abs() < EPS);

        rig.effects.advance(ms(15));
        let half = rig.frame(EYE);

        assert_eq!(half.len(), 1, "drawn without being placed");
        assert_near(translation(&half[0]), at);
        assert!(
            (x_axis(&half[0]).length() - 0.2).abs() < EPS,
            "half size, got {}",
            x_axis(&half[0]).length()
        );
        assert!((y_axis(&half[0]).length() - 0.2).abs() < EPS);
        assert_eq!(rig.effects.live(), 1);

        rig.effects.advance(ms(25));

        assert!(rig.frame(EYE).is_empty());
        assert_eq!(rig.effects.live(), 0);
    }

    #[test]
    fn a_flash_faces_the_eye() {
        let at = Vec3::new(1.0, 1.0, 1.0);
        for eye in [EYE, Vec3::new(-20.0, 4.0, 7.0), at + Vec3::Y * 10.0] {
            let mut rig = Rig::new();
            rig.effects.flash(at, 0.0, flash_style(0.4, 50));

            let drawn = rig.frame(eye);

            assert_eq!(drawn.len(), 1);
            let (x, y, z) = (x_axis(&drawn[0]), y_axis(&drawn[0]), z_axis(&drawn[0]));
            assert_near(z.normalize(), (eye - at).normalize());
            assert!(x.dot(y).abs() < EPS, "X and Y are perpendicular");
            assert!(x.dot(z).abs() < EPS && y.dot(z).abs() < EPS);
            assert!((x.length() - 0.4).abs() < EPS && (y.length() - 0.4).abs() < EPS);
            assert!(x.cross(y).dot(eye - at) > 0.0, "front side toward the eye");
        }

        let second_eye = Vec3::new(-20.0, 4.0, 7.0);
        let mut rig = Rig::new();
        rig.effects.flash(at, 0.0, flash_style(0.4, 50));

        let first = rig.frame(EYE);
        let second = rig.frame(second_eye);

        assert_eq!(first.len(), 1);
        assert_eq!(second.len(), 1);
        assert_near(z_axis(&first[0]).normalize(), (EYE - at).normalize());
        assert_near(
            z_axis(&second[0]).normalize(),
            (second_eye - at).normalize(),
        );
        assert!(
            x_axis(&second[0])
                .cross(y_axis(&second[0]))
                .dot(second_eye - at)
                > 0.0,
            "the same flash follows the eye of the next frame"
        );
    }

    #[test]
    fn a_flash_rolls_about_the_eye_direction() {
        let at = Vec3::new(1.0, 1.0, 1.0);
        let mut rig = Rig::new();
        rig.effects.flash(at, 0.0, flash_style(0.4, 50));
        let unrolled_drawn = rig.frame(EYE);
        assert_eq!(unrolled_drawn.len(), 1);
        let unrolled = unrolled_drawn[0];

        for (roll, dot) in [
            (std::f32::consts::PI, -1.0),
            (std::f32::consts::FRAC_PI_2, 0.0),
        ] {
            let mut rig = Rig::new();
            rig.effects.flash(at, roll, flash_style(0.4, 50));
            let drawn = rig.frame(EYE);

            assert_eq!(drawn.len(), 1, "roll {roll}");
            let x = x_axis(&drawn[0]).normalize();
            assert!(
                (x.dot(x_axis(&unrolled).normalize()) - dot).abs() < EPS,
                "roll {roll}"
            );
            assert_near(z_axis(&drawn[0]).normalize(), z_axis(&unrolled).normalize());
        }
    }

    #[test]
    fn a_mark_lies_along_its_normal_offset_by_the_mark_offset() {
        let at = Vec3::new(4.0, 0.0, 2.0);
        let normal = Vec3::new(1.0, 2.0, -2.0).normalize();
        let mut rig = Rig::new();

        rig.effects.mark(at, normal, 0.0, mark_style(0.2));
        let drawn = rig.frame(EYE);

        assert_eq!(drawn.len(), 1);
        assert_near(translation(&drawn[0]), at + normal * MARK_OFFSET);
        assert_near(z_axis(&drawn[0]).normalize(), normal);
        let (x, y) = mark_basis(normal, 0.0);
        assert_near(x_axis(&drawn[0]), x * 0.2);
        assert_near(y_axis(&drawn[0]), y * 0.2);

        let from_elsewhere = rig.frame(Vec3::new(-30.0, 0.5, 8.0));

        assert_eq!(from_elsewhere.len(), 1);
        assert_eq!(
            from_elsewhere[0].model, drawn[0].model,
            "a mark does not face the eye"
        );
    }

    #[test]
    fn a_mark_rolls_about_its_normal() {
        let normal = Vec3::new(0.0, 0.6, 0.8);
        for roll in [std::f32::consts::FRAC_PI_2, std::f32::consts::PI, -0.7] {
            let mut rig = Rig::new();
            rig.effects.mark(Vec3::ZERO, normal, roll, mark_style(0.5));

            let drawn = rig.frame(EYE);

            assert_eq!(drawn.len(), 1, "roll {roll}");
            let (x, y) = mark_basis(normal, roll);
            assert_near(x_axis(&drawn[0]), x * 0.5);
            assert_near(y_axis(&drawn[0]), y * 0.5);
            assert_near(translation(&drawn[0]), normal * MARK_OFFSET);
        }
    }

    #[test]
    fn a_mark_holds_full_size_then_shrinks_out_in_its_fade() {
        let mut rig = Rig::new();
        let at = Vec3::new(0.0, 1.0, 0.0);
        rig.effects.mark(
            at,
            Vec3::Y,
            0.0,
            MarkStyle::new(0.2, 0, Duration::from_secs(10), Duration::from_secs(1)),
        );

        rig.effects.advance(Duration::from_secs(9));
        let full = rig.frame(EYE);
        assert_eq!(full.len(), 1);
        assert!((x_axis(&full[0]).length() - 0.2).abs() < EPS, "full at 9 s");
        assert!((y_axis(&full[0]).length() - 0.2).abs() < EPS);

        rig.effects.advance(ms(250));
        let quarter = rig.frame(EYE);
        assert_eq!(quarter.len(), 1);
        assert!(
            (x_axis(&quarter[0]).length() - 0.15).abs() < EPS,
            "three quarters at 9.25 s, got {}",
            x_axis(&quarter[0]).length()
        );
        assert!((y_axis(&quarter[0]).length() - 0.15).abs() < EPS);

        rig.effects.advance(ms(250));
        let half = rig.frame(EYE);
        assert_eq!(half.len(), 1);
        assert!(
            (x_axis(&half[0]).length() - 0.1).abs() < EPS,
            "half at 9.5 s, got {}",
            x_axis(&half[0]).length()
        );
        assert!((y_axis(&half[0]).length() - 0.1).abs() < EPS);
        assert_near(translation(&half[0]), at + Vec3::Y * MARK_OFFSET);

        rig.effects.advance(ms(500));
        assert!(rig.frame(EYE).is_empty(), "gone at 10 s");
        assert_eq!(rig.effects.live(), 0);
    }

    #[test]
    fn a_full_cap_drops_the_oldest_fixed_lifetime_effect() {
        let mut rig = Rig::new();
        rig.effects.set_quality(1);
        let streak = rig.effects.begin_streak(style(2.0, 0.1));
        rig.effects
            .place_streak(streak, Vec3::new(0.0, 50.0, 0.0), Vec3::X);
        for i in 0..31 {
            rig.effects
                .mark(Vec3::new(i as f32, 0.0, 0.0), Vec3::Y, 0.0, mark_style(0.2));
        }
        assert_eq!(rig.effects.live(), 32);

        rig.effects
            .mark(Vec3::new(31.0, 0.0, 0.0), Vec3::Y, 0.0, mark_style(0.2));
        rig.effects
            .place_streak(streak, Vec3::new(0.0, 50.0, 0.0), Vec3::X);
        let drawn = rig.frame(EYE);

        assert_eq!(rig.effects.live(), 32, "one in, one out");
        assert_eq!(drawn.len(), 32, "the streak survives with 31 marks");
        assert_eq!(mark_xs(&drawn), (1..=31).collect::<Vec<_>>());
    }

    #[test]
    fn a_full_cap_drops_the_oldest_across_flashes_and_marks() {
        let mut rig = Rig::new();
        rig.effects.set_quality(1);
        rig.effects
            .flash(Vec3::new(0.0, 0.0, 0.0), 0.0, flash_style(0.4, 5000));
        for i in 1..32 {
            rig.effects
                .mark(Vec3::new(i as f32, 0.0, 0.0), Vec3::Y, 0.0, mark_style(0.2));
        }

        rig.effects
            .mark(Vec3::new(32.0, 0.0, 0.0), Vec3::Y, 0.0, mark_style(0.2));
        let drawn = rig.frame(EYE);

        assert_eq!(rig.effects.live(), 32);
        assert_eq!(
            mark_xs(&drawn),
            (1..=32).collect::<Vec<_>>(),
            "the flash, the oldest, went"
        );
    }

    #[test]
    fn a_full_cap_of_streaks_refuses_a_mark_and_keeps_every_streak() {
        let mut rig = Rig::new();
        rig.effects.set_quality(1);
        let handles: Vec<EffectHandle> = (0..32)
            .map(|_| rig.effects.begin_streak(style(2.0, 0.1)))
            .collect();
        let place_all = |rig: &mut Rig| {
            for (i, handle) in handles.iter().enumerate() {
                let head = Vec3::new(i as f32, 50.0, 0.0);
                rig.effects.place_streak(*handle, head, Vec3::X);
            }
        };

        rig.effects
            .mark(Vec3::new(500.0, 0.0, 0.0), Vec3::Y, 0.0, mark_style(0.2));
        place_all(&mut rig);
        let drawn = rig.frame(EYE);

        assert_eq!(rig.effects.live(), 32);
        assert_eq!(drawn.len(), 32, "every streak stays, no mark");
        assert!(mark_xs(&drawn).is_empty(), "the refused mark is not drawn");

        rig.effects.end(handles[0]);
        rig.effects.advance(Duration::from_secs(1));
        place_all(&mut rig);
        let after = rig.frame(EYE);
        assert_eq!(rig.effects.live(), 31);
        assert!(
            mark_xs(&after).is_empty(),
            "the refused mark was not queued"
        );

        rig.effects
            .mark(Vec3::new(500.0, 0.0, 0.0), Vec3::Y, 0.0, mark_style(0.2));
        assert_eq!(rig.effects.live(), 32, "a freed slot takes a new mark");
        assert_eq!(mark_xs(&rig.frame(EYE)), vec![500]);
    }

    #[test]
    fn quality_zero_refuses_flash_and_mark() {
        let mut rig = Rig::new();
        rig.effects.set_quality(0);

        rig.effects.flash(Vec3::X, 0.0, flash_style(0.4, 50));
        rig.effects.mark(Vec3::Y, Vec3::Y, 0.0, mark_style(0.2));

        assert_eq!(rig.effects.live(), 0);
        assert!(rig.frame(EYE).is_empty());

        rig.effects.set_quality(4);
        assert_eq!(rig.effects.live(), 0, "refused spawns are not queued");
        assert!(rig.frame(EYE).is_empty());

        rig.effects.flash(Vec3::X, 0.0, flash_style(0.4, 50));
        assert_eq!(rig.effects.live(), 1);
        assert_eq!(rig.frame(EYE).len(), 1);
    }

    #[test]
    fn lowering_quality_keeps_live_effects_and_a_spawn_replaces_only_the_oldest() {
        let mut rig = Rig::new();
        for i in 0..40 {
            rig.effects
                .mark(Vec3::new(i as f32, 0.0, 0.0), Vec3::Y, 0.0, mark_style(0.2));
        }

        rig.effects.set_quality(1);
        assert_eq!(rig.effects.live(), 40, "kept though over the new cap");
        rig.effects
            .mark(Vec3::new(40.0, 0.0, 0.0), Vec3::Y, 0.0, mark_style(0.2));
        rig.effects
            .mark(Vec3::new(41.0, 0.0, 0.0), Vec3::Y, 0.0, mark_style(0.2));
        let drawn = rig.frame(EYE);

        assert_eq!(rig.effects.live(), 40, "one in, one out each time");
        assert_eq!(mark_xs(&drawn), (2..=41).collect::<Vec<_>>());
    }

    #[test]
    fn lowering_quality_over_streaks_replaces_a_mark_never_a_streak() {
        let mut rig = Rig::new();
        let handles: Vec<EffectHandle> = (0..35)
            .map(|_| rig.effects.begin_streak(style(2.0, 0.1)))
            .collect();
        for i in 0..5 {
            rig.effects
                .mark(Vec3::new(i as f32, 0.0, 0.0), Vec3::Y, 0.0, mark_style(0.2));
        }
        rig.effects.set_quality(1);

        rig.effects
            .mark(Vec3::new(5.0, 0.0, 0.0), Vec3::Y, 0.0, mark_style(0.2));
        for handle in &handles {
            rig.effects
                .place_streak(*handle, Vec3::new(0.0, 50.0, 0.0), Vec3::X);
        }
        let drawn = rig.frame(EYE);

        assert_eq!(rig.effects.live(), 40);
        assert_eq!(drawn.len(), 40, "all 35 streaks and 5 marks");
        assert_eq!(mark_xs(&drawn), (1..=5).collect::<Vec<_>>());
    }

    #[test]
    fn streak_flash_and_mark_meshes_are_registered_once_each_under_distinct_ids() {
        let mut rig = Rig::new();
        let streak = rig.effects.begin_streak(style(2.0, 0.1));
        let flash_at = Vec3::new(1.0, 0.0, 0.0);
        let mark_at = Vec3::new(5.0, 0.0, 0.0);

        let mut handles_per_frame = Vec::new();
        for _ in 0..3 {
            rig.effects
                .place_streak(streak, Vec3::new(0.0, 50.0, 0.0), Vec3::X);
            rig.effects.flash(flash_at, 0.0, flash_style(0.4, 5000));
            rig.effects.mark(mark_at, Vec3::Y, 0.0, mark_style(0.2));
            let frame = frame_by_mesh(&mut rig);
            let handle_at = |wanted: Vec3| {
                frame
                    .iter()
                    .find(|(_, i)| (translation(i) - wanted).length() < 0.1)
                    .map(|(handle, _)| *handle)
                    .expect("the effect drew")
            };
            handles_per_frame.push([
                handle_at(flash_at),
                handle_at(mark_at + Vec3::Y * MARK_OFFSET),
                handle_at(Vec3::new(-2.0, 50.0, 0.0)),
            ]);
        }

        assert_eq!(rig.backend.registered.len(), 3, "each uploaded once");
        let [flash_handle, mark_handle, streak_handle] = handles_per_frame[0];
        assert_ne!(flash_handle, mark_handle);
        assert_ne!(flash_handle, streak_handle);
        assert_ne!(mark_handle, streak_handle);
        assert!(
            handles_per_frame.iter().all(|h| *h == handles_per_frame[0]),
            "each kind keeps its mesh across frames"
        );
    }

    #[test]
    fn the_flash_mesh_is_a_four_point_star_centred_on_the_origin_wound_toward_plus_z() {
        let mut rig = Rig::new();
        rig.effects.flash(Vec3::X, 0.0, flash_style(0.4, 5000));

        rig.frame(EYE);

        assert_eq!(rig.backend.registered.len(), 1);
        let (star, indices) = &rig.backend.registered[0];
        assert_eq!(star.len(), 8, "a four-point star");
        assert!(!indices.is_empty());
        assert_eq!(indices.len() % 3, 0, "whole triangles");
        assert!(indices.iter().all(|i| (*i as usize) < star.len()));
        assert!(star.iter().all(|p| p[2] == 0.0), "in the XY plane");
        let sum: Vec3 = star.iter().map(|p| Vec3::from(*p)).sum();
        assert_near(sum, Vec3::ZERO);
        let tip = star
            .iter()
            .map(|p| Vec3::from(*p).length())
            .fold(0.0, f32::max);
        assert!(
            (tip - 0.5).abs() < EPS,
            "tips at radius 0.5 so size is tip to tip, got {tip}"
        );
        for triangle in indices.chunks(3) {
            let [a, b, c] = [0, 1, 2].map(|i| Vec3::from(star[triangle[i] as usize]));
            assert!((b - a).cross(c - a).z > 0.0, "star triangle {triangle:?}");
        }
    }

    #[test]
    fn the_mark_mesh_is_a_unit_quad_centred_on_the_origin_wound_toward_plus_z() {
        let mut rig = Rig::new();
        rig.effects.mark(Vec3::X, Vec3::Y, 0.0, mark_style(0.2));

        rig.frame(EYE);

        assert_eq!(rig.backend.registered.len(), 1);
        let (quad, indices) = &rig.backend.registered[0];
        assert_eq!(quad.len(), 4);
        assert_eq!(indices.len(), 6, "two triangles");
        assert!(indices.iter().all(|i| (*i as usize) < quad.len()));
        assert!(quad.iter().all(|p| p[2] == 0.0), "in the XY plane");
        for axis in 0..2 {
            let low = quad.iter().map(|p| p[axis]).fold(f32::MAX, f32::min);
            let high = quad.iter().map(|p| p[axis]).fold(f32::MIN, f32::max);
            assert!((low + 0.5).abs() < EPS, "axis {axis} starts at -0.5");
            assert!((high - 0.5).abs() < EPS, "axis {axis} ends at 0.5");
        }
        for triangle in indices.chunks(3) {
            let [a, b, c] = [0, 1, 2].map(|i| Vec3::from(quad[triangle[i] as usize]));
            assert!((b - a).cross(c - a).z > 0.0, "quad triangle {triangle:?}");
        }
    }

    #[test]
    fn a_flash_at_a_full_cap_of_marks_replaces_the_oldest_mark() {
        let mut rig = Rig::new();
        rig.effects.set_quality(1);
        for i in 0..32 {
            rig.effects
                .mark(Vec3::new(i as f32, 0.0, 0.0), Vec3::Y, 0.0, mark_style(0.2));
        }
        assert_eq!(rig.effects.live(), 32);

        rig.effects
            .flash(Vec3::new(32.0, 0.0, 0.0), 0.0, flash_style(0.4, 5000));
        let drawn = rig.frame(EYE);

        assert_eq!(rig.effects.live(), 32, "one in, one out");
        assert_eq!(
            mark_xs(&drawn),
            (1..=32).collect::<Vec<_>>(),
            "mark 0, the oldest, went and the flash is in"
        );
    }

    #[test]
    fn a_flash_at_a_full_cap_of_streaks_is_refused_and_every_streak_stays() {
        let mut rig = Rig::new();
        rig.effects.set_quality(1);
        let handles: Vec<EffectHandle> = (0..32)
            .map(|_| rig.effects.begin_streak(style(2.0, 0.1)))
            .collect();
        let place_all = |rig: &mut Rig| {
            for (i, handle) in handles.iter().enumerate() {
                let head = Vec3::new(i as f32, 50.0, 0.0);
                rig.effects.place_streak(*handle, head, Vec3::X);
            }
        };

        rig.effects
            .flash(Vec3::new(500.0, 0.0, 0.0), 0.0, flash_style(0.4, 5000));
        place_all(&mut rig);
        let drawn = rig.frame(EYE);

        assert_eq!(rig.effects.live(), 32);
        assert_eq!(drawn.len(), 32, "every streak stays, no flash");
        assert!(mark_xs(&drawn).is_empty());

        rig.effects.end(handles[0]);
        rig.effects.advance(Duration::from_secs(1));
        rig.effects
            .flash(Vec3::new(500.0, 0.0, 0.0), 0.0, flash_style(0.4, 5000));
        assert_eq!(rig.effects.live(), 32, "a freed slot takes a new flash");
        assert_eq!(mark_xs(&rig.frame(EYE)), vec![500]);
    }

    #[test]
    fn a_fading_streak_is_not_fixed_lifetime_so_a_mark_neither_evicts_it_nor_fits() {
        let mut rig = Rig::new();
        rig.effects.set_quality(1);
        let handles: Vec<EffectHandle> = (0..32)
            .map(|_| rig.effects.begin_streak(style(2.0, 0.1)))
            .collect();
        for (i, handle) in handles.iter().enumerate() {
            let head = Vec3::new(i as f32, 50.0, 0.0);
            rig.effects.place_streak(*handle, head, Vec3::X);
        }
        rig.frame(EYE);
        rig.effects.end(handles[0]);
        rig.effects.advance(ms(10));

        rig.effects
            .mark(Vec3::new(500.0, 0.0, 0.0), Vec3::Y, 0.0, mark_style(0.2));
        let drawn = rig.frame(EYE);

        assert_eq!(rig.effects.live(), 32, "the fading streak was not evicted");
        assert_eq!(drawn.len(), 1, "only the fading streak draws, unplaced");
        assert!(mark_xs(&drawn).is_empty(), "and the mark was refused");

        rig.effects.advance(ms(50));
        assert_eq!(rig.effects.live(), 31, "the fade ran its course");
        rig.effects
            .mark(Vec3::new(500.0, 0.0, 0.0), Vec3::Y, 0.0, mark_style(0.2));
        assert_eq!(rig.effects.live(), 32, "its freed slot takes a mark");
        assert_eq!(mark_xs(&rig.frame(EYE)), vec![500]);
    }

    #[test]
    fn quality_zero_clears_a_live_flash_and_mark() {
        let mut rig = Rig::new();
        rig.effects.flash(Vec3::X, 0.0, flash_style(0.4, 5000));
        rig.effects.mark(Vec3::Y, Vec3::Y, 0.0, mark_style(0.2));
        assert_eq!(rig.effects.live(), 2);
        assert_eq!(rig.frame(EYE).len(), 2, "both draw before");

        rig.effects.set_quality(0);

        assert_eq!(rig.effects.live(), 0);
        assert!(rig.frame(EYE).is_empty());
        rig.effects.set_quality(4);
        assert_eq!(rig.effects.live(), 0, "they do not come back");
        assert!(rig.frame(EYE).is_empty());
    }

    #[test]
    fn a_mark_with_no_fade_holds_full_size_then_is_cut() {
        let mut rig = Rig::new();
        rig.effects.mark(
            Vec3::new(0.0, 1.0, 0.0),
            Vec3::Y,
            0.0,
            MarkStyle::new(0.2, 0, Duration::from_secs(10), Duration::ZERO),
        );

        rig.effects.advance(ms(9999));
        let last = rig.frame(EYE);

        assert_eq!(last.len(), 1);
        assert!(columns_finite(&last[0]));
        assert!((x_axis(&last[0]).length() - 0.2).abs() < EPS, "still full");
        assert!((y_axis(&last[0]).length() - 0.2).abs() < EPS);

        rig.effects.advance(ms(1));

        assert!(rig.frame(EYE).is_empty(), "gone at its lifetime");
        assert_eq!(rig.effects.live(), 0);
    }

    #[test]
    fn a_mark_whose_fade_exceeds_its_lifetime_fades_over_its_lifetime() {
        let mut rig = Rig::new();
        rig.effects.mark(
            Vec3::new(0.0, 1.0, 0.0),
            Vec3::Y,
            0.0,
            MarkStyle::new(0.2, 0, Duration::from_secs(1), Duration::from_secs(4)),
        );

        let fresh = rig.frame(EYE);
        assert_eq!(fresh.len(), 1);
        assert!(columns_finite(&fresh[0]));
        assert!((x_axis(&fresh[0]).length() - 0.2).abs() < EPS, "full at 0");

        for (advance, scale) in [(250, 0.15), (250, 0.1), (250, 0.05)] {
            rig.effects.advance(ms(advance));
            let drawn = rig.frame(EYE);

            assert_eq!(drawn.len(), 1);
            assert!(columns_finite(&drawn[0]));
            assert!(
                (x_axis(&drawn[0]).length() - scale).abs() < EPS,
                "expected {scale}, got {}",
                x_axis(&drawn[0]).length()
            );
            assert!((y_axis(&drawn[0]).length() - scale).abs() < EPS);
        }

        rig.effects.advance(ms(250));

        assert!(rig.frame(EYE).is_empty());
        assert_eq!(rig.effects.live(), 0);
    }

    #[test]
    fn a_flash_or_mark_with_no_lifetime_draws_nothing_and_does_not_stay_live() {
        let mut rig = Rig::new();
        rig.effects.flash(Vec3::X, 0.0, flash_style(0.4, 0));
        rig.effects.mark(
            Vec3::Z * 5.0,
            Vec3::Y,
            0.0,
            MarkStyle::new(0.2, 0, Duration::ZERO, ms(100)),
        );
        rig.effects
            .flash(Vec3::new(20.0, 0.0, 0.0), 0.0, flash_style(0.4, 50));

        let first = rig.frame(EYE);

        assert_eq!(first.len(), 1, "only the flash with a lifetime draws");
        assert!(first.iter().all(columns_finite));
        assert_near(translation(&first[0]), Vec3::new(20.0, 0.0, 0.0));

        rig.effects.advance(Duration::ZERO);

        assert_eq!(rig.effects.live(), 1, "the zero-lifetime ones are gone");
        let after = rig.frame(EYE);
        assert_eq!(after.len(), 1);
        assert!(after.iter().all(columns_finite));
        assert_near(translation(&after[0]), Vec3::new(20.0, 0.0, 0.0));
    }

    #[test]
    fn flash_and_mark_draw_with_their_material() {
        let mut rig = Rig::new();
        rig.effects
            .flash(Vec3::X, 0.0, FlashStyle::new(0.4, 7, ms(50)));
        rig.effects.mark(
            Vec3::Z * 5.0,
            Vec3::Y,
            0.0,
            MarkStyle::new(0.2, 9, Duration::from_secs(1), ms(100)),
        );

        let mut materials: Vec<u32> = rig.frame(EYE).iter().map(|i| i.material).collect();

        materials.sort_unstable();
        assert_eq!(materials, vec![7, 9]);
    }

    #[test]
    fn the_same_spawns_draw_the_same_instances() {
        let run = || {
            let mut rig = Rig::new();
            let mut frames = Vec::new();
            rig.effects
                .flash(Vec3::new(1.0, 2.0, 3.0), 0.3, flash_style(0.4, 50));
            rig.effects.mark(
                Vec3::new(4.0, 0.0, 1.0),
                Vec3::new(0.0, 0.6, 0.8),
                1.1,
                mark_style(0.2),
            );
            for _ in 0..4 {
                frames.push(model_and_material(&rig.frame(EYE)));
                rig.effects.advance(ms(20));
            }
            frames
        };

        let first = run();
        let second = run();

        assert_eq!(first[0].len(), 2, "the flash and the mark drew");
        assert_eq!(first, second);
    }

    proptest! {
        #[test]
        fn a_mark_basis_is_finite_for_any_normal(
            normal in prop_oneof![
                Just(Vec3::X), Just(Vec3::Y), Just(Vec3::Z),
                Just(-Vec3::X), Just(-Vec3::Y), Just(-Vec3::Z),
                (0.0f32..std::f32::consts::TAU, -1.5f32..1.5).prop_map(|(yaw, pitch)| {
                    Vec3::new(pitch.cos() * yaw.cos(), pitch.sin(), pitch.cos() * yaw.sin())
                }),
            ],
            roll in -7.0f32..7.0,
            size in 0.01f32..2.0,
        ) {
            let mut rig = Rig::new();

            rig.effects.mark(Vec3::new(1.0, 2.0, 3.0), normal, roll, mark_style(size));
            let drawn = rig.frame(EYE);

            prop_assert_eq!(drawn.len(), 1);
            let instance = &drawn[0];
            prop_assert!(columns_finite(instance));
            prop_assert!((x_axis(instance).length() - size).abs() < 1e-3);
            prop_assert!((y_axis(instance).length() - size).abs() < 1e-3);
            prop_assert!(x_axis(instance).dot(normal).abs() < 1e-3 * size);
            prop_assert!(y_axis(instance).dot(normal).abs() < 1e-3 * size);
            prop_assert!(x_axis(instance).dot(y_axis(instance)).abs() < 1e-3 * size * size);
            prop_assert!(z_axis(instance).normalize().dot(normal) > 0.999);
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
    fn an_invalid_mark_draws_nothing_and_a_following_valid_mark_still_draws() {
        let valid_at = Vec3::new(7.0, 0.0, 0.0);
        let invalid: [(Vec3, Vec3); 4] = [
            (Vec3::new(1.0, 0.0, 0.0), Vec3::ZERO),
            (Vec3::new(1.0, 0.0, 0.0), Vec3::new(f32::NAN, 0.0, 1.0)),
            (Vec3::new(f32::INFINITY, 0.0, 0.0), Vec3::Y),
            (Vec3::new(0.0, f32::NAN, 0.0), Vec3::Y),
        ];
        for (at, normal) in invalid {
            let mut rig = Rig::new();

            rig.effects.mark(at, normal, 0.0, mark_style(0.2));
            let none = rig.frame(EYE);
            rig.effects.mark(valid_at, Vec3::Y, 0.0, mark_style(0.2));
            let one = rig.frame(EYE);

            assert!(none.is_empty(), "at {at}, normal {normal}");
            assert_eq!(one.len(), 1, "at {at}, normal {normal}");
            assert!(columns_finite(&one[0]));
            assert_near(translation(&one[0]), valid_at + Vec3::Y * MARK_OFFSET);
        }
    }

    #[test]
    fn a_flash_or_mark_with_a_non_positive_or_nan_size_draws_nothing() {
        for size in [0.0, -0.4, f32::NAN, f32::INFINITY] {
            let mut rig = Rig::new();

            rig.effects.flash(Vec3::X, 0.0, flash_style(size, 50));
            rig.effects
                .mark(Vec3::Z * 5.0, Vec3::Y, 0.0, mark_style(size));
            let drawn = rig.frame(EYE);

            assert!(drawn.is_empty(), "size {size}");
        }
    }

    #[test]
    fn a_mark_with_a_non_unit_normal_draws_the_same_instance_as_the_unit_normal() {
        let at = Vec3::new(2.0, 3.0, 4.0);
        let mut unit = Rig::new();
        unit.effects.mark(at, Vec3::Z, 0.5, mark_style(0.2));
        let expected = unit.frame(EYE);
        for scale in [5.0, 0.25, 1e3] {
            let mut rig = Rig::new();

            rig.effects.mark(at, Vec3::Z * scale, 0.5, mark_style(0.2));
            let drawn = rig.frame(EYE);

            assert_eq!(drawn.len(), 1, "scale {scale}");
            assert_near(translation(&drawn[0]), at + Vec3::Z * MARK_OFFSET);
            for (got, want) in drawn[0]
                .model
                .iter()
                .flatten()
                .zip(expected[0].model.iter().flatten())
            {
                assert!((got - want).abs() < EPS, "scale {scale}: {got} vs {want}");
            }
        }
    }

    #[test]
    fn a_flash_at_the_eye_draws_nothing_that_frame_and_draws_once_the_eye_moves() {
        let at = Vec3::new(1.0, 2.0, 3.0);
        let mut rig = Rig::new();
        rig.effects.flash(at, 0.0, flash_style(0.4, 5000));

        let at_eye = rig.frame(at);
        let moved = rig.frame(at + Vec3::new(0.0, 0.0, 5.0));

        assert!(at_eye.is_empty(), "no draw, so no NaN instance");
        assert_eq!(rig.effects.live(), 1, "still live");
        assert_eq!(moved.len(), 1);
        assert!(columns_finite(&moved[0]));
        assert_near(translation(&moved[0]), at);
        assert!((x_axis(&moved[0]).length() - 0.4).abs() < EPS);
    }

    #[test]
    fn refused_spawns_do_not_consume_a_handle() {
        let mut rig = Rig::new();
        let first = rig.effects.begin_streak(style(2.0, 0.1));

        rig.effects.flash(Vec3::X, 0.0, flash_style(0.4, 0));
        rig.effects.mark(
            Vec3::X,
            Vec3::Y,
            0.0,
            MarkStyle::new(0.2, 0, Duration::ZERO, Duration::ZERO),
        );
        let second = rig.effects.begin_streak(style(2.0, 0.1));
        rig.effects.set_quality(0);
        rig.effects.flash(Vec3::X, 0.0, flash_style(0.4, 50));
        rig.effects.mark(Vec3::X, Vec3::Y, 0.0, mark_style(0.2));
        rig.effects.set_quality(4);
        let third = rig.effects.begin_streak(style(2.0, 0.1));

        assert!(first < second && second < third, "ordered, never reused");
        assert_eq!(second.0, first.0 + 1, "zero-lifetime spawns took no handle");
        assert_eq!(third.0, second.0 + 1, "quality-0 spawns took no handle");
    }

    #[test]
    fn a_spawn_refused_by_a_cap_of_streaks_does_not_consume_a_handle() {
        let mut rig = Rig::new();
        rig.effects.set_quality(1);
        let handles: Vec<EffectHandle> = (0..32)
            .map(|_| rig.effects.begin_streak(style(2.0, 0.1)))
            .collect();
        let last = *handles.last().expect("32 streaks");

        rig.effects.mark(Vec3::X, Vec3::Y, 0.0, mark_style(0.2));
        rig.effects.flash(Vec3::X, 0.0, flash_style(0.4, 50));
        let next = rig.effects.begin_streak(style(2.0, 0.1));

        assert!(next > last);
        assert_eq!(next.0, last.0 + 1, "the refused spawns took no handle");
    }

    #[test]
    fn an_invalid_mark_at_a_full_cap_never_exceeds_the_cap() {
        let mut rig = Rig::new();
        rig.effects.set_quality(1);
        for i in 0..32 {
            rig.effects
                .mark(Vec3::new(i as f32, 0.0, 0.0), Vec3::Y, 0.0, mark_style(0.2));
        }

        rig.effects
            .mark(Vec3::new(99.0, 0.0, 0.0), Vec3::ZERO, 0.0, mark_style(0.2));
        let drawn = rig.frame(EYE);

        assert!(rig.effects.live() <= 32, "live {}", rig.effects.live());
        assert!(drawn.len() <= 32);
    }

    #[test]
    fn a_huge_advance_removes_flashes_and_marks() {
        let mut rig = Rig::new();
        rig.effects.flash(Vec3::X, 0.0, flash_style(0.4, 50));
        rig.effects
            .mark(Vec3::Z * 5.0, Vec3::Y, 0.0, mark_style(0.2));

        rig.effects.advance(Duration::from_secs(1_000_000));

        assert_eq!(rig.effects.live(), 0);
        assert!(rig.frame(EYE).is_empty());
    }

    #[test]
    fn a_flash_a_mark_and_a_streak_in_one_cell_draw_three_times_on_three_meshes() {
        let cell = Vec3::new(2.0, 3.0, 4.0);
        let mut rig = Rig::new();
        let streak = rig.effects.begin_streak(style(2.0, 0.1));
        rig.effects.place_streak(streak, cell, Vec3::X);
        rig.effects.flash(cell, 0.0, flash_style(0.4, 5000));
        rig.effects.mark(cell, Vec3::Y, 0.0, mark_style(0.2));

        let frame = frame_by_mesh(&mut rig);

        assert_eq!(frame.len(), 3);
        let meshes: std::collections::BTreeSet<u32> = frame.iter().map(|(m, _)| *m).collect();
        assert_eq!(meshes.len(), 3, "one draw per mesh id");
        assert!(frame.iter().all(|(_, i)| columns_finite(i)));
    }
}
