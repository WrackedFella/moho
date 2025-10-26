use crate::{MaterialTable, RendererBackend};
use bincode::{Decode, Encode};
use legion::World;
use legion::query::IntoQuery;
use moho_core::actors::{Cube, CustomMesh, InstanceGpu, Sphere};
use moho_core::voxel::VoxelChunk;
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;

/// Type alias for camera data: (position, yaw, pitch)
pub type CameraData = (glam::Vec3, f32, f32);

/// Scene file version. Bump when the on-disk layout changes.
const SCENE_FILE_VERSION: u32 = 2;

/// Scene manager that owns the `MaterialTable` and provides a simple
/// `render` API to submit an ECS world for drawing. This centralizes
/// material deduplication, instance collection, and transparent sorting.
pub struct Scene {
    pub material_table: MaterialTable,
}

impl Scene {
    pub fn new() -> Self {
        Scene {
            material_table: MaterialTable::new(),
        }
    }

    /// Render the provided `world` using `renderer`. `mesh_handle` is the
    /// spherical mesh handle and `cube_mesh_handle` is the cube mesh handle
    /// previously registered with the renderer.
    pub fn render(
        &mut self,
        renderer: &mut dyn RendererBackend,
        world: &mut World,
        mesh_handle: u32,
        cube_mesh_handle: u32,
        camera: (glam::Mat4, glam::Mat4, glam::Vec3),
    ) {
        let material_table = &mut self.material_table;
        // Build sphere instances and deduplicate materials
        let mut sphere_instances: Vec<InstanceGpu> = Vec::new();

        let mut q_s = <&Sphere>::query();
        for s in q_s.iter(world) {
            let midx = material_table.find_or_push(&s.mat_ptr);
            sphere_instances.push(s.to_instance_with_material(midx));
        }

        // Build cube instances and deduplicate materials
        let mut cube_instances: Vec<InstanceGpu> = Vec::new();
        let mut q_c = <&Cube>::query();
        for c in q_c.iter(world) {
            let midx = material_table.find_or_push(&c.mat_ptr);
            cube_instances.push(c.to_instance_with_material(midx));
        }

        // Debug: log material table and instance material indices (kept as-is)
        if !material_table.as_slice().is_empty() {
            log::debug!(
                "[debug] material_table.len={} ",
                material_table.as_slice().len()
            );
            for (i, m) in material_table.as_slice().iter().enumerate().take(8) {
                log::debug!(
                    "[debug] mat[{}] albedo=({:.3},{:.3},{:.3}) fuzz={:.3} ref={:.3}",
                    i,
                    m.albedo[0],
                    m.albedo[1],
                    m.albedo[2],
                    m.params[0],
                    m.params[1]
                );
            }
        }
        for (i, inst) in sphere_instances.iter().enumerate().take(8) {
            log::debug!("[debug] sphere_inst[{}].material={}", i, inst.material);
        }
        for (i, inst) in cube_instances.iter().enumerate().take(8) {
            log::debug!("[debug] cube_inst[{}].material={}", i, inst.material);
        }

        // Upload material table to GPU if it changed.
        if material_table.is_dirty() {
            renderer.set_materials(material_table.as_slice());
            material_table.clear_dirty();
        }

        // Upload VoxelChunk meshes to renderer (first-time registration)
        // Query mutable VoxelChunks to store mesh handles
        {
            let mut q_chunks_mut = <&mut VoxelChunk>::query();
            for chunk in q_chunks_mut.iter_mut(world) {
                // Skip if already uploaded or has no geometry
                if chunk.is_uploaded() || !chunk.has_geometry() {
                    continue;
                }

                // Register the chunk mesh with renderer
                let handle = renderer.register_indexed_mesh(
                    chunk.vertices(),
                    chunk.normals(),
                    chunk.indices(),
                );
                chunk.set_mesh_handle(handle);

                log::info!(
                    "Uploaded VoxelChunk {:?}: {} verts, {} indices -> handle {}",
                    chunk.chunk_pos,
                    chunk.vertices().len(),
                    chunk.indices().len(),
                    handle
                );
            }
        }

        // Build VoxelChunk instances (identity transform, mesh already in world space)
        let mut chunk_renders: Vec<(u32, InstanceGpu)> = Vec::new();
        {
            let mut q_chunks = <&VoxelChunk>::query();
            for chunk in q_chunks.iter(world) {
                if let Some(handle) = chunk.get_mesh_handle() {
                    // VoxelChunk uses identity transform (mesh in world space)
                    // Material index 0 (Lambertian)
                    let inst = InstanceGpu {
                        model: glam::Mat4::IDENTITY.to_cols_array_2d(),
                        material: 0,
                        object_type: 2, // VoxelChunk type
                        padding: [0, 0],
                    };
                    chunk_renders.push((handle, inst));
                }
            }
        }

        log::debug!(
            "[Scene::render] VoxelChunks to render: {}",
            chunk_renders.len()
        );

        // Draw by material type (opaque first). Split instances into opaque
        // and transparent using the material table, then perform a global
        // back-to-front sort for transparent instances across all meshes so
        // blending composites correctly.
        let mats = material_table.as_slice();

        let mut cube_opaque: Vec<InstanceGpu> = Vec::new();
        let mut sph_opaque: Vec<InstanceGpu> = Vec::new();
        // Collect transparent entries across meshes as (mesh_handle, instance)
        let mut transparent_entries: Vec<(u32, InstanceGpu)> = Vec::new();

        for inst in &cube_instances {
            let idx = inst.material as usize;
            let is_transparent = if idx < mats.len() {
                mats[idx].is_transparent()
            } else {
                false
            };
            if is_transparent {
                transparent_entries.push((cube_mesh_handle, *inst));
            } else {
                cube_opaque.push(*inst);
            }
        }
        for inst in &sphere_instances {
            let idx = inst.material as usize;
            let is_transparent = if idx < mats.len() {
                mats[idx].is_transparent()
            } else {
                false
            };
            if is_transparent {
                transparent_entries.push((mesh_handle, *inst));
            } else {
                sph_opaque.push(*inst);
            }
        }

        // Render opaque geometry first (no finalize).
        renderer.render_mesh(cube_mesh_handle, &cube_opaque, camera, false);
        renderer.render_mesh(mesh_handle, &sph_opaque, camera, false);

        // Render VoxelChunks (opaque, each chunk as separate draw)
        for (chunk_handle, chunk_inst) in &chunk_renders {
            renderer.render_mesh(*chunk_handle, &[*chunk_inst], camera, false);
        }

        // If there are transparent entries, compute per-instance depth from the
        // camera eye and sort furthest-first (back-to-front). We extract the
        // translation component from the instance model matrix (column 3).
        if transparent_entries.is_empty() {
            // No transparent draws: finalize by issuing an empty finalize draw.
            // Use any valid mesh handle - prefer chunk handle if available, else sphere mesh
            let finalize_handle = chunk_renders
                .first()
                .map(|(h, _)| *h)
                .unwrap_or(mesh_handle);
            renderer.render_mesh(finalize_handle, &Vec::new(), camera, true);
            return;
        }

        let cam_eye = camera.2;
        // Build vector of (distance_sq, mesh_handle, instance)
        let mut by_depth: Vec<(f32, u32, InstanceGpu)> =
            Vec::with_capacity(transparent_entries.len());
        for (mesh_h, inst) in transparent_entries {
            // instance.model is [[f32;4];4] with column-major layout; the
            // translation lives in column 3 (mat[3][0..2]) as set by `to_instance_with_material`.
            let pos = glam::Vec3::new(inst.model[3][0], inst.model[3][1], inst.model[3][2]);
            let dist2 = (pos - cam_eye).length_squared();
            by_depth.push((dist2, mesh_h, inst));
        }

        // Sort by distance descending (furthest first)
        by_depth.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

        // Group consecutive entries with the same mesh handle to minimize draw calls
        // while preserving the sorted order.
        let mut groups: Vec<(u32, Vec<InstanceGpu>)> = Vec::new();
        for (_d, mesh_h, inst) in by_depth {
            if let Some((last_mesh, vec)) = groups.last_mut()
                && *last_mesh == mesh_h
            {
                vec.push(inst);
                continue;
            }
            groups.push((mesh_h, vec![inst]));
        }

        // Issue draws for each group in order. Mark finalize=true for the last
        // call so the renderer flushes and presents the batched frame.
        for (i, (mesh_h, insts)) in groups.iter().enumerate() {
            let final_call = i + 1 == groups.len();
            renderer.render_mesh(*mesh_h, insts, camera, final_call);
        }
    }

