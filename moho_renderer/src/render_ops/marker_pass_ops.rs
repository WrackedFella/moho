//! Translucent box marker pass.

use crate::markers::UNIT_CUBE_INDICES;
use crate::resources::MarkerBuffers;
use wgpu::{BindGroup, CommandEncoder, RenderPipeline, TextureView};

/// Draw `instance_count` markers over what the main pass left in `frame_view`.
///
/// Loads colour and depth rather than clearing them, so markers are tested
/// against the world's depth; the pipeline writes none, which keeps them out of
/// the SSAO pass that follows.
pub fn render_marker_pass(
    encoder: &mut CommandEncoder,
    frame_view: &TextureView,
    depth_view: &TextureView,
    marker_pipeline: &RenderPipeline,
    camera_bind_group: &BindGroup,
    buffers: &MarkerBuffers,
    instance_count: u32,
) {
    let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("marker-rpass"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: frame_view,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Load,
                store: wgpu::StoreOp::Store,
            },
            depth_slice: None,
        })],
        depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
            view: depth_view,
            depth_ops: Some(wgpu::Operations {
                load: wgpu::LoadOp::Load,
                store: wgpu::StoreOp::Store,
            }),
            stencil_ops: None,
        }),
        occlusion_query_set: None,
        timestamp_writes: None,
        multiview_mask: None,
    });

    rpass.set_pipeline(marker_pipeline);
    rpass.set_bind_group(0, camera_bind_group, &[]);
    rpass.set_vertex_buffer(0, buffers.cube_vertices.slice(..));
    rpass.set_vertex_buffer(1, buffers.instances.slice(..));
    rpass.set_index_buffer(buffers.cube_indices.slice(..), wgpu::IndexFormat::Uint32);
    rpass.draw_indexed(0..UNIT_CUBE_INDICES.len() as u32, 0, 0..instance_count);
}
