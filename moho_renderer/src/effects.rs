//! Engine-drawn effects with one live store, cap and unshadowed draw. A streak is placed
//! by handle: a game starts one, places it every frame and ends it. A flash (a star
//! turned to the eye) and a mark (a quad lying on a surface) are fire-and-forget: the
//! engine draws them every frame until their fixed lifetime is up.

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

/// How far a mark floats off its surface, so it does not z-fight with it.
const MARK_OFFSET: f32 = 0.01;

/// A flash or mark's age against its fixed lifetime.
#[derive(Debug)]
struct Timed<S> {
    style: S,
    age: Duration,
}

#[derive(Debug)]
struct Flash {
    at: Vec3,
    roll: f32,
    timed: Timed<FlashStyle>,
}

#[derive(Debug)]
struct Mark {
    /// Where it is drawn: the surface point already lifted by `MARK_OFFSET`.
    centre: Vec3,
    /// Unit surface normal.
    normal: Vec3,
    roll: f32,
    timed: Timed<MarkStyle>,
}

#[derive(Debug)]
enum Effect {
    Streak(Streak),
    Flash(Flash),
    Mark(Mark),
}

/// The built-in meshes; each uploads once, at its reserved id.
#[derive(Debug, Clone, Copy, PartialEq)]
enum MeshKind {
    Kite,
    Star,
    Quad,
}

/// One instance an effect asks for this frame.
struct Draw {
    kind: MeshKind,
    transform: Affine3A,
    material: u32,
}

fn remaining(age: Duration, lifetime: Duration) -> f32 {
    1.0 - age.as_secs_f32() / lifetime.as_secs_f32()
}

impl Effect {
    fn is_fixed_lifetime(&self) -> bool {
        !matches!(self, Self::Streak(_))
    }

    fn advance(&mut self, dt: Duration) {
        match self {
            Self::Streak(streak) => {
                streak.age += dt;
                if streak.age >= streak.style.max_age {
                    streak.ended_at.get_or_insert(streak.style.max_age);
                }
            }
            Self::Flash(flash) => flash.timed.age += dt,
            Self::Mark(mark) => mark.timed.age += dt,
        }
    }

    fn finished(&self) -> bool {
        match self {
            Self::Streak(streak) => streak.finished_fading(),
            Self::Flash(flash) => flash.timed.age >= flash.timed.style.lifetime,
            Self::Mark(mark) => mark.timed.age >= mark.timed.style.lifetime,
        }
    }

    fn draw(&mut self, eye: Vec3) -> Option<Draw> {
        match self {
            Self::Streak(streak) => streak.draw(eye),
            Self::Flash(flash) => flash.draw(eye),
            Self::Mark(mark) => Some(mark.draw()),
        }
    }
}

impl Streak {
    fn draw(&mut self, eye: Vec3) -> Option<Draw> {
        let placed = std::mem::take(&mut self.placed_this_frame);
        if !placed && self.ended_at.is_none() {
            return None;
        }
        let width = self.style.width * self.width_scale();
        let transform = streak_transform(self.head, self.heading?, self.style.length, width, eye)?;
        Some(Draw {
            kind: MeshKind::Kite,
            transform,
            material: self.style.material,
        })
    }
}

impl Flash {
    /// Draws nothing while the eye is at the flash, which has no direction to face.
    fn draw(&self, eye: Vec3) -> Option<Draw> {
        let facing = (eye - self.at).try_normalize()?;
        let style = &self.timed.style;
        let size = style.size * remaining(self.timed.age, style.lifetime);
        Some(Draw {
            kind: MeshKind::Star,
            transform: facing_transform(self.at, facing, self.roll, size),
            material: style.material,
        })
    }
}

impl Mark {
    fn draw(&self) -> Draw {
        let style = &self.timed.style;
        Draw {
            kind: MeshKind::Quad,
            transform: facing_transform(
                self.centre,
                self.normal,
                self.roll,
                style.size * self.fade_scale(),
            ),
            material: style.material,
        }
    }

    /// 1 until the last `fade` of its lifetime, then falling linearly to 0.
    fn fade_scale(&self) -> f32 {
        let style = &self.timed.style;
        let fade = style.fade.min(style.lifetime);
        let left = style.lifetime.saturating_sub(self.timed.age);
        if fade.is_zero() || left >= fade {
            return 1.0;
        }
        left.as_secs_f32() / fade.as_secs_f32()
    }
}

/// The X and Y directions of the basis facing `n` (a unit vector), rolled about it;
/// X x Y = `n`.
fn rolled_basis(n: Vec3, roll: f32) -> (Vec3, Vec3) {
    let x0 = n.any_orthonormal_vector();
    let y0 = n.cross(x0);
    let (sin, cos) = roll.sin_cos();
    (x0 * cos + y0 * sin, y0 * cos - x0 * sin)
}

/// A unit-square-or-star mesh of side `size` centred at `centre` facing `n`; Z is unscaled.
fn facing_transform(centre: Vec3, n: Vec3, roll: f32, size: f32) -> Affine3A {
    let (x, y) = rolled_basis(n, roll);
    Affine3A::from_cols(
        (x * size).into(),
        (y * size).into(),
        n.into(),
        centre.into(),
    )
}