    /// Save a compact binary snapshot of the scene (method 2: bincode).
    /// This writes a serialized SceneDesc containing all Spheres, Cubes, VoxelChunks,
    /// Materials, and camera position/orientation. It does NOT serialize arbitrary
    /// ECS state; it serializes the application-level scene description
    /// and can be reloaded into a fresh `World` via `load_from_file`.
    pub fn save_to_file<P: AsRef<Path>>(
        &self,
        path: P,
        world: &World,
        camera_position: Option<CameraData>, // (position, yaw, pitch)
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Collect serializable descriptors from the ECS world.
        let mut spheres: Vec<SphereDesc> = Vec::new();
        let mut qs = <&Sphere>::query();
        for s in qs.iter(world) {
            spheres.push(SphereDesc {
                center: s.center.to_array(),
                radius: s.radius,
                material: MaterialDesc::from_material(&s.mat_ptr),
            });
        }
        let mut cubes: Vec<CubeDesc> = Vec::new();
        let mut qc = <&Cube>::query();
        for c in qc.iter(world) {
            cubes.push(CubeDesc {
                center: c.center.to_array(),
                length: c.length,
                width: c.width,
                height: c.height,
                material: MaterialDesc::from_material(&c.mat_ptr),
            });
        }

        // Collect VoxelChunks
        let mut voxel_chunks: Vec<VoxelChunkDesc> = Vec::new();
        let mut qv = <&moho_core::voxel::VoxelChunk>::query();
        for chunk in qv.iter(world) {
            voxel_chunks.push(VoxelChunkDesc {
                chunk_pos: [chunk.chunk_pos.x, chunk.chunk_pos.y, chunk.chunk_pos.z],
                vertices: chunk.vertices.clone(),
                normals: chunk.normals.clone(),
                indices: chunk.indices.clone(),
                material_id: chunk.material_id,
            });
        }

        let camera = camera_position.map(|(pos, yaw, pitch)| CameraDesc {
            position: [pos.x, pos.y, pos.z],
            yaw,
            pitch,
        });

        let desc = SceneDesc {
            version: SCENE_FILE_VERSION,
            spheres,
            cubes,
            voxel_chunks,
            camera,
        };

        let encoded = bincode::encode_to_vec(&desc, bincode::config::standard())?;
        let mut f = File::create(path)?;
        f.write_all(&encoded)?;
        Ok(())
    }

