use crate::types::{GpuInstance, MeshEntry};

/// Mesh rendering coordinator - handles instance preparation, buffer management,
/// and draw call batching for mesh rendering operations.
///
/// This module extracts the rendering stages from the main Renderer impl to
/// improve organization and testability. The MeshRenderer handles:
/// - Instance format conversion (external → internal GPU format)
/// - Instance buffer management (capacity, resizing, uploads)
/// - Draw call batching (accumulate multiple renders before GPU submission)
#[derive(Debug)]
pub struct MeshRenderer;

impl MeshRenderer {
    /// Convert instances from external format to internal GPU format.
    ///
    /// This transformation prepares instance data for GPU upload by converting
    /// from the external `moho_render_api::InstanceGpu` format to the internal
    /// `GpuInstance` representation used by rendering.
    ///
    /// # Arguments
    /// * `instances` - Slice of external instance data
    ///
    /// # Returns
    /// Vector of GPU-ready instance data
    pub fn prepare_instances(instances: &[moho_render_api::InstanceGpu]) -> Vec<GpuInstance> {
        instances
            .iter()
            .map(|ic| GpuInstance {
                model: ic.model,
                material: ic.material,
                object_type: ic.object_type,
                padding: ic.padding,
            })
            .collect()
    }

    /// Ensure instance buffer has sufficient capacity and upload instance data.
    ///
    /// This manages dynamic buffer resizing (doubling capacity when needed) and
    /// uploads instance data to the GPU. If the buffer is too small, it creates
    /// a new buffer with increased capacity.
    ///
    /// # Arguments
    /// * `device` - GPU device for buffer creation
    /// * `queue` - GPU queue for data upload
    /// * `buffer` - Current instance buffer (may be replaced if resizing)
    /// * `capacity` - Current buffer capacity (may be updated if resizing)
    /// * `instances` - Instance data to upload
    ///
    /// # Returns
    /// `true` if successful, `false` if buffer is missing
    ///
    /// # Performance
    /// Buffer doubling strategy reduces reallocation frequency:
    /// - Small scenes: 1-2 resizes total
    /// - Large scenes: log2(N) resizes maximum
    pub fn ensure_capacity_and_upload(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        buffer: &mut Option<wgpu::Buffer>,
        capacity: &mut usize,
        instances: &[GpuInstance],
    ) -> bool {
        let required = instances.len().max(1);

        // Resize buffer if needed (double capacity until sufficient)
        if *capacity < required {
            let mut new_cap = (*capacity).max(1);
            while new_cap < required {
                new_cap = new_cap.saturating_mul(2);
            }

            let size_bytes = (new_cap * std::mem::size_of::<GpuInstance>()) as wgpu::BufferAddress;
            let new_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("instance-buffer"),
                size: size_bytes,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });

