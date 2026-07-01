pub fn auto_save_on_shutdown(app: &mut crate::App) -> Result<(), Box<dyn std::error::Error>> {
    log::info!("Auto-saving on shutdown...");

    let saves_dir = std::path::Path::new("saves");
    if !saves_dir.exists() {
        std::fs::create_dir_all(saves_dir)?;
    }

    let save_path = saves_dir.join("scene.bin");
    let (yaw, pitch) = app.simulation.yaw_pitch();
    let camera_data = Some((app.simulation.position(), yaw, pitch));

    let scene_bytes = app.scene.encode_to_bytes(
        &app.world,
        camera_data,
        &app.window_renderer
            .as_ref()
            .map_or_else(Vec::new, |wr| wr.renderer.all_lights_as_descs()),
    )?;
    let mut spec =
        app.generation
            .last_spec
            .clone()
            .unwrap_or(moho_game::scene_builders::WorldSpec {
                name: "autosave".to_string(),
                seed: None,
                size_xz: 64,
                day_length_seconds: 600.0,
                night_length_seconds: 420.0,
                initial_time_of_day: 6.0,
            });
    spec.initial_time_of_day = app.simulation.time_of_day();
    let block_records: Vec<crate::save::BlockRecord> = app
        .light_system
        .as_ref()
        .map(|ls| {
            ls.grid()
                .iter_block_data()
                .map(crate::save::BlockRecord::from_block_data)
                .collect()
        })
        .unwrap_or_default();
    crate::save::write_scene_with_metadata(&save_path, &scene_bytes, &spec, &block_records)?;
    log::info!(
        "Auto-saved scene (envelope) to {:?} (spec={:?}, {} blocks)",
        save_path,
        spec.name,
        block_records.len()
    );

    Ok(())
}
