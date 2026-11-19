/// How long a world goes between autosaves unless told otherwise.
pub const DEFAULT_INTERVAL: std::time::Duration = std::time::Duration::from_secs(120);

/// When the running world is next due an autosave, counted in play ticks.
#[derive(Debug, Clone)]
pub struct AutosaveSchedule {
    interval_ticks: u32,
    ticks_since_save: u32,
    edited: bool,
}

impl AutosaveSchedule {
    pub fn new(interval_ticks: u32) -> Self {
        Self {
            interval_ticks,
            ticks_since_save: 0,
            edited: false,
        }
    }

    pub fn tick(&mut self) {
        self.ticks_since_save = self.ticks_since_save.saturating_add(1);
    }

    /// Changes the interval, so a setting changed mid-session applies at once.
    pub fn set_interval_ticks(&mut self, interval_ticks: u32) {
        self.interval_ticks = interval_ticks;
    }

    pub fn mark_edited(&mut self) {
        self.edited = true;
    }

    /// Whether an edit has waited a full interval since the last save.
    pub fn due(&self) -> bool {
        self.edited && self.ticks_since_save >= self.interval_ticks
    }

    /// Records a successful save: nothing is unsaved and the interval restarts.
    pub fn saved(&mut self) {
        self.edited = false;
        self.ticks_since_save = 0;
    }

    /// Records a failed save: the edit stays unsaved and the interval restarts.
    pub fn save_failed(&mut self) {
        self.ticks_since_save = 0;
    }

    #[cfg(test)]
    pub fn edited(&self) -> bool {
        self.edited
    }

    #[cfg(test)]
    pub fn ticks_since_save(&self) -> u32 {
        self.ticks_since_save
    }
}

