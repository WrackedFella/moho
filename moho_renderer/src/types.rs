use std::ops::Range;

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
    /// Material-table index; `NO_VERTEX_MATERIAL` defers to the instance's material.
    pub material: u32,
    /// Base-colour texture coordinates; `[0, 0]` samples white on the default texture.
    pub uv: [f32; 2],
}

/// Vertex `material` value meaning "draw in the instance's material".
pub(crate) const NO_VERTEX_MATERIAL: u32 = u32::MAX;

/// GPU-side per-instance data: model matrix + material index + object type
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuInstance {
    pub model: [[f32; 4]; 4],
    pub material: u32,
    pub object_type: u32,
    pub padding: [u32; 2],
}

/// Where a draw reads its vertices from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum VertexSource {
    /// The mesh's own vertex buffer.
    Mesh,
    /// The frame vertex arena, from vertex `first_vertex`: the mesh skinned in a placement's pose.
    Arena { first_vertex: u32 },
}

impl VertexSource {
    /// The bytes of the frame arena holding `vertex_count` vertices of this source; `None`
    /// for a draw that reads the mesh buffer.
    pub(crate) fn arena_bytes(&self, vertex_count: u32) -> Option<Range<u64>> {
        match *self {
            Self::Mesh => None,
            Self::Arena { first_vertex } => {
                let stride = size_of::<Vertex>() as u64;
                let start = u64::from(first_vertex) * stride;
                Some(start..start + u64::from(vertex_count) * stride)
            }
        }
    }
}

/// What a queued draw binds: its mesh (always the index buffer), the vertex source and the
/// base-colour texture of the main pass.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DrawKey {
    pub(crate) mesh: u32,
    pub(crate) texture: u32,
    pub(crate) source: VertexSource,
    /// Whether the shadow pass draws it.
    pub(crate) casts_shadow: bool,
}

impl DrawKey {
    /// A draw of `mesh` reading the arena from vertex `first_vertex`.
    pub(crate) fn skinned(mesh: u32, texture: u32, first_vertex: u32) -> Self {
        Self {
            mesh,
            texture,
            source: VertexSource::Arena { first_vertex },
            casts_shadow: true,
        }
    }

    /// The vertices this draw reads: the mesh's buffer, or its slice of `arena`. `None` when
    /// the draw reads the arena and there is none.
    pub(crate) fn vertex_slice<'a>(
        &self,
        mesh: &'a MeshEntry,
        arena: Option<&'a wgpu::Buffer>,
    ) -> Option<wgpu::BufferSlice<'a>> {
        match self.source.arena_bytes(mesh.vertex_count) {
            None => Some(mesh.buffer.slice(..)),
            Some(bytes) => arena.map(|arena| arena.slice(bytes)),
        }
    }
}

/// Per-mesh GPU data with optional index buffer
#[derive(Debug)]
pub struct MeshEntry {
    pub buffer: wgpu::Buffer,
    pub vertex_count: u32,
    pub index_buffer: Option<wgpu::Buffer>,
    pub index_count: u32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::MAIN_VERTEX_ATTRIBUTES;
    use std::mem::{offset_of, size_of};

    #[test]
    fn vertex_layout_matches_the_main_pipeline_attributes() {
        let fields = [
            (
                0,
                offset_of!(Vertex, position),
                wgpu::VertexFormat::Float32x3,
            ),
            (1, offset_of!(Vertex, normal), wgpu::VertexFormat::Float32x3),
            (2, offset_of!(Vertex, ao), wgpu::VertexFormat::Float32),
            (
                3,
                offset_of!(Vertex, geometry_type),
                wgpu::VertexFormat::Uint32,
            ),
            (
                10,
                offset_of!(Vertex, light_level),
                wgpu::VertexFormat::Float32,
            ),
            (
                11,
                offset_of!(Vertex, block_light_rgb),
                wgpu::VertexFormat::Float32x3,
            ),
            (
                12,
                offset_of!(Vertex, sky_exposed),
                wgpu::VertexFormat::Float32,
            ),
            (13, offset_of!(Vertex, material), wgpu::VertexFormat::Uint32),
            (14, offset_of!(Vertex, uv), wgpu::VertexFormat::Float32x2),
        ];

        let attributes: Vec<_> = MAIN_VERTEX_ATTRIBUTES
            .iter()
            .map(|a| (a.shader_location, a.offset as usize, a.format))
            .collect();

        assert_eq!(
            size_of::<Vertex>(),
            64,
            "documented in the GPU ABI reference"
        );
        assert_eq!(offset_of!(Vertex, uv), 56, "the uv closes the vertex");
        assert_eq!(attributes, fields);
    }
}
