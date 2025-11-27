/// Buffer and mesh management for scene rendering.
///
/// This module handles the registration and lifecycle of mesh buffers,
/// particularly for VoxelChunk meshes that need to be uploaded to the GPU.
///
/// # Double-Buffering Support
///
/// The buffer manager supports double-buffering for seamless mesh updates:
/// - Old mesh continues rendering while new mesh is being generated
/// - Atomic swap when new mesh is ready
/// - Old buffers are pooled for reuse to reduce allocations
use crate::RendererBackend;
use moho_core::voxel::VoxelChunk;
use std::collections::{HashMap, VecDeque};

/// Information about a registered chunk mesh
#[derive(Debug, Clone)]
struct ChunkMeshInfo {
    /// Current active mesh handle
    active_handle: u32,

    /// Pending mesh handle (during double-buffer swap)
    pending_handle: Option<u32>,

    /// Vertex count (for buffer reuse sizing)
    vertex_count: usize,

    /// Index count (for buffer reuse sizing)
    index_count: usize,
}

/// Pooled buffer for reuse
#[derive(Debug)]
struct PooledBuffer {
    handle: u32,
    vertex_capacity: usize,
    index_capacity: usize,
}

/// Manages mesh buffer registration and handles for scene objects.
///
/// This struct tracks which chunks have been uploaded to the GPU and
/// manages their mesh handles to avoid redundant uploads. It supports
/// double-buffering for seamless mesh updates during terrain modification.
pub struct BufferManager {
    /// Maps chunk positions to their mesh info
    chunk_meshes: HashMap<glam::IVec3, ChunkMeshInfo>,

    /// Pool of unused mesh handles for reuse
    buffer_pool: VecDeque<PooledBuffer>,

    /// Maximum number of buffers to keep in the pool
    max_pool_size: usize,

    /// Statistics for monitoring
    stats: BufferStats,
}

/// Statistics for buffer management monitoring
#[derive(Debug, Default, Clone)]
pub struct BufferStats {
    /// Total number of mesh uploads
    pub total_uploads: u64,

    /// Number of buffer reuses from pool
    pub pool_reuses: u64,

    /// Number of mesh swaps performed
    pub swaps_performed: u64,

    /// Current number of active chunk meshes
    pub active_chunks: usize,

    /// Current pool size
    pub pool_size: usize,
}

impl BufferManager {
    /// Create a new BufferManager.
    pub fn new() -> Self {
        Self::with_pool_size(32)
    }

    /// Create a BufferManager with a specific pool size.
    pub fn with_pool_size(max_pool_size: usize) -> Self {
        Self {
            chunk_meshes: HashMap::new(),
            buffer_pool: VecDeque::new(),
            max_pool_size,
            stats: BufferStats::default(),
        }
    }

    /// Get current buffer statistics.
    pub fn stats(&self) -> BufferStats {
        let mut stats = self.stats.clone();
        stats.active_chunks = self.chunk_meshes.len();
        stats.pool_size = self.buffer_pool.len();
        stats
    }

    /// Ensure a VoxelChunk is registered with the renderer.
    ///
    /// If the chunk is already uploaded, returns its existing handle.
    /// If the chunk has no geometry, returns None.
    /// Otherwise, registers the chunk mesh and stores its handle.
    ///
    /// # Arguments
    /// * `chunk` - Mutable reference to the chunk to register
    /// * `renderer` - Backend renderer to register the mesh with
    ///
    /// # Returns
    /// The mesh handle if the chunk has geometry, None otherwise
    pub fn ensure_chunk_registered(
        &mut self,
        chunk: &mut VoxelChunk,
        renderer: &mut dyn RendererBackend,
    ) -> Option<u32> {
        // If already uploaded, return existing handle
        if chunk.is_uploaded() {
            return chunk.get_mesh_handle();
        }

        // Skip chunks with no geometry
        if !chunk.has_geometry() {
            return None;
        }

        // Register the chunk mesh with the renderer
        let handle =
            renderer.register_indexed_mesh(&chunk.vertices, &chunk.normals, &chunk.indices);

        // Store handle in chunk and our tracking map
        chunk.set_mesh_handle(handle);

        let info = ChunkMeshInfo {
            active_handle: handle,
            pending_handle: None,
            vertex_count: chunk.vertices.len(),
            index_count: chunk.indices.len(),
        };
        self.chunk_meshes.insert(chunk.chunk_pos, info);

        self.stats.total_uploads += 1;

        log::info!(
            "Uploaded VoxelChunk {:?}: {} verts, {} indices -> handle {}",
            chunk.chunk_pos,
            chunk.vertices.len(),
            chunk.indices.len(),
            handle
        );

        Some(handle)
    }

