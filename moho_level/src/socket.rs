//! Sockets: one model attached at another model's named node, per drawn copy.

use glam::{Affine3A, Quat, Vec3};

use crate::Model;
use crate::anim::Pose;

/// One drawn copy of a model with the parts attached at its named nodes, nested.
///
/// `K` is the game's key for a loaded model; every method takes a lookup from key to
/// [`Model`], and must return the same model for a key for the assembly's whole life,
/// since sockets resolve to node indices when a part is attached. At most one part sits
/// at a node.
#[derive(Clone, Debug, PartialEq)]
pub struct Assembly<K> {
    model: K,
    parts: Vec<Part<K>>,
}

#[derive(Clone, Debug, PartialEq)]
struct Part<K> {
    node: usize,
    offset: Affine3A,
    assembly: Assembly<K>,
}

/// Why a socket operation was refused; the assembly is unchanged.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum SocketError {
    /// The model has no node called `socket`.
    #[error("model has no node named `{socket}`")]
    UnknownSocket {
        /// The requested node name.
        socket: String,
    },
    /// A step of the path holds no part.
    #[error("no part at `{socket}`")]
    EmptySocket {
        /// The node name that holds nothing.
        socket: String,
    },
    /// The destination lies under the part being moved.
    #[error("a part cannot be moved into itself")]
    IntoItself,
}

/// One tick's character root and pose.
#[derive(Clone, Copy, Debug)]
pub struct Posed<'a> {
    /// The character's root transform that tick.
    pub root: Affine3A,
    /// The character model's pose that tick.
    pub pose: &'a Pose,
}

/// A part taken off an assembly, with the motion it had at that moment.
#[derive(Clone, Debug, PartialEq)]
pub struct Released<K> {
    /// The part with everything attached to it.
    pub part: Assembly<K>,
    /// The part's root transform at the release tick.
    pub transform: Affine3A,
    /// World frame, metres per second.
    pub linear_velocity: Vec3,
    /// World frame, radians per second.
    pub angular_velocity: Vec3,
}

/// The rotation of `transform` with its scale decomposed away; the identity when the scale
/// has a zero axis, which leaves no rotation to read.
fn rotation_of(transform: Affine3A) -> Quat {
    let (_, rotation, _) = transform.to_scale_rotation_translation();
    if rotation.is_finite() {
        rotation
    } else {
        Quat::IDENTITY
    }
}

/// Linear and angular velocity (world frame) that carry `before` to `now` over `dt` seconds.
///
/// Angular is the rotation `r_now * r_before⁻¹` as a scaled axis, taken the short way round
/// (a quaternion and its negation are one rotation, but only one has `w >= 0`).
fn velocities(before: Affine3A, now: Affine3A, dt: f32) -> (Vec3, Vec3) {
    let linear = Vec3::from(now.translation - before.translation) / dt;
    let mut turn = (rotation_of(now) * rotation_of(before).inverse()).normalize();
    if turn.w < 0.0 {
        turn = -turn;
    }
    (linear, turn.to_scaled_axis() / dt)
}

impl<K: Copy> Assembly<K> {
    /// A copy of `model` with nothing attached.
    pub fn new(model: K) -> Self {
        Self {
            model,
            parts: Vec::new(),
        }
    }

    /// The key of the model this copy draws.
    pub fn model(&self) -> K {
        self.model
    }