    /// Encode the scene descriptor to a Vec<u8> for in-memory handling.
    /// This mirrors the on-disk serialization used by `save_to_file`.
    pub fn encode_to_bytes(
        &self,
        world: &World,
        camera_position: Option<CameraData>,
    ) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        // Collect serializable descriptors from the ECS world.
        let mut spheres: Vec<SphereDesc> = Vec::new();
        let mut qs = <&Sphere>::query();
        for s in qs.iter(world) {
            spheres.push(SphereDesc {
                center: s.center.to_array(),
                radius: s.radius,
                material: MaterialDesc::from_material(&s.mat_ptr),
            });
        }
        let mut cubes: Vec<CubeDesc> = Vec::new();
        let mut qc = <&Cube>::query();
        for c in qc.iter(world) {
            cubes.push(CubeDesc {
                center: c.center.to_array(),
                length: c.length,
                width: c.width,
                height: c.height,
                material: MaterialDesc::from_material(&c.mat_ptr),
            });
        }

        // Collect VoxelChunks
        let mut voxel_chunks: Vec<VoxelChunkDesc> = Vec::new();
        let mut qv = <&moho_core::voxel::VoxelChunk>::query();
        for chunk in qv.iter(world) {
            voxel_chunks.push(VoxelChunkDesc {
                chunk_pos: [chunk.chunk_pos.x, chunk.chunk_pos.y, chunk.chunk_pos.z],
                vertices: chunk.vertices.clone(),
                normals: chunk.normals.clone(),
                indices: chunk.indices.clone(),
                material_id: chunk.material_id,
            });
        }

        let camera = camera_position.map(|(pos, yaw, pitch)| CameraDesc {
            position: [pos.x, pos.y, pos.z],
            yaw,
            pitch,
        });

        let desc = SceneDesc {
            version: SCENE_FILE_VERSION,
            spheres,
            cubes,
            voxel_chunks,
            camera,
        };