#[derive(Debug)]
pub struct Effects {
    store: BTreeMap<EffectHandle, Effect>,
    next_handle: u64,
    cap: usize,
    uploaded: Vec<MeshKind>,
}

impl Default for Effects {
    fn default() -> Self {
        Self {
            store: BTreeMap::new(),
            next_handle: 0,
            cap: CAPS[DEFAULT_QUALITY],
            uploaded: Vec::new(),
        }
    }
}

impl Effects {
    fn issue_handle(&mut self) -> EffectHandle {
        let handle = EffectHandle(self.next_handle);
        self.next_handle += 1;
        handle
    }

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

    /// Flash a star at `at` for its lifetime, turned to the eye each frame and shrinking
    /// to nothing. A malformed spawn (non-finite or non-positive size, non-finite point
    /// or roll, zero lifetime) is refused.
    pub fn flash(&mut self, at: Vec3, roll: f32, style: FlashStyle) {
        let sensible = at.is_finite()
            && roll.is_finite()
            && style.size.is_finite()
            && style.size > 0.0
            && !style.lifetime.is_zero();
        if sensible {
            self.spawn(Effect::Flash(Flash {
                at,
                roll,
                timed: Timed {
                    style,
                    age: Duration::ZERO,
                },
            }));
        }
    }

    /// Lay a quad on a surface at `at` along `normal`, full size until the last `fade`
    /// of its lifetime. Refused like a flash, and for a zero or non-finite normal.
    pub fn mark(&mut self, at: Vec3, normal: Vec3, roll: f32, style: MarkStyle) {
        let Some(normal) = normal.try_normalize() else {
            return;
        };
        let sensible = at.is_finite()
            && roll.is_finite()
            && style.size.is_finite()
            && style.size > 0.0
            && !style.lifetime.is_zero();
        if sensible {
            self.spawn(Effect::Mark(Mark {
                centre: at + normal * MARK_OFFSET,
                normal,
                roll,
                timed: Timed {
                    style,
                    age: Duration::ZERO,
                },
            }));
        }
    }

    /// Store a fixed-lifetime effect, dropping the oldest such effects to get under the
    /// cap; refused when only streaks are left to drop.
    fn spawn(&mut self, effect: Effect) {
        while self.store.len() >= self.cap {
            let oldest = self
                .store
                .iter()
                .find_map(|(handle, e)| e.is_fixed_lifetime().then_some(*handle));
            let Some(oldest) = oldest else {
                return;
            };
            self.store.remove(&oldest);
        }
        let handle = self.issue_handle();
        self.store.insert(handle, effect);
    }

    /// Age every effect by the frame time, start the fade of any streak past its
    /// `max_age` and drop the finished ones.
    pub fn advance(&mut self, dt: Duration) {
        for effect in self.store.values_mut() {
            effect.advance(dt);
        }
        self.store.retain(|_, effect| !effect.finished());
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
    /// unshadowed.
    pub(crate) fn draw(&mut self, meshes: &mut WorldMeshes, eye: Vec3) {
        for effect in self.store.values_mut() {
            let Some(draw) = effect.draw(eye) else {
                continue;
            };
            if !self.uploaded.contains(&draw.kind) {
                self.uploaded.push(draw.kind);
                meshes.upsert_movable(draw.kind.id(), draw.kind.mesh());
            }
            meshes.place_unshadowed(draw.kind.id(), draw.transform, draw.material);
        }
    }
}

impl MeshKind {
    fn id(self) -> WorldMeshId {
        let offset = match self {
            Self::Kite => 0,
            Self::Star => 1,
            Self::Quad => 2,
        };
        WorldMeshId(EFFECT_MESH_IDS_FROM + offset)
    }