    /// Put `part` at the first node called `socket`, with `offset` in that node's space,
    /// returning the part it replaces.
    ///
    /// # Errors
    /// [`SocketError::UnknownSocket`] when the model has no such node.
    pub fn attach<'m>(
        &mut self,
        models: &impl Fn(K) -> &'m Model,
        socket: &str,
        part: Assembly<K>,
        offset: Affine3A,
    ) -> Result<Option<Assembly<K>>, SocketError> {
        let node = self.node_of(models, socket)?;
        let new = Part {
            node,
            offset,
            assembly: part,
        };
        if let Some(i) = self.slot(node) {
            return Ok(Some(std::mem::replace(&mut self.parts[i], new).assembly));
        }
        self.parts.push(new);
        Ok(None)
    }

    /// Remove and return the part at `socket`; `Ok(None)` when the node holds nothing.
    ///
    /// # Errors
    /// [`SocketError::UnknownSocket`] when the model has no such node.
    pub fn detach<'m>(
        &mut self,
        models: &impl Fn(K) -> &'m Model,
        socket: &str,
    ) -> Result<Option<Assembly<K>>, SocketError> {
        let node = self.node_of(models, socket)?;
        Ok(self.slot(node).map(|i| self.parts.remove(i).assembly))
    }

    /// The part at `socket`, to attach to it in turn.
    ///
    /// # Errors
    /// [`SocketError::UnknownSocket`] when the model has no such node.
    pub fn part_mut<'m>(
        &mut self,
        models: &impl Fn(K) -> &'m Model,
        socket: &str,
    ) -> Result<Option<&mut Assembly<K>>, SocketError> {
        let node = self.node_of(models, socket)?;
        Ok(self.slot(node).map(|i| &mut self.parts[i].assembly))
    }

    /// The root transform of the part at `path`, the socket names from this assembly down
    /// (`&["hand_r", "magazine"]` is the magazine in the rifle on `hand_r`):
    /// `root * pose[n0] * offset0 * rest[n1] * offset1 ...`.
    ///
    /// Only this copy's own model is posed by `pose`; nested parts keep their rest, as in
    /// [`Assembly::posed_placements`].
    ///
    /// # Errors
    /// [`SocketError::UnknownSocket`] when a step names a node its model lacks;
    /// [`SocketError::EmptySocket`] when a step holds no part.
    ///
    /// # Panics
    /// When `path` is empty, or `pose` is not of this copy's model.
    pub fn part_transform<'m>(
        &self,
        models: &impl Fn(K) -> &'m Model,
        path: &[&str],
        root: Affine3A,
        pose: &Pose,
    ) -> Result<Affine3A, SocketError> {
        Ok(root * self.chain(models, path, Some(pose))?)
    }

    /// Move the part at path `from` to path `to`, with the offset that keeps its
    /// [`Assembly::part_transform`] unchanged at `pose`; from then on it follows its new
    /// node. Returns the part it displaces at `to`, as [`Assembly::attach`] does.
    ///
    /// Everything is checked before anything changes, so on error the assembly is untouched.
    ///
    /// # Errors
    /// [`SocketError::UnknownSocket`] or [`SocketError::EmptySocket`] along `from`, or
    /// along `to` down to its last step (which may be empty);
    /// [`SocketError::IntoItself`] when `from` is a prefix of `to`, equal paths included.
    ///
    /// # Panics
    /// When `from` or `to` is empty, or `pose` is not of this copy's model.
    pub fn move_part<'m>(
        &mut self,
        models: &impl Fn(K) -> &'m Model,
        from: &[&str],
        to: &[&str],
        pose: &Pose,
    ) -> Result<Option<Assembly<K>>, SocketError> {
        let held = self.chain(models, from, Some(pose))?;
        if to.starts_with(from) {
            return Err(SocketError::IntoItself);
        }
        let destination = self.socket_transform(models, to, pose)?;
        let offset = destination.inverse() * held;

        let part = self.take_at(models, from);
        let (socket, parents) = to.split_last().expect("`to` is not empty");
        Ok(self
            .assembly_at_mut(models, parents)
            .and_then(|parent| parent.attach(models, socket, part, offset))
            .expect("`to` was resolved above and moving `from` leaves its parents in place"))
    }

    /// Take the part at `path` off the assembly, with its transform at `now` and the
    /// velocity it has leaving: the game builds a loose object from the result.
    ///
    /// `transform` is [`Assembly::part_transform`] at `now`, with any scale dropped.
    /// Velocities are the backward difference of the part's transform over the last tick
    /// (`before`, `dt` seconds earlier): linear in metres per second, angular as the
    /// shortest-arc rotation `r_now * r_before⁻¹` taken as a scaled axis over `dt`, radians
    /// per second, both in the world frame. A part released on the first tick after a clip
    /// switch gets the jump from the old pose as velocity.
    ///
    /// Get `before` from last tick's playback state, not from a stored pose: keep a clone
    /// of the [`ClipPlayer`](crate::anim::ClipPlayer) before advancing it, then pose both.
    ///
    /// ```text
    /// let last_tick = player.clone();
    /// player.advance(model, dt);
    /// let (before, now) = (last_tick.pose(model), player.pose(model));
    /// ```
    ///
    /// # Errors
    /// [`SocketError::UnknownSocket`] or [`SocketError::EmptySocket`] along `path`; the
    /// assembly is unchanged.
    ///
    /// # Panics
    /// When `path` is empty, `dt` is not positive and finite, or a pose is not of this
    /// copy's model.
    pub fn release<'m>(
        &mut self,
        models: &impl Fn(K) -> &'m Model,
        path: &[&str],
        now: Posed<'_>,
        before: Posed<'_>,
        dt: f32,
    ) -> Result<Released<K>, SocketError> {
        assert!(
            dt.is_finite() && dt > 0.0,
            "dt must be positive and finite, got {dt}"
        );
        let at_now = self.part_transform(models, path, now.root, now.pose)?;
        let at_before = self.part_transform(models, path, before.root, before.pose)?;
        let (linear_velocity, angular_velocity) = velocities(at_before, at_now, dt);

        Ok(Released {
            part: self.take_at(models, path),
            transform: Affine3A::from_rotation_translation(
                rotation_of(at_now),
                Vec3::from(at_now.translation),
            ),
            linear_velocity,
            angular_velocity,
        })
    }

    /// Detach the part at `path`, which the caller has already resolved to a part.
    fn take_at<'m>(&mut self, models: &impl Fn(K) -> &'m Model, path: &[&str]) -> Assembly<K> {
        let (socket, parents) = path.split_last().expect("`path` is not empty");
        self.assembly_at_mut(models, parents)
            .and_then(|parent| parent.detach(models, socket))
            .expect("`path` was resolved by the caller")
            .expect("`path` holds a part, checked by the caller")
    }

    /// The part at `path` as a transform from this copy's root, `pose` (when given) posing
    /// this copy's model and every deeper model at rest.
    fn chain<'m>(
        &self,
        models: &impl Fn(K) -> &'m Model,
        path: &[&str],
        pose: Option<&Pose>,
    ) -> Result<Affine3A, SocketError> {
        let (socket, deeper) = path.split_first().expect("path is not empty");
        let node = self.node_of(models, socket)?;
        let part =
            self.slot(node)
                .map(|i| &self.parts[i])
                .ok_or_else(|| SocketError::EmptySocket {
                    socket: (*socket).to_owned(),
                })?;
        let at_node = pose.map_or(models(self.model).nodes()[node].rest, |p| p.nodes()[node]);
        let here = at_node * part.offset;
        if deeper.is_empty() {
            return Ok(here);
        }
        Ok(here * part.assembly.chain(models, deeper, None)?)
    }

    /// The node named by the last step of `path` as a transform from this copy's root,
    /// posed as in [`Assembly::chain`]; the node need not hold a part.
    fn socket_transform<'m>(
        &self,
        models: &impl Fn(K) -> &'m Model,
        path: &[&str],
        pose: &Pose,
    ) -> Result<Affine3A, SocketError> {
        let (socket, parents) = path.split_last().expect("path is not empty");
        let parent = self.assembly_at(models, parents)?;
        let node = parent.node_of(models, socket)?;
        if parents.is_empty() {
            return Ok(pose.nodes()[node]);
        }
        let above = self.chain(models, parents, Some(pose))?;
        Ok(above * models(parent.model).nodes()[node].rest)
    }

    /// The assembly of the part at `path`; this one for an empty path.
    fn assembly_at<'m>(
        &self,
        models: &impl Fn(K) -> &'m Model,
        path: &[&str],
    ) -> Result<&Assembly<K>, SocketError> {
        let mut assembly = self;
        for socket in path {
            let node = assembly.node_of(models, socket)?;
            assembly = assembly
                .slot(node)
                .map(|i| &assembly.parts[i].assembly)
                .ok_or_else(|| SocketError::EmptySocket {
                    socket: (*socket).to_owned(),
                })?;
        }
        Ok(assembly)
    }

    /// [`Assembly::assembly_at`], to change.
    fn assembly_at_mut<'m>(
        &mut self,
        models: &impl Fn(K) -> &'m Model,
        path: &[&str],
    ) -> Result<&mut Assembly<K>, SocketError> {
        let mut assembly = self;
        for socket in path {
            assembly =
                assembly
                    .part_mut(models, socket)?
                    .ok_or_else(|| SocketError::EmptySocket {
                        socket: (*socket).to_owned(),
                    })?;
        }
        Ok(assembly)
    }

    fn slot(&self, node: usize) -> Option<usize> {
        self.parts.iter().position(|p| p.node == node)
    }

    /// Every mesh to draw as `(model key, mesh index, transform)`: this model's meshes,
    /// then each part in attach order, depth-first, rooted at
    /// `root * nodes[socket].rest * offset`.
    pub fn placements<'m>(
        &self,
        models: &impl Fn(K) -> &'m Model,
        root: Affine3A,
    ) -> Vec<(K, usize, Affine3A)> {
        let mut out = Vec::new();
        self.collect(models, root, None, &mut out);
        out
    }

    /// [`Assembly::placements`] with this copy's model posed by `pose`: its meshes at
    /// `root * pose[mesh.node]` and each part rooted at `root * pose[socket] * offset`.
    /// Parts keep their own rest.
    ///
    /// # Panics
    /// When `pose` is not of this copy's model.
    pub fn posed_placements<'m>(
        &self,
        models: &impl Fn(K) -> &'m Model,
        root: Affine3A,
        pose: &Pose,
    ) -> Vec<(K, usize, Affine3A)> {
        let mut out = Vec::new();
        self.collect(models, root, Some(pose), &mut out);
        out
    }

    /// Walk this copy and its parts. `pose` poses this copy's own model (`None` is its
    /// rest); parts always sit at their own rest.
    fn collect<'m>(
        &self,
        models: &impl Fn(K) -> &'m Model,
        root: Affine3A,
        pose: Option<&Pose>,
        out: &mut Vec<(K, usize, Affine3A)>,
    ) {
        let model = models(self.model);
        let node = |index: usize| pose.map_or(model.nodes()[index].rest, |p| p.nodes()[index]);
        out.extend(
            model
                .placed(root, &node)
                .map(|(mesh, transform)| (self.model, mesh, transform)),
        );
        for part in &self.parts {
            part.assembly
                .collect(models, root * node(part.node) * part.offset, None, out);
        }
    }

    /// Index of the first node called `socket` in this copy's model.
    fn node_of<'m>(
        &self,
        models: &impl Fn(K) -> &'m Model,
        socket: &str,
    ) -> Result<usize, SocketError> {
        models(self.model)
            .nodes()
            .iter()
            .position(|n| n.name.as_deref() == Some(socket))
            .ok_or_else(|| SocketError::UnknownSocket {
                socket: socket.to_owned(),
            })
    }
}

