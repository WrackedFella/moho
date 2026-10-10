//! Position-keyed storage for meshed voxel chunks.

use super::chunk::VoxelChunk;
use glam::IVec3;
use std::collections::BTreeMap;

/// Holds at most one [`VoxelChunk`] per chunk position.
///
/// Iteration order is ascending by `(x, y, z)` regardless of insertion order.
#[derive(Debug, Default)]
pub struct ChunkStore {
    chunks: BTreeMap<[i32; 3], VoxelChunk>,
}

impl ChunkStore {
    /// Creates an empty store.
    pub fn new() -> Self {
        Self::default()
    }

    /// Stores `chunk` at its own `chunk_pos()`, returning the chunk it replaced, if any.
    pub fn insert(&mut self, chunk: VoxelChunk) -> Option<VoxelChunk> {
        self.chunks.insert(chunk.chunk_pos().to_array(), chunk)
    }

    /// Removes and returns the chunk at `pos`, or `None` if there is none.
    pub fn remove(&mut self, pos: IVec3) -> Option<VoxelChunk> {
        self.chunks.remove(&pos.to_array())
    }

    /// Returns the chunk at `pos`, if any.
    pub fn get(&self, pos: IVec3) -> Option<&VoxelChunk> {
        self.chunks.get(&pos.to_array())
    }

    /// Returns the chunk at `pos` mutably, if any.
    ///
    /// Replacing the chunk through this reference with one at another position
    /// desynchronises it from its key; use [`ChunkStore::insert`] instead.
    pub fn get_mut(&mut self, pos: IVec3) -> Option<&mut VoxelChunk> {
        self.chunks.get_mut(&pos.to_array())
    }

    /// Iterates chunks in strictly ascending `(x, y, z)` position order.
    pub fn iter(&self) -> impl Iterator<Item = &VoxelChunk> {
        self.chunks.values()
    }

    /// Iterates chunks mutably in strictly ascending `(x, y, z)` position order.
    ///
    /// Replacing a chunk through this iterator with one at another position
    /// desynchronises it from its key; use [`ChunkStore::insert`] instead.
    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut VoxelChunk> {
        self.chunks.values_mut()
    }

    /// Number of stored chunks.
    pub fn len(&self) -> usize {
        self.chunks.len()
    }

    /// True when no chunks are stored.
    pub fn is_empty(&self) -> bool {
        self.chunks.is_empty()
    }

