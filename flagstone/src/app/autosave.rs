/// When the world is next due an autosave, counted in simulation ticks.
///
/// Due once the world has been edited and `interval_ticks` ticks have passed since the
/// last save (or the last failed attempt).
#[derive(Debug, Clone, PartialEq, Eq)]
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

    /// One simulation tick passed while playing.
    pub fn tick(&mut self) {
        self.ticks_since_save = self.ticks_since_save.saturating_add(1);
    }

    /// The world changed since the last save.
    pub fn mark_edited(&mut self) {
        self.edited = true;
    }

    /// Whether the frame should autosave now.
    pub fn due(&self) -> bool {
        self.edited && self.ticks_since_save >= self.interval_ticks
    }

    /// A save succeeded: no unsaved edit, the interval starts over.
    pub fn saved(&mut self) {
        self.ticks_since_save = 0;
        self.edited = false;
    }

    /// A save failed: the interval starts over, the edit stays unsaved.
    pub fn save_failed(&mut self) {
        self.ticks_since_save = 0;
    }
}

/// Save the current scene to `saves_dir/scene.bin`, creating the directory
/// if needed. With no world loaded (`generation.last_spec` unset) nothing is
/// written, so a session that never loaded one cannot overwrite a save.
pub fn auto_save_on_shutdown(
    app: &mut crate::App,
    clock: &moho_app::GameClock,
    saves_dir: &std::path::Path,
    lights: &[moho_render_api::LightDesc],
) -> Result<(), Box<dyn std::error::Error>> {
    let Some(mut spec) = app.generation.last_spec.clone() else {
        tracing::debug!("No world loaded; skipping autosave");
        return Ok(());
    };
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
        "Auto-saved scene (envelope)"
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::App;
    use crate::app::chunk_streamer::ChunkStreamer;
    use crate::game_state::GameState;
    use flagstone_game::TICK_HZ;
    use flagstone_game::scene_builders::{TerrainConfig, WorldSpec};
    use flagstone_voxel::{BlockChangeReason, BlockPos, StreamingConfig, WorldEvent};
    use moho_app::{EventContext, FrameContext, Game, GameClock, HeadlessLoop, LoopConfig};
    use std::path::{Path, PathBuf};
    use std::time::Duration;
    use winit::event::WindowEvent;

    const WORLD_SIZE: u32 = 256;

    /// A playing app with one terrain column loaded, saving into its own temp dir.
    struct Session {
        app: App,
        saves: PathBuf,
        _temp: tempfile::TempDir,
    }

    fn tick_length() -> Duration {
        LoopConfig::new(TICK_HZ).tick_length()
    }

    /// Chunk files of `world` may linger under `saves/` from an earlier run.
    fn session(world: &str) -> Session {
        crate::save::clear_chunk_files(world).expect("clear stale chunk files");
        let temp = tempfile::tempdir().expect("temp dir");
        let saves = temp.path().join("saves");
        let mut app = App::headless();
        app.game_state = GameState::Playing;
        app.saves_dir = saves.clone();
        app.generation.last_spec = Some(WorldSpec {
            name: world.to_string(),
            size_xz: WORLD_SIZE,
            ..WorldSpec::default()
        });
        let streamer = ChunkStreamer::new(
            TerrainConfig {
                world_size: WORLD_SIZE,
                ..TerrainConfig::default()
            },
            StreamingConfig {
                load_radius_chunks: 0,
                unload_radius_chunks: 0,
                chunks_per_frame: 1,
            },
            world,
            &saves,
        );
        streamer.load_columns_now(
            app.light_system
                .as_mut()
                .expect("App starts with a light system")
                .grid_mut(),
            (0, 0),
            0,
        );
        app.chunk_streamer = Some(streamer);
        let top = surface_block(&app, 8, 8);
        app.simulation.set_position_yaw_pitch(
            glam::Vec3::new(8.5, top.y as f32 + 2.0, 8.5),
            0.0,
            0.0,
        );
        Session {
            app,
            saves,
            _temp: temp,
        }
    }

    fn surface_block(app: &App, x: i32, z: i32) -> BlockPos {
        let grid = app.light_system.as_ref().expect("light system").grid();
        let y = grid
            .get_height(x, z)
            .unwrap_or_else(|| panic!("terrain at ({x}, {z})"));
        BlockPos::new(x, y, z)
    }

    /// Mines the top block of the column at (`x`, 8), as the pawn does: the grid loses
    /// it and a removal event is published.
    fn mine_surface_block(app: &mut App, x: i32) -> BlockPos {
        let block = surface_block(app, x, 8);
        let removed = app
            .light_system
            .as_mut()
            .expect("light system")
            .grid_mut()
            .mutator()
            .remove(block);
        assert!(removed, "there is a block to mine at {block}");
        app.event_bus.publish(WorldEvent::BlockRemoved {
            position: block,
            old_material_id: 1,
            reason: BlockChangeReason::Player,
        });
        app.event_bus.process_deferred();
        block
    }

    /// Moves the player and streams chunks as the tick would.
    fn walk_to(app: &mut App, position: glam::Vec3) {
        let mut streamer = app.chunk_streamer.take().expect("a streamer");
        streamer.update(
            app.light_system.as_mut().expect("light system").grid_mut(),
            position,
        );
        app.chunk_streamer = Some(streamer);
        app.simulation.set_position_yaw_pitch(position, 0.0, 0.0);
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

    fn exit_from_the_menu(app: &mut App) {
        app.show_menu();
        assert_eq!(app.game_state, GameState::Menu);
        app.exit_requested = true;
        let clock = GameClock::default();
        let mut ctx = FrameContext::new(tick_length(), &clock, None, None);

        Game::frame(app, &mut ctx, 0.5);

        assert!(ctx.exit_requested());
    }

    fn load_latest_save(saves: &Path) -> App {
        let mut app = App::headless();
        app.saves_dir = saves.to_path_buf();
        crate::app::scene_loader::load_scene(
            &mut app,
            &mut GameClock::default(),
            &saves.join("scene.bin"),
        )
        .expect("the save loads");
        app
    }

    /// Loads the latest save, walks to `block` and checks it stays mined, with its
    /// neighbour below still there.
    fn assert_mined_block_absent_after_load(saves: &Path, block: BlockPos) {
        let mut loaded = load_latest_save(saves);

        walk_to(
            &mut loaded,
            glam::Vec3::new(
                block.x as f32 + 0.5,
                block.y as f32 + 3.0,
                block.z as f32 + 0.5,
            ),
        );

        let grid = loaded.light_system.as_ref().expect("light system").grid();
        assert!(!grid.is_solid_at(block), "the mined block {block} is back");
        assert!(
            grid.is_solid_at(BlockPos::new(block.x, block.y - 1, block.z)),
            "the terrain under {block} is gone"
        );
    }

    fn far_away() -> glam::Vec3 {
        glam::Vec3::new(8.5 + 16.0 * 10.0, 100.0, 8.5)
    }

    fn near_the_block() -> glam::Vec3 {
        glam::Vec3::new(8.5, 100.0, 8.5)
    }

    #[test]
    fn mined_block_is_absent_after_staying_near_then_closing() {
        let mut s = session("autosave_test_stay_near_then_close");
        let block = mine_surface_block(&mut s.app, 8);

        close_window(&mut s.app);

        assert_mined_block_absent_after_load(&s.saves, block);
    }

    #[test]
    fn mined_block_is_absent_after_staying_near_then_menu_exit() {
        let mut s = session("autosave_test_stay_near_then_menu_exit");
        let block = mine_surface_block(&mut s.app, 8);

        exit_from_the_menu(&mut s.app);

        assert_mined_block_absent_after_load(&s.saves, block);
    }

    #[test]
    fn mined_block_is_absent_after_walking_back_then_closing() {
        let mut s = session("autosave_test_walk_back_then_close");
        let block = mine_surface_block(&mut s.app, 8);
        walk_to(&mut s.app, far_away());
        walk_to(&mut s.app, near_the_block());

        close_window(&mut s.app);

        assert_mined_block_absent_after_load(&s.saves, block);
    }

    #[test]
    fn mined_block_is_absent_after_staying_away_then_closing() {
        let mut s = session("autosave_test_stay_away_then_close");
        let block = mine_surface_block(&mut s.app, 8);
        walk_to(&mut s.app, far_away());

        close_window(&mut s.app);

        assert_mined_block_absent_after_load(&s.saves, block);
    }

    /// A save of a mined world, then a fresh app (start menu, no world) on the same saves dir.
    fn previous_save_and_a_fresh_app(world: &str) -> (Session, BlockPos, Vec<u8>, App) {
        let mut s = session(world);
        let block = mine_surface_block(&mut s.app, 8);
        close_window(&mut s.app);
        let previous = std::fs::read(s.saves.join("scene.bin")).expect("the previous save");
        let mut fresh = App::headless();
        fresh.saves_dir = s.saves.clone();
        assert_eq!(fresh.game_state, GameState::Menu);
        (s, block, previous, fresh)
    }

    #[test]
    fn closing_with_no_world_keeps_the_previous_save() {
        let (s, block, previous, mut fresh) =
            previous_save_and_a_fresh_app("autosave_test_close_without_world");

        close_window(&mut fresh);

        let after = std::fs::read(s.saves.join("scene.bin")).expect("the save is still there");
        assert!(after == previous, "the previous save was overwritten");
        assert_mined_block_absent_after_load(&s.saves, block);
    }

    #[test]
    fn menu_exit_with_no_world_keeps_the_previous_save() {
        let (s, block, previous, mut fresh) =
            previous_save_and_a_fresh_app("autosave_test_menu_exit_without_world");

        exit_from_the_menu(&mut fresh);

        let after = std::fs::read(s.saves.join("scene.bin")).expect("the save is still there");
        assert!(after == previous, "the previous save was overwritten");
        assert_mined_block_absent_after_load(&s.saves, block);
    }

    #[test]
    fn killed_session_keeps_every_edit_before_the_last_autosave() {
        let interval = 120;
        let mut s = session("autosave_test_killed_session");
        s.app.autosave = AutosaveSchedule::new(interval);
        let mut sim = HeadlessLoop::<App>::new(LoopConfig::new(TICK_HZ));
        let first = mine_surface_block(&mut s.app, 8);
        sim.step(&mut s.app, interval);
        sim.advance(&mut s.app, Duration::ZERO);
        let second = mine_surface_block(&mut s.app, 9);
        sim.step(&mut s.app, interval);
        sim.advance(&mut s.app, Duration::ZERO);
        mine_surface_block(&mut s.app, 10);
        sim.step(&mut s.app, interval / 2);
        sim.advance(&mut s.app, Duration::ZERO);
        let saves = s.saves.clone();

        drop(s.app);

        assert_mined_block_absent_after_load(&saves, first);
        assert_mined_block_absent_after_load(&saves, second);
    }

    #[test]
    fn interval_of_30_seconds_saves_the_edit_after_30_seconds() {
        let mut s = session("autosave_test_thirty_second_interval");
        s.app.autosave = AutosaveSchedule::new(30 * TICK_HZ);
        let mut sim = HeadlessLoop::<App>::new(LoopConfig::new(TICK_HZ));
        let block = mine_surface_block(&mut s.app, 8);

        sim.step(&mut s.app, 30 * TICK_HZ - 6);
        sim.advance(&mut s.app, Duration::ZERO);
        let saved_at_29_9_seconds = s.saves.join("scene.bin").exists();
        sim.step(&mut s.app, 6);
        sim.advance(&mut s.app, Duration::ZERO);

        assert!(!saved_at_29_9_seconds, "saved before the interval passed");
        assert!(s.saves.join("scene.bin").is_file(), "no save at 30 seconds");
        assert_mined_block_absent_after_load(&s.saves, block);
    }

    #[test]
    fn schedule_is_due_only_after_the_interval_and_an_edit() {
        let mut idle = AutosaveSchedule::new(3);
        for _ in 0..10 {
            idle.tick();
        }
        let mut edited = AutosaveSchedule::new(3);
        edited.mark_edited();
        edited.tick();
        edited.tick();
        let due_one_tick_early = edited.due();
        edited.tick();

        assert!(!idle.due(), "an idle world is never saved");
        assert!(!due_one_tick_early, "due before the interval passed");
        assert!(edited.due(), "an edited world is due at the interval");
    }

    #[test]
    fn schedule_resets_on_save() {
        let mut schedule = AutosaveSchedule::new(3);
        schedule.mark_edited();
        for _ in 0..3 {
            schedule.tick();
        }
        let due_before_the_save = schedule.due();

        schedule.saved();
        let due_right_after = schedule.due();
        for _ in 0..10 {
            schedule.tick();
        }
        let due_without_a_new_edit = schedule.due();
        schedule.mark_edited();

        assert!(due_before_the_save);
        assert!(!due_right_after);
        assert!(!due_without_a_new_edit, "the saved edit is not saved again");
        assert!(schedule.due(), "a new edit past the interval is due");
    }

    #[test]
    fn schedule_failed_save_retries_next_interval() {
        let mut schedule = AutosaveSchedule::new(3);
        schedule.mark_edited();
        for _ in 0..3 {
            schedule.tick();
        }
        let due_before_the_failure = schedule.due();

        schedule.save_failed();
        let due_right_after = schedule.due();
        schedule.tick();
        schedule.tick();
        let due_one_tick_early = schedule.due();
        schedule.tick();

        assert!(due_before_the_failure);
        assert!(!due_right_after, "a failed save retries every frame");
        assert!(!due_one_tick_early, "retried before the interval passed");
        assert!(schedule.due(), "the edit stays unsaved, so it retries");
    }

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
}
