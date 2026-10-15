use flagstone_voxel::MaterialType;
use glam::Vec3;
use moho_render_api::{InstanceGpu, Renderable};

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

/// Handle to an actor in an [`ActorStore`].
///
/// Identifies the actor's kind, its index within that kind, and the store
/// generation it was spawned in. A handle resolves to nothing once the store
/// has been cleared, even if the same index is later reused.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct ActorId {
    kind: ActorKind,
    index: usize,
    generation: u32,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum ActorKind {
    Sphere,
    Cube,
}

/// Owns every sphere and cube actor, addressed by [`ActorId`].
///
/// Actors iterate in spawn order within their kind. There is no per-actor
/// despawn; [`ActorStore::clear`] removes all actors and invalidates all handles.
#[derive(Debug, Default)]
pub struct ActorStore {
    spheres: Vec<Sphere>,
    cubes: Vec<Cube>,
    generation: u32,
}

impl ActorStore {
    /// Creates an empty store.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a sphere and returns its handle.
    pub fn spawn_sphere(&mut self, sphere: Sphere) -> ActorId {
        self.spheres.push(sphere);
        ActorId {
            kind: ActorKind::Sphere,
            index: self.spheres.len() - 1,
            generation: self.generation,
        }
    }

    /// Adds a cube and returns its handle.
    pub fn spawn_cube(&mut self, cube: Cube) -> ActorId {
        self.cubes.push(cube);
        ActorId {
            kind: ActorKind::Cube,
            index: self.cubes.len() - 1,
            generation: self.generation,
        }
    }

    /// All spheres in spawn order.
    pub fn spheres(&self) -> &[Sphere] {
        &self.spheres
    }

    /// All cubes in spawn order.
    pub fn cubes(&self) -> &[Cube] {
        &self.cubes
    }

    /// The centre of the identified actor, or `None` if the handle is stale
    /// (from before a [`ActorStore::clear`]).
    pub fn center_mut(&mut self, id: ActorId) -> Option<&mut Vec3> {
        if id.generation != self.generation {
            return None;
        }
        match id.kind {
            ActorKind::Sphere => self.spheres.get_mut(id.index).map(|s| &mut s.center),
            ActorKind::Cube => self.cubes.get_mut(id.index).map(|c| &mut c.center),
        }
    }

    /// Removes every actor and invalidates all previously issued handles.
    pub fn clear(&mut self) {
        self.spheres.clear();
        self.cubes.clear();
        self.generation = self.generation.wrapping_add(1);
    }

    /// Total number of actors across all kinds.
    pub fn len(&self) -> usize {
        self.spheres.len() + self.cubes.len()
    }

    /// True when the store holds no actors.
    pub fn is_empty(&self) -> bool {
        self.spheres.is_empty() && self.cubes.is_empty()
    }
}

impl Renderable for Sphere {
    type Material = MaterialType;

    fn material(&self) -> &MaterialType {
        &self.mat_ptr
    }

    fn to_instance_with_material(&self, material_index: u32) -> InstanceGpu {
        Sphere::to_instance_with_material(self, material_index)
    }
}

impl Renderable for Cube {
    type Material = MaterialType;

    fn material(&self) -> &MaterialType {
        &self.mat_ptr
    }

    fn to_instance_with_material(&self, material_index: u32) -> InstanceGpu {
        Cube::to_instance_with_material(self, material_index)
    }
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
            // Viewed from -Y (below), CCW order: front-left, back-left, back-right, front-right
            [-0.5, -0.5, 0.5],  // 12: front-left
            [-0.5, -0.5, -0.5], // 13: back-left
            [0.5, -0.5, -0.5],  // 14: back-right
            [0.5, -0.5, 0.5],   // 15: front-right
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
            0, 1, 2, 0, 2, 3, // +X
            4, 5, 6, 4, 6, 7, // -X
            8, 9, 10, 8, 10, 11, // +Y
            12, 13, 14, 12, 14, 15, // -Y
            16, 17, 18, 16, 18, 19, // +Z
            20, 21, 22, 20, 22, 23, // -Z
        ];
        (verts, normals, indices)
    }
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
                // triangle 1: a, b, c (CCW from outside)
                indices.push(a);
                indices.push(b);
                indices.push(c);
                // triangle 2: b, d, c (CCW from outside)
                indices.push(b);
                indices.push(d);
                indices.push(c);
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

