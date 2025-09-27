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

    // Draw by material type (opaque first, transparent last).
    // Split each mesh's instances into opaque/transparent based on
    // the material parameters uploaded in `material_table`.
    let mats = material_table.as_slice();

    let mut cube_opaque: Vec<InstanceGpu> = Vec::new();
    let mut cube_trans: Vec<InstanceGpu> = Vec::new();
    for inst in &cube_instances {
        let idx = inst.material as usize;
        let is_transparent = if idx < mats.len() { mats[idx].is_transparent() } else { false };
        if is_transparent {
            cube_trans.push(*inst);
        } else {
            cube_opaque.push(*inst);
        }
    }

    let mut sph_opaque: Vec<InstanceGpu> = Vec::new();
    let mut sph_trans: Vec<InstanceGpu> = Vec::new();
    for inst in &sphere_instances {
        let idx = inst.material as usize;
        let is_transparent = if idx < mats.len() { mats[idx].is_transparent() } else { false };
        if is_transparent {
            sph_trans.push(*inst);
        } else {
            sph_opaque.push(*inst);
        }
    }

    // Render opaque geometry first (no finalize). Then render transparent
    // geometry last so blending composes correctly.
    renderer.render_mesh(cube_mesh_handle, &cube_opaque, camera, false);
    renderer.render_mesh(mesh_handle, &sph_opaque, camera, false);

    let mut did_any_trans = false;
    if !cube_trans.is_empty() {
        did_any_trans = true;
        let final_call = sph_trans.is_empty();
        renderer.render_mesh(cube_mesh_handle, &cube_trans, camera, final_call);
    }
    if !sph_trans.is_empty() {
        did_any_trans = true;
        renderer.render_mesh(mesh_handle, &sph_trans, camera, true);
    }
    if !did_any_trans {
        renderer.render_mesh(mesh_handle, &Vec::new(), camera, true);
    }
}
