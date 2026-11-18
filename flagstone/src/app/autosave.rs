use std::time::{Duration, Instant};

/// When the running world is next worth saving, counted in simulation ticks.
///
/// Time is ticks, not wall clock (ADR-0009), so a paused game never comes due.
#[derive(Clone, Debug)]
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

    pub fn set_interval_ticks(&mut self, interval_ticks: u32) {
        self.interval_ticks = interval_ticks;
    }

    pub fn tick(&mut self) {
        self.ticks_since_save = self.ticks_since_save.saturating_add(1);
    }

    /// The world changed since the last save.
    pub fn mark_edited(&mut self) {
        self.edited = true;
    }

    /// An edit is waiting and a whole interval has passed since the last save or attempt.
    pub fn due(&self) -> bool {
        self.edited && self.ticks_since_save >= self.interval_ticks
    }

    /// A save succeeded: nothing is waiting and the interval starts over.
    pub fn saved(&mut self) {
        self.ticks_since_save = 0;
        self.edited = false;
    }

    /// A save failed: the edit stays waiting, but the retry is a whole interval away.
    pub fn failed(&mut self) {
        self.ticks_since_save = 0;
    }
}

/// `interval` in simulation ticks, rounded down and saturating.
pub fn interval_ticks(interval: Duration) -> u32 {
    let ticks = interval.as_millis() * u128::from(flagstone_game::TICK_HZ) / 1000;
    u32::try_from(ticks).unwrap_or(u32::MAX)
}

