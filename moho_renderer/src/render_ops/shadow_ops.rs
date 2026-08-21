//! Shadow rendering operations.
//!
//! This module handles shadow matrix calculation and shadow pass rendering
//! for multi-light shadow mapping (Sun, Moon, Dynamic lights).

use crate::gpu_types::ShadowMatrixGpu;
use crate::shadow::ShadowSystem;
use crate::types::{GpuInstance, MeshEntry};
use glam::Vec3;
use wgpu::{Buffer, CommandEncoder, Queue};

/// Update shadow matrices for multi-light shadow mapping.
///
/// Calculates shadow matrices for sun, moon, and dynamic lights,
/// uploading to GPU buffers for shadow rendering and sampling.
///
/// # Arguments
/// * `shadow_system` - Shadow system containing buffers and state
/// * `queue` - WGPU queue for buffer writes
/// * `cam_pos` - Camera position in world space
pub fn update_shadow_matrices(shadow_system: &mut ShadowSystem, queue: &Queue, cam_pos: Vec3) {
    let lighting = &shadow_system.current_lighting;

    let sun_dir = Vec3::new(
        lighting.sun_direction[0],
        lighting.sun_direction[1],
        lighting.sun_direction[2],
    );

    let moon_dir = Vec3::new(
        lighting.moon_direction[0],
        lighting.moon_direction[1],
        lighting.moon_direction[2],
    );

    // Calculate multi-light shadow matrices
    let multi_light_gpu = shadow_system.calculate_multi_light_matrices(sun_dir, moon_dir, cam_pos);

    // Upload multi-light shadow data to csm buffer (which is now used for multi-light)
    queue.write_buffer(
        &shadow_system.csm_matrix_buffer,
        0,
        bytemuck::bytes_of(&multi_light_gpu),
    );

    // For backward compatibility, also write sun (light 0) matrix to legacy buffer
    let shadow_matrix_gpu = ShadowMatrixGpu {
        sm0: multi_light_gpu.light0_m0,
        sm1: multi_light_gpu.light0_m1,
        sm2: multi_light_gpu.light0_m2,
        sm3: multi_light_gpu.light0_m3,
    };
    queue.write_buffer(
        &shadow_system.shadow_matrix_buffer,
        0,
        bytemuck::bytes_of(&shadow_matrix_gpu),
    );
}

/// Render all meshes into shadow maps for active lights.
///
/// Creates one render pass per active light to render the scene from the light's
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
    // Render shadow map for each active light
    for active_light in &shadow_system.active_lights {
        let light_idx = active_light.light_index;

        // Skip inactive lights
        if active_light.intensity < 0.01 {
            continue;
        }

        let mut shadow_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some(&format!("shadow-light-{}-pass", light_idx)),
            color_attachments: &[],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &shadow_system.csm_cascade_views[light_idx as usize],
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            occlusion_query_set: None,
            timestamp_writes: None,
            multiview_mask: None,
        });

        shadow_pass.set_pipeline(&shadow_system.shadow_pipeline);
        shadow_pass.set_bind_group(0, &shadow_system.csm_pass_bind_group, &[]);

        // Set light index via immediate data (same as cascade_idx was used before,
        // formerly a push constant — wgpu 29 renamed push constants to "immediates")
        shadow_pass.set_immediates(0, bytemuck::bytes_of(&light_idx));

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
    #[test]
    fn test_max_shadow_lights() {
        use crate::gpu_types::MAX_SHADOW_LIGHTS;
        // Verify max shadow lights matches expected value
        assert_eq!(MAX_SHADOW_LIGHTS, 4, "Should have 4 shadow light slots");
    }
}
