//! Marching Cubes implementation for smooth terrain mesh generation.
//!
//! This module implements the Marching Cubes algorithm for extracting isosurfaces
//! from voxel density fields, creating smooth terrain geometry.

use super::VoxelMesh;
use glam::Vec3;

/// Marching Cubes mesh generator for smooth terrain
pub struct MarchingCubes;

impl MarchingCubes {
    /// Offsets of a cell's eight corners from corner 0, in the order the tables index them.
    pub const CORNER_OFFSETS: [[usize; 3]; 8] = [
        [0, 0, 0],
        [1, 0, 0],
        [1, 0, 1],
        [0, 0, 1],
        [0, 1, 0],
        [1, 1, 0],
        [1, 1, 1],
        [0, 1, 1],
    ];

    /// Generate smooth mesh for a chunk using Marching Cubes algorithm.
    ///
    /// # Arguments
    /// * `density_field` - 3D density field where values > 0.5 are solid
    /// * `chunk_size` - Size of the chunk (typically 16x16x16)
    /// * `material_at` - Material of a solid field sample
    /// * `light_at` - Block light and sky exposure of a field sample; each vertex takes
    ///   the light of the air sample of the edge it lies on
    ///
    /// # Returns
    /// A `VoxelMesh` with smooth geometry and computed normals
    pub fn generate_mesh(
        density_field: &[[[f32; 18]; 18]; 18],
        chunk_size: usize,
        material_at: impl Fn([usize; 3]) -> u32,
        light_at: impl Fn([usize; 3]) -> ([u8; 3], bool),
    ) -> VoxelMesh {
        // Marching Cubes implementation currently requires chunk_size=16
        // because density_field is hardcoded to [18][18][18] (16+2 padding)
        assert_eq!(
            chunk_size, 16,
            "MarchingCubes currently only supports chunk_size=16, got {chunk_size}"
        );

        let mut mesh = VoxelMesh::empty();

        // Marching cubes processes cells between density samples
        // density_field is [18][18][18] with samples at indices 0..17
        // Blocks are stored at samples [1..17] (16 blocks)
        // We want to process the 16 cells that cover these blocks
        // Cell at index i uses corners from samples [i] to [i+1]
        // So we process cells 1..17 (16 cells), accessing samples up to [17]
        for x in 1..=chunk_size {
            for y in 1..=chunk_size {
                for z in 1..=chunk_size {
                    Self::process_cell(density_field, x, y, z, &mut mesh, &material_at, &light_at);
                }
            }
        }

        mesh
    }

    /// Triangles of one unit cell for a 0/1 corner pattern, corner 0 at the origin.
    ///
    /// Corner `i` is solid when bit `i` of `cube_index` is set, with the corner order and
    /// winding of [`Self::generate_mesh`]. Indices 0 and 255 yield no triangles.
    pub fn cell_triangles(cube_index: u8) -> impl Iterator<Item = [Vec3; 3]> {
        let corner = |i: usize| Vec3::from_array(Self::CORNER_OFFSETS[i].map(|c| c as f32));
        // With 0/1 densities and iso level 0.5 every edge vertex is the edge midpoint.
        let edge_midpoint = move |edge: u8| {
            let (a, b) = EDGE_CONNECTIONS[usize::from(edge)];
            (corner(a) + corner(b)) * 0.5
        };
        Self::triangle_edges(cube_index).map(move |tri| tri.map(edge_midpoint))
    }

    /// The edges each triangle of `cube_index` puts its three vertices on, from `TRI_TABLE`.
    fn triangle_edges(cube_index: u8) -> impl Iterator<Item = [u8; 3]> {
        TRI_TABLE[usize::from(cube_index)]
            .as_chunks::<3>()
            .0
            .iter()
            .copied()
            .take_while(|tri| tri[0] != 255)
    }

    /// Process a single cell in the marching cubes algorithm
    fn process_cell(
        density_field: &[[[f32; 18]; 18]; 18],
        x: usize,
        y: usize,
        z: usize,
        mesh: &mut VoxelMesh,
        material_at: &impl Fn([usize; 3]) -> u32,
        light_at: &impl Fn([usize; 3]) -> ([u8; 3], bool),
    ) {
        // Get the 8 corner values for this cube
        let corners = [
            density_field[x][y][z],
            density_field[x + 1][y][z],
            density_field[x + 1][y][z + 1],
            density_field[x][y][z + 1],
            density_field[x][y + 1][z],
            density_field[x + 1][y + 1][z],
            density_field[x + 1][y + 1][z + 1],
            density_field[x][y + 1][z + 1],
        ];

        let cube_index = cube_index(density_field, x, y, z);

        // Edges the surface crosses; none when the cube is entirely inside or outside.
        let edge_flags = EDGE_TABLE[cube_index as usize];
        if edge_flags == 0 {
            return;
        }

        // Find vertices along edges where surface crosses
        let mut edge_vertices = [[0.0f32; 3]; 12];
        let mut edge_gradients = [[0.0f32; 3]; 12];
        let mut edge_light = [([0u8; 3], false); 12];
        let iso_level = 0.5;

        for i in 0..12 {
            if edge_flags & (1 << i) != 0 {
                let (v0_idx, v1_idx) = EDGE_CONNECTIONS[i];
                let s0 = Self::corner_sample(x, y, z, v0_idx);
                let s1 = Self::corner_sample(x, y, z, v1_idx);
                let d0 = corners[v0_idx];
                let d1 = corners[v1_idx];

                // A crossed edge has one end above the iso level and one at or below it, so
                // `d1 != d0`.
                let t = (iso_level - d0) / (d1 - d0);

                edge_vertices[i] = lerp3(
                    Self::corner_position(x, y, z, v0_idx),
                    Self::corner_position(x, y, z, v1_idx),
                    t,
                );
                edge_gradients[i] = Self::edge_gradient(density_field, (s0, d0), (s1, d1), t);
                edge_light[i] = light_at(if d0 <= iso_level { s0 } else { s1 });
            }
        }

        // Generate triangles from the triangle table
        for tri_edge_indices in Self::triangle_edges(cube_index) {
            // Record base vertex for this triangle
            let base_vertex = mesh.vertices.len() as u32;

            // Add the three vertices of this triangle
            for edge_idx in tri_edge_indices {
                let vert = edge_vertices[edge_idx as usize];
                mesh.vertices.push(vert);

                mesh.normals
                    .push(Self::outward_normal(edge_gradients[edge_idx as usize]));

                let (block_light, sky_exposed) = edge_light[edge_idx as usize];
                mesh.push_light(block_light, sky_exposed);

                // Smooth terrain doesn't need complex AO - use density as simple approximation
                // Lower density (near surface) = less occlusion
                let density = Self::sample_density(density_field, vert[0], vert[1], vert[2]);
                let ao = 0.6 + (1.0 - density) * 0.4; // Range [0.6, 1.0]
                mesh.ambient_occlusion.push(ao);

                // Mark as smooth terrain (0 = smooth, 1 = blocky)
                mesh.geometry_type.push(0);
            }

            let centroid = tri_edge_indices
                .map(|edge| Vec3::from(edge_vertices[edge as usize]))
                .into_iter()
                .sum::<Vec3>()
                / 3.0
                - Vec3::new(x as f32, y as f32, z as f32);
            let corner = nearest_solid_corner(cube_index, centroid);
            let face_material = material_at(Self::corner_sample(x, y, z, corner));
            mesh.material.extend([face_material; 3]);

            // Add indices (counter-clockwise winding)
            mesh.indices
                .extend([base_vertex, base_vertex + 1, base_vertex + 2]);
        }
    }

