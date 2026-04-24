use crate::materials::MaterialType;
use bytemuck::{Pod, Zeroable};
use glam::Vec3;
use legion::World;
use legion::query::IntoQuery;

#[derive(Copy, Clone, Debug)]
pub struct Sphere {
    pub center: Vec3,
    pub radius: f32,
    pub mat_ptr: MaterialType,
}

#[derive(Copy, Clone, Debug)]
pub struct Cube {
    pub center: Vec3,
    pub length: f32,
    pub width: f32,
    pub height: f32,
    pub mat_ptr: MaterialType,
}

impl Cube {
    /// Create a new axis-aligned cube centered at `center` with the given
    /// length (X), width (Z) and height (Y) and assigned material.
    pub fn new(center: Vec3, length: f32, width: f32, height: f32, mat: MaterialType) -> Cube {
        Cube {
            center,
            length,
            width,
            height,
            mat_ptr: mat,
        }
    }
}

impl Sphere {
    pub fn new(cen: Vec3, r: f32, mat: MaterialType) -> Sphere {
        Sphere {
            center: cen,
            radius: r,
            mat_ptr: mat,
        }
    }
}

/// Trait representing types that can be converted into a GPU instance for rendering.
pub trait Renderable {
    fn to_instance_with_material(&self, material_index: u32) -> InstanceGpu;
}

impl Renderable for Sphere {
    fn to_instance_with_material(&self, material_index: u32) -> InstanceGpu {
        Sphere::to_instance_with_material(self, material_index)
    }
}

impl Renderable for Cube {
    fn to_instance_with_material(&self, material_index: u32) -> InstanceGpu {
        Cube::to_instance_with_material(self, material_index)
    }
}

/// Trait for entities that provide custom mesh geometry.
/// Unlike Renderable which uses shared meshes with instancing,
/// CustomMesh provides unique vertex/index data per entity.
pub trait CustomMesh {
    /// Get vertex positions [x, y, z]
    fn vertices(&self) -> &[[f32; 3]];

    /// Get vertex normals [x, y, z]
    fn normals(&self) -> &[[f32; 3]];

    /// Get triangle indices (3 per triangle)
    fn indices(&self) -> &[u32];

    /// Get world-space transform matrix
    fn transform(&self) -> glam::Mat4;

    /// Get material index for this mesh
    fn material_index(&self) -> u32;
}

/// Serializable mesh data for passing to renderer.
/// Contains owned copies of mesh data to avoid lifetime issues.
#[derive(Clone, Debug)]
pub struct CustomMeshData {
    pub vertices: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub indices: Vec<u32>,
    pub transform: glam::Mat4,
    pub material_index: u32,
}

/// Collect all entities with custom mesh geometry from the ECS World.
/// Currently queries for VoxelChunk entities.
pub fn collect_custom_meshes(world: &World) -> Vec<CustomMeshData> {
    let mut meshes = Vec::new();

    // Query VoxelChunk entities
    let mut query = <&crate::voxel::VoxelChunk>::query();
    for chunk in query.iter(world) {
        if !chunk.is_empty() {
            meshes.push(CustomMeshData {
                vertices: chunk.vertices().to_vec(),
                normals: chunk.normals().to_vec(),
                indices: chunk.indices().to_vec(),
                transform: chunk.transform(),
                material_index: chunk.material_index(),
            });
        }
    }

    meshes
}

/// Convenience helper to collect all Renderable instances from the provided ECS `World`.
/// Currently queries for `Sphere`, `Cube`, and `VoxelChunk` components and returns a Vec of `InstanceGpu`.
pub fn collect_renderable_instances(world: &mut World) -> Vec<InstanceGpu> {
    let mut out: Vec<InstanceGpu> = Vec::new();
    let mut qs = <&Sphere>::query();
    for s in qs.iter(world) {
        out.push(s.to_instance_with_material(0));
    }
    let mut qc = <&Cube>::query();
    for c in qc.iter(world) {
        out.push(c.to_instance_with_material(0));
    }

    // Add voxel chunk rendering
    let mut qv = <&crate::voxel::VoxelChunk>::query();
    for chunk in qv.iter(world) {
        out.push(chunk.to_instance_with_material(0));
    }

    out
}

impl Cube {
    /// Create an InstanceGpu for this cube, assigning the provided material
    /// index. The instance model scales a unit cube to the requested
    /// dimensions and translates to the specified center.
    pub fn to_instance_with_material(&self, material_index: u32) -> InstanceGpu {
        // Note: unit cube is centered at origin with extents [-0.5,0.5] on each axis
        let translate = glam::Mat4::from_translation(self.center);
        let scale = glam::Mat4::from_scale(glam::Vec3::new(self.length, self.height, self.width));
        let model = translate * scale;
        let cols = model.to_cols_array();
        let mut mat = [[0f32; 4]; 4];
        mat[0] = [cols[0], cols[1], cols[2], cols[3]];
        mat[1] = [cols[4], cols[5], cols[6], cols[7]];
        mat[2] = [cols[8], cols[9], cols[10], cols[11]];
        mat[3] = [cols[12], cols[13], cols[14], cols[15]];
        InstanceGpu {
            model: mat,
            material: material_index,
            object_type: 1u32, // Cube actor
            padding: [0u32; 2],
        }
    }