    fn mesh(self) -> WorldMesh {
        match self {
            Self::Kite => flat_mesh(
                vec![
                    [0.0, 0.0, 0.0],
                    [0.8, -0.5, 0.0],
                    [1.0, 0.0, 0.0],
                    [0.8, 0.5, 0.0],
                ],
                vec![0, 1, 2, 0, 2, 3],
            ),
            // Tips and inner diagonal vertices alternate counter-clockwise.
            Self::Star => flat_mesh(
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
                vec![1, 3, 5, 1, 5, 7, 7, 0, 1, 1, 2, 3, 3, 4, 5, 5, 6, 7],
            ),
            Self::Quad => flat_mesh(
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
}

/// A mesh in the XY plane facing +Z, counter-clockwise seen from +Z.
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
    .expect("every channel has one entry per vertex and the indices are in range")
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
        /// Each `unregister_mesh` call's backend handle.
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

    const MARK_OFFSET: f32 = 0.01;
    const LOOSE: f32 = 1e-3;

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

    /// The X and Y directions of the shared basis for facing `n`, rolled by `roll`.
    fn rolled_basis(n: Vec3, roll: f32) -> (Vec3, Vec3) {
        let x0 = n.any_orthonormal_vector();
        let y0 = n.cross(x0);
        (
            x0 * roll.cos() + y0 * roll.sin(),
            y0 * roll.cos() - x0 * roll.sin(),
        )
    }

    /// Like `Rig::frame`, keeping each instance's backend mesh handle.
    fn frame_with_meshes(rig: &mut Rig, eye: Vec3) -> Vec<(u32, InstanceGpu)> {
        rig.effects.draw(&mut rig.meshes, eye);
        rig.meshes.flush(&mut rig.backend);
        let drawn = rig.meshes.unshadowed_draws().collect();
        assert_eq!(rig.meshes.draws().count(), 0);
        rig.meshes.clear_placements();
        drawn
    }

    /// Asserts `instance` is a flat square-or-star of side `size` facing `n`, right-handed.
    fn assert_basis(instance: &InstanceGpu, n: Vec3, roll: f32, size: f32) {
        let (x, y) = rolled_basis(n, roll);
        assert_near_loose(x_axis(instance), x * size);
        assert_near_loose(y_axis(instance), y * size);
        assert_near_loose(z_axis(instance), n);
    }

    fn assert_near_loose(actual: Vec3, expected: Vec3) {
        assert!(
            (actual - expected).length() < LOOSE,
            "expected {expected}, got {actual}"
        );
    }

    #[test]
    fn a_flash_shrinks_linearly_and_expires() {
        let mut rig = Rig::new();
        let at = Vec3::new(2.0, 1.0, -3.0);
        rig.effects.flash(at, 0.0, flash_style(0.4, ms(50)));

        let full = rig.frame(EYE);
        assert_eq!(full.len(), 1, "drawn before any advance");
        assert_near(translation(&full[0]), at);
        assert!((x_axis(&full[0]).length() - 0.4).abs() < EPS);
        assert!((y_axis(&full[0]).length() - 0.4).abs() < EPS);
        assert!(
            (z_axis(&full[0]).length() - 1.0).abs() < EPS,
            "Z is unscaled"
        );

        rig.effects.advance(ms(25));
        let half = rig.frame(EYE);
        assert_eq!(half.len(), 1, "drawn every frame without a place call");
        assert!((x_axis(&half[0]).length() - 0.2).abs() < EPS);
        assert!((y_axis(&half[0]).length() - 0.2).abs() < EPS);
        assert!(
            (z_axis(&half[0]).length() - 1.0).abs() < EPS,
            "Z is unscaled"
        );
        assert_near(translation(&half[0]), at);
        assert_eq!(rig.effects.live(), 1);

        rig.effects.advance(ms(25));
        assert_eq!(rig.effects.live(), 0);
        assert!(rig.frame(EYE).is_empty());
    }

    #[test]
    fn a_flash_stays_live_until_its_lifetime_is_reached() {
        let mut rig = Rig::new();
        rig.effects.flash(Vec3::ZERO, 0.0, flash_style(1.0, ms(50)));

        rig.effects.advance(ms(49));

        assert_eq!(rig.effects.live(), 1);
        let drawn = rig.frame(EYE);
        assert_eq!(drawn.len(), 1);
        assert!((x_axis(&drawn[0]).length() - 0.02).abs() < EPS);
    }

    #[test]
    fn a_live_flash_follows_the_eye_each_frame() {
        let mut rig = Rig::new();
        rig.effects.flash(Vec3::ZERO, 0.0, flash_style(0.4, ms(50)));

        let first = rig.frame(Vec3::new(5.0, 0.0, 0.0));
        let second = rig.frame(Vec3::new(0.0, 0.0, -8.0));

        assert_eq!(first.len(), 1);
        assert_eq!(second.len(), 1);
        assert_near(z_axis(&first[0]), Vec3::X);
        assert_near(z_axis(&second[0]), Vec3::NEG_Z);
    }

    #[test]
    fn a_flash_with_the_eye_at_its_point_draws_nothing_but_stays_live() {
        let mut rig = Rig::new();
        let at = Vec3::new(4.0, 5.0, 6.0);
        rig.effects.flash(at, 0.3, flash_style(0.4, ms(50)));

        assert!(rig.frame(at).is_empty());

        assert_eq!(rig.effects.live(), 1);
        assert_eq!(rig.frame(EYE).len(), 1, "draws once the eye moves off it");
    }

    /// A non-zero direction: one of the axes, a cube diagonal, or a random one.
    fn direction() -> impl Strategy<Value = Vec3> {
        let diagonal = Vec3::ONE;
        prop_oneof![
            prop::sample::select(vec![
                Vec3::X,
                Vec3::NEG_X,
                Vec3::Y,
                Vec3::NEG_Y,
                Vec3::Z,
                Vec3::NEG_Z,
                diagonal,
                -diagonal,
            ]),
            (-1.0f32..1.0, -1.0f32..1.0, -1.0f32..1.0)
                .prop_map(|(x, y, z)| Vec3::new(x, y, z))
                .prop_filter("a direction needs length", |v| v.length() > 0.05),
        ]
    }

    proptest! {
        #[test]
        fn a_flash_faces_the_eye(
            at in (-50.0f32..50.0, -50.0f32..50.0, -50.0f32..50.0),
            toward_eye in direction(),
            distance in 0.5f32..50.0,
            roll in -7.0f32..7.0,
            size in 0.05f32..3.0,
        ) {
            let at = Vec3::new(at.0, at.1, at.2);
            let eye = at + toward_eye.normalize() * distance;
            let mut rig = Rig::new();
            rig.effects.flash(at, roll, flash_style(size, ms(50)));

            let drawn = rig.frame(eye);

            prop_assert_eq!(drawn.len(), 1);
            let instance = &drawn[0];
            let n = (eye - at).normalize();
            let (x, y) = rolled_basis(n, roll);
            prop_assert!((translation(instance) - at).length() < LOOSE);
            prop_assert!((z_axis(instance) - n).length() < LOOSE, "Z points at the eye");
            prop_assert!((x_axis(instance) - x * size).length() < LOOSE);
            prop_assert!((y_axis(instance) - y * size).length() < LOOSE);
        }

        #[test]
        fn a_mark_basis_is_finite_for_any_normal(
            normal in direction(),
            at in (-50.0f32..50.0, -50.0f32..50.0, -50.0f32..50.0),
            roll in -7.0f32..7.0,
            size in 0.05f32..3.0,
        ) {
            let at = Vec3::new(at.0, at.1, at.2);
            let mut rig = Rig::new();
            rig.effects.mark(at, normal, roll, mark_style(size, ms(500), ms(100)));

            let drawn = rig.frame(EYE);

            prop_assert_eq!(drawn.len(), 1);
            let instance = &drawn[0];
            let n = normal.normalize();
            prop_assert!((translation(instance) - (at + n * MARK_OFFSET)).length() < LOOSE);
            prop_assert!((z_axis(instance) - n).length() < LOOSE);
            let (x, y) = rolled_basis(n, roll);
            prop_assert!((x_axis(instance) - x * size).length() < LOOSE);
            prop_assert!((y_axis(instance) - y * size).length() < LOOSE);
        }
    }

    #[test]
    fn a_mark_lies_along_its_normal_offset_by_the_mark_offset() {
        let mut rig = Rig::new();
        let at = Vec3::new(1.0, 2.0, 3.0);
        let style = MarkStyle {
            material: 4,
            ..mark_style(0.2, Duration::from_secs(10), Duration::from_secs(1))
        };
        rig.effects.mark(at, Vec3::new(0.0, 0.0, 5.0), 0.0, style);

        let drawn = rig.frame(EYE);

        assert_eq!(drawn.len(), 1);
        assert_near(translation(&drawn[0]), at + Vec3::Z * 0.01);
        assert_near(z_axis(&drawn[0]), Vec3::Z);
        assert!((x_axis(&drawn[0]).length() - 0.2).abs() < EPS);
        assert!((y_axis(&drawn[0]).length() - 0.2).abs() < EPS);
        assert!(
            x_axis(&drawn[0]).dot(Vec3::Z).abs() < EPS,
            "X lies in the surface"
        );
        assert!(
            y_axis(&drawn[0]).dot(Vec3::Z).abs() < EPS,
            "Y lies in the surface"
        );
        assert!(x_axis(&drawn[0]).dot(y_axis(&drawn[0])).abs() < EPS);
        assert!(
            x_axis(&drawn[0]).cross(y_axis(&drawn[0])).dot(Vec3::Z) > 0.0,
            "wound to face along the normal"
        );
        assert_eq!(drawn[0].material, 4);
    }

    #[test]
    fn a_mark_is_offset_along_a_slanted_normal_and_ignores_the_eye() {
        let mut rig = Rig::new();
        let at = Vec3::new(-2.0, 0.5, 1.0);
        let normal = Vec3::new(1.0, 2.0, -2.0);
        rig.effects
            .mark(at, normal, 0.7, mark_style(0.3, ms(500), ms(100)));

        let near = rig.frame(Vec3::new(0.0, 0.0, 0.0));
        let far = rig.frame(Vec3::new(-40.0, 30.0, 12.0));

        assert_eq!(near.len(), 1);
        assert_eq!(far.len(), 1);
        let n = normal.normalize();
        assert_near(translation(&near[0]), at + n * 0.01);
        assert_basis(&near[0], n, 0.7, 0.3);
        assert_eq!(
            near[0].model, far[0].model,
            "a mark does not turn to the eye"
        );
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
        let first = rig.frame(EYE);
        assert_eq!(first.len(), 1);
        assert!(
            (x_axis(&first[0]).length() - 0.2).abs() < EPS,
            "full on spawn"
        );

        rig.effects.advance(Duration::from_secs(9));
        let full = rig.frame(EYE);
        assert_eq!(full.len(), 1);
        assert!((x_axis(&full[0]).length() - 0.2).abs() < EPS);
        assert!((y_axis(&full[0]).length() - 0.2).abs() < EPS);

        rig.effects.advance(ms(500));
        let half = rig.frame(EYE);
        assert_eq!(half.len(), 1, "drawn every frame without a place call");
        assert!((x_axis(&half[0]).length() - 0.1).abs() < EPS);
        assert!((y_axis(&half[0]).length() - 0.1).abs() < EPS);
        assert_eq!(rig.effects.live(), 1);

        rig.effects.advance(ms(500));
        assert_eq!(rig.effects.live(), 0);
        assert!(rig.frame(EYE).is_empty());
    }

    #[test]
    fn a_mark_with_a_zero_fade_is_full_size_until_it_is_removed() {
        let mut rig = Rig::new();
        rig.effects.mark(
            Vec3::ZERO,
            Vec3::Y,
            0.0,
            mark_style(0.2, Duration::from_secs(10), Duration::ZERO),
        );

        rig.effects.advance(ms(9_999));
        let drawn = rig.frame(EYE);

        assert_eq!(drawn.len(), 1);
        assert!((x_axis(&drawn[0]).length() - 0.2).abs() < EPS);
        rig.effects.advance(ms(1));
        assert_eq!(rig.effects.live(), 0);
        assert!(rig.frame(EYE).is_empty());
    }

    #[test]
    fn a_mark_fade_longer_than_its_lifetime_acts_as_the_whole_lifetime() {
        let mut rig = Rig::new();
        rig.effects.mark(
            Vec3::ZERO,
            Vec3::Y,
            0.0,
            mark_style(0.2, Duration::from_secs(10), Duration::from_secs(30)),
        );
        let start = rig.frame(EYE);
        assert_eq!(start.len(), 1);
        assert!(
            (x_axis(&start[0]).length() - 0.2).abs() < EPS,
            "full at age 0"
        );

        rig.effects.advance(Duration::from_secs(5));
        let half = rig.frame(EYE);
        assert_eq!(half.len(), 1);
        assert!((x_axis(&half[0]).length() - 0.1).abs() < EPS);

        rig.effects.advance(Duration::from_secs(5));
        assert_eq!(rig.effects.live(), 0);
    }

    fn spawn_marks(rig: &mut Rig, count: u16) {
        for i in 0..count {
            rig.effects.mark(
                Vec3::new(f32::from(i), 0.0, 0.0),
                Vec3::Z,
                0.0,
                mark_style(0.2, Duration::from_secs(60), Duration::from_secs(1)),
            );
        }
    }

    fn drawn_xs(rig: &mut Rig) -> Vec<f32> {
        rig.frame(EYE)
            .iter()
            .map(|instance| translation(instance).x.round())
            .collect()
    }

    #[test]
    fn a_full_cap_drops_the_oldest_fixed_lifetime_effect() {
        let mut rig = Rig::new();
        rig.effects.set_quality(1);
        spawn_marks(&mut rig, 32);
        assert_eq!(rig.effects.live(), 32);

        rig.effects.mark(
            Vec3::new(100.0, 0.0, 0.0),
            Vec3::Z,
            0.0,
            mark_style(0.2, Duration::from_secs(60), Duration::from_secs(1)),
        );

        assert_eq!(rig.effects.live(), 32);
        let xs = drawn_xs(&mut rig);
        let mut expected: Vec<f32> = (1u16..32).map(f32::from).collect();
        expected.push(100.0);
        assert_eq!(xs, expected, "the oldest mark is gone, the newest is drawn");
    }

    #[test]
    fn a_full_cap_drops_the_oldest_across_flashes_and_marks() {
        let mut rig = Rig::new();
        rig.effects.set_quality(1);
        let flash = FlashStyle {
            material: 5,
            ..flash_style(0.4, Duration::from_secs(60))
        };
        rig.effects.flash(Vec3::new(-9.0, 0.0, 0.0), 0.0, flash);
        spawn_marks(&mut rig, 31);
        assert_eq!(rig.effects.live(), 32);

        rig.effects.mark(
            Vec3::new(100.0, 0.0, 0.0),
            Vec3::Z,
            0.0,
            mark_style(0.2, Duration::from_secs(60), Duration::from_secs(1)),
        );

        assert_eq!(rig.effects.live(), 32);
        let drawn = rig.frame(EYE);
        assert!(
            drawn.iter().all(|i| i.material != 5),
            "the oldest was the flash"
        );
        assert_eq!(drawn.len(), 32);
    }

    #[test]
    fn a_flash_at_a_full_cap_drops_the_oldest_mark() {
        let mut rig = Rig::new();
        rig.effects.set_quality(1);
        spawn_marks(&mut rig, 32);

        rig.effects.flash(
            Vec3::new(100.0, 0.0, 0.0),
            0.0,
            FlashStyle {
                material: 5,
                ..flash_style(0.4, Duration::from_secs(60))
            },
        );

        assert_eq!(rig.effects.live(), 32);
        let drawn = rig.frame(EYE);
        assert_eq!(drawn.iter().filter(|i| i.material == 5).count(), 1);
        assert_eq!(drawn.len(), 32);
        assert!(
            drawn
                .iter()
                .filter(|i| i.material != 5)
                .all(|i| translation(i).x.round() >= 1.0),
            "the mark at x = 0 was the oldest"
        );
    }

    #[test]
    fn lowering_quality_then_spawning_evicts_until_under_the_new_cap() {
        let mut rig = Rig::new();
        spawn_marks(&mut rig, 40);
        rig.effects.set_quality(1);
        assert_eq!(rig.effects.live(), 40, "lowering keeps what is live");

        rig.effects.mark(
            Vec3::new(100.0, 0.0, 0.0),
            Vec3::Z,
            0.0,
            mark_style(0.2, Duration::from_secs(60), Duration::from_secs(1)),
        );

        assert_eq!(
            rig.effects.live(),
            32,
            "31 left under the cap, plus the new one"
        );
        let xs = drawn_xs(&mut rig);
        let mut expected: Vec<f32> = (9u16..40).map(f32::from).collect();
        expected.push(100.0);
        assert_eq!(xs, expected, "the nine oldest marks were dropped");
    }

    #[test]
    fn a_full_cap_never_evicts_a_streak_to_admit_a_mark() {
        let mut rig = Rig::new();
        rig.effects.set_quality(1);
        let streak = rig.effects.begin_streak(style(2.0, 0.1));
        spawn_marks(&mut rig, 31);
        assert_eq!(rig.effects.live(), 32);

        rig.effects.mark(
            Vec3::new(100.0, 0.0, 0.0),
            Vec3::Z,
            0.0,
            mark_style(0.2, Duration::from_secs(60), Duration::from_secs(1)),
        );

        assert_eq!(rig.effects.live(), 32);
        rig.effects
            .place_streak(streak, Vec3::new(-20.0, 0.0, 0.0), Vec3::X);
        let xs = drawn_xs(&mut rig);
        assert_eq!(xs.len(), 32);
        assert!(xs.contains(&-22.0), "the streak (tail at x = -22) survived");
        assert!(!xs.contains(&0.0), "the oldest mark went instead");
        assert!(xs.contains(&100.0));
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
            Vec3::new(100.0, 0.0, 0.0),
            Vec3::Z,
            0.0,
            MarkStyle {
                material: 9,
                ..mark_style(0.2, Duration::from_secs(60), Duration::from_secs(1))
            },
        );
        rig.effects.flash(
            Vec3::new(100.0, 0.0, 0.0),
            0.0,
            FlashStyle {
                material: 9,
                ..flash_style(0.4, Duration::from_secs(60))
            },
        );

        assert_eq!(rig.effects.live(), 32, "the mark and flash were refused");
        for handle in &handles {
            rig.effects.place_streak(*handle, Vec3::X, Vec3::X);
        }
        let drawn = rig.frame(EYE);
        assert_eq!(drawn.len(), 32, "every streak stays and draws");
        assert!(
            drawn.iter().all(|i| i.material == 0),
            "no mark or flash drew"
        );
    }

    #[test]
    fn a_freed_slot_admits_a_mark_after_a_full_cap_of_streaks() {
        let mut rig = Rig::new();
        rig.effects.set_quality(1);
        let handles: Vec<EffectHandle> = (0..32)
            .map(|_| rig.effects.begin_streak(style(2.0, 0.1)))
            .collect();
        rig.effects.end(handles[0]);
        rig.effects.advance(ms(50));
        assert_eq!(rig.effects.live(), 31);

        rig.effects.mark(
            Vec3::new(100.0, 0.0, 0.0),
            Vec3::Z,
            0.0,
            MarkStyle {
                material: 9,
                ..mark_style(0.2, Duration::from_secs(60), Duration::from_secs(1))
            },
        );

        assert_eq!(rig.effects.live(), 32, "a freed slot admits the mark");
        assert!(rig.frame(EYE).iter().any(|i| i.material == 9));
    }

    #[test]
    fn begin_streak_at_a_cap_full_of_marks_is_inert_and_evicts_nothing() {
        let mut rig = Rig::new();
        rig.effects.set_quality(1);
        spawn_marks(&mut rig, 32);

        let streak = rig.effects.begin_streak(StreakStyle::new(2.0, 0.1, 9));
        rig.effects.place_streak(streak, Vec3::X, Vec3::X);

        assert_eq!(rig.effects.live(), 32);
        let drawn = rig.frame(EYE);
        assert_eq!(drawn.len(), 32, "every mark still draws, no streak does");
        assert!(drawn.iter().all(|i| i.material == 0));
        let xs: Vec<f32> = drawn.iter().map(|i| translation(i).x.round()).collect();
        let expected: Vec<f32> = (0u16..32).map(f32::from).collect();
        assert_eq!(xs, expected, "no mark was evicted");
    }

    #[test]
    fn quality_zero_clears_live_flashes_and_marks() {
        let mut rig = Rig::new();
        rig.effects.flash(Vec3::X, 0.0, flash_style(0.4, ms(50)));
        rig.effects
            .mark(Vec3::X, Vec3::Y, 0.0, mark_style(0.2, ms(500), ms(100)));
        assert_eq!(rig.effects.live(), 2, "both are live before the change");
        assert_eq!(rig.frame(EYE).len(), 2);

        rig.effects.set_quality(0);

        assert_eq!(rig.effects.live(), 0);
        assert!(rig.frame(EYE).is_empty());
    }

    #[test]
    fn quality_zero_refuses_flash_and_mark() {
        let mut rig = Rig::new();
        rig.effects.set_quality(0);

        rig.effects.flash(Vec3::X, 0.0, flash_style(0.4, ms(50)));
        rig.effects
            .mark(Vec3::X, Vec3::Y, 0.0, mark_style(0.2, ms(500), ms(100)));

        assert_eq!(rig.effects.live(), 0);
        assert!(rig.frame(EYE).is_empty());

        rig.effects.set_quality(1);
        rig.effects.flash(Vec3::X, 0.0, flash_style(0.4, ms(50)));
        rig.effects
            .mark(Vec3::X, Vec3::Y, 0.0, mark_style(0.2, ms(500), ms(100)));
        assert_eq!(
            rig.effects.live(),
            2,
            "the same spawns work once effects are on"
        );
        assert_eq!(rig.frame(EYE).len(), 2);
    }

    #[test]
    fn spawns_that_make_no_sense_are_refused_without_drawing() {
        type Spawn = Box<dyn Fn(&mut Effects)>;
        let good_flash = flash_style(0.4, ms(50));
        let good_mark = mark_style(0.2, ms(500), ms(100));
        let nan = f32::NAN;
        let inf = f32::INFINITY;
        let bad_at = Vec3::new(nan, 0.0, 0.0);
        let bad_normal = Vec3::new(0.0, inf, 0.0);
        let mut cases: Vec<(String, Spawn)> = vec![
            (
                "flash at a NaN point".into(),
                Box::new(move |e| e.flash(bad_at, 0.0, good_flash)),
            ),
            (
                "flash with a NaN roll".into(),
                Box::new(move |e| e.flash(Vec3::X, nan, good_flash)),
            ),
            (
                "flash with an infinite roll".into(),
                Box::new(move |e| e.flash(Vec3::X, inf, good_flash)),
            ),
            (
                "flash with a zero lifetime".into(),
                Box::new(move |e| {
                    let zero = FlashStyle {
                        lifetime: Duration::ZERO,
                        ..good_flash
                    };
                    e.flash(Vec3::X, 0.0, zero);
                }),
            ),
            (
                "mark at a NaN point".into(),
                Box::new(move |e| e.mark(bad_at, Vec3::Y, 0.0, good_mark)),
            ),
            (
                "mark with an infinite normal".into(),
                Box::new(move |e| e.mark(Vec3::X, bad_normal, 0.0, good_mark)),
            ),
            (
                "mark with a NaN normal".into(),
                Box::new(move |e| e.mark(Vec3::X, Vec3::splat(nan), 0.0, good_mark)),
            ),
            (
                "mark with a zero normal".into(),
                Box::new(move |e| e.mark(Vec3::X, Vec3::ZERO, 0.0, good_mark)),
            ),
            (
                "mark with a NaN roll".into(),
                Box::new(move |e| e.mark(Vec3::X, Vec3::Y, nan, good_mark)),
            ),
            (
                "mark with an infinite roll".into(),
                Box::new(move |e| e.mark(Vec3::X, Vec3::Y, inf, good_mark)),
            ),
            (
                "mark with a zero lifetime".into(),
                Box::new(move |e| {
                    let zero = MarkStyle {
                        lifetime: Duration::ZERO,
                        ..good_mark
                    };
                    e.mark(Vec3::X, Vec3::Y, 0.0, zero);
                }),
            ),
        ];
        for size in [0.0, -0.4, nan, inf] {
            cases.push((
                format!("flash of size {size}"),
                Box::new(move |e| e.flash(Vec3::X, 0.0, FlashStyle { size, ..good_flash })),
            ));
            cases.push((
                format!("mark of size {size}"),
                Box::new(move |e| e.mark(Vec3::X, Vec3::Y, 0.0, MarkStyle { size, ..good_mark })),
            ));
        }

        for (label, spawn) in &cases {
            let mut rig = Rig::new();

            spawn(&mut rig.effects);

            assert_eq!(rig.effects.live(), 0, "{label}");
            assert!(rig.frame(EYE).is_empty(), "{label}");
        }
        let mut control = Rig::new();
        control.effects.flash(Vec3::X, 0.0, good_flash);
        control.effects.mark(Vec3::X, Vec3::Y, 0.0, good_mark);
        assert_eq!(
            control.effects.live(),
            2,
            "the well-formed spawns are accepted"
        );
        assert_eq!(control.frame(EYE).len(), 2);
    }

    #[test]
    fn a_refused_spawn_does_not_evict_anything() {
        let mut rig = Rig::new();
        rig.effects.set_quality(1);
        spawn_marks(&mut rig, 32);

        rig.effects.mark(
            Vec3::X,
            Vec3::ZERO,
            0.0,
            mark_style(0.2, Duration::from_secs(60), Duration::from_secs(1)),
        );
        rig.effects
            .flash(Vec3::X, 0.0, flash_style(0.4, Duration::ZERO));

        assert_eq!(rig.effects.live(), 32);
        assert_eq!(rig.frame(EYE).len(), 32);
    }

    #[test]
    fn the_same_spawns_draw_the_same_instances() {
        let run = || {
            let mut rig = Rig::new();
            let mut drawn = Vec::new();
            rig.effects
                .flash(Vec3::new(1.0, 2.0, 3.0), 0.9, flash_style(0.4, ms(50)));
            rig.effects.mark(
                Vec3::new(-4.0, 0.0, 2.0),
                Vec3::new(0.0, 1.0, 1.0),
                -1.3,
                mark_style(0.2, ms(500), ms(100)),
            );
            for _ in 0..6 {
                drawn.push(
                    rig.frame(EYE)
                        .iter()
                        .map(|i| (i.model, i.material))
                        .collect::<Vec<_>>(),
                );
                rig.effects.advance(ms(30));
            }
            drawn
        };

        let first = run();

        assert_eq!(first, run());
        assert_eq!(first[0].len(), 2, "both drew at the start");
        assert_ne!(first[0], first[1], "and they change as time passes");
    }

    #[test]
    fn the_star_and_quad_are_registered_once_each_with_their_shape_and_winding() {
        let mut rig = Rig::new();
        rig.effects
            .flash(Vec3::X, 0.0, flash_style(0.4, Duration::from_secs(60)));
        rig.effects.mark(
            Vec3::Y,
            Vec3::Z,
            0.0,
            mark_style(0.2, Duration::from_secs(60), Duration::from_secs(1)),
        );

        for _ in 0..3 {
            rig.frame(EYE);
        }
        rig.effects
            .flash(Vec3::Z, 0.0, flash_style(0.4, Duration::from_secs(60)));
        rig.effects.mark(
            Vec3::Z,
            Vec3::Z,
            0.0,
            mark_style(0.2, Duration::from_secs(60), Duration::from_secs(1)),
        );
        rig.frame(EYE);

        assert_eq!(
            rig.backend.registered.len(),
            2,
            "one star and one quad, ever"
        );
        let star = rig
            .backend
            .registered
            .iter()
            .find(|(positions, _)| positions.len() == 8)
            .expect("an eight-vertex star");
        let quad = rig
            .backend
            .registered
            .iter()
            .find(|(positions, _)| positions.len() == 4)
            .expect("a four-vertex quad");
        for tip in [
            [0.5, 0.0, 0.0],
            [-0.5, 0.0, 0.0],
            [0.0, 0.5, 0.0],
            [0.0, -0.5, 0.0],
        ] {
            assert!(star.0.contains(&tip), "star tip {tip:?}");
        }
        for inner in [
            [0.1, 0.1, 0.0],
            [-0.1, 0.1, 0.0],
            [0.1, -0.1, 0.0],
            [-0.1, -0.1, 0.0],
        ] {
            assert!(star.0.contains(&inner), "star inner vertex {inner:?}");
        }
        for corner in [
            [0.5, 0.5, 0.0],
            [-0.5, 0.5, 0.0],
            [0.5, -0.5, 0.0],
            [-0.5, -0.5, 0.0],
        ] {
            assert!(quad.0.contains(&corner), "quad corner {corner:?}");
        }
        assert_eq!(star.1.len(), 18, "a star is six triangles");
        assert_eq!(quad.1.len(), 6, "a quad is two triangles");
        for (positions, indices) in [star, quad] {
            for triangle in indices.chunks(3) {
                let [a, b, c] = [0, 1, 2].map(|i| Vec3::from(positions[triangle[i] as usize]));
                assert!(
                    (b - a).cross(c - a).z > 0.0,
                    "triangle {triangle:?} is counter-clockwise seen from +Z"
                );
            }
        }
    }

    #[test]
    fn a_flash_draws_with_the_star_at_the_first_id_after_the_streak_mesh() {
        let mut rig = Rig::new();
        rig.effects
            .flash(Vec3::X, 0.0, flash_style(0.4, Duration::from_secs(60)));

        let drawn = frame_with_meshes(&mut rig, EYE);

        assert_eq!(drawn.len(), 1);
        rig.meshes.remove(WorldMeshId(EFFECT_MESH_IDS_FROM + 1));
        rig.meshes.flush(&mut rig.backend);
        assert_eq!(
            rig.backend.unregistered,
            vec![drawn[0].0],
            "the drawn mesh is the one under EFFECT_MESH_IDS_FROM + 1"
        );
    }

    #[test]
    fn a_mark_draws_with_the_quad_at_the_second_id_after_the_streak_mesh() {
        let mut rig = Rig::new();
        rig.effects.mark(
            Vec3::Y,
            Vec3::Z,
            0.0,
            mark_style(0.2, Duration::from_secs(60), Duration::from_secs(1)),
        );

        let drawn = frame_with_meshes(&mut rig, EYE);

        assert_eq!(drawn.len(), 1);
        rig.meshes.remove(WorldMeshId(EFFECT_MESH_IDS_FROM + 2));
        rig.meshes.flush(&mut rig.backend);
        assert_eq!(
            rig.backend.unregistered,
            vec![drawn[0].0],
            "the drawn mesh is the one under EFFECT_MESH_IDS_FROM + 2"
        );
    }

    #[test]
    fn each_mesh_uploads_only_when_its_kind_first_draws() {
        let mut rig = Rig::new();
        let streak = rig.effects.begin_streak(style(2.0, 0.1));
        rig.effects.place_streak(streak, Vec3::X, Vec3::X);
        rig.frame(EYE);
        assert_eq!(rig.backend.registered.len(), 1, "only the streak's kite");

        rig.effects
            .flash(Vec3::X, 0.0, flash_style(0.4, Duration::from_secs(60)));
        rig.frame(EYE);
        assert_eq!(rig.backend.registered.len(), 2, "the star joins it");

        rig.effects.mark(
            Vec3::Y,
            Vec3::Z,
            0.0,
            mark_style(0.2, Duration::from_secs(60), Duration::from_secs(1)),
        );
        rig.frame(EYE);
        assert_eq!(rig.backend.registered.len(), 3, "then the quad");
    }
}
