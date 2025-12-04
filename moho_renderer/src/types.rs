/// Vertex data for rendering
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub ao: f32,
    pub geometry_type: u32,
}

/// GPU-side per-instance data: model matrix + material index + object type
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuInstance {
    pub model: [[f32; 4]; 4],
    pub material: u32,
    pub object_type: u32,
    pub padding: [u32; 2],
}

/// Per-mesh GPU data with optional index buffer
pub struct MeshEntry {
    pub buffer: wgpu::Buffer,
    pub vertex_count: u32,
    pub index_buffer: Option<wgpu::Buffer>,
    pub index_count: u32,
}
