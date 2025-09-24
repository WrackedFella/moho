# engine_core

This crate contains core, engine-level utilities and data types used by the Moho project. It currently focuses on graphics/renderer-friendly types and small ray-tracing helpers for prototyping.

Key items:

- `engine_core::actors::Sphere` — scene primitive used by the ECS.
- `engine_core::actors::InstanceGpu` — `#[repr(C)]` POD used for GPU instance uploads. This struct must match shader inputs exactly.

InstanceGpu layout (CPU side):

- `model: [[f32;4];4]` — column-major 4x4 model matrix. Shader should accept 4 vec4 locations (one per column) or an equivalent mat4 depending on the shading language.
- `material: u32` — material id (0 = lambertian, 1 = metal, 2 = dielectric).
- `object_type: u32` — reserved for future object-type switching.
- `padding: [u32;2]` — ensures 16-byte alignment/padding for GPU buffers.

When editing `InstanceGpu`, update shaders and add a test (`engine_core/tests/instance_pod.rs`) asserting `std::mem::size_of::<InstanceGpu>()` and `bytemuck::Pod` compliance.

Shader parity (GLSL mapping example):

- `layout(location = 0) in vec3 in_position;`           // vertex position
- `layout(location = 1) in vec4 in_model_col0;`         // model matrix column 0
- `layout(location = 2) in vec4 in_model_col1;`         // model matrix column 1
- `layout(location = 3) in vec4 in_model_col2;`         // model matrix column 2
- `layout(location = 4) in vec4 in_model_col3;`         // model matrix column 3
- `layout(location = 5) in uint in_material;`          // material id

When adding or editing shaders, ensure the attribute locations and types exactly match `InstanceGpu`. Mismatches will compile but produce incorrect rendering.