            *buffer = Some(new_buffer);
            *capacity = new_cap;
        }

        // Upload instance data
        let Some(buf) = buffer.as_ref() else {
            return false;
        };

        if instances.is_empty() {
            // Upload zero instance to keep buffer valid
            let zero = GpuInstance {
                model: [[0.0; 4]; 4],
                material: 0,
                object_type: 0,
                padding: [0, 0],
            };
            queue.write_buffer(buf, 0, bytemuck::cast_slice(&[zero]));
        } else {
            queue.write_buffer(buf, 0, bytemuck::cast_slice(instances));
        }

        true
    }

    /// Flatten multiple instance lists into a single contiguous buffer.
    ///
    /// This prepares batched rendering by combining all pending draw calls'
    /// instance data into a single GPU buffer, tracking the start offset for
    /// each draw call.
    ///
    /// # Arguments
    /// * `pending_draws` - List of (mesh_handle, instances) for each draw call
    ///
    /// # Returns
    /// Tuple of (all_instances, offsets) where:
    /// - `all_instances` - Concatenated instance data for GPU upload
    /// - `offsets` - Start index in buffer for each draw call
    ///
    /// # Example
    /// ```ignore
    /// // Draw call 0: 3 cubes (offset 0)
    /// // Draw call 1: 2 spheres (offset 3)
    /// // Draw call 2: 1 chunk (offset 5)
    /// let (instances, offsets) = MeshRenderer::flatten_instances(&draws);
    /// assert_eq!(offsets, vec![0, 3, 5]);
    /// assert_eq!(instances.len(), 6);
    /// ```
    pub fn flatten_instances(
        pending_draws: &[(u32, Vec<GpuInstance>)],
    ) -> (Vec<GpuInstance>, Vec<usize>) {
        let mut all_instances: Vec<GpuInstance> = Vec::new();
        let mut offsets: Vec<usize> = Vec::with_capacity(pending_draws.len());

        for (_mesh, insts) in pending_draws {
            offsets.push(all_instances.len());
            all_instances.extend_from_slice(insts);
        }

        (all_instances, offsets)
    }

    /// Validate mesh handle and check if mesh exists in table.
    ///
    /// # Arguments
    /// * `mesh` - Mesh handle to validate
    /// * `mesh_table` - Table of registered meshes
    ///
    /// # Returns
    /// `true` if mesh handle is valid and mesh exists, `false` otherwise
    pub fn validate_mesh(mesh: u32, mesh_table: &[Option<MeshEntry>]) -> bool {
        let idx = mesh as usize;
        if idx >= mesh_table.len() {
            return false;
        }
        mesh_table[idx].is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prepare_instances_copies_every_field_in_order() {
        let first = moho_render_api::InstanceGpu {
            model: glam::Mat4::from_translation(glam::Vec3::new(1.0, 2.0, 3.0)).to_cols_array_2d(),
            material: 4,
            object_type: 2,
            padding: [7, 8],
        };
        let second = moho_render_api::InstanceGpu {
            model: glam::Mat4::from_translation(glam::Vec3::new(-5.0, 6.0, 9.0)).to_cols_array_2d(),
            material: 11,
            object_type: 1,
            padding: [9, 10],
        };
        let expected = [
            GpuInstance {
                model: first.model,
                material: 4,
                object_type: 2,
                padding: [7, 8],
            },
            GpuInstance {
                model: second.model,
                material: 11,
                object_type: 1,
                padding: [9, 10],
            },
        ];

        let instances = MeshRenderer::prepare_instances(&[first, second]);

        assert_eq!(
            bytemuck::cast_slice::<GpuInstance, u8>(&instances),
            bytemuck::cast_slice::<GpuInstance, u8>(&expected)
        );
    }

    #[test]
    fn test_flatten_instances_empty() {
        let pending_draws: Vec<(u32, Vec<GpuInstance>)> = vec![];
        let (instances, offsets) = MeshRenderer::flatten_instances(&pending_draws);

        assert_eq!(instances.len(), 0);
        assert_eq!(offsets.len(), 0);
    }

    #[test]
    fn test_flatten_instances_multiple_draws() {
        let inst = |material: u32| GpuInstance {
            model: [[1.0; 4]; 4],
            material,
            object_type: 0,
            padding: [0, 0],
        };

        let pending_draws = vec![
            (1u32, vec![inst(1), inst(1), inst(1)]),
            (2u32, vec![inst(2), inst(2)]),
            (3u32, vec![inst(3)]),
        ];

        let (instances, offsets) = MeshRenderer::flatten_instances(&pending_draws);

        assert_eq!(instances.len(), 6);
        assert_eq!(offsets, vec![0, 3, 5]);
        for (k, offset) in offsets.iter().enumerate() {
            assert_eq!(instances[*offset].material, k as u32 + 1);
        }
        assert_eq!(instances[5].material, 3);
    }

    #[test]
    fn test_validate_mesh_invalid_handle() {
        let mesh_table: Vec<Option<MeshEntry>> = vec![None, None, None];
        assert!(!MeshRenderer::validate_mesh(5, &mesh_table));
    }

    #[test]
    fn test_validate_mesh_empty_slot() {
        let mesh_table: Vec<Option<MeshEntry>> = vec![None, None, None];
        assert!(!MeshRenderer::validate_mesh(1, &mesh_table));
    }

    // Note: We can't test ensure_capacity_and_upload without a real GPU device,
    // so those tests are integration-level only
}