#[cfg(test)]
mod tests {
    use super::*;

    fn sphere_at(center: Vec3) -> Sphere {
        Sphere::new(center, 1.0, MaterialType::Lambertian { albedo: Vec3::ONE })
    }

    fn cube_at(center: Vec3) -> Cube {
        Cube::new(
            center,
            1.0,
            1.0,
            1.0,
            MaterialType::Lambertian { albedo: Vec3::ONE },
        )
    }

    #[test]
    fn an_actor_moves_by_its_handle_when_another_kind_shares_its_index() {
        let mut store = ActorStore::new();
        let sphere = store.spawn_sphere(sphere_at(Vec3::ZERO));
        let cube = store.spawn_cube(cube_at(Vec3::ZERO));

        *store.center_mut(cube).expect("live handle") = Vec3::X;

        assert_eq!(store.spheres()[0].center, Vec3::ZERO);
        assert_eq!(store.cubes()[0].center, Vec3::X);

        *store.center_mut(sphere).expect("live handle") = Vec3::Y;

        assert_eq!(store.spheres()[0].center, Vec3::Y);
        assert_eq!(store.cubes()[0].center, Vec3::X);
        assert_ne!(sphere, cube);
    }

    #[test]
    fn len_counts_spheres_and_cubes_together() {
        let mut store = ActorStore::new();
        store.spawn_sphere(sphere_at(Vec3::ZERO));
        store.spawn_cube(cube_at(Vec3::ZERO));
        store.spawn_cube(cube_at(Vec3::X));

        assert_eq!(store.len(), 3);
    }

    #[test]
    fn spawned_actors_iterate_in_spawn_order() {
        let mut store = ActorStore::new();
        store.spawn_sphere(sphere_at(Vec3::new(1.0, 0.0, 0.0)));
        store.spawn_cube(cube_at(Vec3::new(0.0, 1.0, 0.0)));
        store.spawn_sphere(sphere_at(Vec3::new(2.0, 0.0, 0.0)));
        store.spawn_cube(cube_at(Vec3::new(0.0, 2.0, 0.0)));

        let sphere_xs: Vec<f32> = store.spheres().iter().map(|s| s.center.x).collect();
        let cube_ys: Vec<f32> = store.cubes().iter().map(|c| c.center.y).collect();

        assert_eq!(sphere_xs, vec![1.0, 2.0]);
        assert_eq!(cube_ys, vec![1.0, 2.0]);
        assert_eq!(store.len(), 4);
    }

    #[test]
    fn handle_from_before_clear_resolves_to_none() {
        type Spawn = fn(&mut ActorStore, Vec3) -> ActorId;
        type Centers = fn(&ActorStore) -> Vec<Vec3>;
        let rows: [(&str, Spawn, Centers); 2] = [
            (
                "sphere",
                |store, at| store.spawn_sphere(sphere_at(at)),
                |store| store.spheres().iter().map(|s| s.center).collect(),
            ),
            (
                "cube",
                |store, at| store.spawn_cube(cube_at(at)),
                |store| store.cubes().iter().map(|c| c.center).collect(),
            ),
        ];

        for (kind, spawn, centers) in rows {
            let mut store = ActorStore::new();
            let old = spawn(&mut store, Vec3::ZERO);

            store.clear();
            let new = spawn(&mut store, Vec3::ONE);

            assert!(store.center_mut(old).is_none(), "{kind}");
            assert_eq!(store.center_mut(new).map(|c| *c), Some(Vec3::ONE), "{kind}");
            assert_eq!(centers(&store), vec![Vec3::ONE], "{kind}");
        }
    }

    #[test]
    fn is_empty_is_true_only_without_actors() {
        let mut store = ActorStore::new();
        assert!(store.is_empty());
        assert_eq!(store.len(), 0);

        store.spawn_cube(cube_at(Vec3::ZERO));
        assert!(!store.is_empty());

        store.clear();
        assert!(store.is_empty());
        assert_eq!(store.len(), 0);
    }

