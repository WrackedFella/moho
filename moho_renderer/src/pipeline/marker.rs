//! Pipeline for the translucent box marker pass.

use moho_render_api::MarkerGpu;

use crate::MARKER_INFLATE;

const MARKER_SHADER: &str = include_str!("../../../shaders/marker.wgsl");

/// The pipeline-overridable constants of `marker.wgsl`, name and value.
pub(super) fn marker_constants() -> [(&'static str, f64); 1] {
    [("marker_inflate", f64::from(MARKER_INFLATE))]
}

/// Alpha-blended, back-face-culled, depth-tested but depth-write-free pipeline
/// drawing a unit cube instanced by [`MarkerGpu`]: solid triangles for
/// `TriangleList`, one-pixel edges for `LineList`.
///
/// Single-sampled to match the main pass's colour and depth targets, which
/// the marker pass loads rather than clears.
pub(super) fn create_marker_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    surface_format: wgpu::TextureFormat,
    depth_format: wgpu::TextureFormat,
    topology: wgpu::PrimitiveTopology,
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("marker-shader"),
        source: wgpu::ShaderSource::Wgsl(MARKER_SHADER.into()),
    });
    let constants = marker_constants();
    let label = match topology {
        wgpu::PrimitiveTopology::LineList => "marker-outline-pipeline",
        _ => "marker-pipeline",
    };

    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(label),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[
                Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<[f32; 3]>() as wgpu::BufferAddress,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x3],
                }),
                Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<MarkerGpu>() as wgpu::BufferAddress,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &wgpu::vertex_attr_array![
                        1 => Float32x4,
                        2 => Float32x4,
                        3 => Float32x4,
                    ],
                }),
            ],
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            compilation_options: wgpu::PipelineCompilationOptions {
                constants: &constants,
                ..Default::default()
            },
            targets: &[Some(wgpu::ColorTargetState {
                format: surface_format,
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: wgpu::PrimitiveState {
            topology,
            strip_index_format: None,
            front_face: wgpu::FrontFace::Ccw,
            cull_mode: Some(wgpu::Face::Back),
            unclipped_depth: false,
            polygon_mode: wgpu::PolygonMode::Fill,
            conservative: false,
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: depth_format,
            depth_write_enabled: Some(false),
            depth_compare: Some(wgpu::CompareFunction::LessEqual),
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }),
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marker_pipeline_sets_inflate_from_marker_inflate() {
        let constants = marker_constants();

        assert_eq!(constants, [("marker_inflate", f64::from(MARKER_INFLATE))]);
    }
}
