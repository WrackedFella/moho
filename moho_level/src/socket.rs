//! Sockets: one model attached at another model's named node, per drawn copy.

use glam::Affine3A;

use crate::Model;

/// One drawn copy of a model with the parts attached at its named nodes, nested.
///
/// `K` is the game's key for a loaded model; every method takes a lookup from key to
/// [`Model`]. At most one part sits at a node.
#[derive(Clone, Debug, PartialEq)]
pub struct Assembly<K> {
    model: K,
    parts: Vec<(usize, Affine3A, Assembly<K>)>,
}

/// Why a socket operation was refused; the assembly is unchanged.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum SocketError {
    #[error("model has no node named `{socket}`")]
    UnknownSocket { socket: String },
}

impl<K: Copy> Assembly<K> {
    pub fn new(model: K) -> Self {
        Self {
            model,
            parts: Vec::new(),
        }
    }

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
        let _ = (models, socket, part, offset);
        todo!()
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
        let _ = (models, socket);
        todo!()
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
        let _ = (models, socket);
        todo!()
    }

    /// Every mesh to draw as `(model key, mesh index, transform)`: this model's meshes,
    /// then each part in attach order, depth-first, rooted at
    /// `root * nodes[socket].rest * offset`.
    pub fn placements<'m>(
        &self,
        models: &impl Fn(K) -> &'m Model,
        root: Affine3A,
    ) -> Vec<(K, usize, Affine3A)> {
        let _ = (models, root);
        todo!()
    }
}
