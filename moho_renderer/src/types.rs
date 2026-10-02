/// Vertex data for rendering.
///
/// Field order here is load-bearing: `pipeline::create_main_render_pipeline`'s
/// `vertex_attr_array!` computes each attribute's byte offset by summing the
/// preceding attributes' sizes in the order they're listed, so that list must
/// match this struct's field order exactly (no gaps) for the GPU to read the
/// right bytes.
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub ao: f32,
    pub geometry_type: u32,
    pub light_level: f32,
    /// Per-vertex RGB block-light, each channel 0..=1 (raw value / 15).
    pub block_light_rgb: [f32; 3],
    /// Per-vertex sky-exposure factor (0.0 = underground, 1.0 = open sky).
    pub sky_exposed: f32,
}

/// GPU-side per-instance data: model matrix + material index + object type
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuInstance {
    pub model: [[f32; 4]; 4],
    pub material: u32,
    pub object_type: u32,
    pub padding: [u32; 2],
}

/// Per-mesh GPU data with optional index buffer
#[derive(Debug)]
pub struct MeshEntry {
    pub buffer: wgpu::Buffer,
    pub vertex_count: u32,
    pub index_buffer: Option<wgpu::Buffer>,
    pub index_count: u32,
}
