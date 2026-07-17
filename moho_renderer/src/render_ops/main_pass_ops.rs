//! Main color pass rendering operations.
//!
//! This module handles rendering the main color pass with skybox and scene geometry.

use crate::shadow::ShadowSystem;
use crate::types::{GpuInstance, MeshEntry};
use wgpu::{BindGroup, Buffer, CommandEncoder, RenderPipeline, TextureView};

/// Render the main color pass with skybox and all pending draws.
///
/// This creates a single render pass that:
/// 1. Clears color to black and depth to 1.0
/// 2. Renders the skybox at maximum depth (behind everything)
/// 3. Renders all pending mesh instances with shadow mapping
///
/// # Arguments
/// * `encoder` - Command encoder for recording the render pass
/// * `frame_view` - Surface texture view to render into
/// * `depth_view` - Depth texture view for depth testing
/// * `skybox_pipeline` - Pipeline for rendering skybox
/// * `skybox_vertex_buffer` - Vertex buffer containing skybox geometry
/// * `skybox_vertex_count` - Number of skybox vertices to draw
/// * `main_pipeline` - Main rendering pipeline
/// * `camera_bind_group` - Bind group 0 (camera uniforms)
/// * `shadow_system` - Shadow system for bind group 1 (shadows)
/// * `instance_buffer` - Instance buffer containing all instance data
/// * `pending_draws` - List of (mesh_handle, instances) to render
/// * `mesh_table` - Table of registered meshes
/// * `offsets` - Instance offsets into instance buffer for each draw
#[allow(clippy::too_many_arguments)] // Render operations naturally have many parameters
pub fn render_main_pass(
    encoder: &mut CommandEncoder,
    frame_view: &TextureView,
    depth_view: &TextureView,
    skybox_pipeline: &RenderPipeline,
    skybox_vertex_buffer: &Buffer,
    skybox_vertex_count: u32,
    main_pipeline: &RenderPipeline,
    camera_bind_group: &BindGroup,
    shadow_system: &ShadowSystem,
    instance_buffer: &Buffer,
    pending_draws: &[(u32, Vec<GpuInstance>)],
    mesh_table: &[Option<MeshEntry>],
    offsets: &[usize],
) {
    let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("batched-rpass"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: frame_view,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                store: wgpu::StoreOp::Store,
            },
            depth_slice: None,
        })],
        depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
            view: depth_view,
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

    // Render skybox first (at maximum depth, behind everything)
    rpass.set_pipeline(skybox_pipeline);
    rpass.set_bind_group(0, camera_bind_group, &[]);
    rpass.set_vertex_buffer(0, skybox_vertex_buffer.slice(..));
    rpass.draw(0..skybox_vertex_count, 0..1);

    // Then render main scene
    rpass.set_pipeline(main_pipeline);
    rpass.set_bind_group(0, camera_bind_group, &[]);
    rpass.set_bind_group(1, &shadow_system.csm_shadow_bind_group, &[]);

    // Iterate draws and issue draw calls
    for (i, (mesh_handle, insts)) in pending_draws.iter().enumerate() {
        let idx = *mesh_handle as usize;
        if idx >= mesh_table.len() {
            continue;
        }
        if let Some(me) = &mesh_table[idx] {
            rpass.set_vertex_buffer(0, me.buffer.slice(..));

            let offset_instances = offsets[i];
            let actual_instance_count = insts.len();

            if actual_instance_count > 0 {
                let offset_bytes =
                    (offset_instances * std::mem::size_of::<GpuInstance>()) as wgpu::BufferAddress;
                let end_bytes = ((offset_instances + actual_instance_count)
                    * std::mem::size_of::<GpuInstance>())
                    as wgpu::BufferAddress;
                rpass.set_vertex_buffer(1, instance_buffer.slice(offset_bytes..end_bytes));

                let instance_count_u32 = actual_instance_count as u32;
                if let Some(idx_buf) = &me.index_buffer {
                    rpass.set_index_buffer(idx_buf.slice(..), wgpu::IndexFormat::Uint32);
                    rpass.draw_indexed(0..me.index_count, 0, 0..instance_count_u32);
                } else {
                    rpass.draw(0..me.vertex_count, 0..instance_count_u32);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clear_color_black() {
        // Verify clear color is black
        let clear_color = wgpu::Color::BLACK;
        assert_eq!(clear_color.r, 0.0);
        assert_eq!(clear_color.g, 0.0);
        assert_eq!(clear_color.b, 0.0);
        assert_eq!(clear_color.a, 1.0);
    }

    #[test]
    fn test_instance_buffer_slicing() {
        // Test instance buffer offset calculation
        let offset_instances = 5;
        let actual_instance_count = 3;
        let instance_size = std::mem::size_of::<GpuInstance>();

        let offset_bytes = (offset_instances * instance_size) as wgpu::BufferAddress;
        let end_bytes =
            ((offset_instances + actual_instance_count) * instance_size) as wgpu::BufferAddress;

        assert_eq!(offset_bytes, (5 * instance_size) as u64);
        assert_eq!(end_bytes, (8 * instance_size) as u64);
    }
}