#[cfg(test)]
mod tests {
    use glam::{Affine3A, Quat, Vec3};

    use super::velocities;

    fn near(actual: Vec3, expected: Vec3) {
        assert!(actual.abs_diff_eq(expected, 1e-3), "{actual} != {expected}");
    }

    #[test]
    fn velocity_of_a_pure_translation() {
        let before = Affine3A::from_translation(Vec3::new(1.0, 2.0, 3.0));
        let now = Affine3A::from_translation(Vec3::new(1.5, 2.0, 2.0));

        let (linear, angular) = velocities(before, now, 0.5);

        near(linear, Vec3::new(1.0, 0.0, -2.0));
        near(angular, Vec3::ZERO);
    }

    #[test]
    fn velocity_of_a_turn_about_y() {
        let tilted = Quat::from_rotation_x(0.4);
        let before = Affine3A::from_quat(tilted);
        let now = Affine3A::from_quat(Quat::from_rotation_y(0.3) * tilted);

        let (linear, angular) = velocities(before, now, 0.1);

        near(linear, Vec3::ZERO);
        near(angular, Vec3::new(0.0, 3.0, 0.0));
    }

    fn squashed(angle: f32) -> Affine3A {
        Affine3A::from_scale_rotation_translation(
            Vec3::new(2.0, 1.0, 1.0),
            Quat::from_rotation_y(angle),
            Vec3::ZERO,
        )
    }

