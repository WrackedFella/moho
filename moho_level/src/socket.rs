//! Sockets: one model attached at another model's named node, per drawn copy.

use glam::{Affine3A, Vec3};

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
    pub root: Affine3A,
    pub pose: &'a Pose,
}

/// A part taken off an assembly, with the motion it had at that moment.
#[derive(Clone, Debug, PartialEq)]
pub struct Released<K> {
    pub part: Assembly<K>,
    /// The part's root transform at the release tick.
    pub transform: Affine3A,
    /// World frame, metres per second.
    pub linear_velocity: Vec3,
    /// World frame, radians per second.
    pub angular_velocity: Vec3,
}

/// Linear and angular velocity that carry `before` to `now` over `dt` seconds.
fn velocities(before: Affine3A, now: Affine3A, dt: f32) -> (Vec3, Vec3) {
    let _ = (before, now, dt);
    todo!("velocities")
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

    /// The part at `path` (socket names from this assembly down), as a root transform:
    /// `root * pose[n0] * offset0 * rest[n1] * offset1 ...`.
    ///
    /// # Errors
    /// [`SocketError::UnknownSocket`] or [`SocketError::EmptySocket`] along the path.
    pub fn part_transform<'m>(
        &self,
        models: &impl Fn(K) -> &'m Model,
        path: &[&str],
        root: Affine3A,
        pose: &Pose,
    ) -> Result<Affine3A, SocketError> {
        let _ = (models, path, root, pose);
        todo!("part_transform")
    }

    /// Move the part at `from` to `to`, keeping its transform at `pose`; returns the part
    /// displaced at `to`. On error the assembly is unchanged.
    ///
    /// # Errors
    /// [`SocketError`] along either path, or [`SocketError::IntoItself`].
    pub fn move_part<'m>(
        &mut self,
        models: &impl Fn(K) -> &'m Model,
        from: &[&str],
        to: &[&str],
        pose: &Pose,
    ) -> Result<Option<Assembly<K>>, SocketError> {
        let _ = (models, from, to, pose);
        todo!("move_part")
    }

    /// Remove the part at `path` with its transform and velocities at `now`.
    ///
    /// # Errors
    /// [`SocketError`] along the path; the assembly is unchanged.
    pub fn release<'m>(
        &mut self,
        models: &impl Fn(K) -> &'m Model,
        path: &[&str],
        now: Posed<'_>,
        before: Posed<'_>,
        dt: f32,
    ) -> Result<Released<K>, SocketError> {
        let _ = (
            models,
            path,
            now,
            before,
            velocities(before.root, now.root, dt),
        );
        todo!("release")
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
