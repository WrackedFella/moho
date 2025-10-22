use crate::{MaterialTable, RendererBackend};
use bincode::{Decode, Encode};
use engine_core::actors::{Cube, InstanceGpu, Sphere};
use legion::World;
use legion::query::IntoQuery;
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;

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
        world: &World,
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

        // If there are transparent entries, compute per-instance depth from the
        // camera eye and sort furthest-first (back-to-front). We extract the
        // translation component from the instance model matrix (column 3).
        if transparent_entries.is_empty() {
            // No transparent draws: finalize by issuing an empty finalize draw.
            renderer.render_mesh(mesh_handle, &Vec::new(), camera, true);
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
    /// This writes a serialized SceneDesc containing all Spheres, Cubes,
    /// Materials, and camera position/orientation. It does NOT serialize arbitrary
    /// ECS state; it serializes the application-level scene description
    /// and can be reloaded into a fresh `World` via `load_from_file`.
    pub fn save_to_file<P: AsRef<Path>>(
        &self,
        path: P,
        world: &World,
        camera_position: Option<(glam::Vec3, f32, f32)>, // (position, yaw, pitch)
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

        let camera = camera_position.map(|(pos, yaw, pitch)| CameraDesc {
            position: [pos.x, pos.y, pos.z],
            yaw,
            pitch,
        });

        let desc = SceneDesc {
            version: SCENE_FILE_VERSION,
            spheres,
            cubes,
            camera,
        };

        let encoded = bincode::encode_to_vec(&desc, bincode::config::standard())?;
        let mut f = File::create(path)?;
        f.write_all(&encoded)?;
        Ok(())
    }

    /// Load a SceneDesc from a file and populate the provided `world` with
    /// entities. Existing world contents are left untouched; caller may
    /// clear the world beforehand if desired.
    /// Returns camera position and orientation if present in the scene file.
    pub fn load_from_file<P: AsRef<Path>>(
        &mut self,
        path: P,
        world: &mut World,
    ) -> Result<Option<(glam::Vec3, f32, f32)>, Box<dyn std::error::Error>> {
        let mut f = File::open(path)?;
        let mut buf = Vec::new();
        f.read_to_end(&mut buf)?;
        let desc: SceneDesc = bincode::decode_from_slice(&buf, bincode::config::standard())?.0;
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
    camera: Option<CameraDesc>,
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
    fn from_material(m: &engine_core::materials::MaterialType) -> Self {
        match m {
            engine_core::materials::MaterialType::Lambertian { albedo } => {
                MaterialDesc::Lambertian {
                    albedo: [albedo.x, albedo.y, albedo.z],
                }
            }
            engine_core::materials::MaterialType::Metal { albedo, fuzz } => MaterialDesc::Metal {
                albedo: [albedo.x, albedo.y, albedo.z],
                fuzz: *fuzz,
            },
            engine_core::materials::MaterialType::Dielectric { ref_indx } => {
                MaterialDesc::Dielectric {
                    ref_indx: *ref_indx,
                }
            }
        }
    }

    fn into_material_type(self) -> engine_core::materials::MaterialType {
        match self {
            MaterialDesc::Lambertian { albedo } => {
                engine_core::materials::MaterialType::Lambertian {
                    albedo: glam::Vec3::new(albedo[0], albedo[1], albedo[2]),
                }
            }
            MaterialDesc::Metal { albedo, fuzz } => engine_core::materials::MaterialType::Metal {
                albedo: glam::Vec3::new(albedo[0], albedo[1], albedo[2]),
                fuzz,
            },
            MaterialDesc::Dielectric { ref_indx } => {
                engine_core::materials::MaterialType::Dielectric { ref_indx }
            }
        }
    }
}
