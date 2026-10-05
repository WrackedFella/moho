//! World generation — spawns the async generation thread and wires it to the app.
//!
//! The thread closure captures only the data it needs (spec, cancel flag, sender)
//! so the main thread remains free to process events while generation runs.
//! Progress messages flow back via `GenerationMsg`; `GenerationProcessor` polls
//! them each frame.

use crossbeam_channel::unbounded;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// Start async world generation for the given `spec`.
///
/// Spawns the "world-generator" thread, stores the receiver/handle/cancel flag
/// on `app.generation`, and immediately shows the progress overlay in the UI.
/// The thread writes the new world to `saves_dir/scene.bin`.
pub fn generate_new_world(
    app: &mut crate::App,
    spec: moho_game::scene_builders::WorldSpec,
    saves_dir: &std::path::Path,
) -> Result<(), Box<dyn std::error::Error>> {
    log::info!("Starting async generation for spec={spec:?}");

    // Prepare communication channel and cancellation flag
    let (tx, rx) = unbounded::<crate::GenerationMsg>();
    let cancel_flag = Arc::new(AtomicBool::new(false));

    // Start UI progress overlay immediately
    if let Some(ui_adapter) = &app.ui_adapter
        && let Ok(mut a) = ui_adapter.lock()
    {
        a.start_progress(format!("Generating {}", spec.name), true);
        a.set_progress(0.01);
    }

    // Save shared state into local variables for the thread.
    let spec_for_thread = spec;
    let cancel_clone = cancel_flag.clone();
    let sender = tx.clone();
    let saves_dir = saves_dir.to_path_buf();

    // Spawn the generation thread
    let handle = std::thread::Builder::new()
        .name("world-generator".to_string())
        .spawn(move || {
            // Report initial progress
            let _ = sender.send(crate::GenerationMsg::Progress(0.02));

            let local_entities = moho_game::scene::SceneEntities::default();
            // Step 1: clear/build
            if cancel_clone.load(Ordering::Relaxed) {
                let _ = sender.send(crate::GenerationMsg::Canceled);
                return;
            }
            // Build TerrainConfig from the WorldSpec.
            let mut terrain_config = moho_game::scene_builders::TerrainConfig::default();
            if let Some(s) = spec_for_thread.seed {
                terrain_config.seed = s as u32;
            }
            terrain_config.world_size = spec_for_thread.size_xz;

            // With streaming, we no longer pre-generate the full world here.
            // An empty grid is returned; ChunkStreamer fills it on demand.
            let grid = moho_core::voxel::VoxelGrid::new(16);
            let _ = sender.send(crate::GenerationMsg::Progress(0.6));

            if cancel_clone.load(Ordering::Relaxed) {
                let _ = sender.send(crate::GenerationMsg::Canceled);
                return;
            }

            // Encode scene bytes. Provide a sensible default camera for
            // newly generated worlds so the app has a starting
            // viewpoint instead of relying on previous controller state.
            // Place camera above world center looking slightly down
            let camera_height = 24.0f32;
            let camera_position = glam::Vec3::new(0.0, camera_height, 0.0);
            // yaw = 0.0 (look along +Z), pitch negative to look downward
            let camera_yaw = 0.0f32;
            let camera_pitch = -0.4f32;
            let scene_bytes = match moho_game::scene_persistence::encode_to_bytes(
                &local_entities,
                Some((camera_position, camera_yaw, camera_pitch)),
                &[], // fresh world — no spawned lights
            ) {
                Ok(b) => b,
                Err(e) => {
                    let _ =
                        sender.send(crate::GenerationMsg::Failed(format!("encode failed: {e}")));
                    return;
                }
            };

            let _ = sender.send(crate::GenerationMsg::Progress(0.9));

            // Ensure saves directory exists and persist the envelope
            if let Err(e) = std::fs::create_dir_all(&saves_dir) {
                let _ = sender.send(crate::GenerationMsg::Failed(format!("mkdir failed: {e}")));
                return;
            }
            let save_path = saves_dir.join("scene.bin");
            let block_records: Vec<crate::save::BlockRecord> = grid
                .iter_block_data()
                .map(crate::save::BlockRecord::from_block_data)
                .collect();
            if let Err(e) = crate::save::write_scene_with_metadata(
                &save_path,
                &scene_bytes,
                &spec_for_thread,
                &block_records,
            ) {
                let _ = sender.send(crate::GenerationMsg::Failed(format!("write failed: {e}")));
                return;
            }

            let _ = sender.send(crate::GenerationMsg::Progress(1.0));
            let _ = sender.send(crate::GenerationMsg::Completed {
                scene_bytes,
                spec: spec_for_thread,
                terrain_config,
                grid: Box::new(grid),
            });
        })?;

    // Store receiver, handle and cancel flag so the main loop can poll it
    app.generation.receiver = Some(rx);
    app.generation.handle = Some(handle);
    app.generation.cancel = Some(cancel_flag);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{App, GenerationMsg};
    use std::time::Duration;

    #[test]
    fn generate_new_world_starts_job_that_writes_the_scene_and_completes() {
        let temp = tempfile::tempdir().expect("temp dir");
        let saves = temp.path().join("not").join("yet").join("created");
        let mut app = App::headless();
        let spec = moho_game::scene_builders::WorldSpec {
            name: "headless-test-generate".to_string(),
            seed: Some(7),
            size_xz: 64,
            day_length_seconds: 600.0,
            night_length_seconds: 420.0,
            initial_time_of_day: 6.0,
        };

        generate_new_world(&mut app, spec, &saves).expect("generation starts");
        let rx = app
            .generation
            .receiver
            .take()
            .expect("job receiver is stored");
        let completed_name = loop {
            match rx
                .recv_timeout(Duration::from_secs(10))
                .expect("job reports before timeout")
            {
                GenerationMsg::Progress(_) => {}
                GenerationMsg::Completed { spec, .. } => break spec.name,
                GenerationMsg::Canceled => panic!("job canceled"),
                GenerationMsg::Failed(reason) => panic!("job failed: {reason}"),
            }
        };

        assert_eq!(completed_name, "headless-test-generate");
        assert!(app.generation.handle.is_some());
        assert!(saves.join("scene.bin").is_file());
    }
}
