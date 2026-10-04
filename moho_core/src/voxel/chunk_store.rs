//! Position-keyed storage for meshed voxel chunks.

use super::chunk::VoxelChunk;
use glam::IVec3;

/// Holds at most one [`VoxelChunk`] per chunk position.
///
/// Iteration order is ascending by `(x, y, z)` regardless of insertion order.
#[derive(Debug, Default)]
pub struct ChunkStore;

impl ChunkStore {
    /// Creates an empty store.
    pub fn new() -> Self {
        panic!("not yet implemented")
    }

    /// Stores `chunk` at its own `chunk_pos()`, returning the chunk it replaced, if any.
    pub fn insert(&mut self, _chunk: VoxelChunk) -> Option<VoxelChunk> {
        panic!("not yet implemented")
    }

    /// Removes and returns the chunk at `pos`, or `None` if there is none.
    pub fn remove(&mut self, _pos: IVec3) -> Option<VoxelChunk> {
        panic!("not yet implemented")
    }

    /// Returns the chunk at `pos`, if any.
    pub fn get(&self, _pos: IVec3) -> Option<&VoxelChunk> {
        panic!("not yet implemented")
    }

    /// Returns the chunk at `pos` mutably, if any.
    pub fn get_mut(&mut self, _pos: IVec3) -> Option<&mut VoxelChunk> {
        panic!("not yet implemented")
    }

    /// Iterates chunks in strictly ascending `(x, y, z)` position order.
    pub fn iter(&self) -> impl Iterator<Item = &VoxelChunk> {
        std::iter::from_fn(|| panic!("not yet implemented"))
    }

    /// Iterates chunks mutably in strictly ascending `(x, y, z)` position order.
    ///
    /// Callers must not change a chunk's position; it is the store key.
    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut VoxelChunk> {
        std::iter::from_fn(|| panic!("not yet implemented"))
    }

    /// Number of stored chunks.
    pub fn len(&self) -> usize {
        panic!("not yet implemented")
    }

    /// True when no chunks are stored.
    pub fn is_empty(&self) -> bool {
        panic!("not yet implemented")
    }

    /// Removes every chunk.
    pub fn clear(&mut self) {
        panic!("not yet implemented")
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
        store.insert(VoxelChunk::empty(IVec3::new(0, 0, 0)));
        store.insert(VoxelChunk::empty(IVec3::new(1, 0, 0)));

        for (handle, chunk) in (10u32..).zip(store.iter_mut()) {
            chunk.set_mesh_handle(handle);
        }

        assert_eq!(
            store
                .get(IVec3::new(0, 0, 0))
                .and_then(VoxelChunk::get_mesh_handle),
            Some(10)
        );
        assert_eq!(
            store
                .get(IVec3::new(1, 0, 0))
                .and_then(VoxelChunk::get_mesh_handle),
            Some(11)
        );
    }
}
