use std::time::Duration;

/// How much play passes between autosaves of an edited world.
pub const DEFAULT_INTERVAL: Duration = Duration::from_secs(120);

/// When an edited world is next due to be autosaved, counted in simulation ticks.
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

    pub fn tick(&mut self) {
        self.ticks_since_save = self.ticks_since_save.saturating_add(1);
    }

    /// Changes the interval after construction.
    pub fn set_interval_ticks(&mut self, interval_ticks: u32) {
        self.interval_ticks = interval_ticks;
    }

    pub fn mark_edited(&mut self) {
        self.edited = true;
    }

    /// Whether the world was edited and a full interval has passed since the last save.
    pub fn due(&self) -> bool {
        self.edited && self.ticks_since_save >= self.interval_ticks
    }

    /// A different world was entered: forgets the previous world's edits and elapsed play,
    /// keeping the interval.
    pub fn reset_for_new_world(&mut self) {
        *self = Self::new(self.interval_ticks);
    }

    /// A save succeeded: the world is clean and the interval restarts.
    pub fn saved(&mut self) {
        self.ticks_since_save = 0;
        self.edited = false;
    }

    /// A save failed: the interval restarts but the edit stays, so the next interval retries.
    pub fn save_failed(&mut self) {
        self.ticks_since_save = 0;
    }
}

/// `interval` in simulation ticks, saturating.
pub fn interval_to_ticks(interval: Duration) -> u32 {
    let ticks = interval.as_millis() * u128::from(flagstone_game::TICK_HZ) / 1000;
    u32::try_from(ticks).unwrap_or(u32::MAX)
}

