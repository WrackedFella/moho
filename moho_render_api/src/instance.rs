use crate::material::RenderMaterial;
use bytemuck::{Pod, Zeroable};

/// Trait representing types that can be converted into a GPU instance for rendering.
///
/// Implemented by game-domain types (e.g. `Sphere`, `Cube`) so the renderer can
/// collect draw instances and deduplicate materials without naming those
/// concrete types.
pub trait Renderable {
    type Material: RenderMaterial;

    /// The material assigned to this instance, for `MaterialTable` dedup lookup.
    fn material(&self) -> &Self::Material;

    fn to_instance_with_material(&self, material_index: u32) -> InstanceGpu;
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct InstanceGpu {
    pub model: [[f32; 4]; 4], // column-major mat4
    pub material: u32,
    pub object_type: u32,
    pub padding: [u32; 2],
}