    /// Removes every chunk.
    pub fn clear(&mut self) {
        self.chunks.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use std::collections::BTreeSet;

    fn chunk_with_material(pos: IVec3, material_id: u32) -> VoxelChunk {
        VoxelChunk::new(
            pos,
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            material_id,
        )
    }

    #[test]
    fn insert_at_occupied_position_replaces_and_returns_previous() {
        let pos = IVec3::new(1, 0, 2);
        let mut store = ChunkStore::new();
        store.insert(chunk_with_material(pos, 7));

        let previous = store.insert(chunk_with_material(pos, 9));

        assert_eq!(previous.map(|c| c.material_id()), Some(7));
        assert_eq!(store.len(), 1);
        assert_eq!(store.get(pos).map(VoxelChunk::material_id), Some(9));
    }

    #[test]
    fn insert_at_new_position_returns_none() {
        let mut store = ChunkStore::new();
        store.insert(chunk_with_material(IVec3::new(1, 0, 2), 7));

        let previous = store.insert(chunk_with_material(IVec3::new(2, 0, 1), 9));

        assert!(previous.is_none());
        assert_eq!(store.len(), 2);
        assert_eq!(
            store.get(IVec3::new(1, 0, 2)).map(VoxelChunk::material_id),
            Some(7)
        );
    }

    #[test]
    fn get_mut_changes_are_visible_through_get() {
        let mut store = ChunkStore::new();
        store.insert(chunk_with_material(IVec3::ZERO, 5));
        store.insert(chunk_with_material(IVec3::X, 6));

        *store.get_mut(IVec3::X).expect("chunk inserted at X") = chunk_with_material(IVec3::X, 42);

        assert_eq!(store.get(IVec3::X).map(VoxelChunk::material_id), Some(42));
        assert_eq!(store.get(IVec3::ZERO).map(VoxelChunk::material_id), Some(5));
    }

    #[test]
    fn get_mut_missing_position_returns_none() {
        let mut store = ChunkStore::new();
        store.insert(chunk_with_material(IVec3::ZERO, 5));

        assert!(store.get_mut(IVec3::new(5, 5, 5)).is_none());
    }

    #[test]
    fn remove_returns_chunk_and_position_becomes_empty() {
        let mut store = ChunkStore::new();
        store.insert(chunk_with_material(IVec3::ZERO, 5));
        store.insert(chunk_with_material(IVec3::X, 6));

        let removed = store.remove(IVec3::ZERO);

        assert_eq!(removed.map(|c| c.material_id()), Some(5));
        assert!(store.get(IVec3::ZERO).is_none());
        assert_eq!(store.len(), 1);
        assert_eq!(store.get(IVec3::X).map(VoxelChunk::material_id), Some(6));
    }

    #[test]
    fn remove_missing_position_returns_none() {
        let mut store = ChunkStore::new();
        store.insert(chunk_with_material(IVec3::ZERO, 5));

        let removed = store.remove(IVec3::new(3, 3, 3));

        assert!(removed.is_none());
        assert_eq!(store.len(), 1);
    }

    #[test]
    fn clear_removes_every_chunk() {
        let mut store = ChunkStore::new();
        store.insert(chunk_with_material(IVec3::ZERO, 1));
        store.insert(chunk_with_material(IVec3::X, 2));

        store.clear();

        assert_eq!(store.len(), 0);
        assert!(store.is_empty());
        assert!(store.get(IVec3::ZERO).is_none());
        assert!(store.get(IVec3::X).is_none());
        assert!(store.iter().next().is_none());
    }

    #[test]
    fn is_empty_is_true_only_without_chunks() {
        let mut store = ChunkStore::new();
        assert!(store.is_empty());

        store.insert(chunk_with_material(IVec3::ZERO, 1));
        assert!(!store.is_empty());

        store.remove(IVec3::ZERO);
        assert!(store.is_empty());
    }

    proptest! {
        #[test]
        fn iteration_is_unique_and_ascending_for_any_insert_order(
            positions in prop::collection::vec((-3i32..=3, -3i32..=3, -3i32..=3), 0..64)
        ) {
            let mut store = ChunkStore::new();
            for &(x, y, z) in &positions {
                store.insert(VoxelChunk::empty(IVec3::new(x, y, z)));
            }

            let iterated: Vec<(i32, i32, i32)> = store
                .iter()
                .map(|c| {
                    let p = c.chunk_pos();
                    (p.x, p.y, p.z)
                })
                .collect();

            let expected: Vec<(i32, i32, i32)> = positions
                .iter()
                .copied()
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect();
            prop_assert_eq!(&iterated, &expected);
            prop_assert_eq!(store.len(), expected.len());
        }
    }

    #[test]
    fn iter_mut_changes_are_visible_through_get() {
        let mut store = ChunkStore::new();
        store.insert(chunk_with_material(IVec3::new(0, 0, 0), 0));
        store.insert(chunk_with_material(IVec3::new(1, 0, 0), 0));

        for (material_id, chunk) in (10u32..).zip(store.iter_mut()) {
            *chunk = chunk_with_material(chunk.chunk_pos(), material_id);
        }

        assert_eq!(
            store.get(IVec3::new(0, 0, 0)).map(VoxelChunk::material_id),
            Some(10)
        );
        assert_eq!(
            store.get(IVec3::new(1, 0, 0)).map(VoxelChunk::material_id),
            Some(11)
        );
    }
}