    #[test]
    fn handle_from_two_clears_ago_resolves_to_none() {
        let mut store = ActorStore::new();
        let h0 = store.spawn_sphere(sphere_at(Vec3::X));
        store.clear();
        let h1 = store.spawn_sphere(sphere_at(Vec3::Y));
        store.clear();
        let h2 = store.spawn_sphere(sphere_at(Vec3::Z));

        assert!(store.center_mut(h0).is_none());
        assert!(store.center_mut(h1).is_none());
        assert_eq!(store.center_mut(h2).map(|c| *c), Some(Vec3::Z));
        assert_ne!(h0, h1);
        assert_ne!(h1, h2);
    }

    const GEOMETRY_EPS: f32 = 1e-4;

    fn v3(p: [f32; 3]) -> Vec3 {
        Vec3::from_array(p)
    }

    fn triangles(indices: &[u32]) -> Vec<[usize; 3]> {
        indices
            .as_chunks::<3>()
            .0
            .iter()
            .map(|t| [t[0] as usize, t[1] as usize, t[2] as usize])
            .collect()
    }

    #[test]
    fn unit_cube_indexed_faces_wind_outward() {
        let (verts, normals, indices) = Cube::unit_cube_indexed();

        assert_eq!(verts.len(), 24);
        assert_eq!(normals.len(), 24);
        assert_eq!(indices.len(), 36);
        assert!(indices.iter().all(|&i| (i as usize) < verts.len()));
        for [a, b, c] in triangles(&indices) {
            let cross = (v3(verts[b]) - v3(verts[a])).cross(v3(verts[c]) - v3(verts[a]));
            let normal = v3(normals[a]);
            assert_eq!(normals[a], normals[b]);
            assert_eq!(normals[a], normals[c]);
            assert!(cross.dot(normal) > 0.0, "triangle {a},{b},{c} faces inward");
            assert!(cross.cross(normal).length() < GEOMETRY_EPS);
        }
    }

    #[test]
    fn unit_cube_indexed_normals_are_face_axes() {
        let (verts, normals, _) = Cube::unit_cube_indexed();

        for (vert, normal) in verts.iter().zip(&normals) {
            assert!(vert.iter().all(|c| (c.abs() - 0.5).abs() < GEOMETRY_EPS));
            let n = v3(*normal);
            assert!((n.length() - 1.0).abs() < GEOMETRY_EPS);
            assert_eq!(n.to_array().iter().filter(|c| c.abs() > 0.5).count(), 1);
            assert!((v3(*vert).dot(n) - 0.5).abs() < GEOMETRY_EPS);
        }
        let mut rounded: Vec<[i32; 3]> = normals
            .iter()
            .map(|n| n.map(|c| c.round() as i32))
            .collect();
        rounded.sort_unstable();
        let mut expected: Vec<[i32; 3]> = [
            [1, 0, 0],
            [-1, 0, 0],
            [0, 1, 0],
            [0, -1, 0],
            [0, 0, 1],
            [0, 0, -1],
        ]
        .iter()
        .flat_map(|axis| [*axis; 4])
        .collect();
        expected.sort_unstable();
        assert_eq!(rounded, expected);
    }

    const SPHERE_RESOLUTIONS: [(usize, usize); 4] = [(3, 5), (16, 16), (2, 3), (7, 4)];

    #[test]
    fn unit_sphere_indexed_vertices_lie_on_unit_sphere() {
        for (lat, lon) in SPHERE_RESOLUTIONS {
            let (verts, normals, indices) = Sphere::unit_sphere_indexed(lat, lon);

            assert_eq!(verts.len(), (lat + 1) * (lon + 1), "{lat}x{lon}");
            assert_eq!(normals, verts, "{lat}x{lon}");
            assert_eq!(indices.len(), lat * lon * 6, "{lat}x{lon}");
            assert!(indices.iter().all(|&i| (i as usize) < verts.len()));
            for vert in &verts {
                assert!(
                    (v3(*vert).length() - 1.0).abs() < GEOMETRY_EPS,
                    "{lat}x{lon}"
                );
            }
            for ring in verts.chunks_exact(lon + 1) {
                assert!(v3(ring[lon]).abs_diff_eq(v3(ring[0]), GEOMETRY_EPS));
            }
        }
    }

    #[test]
    fn unit_sphere_indexed_poles_sit_on_y_axis() {
        for (lat, lon) in SPHERE_RESOLUTIONS {
            let (verts, _, _) = Sphere::unit_sphere_indexed(lat, lon);

            let first = &verts[..=lon];
            let last = &verts[lat * (lon + 1)..];

            assert!(
                first
                    .iter()
                    .all(|v| v3(*v).abs_diff_eq(Vec3::Y, GEOMETRY_EPS))
            );
            assert!(
                last.iter()
                    .all(|v| v3(*v).abs_diff_eq(Vec3::NEG_Y, GEOMETRY_EPS))
            );
        }
    }