    /// Sample density at a specific position using trilinear interpolation
    fn sample_density(field: &[[[f32; 18]; 18]; 18], x: f32, y: f32, z: f32) -> f32 {
        let x0 = x.floor() as usize;
        let y0 = y.floor() as usize;
        let z0 = z.floor() as usize;
        let x1 = (x0 + 1).min(17);
        let y1 = (y0 + 1).min(17);
        let z1 = (z0 + 1).min(17);

        let fx = x - x0 as f32;
        let fy = y - y0 as f32;
        let fz = z - z0 as f32;

        // Trilinear interpolation
        let c000 = field[x0][y0][z0];
        let c001 = field[x0][y0][z1];
        let c010 = field[x0][y1][z0];
        let c011 = field[x0][y1][z1];
        let c100 = field[x1][y0][z0];
        let c101 = field[x1][y0][z1];
        let c110 = field[x1][y1][z0];
        let c111 = field[x1][y1][z1];

        let c00 = c000 * (1.0 - fx) + c100 * fx;
        let c01 = c001 * (1.0 - fx) + c101 * fx;
        let c10 = c010 * (1.0 - fx) + c110 * fx;
        let c11 = c011 * (1.0 - fx) + c111 * fx;

        let c0 = c00 * (1.0 - fy) + c10 * fy;
        let c1 = c01 * (1.0 - fy) + c11 * fy;

        c0 * (1.0 - fz) + c1 * fz
    }

    /// Get position of a cube corner
    fn corner_position(x: usize, y: usize, z: usize, corner_idx: usize) -> [f32; 3] {
        Self::corner_sample(x, y, z, corner_idx).map(|c| c as f32)
    }

    pub(crate) fn corner_sample(x: usize, y: usize, z: usize, corner_idx: usize) -> [usize; 3] {
        let [dx, dy, dz] = Self::CORNER_OFFSETS[corner_idx];
        [x + dx, y + dy, z + dz]
    }

    /// Density gradient at an integer sample: central differences, one-sided at the
    /// field's ends so the outermost meshed layer still gets a gradient.
    fn sample_gradient(field: &[[[f32; 18]; 18]; 18], [x, y, z]: [usize; 3]) -> [f32; 3] {
        let diff = |lo: [usize; 3], hi: [usize; 3], span: f32| {
            (field[hi[0]][hi[1]][hi[2]] - field[lo[0]][lo[1]][lo[2]]) / span
        };
        let (xl, xh) = (x.saturating_sub(1), (x + 1).min(17));
        let (yl, yh) = (y.saturating_sub(1), (y + 1).min(17));
        let (zl, zh) = (z.saturating_sub(1), (z + 1).min(17));

        [
            diff([xl, y, z], [xh, y, z], (xh - xl) as f32),
            diff([x, yl, z], [x, yh, z], (yh - yl) as f32),
            diff([x, y, zl], [x, y, zh], (zh - zl) as f32),
        ]
    }

    /// Density gradient at a vertex `t` along the edge from `s0` to `s1`, interpolated from
    /// the endpoint gradients. When both endpoints sit on flat neighbourhoods (a solid one
    /// sample from another, say) it falls back to the difference along the edge itself,
    /// which is never zero because the surface crosses it.
    fn edge_gradient(
        field: &[[[f32; 18]; 18]; 18],
        (s0, d0): ([usize; 3], f32),
        (s1, d1): ([usize; 3], f32),
        t: f32,
    ) -> [f32; 3] {
        let gradient = lerp3(
            Self::sample_gradient(field, s0),
            Self::sample_gradient(field, s1),
            t,
        );
        if gradient == [0.0; 3] {
            std::array::from_fn(|axis| (s1[axis] as f32 - s0[axis] as f32) * (d1 - d0))
        } else {
            gradient
        }
    }

    /// Outward unit normal from a non-zero density gradient.
    fn outward_normal(gradient: [f32; 3]) -> [f32; 3] {
        let len = gradient.iter().map(|g| g * g).sum::<f32>().sqrt();
        // Density rises into the solid, so outward is the negated gradient.
        gradient.map(|g| -g / len)
    }
}

/// Bit `i` is set when corner `i` (in `MarchingCubes::CORNER_OFFSETS` order) of the cell
/// at field index `(x, y, z)` is inside the surface, that is, denser than 0.5.
pub(crate) fn cube_index(
    density_field: &[[[f32; 18]; 18]; 18],
    x: usize,
    y: usize,
    z: usize,
) -> u8 {
    MarchingCubes::CORNER_OFFSETS
        .iter()
        .enumerate()
        .filter(|(_, [dx, dy, dz])| density_field[x + dx][y + dy][z + dz] > 0.5)
        .fold(0u8, |index, (corner, _)| index | 1 << corner)
}

/// The corner of a cell (in `MarchingCubes::CORNER_OFFSETS` order) that is solid in
/// `cube_index` and nearest `centroid`, given in cell coordinates. Ties go to the lowest
/// corner index. `cube_index` must have a solid corner.
pub(crate) fn nearest_solid_corner(cube_index: u8, centroid: Vec3) -> usize {
    let mut best = (f32::MAX, 0);
    for k in 0..8 {
        if cube_index & (1 << k) != 0 {
            let d = Vec3::from_array(MarchingCubes::CORNER_OFFSETS[k].map(|c| c as f32))
                .distance_squared(centroid);
            if d < best.0 {
                best = (d, k);
            }
        }
    }
    best.1
}

fn lerp3(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [
        a[0] + t * (b[0] - a[0]),
        a[1] + t * (b[1] - a[1]),
        a[2] + t * (b[2] - a[2]),
    ]
}

// Edge connections: which corners does each edge connect?
const EDGE_CONNECTIONS: [(usize, usize); 12] = [
    (0, 1),
    (1, 2),
    (2, 3),
    (3, 0),
    (4, 5),
    (5, 6),
    (6, 7),
    (7, 4),
    (0, 4),
    (1, 5),
    (2, 6),
    (3, 7),
];

// Edge table: which edges intersect the surface for each cube configuration
// This is a compact representation - full table would be 256 entries
const EDGE_TABLE: [u16; 256] = [
    0x0, 0x109, 0x203, 0x30a, 0x406, 0x50f, 0x605, 0x70c, 0x80c, 0x905, 0xa0f, 0xb06, 0xc0a, 0xd03,
    0xe09, 0xf00, 0x190, 0x99, 0x393, 0x29a, 0x596, 0x49f, 0x795, 0x69c, 0x99c, 0x895, 0xb9f,
    0xa96, 0xd9a, 0xc93, 0xf99, 0xe90, 0x230, 0x339, 0x33, 0x13a, 0x636, 0x73f, 0x435, 0x53c,
    0xa3c, 0xb35, 0x83f, 0x936, 0xe3a, 0xf33, 0xc39, 0xd30, 0x3a0, 0x2a9, 0x1a3, 0xaa, 0x7a6,
    0x6af, 0x5a5, 0x4ac, 0xbac, 0xaa5, 0x9af, 0x8a6, 0xfaa, 0xea3, 0xda9, 0xca0, 0x460, 0x569,
    0x663, 0x76a, 0x66, 0x16f, 0x265, 0x36c, 0xc6c, 0xd65, 0xe6f, 0xf66, 0x86a, 0x963, 0xa69,
    0xb60, 0x5f0, 0x4f9, 0x7f3, 0x6fa, 0x1f6, 0xff, 0x3f5, 0x2fc, 0xdfc, 0xcf5, 0xfff, 0xef6,
    0x9fa, 0x8f3, 0xbf9, 0xaf0, 0x650, 0x759, 0x453, 0x55a, 0x256, 0x35f, 0x55, 0x15c, 0xe5c,
    0xf55, 0xc5f, 0xd56, 0xa5a, 0xb53, 0x859, 0x950, 0x7c0, 0x6c9, 0x5c3, 0x4ca, 0x3c6, 0x2cf,
    0x1c5, 0xcc, 0xfcc, 0xec5, 0xdcf, 0xcc6, 0xbca, 0xac3, 0x9c9, 0x8c0, 0x8c0, 0x9c9, 0xac3,
    0xbca, 0xcc6, 0xdcf, 0xec5, 0xfcc, 0xcc, 0x1c5, 0x2cf, 0x3c6, 0x4ca, 0x5c3, 0x6c9, 0x7c0,
    0x950, 0x859, 0xb53, 0xa5a, 0xd56, 0xc5f, 0xf55, 0xe5c, 0x15c, 0x55, 0x35f, 0x256, 0x55a,
    0x453, 0x759, 0x650, 0xaf0, 0xbf9, 0x8f3, 0x9fa, 0xef6, 0xfff, 0xcf5, 0xdfc, 0x2fc, 0x3f5,
    0xff, 0x1f6, 0x6fa, 0x7f3, 0x4f9, 0x5f0, 0xb60, 0xa69, 0x963, 0x86a, 0xf66, 0xe6f, 0xd65,
    0xc6c, 0x36c, 0x265, 0x16f, 0x66, 0x76a, 0x663, 0x569, 0x460, 0xca0, 0xda9, 0xea3, 0xfaa,
    0x8a6, 0x9af, 0xaa5, 0xbac, 0x4ac, 0x5a5, 0x6af, 0x7a6, 0xaa, 0x1a3, 0x2a9, 0x3a0, 0xd30,
    0xc39, 0xf33, 0xe3a, 0x936, 0x83f, 0xb35, 0xa3c, 0x53c, 0x435, 0x73f, 0x636, 0x13a, 0x33,
    0x339, 0x230, 0xe90, 0xf99, 0xc93, 0xd9a, 0xa96, 0xb9f, 0x895, 0x99c, 0x69c, 0x795, 0x49f,
    0x596, 0x29a, 0x393, 0x99, 0x190, 0xf00, 0xe09, 0xd03, 0xc0a, 0xb06, 0xa0f, 0x905, 0x80c,
    0x70c, 0x605, 0x50f, 0x406, 0x30a, 0x203, 0x109, 0x0,
];

