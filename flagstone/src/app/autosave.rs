/// Time of play between autosaves of an edited world.
pub const DEFAULT_AUTOSAVE_INTERVAL: std::time::Duration = std::time::Duration::from_secs(120);

/// When an edited world is next due an autosave, counted in simulation ticks.
///
/// A world is due once it has been edited since the last save and at least
/// `interval_ticks` ticks have passed since that save. An idle world is never due.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutosaveSchedule {
    interval_ticks: u64,
    ticks_since_save: u64,
    edited: bool,
}

impl AutosaveSchedule {
    pub fn new(interval_ticks: u64) -> Self {
        Self {
            interval_ticks,
            ticks_since_save: 0,
            edited: false,
        }
    }

    /// Counts one simulation tick of play.
    pub fn tick(&mut self) {
        self.ticks_since_save = self.ticks_since_save.saturating_add(1);
    }

    /// Records that the world changed since the last save.
    pub fn mark_edited(&mut self) {
        self.edited = true;
    }

    /// Whether the world has been edited and a full interval has passed since the last save.
    pub fn due(&self) -> bool {
        self.edited && self.ticks_since_save >= self.interval_ticks
    }

    /// A save succeeded: nothing is unsaved and the interval starts again.
    pub fn saved(&mut self) {
        self.ticks_since_save = 0;
        self.edited = false;
    }

    /// A save failed: the interval starts again but the edits stay unsaved, so the next
    /// attempt comes one interval later rather than on the next frame.
    pub fn save_failed(&mut self) {
        self.ticks_since_save = 0;
    }
}

