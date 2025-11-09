use crate::{BufferManager, InstanceCollector, MaterialTable, RendererBackend};
use bincode::{Decode, Encode};
use legion::World;
use legion::query::IntoQuery;
use moho_core::actors::{Cube, InstanceGpu, Sphere};
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
    buffer_manager: BufferManager,
    instance_collector: InstanceCollector,
}

impl Scene {
    pub fn new() -> Self {
        Scene {
            material_table: MaterialTable::new(),
            buffer_manager: BufferManager::new(),
            instance_collector: InstanceCollector::new(),
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
        // Collect instances from world (delegates to InstanceCollector)
        self.instance_collector.collect_from_world(
            world,
            &mut self.material_table,
            &mut self.buffer_manager,
            renderer,
        );

        // Debug logging (kept from original implementation)
        let material_table = &mut self.material_table;
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
        for (i, inst) in self
            .instance_collector
            .sphere_instances()
            .iter()
            .enumerate()
            .take(8)
        {
            log::debug!("[debug] sphere_inst[{}].material={}", i, inst.material);
        }
        for (i, inst) in self
            .instance_collector
            .cube_instances()
            .iter()
            .enumerate()
            .take(8)
        {
            log::debug!("[debug] cube_inst[{}].material={}", i, inst.material);
        }

        // Upload material table to GPU if it changed
        if material_table.is_dirty() {
            renderer.set_materials(material_table.as_slice());
            material_table.clear_dirty();
        }

        log::debug!(
            "[Scene::render] VoxelChunks to render: {}",
            self.instance_collector.chunk_renders().len()
        );

        // Separate opaque and transparent instances
        let (cube_opaque, sph_opaque, transparent_entries) =
            self.separate_opaque_transparent(mesh_handle, cube_mesh_handle);

        // Render opaque geometry first (no finalize)
        renderer.render_mesh(cube_mesh_handle, &cube_opaque, camera, false);
        renderer.render_mesh(mesh_handle, &sph_opaque, camera, false);

        // Render VoxelChunks (opaque, each chunk as separate draw)
        for (chunk_handle, chunk_inst) in self.instance_collector.chunk_renders() {
            renderer.render_mesh(*chunk_handle, &[*chunk_inst], camera, false);
        }

        // Render transparent instances (back-to-front sorted)
        self.render_transparent(
            renderer,
            transparent_entries,
            camera,
            mesh_handle,
            self.instance_collector.chunk_renders(),
        );
    }

    /// Separate instances into opaque and transparent groups.
    ///
    /// Returns (opaque_cubes, opaque_spheres, transparent_entries)
    fn separate_opaque_transparent(
        &self,
        mesh_handle: u32,
        cube_mesh_handle: u32,
    ) -> (Vec<InstanceGpu>, Vec<InstanceGpu>, Vec<(u32, InstanceGpu)>) {
        let mats = self.material_table.as_slice();
        let mut cube_opaque = Vec::new();
        let mut sph_opaque = Vec::new();
        let mut transparent_entries = Vec::new();

        // Separate cubes
        for inst in self.instance_collector.cube_instances() {
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

        // Separate spheres
        for inst in self.instance_collector.sphere_instances() {
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

        (cube_opaque, sph_opaque, transparent_entries)
    }

    /// Render transparent instances sorted back-to-front.
    fn render_transparent(
        &self,
        renderer: &mut dyn RendererBackend,
        transparent_entries: Vec<(u32, InstanceGpu)>,
        camera: (glam::Mat4, glam::Mat4, glam::Vec3),
        mesh_handle: u32,
        chunk_renders: &[(u32, InstanceGpu)],
    ) {
        // If no transparent draws, finalize with empty draw
        if transparent_entries.is_empty() {
            let finalize_handle = chunk_renders
                .first()
                .map(|(h, _)| *h)
                .unwrap_or(mesh_handle);
            renderer.render_mesh(finalize_handle, &Vec::new(), camera, true);
            return;
        }

        let cam_eye = camera.2;

        // Sort transparent entries by depth (back-to-front)
        let mut by_depth: Vec<(f32, u32, InstanceGpu)> =
            Vec::with_capacity(transparent_entries.len());
        for (mesh_h, inst) in transparent_entries {
            let pos = glam::Vec3::new(inst.model[3][0], inst.model[3][1], inst.model[3][2]);
            let dist2 = (pos - cam_eye).length_squared();
            by_depth.push((dist2, mesh_h, inst));
        }

        by_depth.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

        // Group consecutive entries with the same mesh handle
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

        // Render each group, marking the last call for finalization
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
