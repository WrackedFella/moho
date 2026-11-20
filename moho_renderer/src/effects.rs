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

/// A flash: a camera-facing star at a point that shrinks to nothing over its lifetime.
/// `size` is the star's width tip to tip.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FlashStyle {
    pub size: f32,
    pub material: u32,
    pub lifetime: Duration,
}

/// An impact mark: a flat square on a surface that holds full size, then shrinks out over
/// its last `fade`.
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

impl Flash {
    /// Size factor: 1 at spawn, falling linearly to 0 at the end of the lifetime.
    fn scale(&self) -> f32 {
        let lifetime = self.style.lifetime.as_secs_f32();
        if lifetime <= 0.0 {
            return 0.0;
        }
        (1.0 - self.age.as_secs_f32() / lifetime).clamp(0.0, 1.0)
    }
}

#[derive(Debug)]
struct Mark {
    at: Vec3,
    /// Unit surface normal.
    normal: Vec3,
    roll: f32,
    style: MarkStyle,
    age: Duration,
}

impl Mark {
    /// Size factor: 1 until the last `fade` of the lifetime, then falling linearly to 0.
    fn scale(&self) -> f32 {
        let fade = self.style.fade.min(self.style.lifetime);
        let fade_start = self.style.lifetime.saturating_sub(fade);
        if self.age < fade_start {
            return 1.0;
        }
        if fade.is_zero() {
            return 0.0;
        }
        let into_fade = self.age.saturating_sub(fade_start);
        (1.0 - into_fade.as_secs_f32() / fade.as_secs_f32()).clamp(0.0, 1.0)
    }
}

/// One live effect; the closed set the store holds.
#[derive(Debug)]
enum Effect {
    Streak(Streak),
    Flash(Flash),
    Mark(Mark),
}

/// An effect kind's reserved mesh: its id is `EFFECT_MESH_IDS_FROM` plus the slot's index.
#[derive(Debug, Clone, Copy)]
enum MeshSlot {
    Streak,
    Flash,
    Mark,
}

impl MeshSlot {
    const COUNT: usize = 3;

    fn id(self) -> WorldMeshId {
        WorldMeshId(EFFECT_MESH_IDS_FROM + self as u64)
    }

    fn mesh(self) -> WorldMesh {
        match self {
            Self::Streak => streak_mesh(),
            Self::Flash => star_mesh(),
            Self::Mark => quad_mesh(),
        }
    }
}

#[derive(Debug)]
pub struct Effects {
    store: BTreeMap<EffectHandle, Effect>,
    next_handle: u64,
    cap: usize,
    /// Whether each `MeshSlot`'s mesh has been queued for upload.
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
    fn next_handle(&mut self) -> EffectHandle {
        let handle = EffectHandle(self.next_handle);
        self.next_handle += 1;
        handle
    }

