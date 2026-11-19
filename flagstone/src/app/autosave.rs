/// When the running world is next due an autosave, counted in simulation ticks.
#[derive(Debug, Clone)]
pub struct AutosaveSchedule {
    interval_ticks: u64,
    ticks_since_save: u64,
    edited: bool,
}

impl AutosaveSchedule {
    /// A schedule that comes due `interval_ticks` ticks after the last save, once edited.
    pub fn new(interval_ticks: u64) -> Self {
        Self {
            interval_ticks,
            ticks_since_save: 0,
            edited: false,
        }
    }

    /// A schedule due after `interval` of simulation time, rounded to whole ticks.
    pub fn for_interval(interval: std::time::Duration) -> Self {
        let ticks = (interval.as_secs_f64() * f64::from(flagstone_game::TICK_HZ)).round();
        Self::new(ticks as u64)
    }

    /// Counts one simulation tick since the last save.
    pub fn tick(&mut self) {
        todo!("count a tick since the last save")
    }

    /// Records that the world changed since the last save.
    pub fn mark_edited(&mut self) {
        todo!("record an edit")
    }

    /// Whether an edit is waiting and a full interval has passed since the last save.
    pub fn due(&self) -> bool {
        todo!("edited and ticks_since_save >= interval_ticks")
    }

    /// Records a successful save: no edit waits and the interval starts over.
    pub fn saved(&mut self) {
        todo!("reset ticks and edited")
    }

    /// Records a failed save: the interval starts over, the edit still waits.
    pub fn save_failed(&mut self) {
        todo!("reset ticks, keep edited")
    }
}

