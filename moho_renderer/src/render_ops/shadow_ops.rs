//! Shadow rendering operations.
//!
//! This module handles shadow matrix calculation and shadow pass rendering
//! for cascaded shadow mapping (CSM).

use crate::gpu_types::ShadowMatrixGpu;
use crate::shadow::{NUM_SHADOW_CASCADES, ShadowSystem};
use crate::types::{GpuInstance, MeshEntry};
use glam::Vec3;
use wgpu::{Buffer, CommandEncoder, Queue};

/// Update shadow matrices for cascaded shadow mapping.
///
/// Calculates CSM cascade matrices and uploads to GPU buffers:
/// - Full CSM data to csm_matrix_buffer (for shadow rendering)
/// - Cascade 0 matrix to legacy shadow_matrix_buffer (for main pass sampling)
///
/// # Arguments
/// * `shadow_system` - Shadow system containing buffers and state
/// * `queue` - WGPU queue for buffer writes
/// * `cam_pos` - Camera position in world space
pub fn update_shadow_matrices(shadow_system: &mut ShadowSystem, queue: &Queue, cam_pos: Vec3) {
    let sun_dir = Vec3::new(
        shadow_system.current_lighting.sun_direction[0],
        shadow_system.current_lighting.sun_direction[1],
        shadow_system.current_lighting.sun_direction[2],
    );

    // Calculate cascade matrices for CSM
    let (cascade_matrices, cascade_gpu_data) =
        shadow_system.calculate_cascade_matrices(sun_dir, cam_pos);

    // Upload full CSM data to CSM buffer (for shadow rendering)
    queue.write_buffer(
        &shadow_system.csm_matrix_buffer,
        0,
        bytemuck::bytes_of(&cascade_gpu_data),
    );

    // Extract cascade 0 matrix and write to legacy shadow buffer (for main pass sampling)
    let cascade0_mat = cascade_matrices[0];
    let cols = cascade0_mat.to_cols_array_2d();
    let shadow_matrix_gpu = ShadowMatrixGpu {
        sm0: cols[0],
        sm1: cols[1],
        sm2: cols[2],
        sm3: cols[3],
    };
    queue.write_buffer(
        &shadow_system.shadow_matrix_buffer,
        0,
        bytemuck::bytes_of(&shadow_matrix_gpu),
    );
}

/// Render all meshes into shadow cascade maps.
///
/// Creates one render pass per cascade to render the scene from the light's
/// perspective into the shadow depth maps.
///
/// # Arguments
/// * `encoder` - Command encoder for recording render passes
/// * `shadow_system` - Shadow system containing pipelines and views
/// * `instance_buffer` - Instance buffer containing all instance data
/// * `pending_draws` - List of (mesh_handle, instances) to render
/// * `mesh_table` - Table of registered meshes
/// * `offsets` - Byte offsets into instance buffer for each draw
pub fn render_shadow_passes(
    encoder: &mut CommandEncoder,
    shadow_system: &ShadowSystem,
    instance_buffer: &Buffer,
    pending_draws: &[(u32, Vec<GpuInstance>)],
    mesh_table: &[Option<MeshEntry>],
    offsets: &[usize],
) {
    for cascade_idx in 0..NUM_SHADOW_CASCADES {
        let mut shadow_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some(&format!("csm-cascade-{}-pass", cascade_idx)),
            color_attachments: &[],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &shadow_system.csm_cascade_views[cascade_idx as usize],
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            occlusion_query_set: None,
            timestamp_writes: None,
        });

        shadow_pass.set_pipeline(&shadow_system.shadow_pipeline);
        shadow_pass.set_bind_group(0, &shadow_system.csm_pass_bind_group, &[]);

        // Set cascade index via push constant
        shadow_pass.set_push_constants(
            wgpu::ShaderStages::VERTEX,
            0,
            bytemuck::bytes_of(&cascade_idx),
        );

        // Draw all pending meshes from light's perspective
        for (i, (mesh_handle, insts)) in pending_draws.iter().enumerate() {
            let idx = *mesh_handle as usize;
            if idx >= mesh_table.len() {
                continue;
            }
            if let Some(me) = &mesh_table[idx] {
                shadow_pass.set_vertex_buffer(0, me.buffer.slice(..));

                let offset_instances = offsets[i];
                let actual_instance_count = insts.len();

                if actual_instance_count > 0 {
                    let offset_bytes = (offset_instances * std::mem::size_of::<GpuInstance>())
                        as wgpu::BufferAddress;
                    let end_bytes = ((offset_instances + actual_instance_count)
                        * std::mem::size_of::<GpuInstance>())
                        as wgpu::BufferAddress;
                    shadow_pass
                        .set_vertex_buffer(1, instance_buffer.slice(offset_bytes..end_bytes));

                    let instance_count_u32 = actual_instance_count as u32;
                    if let Some(idx_buf) = &me.index_buffer {
                        shadow_pass.set_index_buffer(idx_buf.slice(..), wgpu::IndexFormat::Uint32);
                        shadow_pass.draw_indexed(0..me.index_count, 0, 0..instance_count_u32);
                    } else {
                        shadow_pass.draw(0..me.vertex_count, 0..instance_count_u32);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gpu_types::CascadedShadowMatrixGpu;

    #[test]
    fn test_shadow_matrix_gpu_layout() {
        // Verify ShadowMatrixGpu has expected size for GPU alignment
        assert_eq!(
            std::mem::size_of::<ShadowMatrixGpu>(),
            64,
            "ShadowMatrixGpu should be 64 bytes (4x4 f32 matrix)"
        );
    }

    #[test]
    fn test_cascaded_shadow_matrix_gpu_layout() {
        // Verify CascadedShadowMatrixGpu has expected size for GPU alignment
        // 2 cascades * 16 floats per matrix * 4 bytes per float = 128 bytes
        // Plus 1 vec4 for split distances = 16 bytes
        // Total expected: 144 bytes
        let size = std::mem::size_of::<CascadedShadowMatrixGpu>();
        assert_eq!(
            size, 144,
            "CascadedShadowMatrixGpu should be 144 bytes (2 matrices + split distances), got {}",
            size
        );
    }

    #[test]
    fn test_num_shadow_cascades() {
        // Verify cascade count matches expected value (reduced to 2 for performance)
        assert_eq!(NUM_SHADOW_CASCADES, 2, "Should have 2 shadow cascades for optimized 2-cascade system");
    }

    #[test]
    fn test_instance_offset_calculation() {
        // Test offset calculation logic used in render passes
        let offset_instances = 10;
        let actual_instance_count = 5;

        let offset_bytes =
            (offset_instances * std::mem::size_of::<GpuInstance>()) as wgpu::BufferAddress;
        let end_bytes = ((offset_instances + actual_instance_count)
            * std::mem::size_of::<GpuInstance>()) as wgpu::BufferAddress;

        let expected_instance_size = std::mem::size_of::<GpuInstance>();
        assert_eq!(
            offset_bytes,
            (10 * expected_instance_size) as wgpu::BufferAddress
        );
        assert_eq!(
            end_bytes,
            (15 * expected_instance_size) as wgpu::BufferAddress
        );
    }
}
