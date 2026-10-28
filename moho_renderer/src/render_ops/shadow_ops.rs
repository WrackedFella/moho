//! Shadow rendering operations.
//!
//! This module handles cascade fitting and shadow pass rendering for the sun's and the
//! moon's cascaded shadow maps.

use crate::gpu_types::ShadowGpu;
use crate::shadow::ShadowSystem;
use crate::types::{DrawKey, GpuInstance, MeshEntry};
use glam::{Mat4, Vec3};
use wgpu::{Buffer, CommandEncoder, Queue};

/// Fit the cascades to the camera and upload the shadow block.
///
/// # Arguments
/// * `shadow_system` - Shadow system containing buffers and state
/// * `queue` - WGPU queue for buffer writes
/// * `view_mat` - World-to-view matrix of the camera
/// * `proj_mat` - Projection matrix of the camera
/// * `cut` - View-height cut, from `view_height_uniform`
pub fn update_shadow_matrices(
    shadow_system: &mut ShadowSystem,
    queue: &Queue,
    view_mat: Mat4,
    proj_mat: Mat4,
    cut: f32,
) {
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

    let shadow_gpu = with_view_height(
        shadow_system.update_cascades(sun_dir, moon_dir, view_mat, proj_mat),
        cut,
    );

    queue.write_buffer(
        &shadow_system.csm_matrix_buffer,
        0,
        bytemuck::bytes_of(&shadow_gpu),
    );
}

/// Set the view-height cut in `metadata.w`, which the shadow shader reads.
fn with_view_height(mut gpu: ShadowGpu, cut: f32) -> ShadowGpu {
    gpu.metadata[3] = cut;
    gpu
}

/// Render all meshes into shadow maps for active lights.
///
/// Creates one render pass per cascade layer of each active light to render the scene from the
/// cascade's perspective into its shadow depth map.
///
/// # Arguments
/// * `encoder` - Command encoder for recording render passes
/// * `shadow_system` - Shadow system containing pipelines and views
/// * `instance_buffer` - Instance buffer containing all instance data
/// * `pending_draws` - List of (draw key, instances) to render; the texture is unused here
/// * `mesh_table` - Table of registered meshes
/// * `offsets` - Byte offsets into instance buffer for each draw
/// * `arena` - Frame vertex arena that skinned draws read their vertices from
pub fn render_shadow_passes(
    encoder: &mut CommandEncoder,
    shadow_system: &ShadowSystem,
    instance_buffer: &Buffer,
    pending_draws: &[(DrawKey, Vec<GpuInstance>)],
    mesh_table: &[Option<MeshEntry>],
    offsets: &[usize],
    arena: Option<&Buffer>,
) {
    for layer in shadow_system.layers_to_render() {
        let mut shadow_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some(&format!("shadow-layer-{layer}-pass")),
            color_attachments: &[],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &shadow_system.csm_cascade_views[layer as usize],
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

        shadow_pass.set_immediates(0, bytemuck::bytes_of(&layer));

        for (i, (key, insts)) in pending_draws.iter().enumerate() {
            let idx = key.mesh as usize;
            if idx >= mesh_table.len() {
                continue;
            }
            if let Some(me) = &mesh_table[idx] {
                let Some(vertices) = key.vertex_slice(me, arena) else {
                    continue;
                };
                shadow_pass.set_vertex_buffer(0, vertices);

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

    #[test]
    fn shadow_uniform_carries_the_cut_in_metadata_w() {
        let gpu = ShadowGpu {
            metadata: [1.0, 2.0, 3.0, 0.0],
            ..Default::default()
        };

        let cut = with_view_height(gpu, 12.5);

        assert_eq!(cut.metadata, [1.0, 2.0, 3.0, 12.5]);
    }
}