/// Save the current scene to `saves_dir/scene.bin`, creating the directory
/// if needed.
///
/// Writes nothing and returns `Ok` when no world is loaded, so closing from the start
/// menu cannot overwrite an existing save with an empty world.
pub fn auto_save_on_shutdown(
    app: &mut crate::App,
    clock: &moho_app::GameClock,
    saves_dir: &std::path::Path,
    lights: &[moho_render_api::LightDesc],
) -> Result<(), Box<dyn std::error::Error>> {
    let Some(mut spec) = app.generation.last_spec.clone() else {
        tracing::info!("No world loaded; nothing to auto-save");
        return Ok(());
    };
    tracing::info!("Auto-saving...");
    let started = std::time::Instant::now();

    std::fs::create_dir_all(saves_dir)?;

    let save_path = saves_dir.join("scene.bin");
    let (yaw, pitch) = app.simulation.yaw_pitch();
    let camera_data = Some((app.simulation.position(), yaw, pitch));

    let scene_bytes =
        flagstone_game::scene_persistence::encode_to_bytes(&app.entities, camera_data, lights)?;
    spec.initial_time_of_day = clock.time_of_day();
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
    tracing::info!(
        path = %save_path.display(),
        spec = ?spec.name,
        count = block_records.len(),
        elapsed_ms = started.elapsed().as_millis(),
        "Auto-saved scene (envelope)"
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::App;
    use crate::app::chunk_streamer::ChunkStreamer;
    use crate::app::event_loop::{EventProcessor, FrameProcessor};
    use crate::game_state::GameState;
    use crate::input_event::InputEvent;
    use flagstone_game::TICK_HZ;
    use flagstone_game::controller::CameraMode;
    use flagstone_game::scene_builders::{TerrainConfig, WorldSpec};
    use flagstone_voxel::{BlockPos, StreamingConfig, WorldEvent};
    use glam::Vec3;
    use moho_app::{EventContext, FrameContext, Game, GameClock, HeadlessLoop, LoopConfig};
    use moho_core::events::UiEvent;
    use std::path::{Path, PathBuf};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;
    use winit::event::WindowEvent;

    const WIDE_WORLD_SIZE: u32 = 256;

    fn world_spec(name: &str) -> WorldSpec {
        WorldSpec {
            name: name.to_string(),
            size_xz: WIDE_WORLD_SIZE,
            ..WorldSpec::default()
        }
    }

    fn saves_dir(temp: &tempfile::TempDir) -> PathBuf {
        temp.path().join("saves")
    }

    fn scene_file(saves: &Path) -> PathBuf {
        saves.join("scene.bin")
    }

    fn tick_length() -> Duration {
        LoopConfig::new(TICK_HZ).tick_length()
    }

    /// Removes the chunk files a world's evictions leave under the working directory's
    /// `saves/` while chunk files do not follow the configured saves directory.
    struct WorkingDirChunks(&'static str);

    impl Drop for WorkingDirChunks {
        fn drop(&mut self) {
            let world_dir = Path::new("saves").join(self.0);
            if let Err(error) = std::fs::remove_dir_all(&world_dir)
                && error.kind() != std::io::ErrorKind::NotFound
            {
                eprintln!("could not remove {}: {error}", world_dir.display());
            }
            // Fails while another test's world is still under `saves/`, which is fine.
            std::fs::remove_dir("saves").ok();
        }
    }

    fn streamer(world: &str, saves: &Path) -> ChunkStreamer {
        ChunkStreamer::new(
            TerrainConfig {
                world_size: WIDE_WORLD_SIZE,
                ..TerrainConfig::default()
            },
            StreamingConfig {
                load_radius_chunks: 1,
                unload_radius_chunks: 2,
                chunks_per_frame: 1,
            },
            world,
            saves.to_path_buf(),
        )
    }

    /// A loaded wide world, playing in the strategy view with the pawn over column
    /// (40, 40) of chunk column (2, 2). The streamer keeps only that 5x5 block of chunk
    /// columns. Autosaves go to `temp`'s saves directory, which holds no save yet.
    fn loaded_session(world: &str, temp: &tempfile::TempDir) -> App {
        let seed_dir = temp.path().join("seed");
        let mut seed = App::headless();
        seed.generation.last_spec = Some(world_spec(world));
        seed.simulation
            .set_position_yaw_pitch(Vec3::new(40.5, 200.0, 40.5), 0.0, 0.0);
        auto_save_on_shutdown(&mut seed, &GameClock::default(), &seed_dir, &[]).expect("seed save");
        let mut app = App::headless();
        app.saves_dir = saves_dir(temp);
        crate::app::scene_loader::load_scene(
            &mut app,
            &mut GameClock::default(),
            &scene_file(&seed_dir),
        )
        .expect("the seed world loads");
        assert_eq!(app.game_state, GameState::Playing);
        app.chunk_streamer = Some(streamer(world, &app.saves_dir));
        app.simulation.set_camera_mode(CameraMode::Isometric);
        app
    }

    /// The player mines the surface block over column (`x`, `z`) and returns its position.
    fn mine_surface_block(app: &mut App, x: i32, z: i32) -> BlockPos {
        let height = app
            .light_system
            .as_ref()
            .expect("light system")
            .grid()
            .get_height(x, z)
            .expect("terrain under the column");
        let eye = Vec3::new(x as f32 + 0.5, height as f32 + 3.0, z as f32 + 0.5);
        app.camera.0 = glam::camera::rh::view::look_to_mat4(eye, Vec3::NEG_Y, Vec3::Z);
        app.simulation.set_camera_mode(CameraMode::FirstPerson);
        let removed = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&removed);
        app.event_bus.subscribe(move |event: &WorldEvent| {
            if let WorldEvent::BlockRemoved { position, .. } = event {
                sink.lock().expect("removal sink").push(*position);
            }
        });
        let (tx, rx) = std::sync::mpsc::channel();
        app.input.unconsumed_tx = Some(tx.clone());
        app.input.unconsumed_rx = Some(rx);
        tx.send(InputEvent::MineRequested).expect("channel open");

        EventProcessor::new().process_input_events(app);

        app.simulation.set_camera_mode(CameraMode::Isometric);
        let removed = removed.lock().expect("removal sink");
        assert_eq!(removed.len(), 1, "the player mined one block: {removed:?}");
        let mined = removed[0];
        let grid = app.light_system.as_ref().expect("light system").grid();
        assert_eq!(grid.material_at(mined), None, "the block is gone");
        mined
    }

    /// Moves the pawn to column (`x`, `z`) and streams once, as a frame would.
    fn walk_to(app: &mut App, x: f32, z: f32) {
        let (yaw, pitch) = app.simulation.yaw_pitch();
        let y = app.simulation.position().y;
        app.simulation
            .set_position_yaw_pitch(Vec3::new(x, y, z), yaw, pitch);
        FrameProcessor::new().update_chunk_streaming(app);
    }

    /// Closes the window; returns whether the game asked to exit.
    fn close_window(app: &mut App) -> bool {
        let clock = GameClock::default();
        let mut ctx = EventContext::new(&clock, None, None);
        Game::event(
            app,
            &mut ctx,
            moho_app::Event::Window(WindowEvent::CloseRequested),
        );
        ctx.exit_requested()
    }

    /// Chooses Exit in a menu and runs the frame that handles it; returns whether the
    /// game asked to exit.
    fn exit_from_menu(app: &mut App) -> bool {
        let mut clock = GameClock::default();
        app.event_bus.publish(UiEvent::ExitRequested);
        app.event_bus.process_deferred();
        EventProcessor::new().process_ui_events(app, &mut clock);
        let mut ctx = FrameContext::new(tick_length(), &clock, None, None);
        Game::frame(app, &mut ctx, 0.0);
        ctx.exit_requested()
    }

    /// A fresh app loading the save in `saves`, as the player does from the start menu.
    fn load_save(saves: &Path) -> App {
        let mut app = App::headless();
        app.saves_dir = saves.to_path_buf();
        crate::app::scene_loader::load_scene(
            &mut app,
            &mut GameClock::default(),
            &scene_file(saves),
        )
        .expect("the save loads");
        app
    }

    /// Whether `pos` holds no block in the loaded save, streaming in its column from the
    /// chunk files first when the save left it out.
    fn is_mined_out(app: &mut App, pos: BlockPos) -> bool {
        let column = (pos.x.div_euclid(16), pos.z.div_euclid(16));
        let streamer = app
            .chunk_streamer
            .as_ref()
            .expect("a loaded world streams chunks");
        let grid = app.light_system.as_mut().expect("light system").grid_mut();
        streamer.load_columns_now(grid, column, 0);
        grid.material_at(pos).is_none()
    }

    /// Whether the block under `pos`, which the player never mined, is in the loaded
    /// world: a reload that lost the world cannot pass on absence alone.
    fn block_below_is_present(app: &App, pos: BlockPos) -> bool {
        let below = BlockPos::new(pos.x, pos.y - 1, pos.z);
        app.light_system
            .as_ref()
            .expect("light system")
            .grid()
            .material_at(below)
            .is_some()
    }

    fn set_interval(app: &mut App, interval: Duration) {
        app.autosave_interval = interval;
        app.autosave = AutosaveSchedule::new(interval.as_secs() * u64::from(TICK_HZ));
    }

    /// Runs `ticks` ticks, drawing a frame after each.
    fn play_with_frames(app: &mut App, sim: &mut HeadlessLoop<App>, ticks: u32) {
        for _ in 0..ticks {
            sim.step(app, 1);
            sim.advance(app, Duration::ZERO);
        }
    }

    /// Draws one frame, running no tick.
    fn draw_frame(app: &mut App, sim: &mut HeadlessLoop<App>) {
        sim.advance(app, Duration::ZERO);
    }

    /// A save of a world with one block, as an earlier session leaves it.
    fn earlier_save(saves: &Path) -> Vec<u8> {
        let mut earlier = App::headless();
        earlier.generation.last_spec = Some(world_spec("earlier_session_world"));
        earlier
            .light_system
            .as_mut()
            .expect("App starts with a light system")
            .grid_mut()
            .mutator()
            .place(BlockPos::new(3, 70, 3), 1, None);
        auto_save_on_shutdown(&mut earlier, &GameClock::default(), saves, &[])
            .expect("the earlier session saves");
        std::fs::read(scene_file(saves)).expect("read the earlier save")
    }

    #[test]
    fn autosave_then_load_restores_actors_camera_time_and_blocks() {
        let temp = tempfile::tempdir().expect("temp dir");
        let saves = temp.path().join("not").join("yet").join("created");
        let mut saved = App::headless();
        saved.generation.last_spec = Some(WorldSpec::default());
        let sphere_center = glam::Vec3::new(1.0, 75.0, 2.0);
        saved
            .entities
            .actors
            .spawn_sphere(flagstone_game::actors::Sphere::new(
                sphere_center,
                0.5,
                flagstone_voxel::MaterialType::Lambertian {
                    albedo: glam::Vec3::ONE,
                },
            ));
        saved
            .simulation
            .set_position_yaw_pitch(glam::Vec3::new(3.5, 90.0, 3.5), 1.25, -0.5);
        let mut saved_clock = GameClock::default();
        saved_clock.reset_to(17.5);
        saved
            .light_system
            .as_mut()
            .expect("App starts with a light system")
            .grid_mut()
            .mutator()
            .place(flagstone_voxel::BlockPos::new(3, 70, 3), 1, None);

        auto_save_on_shutdown(&mut saved, &saved_clock, &saves, &[]).expect("autosave");
        let mut loaded = App::headless();
        let mut loaded_clock = GameClock::default();
        crate::app::scene_loader::load_scene(
            &mut loaded,
            &mut loaded_clock,
            &saves.join("scene.bin"),
        )
        .expect("load");

        assert_eq!(loaded.game_state, GameState::Playing);
        let spheres = loaded.entities.actors.spheres();
        assert_eq!(spheres.len(), 1);
        assert_eq!(spheres[0].center, sphere_center);
        assert_eq!(loaded_clock.time_of_day(), 17.5);
        let pos = loaded.simulation.position();
        assert_eq!((pos.x, pos.z), (3.5, 3.5));
        assert_eq!(loaded.simulation.yaw_pitch(), (1.25, -0.5));
        let grid = loaded.light_system.as_ref().expect("light system").grid();
        assert_eq!(grid.get_height(3, 3), Some(70));
        assert!(loaded.chunk_streamer.is_some());
    }

    mod ending_a_session {
        use super::*;

        #[test]
        fn mined_block_is_absent_after_staying_near_then_closing() {
            let temp = tempfile::tempdir().expect("temp dir");
            let mut app = loaded_session("autosave_test_near_close", &temp);
            let mined = mine_surface_block(&mut app, 40, 40);

            let exit = close_window(&mut app);

            assert!(exit);
            let mut reloaded = load_save(&saves_dir(&temp));
            assert!(is_mined_out(&mut reloaded, mined));
            assert!(block_below_is_present(&reloaded, mined));
        }

        #[test]
        fn mined_block_is_absent_after_staying_near_then_menu_exit() {
            let temp = tempfile::tempdir().expect("temp dir");
            let mut app = loaded_session("autosave_test_near_exit", &temp);
            let mined = mine_surface_block(&mut app, 40, 40);
            app.game_state = GameState::Paused;

            let exit = exit_from_menu(&mut app);

            assert!(exit);
            let mut reloaded = load_save(&saves_dir(&temp));
            assert!(is_mined_out(&mut reloaded, mined));
            assert!(block_below_is_present(&reloaded, mined));
        }

        #[test]
        fn mined_block_is_absent_after_walking_back_then_closing() {
            const WORLD: &str = "autosave_test_walk_back";
            let _chunks = WorkingDirChunks(WORLD);
            let temp = tempfile::tempdir().expect("temp dir");
            let mut app = loaded_session(WORLD, &temp);
            let mined = mine_surface_block(&mut app, 40, 40);
            walk_to(&mut app, 168.5, 40.5);
            assert_eq!(
                app.light_system
                    .as_ref()
                    .expect("light system")
                    .grid()
                    .material_at(mined),
                None,
                "the mined chunk left the grid while the player was away"
            );

            walk_to(&mut app, 40.5, 40.5);
            let exit = close_window(&mut app);

            assert!(exit);
            let mut reloaded = load_save(&saves_dir(&temp));
            assert!(is_mined_out(&mut reloaded, mined));
            assert!(block_below_is_present(&reloaded, mined));
        }

        #[test]
        fn mined_block_is_absent_after_staying_away_then_closing() {
            const WORLD: &str = "autosave_test_stay_away";
            let _chunks = WorkingDirChunks(WORLD);
            let temp = tempfile::tempdir().expect("temp dir");
            let mut app = loaded_session(WORLD, &temp);
            let mined = mine_surface_block(&mut app, 40, 40);

            walk_to(&mut app, 168.5, 40.5);
            let exit = close_window(&mut app);

            assert!(exit);
            let mut reloaded = load_save(&saves_dir(&temp));
            assert!(is_mined_out(&mut reloaded, mined));
            assert!(block_below_is_present(&reloaded, mined));
        }
    }

    mod without_a_world {
        use super::*;

        #[test]
        fn closing_with_no_world_keeps_the_previous_save() {
            let temp = tempfile::tempdir().expect("temp dir");
            let saves = saves_dir(&temp);
            let before = earlier_save(&saves);
            let mut app = App::headless();
            app.saves_dir = saves.clone();

            let exit = close_window(&mut app);

            assert!(exit, "the window still closes");
            let after = std::fs::read(scene_file(&saves)).expect("the save is still there");
            assert_eq!(
                after, before,
                "closing from the start menu rewrote the save"
            );
        }

        #[test]
        fn menu_exit_with_no_world_keeps_the_previous_save() {
            let temp = tempfile::tempdir().expect("temp dir");
            let saves = saves_dir(&temp);
            let before = earlier_save(&saves);
            let mut app = App::headless();
            app.saves_dir = saves.clone();

            let exit = exit_from_menu(&mut app);

            assert!(exit, "Exit still quits");
            let after = std::fs::read(scene_file(&saves)).expect("the save is still there");
            assert_eq!(after, before, "Exit from the start menu rewrote the save");
        }

        #[test]
        fn closing_with_no_world_and_no_save_writes_nothing() {
            let temp = tempfile::tempdir().expect("temp dir");
            let saves = saves_dir(&temp);
            let mut app = App::headless();
            app.saves_dir = saves.clone();

            let exit = close_window(&mut app);

            assert!(exit);
            assert!(!scene_file(&saves).exists(), "an empty world was saved");
        }
    }

    mod interval {
        use super::*;

        #[test]
        fn killed_session_keeps_every_edit_before_the_last_autosave() {
            const MINE_AT_TICKS: [u32; 7] = [0, 70, 140, 210, 280, 350, 450];
            const KILLED_AFTER_TICK: u32 = 470;
            // Edits up to tick 350 are older than the last autosave (tick 359 at the
            // latest); the one at tick 450 is within the final interval and may be lost.
            const EDITS_BEFORE_THE_LAST_AUTOSAVE: usize = 6;
            let temp = tempfile::tempdir().expect("temp dir");
            let saves = saves_dir(&temp);
            let mut app = loaded_session("autosave_test_killed", &temp);
            set_interval(&mut app, Duration::from_secs(2));
            let mut sim = HeadlessLoop::<App>::new(LoopConfig::new(TICK_HZ));
            let mut mined = Vec::new();

            for tick in 0..=KILLED_AFTER_TICK {
                if MINE_AT_TICKS.contains(&tick) {
                    mined.push(mine_surface_block(&mut app, 36 + mined.len() as i32, 40));
                }
                play_with_frames(&mut app, &mut sim, 1);
            }
            drop(app);

            assert!(
                scene_file(&saves).is_file(),
                "no autosave was written in {KILLED_AFTER_TICK} ticks"
            );
            let mut reloaded = load_save(&saves);
            for (n, &pos) in mined
                .iter()
                .enumerate()
                .take(EDITS_BEFORE_THE_LAST_AUTOSAVE)
            {
                assert!(
                    is_mined_out(&mut reloaded, pos),
                    "the block mined at tick {} is back after the kill",
                    MINE_AT_TICKS[n]
                );
            }
            assert!(block_below_is_present(&reloaded, mined[0]));
        }

        #[test]
        fn interval_of_30_seconds_saves_the_edit_after_30_seconds() {
            let temp = tempfile::tempdir().expect("temp dir");
            let saves = saves_dir(&temp);
            let mut app = loaded_session("autosave_test_30_seconds", &temp);
            set_interval(&mut app, Duration::from_secs(30));
            let mut sim = HeadlessLoop::<App>::new(LoopConfig::new(TICK_HZ));
            let mined = mine_surface_block(&mut app, 40, 40);

            sim.step(&mut app, 30 * TICK_HZ - 6);
            draw_frame(&mut app, &mut sim);
            let saved_at_29_9_seconds = scene_file(&saves).exists();
            sim.step(&mut app, 6);
            draw_frame(&mut app, &mut sim);

            assert!(!saved_at_29_9_seconds, "saved before the interval passed");
            assert!(
                scene_file(&saves).is_file(),
                "no save after 30 seconds of play"
            );
            let mut reloaded = load_save(&saves);
            assert!(is_mined_out(&mut reloaded, mined));
            assert!(block_below_is_present(&reloaded, mined));
        }

        #[test]
        fn idle_world_is_not_rewritten_after_the_interval() {
            let temp = tempfile::tempdir().expect("temp dir");
            let saves = saves_dir(&temp);
            let mut app = loaded_session("autosave_test_idle", &temp);
            set_interval(&mut app, Duration::from_secs(1));
            let mut sim = HeadlessLoop::<App>::new(LoopConfig::new(TICK_HZ));

            sim.step(&mut app, 3 * TICK_HZ);
            draw_frame(&mut app, &mut sim);

            assert!(!scene_file(&saves).exists(), "an unedited world was saved");
        }

        #[test]
        fn failed_autosave_retries_after_the_next_interval_not_the_next_frame() {
            let temp = tempfile::tempdir().expect("temp dir");
            let working_saves = saves_dir(&temp);
            let mut app = loaded_session("autosave_test_failed", &temp);
            set_interval(&mut app, Duration::from_secs(1));
            let mut sim = HeadlessLoop::<App>::new(LoopConfig::new(TICK_HZ));
            let mined = mine_surface_block(&mut app, 40, 40);
            let blocker = temp.path().join("blocker");
            std::fs::write(&blocker, b"a file where the saves directory should be")
                .expect("write blocker");
            app.saves_dir = blocker.join("saves");

            sim.step(&mut app, TICK_HZ + 10);
            draw_frame(&mut app, &mut sim);
            app.saves_dir = working_saves.clone();
            draw_frame(&mut app, &mut sim);
            let retried_on_the_next_frame = scene_file(&working_saves).exists();
            sim.step(&mut app, TICK_HZ + 10);
            draw_frame(&mut app, &mut sim);

            assert!(
                !retried_on_the_next_frame,
                "a failed save was retried on the very next frame"
            );
            assert!(
                scene_file(&working_saves).is_file(),
                "the edit was never saved once the disk recovered"
            );
            let mut reloaded = load_save(&working_saves);
            assert!(is_mined_out(&mut reloaded, mined));
        }
    }

    mod schedule {
        use super::*;

        #[test]
        fn schedule_is_due_only_after_the_interval_and_an_edit() {
            let mut idle = AutosaveSchedule::new(3);
            for _ in 0..10 {
                idle.tick();
            }
            assert!(!idle.due(), "an unedited world is never due");

            idle.mark_edited();
            assert!(
                idle.due(),
                "the interval had already passed when the edit came"
            );

            let mut edited = AutosaveSchedule::new(3);
            edited.mark_edited();
            edited.tick();
            edited.tick();
            assert!(!edited.due(), "one tick short of the interval");
            edited.tick();
            assert!(edited.due(), "due exactly at the interval");
        }

        #[test]
        fn schedule_resets_on_save() {
            let mut schedule = AutosaveSchedule::new(2);
            schedule.mark_edited();
            schedule.tick();
            schedule.tick();
            assert!(schedule.due(), "due before the save");

            schedule.saved();

            assert!(!schedule.due(), "a save clears the edit");
            schedule.tick();
            schedule.tick();
            assert!(!schedule.due(), "the saved edit stays cleared");

            let mut restarted = AutosaveSchedule::new(2);
            restarted.mark_edited();
            restarted.tick();
            restarted.tick();
            restarted.saved();
            restarted.mark_edited();
            assert!(!restarted.due(), "the interval restarts at the save");
            restarted.tick();
            assert!(!restarted.due(), "half an interval after the save");
            restarted.tick();
            assert!(
                restarted.due(),
                "a new edit is due one interval after the save"
            );
        }

        #[test]
        fn schedule_failed_save_retries_next_interval() {
            let mut schedule = AutosaveSchedule::new(2);
            schedule.mark_edited();
            schedule.tick();
            schedule.tick();
            assert!(schedule.due(), "due before the failed save");

            schedule.save_failed();

            assert!(!schedule.due(), "no retry straight after a failure");
            schedule.tick();
            assert!(!schedule.due(), "no retry half an interval later");
            schedule.tick();
            assert!(
                schedule.due(),
                "the unsaved edit is retried one interval later"
            );
        }
    }
}