    /// Upload a new mesh for a chunk, preparing for double-buffer swap.
    ///
    /// The new mesh is uploaded as a pending buffer. Call `swap_chunk_mesh`
    /// to atomically swap to the new mesh.
    ///
    /// # Arguments
    /// * `chunk_pos` - Position of the chunk
    /// * `vertices` - New vertex data
    /// * `normals` - New normal data
    /// * `indices` - New index data
    /// * `renderer` - Backend renderer
    ///
    /// # Returns
    /// The new mesh handle if successful
    pub fn upload_pending_mesh(
        &mut self,
        chunk_pos: glam::IVec3,
        vertices: &[[f32; 3]],
        normals: &[[f32; 3]],
        indices: &[u32],
        renderer: &mut dyn RendererBackend,
    ) -> Option<u32> {
        if vertices.is_empty() || indices.is_empty() {
            return None;
        }

        // Try to reuse a pooled buffer if available and appropriately sized
        let handle = self
            .try_reuse_buffer(vertices.len(), indices.len(), renderer)
            .unwrap_or_else(|| renderer.register_indexed_mesh(vertices, normals, indices));

        self.stats.total_uploads += 1;

        // Store as pending on existing chunk info, or create new
        if let Some(info) = self.chunk_meshes.get_mut(&chunk_pos) {
            // If there's already a pending mesh, return it to the pool
            if let Some(old_pending) = info.pending_handle.take() {
                let vc = info.vertex_count;
                let ic = info.index_count;
                self.return_to_pool(old_pending, vc, ic);
            }
            // Re-borrow after return_to_pool
            if let Some(info) = self.chunk_meshes.get_mut(&chunk_pos) {
                info.pending_handle = Some(handle);
            }
        } else {
            // New chunk - set as pending until swapped
            let info = ChunkMeshInfo {
                active_handle: handle, // Will be immediately active since no prior mesh
                pending_handle: None,
                vertex_count: vertices.len(),
                index_count: indices.len(),
            };
            self.chunk_meshes.insert(chunk_pos, info);
        }

        log::debug!(
            "Uploaded pending mesh for {:?}: {} verts -> handle {}",
            chunk_pos,
            vertices.len(),
            handle
        );

        Some(handle)
    }

    /// Swap to the pending mesh for a chunk.
    ///
    /// Returns the old mesh handle that was replaced (now pooled).
    pub fn swap_chunk_mesh(&mut self, chunk_pos: glam::IVec3) -> Option<u32> {
        // Extract values first to avoid borrow conflicts
        let (old_handle, new_handle, vc, ic) = {
            let info = self.chunk_meshes.get_mut(&chunk_pos)?;
            let new_handle = info.pending_handle.take()?;
            let old_handle = info.active_handle;
            info.active_handle = new_handle;
            (old_handle, new_handle, info.vertex_count, info.index_count)
        };

        // Return old handle to pool (now safe since we've released the borrow)
        self.return_to_pool(old_handle, vc, ic);

        self.stats.swaps_performed += 1;

        log::debug!(
            "Swapped chunk {:?} mesh: {} -> {}",
            chunk_pos,
            old_handle,
            new_handle
        );

        Some(old_handle)
    }

    /// Get the active mesh handle for a chunk.
    pub fn get_chunk_handle(&self, chunk_pos: glam::IVec3) -> Option<u32> {
        self.chunk_meshes
            .get(&chunk_pos)
            .map(|info| info.active_handle)
    }

    /// Check if a chunk has a pending mesh waiting to be swapped.
    pub fn has_pending_mesh(&self, chunk_pos: glam::IVec3) -> bool {
        self.chunk_meshes
            .get(&chunk_pos)
            .is_some_and(|info| info.pending_handle.is_some())
    }