/// Save the current scene to `saves_dir/scene.bin`, creating the directory
/// if needed. Does nothing when no world is loaded, so a session that never
/// reached a world cannot replace the save on disk.
pub fn auto_save_on_shutdown(
    app: &mut crate::App,
    clock: &moho_app::GameClock,
    saves_dir: &std::path::Path,
    lights: &[moho_render_api::LightDesc],
) -> Result<(), Box<dyn std::error::Error>> {
    let Some(mut spec) = app.generation.last_spec.clone() else {
        tracing::info!("No world loaded; autosave skipped");
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
    use crate::game_state::GameState;
    use moho_app::GameClock;

    #[test]
    fn autosave_then_load_restores_actors_camera_time_and_blocks() {
        let temp = tempfile::tempdir().expect("temp dir");
        let saves = temp.path().join("not").join("yet").join("created");
        let mut saved = App::headless();
        saved.generation.last_spec = Some(flagstone_game::scene_builders::WorldSpec::default());
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

    mod schedule {
        use super::*;

        #[test]
        fn schedule_is_due_only_after_the_interval_and_an_edit() {
            let mut schedule = AutosaveSchedule::new(3);
            for _ in 0..3 {
                schedule.tick();
            }
            let without_edit = schedule.due();
            schedule.mark_edited();
            let with_edit = schedule.due();

            let mut early = AutosaveSchedule::new(3);
            early.mark_edited();
            early.tick();
            early.tick();
            let one_tick_short = early.due();
            early.tick();

            assert!(!without_edit, "a quiet world is never saved");
            assert!(with_edit, "an edit older than the interval is due");
            assert!(!one_tick_short, "the interval has not passed");
            assert!(early.due(), "the interval has passed");
        }

        #[test]
        fn schedule_resets_on_save() {
            let mut schedule = AutosaveSchedule::new(2);
            schedule.mark_edited();
            schedule.tick();
            schedule.tick();
            assert!(schedule.due());

            schedule.saved();
            for _ in 0..5 {
                schedule.tick();
            }

            assert!(!schedule.edited(), "the save took the edit");
            assert!(!schedule.due(), "nothing unsaved, nothing due");
            let mut restarted = AutosaveSchedule::new(2);
            restarted.mark_edited();
            restarted.tick();
            restarted.tick();
            restarted.saved();
            assert_eq!(restarted.ticks_since_save(), 0);
        }

        #[test]
        fn schedule_failed_save_retries_next_interval() {
            let mut schedule = AutosaveSchedule::new(2);
            schedule.mark_edited();
            schedule.tick();
            schedule.tick();
            assert!(schedule.due());

            schedule.save_failed();
            let right_after = schedule.due();
            schedule.tick();
            let one_tick_in = schedule.due();
            schedule.tick();

            assert!(schedule.edited(), "the edit is still unsaved");
            assert!(!right_after, "no retry in the same instant");
            assert!(!one_tick_in, "the retry waits a full interval");
            assert!(schedule.due(), "the retry is due one interval later");
        }
    }

    mod sessions {
        use super::*;
        use flagstone_game::TICK_HZ;
        use flagstone_game::scene_builders::WorldSpec;
        use flagstone_voxel::{BlockChangeReason, BlockPos, WorldEvent};
        use moho_app::{EventContext, FrameContext, Game, HeadlessLoop, LoopConfig};
        use std::path::Path;
        use std::time::Duration;
        use winit::event::WindowEvent;

        /// Wide enough that every column near the spawn point has terrain.
        const WORLD_SIZE: u32 = 256;
        /// Surface blocks mined in the tests lie in this column of the chunk grid.
        const MINED_CHUNK_XZ: i32 = 2;
        /// Beyond the default unload radius of 12 chunks from the spawn column.
        const FAR_X: f32 = 40.5 + 16.0 * 14.0;
        const NEAR_X: f32 = 40.5;

        fn world_name(saves: &Path) -> String {
            let dir = saves.file_name().expect("temp dir has a name");
            format!("autosave_test_{}", dir.to_string_lossy())
        }

        /// A save of an untouched world, as a first session would leave it.
        fn seed_save(saves: &Path) {
            let mut seed = App::headless();
            seed.generation.last_spec = Some(WorldSpec {
                name: world_name(saves),
                size_xz: WORLD_SIZE,
                ..WorldSpec::default()
            });
            seed.simulation.set_position_yaw_pitch(
                glam::Vec3::new(NEAR_X, 200.0, NEAR_X),
                0.0,
                0.0,
            );
            auto_save_on_shutdown(&mut seed, &GameClock::default(), saves, &[]).expect("seed save");
        }

        /// A playing app that loaded the world saved in `saves`.
        fn load_world(saves: &Path) -> App {
            let mut app = App::headless();
            app.saves_dir = saves.to_path_buf();
            crate::app::scene_loader::load_scene(
                &mut app,
                &mut GameClock::default(),
                &saves.join("scene.bin"),
            )
            .expect("load the saved world");
            assert_eq!(app.game_state, GameState::Playing);
            app
        }

        fn loaded_world() -> (App, tempfile::TempDir) {
            let temp = tempfile::tempdir().expect("temp dir");
            seed_save(temp.path());
            let mut app = load_world(temp.path());
            // A free-flying camera stays where the test puts it.
            app.simulation
                .set_camera_mode(flagstone_game::controller::CameraMode::FirstPerson);
            app.physics.noclip = true;
            (app, temp)
        }

        fn grid(app: &App) -> &flagstone_voxel::VoxelGrid {
            app.light_system.as_ref().expect("light system").grid()
        }

        /// The surface blocks of the first `count` columns beside the spawn point.
        fn surface_blocks(app: &App, count: i32) -> Vec<BlockPos> {
            (0..count)
                .map(|dx| {
                    let (x, z) = (40 + dx, 40);
                    let y = grid(app)
                        .get_height(x, z)
                        .unwrap_or_else(|| panic!("terrain at ({x}, {z})"));
                    BlockPos::new(x, y, z)
                })
                .collect()
        }

        /// Mines `pos` as the player does: the grid changes and the removal is announced.
        fn mine(app: &mut App, pos: BlockPos) {
            let grid = app.light_system.as_mut().expect("light system").grid_mut();
            assert!(grid.mutator().remove(pos), "{pos} was there to mine");
            app.event_bus.publish(WorldEvent::BlockRemoved {
                position: pos,
                old_material_id: 0,
                reason: BlockChangeReason::Player,
            });
        }

        fn tick_length() -> Duration {
            LoopConfig::new(TICK_HZ).tick_length()
        }

        fn new_loop() -> HeadlessLoop<App> {
            HeadlessLoop::<App>::new(LoopConfig::new(TICK_HZ))
        }

        /// Plays `ticks` ticks, each followed by its frame.
        fn play(app: &mut App, sim: &mut HeadlessLoop<App>, ticks: u32) {
            let mut played = 0;
            while played < ticks {
                played += sim.advance(app, tick_length());
            }
        }

        fn move_player(app: &mut App, x: f32) {
            app.simulation
                .set_position_yaw_pitch(glam::Vec3::new(x, 100.0, NEAR_X), 0.0, 0.0);
        }

        fn is_loaded(app: &App, pos: BlockPos) -> bool {
            grid(app).has_chunk(pos >> 4)
        }

        /// Walks out of range of `pos`, until its chunk has been evicted.
        fn walk_out_of_range(app: &mut App, sim: &mut HeadlessLoop<App>, pos: BlockPos) {
            move_player(app, FAR_X);
            play(app, sim, 5);
            assert!(!is_loaded(app, pos), "{pos} is out of range and evicted");
        }

        /// Walks back, until the chunk of `pos` is streamed in again.
        fn walk_back(app: &mut App, sim: &mut HeadlessLoop<App>, pos: BlockPos) {
            move_player(app, NEAR_X);
            play(app, sim, 5);
            assert!(is_loaded(app, pos), "{pos} is back in range and loaded");
        }

        fn close_window(app: &mut App) {
            let clock = GameClock::default();
            let mut ctx = EventContext::new(&clock, None, None);
            Game::event(
                app,
                &mut ctx,
                moho_app::Event::Window(WindowEvent::CloseRequested),
            );
            assert!(ctx.exit_requested(), "closing the window ends the session");
        }

        fn menu_exit(app: &mut App) {
            app.exit_requested = true;
            let clock = GameClock::default();
            let mut ctx = FrameContext::new(tick_length(), &clock, None, None);
            Game::frame(app, &mut ctx, 0.5);
            assert!(ctx.exit_requested(), "Exit in the menu ends the session");
        }

        /// Which of `blocks` are solid after loading the save in `saves` and streaming
        /// the columns they stand in, as a player returning to them would.
        fn present_after_load(saves: &Path, blocks: &[BlockPos]) -> Vec<bool> {
            let mut loaded = load_world(saves);
            let streamer = loaded.chunk_streamer.as_ref().expect("chunk streamer");
            let grid = loaded
                .light_system
                .as_mut()
                .expect("light system")
                .grid_mut();
            streamer.load_columns_now(grid, (MINED_CHUNK_XZ, MINED_CHUNK_XZ), 0);
            blocks.iter().map(|&pos| grid.is_solid_at(pos)).collect()
        }

        #[test]
        fn mined_block_is_absent_after_staying_near_then_closing() {
            let (mut app, temp) = loaded_world();
            let mined = surface_blocks(&app, 1)[0];
            let mut sim = new_loop();
            mine(&mut app, mined);
            play(&mut app, &mut sim, 1);

            close_window(&mut app);

            assert_eq!(present_after_load(temp.path(), &[mined]), vec![false]);
        }

        #[test]
        fn mined_block_is_absent_after_staying_near_then_menu_exit() {
            let (mut app, temp) = loaded_world();
            let mined = surface_blocks(&app, 1)[0];
            let mut sim = new_loop();
            mine(&mut app, mined);
            play(&mut app, &mut sim, 1);

            menu_exit(&mut app);

            assert_eq!(present_after_load(temp.path(), &[mined]), vec![false]);
        }

        #[test]
        fn mined_block_is_absent_after_walking_back_then_closing() {
            let (mut app, temp) = loaded_world();
            let mined = surface_blocks(&app, 1)[0];
            let mut sim = new_loop();
            mine(&mut app, mined);
            play(&mut app, &mut sim, 1);
            walk_out_of_range(&mut app, &mut sim, mined);
            walk_back(&mut app, &mut sim, mined);

            close_window(&mut app);

            assert_eq!(present_after_load(temp.path(), &[mined]), vec![false]);
        }

        #[test]
        fn mined_block_is_absent_after_staying_away_then_closing() {
            let (mut app, temp) = loaded_world();
            let mined = surface_blocks(&app, 1)[0];
            let mut sim = new_loop();
            mine(&mut app, mined);
            play(&mut app, &mut sim, 1);
            walk_out_of_range(&mut app, &mut sim, mined);

            close_window(&mut app);

            assert_eq!(present_after_load(temp.path(), &[mined]), vec![false]);
        }

        fn scene_bytes(saves: &Path) -> Vec<u8> {
            std::fs::read(saves.join("scene.bin")).expect("the previous save exists")
        }

        #[test]
        fn closing_with_no_world_keeps_the_previous_save() {
            let temp = tempfile::tempdir().expect("temp dir");
            seed_save(temp.path());
            let before = scene_bytes(temp.path());
            let mut app = App::headless();
            app.saves_dir = temp.path().to_path_buf();

            close_window(&mut app);

            assert!(
                scene_bytes(temp.path()) == before,
                "no world was loaded, so the save on disk is not replaced"
            );
        }

        #[test]
        fn menu_exit_with_no_world_keeps_the_previous_save() {
            let temp = tempfile::tempdir().expect("temp dir");
            seed_save(temp.path());
            let before = scene_bytes(temp.path());
            let mut app = App::headless();
            app.saves_dir = temp.path().to_path_buf();

            menu_exit(&mut app);

            assert!(
                scene_bytes(temp.path()) == before,
                "no world was loaded, so the save on disk is not replaced"
            );
        }

        #[test]
        fn killed_session_keeps_every_edit_before_the_last_autosave() {
            let (mut app, temp) = loaded_world();
            app.autosave_interval = Duration::from_secs(2);
            let blocks = surface_blocks(&app, 4);
            let interval_ticks = 2 * TICK_HZ;
            let mut sim = new_loop();
            for &block in &blocks[..3] {
                mine(&mut app, block);
                play(&mut app, &mut sim, interval_ticks + TICK_HZ / 2);
            }
            mine(&mut app, blocks[3]);
            play(&mut app, &mut sim, interval_ticks / 2);

            drop(app);

            assert_eq!(
                present_after_load(temp.path(), &blocks[..3]),
                vec![false; 3],
                "blocks mined before the last autosave are gone"
            );
        }

        #[test]
        fn idle_world_is_not_rewritten_after_loading() {
            let (mut app, temp) = loaded_world();
            app.autosave_interval = Duration::from_secs(2);
            let sentinel = b"sentinel: an idle world must not overwrite this".to_vec();
            std::fs::write(temp.path().join("scene.bin"), &sentinel).expect("plant sentinel");
            let mut sim = new_loop();

            play(&mut app, &mut sim, 6 * TICK_HZ);

            assert!(
                !app.autosave_schedule.edited(),
                "loading a world is not an edit"
            );
            assert!(
                scene_bytes(temp.path()) == sentinel,
                "an untouched world is not rewritten"
            );
        }

        #[test]
        fn failed_autosave_retries_once_per_interval_then_saves_when_repaired() {
            let (mut app, temp) = loaded_world();
            app.autosave_interval = Duration::from_secs(2);
            let interval_ticks = 2 * TICK_HZ;
            let blocker = temp.path().join("blocker");
            std::fs::write(&blocker, b"a file, not a directory").expect("blocker file");
            app.saves_dir = blocker.join("saves");
            let mined = surface_blocks(&app, 1)[0];
            let mut sim = new_loop();
            mine(&mut app, mined);

            let mut attempts = 0;
            let mut previous = 0;
            for _ in 0..3 * interval_ticks {
                play(&mut app, &mut sim, 1);
                let now = app.autosave_schedule.ticks_since_save();
                if now < previous {
                    attempts += 1;
                }
                previous = now;
                assert!(
                    now <= interval_ticks,
                    "ticks since save {now} exceeds the interval: no retry happened"
                );
            }

            assert!(app.autosave_schedule.edited(), "the edit is still unsaved");
            assert!(
                (1..=3).contains(&attempts),
                "at most one attempt per interval, got {attempts}"
            );
            assert!(
                app.autosave_schedule.ticks_since_save() < interval_ticks,
                "the last failed attempt restarted the interval"
            );

            app.saves_dir = temp.path().to_path_buf();
            play(&mut app, &mut sim, interval_ticks + 1);

            assert!(!app.autosave_schedule.edited(), "the retry saved the edit");
            assert_eq!(present_after_load(temp.path(), &[mined]), vec![false]);
        }

        #[test]
        fn interval_of_30_seconds_saves_the_edit_after_30_seconds() {
            let (mut app, temp) = loaded_world();
            app.autosave_interval = Duration::from_secs(30);
            let mined = surface_blocks(&app, 1)[0];
            let mut sim = new_loop();
            mine(&mut app, mined);
            let just_short = (29.9 * f64::from(TICK_HZ)) as u32;

            play(&mut app, &mut sim, just_short);
            let at_29_9_seconds = present_after_load(temp.path(), &[mined]);
            play(&mut app, &mut sim, 30 * TICK_HZ - just_short);
            let at_30_seconds = present_after_load(temp.path(), &[mined]);

            assert_eq!(at_29_9_seconds, vec![true], "no save before the interval");
            assert_eq!(at_30_seconds, vec![false], "the edit is saved at 30 s");
        }
    }
}