    #[test]
    fn unit_sphere_indexed_equator_spans_the_full_circle() {
        let (verts, _, _) = Sphere::unit_sphere_indexed(16, 16);

        let equator = &verts[8 * 17..9 * 17];

        assert!(v3(equator[0]).abs_diff_eq(Vec3::X, GEOMETRY_EPS));
        assert!(v3(equator[4]).abs_diff_eq(Vec3::Z, GEOMETRY_EPS));
        assert!(v3(equator[8]).abs_diff_eq(Vec3::NEG_X, GEOMETRY_EPS));
        assert!(v3(equator[12]).abs_diff_eq(Vec3::NEG_Z, GEOMETRY_EPS));
        assert!(v3(equator[16]).abs_diff_eq(Vec3::X, GEOMETRY_EPS));
    }

    #[test]
    fn unit_sphere_indexed_triangles_wind_outward() {
        for (lat, lon) in SPHERE_RESOLUTIONS {
            let (verts, _, indices) = Sphere::unit_sphere_indexed(lat, lon);

            let tris = triangles(&indices);
            let mut outward = 0;
            for [a, b, c] in &tris {
                let (pa, pb, pc) = (v3(verts[*a]), v3(verts[*b]), v3(verts[*c]));
                let cross = (pb - pa).cross(pc - pa);
                if cross.length() < GEOMETRY_EPS {
                    continue;
                }
                let centroid = (pa + pb + pc) / 3.0;
                assert!(cross.dot(centroid) > 0.0, "{lat}x{lon}: inward {a},{b},{c}");
                outward += 1;
            }

            assert_eq!(tris.len(), lat * lon * 2, "{lat}x{lon}");
            assert_eq!(outward, lat * lon * 2 - 2 * lon, "{lat}x{lon}");
        }
    }

    #[test]
    fn unit_sphere_indexed_floors_segment_counts() {
        let floored = Sphere::unit_sphere_indexed(2, 3);
        assert_eq!(floored.0.len(), 12);
        assert_eq!(floored.2.len(), 36);

        for (lat, lon) in [(0, 0), (1, 3), (2, 2)] {
            let (verts, normals, indices) = Sphere::unit_sphere_indexed(lat, lon);

            assert_eq!(verts, floored.0, "{lat}x{lon}");
            assert_eq!(normals, floored.1, "{lat}x{lon}");
            assert_eq!(indices, floored.2, "{lat}x{lon}");
        }
    }

    #[test]
    fn to_instance_scales_and_translates_by_actor_dims() {
        let cube_mat = MaterialType::Lambertian { albedo: Vec3::X };
        let sphere_mat = MaterialType::Metal {
            albedo: Vec3::Y,
            fuzz: 0.25,
        };
        let cube = Cube::new(Vec3::new(7.0, 11.0, 13.0), 2.0, 5.0, 3.0, cube_mat);
        let sphere = Sphere::new(Vec3::new(-1.5, 2.5, 6.0), 4.0, sphere_mat);

        let cube_instance = Renderable::to_instance_with_material(&cube, 9);
        let sphere_instance = Renderable::to_instance_with_material(&sphere, 4);

        assert_eq!(
            cube_instance.model,
            [
                [2.0, 0.0, 0.0, 0.0],
                [0.0, 3.0, 0.0, 0.0],
                [0.0, 0.0, 5.0, 0.0],
                [7.0, 11.0, 13.0, 1.0],
            ]
        );
        assert_eq!((cube_instance.material, cube_instance.object_type), (9, 1));
        assert_eq!(
            sphere_instance.model,
            [
                [4.0, 0.0, 0.0, 0.0],
                [0.0, 4.0, 0.0, 0.0],
                [0.0, 0.0, 4.0, 0.0],
                [-1.5, 2.5, 6.0, 1.0],
            ]
        );
        assert_eq!(
            (sphere_instance.material, sphere_instance.object_type),
            (4, 2)
        );
        assert_eq!(*Renderable::material(&cube), cube_mat);
        assert_eq!(*Renderable::material(&sphere), sphere_mat);
    }
}
