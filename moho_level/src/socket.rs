//! Sockets: one model attached at another model's named node, per drawn copy.

use glam::Affine3A;

use crate::Model;

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
        self.collect(models, root, &mut out);
        out
    }

    fn collect<'m>(
        &self,
        models: &impl Fn(K) -> &'m Model,
        root: Affine3A,
        out: &mut Vec<(K, usize, Affine3A)>,
    ) {
        let model = models(self.model);
        out.extend(
            model
                .placements(root)
                .map(|(mesh, transform)| (self.model, mesh, transform)),
        );
        for part in &self.parts {
            let socket = model.nodes()[part.node].rest;
            part.assembly
                .collect(models, root * socket * part.offset, out);
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
