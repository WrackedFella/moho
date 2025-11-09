/// Buffer and mesh management for scene rendering.
///
/// This module handles the registration and lifecycle of mesh buffers,
/// particularly for VoxelChunk meshes that need to be uploaded to the GPU.
use crate::RendererBackend;
use moho_core::voxel::VoxelChunk;
use std::collections::HashMap;

/// Manages mesh buffer registration and handles for scene objects.
///
/// This struct tracks which chunks have been uploaded to the GPU and
/// manages their mesh handles to avoid redundant uploads.
pub struct BufferManager {
    /// Maps chunk positions to their registered mesh handles
    chunk_handles: HashMap<glam::IVec3, u32>,
}

impl BufferManager {
    /// Create a new BufferManager.
    pub fn new() -> Self {
        Self {
            chunk_handles: HashMap::new(),
        }
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
        self.chunk_handles.insert(chunk.chunk_pos, handle);

        log::info!(
            "Uploaded VoxelChunk {:?}: {} verts, {} indices -> handle {}",
            chunk.chunk_pos,
            chunk.vertices.len(),
            chunk.indices.len(),
            handle
        );

        Some(handle)
    }

    /// Unregister all chunk meshes from the renderer.
    ///
    /// This should be called when clearing or destroying the scene.
    pub fn unregister_chunks(&mut self, renderer: &mut dyn RendererBackend) {
        for handle in self.chunk_handles.values() {
            renderer.unregister_mesh(*handle);
        }
        self.chunk_handles.clear();
    }

    /// Get the number of registered chunks.
    pub fn chunk_count(&self) -> usize {
        self.chunk_handles.len()
    }

    /// Check if a specific chunk position is registered.
    pub fn is_chunk_registered(&self, pos: glam::IVec3) -> bool {
        self.chunk_handles.contains_key(&pos)
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