    /// Generate a unit cube (centered at origin) vertex list, normals, and
    /// indices for an indexed mesh. The cube spans [-0.5,0.5] on each axis.
    /// Each face has 4 vertices with proper face normals for correct lighting.
    /// All faces use CCW winding when viewed from OUTSIDE the cube.
    pub fn unit_cube_indexed() -> (Vec<[f32; 3]>, Vec<[f32; 3]>, Vec<u32>) {
        // 24 vertices total: 4 per face, 6 faces
        // Vertices ordered CCW when viewed from outside for each face
        let verts: Vec<[f32; 3]> = vec![
            // +X face (right) - normal [1,0,0] pointing right
            // Viewed from +X (right side), CCW order: bottom-back, top-back, top-front, bottom-front
            [0.5, -0.5, -0.5], // 0: bottom-back
            [0.5, 0.5, -0.5],  // 1: top-back
            [0.5, 0.5, 0.5],   // 2: top-front
            [0.5, -0.5, 0.5],  // 3: bottom-front
            // -X face (left) - normal [-1,0,0] pointing left
            // Viewed from -X (left side), CCW order: bottom-front, top-front, top-back, bottom-back
            [-0.5, -0.5, 0.5],  // 4: bottom-front
            [-0.5, 0.5, 0.5],   // 5: top-front
            [-0.5, 0.5, -0.5],  // 6: top-back
            [-0.5, -0.5, -0.5], // 7: bottom-back
            // +Y face (top) - normal [0,1,0] pointing up
            // Viewed from +Y (above), CCW order: back-left, front-left, front-right, back-right
            [-0.5, 0.5, -0.5], // 8: back-left
            [-0.5, 0.5, 0.5],  // 9: front-left
            [0.5, 0.5, 0.5],   // 10: front-right
            [0.5, 0.5, -0.5],  // 11: back-right
            // -Y face (bottom) - normal [0,-1,0] pointing down
            // Viewed from -Y (below), CCW order: front-left, front-right, back-right, back-left
            [-0.5, -0.5, 0.5],  // 12: front-left
            [0.5, -0.5, 0.5],   // 13: front-right
            [0.5, -0.5, -0.5],  // 14: back-right
            [-0.5, -0.5, -0.5], // 15: back-left
            // +Z face (front) - normal [0,0,1] pointing forward
            // Viewed from +Z (front), CCW order: bottom-left, bottom-right, top-right, top-left
            [-0.5, -0.5, 0.5], // 16: bottom-left
            [0.5, -0.5, 0.5],  // 17: bottom-right
            [0.5, 0.5, 0.5],   // 18: top-right
            [-0.5, 0.5, 0.5],  // 19: top-left
            // -Z face (back) - normal [0,0,-1] pointing backward
            // Viewed from -Z (back), CCW order: bottom-right, bottom-left, top-left, top-right
            [0.5, -0.5, -0.5], // 20: bottom-right (when viewed from -Z, +X is on the right)
            [-0.5, -0.5, -0.5], // 21: bottom-left
            [-0.5, 0.5, -0.5], // 22: top-left
            [0.5, 0.5, -0.5],  // 23: top-right
        ];

        // Normals: each face has 4 vertices with the same normal
        let normals: Vec<[f32; 3]> = vec![
            // +X face (right)
            [1.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            // -X face (left)
            [-1.0, 0.0, 0.0],
            [-1.0, 0.0, 0.0],
            [-1.0, 0.0, 0.0],
            [-1.0, 0.0, 0.0],
            // +Y face (top)
            [0.0, 1.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 1.0, 0.0],
            // -Y face (bottom)
            [0.0, -1.0, 0.0],
            [0.0, -1.0, 0.0],
            [0.0, -1.0, 0.0],
            [0.0, -1.0, 0.0],
            // +Z face (front)
            [0.0, 0.0, 1.0],
            [0.0, 0.0, 1.0],
            [0.0, 0.0, 1.0],
            [0.0, 0.0, 1.0],
            // -Z face (back)
            [0.0, 0.0, -1.0],
            [0.0, 0.0, -1.0],
            [0.0, 0.0, -1.0],
            [0.0, 0.0, -1.0],
        ];

        // Indices: 2 triangles per face, 6 faces = 36 indices
        // Vertices are ordered CCW, so indices use standard pattern (0,1,2) and (0,2,3)
        let indices: Vec<u32> = vec![
            // +X face (vertices 0-3, ordered CCW from outside)
            0, 1, 2, 0, 2, 3, // -X face (vertices 4-7, ordered CCW from outside)
            4, 5, 6, 4, 6, 7, // +Y face (vertices 8-11, ordered CCW from outside)
            8, 9, 10, 8, 10, 11, // -Y face (vertices 12-15, ordered CCW from outside)
            12, 13, 14, 12, 14, 15, // +Z face (vertices 16-19, ordered CCW from outside)
            16, 17, 18, 16, 18, 19, // -Z face (vertices 20-23, ordered CCW from outside)
            20, 21, 22, 20, 22, 23,
        ];
        (verts, normals, indices)
    }
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct InstanceGpu {
    pub model: [[f32; 4]; 4], // column-major mat4
    pub material: u32,
    pub object_type: u32,
    pub padding: [u32; 2],
}

impl Sphere {
    /// Create an InstanceGpu for this sphere, assigning the provided
    /// material index (index into the renderer's material table).
    pub fn to_instance_with_material(&self, material_index: u32) -> InstanceGpu {
        let translate = glam::Mat4::from_translation(self.center);
        let scale = glam::Mat4::from_scale(glam::Vec3::new(self.radius, self.radius, self.radius));
        let model = translate * scale;
        let cols = model.to_cols_array();
        let mut mat = [[0f32; 4]; 4];
        mat[0] = [cols[0], cols[1], cols[2], cols[3]];
        mat[1] = [cols[4], cols[5], cols[6], cols[7]];
        mat[2] = [cols[8], cols[9], cols[10], cols[11]];
        mat[3] = [cols[12], cols[13], cols[14], cols[15]];
        // Per-instance material parameters will be filled from the
        // Sphere's MaterialType below.
        InstanceGpu {
            model: mat,
            material: material_index,
            object_type: 2u32, // Sphere actor
            padding: [0u32; 2],
        }
    }

    /// Generate a unit-sphere triangle list (non-indexed) with the given
    /// latitude/longitude resolution. Returned vertices are positions on the
    /// unit sphere centered at the origin. Use the `InstanceGpu::model`
    /// matrix to scale/translate per-instance spheres.
    pub fn unit_sphere_vertices(lat_segments: usize, lon_segments: usize) -> Vec<[f32; 3]> {
        let lat = lat_segments.max(2);
        let lon = lon_segments.max(3);
        // Precompute the grid of positions (lat+1) x (lon+1)
        let mut pts: Vec<[f32; 3]> = Vec::with_capacity((lat + 1) * (lon + 1));
        for i in 0..=lat {
            let theta = std::f32::consts::PI * (i as f32) / (lat as f32);
            let sin_theta = theta.sin();
            let cos_theta = theta.cos();
            for j in 0..=lon {
                let phi = 2.0 * std::f32::consts::PI * (j as f32) / (lon as f32);
                let x = sin_theta * phi.cos();
                let y = cos_theta;
                let z = sin_theta * phi.sin();
                pts.push([x, y, z]);
            }
        }

        // Build triangle list (two triangles per quad)
        let mut verts: Vec<[f32; 3]> = Vec::new();
        for i in 0..lat {
            for j in 0..lon {
                let a = i * (lon + 1) + j;
                let b = a + 1;
                let c = a + (lon + 1);
                let d = c + 1;
                // triangle 1: a, c, b
                verts.push(pts[a]);
                verts.push(pts[c]);
                verts.push(pts[b]);
                // triangle 2: b, c, d
                verts.push(pts[b]);
                verts.push(pts[c]);
                verts.push(pts[d]);
            }
        }
        verts
    }

    /// Generate a unit-sphere using indexed triangle lists.
    /// Returns (vertices, indices) where `vertices` is the unique vertex
    /// list and `indices` contains 32-bit triangle indices into `vertices`.
    pub fn unit_sphere_indexed(
        lat_segments: usize,
        lon_segments: usize,
    ) -> (Vec<[f32; 3]>, Vec<[f32; 3]>, Vec<u32>) {
        let lat = lat_segments.max(2);
        let lon = lon_segments.max(3);
        // grid of unique positions (lat+1) x (lon+1)
        let mut pts: Vec<[f32; 3]> = Vec::with_capacity((lat + 1) * (lon + 1));
        for i in 0..=lat {
            let theta = std::f32::consts::PI * (i as f32) / (lat as f32);
            let sin_theta = theta.sin();
            let cos_theta = theta.cos();
            for j in 0..=lon {
                let phi = 2.0 * std::f32::consts::PI * (j as f32) / (lon as f32);
                let x = sin_theta * phi.cos();
                let y = cos_theta;
                let z = sin_theta * phi.sin();
                pts.push([x, y, z]);
            }
        }

        // Build index list (two triangles per quad)
        let mut indices: Vec<u32> = Vec::with_capacity(lat * lon * 6);
        for i in 0..lat {
            for j in 0..lon {
                let a = (i * (lon + 1) + j) as u32;
                let b = a + 1;
                let c = a + (lon + 1) as u32; // careful cast
                let d = c + 1;
                // triangle 1: a, c, b
                indices.push(a);
                indices.push(c);
                indices.push(b);
                // triangle 2: b, c, d
                indices.push(b);
                indices.push(c);
                indices.push(d);
            }
        }
        // For a unit sphere the normal at each vertex equals the position.
        let mut normals: Vec<[f32; 3]> = Vec::with_capacity(pts.len());
        for p in &pts {
            normals.push([p[0], p[1], p[2]]);
        }
        (pts, normals, indices)
    }
}