        let encoded = bincode::encode_to_vec(&desc, bincode::config::standard())?;
        Ok(encoded)
    }

    /// Load a SceneDesc from a file and populate the provided `world` with
    /// entities. Existing world contents are left untouched; caller may
    /// clear the world beforehand if desired.
    /// Returns camera position and orientation if present in the scene file.
    pub fn load_from_file<P: AsRef<Path>>(
        &mut self,
        path: P,
        world: &mut World,
    ) -> Result<Option<CameraData>, Box<dyn std::error::Error>> {
        let mut f = File::open(path)?;
        let mut buf = Vec::new();
        f.read_to_end(&mut buf)?;
        // Delegate to helper that decodes from bytes so load_from_bytes can reuse it.
        Self::decode_and_populate(&buf, world)
    }

    /// Decode a scene saved as bincode bytes and populate `world`. Returns
    /// optional camera data if present.
    pub fn load_from_bytes(
        &mut self,
        bytes: &[u8],
        world: &mut World,
    ) -> Result<Option<CameraData>, Box<dyn std::error::Error>> {
        Self::decode_and_populate(bytes, world)
    }

    /// Internal helper to decode SceneDesc from bytes and populate a world.
    fn decode_and_populate(
        bytes: &[u8],
        world: &mut World,
    ) -> Result<Option<CameraData>, Box<dyn std::error::Error>> {
        let desc: SceneDesc = bincode::decode_from_slice(bytes, bincode::config::standard())?.0;
        if desc.version < 1 || desc.version > SCENE_FILE_VERSION {
            return Err(format!(
                "unsupported scene file version: {} (expected 1-{})",
                desc.version, SCENE_FILE_VERSION
            )
            .into());
        }

        // Recreate materials and push entities into the world.
        for s in desc.spheres {
            let mat = s.material.into_material_type();
            let sphere = Sphere::new(glam::Vec3::from_array(s.center), s.radius, mat);
            world.push((sphere,));
        }
        for c in desc.cubes {
            let mat = c.material.into_material_type();
            let cube = Cube::new(
                glam::Vec3::from_array(c.center),
                c.length,
                c.width,
                c.height,
                mat,
            );
            world.push((cube,));
        }

        // Load VoxelChunks
        for chunk_desc in desc.voxel_chunks {
            let chunk = moho_core::voxel::VoxelChunk {
                chunk_pos: glam::IVec3::new(
                    chunk_desc.chunk_pos[0],
                    chunk_desc.chunk_pos[1],
                    chunk_desc.chunk_pos[2],
                ),
                vertices: chunk_desc.vertices,
                normals: chunk_desc.normals,
                indices: chunk_desc.indices,
                material_id: chunk_desc.material_id,
                mesh_handle: None, // Will be uploaded on next render
            };
            world.push((chunk,));
        }

        // Return camera position and orientation if present
        let camera_data = desc
            .camera
            .map(|cam| (glam::Vec3::from_array(cam.position), cam.yaw, cam.pitch));

        Ok(camera_data)
    }
}

impl Default for Scene {
    fn default() -> Self {
        Self::new()
    }
}

/// Serializable scene descriptor used for bincode snapshotting.
#[derive(Encode, Decode, Serialize, Deserialize)]
struct SceneDesc {
    /// on-disk format version. Bump when making breaking changes.
    version: u32,
    spheres: Vec<SphereDesc>,
    cubes: Vec<CubeDesc>,
    #[serde(default)]
    voxel_chunks: Vec<VoxelChunkDesc>,
    #[serde(default)]
    camera: Option<CameraDesc>,
}

#[derive(Encode, Decode, Serialize, Deserialize)]
struct VoxelChunkDesc {
    chunk_pos: [i32; 3],
    vertices: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    indices: Vec<u32>,
    material_id: u32,
}

#[derive(Encode, Decode, Serialize, Deserialize)]
struct SphereDesc {
    center: [f32; 3],
    radius: f32,
    material: MaterialDesc,
}

#[derive(Encode, Decode, Serialize, Deserialize)]
struct CubeDesc {
    center: [f32; 3],
    length: f32,
    width: f32,
    height: f32,
    material: MaterialDesc,
}

#[derive(Encode, Decode, Serialize, Deserialize)]
struct CameraDesc {
    position: [f32; 3],
    yaw: f32,
    pitch: f32,
}

#[derive(Encode, Decode, Serialize, Deserialize)]
enum MaterialDesc {
    Lambertian { albedo: [f32; 3] },
    Metal { albedo: [f32; 3], fuzz: f32 },
    Dielectric { ref_indx: f32 },
}

impl MaterialDesc {
    fn from_material(m: &moho_core::materials::MaterialType) -> Self {
        match m {
            moho_core::materials::MaterialType::Lambertian { albedo } => MaterialDesc::Lambertian {
                albedo: [albedo.x, albedo.y, albedo.z],
            },
            moho_core::materials::MaterialType::Metal { albedo, fuzz } => MaterialDesc::Metal {
                albedo: [albedo.x, albedo.y, albedo.z],
                fuzz: *fuzz,
            },
            moho_core::materials::MaterialType::Dielectric { ref_indx } => {
                MaterialDesc::Dielectric {
                    ref_indx: *ref_indx,
                }
            }
        }
    }

    fn into_material_type(self) -> moho_core::materials::MaterialType {
        match self {
            MaterialDesc::Lambertian { albedo } => moho_core::materials::MaterialType::Lambertian {
                albedo: glam::Vec3::new(albedo[0], albedo[1], albedo[2]),
            },
            MaterialDesc::Metal { albedo, fuzz } => moho_core::materials::MaterialType::Metal {
                albedo: glam::Vec3::new(albedo[0], albedo[1], albedo[2]),
                fuzz,
            },
            MaterialDesc::Dielectric { ref_indx } => {
                moho_core::materials::MaterialType::Dielectric { ref_indx }
            }
        }
    }
}