    /// Start a streak. At the quality's cap the handle is inert: never stored, so placing
    /// or ending it does nothing.
    pub fn begin_streak(&mut self, style: StreakStyle) -> EffectHandle {
        let handle = self.next_handle();
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

    /// Spawn a flash at `at`, turned by `roll` radians about the line to the eye. It draws
    /// every frame until its lifetime ends; nothing is returned or needs removing.
    ///
    /// Refused, storing and evicting nothing, when `at` or `roll` is not finite or `size` is
    /// not a positive finite number, and at quality 0. At the cap the oldest flash or mark is
    /// dropped; with only streaks live the flash is refused.
    pub fn flash(&mut self, at: Vec3, roll: f32, style: FlashStyle) {
        if !spawnable(at, roll, style.size) {
            return;
        }
        self.spawn(Effect::Flash(Flash {
            at,
            roll,
            style,
            age: Duration::ZERO,
        }));
    }

    /// Spawn a mark at `at` lying on a surface with `normal` (normalised here), turned by
    /// `roll` radians about the normal. It holds full size until `fade` before the end of
    /// its lifetime, then shrinks out; nothing is returned or needs removing.
    ///
    /// Refused, storing and evicting nothing, when `at` or `roll` is not finite, `normal`
    /// cannot be normalised or `size` is not a positive finite number, and at quality 0. At the cap
    /// the oldest flash or mark is dropped; with only streaks live the mark is refused.
    pub fn mark(&mut self, at: Vec3, normal: Vec3, roll: f32, style: MarkStyle) {
        let Some(normal) = normal.try_normalize() else {
            return;
        };
        if !spawnable(at, roll, style.size) {
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

    /// Store a fixed-lifetime effect, first dropping the oldest flash or mark if the cap
    /// is reached; refused when only streaks are live.
    fn spawn(&mut self, effect: Effect) {
        if self.store.len() >= self.cap {
            let oldest = self
                .store
                .iter()
                .find(|(_, effect)| !matches!(effect, Effect::Streak(_)))
                .map(|(handle, _)| *handle);
            let Some(oldest) = oldest else {
                return;
            };
            self.store.remove(&oldest);
        }
        let handle = self.next_handle();
        self.store.insert(handle, effect);
    }

    /// Start the fade; the streak then draws at its last placement until it is gone.
    pub fn end(&mut self, handle: EffectHandle) {
        if let Some(Effect::Streak(streak)) = self.store.get_mut(&handle) {
            streak.ended_at.get_or_insert(streak.age);
        }
    }

    /// Age every effect by the frame time, start the fade of any streak past its
    /// `max_age` and drop the finished and expired ones.
    pub fn advance(&mut self, dt: Duration) {
        for effect in self.store.values_mut() {
            match effect {
                Effect::Streak(streak) => {
                    streak.age += dt;
                    if streak.age >= streak.style.max_age {
                        streak.ended_at.get_or_insert(streak.style.max_age);
                    }
                }
                Effect::Flash(flash) => flash.age += dt,
                Effect::Mark(mark) => mark.age += dt,
            }
        }
        self.store.retain(|_, effect| match effect {
            Effect::Streak(streak) => !streak.finished_fading(),
            Effect::Flash(flash) => flash.age < flash.style.lifetime,
            Effect::Mark(mark) => mark.age < mark.style.lifetime,
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

    /// Place every streak placed this frame or fading and every flash and mark,
    /// unshadowed; streaks and flashes face `eye`.
    pub(crate) fn draw(&mut self, meshes: &mut WorldMeshes, eye: Vec3) {
        for effect in self.store.values_mut() {
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
                            &mut self.uploaded,
                            meshes,
                            MeshSlot::Streak,
                            transform,
                            streak.style.material,
                        );
                    }
                }
                Effect::Flash(flash) => {
                    let size = flash.style.size * flash.scale();
                    if size <= 0.0 {
                        continue;
                    }
                    let toward_eye = (eye - flash.at).try_normalize().unwrap_or(Vec3::Z);
                    let transform = flat_transform(toward_eye, flash.roll, size, flash.at);
                    place(
                        &mut self.uploaded,
                        meshes,
                        MeshSlot::Flash,
                        transform,
                        flash.style.material,
                    );
                }
                Effect::Mark(mark) => {
                    let size = mark.style.size * mark.scale();
                    if size <= 0.0 {
                        continue;
                    }
                    let lifted = mark.at + mark.normal * MARK_OFFSET;
                    let transform = flat_transform(mark.normal, mark.roll, size, lifted);
                    place(
                        &mut self.uploaded,
                        meshes,
                        MeshSlot::Mark,
                        transform,
                        mark.style.material,
                    );
                }
            }
        }
    }
}

/// Whether a flash or mark at `at`, turned by `roll`, of `size` can be drawn at all.
fn spawnable(at: Vec3, roll: f32, size: f32) -> bool {
    at.is_finite() && roll.is_finite() && size.is_finite() && size > 0.0
}

/// Queue `slot`'s mesh the first time only, then place an unshadowed instance of it.
fn place(
    uploaded: &mut [bool; MeshSlot::COUNT],
    meshes: &mut WorldMeshes,
    slot: MeshSlot,
    transform: Affine3A,
    material: u32,
) {
    if !std::mem::replace(&mut uploaded[slot as usize], true) {
        meshes.upsert_movable(slot.id(), slot.mesh());
    }
    meshes.place_unshadowed(slot.id(), transform, material);
}

/// A flat mesh `size` across, facing along the unit `z`, turned by `roll`, centred on `at`.
fn flat_transform(z: Vec3, roll: f32, size: f32, at: Vec3) -> Affine3A {
    let (x, y) = rolled_basis(z, roll);
    Affine3A::from_cols((x * size).into(), (y * size).into(), z.into(), at.into())
}

/// X and Y completing a right-handed basis about the unit `z`, turned by `roll` radians.
fn rolled_basis(z: Vec3, roll: f32) -> (Vec3, Vec3) {
    let x0 = z.any_orthonormal_vector();
    let y0 = z.cross(x0);
    let (sin, cos) = roll.sin_cos();
    let x = x0 * cos + y0 * sin;
    (x, z.cross(x))
}

/// A mesh in the XY plane facing +Z, every other channel neutral.
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

/// A four-point star, tips on the axes at radius 0.5, counter-clockwise seen from +Z.
fn star_mesh() -> WorldMesh {
    flat_mesh(
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
    )
}

/// A unit square centred on the origin, counter-clockwise seen from +Z.
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

    impl Rig {
        /// Like `frame`, with the backend mesh handle each instance draws.
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

        /// The `(positions, indices)` registered for a backend mesh handle.
        fn mesh_of(&self, handle: u32) -> &(Vec<[f32; 3]>, Vec<u32>) {
            let index = usize::try_from(handle).expect("a small handle") - 1;
            &self.backend.registered[index]
        }
    }

    const AT: Vec3 = Vec3::new(1.0, 2.0, 3.0);

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

    /// Every instance's x translation, ascending.
    fn sorted_xs(drawn: &[InstanceGpu]) -> Vec<f32> {
        let mut xs: Vec<f32> = drawn.iter().map(|i| translation(i).x).collect();
        xs.sort_by(f32::total_cmp);
        xs
    }

    fn assert_xs(drawn: &[InstanceGpu], expected: &[f32]) {
        let xs = sorted_xs(drawn);
        assert_eq!(xs.len(), expected.len(), "drawn x positions {xs:?}");
        for (actual, wanted) in xs.iter().zip(expected) {
            assert!(
                (actual - wanted).abs() < EPS,
                "drawn x positions {xs:?}, expected {expected:?}"
            );
        }
    }

    fn xs_from(range: std::ops::RangeInclusive<u8>) -> Vec<f32> {
        range.map(f32::from).collect()
    }

    fn assert_ccw_from_plus_z(positions: &[[f32; 3]], indices: &[u32]) {
        assert!(!indices.is_empty() && indices.len().is_multiple_of(3));
        for triangle in indices.chunks(3) {
            let [a, b, c] = [0, 1, 2].map(|i| {
                let index = triangle[i] as usize;
                assert!(index < positions.len(), "index {index} is in range");
                Vec3::from(positions[index])
            });
            assert!(
                (b - a).cross(c - a).z > 0.0,
                "triangle {triangle:?} is counter-clockwise seen from +Z"
            );
        }
    }

    /// `rolled` is `base` turned by `roll` about `axis` (right-handed).
    fn assert_rolled(base: &InstanceGpu, rolled: &InstanceGpu, axis: Vec3, roll: f32) {
        for column in [x_axis, y_axis] {
            let from = column(base);
            let expected = from * roll.cos() + axis.cross(from) * roll.sin();
            assert_near(column(rolled), expected);
        }
    }

    fn one_drawn(rig: &mut Rig, eye: Vec3) -> InstanceGpu {
        let drawn = rig.frame(eye);
        assert_eq!(drawn.len(), 1, "exactly one instance draws");
        drawn[0]
    }

    #[test]
    fn a_flash_shrinks_linearly_and_expires() {
        let mut rig = Rig::new();
        rig.effects.flash(AT, 0.0, flash_style(0.4, ms(50)));

        let spawned = one_drawn(&mut rig, EYE);
        assert!((x_axis(&spawned).length() - 0.4).abs() < EPS);
        assert!((y_axis(&spawned).length() - 0.4).abs() < EPS);
        assert_eq!(rig.frame(EYE).len(), 1, "drawn again without being placed");
        rig.effects.advance(ms(10));
        let early = one_drawn(&mut rig, EYE);
        assert!((x_axis(&early).length() - 0.32).abs() < EPS, "linear");
        rig.effects.advance(ms(15));
        let half = one_drawn(&mut rig, EYE);
        assert!((x_axis(&half).length() - 0.2).abs() < EPS);
        assert!((y_axis(&half).length() - 0.2).abs() < EPS);
        assert_eq!(rig.effects.live(), 1);
        rig.effects.advance(ms(25));

        assert!(rig.frame(EYE).is_empty());
        assert_eq!(rig.effects.live(), 0);
    }

    #[test]
    fn a_flash_expires_when_a_frame_overshoots_its_lifetime() {
        let mut rig = Rig::new();
        rig.effects.flash(AT, 0.0, flash_style(0.4, ms(50)));
        assert_eq!(one_drawn(&mut rig, EYE).material, 0, "drawn while young");

        rig.effects.advance(ms(80));

        assert!(rig.frame(EYE).is_empty());
        assert_eq!(rig.effects.live(), 0);
    }

    #[test]
    fn a_flash_faces_the_eye() {
        let mut rig = Rig::new();
        let mut rolled_rig = Rig::new();
        let roll = 0.7;
        rig.effects.flash(AT, 0.0, flash_style(0.4, ms(50)));
        rolled_rig.effects.flash(AT, roll, flash_style(0.4, ms(50)));

        for eye in [EYE, Vec3::new(-5.0, 4.0, -2.0)] {
            let flat = one_drawn(&mut rig, eye);
            let turned = one_drawn(&mut rolled_rig, eye);

            let toward_eye = (eye - AT).normalize();
            assert_near(translation(&flat), AT);
            assert!(x_axis(&flat).dot(y_axis(&flat)).abs() < EPS, "square on");
            assert!((x_axis(&flat).length() - 0.4).abs() < EPS);
            assert_near(x_axis(&flat).cross(y_axis(&flat)).normalize(), toward_eye);
            assert_near(
                x_axis(&turned).cross(y_axis(&turned)).normalize(),
                toward_eye,
            );
            assert_rolled(&flat, &turned, toward_eye, roll);
        }
    }

    #[test]
    fn a_flash_carries_its_material() {
        let mut rig = Rig::new();
        rig.effects.flash(
            AT,
            0.0,
            FlashStyle {
                material: 6,
                ..flash_style(0.4, ms(50))
            },
        );

        assert_eq!(one_drawn(&mut rig, EYE).material, 6);
    }

    #[test]
    fn a_mark_lies_along_its_normal_offset_by_the_mark_offset() {
        let mut rig = Rig::new();
        let normal = Vec3::new(1.0, 2.0, 2.0) / 3.0;
        rig.effects.mark(
            AT,
            normal,
            0.0,
            MarkStyle {
                material: 5,
                ..mark_style(0.2, Duration::from_secs(10), Duration::from_secs(1))
            },
        );

        let drawn = one_drawn(&mut rig, EYE);

        assert_near(translation(&drawn), AT + normal * MARK_OFFSET);
        assert_near(z_axis(&drawn).normalize(), normal);
        let x0 = normal.any_orthonormal_vector();
        assert_near(x_axis(&drawn), x0 * 0.2);
        assert_near(y_axis(&drawn), normal.cross(x0) * 0.2);
        assert_eq!(drawn.material, 5);
        assert_eq!(rig.frame(EYE).len(), 1, "drawn again without being placed");
        let other_eye = one_drawn(&mut rig, Vec3::new(-7.0, 1.0, 4.0));
        assert_near(x_axis(&other_eye), x0 * 0.2);
    }

    #[test]
    fn a_mark_roll_turns_it_about_its_normal() {
        let mut flat_rig = Rig::new();
        let mut rolled_rig = Rig::new();
        let normal = Vec3::new(1.0, 2.0, 2.0) / 3.0;
        let roll = 1.1;
        let style = mark_style(0.2, Duration::from_secs(10), Duration::from_secs(1));
        flat_rig.effects.mark(AT, normal, 0.0, style);
        rolled_rig.effects.mark(AT, normal, roll, style);

        let flat = one_drawn(&mut flat_rig, EYE);
        let turned = one_drawn(&mut rolled_rig, EYE);

        assert_rolled(&flat, &turned, normal, roll);
        assert_near(translation(&turned), translation(&flat));
        assert_near(z_axis(&turned).normalize(), normal);
    }

    #[test]
    fn a_mark_holds_full_size_then_shrinks_out_in_its_fade() {
        let mut rig = Rig::new();
        let style = mark_style(0.2, Duration::from_secs(10), Duration::from_secs(1));
        rig.effects.mark(AT, Vec3::Y, 0.0, style);

        rig.effects.advance(Duration::from_secs(5));
        let middle = one_drawn(&mut rig, EYE);
        assert!((x_axis(&middle).length() - 0.2).abs() < EPS, "full at 5 s");
        rig.effects.advance(Duration::from_secs(4));
        let full = one_drawn(&mut rig, EYE);
        assert!((x_axis(&full).length() - 0.2).abs() < EPS, "full at 9 s");
        assert!((y_axis(&full).length() - 0.2).abs() < EPS);
        rig.effects.advance(ms(250));
        let three_quarters = one_drawn(&mut rig, EYE);
        assert!(
            (x_axis(&three_quarters).length() - 0.15).abs() < EPS,
            "linear at 9.25 s"
        );
        rig.effects.advance(ms(250));
        let half = one_drawn(&mut rig, EYE);
        assert!((x_axis(&half).length() - 0.1).abs() < EPS, "half at 9.5 s");
        assert!((y_axis(&half).length() - 0.1).abs() < EPS);
        assert_near(translation(&half), translation(&full));
        assert_eq!(rig.effects.live(), 1);
        rig.effects.advance(ms(500));

        assert!(rig.frame(EYE).is_empty(), "gone at 10 s");
        assert_eq!(rig.effects.live(), 0);
    }

    #[test]
    fn a_mark_fade_shrinks_in_proportion_to_its_length_whatever_that_is() {
        for (lifetime, fade, ages_and_scales) in [
            (
                Duration::from_secs(10),
                Duration::from_secs(4),
                [(6000, 1.0), (7000, 0.75), (8000, 0.5), (9000, 0.25)],
            ),
            (
                Duration::from_secs(1),
                ms(400),
                [(600, 1.0), (700, 0.75), (800, 0.5), (900, 0.25)],
            ),
        ] {
            let mut rig = Rig::new();
            rig.effects
                .mark(AT, Vec3::Y, 0.0, mark_style(0.2, lifetime, fade));
            let mut elapsed = 0;

            for (age_ms, scale) in ages_and_scales {
                rig.effects.advance(ms(age_ms - elapsed));
                elapsed = age_ms;
                let drawn = one_drawn(&mut rig, EYE);
                assert!(
                    (x_axis(&drawn).length() - 0.2 * scale).abs() < EPS,
                    "fade {fade:?} at {age_ms} ms: expected scale {scale}, size {}",
                    x_axis(&drawn).length()
                );
            }
        }
    }

    #[test]
    fn a_mark_with_no_fade_holds_full_size_until_its_lifetime_then_is_gone() {
        let mut rig = Rig::new();
        rig.effects.mark(
            AT,
            Vec3::Y,
            0.0,
            mark_style(0.2, Duration::from_secs(2), Duration::ZERO),
        );

        rig.effects.advance(ms(1999));
        let last = one_drawn(&mut rig, EYE);
        assert!((x_axis(&last).length() - 0.2).abs() < EPS);
        rig.effects.advance(ms(1));

        assert!(rig.frame(EYE).is_empty());
        assert_eq!(rig.effects.live(), 0);
    }

    #[test]
    fn a_full_cap_drops_the_oldest_fixed_lifetime_effect() {
        let mut rig = Rig::new();
        rig.effects.set_quality(1);
        let style = mark_style(0.2, Duration::from_secs(10), Duration::from_secs(1));
        for x in 0..32u8 {
            rig.effects
                .mark(Vec3::new(f32::from(x), 0.0, 0.0), Vec3::Y, 0.0, style);
        }
        assert_eq!(rig.effects.live(), 32);

        rig.effects
            .mark(Vec3::new(100.0, 0.0, 0.0), Vec3::Y, 0.0, style);

        assert_eq!(rig.effects.live(), 32);
        let mut expected = xs_from(1..=31);
        expected.push(100.0);
        assert_xs(&rig.frame(EYE), &expected);

        rig.effects.flash(
            Vec3::new(101.0, 0.0, 0.0),
            0.0,
            flash_style(0.4, Duration::from_secs(1)),
        );

        assert_eq!(rig.effects.live(), 32);
        let mut expected = xs_from(2..=31);
        expected.extend([100.0, 101.0]);
        assert_xs(&rig.frame(EYE), &expected);
    }

    #[test]
    fn a_full_cap_spares_an_older_streak_and_drops_the_oldest_flash_or_mark() {
        let mut rig = Rig::new();
        rig.effects.set_quality(1);
        let streak = rig.effects.begin_streak(style(2.0, 0.1));
        let marks = mark_style(0.2, Duration::from_secs(10), Duration::from_secs(1));
        for x in 1..=31u8 {
            rig.effects
                .mark(Vec3::new(f32::from(x), 0.0, 0.0), Vec3::Y, 0.0, marks);
        }
        assert_eq!(rig.effects.live(), 32);

        rig.effects.flash(
            Vec3::new(100.0, 0.0, 0.0),
            0.0,
            flash_style(0.4, Duration::from_secs(1)),
        );

        assert_eq!(rig.effects.live(), 32);
        rig.effects
            .place_streak(streak, Vec3::new(50.0, 0.0, 0.0), Vec3::X);
        let mut expected = xs_from(2..=31);
        expected.extend([48.0, 100.0]);
        assert_xs(&rig.frame(EYE), &expected);
    }

    #[test]
    fn a_full_cap_of_streaks_refuses_a_mark_and_keeps_every_streak() {
        let mut rig = Rig::new();
        rig.effects.set_quality(1);
        let handles: Vec<EffectHandle> = (0..32)
            .map(|_| rig.effects.begin_streak(style(2.0, 0.1)))
            .collect();
        assert_eq!(rig.effects.live(), 32);
        let far = Vec3::new(500.0, 2.0, 3.0);

        rig.effects.mark(
            far,
            Vec3::Y,
            0.0,
            mark_style(0.2, Duration::from_secs(10), Duration::from_secs(1)),
        );
        rig.effects
            .flash(far, 0.0, flash_style(0.4, Duration::from_secs(1)));

        assert_eq!(rig.effects.live(), 32, "refused, nothing evicted");
        for (i, handle) in (0..32u8).zip(&handles) {
            let head = Vec3::new(f32::from(i) * 3.0, 0.0, 0.0);
            rig.effects.place_streak(*handle, head, Vec3::X);
        }
        let drawn = rig.frame(EYE);
        let tails: Vec<f32> = (0..32u8).map(|i| f32::from(i) * 3.0 - 2.0).collect();
        assert_xs(&drawn, &tails);
        assert!(
            drawn.iter().all(|i| (translation(i) - far).length() > 1.0),
            "nothing draws at the refused spawns"
        );
    }

    fn spawn_fixed(effects: &mut Effects, flash: bool, x: f32) {
        let at = Vec3::new(x, 0.0, 0.0);
        if flash {
            effects.flash(at, 0.0, flash_style(0.4, Duration::from_secs(1)));
        } else {
            effects.mark(
                at,
                Vec3::Y,
                0.0,
                mark_style(0.2, Duration::from_secs(10), Duration::from_secs(1)),
            );
        }
    }

    #[test]
    fn a_full_cap_drops_the_oldest_effect_whatever_the_kind_of_it_and_of_the_newcomer() {
        for (oldest_is_flash, new_is_flash) in
            [(true, true), (true, false), (false, true), (false, false)]
        {
            let mut rig = Rig::new();
            rig.effects.set_quality(1);
            for x in 0..32u8 {
                spawn_fixed(&mut rig.effects, oldest_is_flash, f32::from(x));
            }
            assert_eq!(rig.effects.live(), 32);

            spawn_fixed(&mut rig.effects, new_is_flash, 100.0);

            assert_eq!(
                rig.effects.live(),
                32,
                "flash first {oldest_is_flash}, flash new {new_is_flash}"
            );
            let mut expected = xs_from(1..=31);
            expected.push(100.0);
            assert_xs(&rig.frame(EYE), &expected);
        }
    }

    #[test]
    fn a_mark_with_a_normal_that_cannot_be_normalised_is_refused_without_evicting() {
        let mut rig = Rig::new();
        rig.effects.set_quality(1);
        let style = mark_style(0.2, Duration::from_secs(10), Duration::from_secs(1));
        for normal in [
            Vec3::ZERO,
            Vec3::splat(f32::NAN),
            Vec3::new(f32::INFINITY, 0.0, 1.0),
        ] {
            rig.effects.mark(AT, normal, 0.0, style);
        }
        assert_eq!(rig.effects.live(), 0, "refused on an empty store");
        assert!(rig.frame(EYE).is_empty());
        for x in 0..32u8 {
            spawn_fixed(&mut rig.effects, false, f32::from(x));
        }

        for normal in [
            Vec3::ZERO,
            Vec3::splat(f32::NAN),
            Vec3::new(f32::INFINITY, 0.0, 1.0),
        ] {
            rig.effects
                .mark(Vec3::new(100.0, 0.0, 0.0), normal, 0.0, style);
        }

        assert_eq!(rig.effects.live(), 32);
        assert_xs(&rig.frame(EYE), &xs_from(0..=31));
    }

    #[test]
    fn a_non_unit_normal_is_normalised_and_draws_like_the_unit_one() {
        let style = mark_style(0.2, Duration::from_secs(10), Duration::from_secs(1));
        let mut unit_rig = Rig::new();
        let mut long_rig = Rig::new();
        unit_rig.effects.mark(AT, Vec3::Z, 0.4, style);
        long_rig.effects.mark(AT, Vec3::Z * 5.0, 0.4, style);

        let unit = one_drawn(&mut unit_rig, EYE);
        let long = one_drawn(&mut long_rig, EYE);

        assert_near(translation(&long), translation(&unit));
        assert_near(x_axis(&long), x_axis(&unit));
        assert_near(y_axis(&long), y_axis(&unit));
        assert_near(z_axis(&long).normalize(), Vec3::Z);
        assert!((x_axis(&long).length() - 0.2).abs() < EPS, "size unscaled");
    }

    #[test]
    fn a_spawn_that_cannot_draw_is_refused_without_evicting() {
        let mut rig = Rig::new();
        rig.effects.set_quality(1);
        let bad_points = [
            Vec3::new(f32::NAN, 0.0, 0.0),
            Vec3::new(0.0, f32::INFINITY, 0.0),
            Vec3::new(0.0, 0.0, f32::NEG_INFINITY),
        ];
        let bad_sizes = [0.0, -0.5, f32::NAN, f32::INFINITY];
        let life = Duration::from_secs(10);
        let spawn_bad = |effects: &mut Effects| {
            for at in bad_points {
                effects.flash(at, 0.0, flash_style(0.4, life));
                effects.mark(at, Vec3::Y, 0.0, mark_style(0.2, life, ms(500)));
            }
            for size in bad_sizes {
                effects.flash(Vec3::X, 0.0, flash_style(size, life));
                effects.mark(Vec3::X, Vec3::Y, 0.0, mark_style(size, life, ms(500)));
            }
        };

        spawn_bad(&mut rig.effects);
        assert_eq!(rig.effects.live(), 0, "refused on an empty store");
        assert!(rig.frame(EYE).is_empty());
        for x in 0..32u8 {
            spawn_fixed(&mut rig.effects, false, f32::from(x));
        }
        spawn_bad(&mut rig.effects);

        assert_eq!(rig.effects.live(), 32, "nothing evicted");
        assert_xs(&rig.frame(EYE), &xs_from(0..=31));
    }

    #[test]
    fn quality_zero_refuses_flash_and_mark() {
        let mut rig = Rig::new();
        rig.effects.set_quality(0);

        rig.effects
            .flash(AT, 0.0, flash_style(0.4, Duration::from_secs(1)));
        rig.effects.mark(
            AT,
            Vec3::Y,
            0.0,
            mark_style(0.2, Duration::from_secs(10), Duration::from_secs(1)),
        );

        assert_eq!(rig.effects.live(), 0);
        assert!(rig.frame(EYE).is_empty());
        assert!(rig.backend.registered.is_empty(), "nothing uploaded");
    }

    #[test]
    fn quality_zero_clears_a_live_flash_and_mark() {
        let mut rig = Rig::new();
        rig.effects
            .flash(AT, 0.0, flash_style(0.4, Duration::from_secs(1)));
        rig.effects.mark(
            AT,
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
        fn run() -> Vec<([[f32; 4]; 4], u32)> {
            let mut rig = Rig::new();
            let mut drawn = Vec::new();
            let () = rig.effects.flash(AT, 0.3, flash_style(0.4, ms(50)));
            let () = rig.effects.mark(
                Vec3::new(4.0, 0.0, 1.0),
                Vec3::Z,
                -0.9,
                mark_style(0.2, Duration::from_secs(10), Duration::from_secs(1)),
            );
            for step in [ms(0), ms(20), ms(30)] {
                rig.effects.advance(step);
                drawn.extend(rig.frame(EYE).iter().map(|i| (i.model, i.material)));
            }
            drawn
        }

        let first = run();
        let second = run();

        assert_eq!(
            first.len(),
            2 + 2 + 1,
            "flash and mark, then the mark alone"
        );
        assert_eq!(first, second);
    }

    #[test]
    fn the_flash_mesh_is_an_eight_vertex_four_point_star_in_the_xy_plane() {
        let mut rig = Rig::new();
        rig.effects.flash(AT, 0.0, flash_style(0.4, ms(50)));

        let drawn = rig.frame_with_meshes(EYE);
        assert_eq!(drawn.len(), 1);
        let (positions, indices) = rig.mesh_of(drawn[0].0);

        assert_eq!(positions.len(), 8);
        assert!(
            positions.iter().all(|p| p[2].abs() < EPS),
            "in the XY plane"
        );
        let mut radii: Vec<f32> = positions.iter().map(|p| Vec3::from(*p).length()).collect();
        radii.sort_by(f32::total_cmp);
        assert!(radii[0] > EPS, "no vertex at the centre");
        assert!(radii[3] - radii[0] < EPS, "four inner vertices");
        assert!(radii[7] - radii[4] < EPS, "four tips");
        assert!(radii[4] - radii[3] > EPS, "tips reach past the inner ones");
        assert!((radii[7] - 0.5).abs() < EPS, "size is tip to tip");
        assert_ccw_from_plus_z(positions, indices);
        let used: std::collections::BTreeSet<u32> = indices.iter().copied().collect();
        assert_eq!(used.len(), 8, "every vertex is used");
    }

    #[test]
    fn the_mark_mesh_is_a_unit_quad_centred_on_the_origin_wound_toward_plus_z() {
        let mut rig = Rig::new();
        rig.effects.mark(
            AT,
            Vec3::Y,
            0.0,
            mark_style(0.2, Duration::from_secs(10), Duration::from_secs(1)),
        );

        let drawn = rig.frame_with_meshes(EYE);
        assert_eq!(drawn.len(), 1);
        let (positions, indices) = rig.mesh_of(drawn[0].0);

        assert_eq!(positions.len(), 4);
        for corner in [[-0.5, -0.5], [0.5, -0.5], [0.5, 0.5], [-0.5, 0.5]] {
            assert!(
                positions.iter().any(|p| (p[0] - corner[0]).abs() < EPS
                    && (p[1] - corner[1]).abs() < EPS
                    && p[2].abs() < EPS),
                "corner {corner:?} in {positions:?}"
            );
        }
        assert_eq!(indices.len(), 6, "a quad is two triangles");
        assert_ccw_from_plus_z(positions, indices);
    }

    #[test]
    fn flash_and_mark_upload_their_meshes_once_each_beside_the_streak() {
        let mut rig = Rig::new();
        let streak = rig.effects.begin_streak(style(2.0, 0.1));
        rig.effects
            .flash(AT, 0.0, flash_style(0.4, Duration::from_secs(5)));
        rig.effects.mark(
            AT,
            Vec3::Y,
            0.0,
            mark_style(0.2, Duration::from_secs(10), Duration::from_secs(1)),
        );

        for _ in 0..2 {
            rig.effects.place_streak(streak, Vec3::X * 5.0, Vec3::X);
            assert_eq!(rig.frame(EYE).len(), 3);
        }

        let mut sizes: Vec<usize> = rig.backend.registered.iter().map(|m| m.0.len()).collect();
        sizes.sort_unstable();
        assert_eq!(sizes, [4, 4, 8], "kite, quad and star, each uploaded once");
        assert_eq!(rig.meshes.live_len(), 3);
    }

    #[test]
    fn flash_and_mark_draw_under_the_next_two_reserved_mesh_ids() {
        let mut rig = Rig::new();
        let streak = rig.effects.begin_streak(StreakStyle::new(2.0, 0.1, 10));
        rig.effects.flash(
            AT,
            0.0,
            FlashStyle {
                material: 11,
                ..flash_style(0.4, Duration::from_secs(5))
            },
        );
        rig.effects.mark(
            AT,
            Vec3::Y,
            0.0,
            MarkStyle {
                material: 12,
                ..mark_style(0.2, Duration::from_secs(10), Duration::from_secs(1))
            },
        );
        let materials = |rig: &mut Rig| {
            rig.effects.place_streak(streak, Vec3::X * 5.0, Vec3::X);
            let mut m: Vec<u32> = rig.frame(EYE).iter().map(|i| i.material).collect();
            m.sort_unstable();
            m
        };
        assert_eq!(materials(&mut rig), [10, 11, 12]);

        rig.meshes.remove(WorldMeshId(EFFECT_MESH_IDS_FROM + 1));
        assert_eq!(materials(&mut rig), [10, 12], "removing +1 drops the flash");
        rig.meshes.remove(WorldMeshId(EFFECT_MESH_IDS_FROM + 2));
        assert_eq!(materials(&mut rig), [10], "removing +2 drops the mark");
    }

    #[test]
    fn a_non_finite_roll_is_refused_without_evicting_and_draws_nothing() {
        let life = Duration::from_secs(10);
        let spawn_bad = |effects: &mut Effects| {
            for roll in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
                effects.flash(AT, roll, flash_style(0.4, life));
                effects.mark(AT, Vec3::Y, roll, mark_style(0.2, life, ms(500)));
            }
        };
        let mut rig = Rig::new();
        rig.effects.set_quality(1);

        spawn_bad(&mut rig.effects);
        assert_eq!(rig.effects.live(), 0, "refused on an empty store");
        assert!(rig.frame(EYE).is_empty());
        for x in 0..32u8 {
            spawn_fixed(&mut rig.effects, false, f32::from(x));
        }
        spawn_bad(&mut rig.effects);

        assert_eq!(rig.effects.live(), 32, "nothing evicted");
        assert_xs(&rig.frame(EYE), &xs_from(0..=31));
    }

    #[test]
    fn a_zero_lifetime_flash_or_mark_never_draws_and_is_gone_after_any_advance() {
        for advance in [Duration::ZERO, ms(1)] {
            let mut rig = Rig::new();
            rig.effects.flash(AT, 0.0, flash_style(0.4, Duration::ZERO));
            rig.effects.mark(
                AT,
                Vec3::Y,
                0.0,
                mark_style(0.2, Duration::ZERO, Duration::ZERO),
            );
            assert!(rig.frame(EYE).is_empty(), "never draws a frame");

            rig.effects.advance(advance);

            assert!(rig.frame(EYE).is_empty());
            assert_eq!(rig.effects.live(), 0, "gone after advance({advance:?})");
        }
    }

    fn unit_normals() -> impl Strategy<Value = Vec3> {
        prop_oneof![
            Just(Vec3::X),
            Just(Vec3::Y),
            Just(Vec3::Z),
            Just(Vec3::NEG_X),
            Just(Vec3::NEG_Y),
            Just(Vec3::NEG_Z),
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
            normal in unit_normals(),
            roll in -7.0f32..7.0,
        ) {
            let mut rig = Rig::new();
            rig.effects.mark(
                AT,
                normal,
                roll,
                mark_style(0.2, Duration::from_secs(10), Duration::from_secs(1)),
            );

            let drawn = rig.frame(EYE);

            prop_assert_eq!(drawn.len(), 1);
            let model = drawn[0].model;
            prop_assert!(model.iter().flatten().all(|v| v.is_finite()));
            let (x, y, z) = (x_axis(&drawn[0]), y_axis(&drawn[0]), z_axis(&drawn[0]));
            prop_assert!((x.length() - 0.2).abs() < 1e-3, "x spans 0.2");
            prop_assert!((y.length() - 0.2).abs() < 1e-3, "y spans 0.2");
            prop_assert!(x.dot(y).abs() < 1e-3, "x and y are square");
            prop_assert!(x.dot(normal).abs() < 1e-3, "x lies in the surface");
            prop_assert!(z.length() > 1e-3);
            prop_assert!((z.normalize() - normal).length() < 1e-3, "Z is the normal");
            prop_assert!(x.cross(y).dot(normal) > 0.0, "faces along the normal");
            prop_assert!((translation(&drawn[0]) - (AT + normal * MARK_OFFSET)).length() < 1e-3);
        }
    }
}
