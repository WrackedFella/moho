//! Scene save/load: serializes game-domain entity state (spheres, cubes, voxel
//! chunks, materials) alongside camera pose and lights into a compact binary
//! format. Extracted from `moho_renderer::scene` so the renderer doesn't need
//! to name game-domain types — this module owns that coupling instead.

use crate::actors::{Cube, Sphere};
use crate::scene::SceneEntities;
use bincode::{Decode, Encode};
use moho_core::materials::MaterialType;
use moho_render_api::{CameraDesc, LightDesc};
use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;

/// Type alias for camera data: (position, yaw, pitch)
pub type CameraData = (glam::Vec3, f32, f32);

/// Scene file version. Bump when the on-disk layout changes.
const SCENE_FILE_VERSION: u32 = 3;

/// Save a compact binary snapshot of the scene (method 2: bincode).
/// This writes a serialized SceneDesc containing all Spheres, Cubes, VoxelChunks,
/// Materials, and camera position/orientation. Reloadable into fresh [`SceneEntities`]
/// via `load_from_file`.
pub fn save_to_file<P: AsRef<Path>>(
    path: P,
    entities: &SceneEntities,
    camera_position: Option<CameraData>, // (position, yaw, pitch)
    lights: &[LightDesc],
) -> Result<(), Box<dyn std::error::Error>> {
    let encoded = encode_to_bytes(entities, camera_position, lights)?;
    let mut f = File::create(path)?;
    f.write_all(&encoded)?;
    Ok(())
}