// Triangle table: which triangles to generate for each cube configuration
// Each entry lists edge indices that form triangles (255 = end marker)
// This is the standard Marching Cubes triangle table
const TRI_TABLE: [[u8; 16]; 256] = [
    [
        255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
    ],
    [
        0, 8, 3, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
    ],
    [
        0, 1, 9, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
    ],
    [
        1, 8, 3, 9, 8, 1, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
    ],
    [
        1, 2, 10, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
    ],
    [
        0, 8, 3, 1, 2, 10, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
    ],
    [
        9, 2, 10, 0, 2, 9, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
    ],
    [
        2, 8, 3, 2, 10, 8, 10, 9, 8, 255, 255, 255, 255, 255, 255, 255,
    ],
    [
        3, 11, 2, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
    ],
    [
        0, 11, 2, 8, 11, 0, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
    ],
    [
        1, 9, 0, 2, 3, 11, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
    ],
    [
        1, 11, 2, 1, 9, 11, 9, 8, 11, 255, 255, 255, 255, 255, 255, 255,
    ],
    [
        3, 10, 1, 11, 10, 3, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
    ],
    [
        0, 10, 1, 0, 8, 10, 8, 11, 10, 255, 255, 255, 255, 255, 255, 255,
    ],
    [
        3, 9, 0, 3, 11, 9, 11, 10, 9, 255, 255, 255, 255, 255, 255, 255,
    ],
    [
        9, 8, 10, 10, 8, 11, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
    ],
    [
        4, 7, 8, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
    ],
    [
        4, 3, 0, 7, 3, 4, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
    ],
    [
        0, 1, 9, 8, 4, 7, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
    ],
    [4, 1, 9, 4, 7, 1, 7, 3, 1, 255, 255, 255, 255, 255, 255, 255],
    [
        1, 2, 10, 8, 4, 7, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
    ],
    [
        3, 4, 7, 3, 0, 4, 1, 2, 10, 255, 255, 255, 255, 255, 255, 255,
    ],
    [
        9, 2, 10, 9, 0, 2, 8, 4, 7, 255, 255, 255, 255, 255, 255, 255,
    ],
    [2, 10, 9, 2, 9, 7, 2, 7, 3, 7, 9, 4, 255, 255, 255, 255],
    [
        8, 4, 7, 3, 11, 2, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
    ],
    [
        11, 4, 7, 11, 2, 4, 2, 0, 4, 255, 255, 255, 255, 255, 255, 255,
    ],
    [
        9, 0, 1, 8, 4, 7, 2, 3, 11, 255, 255, 255, 255, 255, 255, 255,
    ],
    [4, 7, 11, 9, 4, 11, 9, 11, 2, 9, 2, 1, 255, 255, 255, 255],
    [
        3, 10, 1, 3, 11, 10, 7, 8, 4, 255, 255, 255, 255, 255, 255, 255,
    ],
    [1, 11, 10, 1, 4, 11, 1, 0, 4, 7, 11, 4, 255, 255, 255, 255],
    [4, 7, 8, 9, 0, 11, 9, 11, 10, 11, 0, 3, 255, 255, 255, 255],
    [
        4, 7, 11, 4, 11, 9, 9, 11, 10, 255, 255, 255, 255, 255, 255, 255,
    ],
    [
        9, 5, 4, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
    ],
    [
        9, 5, 4, 0, 8, 3, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
    ],
    [
        0, 5, 4, 1, 5, 0, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
    ],
    [8, 5, 4, 8, 3, 5, 3, 1, 5, 255, 255, 255, 255, 255, 255, 255],
    [
        1, 2, 10, 9, 5, 4, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
    ],
    [
        3, 0, 8, 1, 2, 10, 4, 9, 5, 255, 255, 255, 255, 255, 255, 255,
    ],
    [
        5, 2, 10, 5, 4, 2, 4, 0, 2, 255, 255, 255, 255, 255, 255, 255,
    ],
    [2, 10, 5, 3, 2, 5, 3, 5, 4, 3, 4, 8, 255, 255, 255, 255],
    [
        9, 5, 4, 2, 3, 11, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
    ],
    [
        0, 11, 2, 0, 8, 11, 4, 9, 5, 255, 255, 255, 255, 255, 255, 255,
    ],
    [
        0, 5, 4, 0, 1, 5, 2, 3, 11, 255, 255, 255, 255, 255, 255, 255,
    ],
    [2, 1, 5, 2, 5, 8, 2, 8, 11, 4, 8, 5, 255, 255, 255, 255],
    [
        10, 3, 11, 10, 1, 3, 9, 5, 4, 255, 255, 255, 255, 255, 255, 255,
    ],
    [4, 9, 5, 0, 8, 1, 8, 10, 1, 8, 11, 10, 255, 255, 255, 255],
    [5, 4, 0, 5, 0, 11, 5, 11, 10, 11, 0, 3, 255, 255, 255, 255],
    [
        5, 4, 8, 5, 8, 10, 10, 8, 11, 255, 255, 255, 255, 255, 255, 255,
    ],
    [
        9, 7, 8, 5, 7, 9, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
    ],
    [9, 3, 0, 9, 5, 3, 5, 7, 3, 255, 255, 255, 255, 255, 255, 255],
    [0, 7, 8, 0, 1, 7, 1, 5, 7, 255, 255, 255, 255, 255, 255, 255],
    [
        1, 5, 3, 3, 5, 7, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
    ],
    [
        9, 7, 8, 9, 5, 7, 10, 1, 2, 255, 255, 255, 255, 255, 255, 255,
    ],
    [10, 1, 2, 9, 5, 0, 5, 3, 0, 5, 7, 3, 255, 255, 255, 255],
    [8, 0, 2, 8, 2, 5, 8, 5, 7, 10, 5, 2, 255, 255, 255, 255],
    [
        2, 10, 5, 2, 5, 3, 3, 5, 7, 255, 255, 255, 255, 255, 255, 255,
    ],
    [
        7, 9, 5, 7, 8, 9, 3, 11, 2, 255, 255, 255, 255, 255, 255, 255,
    ],
    [9, 5, 7, 9, 7, 2, 9, 2, 0, 2, 7, 11, 255, 255, 255, 255],
    [2, 3, 11, 0, 1, 8, 1, 7, 8, 1, 5, 7, 255, 255, 255, 255],
    [
        11, 2, 1, 11, 1, 7, 7, 1, 5, 255, 255, 255, 255, 255, 255, 255,
    ],
    [9, 5, 8, 8, 5, 7, 10, 1, 3, 10, 3, 11, 255, 255, 255, 255],
    [5, 7, 0, 5, 0, 9, 7, 11, 0, 1, 0, 10, 11, 10, 0, 255],
    [11, 10, 0, 11, 0, 3, 10, 5, 0, 8, 0, 7, 5, 7, 0, 255],
    [
        11, 10, 5, 7, 11, 5, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
    ],
    [
        10, 6, 5, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
    ],
    [
        0, 8, 3, 5, 10, 6, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
    ],
    [
        9, 0, 1, 5, 10, 6, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
    ],
    [
        1, 8, 3, 1, 9, 8, 5, 10, 6, 255, 255, 255, 255, 255, 255, 255,
    ],
    [
        1, 6, 5, 2, 6, 1, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
    ],
    [1, 6, 5, 1, 2, 6, 3, 0, 8, 255, 255, 255, 255, 255, 255, 255],
    [9, 6, 5, 9, 0, 6, 0, 2, 6, 255, 255, 255, 255, 255, 255, 255],
    [5, 9, 8, 5, 8, 2, 5, 2, 6, 3, 2, 8, 255, 255, 255, 255],
    [
        2, 3, 11, 10, 6, 5, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
    ],
    [
        11, 0, 8, 11, 2, 0, 10, 6, 5, 255, 255, 255, 255, 255, 255, 255,
    ],
    [
        0, 1, 9, 2, 3, 11, 5, 10, 6, 255, 255, 255, 255, 255, 255, 255,
    ],
    [5, 10, 6, 1, 9, 2, 9, 11, 2, 9, 8, 11, 255, 255, 255, 255],
    [
        6, 3, 11, 6, 5, 3, 5, 1, 3, 255, 255, 255, 255, 255, 255, 255,
    ],
    [0, 8, 11, 0, 11, 5, 0, 5, 1, 5, 11, 6, 255, 255, 255, 255],
    [3, 11, 6, 0, 3, 6, 0, 6, 5, 0, 5, 9, 255, 255, 255, 255],
    [
        6, 5, 9, 6, 9, 11, 11, 9, 8, 255, 255, 255, 255, 255, 255, 255,
    ],
    [
        5, 10, 6, 4, 7, 8, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
    ],
    [
        4, 3, 0, 4, 7, 3, 6, 5, 10, 255, 255, 255, 255, 255, 255, 255,
    ],
    [
        1, 9, 0, 5, 10, 6, 8, 4, 7, 255, 255, 255, 255, 255, 255, 255,
    ],
    [10, 6, 5, 1, 9, 7, 1, 7, 3, 7, 9, 4, 255, 255, 255, 255],
    [6, 1, 2, 6, 5, 1, 4, 7, 8, 255, 255, 255, 255, 255, 255, 255],
    [1, 2, 5, 5, 2, 6, 3, 0, 4, 3, 4, 7, 255, 255, 255, 255],
    [8, 4, 7, 9, 0, 5, 0, 6, 5, 0, 2, 6, 255, 255, 255, 255],
    [7, 3, 9, 7, 9, 4, 3, 2, 9, 5, 9, 6, 2, 6, 9, 255],
    [
        3, 11, 2, 7, 8, 4, 10, 6, 5, 255, 255, 255, 255, 255, 255, 255,
    ],
    [5, 10, 6, 4, 7, 2, 4, 2, 0, 2, 7, 11, 255, 255, 255, 255],
    [0, 1, 9, 4, 7, 8, 2, 3, 11, 5, 10, 6, 255, 255, 255, 255],
    [9, 2, 1, 9, 11, 2, 9, 4, 11, 7, 11, 4, 5, 10, 6, 255],
    [8, 4, 7, 3, 11, 5, 3, 5, 1, 5, 11, 6, 255, 255, 255, 255],
    [5, 1, 11, 5, 11, 6, 1, 0, 11, 7, 11, 4, 0, 4, 11, 255],
    [0, 5, 9, 0, 6, 5, 0, 3, 6, 11, 6, 3, 8, 4, 7, 255],
    [6, 5, 9, 6, 9, 11, 4, 7, 9, 7, 11, 9, 255, 255, 255, 255],
    [
        10, 4, 9, 6, 4, 10, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
    ],
    [
        4, 10, 6, 4, 9, 10, 0, 8, 3, 255, 255, 255, 255, 255, 255, 255,
    ],
    [
        10, 0, 1, 10, 6, 0, 6, 4, 0, 255, 255, 255, 255, 255, 255, 255,
    ],
    [8, 3, 1, 8, 1, 6, 8, 6, 4, 6, 1, 10, 255, 255, 255, 255],
    [1, 4, 9, 1, 2, 4, 2, 6, 4, 255, 255, 255, 255, 255, 255, 255],
    [3, 0, 8, 1, 2, 9, 2, 4, 9, 2, 6, 4, 255, 255, 255, 255],
    [
        0, 2, 4, 4, 2, 6, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
    ],
    [8, 3, 2, 8, 2, 4, 4, 2, 6, 255, 255, 255, 255, 255, 255, 255],
    [
        10, 4, 9, 10, 6, 4, 11, 2, 3, 255, 255, 255, 255, 255, 255, 255,
    ],
    [0, 8, 2, 2, 8, 11, 4, 9, 10, 4, 10, 6, 255, 255, 255, 255],
    [3, 11, 2, 0, 1, 6, 0, 6, 4, 6, 1, 10, 255, 255, 255, 255],
    [6, 4, 1, 6, 1, 10, 4, 8, 1, 2, 1, 11, 8, 11, 1, 255],
    [9, 6, 4, 9, 3, 6, 9, 1, 3, 11, 6, 3, 255, 255, 255, 255],
    [8, 11, 1, 8, 1, 0, 11, 6, 1, 9, 1, 4, 6, 4, 1, 255],
    [
        3, 11, 6, 3, 6, 0, 0, 6, 4, 255, 255, 255, 255, 255, 255, 255,
    ],
    [
        6, 4, 8, 11, 6, 8, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
    ],
    [
        7, 10, 6, 7, 8, 10, 8, 9, 10, 255, 255, 255, 255, 255, 255, 255,
    ],
    [0, 7, 3, 0, 10, 7, 0, 9, 10, 6, 7, 10, 255, 255, 255, 255],
    [10, 6, 7, 1, 10, 7, 1, 7, 8, 1, 8, 0, 255, 255, 255, 255],
    [
        10, 6, 7, 10, 7, 1, 1, 7, 3, 255, 255, 255, 255, 255, 255, 255,
    ],
    [1, 2, 6, 1, 6, 8, 1, 8, 9, 8, 6, 7, 255, 255, 255, 255],
    [2, 6, 9, 2, 9, 1, 6, 7, 9, 0, 9, 3, 7, 3, 9, 255],
    [7, 8, 0, 7, 0, 6, 6, 0, 2, 255, 255, 255, 255, 255, 255, 255],
    [
        7, 3, 2, 6, 7, 2, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
    ],
    [2, 3, 11, 10, 6, 8, 10, 8, 9, 8, 6, 7, 255, 255, 255, 255],
    [2, 0, 7, 2, 7, 11, 0, 9, 7, 6, 7, 10, 9, 10, 7, 255],
    [1, 8, 0, 1, 7, 8, 1, 10, 7, 6, 7, 10, 2, 3, 11, 255],
    [11, 2, 1, 11, 1, 7, 10, 6, 1, 6, 7, 1, 255, 255, 255, 255],
    [8, 9, 6, 8, 6, 7, 9, 1, 6, 11, 6, 3, 1, 3, 6, 255],
    [
        0, 9, 1, 11, 6, 7, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
    ],
    [7, 8, 0, 7, 0, 6, 3, 11, 0, 11, 6, 0, 255, 255, 255, 255],
    [
        7, 11, 6, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
    ],
    [
        7, 6, 11, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
    ],
    [
        3, 0, 8, 11, 7, 6, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
    ],
    [
        0, 1, 9, 11, 7, 6, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
    ],
    [
        8, 1, 9, 8, 3, 1, 11, 7, 6, 255, 255, 255, 255, 255, 255, 255,
    ],
    [
        10, 1, 2, 6, 11, 7, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
    ],
    [
        1, 2, 10, 3, 0, 8, 6, 11, 7, 255, 255, 255, 255, 255, 255, 255,
    ],
    [
        2, 9, 0, 2, 10, 9, 6, 11, 7, 255, 255, 255, 255, 255, 255, 255,
    ],
    [6, 11, 7, 2, 10, 3, 10, 8, 3, 10, 9, 8, 255, 255, 255, 255],
    [
        7, 2, 3, 6, 2, 7, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
    ],
    [7, 0, 8, 7, 6, 0, 6, 2, 0, 255, 255, 255, 255, 255, 255, 255],
    [2, 7, 6, 2, 3, 7, 0, 1, 9, 255, 255, 255, 255, 255, 255, 255],
    [1, 6, 2, 1, 8, 6, 1, 9, 8, 8, 7, 6, 255, 255, 255, 255],
    [
        10, 7, 6, 10, 1, 7, 1, 3, 7, 255, 255, 255, 255, 255, 255, 255,
    ],
    [10, 7, 6, 1, 7, 10, 1, 8, 7, 1, 0, 8, 255, 255, 255, 255],
    [0, 3, 7, 0, 7, 10, 0, 10, 9, 6, 10, 7, 255, 255, 255, 255],
    [
        7, 6, 10, 7, 10, 8, 8, 10, 9, 255, 255, 255, 255, 255, 255, 255,
    ],
    [
        6, 8, 4, 11, 8, 6, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
    ],
    [
        3, 6, 11, 3, 0, 6, 0, 4, 6, 255, 255, 255, 255, 255, 255, 255,
    ],
    [
        8, 6, 11, 8, 4, 6, 9, 0, 1, 255, 255, 255, 255, 255, 255, 255,
    ],
    [9, 4, 6, 9, 6, 3, 9, 3, 1, 11, 3, 6, 255, 255, 255, 255],
    [
        6, 8, 4, 6, 11, 8, 2, 10, 1, 255, 255, 255, 255, 255, 255, 255,
    ],
    [1, 2, 10, 3, 0, 11, 0, 6, 11, 0, 4, 6, 255, 255, 255, 255],
    [4, 11, 8, 4, 6, 11, 0, 2, 9, 2, 10, 9, 255, 255, 255, 255],
    [10, 9, 3, 10, 3, 2, 9, 4, 3, 11, 3, 6, 4, 6, 3, 255],
    [8, 2, 3, 8, 4, 2, 4, 6, 2, 255, 255, 255, 255, 255, 255, 255],
    [
        0, 4, 2, 4, 6, 2, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
    ],
    [1, 9, 0, 2, 3, 4, 2, 4, 6, 4, 3, 8, 255, 255, 255, 255],
    [1, 9, 4, 1, 4, 2, 2, 4, 6, 255, 255, 255, 255, 255, 255, 255],
    [8, 1, 3, 8, 6, 1, 8, 4, 6, 6, 10, 1, 255, 255, 255, 255],
    [
        10, 1, 0, 10, 0, 6, 6, 0, 4, 255, 255, 255, 255, 255, 255, 255,
    ],
    [4, 6, 3, 4, 3, 8, 6, 10, 3, 0, 3, 9, 10, 9, 3, 255],
    [
        10, 9, 4, 6, 10, 4, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
    ],
    [
        4, 9, 5, 7, 6, 11, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
    ],
    [
        0, 8, 3, 4, 9, 5, 11, 7, 6, 255, 255, 255, 255, 255, 255, 255,
    ],
    [
        5, 0, 1, 5, 4, 0, 7, 6, 11, 255, 255, 255, 255, 255, 255, 255,
    ],
    [11, 7, 6, 8, 3, 4, 3, 5, 4, 3, 1, 5, 255, 255, 255, 255],
    [
        9, 5, 4, 10, 1, 2, 7, 6, 11, 255, 255, 255, 255, 255, 255, 255,
    ],
    [6, 11, 7, 1, 2, 10, 0, 8, 3, 4, 9, 5, 255, 255, 255, 255],
    [7, 6, 11, 5, 4, 10, 4, 2, 10, 4, 0, 2, 255, 255, 255, 255],
    [3, 4, 8, 3, 5, 4, 3, 2, 5, 10, 5, 2, 11, 7, 6, 255],
    [7, 2, 3, 7, 6, 2, 5, 4, 9, 255, 255, 255, 255, 255, 255, 255],
    [9, 5, 4, 0, 8, 6, 0, 6, 2, 6, 8, 7, 255, 255, 255, 255],
    [3, 6, 2, 3, 7, 6, 1, 5, 0, 5, 4, 0, 255, 255, 255, 255],
    [6, 2, 8, 6, 8, 7, 2, 1, 8, 4, 8, 5, 1, 5, 8, 255],
    [9, 5, 4, 10, 1, 6, 1, 7, 6, 1, 3, 7, 255, 255, 255, 255],
    [1, 6, 10, 1, 7, 6, 1, 0, 7, 8, 7, 0, 9, 5, 4, 255],
    [4, 0, 10, 4, 10, 5, 0, 3, 10, 6, 10, 7, 3, 7, 10, 255],
    [7, 6, 10, 7, 10, 8, 5, 4, 10, 4, 8, 10, 255, 255, 255, 255],
    [
        6, 9, 5, 6, 11, 9, 11, 8, 9, 255, 255, 255, 255, 255, 255, 255,
    ],
    [3, 6, 11, 0, 6, 3, 0, 5, 6, 0, 9, 5, 255, 255, 255, 255],
    [0, 11, 8, 0, 5, 11, 0, 1, 5, 5, 6, 11, 255, 255, 255, 255],
    [
        6, 11, 3, 6, 3, 5, 5, 3, 1, 255, 255, 255, 255, 255, 255, 255,
    ],
    [1, 2, 10, 9, 5, 11, 9, 11, 8, 11, 5, 6, 255, 255, 255, 255],
    [0, 11, 3, 0, 6, 11, 0, 9, 6, 5, 6, 9, 1, 2, 10, 255],
    [11, 8, 5, 11, 5, 6, 8, 0, 5, 10, 5, 2, 0, 2, 5, 255],
    [6, 11, 3, 6, 3, 5, 2, 10, 3, 10, 5, 3, 255, 255, 255, 255],
    [5, 8, 9, 5, 2, 8, 5, 6, 2, 3, 8, 2, 255, 255, 255, 255],
    [9, 5, 6, 9, 6, 0, 0, 6, 2, 255, 255, 255, 255, 255, 255, 255],
    [1, 5, 8, 1, 8, 0, 5, 6, 8, 3, 8, 2, 6, 2, 8, 255],
    [
        1, 5, 6, 2, 1, 6, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
    ],
    [1, 3, 6, 1, 6, 10, 3, 8, 6, 5, 6, 9, 8, 9, 6, 255],
    [10, 1, 0, 10, 0, 6, 9, 5, 0, 5, 6, 0, 255, 255, 255, 255],
    [
        0, 3, 8, 5, 6, 10, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
    ],
    [
        10, 5, 6, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
    ],
    [
        11, 5, 10, 7, 5, 11, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
    ],
    [
        11, 5, 10, 11, 7, 5, 8, 3, 0, 255, 255, 255, 255, 255, 255, 255,
    ],
    [
        5, 11, 7, 5, 10, 11, 1, 9, 0, 255, 255, 255, 255, 255, 255, 255,
    ],
    [10, 7, 5, 10, 11, 7, 9, 8, 1, 8, 3, 1, 255, 255, 255, 255],
    [
        11, 1, 2, 11, 7, 1, 7, 5, 1, 255, 255, 255, 255, 255, 255, 255,
    ],
    [0, 8, 3, 1, 2, 7, 1, 7, 5, 7, 2, 11, 255, 255, 255, 255],
    [9, 7, 5, 9, 2, 7, 9, 0, 2, 2, 11, 7, 255, 255, 255, 255],
    [7, 5, 2, 7, 2, 11, 5, 9, 2, 3, 2, 8, 9, 8, 2, 255],
    [
        2, 5, 10, 2, 3, 5, 3, 7, 5, 255, 255, 255, 255, 255, 255, 255,
    ],
    [8, 2, 0, 8, 5, 2, 8, 7, 5, 10, 2, 5, 255, 255, 255, 255],
    [9, 0, 1, 5, 10, 3, 5, 3, 7, 3, 10, 2, 255, 255, 255, 255],
    [9, 8, 2, 9, 2, 1, 8, 7, 2, 10, 2, 5, 7, 5, 2, 255],
    [
        1, 3, 5, 3, 7, 5, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
    ],
    [0, 8, 7, 0, 7, 1, 1, 7, 5, 255, 255, 255, 255, 255, 255, 255],
    [9, 0, 3, 9, 3, 5, 5, 3, 7, 255, 255, 255, 255, 255, 255, 255],
    [
        9, 8, 7, 5, 9, 7, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
    ],
    [
        5, 8, 4, 5, 10, 8, 10, 11, 8, 255, 255, 255, 255, 255, 255, 255,
    ],
    [5, 0, 4, 5, 11, 0, 5, 10, 11, 11, 3, 0, 255, 255, 255, 255],
    [0, 1, 9, 8, 4, 10, 8, 10, 11, 10, 4, 5, 255, 255, 255, 255],
    [10, 11, 4, 10, 4, 5, 11, 3, 4, 9, 4, 1, 3, 1, 4, 255],
    [2, 5, 1, 2, 8, 5, 2, 11, 8, 4, 5, 8, 255, 255, 255, 255],
    [0, 4, 11, 0, 11, 3, 4, 5, 11, 2, 11, 1, 5, 1, 11, 255],
    [0, 2, 5, 0, 5, 9, 2, 11, 5, 4, 5, 8, 11, 8, 5, 255],
    [
        9, 4, 5, 2, 11, 3, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
    ],
    [2, 5, 10, 3, 5, 2, 3, 4, 5, 3, 8, 4, 255, 255, 255, 255],
    [
        5, 10, 2, 5, 2, 4, 4, 2, 0, 255, 255, 255, 255, 255, 255, 255,
    ],
    [3, 10, 2, 3, 5, 10, 3, 8, 5, 4, 5, 8, 0, 1, 9, 255],
    [5, 10, 2, 5, 2, 4, 1, 9, 2, 9, 4, 2, 255, 255, 255, 255],
    [8, 4, 5, 8, 5, 3, 3, 5, 1, 255, 255, 255, 255, 255, 255, 255],
    [
        0, 4, 5, 1, 0, 5, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
    ],
    [8, 4, 5, 8, 5, 3, 9, 0, 5, 0, 3, 5, 255, 255, 255, 255],
    [
        9, 4, 5, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
    ],
    [
        4, 11, 7, 4, 9, 11, 9, 10, 11, 255, 255, 255, 255, 255, 255, 255,
    ],
    [0, 8, 3, 4, 9, 7, 9, 11, 7, 9, 10, 11, 255, 255, 255, 255],
    [1, 10, 11, 1, 11, 4, 1, 4, 0, 7, 4, 11, 255, 255, 255, 255],
    [3, 1, 4, 3, 4, 8, 1, 10, 4, 7, 4, 11, 10, 11, 4, 255],
    [4, 11, 7, 9, 11, 4, 9, 2, 11, 9, 1, 2, 255, 255, 255, 255],
    [9, 7, 4, 9, 11, 7, 9, 1, 11, 2, 11, 1, 0, 8, 3, 255],
    [
        11, 7, 4, 11, 4, 2, 2, 4, 0, 255, 255, 255, 255, 255, 255, 255,
    ],
    [11, 7, 4, 11, 4, 2, 8, 3, 4, 3, 2, 4, 255, 255, 255, 255],
    [2, 9, 10, 2, 7, 9, 2, 3, 7, 7, 4, 9, 255, 255, 255, 255],
    [9, 10, 7, 9, 7, 4, 10, 2, 7, 8, 7, 0, 2, 0, 7, 255],
    [3, 7, 10, 3, 10, 2, 7, 4, 10, 1, 10, 0, 4, 0, 10, 255],
    [
        1, 10, 2, 8, 7, 4, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
    ],
    [4, 9, 1, 4, 1, 7, 7, 1, 3, 255, 255, 255, 255, 255, 255, 255],
    [4, 9, 1, 4, 1, 7, 0, 8, 1, 8, 7, 1, 255, 255, 255, 255],
    [
        4, 0, 3, 7, 4, 3, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
    ],
    [
        4, 8, 7, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
    ],
    [
        9, 10, 8, 10, 11, 8, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
    ],
    [
        3, 0, 9, 3, 9, 11, 11, 9, 10, 255, 255, 255, 255, 255, 255, 255,
    ],
    [
        0, 1, 10, 0, 10, 8, 8, 10, 11, 255, 255, 255, 255, 255, 255, 255,
    ],
    [
        3, 1, 10, 11, 3, 10, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
    ],
    [
        1, 2, 11, 1, 11, 9, 9, 11, 8, 255, 255, 255, 255, 255, 255, 255,
    ],
    [3, 0, 9, 3, 9, 11, 1, 2, 9, 2, 11, 9, 255, 255, 255, 255],
    [
        0, 2, 11, 8, 0, 11, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
    ],
    [
        3, 2, 11, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
    ],
    [
        2, 3, 8, 2, 8, 10, 10, 8, 9, 255, 255, 255, 255, 255, 255, 255,
    ],
    [
        9, 10, 2, 0, 9, 2, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
    ],
    [2, 3, 8, 2, 8, 10, 0, 1, 8, 1, 10, 8, 255, 255, 255, 255],
    [
        1, 10, 2, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
    ],
    [
        1, 3, 8, 9, 1, 8, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
    ],
    [
        0, 9, 1, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
    ],
    [
        0, 3, 8, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
    ],
    [
        255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
    ],
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_corner_at_exactly_the_iso_level_is_air() {
        let mut field = [[[0.0f32; 18]; 18]; 18];
        field[5][5][5] = 0.5;

        let mesh = MarchingCubes::generate_mesh(&field, 16, |_| 0, |_| ([0; 3], true));

        assert!(
            mesh.indices.is_empty(),
            "{} triangles",
            mesh.indices.len() / 3
        );
        assert_eq!(cube_index(&field, 5, 5, 5), 0);
        field[5][5][5] = 0.5001;
        assert_eq!(
            cube_index(&field, 5, 5, 5),
            1,
            "just over the iso level is solid"
        );
    }

    #[test]
    fn sample_density_interpolates_trilinearly_inside_a_cell() {
        // Corner (dx, dy, dz) holds 2^(4dx + 2dy + dz), so every corner is distinct
        // and the result identifies each corner's weight.
        let mut field = [[[0.0f32; 18]; 18]; 18];
        let (ox, oy, oz) = (8, 8, 8);
        for dx in 0..2 {
            for dy in 0..2 {
                for dz in 0..2 {
                    field[ox + dx][oy + dy][oz + dz] = (1u32 << (4 * dx + 2 * dy + dz)) as f32;
                }
            }
        }
        let expected = |fx: f32, fy: f32, fz: f32| -> f32 {
            let mut sum = 0.0;
            for dx in 0..2 {
                for dy in 0..2 {
                    for dz in 0..2 {
                        let wx = if dx == 1 { fx } else { 1.0 - fx };
                        let wy = if dy == 1 { fy } else { 1.0 - fy };
                        let wz = if dz == 1 { fz } else { 1.0 - fz };
                        sum += (1u32 << (4 * dx + 2 * dy + dz)) as f32 * wx * wy * wz;
                    }
                }
            }
            sum
        };

        for (fx, fy, fz) in [(0.25, 0.5, 0.75), (0.75, 0.25, 0.5), (0.5, 0.75, 0.25)] {
            let actual = MarchingCubes::sample_density(&field, 8.0 + fx, 8.0 + fy, 8.0 + fz);

            assert!(
                (actual - expected(fx, fy, fz)).abs() < 1e-3,
                "at ({fx}, {fy}, {fz}): got {actual}, expected {}",
                expected(fx, fy, fz)
            );
        }
    }

    #[test]
    fn test_empty_density_field() {
        let field = [[[0.0f32; 18]; 18]; 18];
        let mesh = MarchingCubes::generate_mesh(&field, 16, |_| 0, |_| ([0; 3], true));

        // Empty field should produce no geometry
        assert_eq!(mesh.vertices.len(), 0);
        assert_eq!(mesh.indices.len(), 0);
    }

    #[test]
    fn test_full_density_field() {
        let solid =
            (6..=7).flat_map(|x| (6..=7).flat_map(move |y| (6..=7).map(move |z| (x, y, z))));
        let field = solid_samples_field(solid);

        let mesh = MarchingCubes::generate_mesh(&field, 16, |_| 0, |_| ([0; 3], true));

        assert!(!mesh.vertices.is_empty(), "solid must produce geometry");
        assert!(!mesh.indices.is_empty());
        let n = mesh.vertices.len();
        assert_eq!(mesh.normals.len(), n);
        assert_eq!(mesh.ambient_occlusion.len(), n);
        assert_eq!(mesh.geometry_type.len(), n);
        assert_eq!(mesh.light_level.len(), n);
        assert_eq!(mesh.block_light_rgb.len(), n);
        assert_eq!(mesh.sky_exposed.len(), n);
        assert!(mesh.geometry_type.iter().all(|&t| t == 0));
        for v in &mesh.vertices {
            assert!(
                v.iter().all(|c| (5.5 - 1e-4..=7.5 + 1e-4).contains(c)),
                "vertex {v:?} outside the solid's shell"
            );
            let density = MarchingCubes::sample_density(&field, v[0], v[1], v[2]);
            assert!(
                (density - 0.5).abs() < 1e-4,
                "vertex {v:?} has density {density}, not on the isosurface"
            );
        }
        assert_normals_point_away_from(&mesh, [6.5, 6.5, 6.5]);
    }

    #[test]
    fn sample_at_iso_level_is_not_solid() {
        let mut field = [[[0.0f32; 18]; 18]; 18];
        field[8][8][8] = 0.5;

        let mesh = MarchingCubes::generate_mesh(&field, 16, |_| 0, |_| ([0; 3], true));

        assert!(mesh.vertices.is_empty());
        assert!(mesh.indices.is_empty());
    }

    fn solid_samples_field(
        solid: impl IntoIterator<Item = (usize, usize, usize)>,
    ) -> [[[f32; 18]; 18]; 18] {
        let mut field = [[[0.0f32; 18]; 18]; 18];
        for (x, y, z) in solid {
            field[x][y][z] = 1.0;
        }
        field
    }

    fn assert_normals_point_away_from(mesh: &VoxelMesh, centre: [f32; 3]) {
        assert!(!mesh.vertices.is_empty(), "solid must produce geometry");
        assert_eq!(mesh.vertices.len(), mesh.normals.len());

        for (v, n) in mesh.vertices.iter().zip(&mesh.normals) {
            let radial = [v[0] - centre[0], v[1] - centre[1], v[2] - centre[2]];
            let radial_len = radial.iter().map(|c| c * c).sum::<f32>().sqrt();
            let normal_len = n.iter().map(|c| c * c).sum::<f32>().sqrt();
            let cosine = (radial[0] * n[0] + radial[1] * n[1] + radial[2] * n[2])
                / (radial_len * normal_len);

            assert!(
                (normal_len - 1.0).abs() < 1e-3,
                "normal {n:?} at vertex {v:?} is not unit length"
            );
            // These solids are symmetric about `centre`, so the normals are exactly radial.
            assert!(
                cosine > 0.9999,
                "normal {n:?} at vertex {v:?} does not point away from {centre:?} (cosine {cosine})"
            );
        }
    }

    #[test]
    fn single_voxel_normals_point_outward() {
        // Interior sample, and the chunk's last meshed layer, whose +1 neighbour is padding.
        for sample in [(8, 8, 8), (16, 16, 16)] {
            let field = solid_samples_field([sample]);

            let mesh = MarchingCubes::generate_mesh(&field, 16, |_| 0, |_| ([0; 3], true));

            // A lone sample is an octahedron: 8 triangles, none dropped.
            assert_eq!(mesh.vertices.len(), 24, "lone voxel at {sample:?}");
            let (x, y, z) = sample;
            assert_normals_point_away_from(&mesh, [x as f32, y as f32, z as f32]);
        }
    }

    #[test]
    fn voxels_one_sample_apart_normals_point_outward() {
        // Between the two solids, both edge endpoints have a flat central difference.
        let field = solid_samples_field([(6, 8, 8), (8, 8, 8)]);

        let mesh = MarchingCubes::generate_mesh(&field, 16, |_| 0, |_| ([0; 3], true));

        assert_eq!(mesh.vertices.len(), 48, "two separate octahedra");
        for (v, n) in mesh.vertices.iter().zip(&mesh.normals) {
            let centre_x = if v[0] < 7.0 { 6.0 } else { 8.0 };
            let outward = [v[0] - centre_x, v[1] - 8.0, v[2] - 8.0].map(|c| c * 2.0);
            assert!(
                n.iter().zip(&outward).all(|(a, b)| (a - b).abs() < 1e-5),
                "normal {n:?} at vertex {v:?}, expected {outward:?}"
            );
        }
    }

    #[test]
    fn last_layer_gradient_is_one_sided_into_padding() {
        // 2x2x2 solid on samples 15..=16. At (16.5, 15, 15) the gradient interpolates
        // sample 16's central difference (-0.5, 0.5, 0.5) with sample 17's one-sided
        // (-1, 0, 0), giving (-0.75, 0.25, 0.25): outward (3, -1, -1) / sqrt(11).
        let solid =
            (15..=16).flat_map(|x| (15..=16).flat_map(move |y| (15..=16).map(move |z| (x, y, z))));
        let mesh = MarchingCubes::generate_mesh(
            &solid_samples_field(solid),
            16,
            |_| 0,
            |_| ([0; 3], true),
        );

        let i = mesh
            .vertices
            .iter()
            .position(|v| *v == [16.5, 15.0, 15.0])
            .expect("vertex on the edge into padding");
        let expected = [3.0, -1.0, -1.0].map(|c: f32| c / 11.0f32.sqrt());
        let n = mesh.normals[i];
        assert!(
            n.iter().zip(&expected).all(|(a, b)| (a - b).abs() < 1e-5),
            "normal {n:?}, expected {expected:?}"
        );
    }

    #[test]
    fn two_cube_solid_normals_point_outward() {
        let solid =
            (6..=7).flat_map(|x| (6..=7).flat_map(move |y| (6..=7).map(move |z| (x, y, z))));
        let field = solid_samples_field(solid);

        let mesh = MarchingCubes::generate_mesh(&field, 16, |_| 0, |_| ([0; 3], true));

        assert_normals_point_away_from(&mesh, [6.5, 6.5, 6.5]);
    }

    #[test]
    fn all_indices_in_range() {
        // Regression guard: out-of-range indices cause silent GPU crashes on some hardware.
        let solid =
            (1..=16).flat_map(|x| (1..=16).flat_map(move |y| (1..=16).map(move |z| (x, y, z))));
        let field = solid_samples_field(solid);
        let mesh = MarchingCubes::generate_mesh(&field, 16, |_| 0, |_| ([0; 3], true));

        let vertex_count = mesh.vertices.len() as u32;
        for &idx in &mesh.indices {
            assert!(
                idx < vertex_count,
                "index {idx} out of range ({vertex_count} vertices)"
            );
        }
    }

    /// Corner offsets in `process_cell` / `corner_sample` order.
    const CORNER_OFFSETS: [[usize; 3]; 8] = [
        [0, 0, 0],
        [1, 0, 0],
        [1, 0, 1],
        [0, 0, 1],
        [0, 1, 0],
        [1, 1, 0],
        [1, 1, 1],
        [0, 1, 1],
    ];

    const CELL: [usize; 3] = [8, 8, 8];

    fn mesh_triangles_in_cell(cube_index: u8) -> Vec<[Vec3; 3]> {
        let mut field = [[[0.0f32; 18]; 18]; 18];
        for (corner, offset) in CORNER_OFFSETS.iter().enumerate() {
            if cube_index & (1 << corner) != 0 {
                field[CELL[0] + offset[0]][CELL[1] + offset[1]][CELL[2] + offset[2]] = 1.0;
            }
        }
        let origin = Vec3::new(CELL[0] as f32, CELL[1] as f32, CELL[2] as f32);

        let mesh = MarchingCubes::generate_mesh(&field, 16, |_| 0, |_| ([0; 3], true));

        let vertex = |i: u32| {
            let v = mesh.vertices[i as usize];
            Vec3::new(v[0], v[1], v[2]) - origin
        };
        mesh.indices
            .chunks(3)
            .map(|t| [vertex(t[0]), vertex(t[1]), vertex(t[2])])
            .filter(|tri| {
                let c = (tri[0] + tri[1] + tri[2]) / 3.0;
                c.to_array().iter().all(|&a| a > 1e-4 && a < 1.0 - 1e-4)
            })
            .collect()
    }

    /// Same triangle, same winding, up to which vertex is listed first.
    fn same_triangle(a: &[Vec3; 3], b: &[Vec3; 3]) -> bool {
        (0..3).any(|r| (0..3).all(|i| a[i].abs_diff_eq(b[(i + r) % 3], 1e-5)))
    }

    #[test]
    fn cell_triangles_match_generated_mesh_for_every_cube_index() {
        for cube_index in 0..=255u8 {
            let from_mesh = mesh_triangles_in_cell(cube_index);

            let mut unmatched: Vec<[Vec3; 3]> = MarchingCubes::cell_triangles(cube_index).collect();

            let uniform = cube_index == 0 || cube_index == 255;
            assert_eq!(
                from_mesh.is_empty(),
                uniform,
                "index {cube_index}: the mesher's triangles in the cell"
            );
            for tri in &from_mesh {
                let found = unmatched.iter().position(|t| same_triangle(t, tri));
                let Some(found) = found else {
                    panic!(
                        "index {cube_index}: mesher triangle {tri:?} missing from cell_triangles"
                    );
                };
                unmatched.swap_remove(found);
            }
            assert!(
                unmatched.is_empty(),
                "index {cube_index}: cell_triangles has extra triangles {unmatched:?}"
            );
        }
    }

    /// Samples (8,8,8) and (9,8,8) at the given densities, everything else empty.
    fn mesh_of_two_samples(d0: f32, d1: f32) -> VoxelMesh {
        let mut field = [[[0.0f32; 18]; 18]; 18];
        field[8][8][8] = d0;
        field[9][8][8] = d1;
        MarchingCubes::generate_mesh(&field, 16, |_| 0, |_| ([0; 3], true))
    }

    #[test]
    fn edge_vertex_sits_at_iso_crossing() {
        // (density at x=8, density at x=9, x of the crossing between them)
        let rows = [
            (0.2, 0.8, 8.5),
            (0.0, 0.8, 8.625),
            (0.2, 0.6, 8.75),
            (0.8, 0.0, 8.375),
            (0.9, 0.3, 8.0 + 0.4 / 0.6),
        ];

        for (d0, d1, crossing) in rows {
            let mesh = mesh_of_two_samples(d0, d1);

            let on_edge: Vec<[f32; 3]> = mesh
                .vertices
                .iter()
                .copied()
                .filter(|v| v[0] > 8.0 && v[0] < 9.0 && v[1] == 8.0 && v[2] == 8.0)
                .collect();
            assert!(!on_edge.is_empty(), "{d0}/{d1}: no vertex on the edge");
            for v in on_edge {
                assert!(
                    (v[0] - crossing).abs() < 1e-5,
                    "{d0}/{d1}: vertex at x={}, expected {crossing}",
                    v[0]
                );
            }
        }
    }

    #[test]
    fn ao_follows_density_at_vertex() {
        // Every vertex sits on the 0.5 isosurface, where AO is 0.6 + 0.4 * (1 - 0.5).
        for (d0, d1) in [(0.2, 0.8), (0.0, 0.8), (0.9, 0.3)] {
            let mesh = mesh_of_two_samples(d0, d1);

            assert!(!mesh.vertices.is_empty());
            assert_eq!(mesh.ambient_occlusion.len(), mesh.vertices.len());
            for (v, ao) in mesh.vertices.iter().zip(&mesh.ambient_occlusion) {
                assert!(
                    (ao - 0.8).abs() < 1e-5,
                    "{d0}/{d1}: vertex {v:?} has AO {ao}, expected 0.8"
                );
            }
        }
    }

    fn corner_position(corner: usize) -> Vec3 {
        Vec3::from_array(MarchingCubes::CORNER_OFFSETS[corner].map(|c| c as f32))
    }

    #[test]
    fn nearest_solid_corner_breaks_ties_to_the_lowest_corner_index() {
        let cases = [
            (0b0000_0011u8, Vec3::new(0.5, 0.0, 0.0), 0),
            (0b0010_0100, Vec3::splat(0.5), 2),
            (0b1000_0100, Vec3::splat(0.5), 2),
            (0b1111_1111, Vec3::splat(0.5), 0),
            (0b1111_1000, Vec3::splat(0.5), 3),
            // Corner 0 is air; its three neighbours 1, 3 and 4 are equally near.
            (0b1111_1110, Vec3::splat(0.1), 1),
        ];

        for (cube_index, centroid, expected) in cases {
            assert_eq!(
                nearest_solid_corner(cube_index, centroid),
                expected,
                "cube index {cube_index:#010b}, centroid {centroid:?}"
            );
        }
    }

    #[test]
    fn nearest_solid_corner_of_every_triangle_is_a_solid_corner_no_farther_than_any_other() {
        for cube_index in 1..=254u8 {
            for tri in MarchingCubes::cell_triangles(cube_index) {
                let centroid = (tri[0] + tri[1] + tri[2]) / 3.0;

                let nearest = nearest_solid_corner(cube_index, centroid);

                assert!(
                    cube_index & (1 << nearest) != 0,
                    "cube index {cube_index}: corner {nearest} is air"
                );
                let nearest_distance = corner_position(nearest).distance(centroid);
                for other in (0..8).filter(|k| cube_index & (1 << k) != 0) {
                    let distance = corner_position(other).distance(centroid);
                    assert!(
                        nearest_distance <= distance + 1e-5,
                        "cube index {cube_index}: corner {other} is nearer than {nearest}"
                    );
                }
            }
        }
    }

    #[test]
    fn mesh_materials_follow_the_solid_sample_nearest_each_triangle() {
        let field = solid_samples_field([(8, 8, 8), (9, 8, 8)]);
        let material_at = |sample: [usize; 3]| if sample == [8, 8, 8] { 1 } else { 2 };

        let mesh = MarchingCubes::generate_mesh(&field, 16, material_at, |_| ([0; 3], true));

        assert_eq!(mesh.material.len(), mesh.vertices.len());
        let mut left = 0;
        let mut right = 0;
        for tri in mesh.indices.as_chunks::<3>().0 {
            let materials = tri.map(|i| mesh.material[i as usize]);
            let x = tri
                .iter()
                .map(|&i| mesh.vertices[i as usize][0])
                .sum::<f32>()
                / 3.0;
            assert_eq!(materials[0], materials[1]);
            assert_eq!(materials[0], materials[2]);
            if x < 8.5 - 1e-4 {
                left += 1;
                assert_eq!(materials[0], 1, "triangle at x {x}");
            } else if x > 8.5 + 1e-4 {
                right += 1;
                assert_eq!(materials[0], 2, "triangle at x {x}");
            }
        }
        assert!(left > 0 && right > 0, "{left} left, {right} right");
    }
}