/// Save the current scene to `saves_dir/scene.bin`, creating the directory
/// if needed. Does nothing when no world is loaded, so an empty grid never
/// overwrites a previous save.
pub fn auto_save_on_shutdown(
    app: &mut crate::App,
    clock: &moho_app::GameClock,
    saves_dir: &std::path::Path,
    lights: &[moho_render_api::LightDesc],
) -> Result<(), Box<dyn std::error::Error>> {
    let Some(mut spec) = app.generation.last_spec.clone() else {
        tracing::debug!("No world loaded; skipping auto-save");
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
        duration_ms = started.elapsed().as_millis(),
        "Auto-saved scene (envelope)"
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::App;
    use crate::game_state::GameState;
    use flagstone_game::TICK_HZ;
    use flagstone_game::controller::CameraMode;
    use flagstone_game::scene_builders::{TerrainConfig, WorldSpec, generate_chunk};
    use flagstone_voxel::{BlockChangeReason, WorldEvent};
    use glam::{IVec3, Vec3};
    use moho_app::{EventContext, FrameContext, Game, GameClock, HeadlessLoop, LoopConfig};
    use std::path::Path;
    use winit::event::WindowEvent;

    mod schedule {
        use super::*;

        #[test]
        fn schedule_is_due_only_after_the_interval_and_an_edit() {
            let mut idle = AutosaveSchedule::new(3);
            for _ in 0..10 {
                idle.tick();
            }
            let mut early = AutosaveSchedule::new(3);
            early.mark_edited();
            early.tick();
            early.tick();
            let mut late_edit = AutosaveSchedule::new(3);
            for _ in 0..5 {
                late_edit.tick();
            }
            late_edit.mark_edited();
            let mut on_time = AutosaveSchedule::new(3);
            on_time.mark_edited();
            for _ in 0..3 {
                on_time.tick();
            }

            assert!(!idle.due(), "an idle world is not saved");
            assert!(!early.due(), "two of three ticks is not an interval");
            assert!(late_edit.due(), "an edit after the interval is due at once");
            assert!(on_time.due(), "an edit and a full interval is due");
        }

        #[test]
        fn schedule_resets_on_save() {
            let mut schedule = AutosaveSchedule::new(2);
            schedule.mark_edited();
            schedule.tick();
            schedule.tick();
            assert!(schedule.due(), "due before the save");

            schedule.saved();
            let clean = schedule.due();
            schedule.mark_edited();
            let restarted = schedule.due();
            schedule.tick();
            let one_tick = schedule.due();
            schedule.tick();

            assert!(!clean, "a saved world is not due");
            assert!(!restarted, "the interval restarts at the save");
            assert!(!one_tick, "one tick of two is not an interval");
            assert!(
                schedule.due(),
                "an edit after the save is due after an interval"
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
            let right_after = schedule.due();
            schedule.tick();
            let one_tick = schedule.due();
            schedule.tick();

            assert!(!right_after, "a failed save is not retried every frame");
            assert!(!one_tick, "one tick of two is not an interval");
            assert!(
                schedule.due(),
                "the edit is still unsaved a full interval later"
            );
        }
    }

    fn frame_dt() -> std::time::Duration {
        std::time::Duration::from_nanos(1_000_000_000u64.div_ceil(u64::from(TICK_HZ)))
    }

    fn spec(name: &str) -> WorldSpec {
        WorldSpec {
            name: name.to_string(),
            size_xz: 256,
            ..WorldSpec::default()
        }
    }

    /// Writes a save of a world whose only terrain is chunk column (2, 2), with the player above it.
    fn save_world(name: &str, saves: &Path) {
        let mut saved = App::headless();
        saved.saves_dir = saves.to_path_buf();
        saved.generation.last_spec = Some(spec(name));
        saved
            .simulation
            .set_position_yaw_pitch(Vec3::new(40.5, 200.0, 40.5), 0.0, 0.0);
        let config = TerrainConfig {
            world_size: 256,
            ..TerrainConfig::default()
        };
        let grid = saved
            .light_system
            .as_mut()
            .expect("App starts with a light system")
            .grid_mut();
        for cy in 0..=8 {
            for (pos, material, resource) in generate_chunk(&config, IVec3::new(2, cy, 2)) {
                grid.mutator().place(pos, material, resource);
            }
        }
        auto_save_on_shutdown(&mut saved, &GameClock::default(), saves, &[])
            .expect("the world is saved");
    }

    /// An app that has loaded the save in `saves`, as the player does from the menu.
    fn load_world(saves: &Path) -> App {
        let mut app = App::headless();
        app.saves_dir = saves.to_path_buf();
        crate::app::scene_loader::load_scene(
            &mut app,
            &mut GameClock::default(),
            &saves.join("scene.bin"),
        )
        .expect("the save loads");
        app.simulation.set_camera_mode(CameraMode::Isometric);
        app
    }

    /// A loaded world and the surface block of column (40, 40), in a fresh saves dir.
    fn loaded_world(name: &str) -> (tempfile::TempDir, App, IVec3) {
        let temp = tempfile::tempdir().expect("temp dir");
        save_world(name, temp.path());
        let app = load_world(temp.path());
        let block = surface_block(&app, 40, 40);
        (temp, app, block)
    }

    fn surface_block(app: &App, x: i32, z: i32) -> IVec3 {
        let grid = app.light_system.as_ref().expect("light system").grid();
        let y = grid.get_height(x, z).expect("terrain under the column");
        IVec3::new(x, y, z)
    }

    /// Mines `pos` the way the pawn does: the grid changes and a removal is published.
    fn mine(app: &mut App, pos: IVec3) {
        let grid = app.light_system.as_mut().expect("light system").grid_mut();
        let old_material_id = grid.material_at(pos).expect("a block to mine");
        assert!(grid.mutator().remove(pos), "the block is removed");
        app.event_bus.publish(WorldEvent::BlockRemoved {
            position: pos,
            old_material_id,
            reason: BlockChangeReason::Player,
        });
    }

    fn is_solid(app: &App, pos: IVec3) -> bool {
        app.light_system
            .as_ref()
            .expect("light system")
            .grid()
            .is_solid_at(pos)
    }

    fn has_column(app: &App, block: IVec3) -> bool {
        app.light_system
            .as_ref()
            .expect("light system")
            .grid()
            .has_chunk(IVec3::new(
                block.x.div_euclid(16),
                block.y.div_euclid(16),
                block.z.div_euclid(16),
            ))
    }

    /// Whether `pos` is a block after the player loads the save in `saves` and its column streams in.
    fn solid_after_loading(saves: &Path, pos: IVec3) -> bool {
        let mut app = load_world(saves);
        let column = (pos.x.div_euclid(16), pos.z.div_euclid(16));
        let streamer = app
            .chunk_streamer
            .as_ref()
            .expect("a loaded world streams chunks");
        let grid = app.light_system.as_mut().expect("light system").grid_mut();
        streamer.load_columns_now(grid, column, 0);
        grid.is_solid_at(pos)
    }

    fn sim() -> HeadlessLoop<App> {
        HeadlessLoop::<App>::new(LoopConfig::new(TICK_HZ))
    }

    /// Runs `ticks` ticks, each followed by a frame.
    fn play(app: &mut App, sim: &mut HeadlessLoop<App>, ticks: u32) {
        let mut ran = 0;
        for _ in 0..ticks {
            ran += sim.advance(app, frame_dt());
        }
        assert_eq!(ran, ticks, "each frame delivers one tick");
    }

    fn close_window(app: &mut App) {
        let clock = GameClock::default();
        let mut ctx = EventContext::new(&clock, None, None);

        Game::event(
            app,
            &mut ctx,
            moho_app::Event::Window(WindowEvent::CloseRequested),
        );

        assert!(ctx.exit_requested());
    }

    fn choose_exit_in_the_menu(app: &mut App) {
        app.game_state = GameState::Paused;
        app.exit_requested = true;
        let clock = GameClock::default();
        let tick_length = LoopConfig::new(TICK_HZ).tick_length();
        let mut ctx = FrameContext::new(tick_length, &clock, None, None);

        Game::frame(app, &mut ctx, 0.5);

        assert!(ctx.exit_requested());
    }

    /// Moves the player to `x` and runs the frames that stream chunks around them.
    fn walk_to(app: &mut App, sim: &mut HeadlessLoop<App>, x: f32) {
        app.simulation
            .set_position_yaw_pitch(Vec3::new(x, 200.0, 40.5), 0.0, 0.0);
        play(app, sim, 5);
    }

    /// Far enough from column (2, 2) to unload it at the default unload radius.
    const FAR_X: f32 = 40.5 + 16.0 * 30.0;

    #[test]
    fn mined_block_is_absent_after_staying_near_then_closing() {
        let (temp, mut app, block) = loaded_world("autosave_unit_test_near_close");
        assert!(is_solid(&app, block), "the block is there to mine");
        mine(&mut app, block);
        play(&mut app, &mut sim(), 5);

        close_window(&mut app);

        assert!(!solid_after_loading(temp.path(), block));
    }

    #[test]
    fn mined_block_is_absent_after_staying_near_then_menu_exit() {
        let (temp, mut app, block) = loaded_world("autosave_unit_test_near_menu");
        assert!(is_solid(&app, block), "the block is there to mine");
        mine(&mut app, block);
        play(&mut app, &mut sim(), 5);

        choose_exit_in_the_menu(&mut app);

        assert!(!solid_after_loading(temp.path(), block));
    }

    #[test]
    fn mined_block_is_absent_after_walking_back_then_closing() {
        let (temp, mut app, block) = loaded_world("autosave_unit_test_walk_back");
        let mut sim = sim();
        assert!(is_solid(&app, block), "the block is there to mine");
        mine(&mut app, block);
        play(&mut app, &mut sim, 5);
        walk_to(&mut app, &mut sim, FAR_X);
        assert!(!has_column(&app, block), "the column unloaded");

        walk_to(&mut app, &mut sim, 40.5);

        assert!(has_column(&app, block), "the column streamed back in");
        assert!(!is_solid(&app, block), "the mined block stayed mined");
        close_window(&mut app);
        assert!(!solid_after_loading(temp.path(), block));
    }

    #[test]
    fn mined_block_is_absent_after_staying_away_then_closing() {
        let (temp, mut app, block) = loaded_world("autosave_unit_test_stay_away");
        let mut sim = sim();
        assert!(is_solid(&app, block), "the block is there to mine");
        mine(&mut app, block);
        play(&mut app, &mut sim, 5);
        walk_to(&mut app, &mut sim, FAR_X);
        assert!(!has_column(&app, block), "the column unloaded");

        close_window(&mut app);

        assert!(!solid_after_loading(temp.path(), block));
    }

    #[test]
    fn closing_with_no_world_keeps_the_previous_save() {
        let temp = tempfile::tempdir().expect("temp dir");
        save_world("autosave_unit_test_no_world_close", temp.path());
        let scene = temp.path().join("scene.bin");
        let before = std::fs::read(&scene).expect("the previous save");
        let mut app = App::headless();
        app.saves_dir = temp.path().to_path_buf();

        close_window(&mut app);

        let after = std::fs::read(&scene).expect("the save is still there");
        assert!(
            after == before,
            "closing from the start menu leaves the saved world alone"
        );
    }

    #[test]
    fn menu_exit_with_no_world_keeps_the_previous_save() {
        let temp = tempfile::tempdir().expect("temp dir");
        save_world("autosave_unit_test_no_world_exit", temp.path());
        let scene = temp.path().join("scene.bin");
        let before = std::fs::read(&scene).expect("the previous save");
        let mut app = App::headless();
        app.saves_dir = temp.path().to_path_buf();

        choose_exit_in_the_menu(&mut app);

        let after = std::fs::read(&scene).expect("the save is still there");
        assert!(
            after == before,
            "Exit from the start menu leaves the saved world alone"
        );
    }

    #[test]
    fn world_loaded_after_leaving_an_edited_one_is_not_autosaved_untouched() {
        let (temp, mut app, block) = loaded_world("autosave_unit_test_stale_schedule");
        app.autosave_interval = std::time::Duration::from_secs(2);
        let interval_ticks = 2 * TICK_HZ;
        let mut sim = sim();
        mine(&mut app, block);
        play(&mut app, &mut sim, 5);
        app.main_menu_requested = true;
        let clock = GameClock::default();
        let tick_length = LoopConfig::new(TICK_HZ).tick_length();
        let mut ctx = FrameContext::new(tick_length, &clock, None, None);
        Game::frame(&mut app, &mut ctx, 0.5);
        assert_eq!(app.game_state, GameState::Menu, "the player is at the menu");
        let save = temp.path().join("scene.bin");
        let left_behind = std::fs::read(&save).expect("leaving saved the edited world");
        crate::app::scene_loader::load_scene(&mut app, &mut GameClock::default(), &save)
            .expect("the save loads");
        std::fs::remove_file(&save).expect("clear the save, so only an autosave can write one");

        play(&mut app, &mut sim, interval_ticks + 5);

        assert!(
            !save.exists(),
            "a world with no edit since loading is not autosaved ({} bytes saved)",
            left_behind.len()
        );
    }

    #[test]
    fn killed_session_keeps_every_edit_before_the_last_autosave() {
        let (temp, mut app, _) = loaded_world("autosave_unit_test_killed");
        app.autosave_interval = std::time::Duration::from_secs(2);
        let interval_ticks = 2 * TICK_HZ;
        let blocks: Vec<IVec3> = (40..44).map(|x| surface_block(&app, x, 40)).collect();
        let mut sim = sim();

        for block in &blocks[..3] {
            mine(&mut app, *block);
            play(&mut app, &mut sim, interval_ticks + 5);
        }
        mine(&mut app, blocks[3]);
        play(&mut app, &mut sim, 30);
        drop(app);

        for block in &blocks[..3] {
            assert!(
                !solid_after_loading(temp.path(), *block),
                "{block} was mined before the last autosave"
            );
        }
    }

    #[test]
    fn interval_of_30_seconds_saves_the_edit_after_30_seconds() {
        let (temp, mut app, block) = loaded_world("autosave_unit_test_30_seconds");
        let save = temp.path().join("scene.bin");
        std::fs::remove_file(&save)
            .expect("clear the setup save, so only an autosave can write one");
        app.autosave_interval = std::time::Duration::from_secs(30);
        let mut sim = sim();
        mine(&mut app, block);

        play(&mut app, &mut sim, 30 * TICK_HZ - 6);
        let at_29_9_seconds = save.exists();
        play(&mut app, &mut sim, 6);

        assert!(!at_29_9_seconds, "nothing is saved before the interval");
        assert!(save.is_file(), "the edit is saved after 30 seconds");
        assert!(!solid_after_loading(temp.path(), block));
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
}