    /// Cancel a pending mesh upload (discard without swapping).
    pub fn cancel_pending_mesh(&mut self, chunk_pos: glam::IVec3) {
        // Extract values first to avoid borrow conflict
        let to_pool = if let Some(info) = self.chunk_meshes.get_mut(&chunk_pos) {
            info.pending_handle
                .take()
                .map(|pending| (pending, info.vertex_count, info.index_count))
        } else {
            None
        };

        if let Some((pending, vc, ic)) = to_pool {
            self.return_to_pool(pending, vc, ic);
        }
    }

    /// Try to reuse a buffer from the pool.
    fn try_reuse_buffer(
        &mut self,
        needed_vertices: usize,
        needed_indices: usize,
        _renderer: &mut dyn RendererBackend,
    ) -> Option<u32> {
        // Find a buffer that's large enough
        let idx = self.buffer_pool.iter().position(|buf| {
            buf.vertex_capacity >= needed_vertices && buf.index_capacity >= needed_indices
        })?;

        let buffer = self.buffer_pool.remove(idx)?;
        self.stats.pool_reuses += 1;

        log::debug!(
            "Reusing pooled buffer {} (capacity: {} verts, {} indices)",
            buffer.handle,
            buffer.vertex_capacity,
            buffer.index_capacity
        );

        Some(buffer.handle)
    }

    /// Return a buffer to the pool for reuse.
    fn return_to_pool(&mut self, handle: u32, vertex_count: usize, index_count: usize) {
        if self.buffer_pool.len() >= self.max_pool_size {
            // Pool is full, just drop the oldest buffer
            // In a real implementation, we'd call renderer.unregister_mesh on it
            self.buffer_pool.pop_front();
        }

        self.buffer_pool.push_back(PooledBuffer {
            handle,
            vertex_capacity: vertex_count,
            index_capacity: index_count,
        });
    }

    /// Unregister a specific chunk mesh.
    pub fn unregister_chunk(&mut self, chunk_pos: glam::IVec3, renderer: &mut dyn RendererBackend) {
        if let Some(info) = self.chunk_meshes.remove(&chunk_pos) {
            renderer.unregister_mesh(info.active_handle);
            if let Some(pending) = info.pending_handle {
                renderer.unregister_mesh(pending);
            }
        }
    }

    /// Unregister all chunk meshes from the renderer.
    ///
    /// This should be called when clearing or destroying the scene.
    pub fn unregister_chunks(&mut self, renderer: &mut dyn RendererBackend) {
        for info in self.chunk_meshes.values() {
            renderer.unregister_mesh(info.active_handle);
            if let Some(pending) = info.pending_handle {
                renderer.unregister_mesh(pending);
            }
        }
        self.chunk_meshes.clear();

        // Also clear the pool
        for buf in self.buffer_pool.drain(..) {
            renderer.unregister_mesh(buf.handle);
        }
    }

    /// Get the number of registered chunks.
    pub fn chunk_count(&self) -> usize {
        self.chunk_meshes.len()
    }

    /// Check if a specific chunk position is registered.
    pub fn is_chunk_registered(&self, pos: glam::IVec3) -> bool {
        self.chunk_meshes.contains_key(&pos)
    }

    /// Clear the buffer pool (frees GPU memory).
    pub fn clear_pool(&mut self, renderer: &mut dyn RendererBackend) {
        for buf in self.buffer_pool.drain(..) {
            renderer.unregister_mesh(buf.handle);
        }
    }
}