    #[test]
    fn velocity_ignores_scale_when_the_part_does_not_move() {
        let (linear, angular) = velocities(squashed(0.3), squashed(0.3), 0.1);

        near(linear, Vec3::ZERO);
        near(angular, Vec3::ZERO);
    }

    #[test]
    fn velocity_ignores_scale_when_the_part_turns() {
        let (linear, angular) = velocities(squashed(0.3), squashed(0.4), 0.1);

        near(linear, Vec3::ZERO);
        near(angular, Vec3::new(0.0, 1.0, 0.0));
    }

    #[test]
    fn velocity_with_a_zero_scale_axis_is_finite() {
        let flat = Affine3A::from_scale_rotation_translation(
            Vec3::new(1.0, 0.0, 1.0),
            Quat::from_rotation_y(0.3),
            Vec3::ZERO,
        );

        let (linear, angular) = velocities(flat, flat, 0.1);

        assert!(
            linear.is_finite() && angular.is_finite(),
            "{linear} {angular}"
        );
    }

    #[test]
    fn angular_velocity_takes_the_shortest_arc_for_a_negated_quaternion() {
        let before = Affine3A::from_quat(Quat::from_rotation_y(-1.5));
        let now = Affine3A::from_quat(Quat::from_rotation_y(-2.0));
        let raw = Quat::from_mat3a(&now.matrix3) * Quat::from_mat3a(&before.matrix3).inverse();
        assert!(
            raw.w < 0.0,
            "the fixture must exercise the negated case: {raw}"
        );

        let (linear, angular) = velocities(before, now, 0.1);

        near(linear, Vec3::ZERO);
        near(angular, Vec3::new(0.0, -5.0, 0.0));
    }
}