/// Save the current scene to `saves_dir/scene.bin`, creating the directory
/// if needed. Does nothing while no world is loaded: the grid then is the
/// empty one a fresh `App` holds, and saving it would overwrite a real save.
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
    let started = Instant::now();
    tracing::info!("Auto-saving...");

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

        fn ticked(schedule: &mut AutosaveSchedule, n: u32) {
            for _ in 0..n {
                schedule.tick();
            }
        }

        #[test]
        fn schedule_is_due_only_after_the_interval_and_an_edit() {
            let mut idle = AutosaveSchedule::new(3);
            ticked(&mut idle, 10);
            let mut edited = AutosaveSchedule::new(3);
            edited.mark_edited();

            ticked(&mut edited, 2);
            let two_ticks = edited.due();
            ticked(&mut edited, 1);

            assert!(!idle.due(), "an unedited world is never rewritten");
            assert!(!two_ticks, "an edit is not saved before the interval");
            assert!(edited.due(), "the edit is due once the interval passed");
        }

        #[test]
        fn schedule_resets_on_save() {
            let mut schedule = AutosaveSchedule::new(3);
            schedule.mark_edited();
            ticked(&mut schedule, 3);
            assert!(schedule.due());

            schedule.saved();
            ticked(&mut schedule, 3);
            let without_a_new_edit = schedule.due();
            schedule.saved();
            schedule.mark_edited();
            ticked(&mut schedule, 2);
            let two_ticks_after = schedule.due();
            ticked(&mut schedule, 1);

            assert!(!without_a_new_edit, "a saved edit is not saved again");
            assert!(!two_ticks_after, "the interval counts from the save");
            assert!(schedule.due(), "a new edit is due one interval later");
        }

        #[test]
        fn schedule_failed_save_retries_next_interval() {
            let mut schedule = AutosaveSchedule::new(3);
            schedule.mark_edited();
            ticked(&mut schedule, 3);
            assert!(schedule.due());

            schedule.failed();
            let right_after = schedule.due();
            ticked(&mut schedule, 2);
            let two_ticks_after = schedule.due();
            ticked(&mut schedule, 1);

            assert!(!right_after, "a failed save is not retried every frame");
            assert!(!two_ticks_after, "the retry waits a whole interval");
            assert!(schedule.due(), "the unsaved edit is retried");
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

        const WORLD: &str = "autosave_unit_test_world";

        /// One tick's worth of frame time, rounded up so a frame never lands one tick short.
        fn one_tick() -> Duration {
            Duration::from_nanos(1_000_000_000u64.div_ceil(u64::from(TICK_HZ)))
        }

        fn spec() -> WorldSpec {
            WorldSpec {
                name: WORLD.to_string(),
                size_xz: 256,
                ..WorldSpec::default()
            }
        }

        fn untouched_save(saves: &Path) {
            let mut writer = App::headless();
            writer.generation.last_spec = Some(spec());
            writer
                .simulation
                .set_position_yaw_pitch(glam::Vec3::new(8.5, 200.0, 8.5), 0.0, 0.0);
            auto_save_on_shutdown(&mut writer, &GameClock::default(), saves, &[])
                .expect("seed save");
        }

        /// The world saved under `saves`, loaded the way the Load menu does.
        fn load(saves: &Path) -> App {
            let mut app = App::headless();
            app.saves_dir = saves.to_path_buf();
            crate::app::scene_loader::load_scene(
                &mut app,
                &mut GameClock::default(),
                &saves.join("scene.bin"),
            )
            .expect("load");
            app
        }

        fn new_loop() -> HeadlessLoop<App> {
            HeadlessLoop::new(LoopConfig::new(TICK_HZ))
        }

        fn session(saves: &Path) -> (App, HeadlessLoop<App>) {
            untouched_save(saves);
            let app = load(saves);
            assert_eq!(app.game_state, GameState::Playing);
            (app, new_loop())
        }

        fn grid(app: &App) -> &flagstone_voxel::VoxelGrid {
            app.light_system.as_ref().expect("light system").grid()
        }

        fn surface(app: &App, x: i32, z: i32) -> BlockPos {
            let y = grid(app).get_height(x, z).expect("terrain in the column");
            BlockPos::new(x, y, z)
        }

        /// Mines `pos` the way the player's mining does: the grid loses the block and
        /// the world says so.
        fn mine(app: &mut App, pos: BlockPos) {
            let grid = app.light_system.as_mut().expect("light system").grid_mut();
            let old_material_id = grid.material_at(pos).expect("there is a block to mine");
            assert!(grid.mutator().remove(pos), "the block was removed");
            app.event_bus.publish(WorldEvent::BlockRemoved {
                position: pos,
                old_material_id,
                reason: BlockChangeReason::Player,
            });
        }

        fn play(app: &mut App, sim: &mut HeadlessLoop<App>, ticks: u32) {
            for _ in 0..ticks {
                sim.advance(app, one_tick());
            }
        }

        fn close_window(app: &mut App, sim: &HeadlessLoop<App>) -> bool {
            let mut ctx = EventContext::new(sim.clock(), None, None);
            Game::event(
                app,
                &mut ctx,
                moho_app::Event::Window(WindowEvent::CloseRequested),
            );
            ctx.exit_requested()
        }

        fn choose_exit_in_the_menu(app: &mut App, sim: &mut HeadlessLoop<App>) -> bool {
            app.event_bus
                .publish(moho_core::events::UiEvent::ExitRequested);
            app.event_bus.process_deferred();
            sim.step(app, 1);
            let mut ctx = FrameContext::new(
                LoopConfig::new(TICK_HZ).tick_length(),
                sim.clock(),
                None,
                None,
            );
            Game::frame(app, &mut ctx, 0.0);
            ctx.exit_requested()
        }

        fn assert_mined(app: &App, pos: BlockPos) {
            let chunk = glam::IVec3::new(
                pos.x.div_euclid(16),
                pos.y.div_euclid(16),
                pos.z.div_euclid(16),
            );
            assert!(grid(app).has_chunk(chunk), "the block's chunk is loaded");
            assert!(!grid(app).is_solid_at(pos), "the mined block {pos} is back");
        }

        fn walk_to(app: &mut App, sim: &mut HeadlessLoop<App>, column_x: i32) {
            // High enough that the character is still falling, not past the kill plane (which
            // respawns it at column 0), when the destination's chunks finish loading under it.
            let target = glam::Vec3::new(column_x as f32 * 16.0 + 8.5, 250.0, 8.5);
            app.simulation.set_position_yaw_pitch(target, 0.0, 0.0);
            // First person: each tick copies the physics character back into the simulation,
            // so the character has to be relocated too, as a respawn does.
            app.physics.teleport_character(target);
            // Eviction is immediate; loading is budgeted, so give the walk-back time.
            play(app, sim, 200);
        }

        /// Far enough from the origin that every chunk there is outside the unload radius.
        fn far_column(app: &App) -> i32 {
            app.prefs.world_unload_radius() as i32 + 8
        }

        #[test]
        fn mined_block_is_absent_after_staying_near_then_closing() {
            let temp = tempfile::tempdir().expect("temp dir");
            let saves = temp.path().join("saves");
            let (mut app, mut sim) = session(&saves);
            let block = surface(&app, 8, 8);
            mine(&mut app, block);
            play(&mut app, &mut sim, 5);

            let exit = close_window(&mut app, &sim);

            assert!(exit);
            assert_mined(&load(&saves), block);
        }

        #[test]
        fn mined_block_is_absent_after_staying_near_then_menu_exit() {
            let temp = tempfile::tempdir().expect("temp dir");
            let saves = temp.path().join("saves");
            let (mut app, mut sim) = session(&saves);
            let block = surface(&app, 8, 8);
            mine(&mut app, block);
            play(&mut app, &mut sim, 5);
            app.escape_pressed();
            assert_eq!(app.game_state, GameState::Paused);

            let exit = choose_exit_in_the_menu(&mut app, &mut sim);

            assert!(exit);
            assert_mined(&load(&saves), block);
        }

        #[test]
        fn mined_block_is_absent_after_walking_back_then_closing() {
            let temp = tempfile::tempdir().expect("temp dir");
            let saves = temp.path().join("saves");
            let (mut app, mut sim) = session(&saves);
            let block = surface(&app, 8, 8);
            mine(&mut app, block);
            play(&mut app, &mut sim, 5);
            let far = far_column(&app);
            walk_to(&mut app, &mut sim, far);
            assert!(
                !grid(&app).has_chunk(glam::IVec3::new(0, block.y.div_euclid(16), 0)),
                "the mined block's chunk left the grid"
            );
            walk_to(&mut app, &mut sim, 0);

            close_window(&mut app, &sim);

            assert_mined(&load(&saves), block);
        }

        #[test]
        fn mined_block_is_absent_after_staying_away_then_closing() {
            let temp = tempfile::tempdir().expect("temp dir");
            let saves = temp.path().join("saves");
            let (mut app, mut sim) = session(&saves);
            let block = surface(&app, 8, 8);
            mine(&mut app, block);
            play(&mut app, &mut sim, 5);
            let far = far_column(&app);
            walk_to(&mut app, &mut sim, far);
            assert!(
                !grid(&app).has_chunk(glam::IVec3::new(0, block.y.div_euclid(16), 0)),
                "the mined block's chunk left the grid"
            );

            close_window(&mut app, &sim);

            let mut loaded = load(&saves);
            let mut loaded_sim = new_loop();
            walk_to(&mut loaded, &mut loaded_sim, 0);
            assert_mined(&loaded, block);
        }

        fn save_with_a_block(saves: &Path) -> BlockPos {
            let block = BlockPos::new(3, 70, 3);
            let mut writer = App::headless();
            writer.generation.last_spec = Some(spec());
            writer
                .light_system
                .as_mut()
                .expect("light system")
                .grid_mut()
                .mutator()
                .place(block, 1, None);
            auto_save_on_shutdown(&mut writer, &GameClock::default(), saves, &[])
                .expect("seed save");
            block
        }

        fn assert_still_saved(saves: &Path, block: BlockPos) {
            let loaded = load(saves);
            assert!(
                grid(&loaded).is_solid_at(block),
                "the earlier save lost its block {block}"
            );
        }

        #[test]
        fn closing_with_no_world_keeps_the_previous_save() {
            let temp = tempfile::tempdir().expect("temp dir");
            let saves = temp.path().join("saves");
            let block = save_with_a_block(&saves);
            let mut app = App::headless();
            app.saves_dir = saves.clone();
            assert_eq!(app.game_state, GameState::Menu, "the start menu");

            let exit = close_window(&mut app, &new_loop());

            assert!(exit);
            assert_still_saved(&saves, block);
        }

        #[test]
        fn menu_exit_with_no_world_keeps_the_previous_save() {
            let temp = tempfile::tempdir().expect("temp dir");
            let saves = temp.path().join("saves");
            let block = save_with_a_block(&saves);
            let mut app = App::headless();
            app.saves_dir = saves.clone();
            assert_eq!(app.game_state, GameState::Menu, "the start menu");

            let exit = choose_exit_in_the_menu(&mut app, &mut new_loop());

            assert!(exit);
            assert_still_saved(&saves, block);
        }

        #[test]
        fn killed_session_keeps_every_edit_before_the_last_autosave() {
            let interval = Duration::from_secs(2);
            let interval_ticks = 2 * TICK_HZ;
            let temp = tempfile::tempdir().expect("temp dir");
            let saves = temp.path().join("saves");
            let (mut app, mut sim) = session(&saves);
            app.autosave_interval = interval;
            let first = surface(&app, 8, 8);
            let second = surface(&app, 9, 8);
            let third = surface(&app, 10, 8);
            mine(&mut app, first);
            play(&mut app, &mut sim, interval_ticks + 10);
            mine(&mut app, second);
            play(&mut app, &mut sim, interval_ticks + 10);
            mine(&mut app, third);
            play(&mut app, &mut sim, interval_ticks / 2);

            drop(app);

            let loaded = load(&saves);
            assert_mined(&loaded, first);
            assert_mined(&loaded, second);
        }

        #[test]
        fn interval_of_30_seconds_saves_the_edit_after_30_seconds() {
            let temp = tempfile::tempdir().expect("temp dir");
            let saves = temp.path().join("saves");
            let (mut app, mut sim) = session(&saves);
            app.autosave_interval = Duration::from_secs(30);
            let block = surface(&app, 8, 8);
            mine(&mut app, block);

            play(&mut app, &mut sim, 30 * TICK_HZ - TICK_HZ / 10);
            let before = load(&saves);
            play(&mut app, &mut sim, TICK_HZ / 10);
            let after = load(&saves);

            assert!(
                grid(&before).is_solid_at(block),
                "no save held the edit at 29.9 s"
            );
            assert_mined(&after, block);
        }
    }
}
