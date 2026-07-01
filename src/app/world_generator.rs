//! World generation — spawns the async generation thread and wires it to the app.
//!
//! The thread closure captures only the data it needs (spec, cancel flag, sender)
//! so the main thread remains free to process events while generation runs.
//! Progress messages flow back via `GenerationMsg`; `GenerationProcessor` polls
//! them each frame.

use crossbeam_channel::unbounded;
use legion::World;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// Start async world generation for the given `spec`.
///
/// Spawns the "world-generator" thread, stores the receiver/handle/cancel flag
/// on `app.generation`, and immediately shows the progress overlay in the UI.
pub fn generate_new_world(
    app: &mut crate::App,
    spec: moho_game::scene_builders::WorldSpec,
) -> Result<(), Box<dyn std::error::Error>> {
    log::info!("Starting async generation for spec={:?}", spec);

    use std::path::PathBuf;

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

    // Spawn the generation thread
    let handle = std::thread::Builder::new()
        .name("world-generator".to_string())
        .spawn(move || {
            // Report initial progress
            let _ = sender.send(crate::GenerationMsg::Progress(0.02));

            // Local world and scene used for generation
            let local_world = World::default();
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
            let local_scene = moho_renderer::Scene::new();
            // Place camera above world center looking slightly down
            let camera_height = 24.0f32;
            let camera_position = glam::Vec3::new(0.0, camera_height, 0.0);
            // yaw = 0.0 (look along +Z), pitch negative to look downward
            let camera_yaw = 0.0f32;
            let camera_pitch = -0.4f32;
            let scene_bytes = match local_scene.encode_to_bytes(
                &local_world,
                Some((camera_position, camera_yaw, camera_pitch)),
                &[], // fresh world — no spawned lights
            ) {
                Ok(b) => b,
                Err(e) => {
                    let _ = sender.send(crate::GenerationMsg::Failed(format!(
                        "encode failed: {}",
                        e
                    )));
                    return;
                }
            };

            let _ = sender.send(crate::GenerationMsg::Progress(0.9));

            // Ensure saves directory exists and persist the envelope
            let saves_dir = PathBuf::from("saves");
            if !saves_dir.exists()
                && let Err(e) = std::fs::create_dir_all(&saves_dir)
            {
                let _ = sender.send(crate::GenerationMsg::Failed(format!("mkdir failed: {}", e)));
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
                let _ = sender.send(crate::GenerationMsg::Failed(format!("write failed: {}", e)));
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
