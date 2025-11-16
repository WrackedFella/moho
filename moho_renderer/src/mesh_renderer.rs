use crate::types::{GpuInstance, MeshEntry};
use wgpu::util::DeviceExt;

/// Mesh rendering coordinator - handles instance preparation, buffer management,
/// and draw call batching for mesh rendering operations.
///
/// This module extracts the rendering stages from the main Renderer impl to
/// improve organization and testability. The MeshRenderer handles:
/// - Instance format conversion (external → internal GPU format)
/// - Instance buffer management (capacity, resizing, uploads)
/// - Draw call batching (accumulate multiple renders before GPU submission)
pub struct MeshRenderer;

impl MeshRenderer {
    /// Convert instances from external format to internal GPU format.
    ///
    /// This transformation prepares instance data for GPU upload by converting
    /// from the external `moho_core::actors::InstanceGpu` format to the internal
    /// `GpuInstance` representation used by rendering.
    ///
    /// # Arguments
    /// * `instances` - Slice of external instance data
    ///
    /// # Returns
    /// Vector of GPU-ready instance data
    pub fn prepare_instances(
        instances: &[moho_core::actors::InstanceGpu],
    ) -> Vec<GpuInstance> {
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

            let size_bytes =
                (new_cap * std::mem::size_of::<GpuInstance>()) as wgpu::BufferAddress;
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
        let buf = match buffer.as_ref() {
            Some(b) => b,
            None => return false,
        };

        if !instances.is_empty() {
            queue.write_buffer(buf, 0, bytemuck::cast_slice(instances));
        } else {
            // Upload zero instance to keep buffer valid
            let zero = GpuInstance {
                model: [[0.0; 4]; 4],
                material: 0,
                object_type: 0,
                padding: [0, 0],
            };
            queue.write_buffer(buf, 0, bytemuck::cast_slice(&[zero]));
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

    fn test_instance() -> moho_core::actors::InstanceGpu {
        moho_core::actors::InstanceGpu {
            model: [
                [1.0, 0.0, 0.0, 0.0],
                [0.0, 1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0],
                [0.0, 0.0, 0.0, 1.0],
            ],
            material: 1,
            object_type: 0,
            padding: [0, 0],
        }
    }

    #[test]
    fn test_prepare_instances_empty() {
        let instances = MeshRenderer::prepare_instances(&[]);
        assert_eq!(instances.len(), 0);
    }

    #[test]
    fn test_prepare_instances_single() {
        let ext_instance = test_instance();
        let instances = MeshRenderer::prepare_instances(&[ext_instance]);

        assert_eq!(instances.len(), 1);
        assert_eq!(instances[0].material, 1);
        assert_eq!(instances[0].object_type, 0);
        assert_eq!(instances[0].model[0][0], 1.0);
    }

    #[test]
    fn test_prepare_instances_multiple() {
        let mut ext_instances = vec![];
        for i in 0..5 {
            let mut inst = test_instance();
            inst.material = i;
            ext_instances.push(inst);
        }

        let instances = MeshRenderer::prepare_instances(&ext_instances);

        assert_eq!(instances.len(), 5);
        for i in 0..5 {
            assert_eq!(instances[i].material, i as u32);
        }
    }

    #[test]
    fn test_flatten_instances_empty() {
        let pending_draws: Vec<(u32, Vec<GpuInstance>)> = vec![];
        let (instances, offsets) = MeshRenderer::flatten_instances(&pending_draws);

        assert_eq!(instances.len(), 0);
        assert_eq!(offsets.len(), 0);
    }

    #[test]
    fn test_flatten_instances_single_draw() {
        let gpu_inst = GpuInstance {
            model: [[1.0; 4]; 4],
            material: 0,
            object_type: 0,
            padding: [0, 0],
        };

        let pending_draws = vec![(1u32, vec![gpu_inst, gpu_inst, gpu_inst])];
        let (instances, offsets) = MeshRenderer::flatten_instances(&pending_draws);

        assert_eq!(instances.len(), 3);
        assert_eq!(offsets.len(), 1);
        assert_eq!(offsets[0], 0);
    }

    #[test]
    fn test_flatten_instances_multiple_draws() {
        let gpu_inst = GpuInstance {
            model: [[1.0; 4]; 4],
            material: 0,
            object_type: 0,
            padding: [0, 0],
        };

        let pending_draws = vec![
            (1u32, vec![gpu_inst, gpu_inst, gpu_inst]),     // 3 instances, offset 0
            (2u32, vec![gpu_inst, gpu_inst]),               // 2 instances, offset 3
            (3u32, vec![gpu_inst]),                         // 1 instance, offset 5
        ];

        let (instances, offsets) = MeshRenderer::flatten_instances(&pending_draws);

        assert_eq!(instances.len(), 6);
        assert_eq!(offsets.len(), 3);
        assert_eq!(offsets[0], 0);
        assert_eq!(offsets[1], 3);
        assert_eq!(offsets[2], 5);
    }

    #[test]
    fn test_validate_mesh_invalid_handle() {
        let mesh_table: Vec<Option<MeshEntry>> = vec![None, None, None];
        assert_eq!(MeshRenderer::validate_mesh(5, &mesh_table), false);
    }

    #[test]
    fn test_validate_mesh_empty_slot() {
        let mesh_table: Vec<Option<MeshEntry>> = vec![None, None, None];
        assert_eq!(MeshRenderer::validate_mesh(1, &mesh_table), false);
    }

    // Note: We can't test ensure_capacity_and_upload without a real GPU device,
    // so those tests are integration-level only
}