/// Encode the scene descriptor to a Vec<u8> for in-memory handling.
/// This mirrors the on-disk serialization used by `save_to_file`.
pub fn encode_to_bytes(
    entities: &SceneEntities,
    camera_position: Option<CameraData>,
    lights: &[LightDesc],
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut spheres: Vec<SphereDesc> = Vec::new();
    for s in entities.actors.spheres() {
        spheres.push(SphereDesc {
            center: s.center.to_array(),
            radius: s.radius,
            material: MaterialDesc::from_material(&s.mat_ptr),
        });
    }
    let mut cubes: Vec<CubeDesc> = Vec::new();
    for c in entities.actors.cubes() {
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
    for chunk in entities.chunks.iter() {
        voxel_chunks.push(VoxelChunkDesc {
            chunk_pos: [
                chunk.chunk_pos().x,
                chunk.chunk_pos().y,
                chunk.chunk_pos().z,
            ],
            vertices: chunk.vertices().to_vec(),
            normals: chunk.normals().to_vec(),
            indices: chunk.indices().to_vec(),
            material_id: chunk.material_id(),
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
        lights: lights.to_vec(),
    };

    let encoded = bincode::encode_to_vec(&desc, bincode::config::standard())?;
    Ok(encoded)
}

/// Load a SceneDesc from a file and populate the provided `entities`.
/// Actors are appended to existing ones, and a loaded chunk replaces any
/// existing chunk at the same position; clear `entities` beforehand for a
/// fresh load.
/// Returns camera data and any persisted lights.
pub fn load_from_file<P: AsRef<Path>>(
    path: P,
    entities: &mut SceneEntities,
) -> Result<(Option<CameraData>, Vec<LightDesc>), Box<dyn std::error::Error>> {
    let mut f = File::open(path)?;
    let mut buf = Vec::new();
    f.read_to_end(&mut buf)?;
    // Delegate to helper that decodes from bytes so load_from_bytes can reuse it.
    decode_and_populate(&buf, entities)
}

/// Decode a scene saved as bincode bytes and populate `entities`.
/// Actors are appended and a loaded chunk replaces any chunk at the same
/// position, as in [`load_from_file`].
/// Returns camera data and any persisted lights.
pub fn load_from_bytes(
    bytes: &[u8],
    entities: &mut SceneEntities,
) -> Result<(Option<CameraData>, Vec<LightDesc>), Box<dyn std::error::Error>> {
    decode_and_populate(bytes, entities)
}

/// Internal helper to decode SceneDesc from bytes and populate entities.
fn decode_and_populate(
    bytes: &[u8],
    entities: &mut SceneEntities,
) -> Result<(Option<CameraData>, Vec<LightDesc>), Box<dyn std::error::Error>> {
    let cfg = bincode::config::standard();

    // Try decoding as v3 (includes lights). For v1/v2 saves this will fail
    // because the lights field doesn't exist in the binary data.
    if let Ok((desc, _)) = bincode::decode_from_slice::<SceneDesc, _>(bytes, cfg)
        && desc.version >= 1
        && desc.version <= SCENE_FILE_VERSION
    {
        return populate_entities(
            desc.spheres,
            desc.cubes,
            desc.voxel_chunks,
            desc.camera,
            desc.lights,
            entities,
        );
    }

    // Fallback: try decoding as legacy v1/v2 (without lights field).
    let (legacy, _) = bincode::decode_from_slice::<SceneDescLegacy, _>(bytes, cfg)
        .map_err(|e| format!("failed to decode scene: {e}"))?;

    if legacy.version < 1 || legacy.version > 2 {
        return Err(format!(
            "unsupported scene file version: {} (expected 1-{})",
            legacy.version, SCENE_FILE_VERSION
        )
        .into());
    }

    tracing::info!(
        version = legacy.version,
        "Loaded legacy scene format — spawned lights will not be restored"
    );
    populate_entities(
        legacy.spheres,
        legacy.cubes,
        legacy.voxel_chunks,
        legacy.camera,
        vec![],
        entities,
    )
}

/// Shared population logic used by both v3 and legacy load paths.
fn populate_entities(
    spheres: Vec<SphereDesc>,
    cubes: Vec<CubeDesc>,
    voxel_chunks: Vec<VoxelChunkDesc>,
    camera: Option<CameraDesc>,
    lights: Vec<LightDesc>,
    entities: &mut SceneEntities,
) -> Result<(Option<CameraData>, Vec<LightDesc>), Box<dyn std::error::Error>> {
    for s in spheres {
        let mat = s.material.into_material_type();
        let sphere = Sphere::new(glam::Vec3::from_array(s.center), s.radius, mat);
        entities.actors.spawn_sphere(sphere);
    }
    for c in cubes {
        let mat = c.material.into_material_type();
        let cube = Cube::new(
            glam::Vec3::from_array(c.center),
            c.length,
            c.width,
            c.height,
            mat,
        );
        entities.actors.spawn_cube(cube);
    }
    for chunk_desc in voxel_chunks {
        let vertex_count = chunk_desc.vertices.len();
        let chunk = moho_core::voxel::VoxelChunk::new(
            glam::IVec3::new(
                chunk_desc.chunk_pos[0],
                chunk_desc.chunk_pos[1],
                chunk_desc.chunk_pos[2],
            ),
            chunk_desc.vertices,
            chunk_desc.normals,
            vec![1.0; vertex_count],
            vec![1; vertex_count],
            vec![1.0; vertex_count],
            vec![[1.0, 1.0, 1.0]; vertex_count],
            vec![1.0; vertex_count],
            chunk_desc.indices,
            chunk_desc.material_id,
        );
        let chunk_pos = chunk.chunk_pos();
        if entities.chunks.insert(chunk).is_some() {
            tracing::warn!(
                chunk = ?chunk_pos,
                "Scene holds more than one chunk at position; the earlier one was replaced"
            );
        }
    }

    let camera_data = camera.map(|cam| (glam::Vec3::from_array(cam.position), cam.yaw, cam.pitch));

    Ok((camera_data, lights))
}

/// Serializable scene descriptor used for bincode snapshotting.
#[derive(Encode, Decode)]
struct SceneDesc {
    version: u32,
    spheres: Vec<SphereDesc>,
    cubes: Vec<CubeDesc>,
    voxel_chunks: Vec<VoxelChunkDesc>,
    camera: Option<CameraDesc>,
    lights: Vec<LightDesc>,
}

/// Legacy scene descriptor for v1/v2 saves (no lights field).
/// Used as a fallback when the current SceneDesc fails to decode.
#[derive(Encode, Decode)]
struct SceneDescLegacy {
    version: u32,
    spheres: Vec<SphereDesc>,
    cubes: Vec<CubeDesc>,
    voxel_chunks: Vec<VoxelChunkDesc>,
    camera: Option<CameraDesc>,
}

#[derive(Encode, Decode)]
struct VoxelChunkDesc {
    chunk_pos: [i32; 3],
    vertices: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    indices: Vec<u32>,
    material_id: u32,
}

#[derive(Encode, Decode)]
struct SphereDesc {
    center: [f32; 3],
    radius: f32,
    material: MaterialDesc,
}

#[derive(Encode, Decode)]
struct CubeDesc {
    center: [f32; 3],
    length: f32,
    width: f32,
    height: f32,
    material: MaterialDesc,
}

#[derive(Encode, Decode)]
enum MaterialDesc {
    Lambertian {
        albedo: [f32; 3],
    },
    Metal {
        albedo: [f32; 3],
        fuzz: f32,
    },
    Dielectric {
        ref_indx: f32,
    },
    Emissive {
        color: [f32; 3],
        intensity: f32,
    },
    VoxelTerrain {
        top_albedo: [f32; 3],
        side_albedo: [f32; 3],
    },
}

impl MaterialDesc {
    fn from_material(m: &MaterialType) -> Self {
        match m {
            MaterialType::Lambertian { albedo } => MaterialDesc::Lambertian {
                albedo: [albedo.x, albedo.y, albedo.z],
            },
            MaterialType::Metal { albedo, fuzz } => MaterialDesc::Metal {
                albedo: [albedo.x, albedo.y, albedo.z],
                fuzz: *fuzz,
            },
            MaterialType::Dielectric { ref_indx } => MaterialDesc::Dielectric {
                ref_indx: *ref_indx,
            },
            MaterialType::Emissive { color, intensity } => MaterialDesc::Emissive {
                color: [color.x, color.y, color.z],
                intensity: *intensity,
            },
            MaterialType::VoxelTerrain {
                top_albedo,
                side_albedo,
            } => MaterialDesc::VoxelTerrain {
                top_albedo: [top_albedo.x, top_albedo.y, top_albedo.z],
                side_albedo: [side_albedo.x, side_albedo.y, side_albedo.z],
            },
        }
    }

    fn into_material_type(self) -> MaterialType {
        match self {
            MaterialDesc::Lambertian { albedo } => MaterialType::Lambertian {
                albedo: glam::Vec3::new(albedo[0], albedo[1], albedo[2]),
            },
            MaterialDesc::Metal { albedo, fuzz } => MaterialType::Metal {
                albedo: glam::Vec3::new(albedo[0], albedo[1], albedo[2]),
                fuzz,
            },
            MaterialDesc::Dielectric { ref_indx } => MaterialType::Dielectric { ref_indx },
            MaterialDesc::Emissive { color, intensity } => MaterialType::Emissive {
                color: glam::Vec3::new(color[0], color[1], color[2]),
                intensity,
            },
            MaterialDesc::VoxelTerrain {
                top_albedo,
                side_albedo,
            } => MaterialType::VoxelTerrain {
                top_albedo: glam::Vec3::new(top_albedo[0], top_albedo[1], top_albedo[2]),
                side_albedo: glam::Vec3::new(side_albedo[0], side_albedo[1], side_albedo[2]),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actors::{Cube, Sphere};
    use crate::scene::SceneEntities;
    use moho_core::materials::MaterialType;
    use moho_core::voxel::VoxelChunk;

    #[test]
    fn camera_and_lights_round_trip_through_encode_decode() {
        let entities = SceneEntities::default();

        let cam_pos = glam::Vec3::new(1.5, 2.5, 3.5);
        let yaw = 0.123_f32;
        let pitch = -0.456_f32;
        let lights = vec![
            LightDesc {
                position: [10.0, 20.0, 30.0],
                color: [1.0, 0.5, 0.0],
                intensity: 2.5,
                range: 50.0,
                enabled: true,
            },
            LightDesc {
                position: [-5.0, 0.0, 5.0],
                color: [0.0, 1.0, 1.0],
                intensity: 1.0,
                range: 25.0,
                enabled: false,
            },
        ];

        let bytes =
            encode_to_bytes(&entities, Some((cam_pos, yaw, pitch)), &lights).expect("encode");

        let mut loaded = SceneEntities::default();
        let (cam_out, lights_out) = load_from_bytes(&bytes, &mut loaded).expect("decode");

        let (pos_out, yaw_out, pitch_out) = cam_out.expect("camera present");
        assert!((pos_out.x - cam_pos.x).abs() < 1e-6);
        assert!((pos_out.y - cam_pos.y).abs() < 1e-6);
        assert!((pos_out.z - cam_pos.z).abs() < 1e-6);
        assert!((yaw_out - yaw).abs() < 1e-6);
        assert!((pitch_out - pitch).abs() < 1e-6);

        assert_eq!(lights_out.len(), 2);
        assert!((lights_out[0].intensity - 2.5).abs() < 1e-6);
        assert!(lights_out[0].enabled);
        assert!(!lights_out[1].enabled);
        assert!((lights_out[1].range - 25.0).abs() < 1e-6);
    }

    #[test]
    fn encode_without_camera_round_trips() {
        let entities = SceneEntities::default();

        let bytes = encode_to_bytes(&entities, None, &[]).expect("encode");

        let mut loaded = SceneEntities::default();
        let (cam_out, lights_out) = load_from_bytes(&bytes, &mut loaded).expect("decode");

        assert!(cam_out.is_none());
        assert!(lights_out.is_empty());
    }

    fn meshed_chunk(pos: glam::IVec3, seed: f32, material_id: u32) -> VoxelChunk {
        VoxelChunk::new(
            pos,
            vec![[seed, 0.0, 0.0], [seed + 1.0, 0.0, 0.0], [seed, 1.0, 0.0]],
            vec![[0.0, 0.0, 1.0]; 3],
            vec![1.0; 3],
            vec![1; 3],
            vec![1.0; 3],
            vec![[1.0, 1.0, 1.0]; 3],
            vec![1.0; 3],
            vec![0, 1, 2],
            material_id,
        )
    }

    #[test]
    fn round_trip_preserves_spheres_cubes_and_chunks() {
        let mut entities = SceneEntities::default();
        let sphere_a = Sphere::new(
            glam::Vec3::new(1.0, 2.0, 3.0),
            1.5,
            MaterialType::Lambertian {
                albedo: glam::Vec3::new(0.1, 0.2, 0.3),
            },
        );
        let sphere_b = Sphere::new(
            glam::Vec3::new(-4.0, 5.0, 6.0),
            0.25,
            MaterialType::Metal {
                albedo: glam::Vec3::new(0.7, 0.8, 0.9),
                fuzz: 0.2,
            },
        );
        let cube = Cube::new(
            glam::Vec3::new(7.0, 8.0, 9.0),
            1.0,
            2.0,
            3.0,
            MaterialType::Lambertian {
                albedo: glam::Vec3::new(0.4, 0.5, 0.6),
            },
        );
        entities.actors.spawn_sphere(sphere_a);
        entities.actors.spawn_sphere(sphere_b);
        entities.actors.spawn_cube(cube);
        entities
            .chunks
            .insert(meshed_chunk(glam::IVec3::new(2, 0, 0), 20.0, 7));
        entities
            .chunks
            .insert(meshed_chunk(glam::IVec3::new(-1, 0, 3), 30.0, 8));
        entities
            .chunks
            .insert(meshed_chunk(glam::IVec3::new(0, 0, 0), 10.0, 9));

        let bytes = encode_to_bytes(&entities, None, &[]).expect("encode");
        let mut loaded = SceneEntities::default();
        load_from_bytes(&bytes, &mut loaded).expect("decode");

        let spheres = loaded.actors.spheres();
        assert_eq!(spheres.len(), 2);
        for (got, want) in spheres.iter().zip([sphere_a, sphere_b]) {
            assert_eq!(got.center, want.center);
            assert_eq!(got.radius, want.radius);
            assert_eq!(got.mat_ptr, want.mat_ptr);
        }

        let cubes = loaded.actors.cubes();
        assert_eq!(cubes.len(), 1);
        assert_eq!(cubes[0].center, cube.center);
        assert_eq!(
            (cubes[0].length, cubes[0].width, cubes[0].height),
            (1.0, 2.0, 3.0)
        );
        assert_eq!(cubes[0].mat_ptr, cube.mat_ptr);

        let got: Vec<&VoxelChunk> = loaded.chunks.iter().collect();
        let want: Vec<&VoxelChunk> = entities.chunks.iter().collect();
        assert_eq!(got.len(), 3);
        let positions: Vec<glam::IVec3> = got.iter().map(|c| c.chunk_pos()).collect();
        assert_eq!(
            positions,
            vec![
                glam::IVec3::new(-1, 0, 3),
                glam::IVec3::new(0, 0, 0),
                glam::IVec3::new(2, 0, 0),
            ]
        );
        for (g, w) in got.iter().zip(&want) {
            assert_eq!(g.chunk_pos(), w.chunk_pos());
            assert_eq!(g.vertices(), w.vertices());
            assert_eq!(g.normals(), w.normals());
            assert_eq!(g.indices(), w.indices());
            assert_eq!(g.material_id(), w.material_id());
        }
        let ids: Vec<u32> = got.iter().map(|c| c.material_id()).collect();
        assert_eq!(ids, vec![8, 9, 7]);
    }

    #[test]
    fn save_to_file_then_load_from_file_round_trips() {
        let path = std::env::temp_dir().join(format!(
            "moho_save_to_file_then_load_from_file_round_trips_{}.scene",
            std::process::id()
        ));
        let mut entities = SceneEntities::default();
        entities.actors.spawn_sphere(Sphere::new(
            glam::Vec3::new(1.0, 2.0, 3.0),
            0.75,
            MaterialType::Lambertian {
                albedo: glam::Vec3::new(0.1, 0.2, 0.3),
            },
        ));
        entities
            .chunks
            .insert(meshed_chunk(glam::IVec3::new(4, 0, -2), 1.0, 3));
        let cam_pos = glam::Vec3::new(5.0, 6.0, 7.0);
        let light = LightDesc {
            position: [1.0, 2.0, 3.0],
            color: [0.5, 0.25, 1.0],
            intensity: 3.0,
            range: 12.0,
            enabled: true,
        };

        let saved = save_to_file(&path, &entities, Some((cam_pos, 0.5, -0.25)), &[light]);
        let mut loaded = SceneEntities::default();
        let result = load_from_file(&path, &mut loaded);
        let _ = std::fs::remove_file(&path);

        saved.expect("save");
        let (camera, lights) = result.expect("load");
        let (pos, yaw, pitch) = camera.expect("camera present");
        assert_eq!(pos, cam_pos);
        assert_eq!((yaw, pitch), (0.5, -0.25));
        assert_eq!(lights.len(), 1);
        assert_eq!(lights[0].position, [1.0, 2.0, 3.0]);
        assert_eq!(lights[0].intensity, 3.0);
        assert_eq!(loaded.actors.spheres().len(), 1);
        assert_eq!(
            loaded.actors.spheres()[0].center,
            glam::Vec3::new(1.0, 2.0, 3.0)
        );
        let positions: Vec<glam::IVec3> = loaded
            .chunks
            .iter()
            .map(moho_core::voxel::VoxelChunk::chunk_pos)
            .collect();
        assert_eq!(positions, vec![glam::IVec3::new(4, 0, -2)]);
    }

    #[test]
    fn load_with_duplicate_chunk_positions_keeps_the_last() {
        let chunk_desc = |material_id: u32| VoxelChunkDesc {
            chunk_pos: [3, 0, 1],
            vertices: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            normals: vec![[0.0, 0.0, 1.0]; 3],
            indices: vec![0, 1, 2],
            material_id,
        };
        let desc = SceneDesc {
            version: SCENE_FILE_VERSION,
            spheres: Vec::new(),
            cubes: Vec::new(),
            voxel_chunks: vec![chunk_desc(11), chunk_desc(22)],
            camera: None,
            lights: Vec::new(),
        };
        let bytes = bincode::encode_to_vec(&desc, bincode::config::standard()).expect("encode");

        let mut loaded = SceneEntities::default();
        load_from_bytes(&bytes, &mut loaded).expect("decode");

        assert_eq!(loaded.chunks.len(), 1);
        let chunk = loaded
            .chunks
            .get(glam::IVec3::new(3, 0, 1))
            .expect("chunk present");
        assert_eq!(chunk.material_id(), 22);
    }
}