/// Save the current scene to `saves_dir/scene.bin`, creating the directory
/// if needed.
pub fn auto_save_on_shutdown(
    app: &mut crate::App,
    clock: &moho_app::GameClock,
    saves_dir: &std::path::Path,
    lights: &[moho_render_api::LightDesc],
) -> Result<(), Box<dyn std::error::Error>> {
    tracing::info!("Auto-saving on shutdown...");

    std::fs::create_dir_all(saves_dir)?;

    let save_path = saves_dir.join("scene.bin");
    let (yaw, pitch) = app.simulation.yaw_pitch();
    let camera_data = Some((app.simulation.position(), yaw, pitch));

    let scene_bytes =
        flagstone_game::scene_persistence::encode_to_bytes(&app.entities, camera_data, lights)?;
    let mut spec =
        app.generation
            .last_spec
            .clone()
            .unwrap_or(flagstone_game::scene_builders::WorldSpec {
                name: "autosave".to_string(),
                seed: None,
                size_xz: 64,
                day_length_seconds: 600.0,
                night_length_seconds: 420.0,
                initial_time_of_day: 6.0,
            });
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
    use crate::app::event_loop::{EventProcessor, FrameProcessor};
    use crate::game_state::GameState;
    use crate::input_event::InputEvent;
    use flagstone_game::TICK_HZ;
    use flagstone_game::scene_builders::{TerrainConfig, WorldSpec};
    use flagstone_voxel::{BlockPos, StreamingConfig};
    use moho_app::{Event, EventContext, FrameContext, Game, GameClock, HeadlessLoop, LoopConfig};
    use moho_core::events::UiEvent;
    use std::path::{Path, PathBuf};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;
    use winit::event::WindowEvent;

    const WORLD_SIZE: u32 = 256;

    /// Chunk files a streamer writes under the working directory's `saves/`, removed on drop.
    struct StrayChunkFiles(String);

    impl Drop for StrayChunkFiles {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(Path::new("saves").join(&self.0));
        }
    }

    /// A loaded world with the player aimed down at its surface, holding the mining tool.
    struct Session {
        app: App,
        saves: PathBuf,
        start: glam::Vec3,
        mined: Arc<Mutex<Vec<BlockPos>>>,
        mine_tx: std::sync::mpsc::Sender<InputEvent>,
        _temp: tempfile::TempDir,
        _stray: StrayChunkFiles,
    }

    enum Movement {
        StaysNear,
        WalksOutOfRangeAndBack,
        StaysOutOfRange,
    }

    enum Exit {
        ClosingTheWindow,
        ExitInTheMenu,
    }

    fn tick_length() -> Duration {
        LoopConfig::new(TICK_HZ).tick_length()
    }

    fn start_session(world: &str) -> Session {
        let temp = tempfile::tempdir().expect("temp dir");
        let saves = temp.path().join("saves");
        let mut app = App::headless();
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
                load_radius_chunks: 2,
                unload_radius_chunks: 3,
                chunks_per_frame: 25,
            },
            world,
            saves.clone(),
        );
        let grid = app
            .light_system
            .as_mut()
            .expect("App starts with a light system")
            .grid_mut();
        streamer.load_columns_now(grid, (0, 0), 2);
        let surface = grid.get_height(8, 8).expect("terrain under the player");
        app.chunk_streamer = Some(streamer);
        app.game_state = GameState::Playing;
        app.simulation
            .set_camera_mode(flagstone_game::controller::CameraMode::FirstPerson);
        app.pawn.select_slot(flagstone_game::pawn::TOOL_SLOT);
        let start = glam::Vec3::new(8.5, surface as f32 + 3.0, 8.5);
        app.simulation.set_position_yaw_pitch(start, 0.0, 0.0);
        let (mine_tx, rx) = std::sync::mpsc::channel();
        app.input.unconsumed_rx = Some(rx);
        let mined = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&mined);
        app.event_bus
            .subscribe(move |event: &flagstone_voxel::WorldEvent| {
                if let flagstone_voxel::WorldEvent::BlockRemoved { position, .. } = event {
                    sink.lock().expect("mined sink").push(*position);
                }
            });
        Session {
            app,
            saves,
            start,
            mined,
            mine_tx,
            _temp: temp,
            _stray: StrayChunkFiles(world.to_string()),
        }
    }

    /// Mines the block under the player and returns where it was.
    fn mine(session: &mut Session) -> BlockPos {
        let before = session.mined.lock().expect("mined sink").len();
        let eye = session.app.simulation.position();
        let looking_down = glam::Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2);
        session.app.camera.0 = glam::Mat4::from_rotation_translation(looking_down, eye).inverse();
        session
            .mine_tx
            .send(InputEvent::MineRequested)
            .expect("input channel open");

        EventProcessor::new().process_input_events(&mut session.app);

        let mined = session.mined.lock().expect("mined sink");
        assert_eq!(mined.len(), before + 1, "one block was mined");
        let position = mined[before];
        let grid = session.app.light_system.as_ref().expect("light").grid();
        assert!(!grid.is_solid_at(position), "the block left the grid");
        position
    }

    fn chunk_of(block: BlockPos) -> glam::IVec3 {
        glam::IVec3::new(
            block.x.div_euclid(16),
            block.y.div_euclid(16),
            block.z.div_euclid(16),
        )
    }

    fn is_loaded(app: &App, chunk: glam::IVec3) -> bool {
        app.light_system
            .as_ref()
            .expect("light system")
            .grid()
            .has_chunk(chunk)
    }

    fn walk_to(app: &mut App, position: glam::Vec3) {
        app.simulation.set_position_yaw_pitch(position, 0.0, 0.0);
        for _ in 0..10 {
            FrameProcessor::new().update_chunk_streaming(app);
        }
    }

    fn close_window(app: &mut App) {
        let clock = GameClock::default();
        let mut ctx = EventContext::new(&clock, None, None);

        Game::event(app, &mut ctx, Event::Window(WindowEvent::CloseRequested));

        assert!(ctx.exit_requested());
    }

    fn exit_from_the_menu(app: &mut App) {
        app.game_state = GameState::Paused;
        app.event_bus.publish(UiEvent::ExitRequested);
        app.event_bus.process_deferred();
        EventProcessor::new().process_ui_events(app, &mut GameClock::default());
        let clock = GameClock::default();
        let mut ctx = FrameContext::new(tick_length(), &clock, None, None);

        Game::frame(app, &mut ctx, 0.5);

        assert!(ctx.exit_requested());
    }

    fn return_to_the_main_menu(app: &mut App) {
        app.game_state = GameState::Paused;
        app.main_menu_requested = true;
        let clock = GameClock::default();
        let mut ctx = FrameContext::new(tick_length(), &clock, None, None);

        Game::frame(app, &mut ctx, 0.5);

        assert!(!ctx.exit_requested());
    }

    /// Loads the save into a fresh app, walks the player back to the start and
    /// reports whether the mined block is missing from the world it streams in.
    fn mined_block_is_absent_after_loading(session: &Session, mined: BlockPos) -> bool {
        let mut loaded = App::headless();
        loaded.saves_dir = session.saves.clone();
        loaded
            .load_scene(&mut GameClock::default(), session.saves.join("scene.bin"))
            .expect("the save loads");
        loaded
            .simulation
            .set_position_yaw_pitch(session.start, 0.0, 0.0);
        for _ in 0..10 {
            FrameProcessor::new().update_chunk_streaming(&mut loaded);
        }

        assert!(
            is_loaded(&loaded, chunk_of(mined)),
            "the mined block's chunk is back, so its absence means something"
        );
        let grid = loaded.light_system.as_ref().expect("light system").grid();
        !grid.is_solid_at(mined)
    }

    fn assert_mined_block_survives_the_session(world: &str, movement: Movement, exit: Exit) {
        let mut session = start_session(world);
        let mined = mine(&mut session);
        let far = session.start + glam::Vec3::new(16.0 * 10.0, 0.0, 0.0);
        match movement {
            Movement::StaysNear => {}
            Movement::WalksOutOfRangeAndBack => {
                walk_to(&mut session.app, far);
                assert!(!is_loaded(&session.app, chunk_of(mined)), "the chunk left");
                walk_to(&mut session.app, session.start);
            }
            Movement::StaysOutOfRange => {
                walk_to(&mut session.app, far);
                assert!(!is_loaded(&session.app, chunk_of(mined)), "the chunk left");
            }
        }

        match exit {
            Exit::ClosingTheWindow => close_window(&mut session.app),
            Exit::ExitInTheMenu => exit_from_the_menu(&mut session.app),
        }

        assert!(mined_block_is_absent_after_loading(&session, mined));
    }

    #[test]
    fn mined_block_is_absent_after_staying_near_then_closing() {
        assert_mined_block_survives_the_session(
            "autosave_unit_test_near_closing",
            Movement::StaysNear,
            Exit::ClosingTheWindow,
        );
    }

    #[test]
    fn mined_block_is_absent_after_staying_near_then_menu_exit() {
        assert_mined_block_survives_the_session(
            "autosave_unit_test_near_menu_exit",
            Movement::StaysNear,
            Exit::ExitInTheMenu,
        );
    }

    #[test]
    fn mined_block_is_absent_after_walking_back_then_closing() {
        assert_mined_block_survives_the_session(
            "autosave_unit_test_walking_back_closing",
            Movement::WalksOutOfRangeAndBack,
            Exit::ClosingTheWindow,
        );
    }

    #[test]
    fn mined_block_is_absent_after_staying_away_then_closing() {
        assert_mined_block_survives_the_session(
            "autosave_unit_test_staying_away_closing",
            Movement::StaysOutOfRange,
            Exit::ClosingTheWindow,
        );
    }

    /// A previous save in `saves` that an app with no world must leave alone.
    fn write_previous_save(saves: &Path) -> Vec<u8> {
        std::fs::create_dir_all(saves).expect("saves dir");
        let previous = b"the previous world".to_vec();
        std::fs::write(saves.join("scene.bin"), &previous).expect("previous save");
        previous
    }

    fn app_without_a_world(saves: &Path) -> App {
        let mut app = App::headless();
        app.saves_dir = saves.to_path_buf();
        app
    }

    #[test]
    fn closing_with_no_world_keeps_the_previous_save() {
        let temp = tempfile::tempdir().expect("temp dir");
        let saves = temp.path().join("saves");
        let previous = write_previous_save(&saves);
        let mut app = app_without_a_world(&saves);

        close_window(&mut app);

        assert_eq!(std::fs::read(saves.join("scene.bin")).unwrap(), previous);
    }

    #[test]
    fn menu_exit_with_no_world_keeps_the_previous_save() {
        let temp = tempfile::tempdir().expect("temp dir");
        let saves = temp.path().join("saves");
        let previous = write_previous_save(&saves);
        let mut app = app_without_a_world(&saves);

        exit_from_the_menu(&mut app);

        assert_eq!(std::fs::read(saves.join("scene.bin")).unwrap(), previous);
    }

    #[test]
    fn returning_to_the_main_menu_with_no_world_keeps_the_previous_save() {
        let temp = tempfile::tempdir().expect("temp dir");
        let saves = temp.path().join("saves");
        let previous = write_previous_save(&saves);
        let mut app = app_without_a_world(&saves);

        return_to_the_main_menu(&mut app);

        assert_eq!(std::fs::read(saves.join("scene.bin")).unwrap(), previous);
    }

    fn autosave_every(app: &mut App, interval: Duration) {
        app.autosave_interval = interval;
        app.autosave = AutosaveSchedule::for_interval(interval);
    }

    fn play(sim: &mut HeadlessLoop<App>, app: &mut App, ticks: u32) {
        let mut run = 0;
        while run < ticks {
            run += sim.advance(app, tick_length());
        }
        assert_eq!(run, ticks, "the loop ran exactly the requested ticks");
    }

    /// Whether the latest save at `saves` holds no block at `pos`; the save must hold blocks.
    fn latest_save_lacks(saves: &Path, pos: BlockPos) -> bool {
        let (_, _, blocks) = crate::save::read_scene_and_metadata(saves.join("scene.bin"))
            .expect("an autosave was written");
        assert!(!blocks.is_empty(), "the save holds the world's blocks");
        !blocks
            .iter()
            .any(|b| (b.x, b.y, b.z) == (pos.x, pos.y, pos.z))
    }

    #[test]
    fn killed_session_keeps_every_edit_before_the_last_autosave() {
        let mut session = start_session("autosave_unit_test_killed_session");
        autosave_every(&mut session.app, Duration::from_secs(5));
        let mut sim = HeadlessLoop::<App>::new(LoopConfig::new(TICK_HZ));
        let first = mine(&mut session);
        play(&mut sim, &mut session.app, 7 * 60);
        let second = mine(&mut session);
        play(&mut sim, &mut session.app, 4 * 60);
        let in_flight = mine(&mut session);
        play(&mut sim, &mut session.app, 60);

        let saves = session.saves.clone();
        drop(session.app);

        assert!(latest_save_lacks(&saves, first), "first edit is saved");
        assert!(latest_save_lacks(&saves, second), "second edit is saved");
        assert!(
            !latest_save_lacks(&saves, in_flight),
            "an edit one second old waits for the next interval"
        );
    }

    #[test]
    fn interval_of_30_seconds_saves_the_edit_after_30_seconds() {
        let mut session = start_session("autosave_unit_test_thirty_seconds");
        autosave_every(&mut session.app, Duration::from_secs(30));
        let mut sim = HeadlessLoop::<App>::new(LoopConfig::new(TICK_HZ));
        let mined = mine(&mut session);

        play(&mut sim, &mut session.app, 29 * 60 + 54);
        let saved_early = session.saves.join("scene.bin").exists();
        play(&mut sim, &mut session.app, 6);

        assert!(!saved_early, "nothing is saved at 29.9 s");
        assert!(latest_save_lacks(&session.saves, mined));
    }

    #[test]
    fn default_autosave_interval_is_two_minutes() {
        let mut app = App::headless();

        app.autosave.mark_edited();
        for _ in 0..(120 * 60 - 1) {
            app.autosave.tick();
        }
        let before = app.autosave.due();
        app.autosave.tick();

        assert_eq!(app.autosave_interval, Duration::from_secs(120));
        assert!(!before, "not due one tick early");
        assert!(app.autosave.due(), "due after two minutes of ticks");
    }

    #[test]
    fn schedule_is_due_only_after_the_interval_and_an_edit() {
        let mut edited = AutosaveSchedule::new(3);
        let mut untouched = AutosaveSchedule::new(3);
        edited.mark_edited();

        edited.tick();
        edited.tick();
        let two_ticks = edited.due();
        edited.tick();
        for _ in 0..10 {
            untouched.tick();
        }

        assert!(!two_ticks, "an edit alone is not enough");
        assert!(edited.due(), "an edit and a full interval");
        assert!(!untouched.due(), "an interval alone is not enough");
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
        let right_after = schedule.due();
        schedule.mark_edited();
        schedule.tick();
        schedule.tick();
        let two_ticks_later = schedule.due();
        schedule.tick();

        assert!(!right_after, "the save cleared the edit");
        assert!(!two_ticks_later, "the save restarted the interval");
        assert!(schedule.due());
    }

    #[test]
    fn schedule_failed_save_retries_next_interval() {
        let mut schedule = AutosaveSchedule::new(3);
        schedule.mark_edited();
        for _ in 0..3 {
            schedule.tick();
        }
        assert!(schedule.due());

        schedule.save_failed();
        let right_after = schedule.due();
        schedule.tick();
        schedule.tick();
        let two_ticks_later = schedule.due();
        schedule.tick();

        assert!(!right_after, "no retry on the same tick");
        assert!(!two_ticks_later, "no retry before a full interval");
        assert!(schedule.due(), "the edit is still waiting");
    }

    #[test]
    fn for_interval_counts_the_interval_in_simulation_ticks() {
        let ticks_until_due = |interval: Duration| {
            let mut schedule = AutosaveSchedule::for_interval(interval);
            schedule.mark_edited();
            let mut ticks = 0;
            while !schedule.due() {
                schedule.tick();
                ticks += 1;
                assert!(ticks <= 10 * 60 * TICK_HZ, "the schedule never came due");
            }
            ticks
        };

        assert_eq!(ticks_until_due(Duration::from_secs(30)), 30 * TICK_HZ);
        assert_eq!(ticks_until_due(Duration::from_millis(500)), TICK_HZ / 2);
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
