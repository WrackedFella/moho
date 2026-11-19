/// When the world is next due an autosave, in simulation ticks.
///
/// A save is due once the world has been edited and `interval_ticks` ticks have passed
/// since the last save.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutosaveSchedule {
    pub(crate) interval_ticks: u64,
    pub(crate) ticks_since_save: u64,
    pub(crate) edited: bool,
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

    /// Whether the world is edited and a full interval has passed since the last save.
    pub fn due(&self) -> bool {
        self.edited && self.ticks_since_save >= self.interval_ticks
    }

    /// Records a failed save: the clock restarts so the retry waits a full interval, and the
    /// edit stays pending.
    pub fn save_failed(&mut self) {
        self.ticks_since_save = 0;
    }

    /// Records a successful save: the clock and the edit flag both restart.
    pub fn saved(&mut self) {
        self.ticks_since_save = 0;
        self.edited = false;
    }
}

/// Save the current scene to `saves_dir/scene.bin`, creating the directory
/// if needed.
///
/// Writes nothing when no world is loaded (no `generation.last_spec`), so leaving
/// from the start menu cannot overwrite an earlier save with an empty world.
pub fn auto_save_on_shutdown(
    app: &mut crate::App,
    clock: &moho_app::GameClock,
    saves_dir: &std::path::Path,
    lights: &[moho_render_api::LightDesc],
) -> Result<(), Box<dyn std::error::Error>> {
    let Some(mut spec) = app.generation.last_spec.clone() else {
        tracing::debug!("No world loaded; autosave skipped");
        return Ok(());
    };
    tracing::info!("Auto-saving on shutdown...");

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
    use glam::Vec3;
    use moho_app::{EventContext, FrameContext, Game, GameClock, HeadlessLoop, LoopConfig};
    use std::path::Path;
    use std::time::Duration;
    use winit::event::WindowEvent;

    const WORLD_SIZE: u32 = 64;

    fn spec(name: &str, size_xz: u32) -> WorldSpec {
        WorldSpec {
            name: name.to_string(),
            size_xz,
            ..WorldSpec::default()
        }
    }

    fn tick_length() -> Duration {
        LoopConfig::new(TICK_HZ).tick_length()
    }

    /// The top block of the terrain column at `(x, z)`.
    fn surface(app: &App, x: i32, z: i32) -> BlockPos {
        let grid = app.light_system.as_ref().expect("light system").grid();
        BlockPos::new(x, grid.get_height(x, z).expect("terrain in the column"), z)
    }

    /// A playing app with a world loaded: terrain around the origin, a streamer writing
    /// under `saves`, and the world's spec set as a load or a new world sets it.
    fn loaded_app(world: &str, saves: &Path) -> App {
        let mut app = App::headless();
        app.saves_dir = saves.to_path_buf();
        app.generation.last_spec = Some(spec(world, WORLD_SIZE));
        let streamer = ChunkStreamer::new(
            TerrainConfig {
                world_size: WORLD_SIZE,
                ..TerrainConfig::default()
            },
            StreamingConfig {
                load_radius_chunks: 1,
                unload_radius_chunks: 1,
                chunks_per_frame: 9,
            },
            world,
            saves,
        );
        let grid = app.light_system.as_mut().expect("light system").grid_mut();
        streamer.load_columns_now(grid, (0, 0), 1);
        app.chunk_streamer = Some(streamer);
        app.game_state = GameState::Playing;
        let top = surface(&app, 8, 8);
        app.simulation
            .set_position_yaw_pitch(Vec3::new(8.5, top.y as f32 + 3.0, 8.5), 0.0, 0.0);
        app
    }

    /// Mines `pos` the way the pawn does: the block leaves the grid and the removal is published.
    fn mine(app: &mut App, pos: BlockPos) {
        let grid = app.light_system.as_mut().expect("light system").grid_mut();
        let old_material_id = grid.material_at(pos).expect("a block to mine");
        assert!(grid.mutator().remove(pos));
        app.event_bus.publish(WorldEvent::BlockRemoved {
            position: pos,
            old_material_id,
            reason: BlockChangeReason::Player,
        });
    }

    /// Moves the player and runs the streamer for a few frames, as walking does.
    fn stream_at(app: &mut App, position: Vec3, frames: usize) {
        app.simulation.set_position_yaw_pitch(position, 0.0, 0.0);
        let streamer = app.chunk_streamer.as_mut().expect("streamer");
        let grid = app.light_system.as_mut().expect("light system").grid_mut();
        for _ in 0..frames {
            streamer.update(grid, position);
        }
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

    fn run_frame(app: &mut App) -> bool {
        let clock = GameClock::default();
        let mut ctx = FrameContext::new(tick_length(), &clock, None, None);
        Game::frame(app, &mut ctx, 0.5);
        ctx.exit_requested()
    }

    fn menu_exit(app: &mut App) {
        app.exit_requested = true;
        assert!(run_frame(app));
    }

    fn return_to_main_menu(app: &mut App) {
        app.main_menu_requested = true;
        assert!(!run_frame(app));
    }

    fn load_save(saves: &Path) -> App {
        let mut loaded = App::headless();
        loaded.saves_dir = saves.to_path_buf();
        crate::app::scene_loader::load_scene(
            &mut loaded,
            &mut GameClock::default(),
            &saves.join("scene.bin"),
        )
        .expect("the save loads");
        loaded
    }

    fn saved_blocks(saves: &Path) -> Vec<BlockPos> {
        let (_, _, records) =
            crate::save::read_scene_and_metadata(saves.join("scene.bin")).expect("a readable save");
        records
            .iter()
            .map(|record| BlockPos::new(record.x, record.y, record.z))
            .collect()
    }

    /// Runs `ticks` ticks, then one frame, as the loop does.
    fn play(sim: &mut HeadlessLoop<App>, app: &mut App, ticks: u32) {
        sim.step(app, ticks);
        sim.advance(app, Duration::ZERO);
    }

    #[derive(Clone, Copy)]
    enum Movement {
        StaysNear,
        WalksOutAndBack,
        WalksOutAndStaysAway,
    }

    #[derive(Clone, Copy)]
    enum Exit {
        CloseWindow,
        MenuExit,
    }

    fn assert_mined_block_absent_after(world: &'static str, movement: Movement, exit: Exit) {
        let temp = tempfile::tempdir().expect("temp dir");
        let saves = temp.path().join("saves");
        let mut app = loaded_app(world, &saves);
        let mined = surface(&app, 8, 8);
        let near = app.simulation.position();
        let far = Vec3::new(1000.5, near.y, 1000.5);
        mine(&mut app, mined);

        match movement {
            Movement::StaysNear => {}
            Movement::WalksOutAndBack => {
                stream_at(&mut app, far, 4);
                stream_at(&mut app, near, 4);
            }
            Movement::WalksOutAndStaysAway => stream_at(&mut app, far, 4),
        }
        match exit {
            Exit::CloseWindow => close_window(&mut app),
            Exit::MenuExit => menu_exit(&mut app),
        }
        let mut loaded = load_save(&saves);
        // A column the player stayed away from streams in when they come back to it.
        stream_at(&mut loaded, near, 20);

        let grid = loaded.light_system.as_ref().expect("light system").grid();
        let mined_chunk = glam::IVec3::new(0, mined.y.div_euclid(16), 0);
        assert!(
            grid.has_chunk(mined_chunk),
            "the mined block's chunk is loaded"
        );
        assert!(!grid.is_solid_at(mined), "the mined block is absent");
    }

    #[test]
    fn mined_block_is_absent_after_staying_near_then_closing() {
        assert_mined_block_absent_after(
            "autosave_unit_test_near_close",
            Movement::StaysNear,
            Exit::CloseWindow,
        );
    }

    #[test]
    fn mined_block_is_absent_after_staying_near_then_menu_exit() {
        assert_mined_block_absent_after(
            "autosave_unit_test_near_menu_exit",
            Movement::StaysNear,
            Exit::MenuExit,
        );
    }

    #[test]
    fn mined_block_is_absent_after_walking_back_then_closing() {
        assert_mined_block_absent_after(
            "autosave_unit_test_back_close",
            Movement::WalksOutAndBack,
            Exit::CloseWindow,
        );
    }

    #[test]
    fn mined_block_is_absent_after_staying_away_then_closing() {
        assert_mined_block_absent_after(
            "autosave_unit_test_away_close",
            Movement::WalksOutAndStaysAway,
            Exit::CloseWindow,
        );
    }

    /// A world is saved, then an app with no world (the start menu) leaves by `leave`.
    fn assert_leaving_with_no_world_keeps_the_save(
        world: &'static str,
        leave: impl FnOnce(&mut App),
    ) {
        let temp = tempfile::tempdir().expect("temp dir");
        let saves = temp.path().join("saves");
        let mut session = loaded_app(world, &saves);
        let mined = surface(&session, 8, 8);
        mine(&mut session, mined);
        close_window(&mut session);
        let kept = saved_blocks(&saves).len();
        let mut start_menu = App::headless();
        start_menu.saves_dir = saves.clone();

        leave(&mut start_menu);

        assert!(kept > 0, "the first session saved its world");
        assert_eq!(
            saved_blocks(&saves).len(),
            kept,
            "the save is not overwritten"
        );
    }

    #[test]
    fn closing_with_no_world_keeps_the_previous_save() {
        assert_leaving_with_no_world_keeps_the_save("autosave_unit_test_empty_close", close_window);
    }

    #[test]
    fn menu_exit_with_no_world_keeps_the_previous_save() {
        assert_leaving_with_no_world_keeps_the_save("autosave_unit_test_empty_exit", menu_exit);
    }

    #[test]
    fn returning_to_the_main_menu_with_no_world_keeps_the_previous_save() {
        assert_leaving_with_no_world_keeps_the_save(
            "autosave_unit_test_empty_main_menu",
            return_to_main_menu,
        );
    }

    #[test]
    fn killed_session_keeps_every_edit_before_the_last_autosave() {
        let temp = tempfile::tempdir().expect("temp dir");
        let saves = temp.path().join("saves");
        let mut app = loaded_app("autosave_unit_test_killed", &saves);
        app.autosave_interval = Duration::from_secs(1);
        let mut sim = HeadlessLoop::<App>::new(LoopConfig::new(TICK_HZ));
        let (first, second, third) = (
            surface(&app, 8, 8),
            surface(&app, 9, 8),
            surface(&app, 10, 8),
        );

        mine(&mut app, first);
        play(&mut sim, &mut app, TICK_HZ + 1);
        mine(&mut app, second);
        play(&mut sim, &mut app, TICK_HZ + 1);
        mine(&mut app, third);
        play(&mut sim, &mut app, TICK_HZ / 2);
        drop(app);

        assert!(saves.join("scene.bin").is_file(), "an autosave was written");
        let loaded = load_save(&saves);
        let grid = loaded.light_system.as_ref().expect("light system").grid();
        assert!(!grid.is_solid_at(first), "an edit from the first interval");
        assert!(
            !grid.is_solid_at(second),
            "an edit from the second interval"
        );
        assert!(
            grid.is_solid_at(third),
            "the save is the last autosave, not a save at the kill"
        );
    }

    #[test]
    fn interval_of_30_seconds_saves_the_edit_after_30_seconds() {
        let temp = tempfile::tempdir().expect("temp dir");
        let saves = temp.path().join("saves");
        let mut app = loaded_app("autosave_unit_test_interval", &saves);
        app.autosave_interval = Duration::from_secs(30);
        let mut sim = HeadlessLoop::<App>::new(LoopConfig::new(TICK_HZ));
        let mined = surface(&app, 8, 8);
        mine(&mut app, mined);

        // 29.9 s of play.
        play(&mut sim, &mut app, 30 * TICK_HZ - 6);
        let saved_early = saves.join("scene.bin").exists();
        play(&mut sim, &mut app, 6);

        assert!(!saved_early, "nothing is saved at 29.9 s");
        assert!(saves.join("scene.bin").is_file(), "saved at 30 s");
        let blocks = saved_blocks(&saves);
        assert!(!blocks.is_empty(), "the world is saved at 30 s");
        assert!(!blocks.contains(&mined), "the save holds the edit");
    }

    #[test]
    fn unedited_world_is_not_saved_after_the_interval() {
        let temp = tempfile::tempdir().expect("temp dir");
        let saves = temp.path().join("saves");
        let mut app = loaded_app("autosave_unit_test_idle", &saves);
        app.autosave_interval = Duration::from_secs(1);
        let mut sim = HeadlessLoop::<App>::new(LoopConfig::new(TICK_HZ));

        play(&mut sim, &mut app, 5 * TICK_HZ);

        assert!(
            !saves.join("scene.bin").exists(),
            "an idle world is not rewritten"
        );
    }

    #[test]
    fn schedule_is_due_only_after_the_interval_and_an_edit() {
        let mut unedited = AutosaveSchedule::new(3);
        let mut edited = AutosaveSchedule::new(3);
        edited.mark_edited();

        for _ in 0..3 {
            unedited.tick();
        }
        edited.tick();
        edited.tick();
        let short = edited.due();
        edited.tick();

        assert!(!unedited.due(), "no edit, no save");
        assert!(!short, "an edit before the interval is not due");
        assert!(edited.due(), "an edit and a full interval is due");
    }

    #[test]
    fn schedule_resets_on_save() {
        let mut schedule = AutosaveSchedule::new(3);
        schedule.mark_edited();
        for _ in 0..3 {
            schedule.tick();
        }
        assert!(schedule.due());

        schedule.saved();
        let after_save = schedule.due();
        for _ in 0..3 {
            schedule.tick();
        }
        let unedited_after_interval = schedule.due();
        schedule.mark_edited();
        let edited_after_interval = schedule.due();

        assert!(!after_save);
        assert!(!unedited_after_interval, "the edit flag was cleared");
        assert!(edited_after_interval, "the clock restarted at the save");
    }

    #[test]
    fn schedule_failed_save_retries_next_interval() {
        let temp = tempfile::tempdir().expect("temp dir");
        let saves = temp.path().join("saves");
        let blocker = temp.path().join("blocker");
        std::fs::write(&blocker, b"a file where the saves directory must go").expect("blocker");
        let mut app = loaded_app("autosave_unit_test_retry", &saves);
        app.autosave_interval = Duration::from_secs(1);
        app.saves_dir = blocker.join("saves");
        let mut sim = HeadlessLoop::<App>::new(LoopConfig::new(TICK_HZ));
        let mined = surface(&app, 8, 8);
        mine(&mut app, mined);

        play(&mut sim, &mut app, TICK_HZ);
        assert!(
            app.autosave_schedule.edited,
            "a failed save keeps the edit pending"
        );
        assert!(
            !app.autosave_schedule.due(),
            "a failed save is not retried at once"
        );
        sim.step(&mut app, TICK_HZ - 1);
        assert!(!app.autosave_schedule.due(), "not before a full interval");
        sim.step(&mut app, 1);
        assert!(app.autosave_schedule.due(), "retried after the interval");
        app.saves_dir = saves.clone();
        sim.advance(&mut app, Duration::ZERO);

        assert!(
            !saved_blocks(&saves).is_empty(),
            "the retry saved the world"
        );
        assert!(!app.autosave_schedule.edited, "the retry cleared the edit");
    }

    #[test]
    fn loaded_world_is_not_rewritten_after_the_interval_without_an_edit() {
        let temp = tempfile::tempdir().expect("temp dir");
        let saves = temp.path().join("saves");
        let mut session = loaded_app("autosave_unit_test_load_idle", &saves);
        let mined = surface(&session, 8, 8);
        mine(&mut session, mined);
        close_window(&mut session);
        let mut loaded = load_save(&saves);
        loaded.autosave_interval = Duration::from_secs(1);
        std::fs::remove_file(saves.join("scene.bin")).expect("remove the save to detect a rewrite");
        let mut sim = HeadlessLoop::<App>::new(LoopConfig::new(TICK_HZ));

        play(&mut sim, &mut loaded, 5 * TICK_HZ);

        assert!(
            !saves.join("scene.bin").exists(),
            "a loaded world with no edit is not rewritten"
        );
    }

    #[test]
    fn failed_autosave_is_followed_by_a_save_of_the_edit_once_the_dir_is_fixed() {
        let temp = tempfile::tempdir().expect("temp dir");
        let saves = temp.path().join("saves");
        let blocker = temp.path().join("blocker");
        std::fs::write(&blocker, b"a file where the saves directory must go").expect("blocker");
        let mut app = loaded_app("autosave_unit_test_recover", &saves);
        app.autosave_interval = Duration::from_secs(1);
        app.saves_dir = blocker.join("saves");
        let mut sim = HeadlessLoop::<App>::new(LoopConfig::new(TICK_HZ));
        let mined = surface(&app, 8, 8);
        mine(&mut app, mined);
        play(&mut sim, &mut app, TICK_HZ);
        let pending_after_failure = app.autosave_schedule.edited;
        let written_while_broken = saves.join("scene.bin").exists();
        app.saves_dir = saves.clone();

        play(&mut sim, &mut app, TICK_HZ);

        assert!(pending_after_failure, "the failed save keeps the edit");
        assert!(!written_while_broken, "nothing was written while broken");
        assert!(saves.join("scene.bin").is_file(), "the recovery saved");
        let blocks = saved_blocks(&saves);
        assert!(!blocks.is_empty(), "the recovered save holds the world");
        assert!(
            !blocks.contains(&mined),
            "the recovered save holds the edit"
        );
    }

    #[test]
    fn autosave_then_load_restores_actors_camera_time_and_blocks() {
        let temp = tempfile::tempdir().expect("temp dir");
        let saves = temp.path().join("not").join("yet").join("created");
        let mut saved = App::headless();
        saved.generation.last_spec = Some(spec("autosave_unit_test_round_trip", 64));
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