impl Default for BufferManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Mock renderer for testing buffer management
    struct MockRenderer {
        next_handle: u32,
        registered_meshes: Vec<u32>,
    }

    impl MockRenderer {
        fn new() -> Self {
            Self {
                next_handle: 1,
                registered_meshes: Vec::new(),
            }
        }
    }

    impl RendererBackend for MockRenderer {
        fn register_indexed_mesh(
            &mut self,
            _vertices: &[[f32; 3]],
            _normals: &[[f32; 3]],
            _indices: &[u32],
        ) -> u32 {
            let handle = self.next_handle;
            self.next_handle += 1;
            self.registered_meshes.push(handle);
            handle
        }

        fn unregister_mesh(&mut self, handle: u32) {
            self.registered_meshes.retain(|&h| h != handle);
        }

        fn render_mesh(
            &mut self,
            _mesh: u32,
            _instances: &[moho_core::actors::InstanceGpu],
            _camera: (glam::Mat4, glam::Mat4, glam::Vec3),
            _finalize: bool,
        ) {
        }

        fn set_materials(&mut self, _materials: &[crate::Material]) {}

        fn request_redraw(&self) {}
        fn resize(&mut self, _width: u32, _height: u32) {}
        fn set_cursor_visible(&self, _visible: bool) {}
        fn set_cursor_grab(&self, _locked: bool) -> Result<(), Box<dyn std::error::Error>> {
            Ok(())
        }
        fn register_mesh(&mut self, _vertices: &[[f32; 3]]) -> u32 {
            0
        }
        fn update_lighting(&mut self, _lighting: crate::gpu_types::LightingGpu) {}
        fn set_frame_callback_raw(&mut self, _ptr: Option<*mut dyn crate::FrameCallback>) {}
        fn set_frame_callback_arc(
            &mut self,
            _cb: Option<std::sync::Arc<std::sync::Mutex<dyn crate::FrameCallback>>>,
        ) {
        }
    }

    #[test]
    fn test_buffer_manager_new() {
        let manager = BufferManager::new();
        assert_eq!(manager.chunk_count(), 0);
    }

    #[test]
    fn test_ensure_chunk_registered_with_geometry() {
        let mut manager = BufferManager::new();
        let mut renderer = MockRenderer::new();

        let mut chunk = VoxelChunk {
            chunk_pos: glam::IVec3::new(0, 0, 0),
            vertices: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            normals: vec![[0.0, 0.0, 1.0], [0.0, 0.0, 1.0], [0.0, 0.0, 1.0]],
            indices: vec![0, 1, 2],
            material_id: 0,
            mesh_handle: None,
        };

        let handle = manager.ensure_chunk_registered(&mut chunk, &mut renderer);

        assert!(handle.is_some());
        assert_eq!(handle.unwrap(), 1);
        assert_eq!(manager.chunk_count(), 1);
        assert!(manager.is_chunk_registered(glam::IVec3::new(0, 0, 0)));
        assert!(chunk.is_uploaded());
    }

    #[test]
    fn test_ensure_chunk_registered_no_geometry() {
        let mut manager = BufferManager::new();
        let mut renderer = MockRenderer::new();

        let mut chunk = VoxelChunk {
            chunk_pos: glam::IVec3::new(1, 1, 1),
            vertices: vec![],
            normals: vec![],
            indices: vec![],
            material_id: 0,
            mesh_handle: None,
        };

        let handle = manager.ensure_chunk_registered(&mut chunk, &mut renderer);

        assert!(handle.is_none());
        assert_eq!(manager.chunk_count(), 0);
        assert!(!manager.is_chunk_registered(glam::IVec3::new(1, 1, 1)));
    }

    #[test]
    fn test_ensure_chunk_registered_already_uploaded() {
        let mut manager = BufferManager::new();
        let mut renderer = MockRenderer::new();

        let mut chunk = VoxelChunk {
            chunk_pos: glam::IVec3::new(2, 2, 2),
            vertices: vec![[0.0, 0.0, 0.0]],
            normals: vec![[0.0, 0.0, 1.0]],
            indices: vec![0],
            material_id: 0,
            mesh_handle: Some(42), // Already has a handle
        };

        let handle = manager.ensure_chunk_registered(&mut chunk, &mut renderer);

        assert_eq!(handle, Some(42));
        assert_eq!(manager.chunk_count(), 0); // Not added to our tracking
        assert_eq!(renderer.registered_meshes.len(), 0); // No new registration
    }

    #[test]
    fn test_unregister_chunks() {
        let mut manager = BufferManager::new();
        let mut renderer = MockRenderer::new();

        // Register several chunks
        for i in 0..3 {
            let mut chunk = VoxelChunk {
                chunk_pos: glam::IVec3::new(i, 0, 0),
                vertices: vec![[0.0, 0.0, 0.0]],
                normals: vec![[0.0, 0.0, 1.0]],
                indices: vec![0],
                material_id: 0,
                mesh_handle: None,
            };
            manager.ensure_chunk_registered(&mut chunk, &mut renderer);
        }

        assert_eq!(manager.chunk_count(), 3);
        assert_eq!(renderer.registered_meshes.len(), 3);

        manager.unregister_chunks(&mut renderer);

        assert_eq!(manager.chunk_count(), 0);
        assert_eq!(renderer.registered_meshes.len(), 0);
    }
}
