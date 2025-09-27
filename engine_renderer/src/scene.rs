use crate::{MaterialTable, RendererBackend};
use engine_core::actors::{Cube, InstanceGpu, Sphere};
use legion::World;
use legion::query::IntoQuery;

/// Collect renderable instances from the ECS world, update the material table,
/// upload materials if dirty, and issue renderer calls to draw opaque then
/// transparent geometry. This consolidates per-frame rendering concerns
/// (material dedupe, instance building, opaque/transparent split, finalizing)
/// so application code can remain minimal.
pub fn render_world(
    renderer: &mut dyn RendererBackend,
    world: &World,
    material_table: &mut MaterialTable,
    mesh_handle: u32,
    cube_mesh_handle: u32,
    camera: (glam::Mat4, glam::Mat4, glam::Vec3),
) {
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

    // Debug: print material table and instance material indices (kept as-is)
    if !material_table.as_slice().is_empty() {
        println!("[debug] material_table.len={} ", material_table.as_slice().len());
        for (i, m) in material_table.as_slice().iter().enumerate().take(8) {
            println!(
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
        println!("[debug] sphere_inst[{}].material={}", i, inst.material);
    }
    for (i, inst) in cube_instances.iter().enumerate().take(8) {
        println!("[debug] cube_inst[{}].material={}", i, inst.material);
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
        let is_transparent = if idx < mats.len() { mats[idx].is_transparent() } else { false };
        if is_transparent {
            transparent_entries.push((cube_mesh_handle, *inst));
        } else {
            cube_opaque.push(*inst);
        }
    }
    for inst in &sphere_instances {
        let idx = inst.material as usize;
        let is_transparent = if idx < mats.len() { mats[idx].is_transparent() } else { false };
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
    let mut by_depth: Vec<(f32, u32, InstanceGpu)> = Vec::with_capacity(transparent_entries.len());
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
        if let Some((last_mesh, vec)) = groups.last_mut() {
            if *last_mesh == mesh_h {
                vec.push(inst);
                continue;
            }
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
