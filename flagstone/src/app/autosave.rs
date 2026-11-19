/// Save the current scene to `saves_dir/scene.bin`, creating the directory
/// if needed.
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

/// When an edited world is next due a save, counted in simulation ticks.
///
/// Due once the world has been edited and `interval_ticks` ticks have passed since the last
/// save; an unedited world is never due.
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

    /// One simulation tick of play has passed.
    pub fn tick(&mut self) {
        self.ticks_since_save = self.ticks_since_save.saturating_add(1);
    }

    /// The world changed since the last save.
    pub fn mark_edited(&mut self) {
        self.edited = true;
    }

    pub fn due(&self) -> bool {
        self.edited && self.ticks_since_save >= self.interval_ticks
    }

    /// A save succeeded: the world is clean and the interval starts over.
    pub fn saved(&mut self) {
        self.edited = false;
        self.ticks_since_save = 0;
    }

    /// A save failed: the interval starts over but the world stays edited, so the save
    /// is retried next interval and not on every frame.
    pub fn save_failed(&mut self) {
        self.ticks_since_save = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::App;
    use crate::app::chunk_streamer::ChunkStreamer;
    use crate::app::event_loop::EventProcessor;
    use crate::game_state::GameState;
    use crate::input_event::InputEvent;
    use flagstone_game::TICK_HZ;
    use flagstone_game::controller::CameraMode;
    use flagstone_game::scene_builders::{TerrainConfig, WorldSpec};
    use flagstone_voxel::{BlockPos, StreamingConfig};
    use moho_app::{EventContext, FrameContext, Game, GameClock, HeadlessLoop, LoopConfig};
    use std::path::Path;
    use std::time::Duration;
    use winit::event::WindowEvent;

    const WORLD_SIZE: u32 = 256;
    /// Z of a column far outside the unload radius of the spawn column.
    const AWAY_Z: f32 = 8.5 + 16.0 * 6.0;

    fn spec(world: &str) -> WorldSpec {
        WorldSpec {
            name: world.to_string(),
            size_xz: WORLD_SIZE,
            ..WorldSpec::default()
        }
    }

    /// A world name no earlier run used, so chunk files an earlier run left in the working
    /// directory are never read back.
    fn unique(world: &str) -> String {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("the clock is after the epoch")
            .as_nanos();
        format!("{world}_{nanos}")
    }

    fn tick_length() -> Duration {
        LoopConfig::new(TICK_HZ).tick_length()
    }

    /// A streamer over a small area, so walking a few chunks evicts the spawn area.
    fn streamer(world: &str, saves: &Path) -> ChunkStreamer {
        ChunkStreamer::new(
            TerrainConfig {
                world_size: WORLD_SIZE,
                ..TerrainConfig::default()
            },
            StreamingConfig {
                load_radius_chunks: 1,
                unload_radius_chunks: 1,
                chunks_per_frame: 16,
            },
            world,
            saves.to_path_buf(),
        )
    }

    /// Loads the save in `saves` into a fresh app that saves, and streams chunk files, there.
    fn open_world(saves: &Path, world: &str) -> App {
        let mut app = App::headless();
        app.saves_dir = saves.to_path_buf();
        crate::app::scene_loader::load_scene(
            &mut app,
            &mut GameClock::default(),
            &saves.join("scene.bin"),
        )
        .expect("the save loads");
        app.chunk_streamer = Some(streamer(world, saves));
        app
    }

    /// A world with terrain around block column (8, 8), saved once with nothing mined, then
    /// loaded: a loaded world in play.
    fn new_world(saves: &Path, world: &str) -> App {
        let mut seed = App::headless();
        seed.generation.last_spec = Some(spec(world));
        seed.simulation
            .set_position_yaw_pitch(glam::Vec3::new(8.5, 200.0, 8.5), 0.0, 0.0);
        auto_save_on_shutdown(&mut seed, &GameClock::default(), saves, &[]).expect("seed save");
        open_world(saves, world)
    }

    /// Mines the surface block of column (`x`, `z`) the way the player does, from above.
    fn mine_surface(app: &mut App, x: i32, z: i32) -> BlockPos {
        let grid = app.light_system.as_ref().expect("light system").grid();
        let top = grid.get_height(x, z).expect("terrain under the column");
        let block = BlockPos::new(x, top, z);
        assert!(
            grid.is_solid_at(block),
            "the block exists before it is mined"
        );
        app.simulation.set_camera_mode(CameraMode::FirstPerson);
        app.pawn.select_slot(flagstone_game::pawn::TOOL_SLOT);
        let eye = glam::Vec3::new(x as f32 + 0.5, top as f32 + 2.5, z as f32 + 0.5);
        app.camera.0 =
            glam::camera::rh::view::look_at_mat4(eye, eye + glam::Vec3::NEG_Y, glam::Vec3::Z);
        let (tx, rx) = std::sync::mpsc::channel();
        app.input.unconsumed_rx = Some(rx);
        tx.send(InputEvent::MineRequested).expect("channel open");

        EventProcessor::new().process_input_events(app);

        let grid = app.light_system.as_ref().expect("light system").grid();
        assert!(!grid.is_solid_at(block), "the mine request removed it");
        block
    }

    fn chunk_of(block: BlockPos) -> glam::IVec3 {
        glam::IVec3::new(
            block.x.div_euclid(16),
            block.y.div_euclid(16),
            block.z.div_euclid(16),
        )
    }

    fn has_chunk_of(app: &App, block: BlockPos) -> bool {
        app.light_system
            .as_ref()
            .expect("light system")
            .grid()
            .has_chunk(chunk_of(block))
    }

    /// Runs `ticks` ticks, drawing a frame after each as the loop does.
    fn play(app: &mut App, sim: &mut HeadlessLoop<App>, ticks: u32) {
        for _ in 0..ticks {
            sim.step(app, 1);
            let mut ctx = FrameContext::new(tick_length(), sim.clock(), None, None);
            Game::frame(app, &mut ctx, 0.0);
        }
    }

    /// Moves the player over (`x`, `z`) in the strategy view, where no collision pulls it
    /// back, and ticks so streaming catches up.
    fn walk_to(app: &mut App, sim: &mut HeadlessLoop<App>, x: f32, z: f32) {
        app.simulation.set_camera_mode(CameraMode::Isometric);
        app.simulation
            .set_position_yaw_pitch(glam::Vec3::new(x, 90.0, z), 0.0, 0.0);

        sim.step(app, 3);

        let at = app.simulation.position();
        assert!(
            (at.x - x).abs() < 1.0 && (at.z - z).abs() < 1.0,
            "the player stayed where it was put: {at:?}"
        );
    }

    fn new_loop() -> HeadlessLoop<App> {
        HeadlessLoop::<App>::new(LoopConfig::new(TICK_HZ))
    }

    fn close_window(app: &mut App) {
        let clock = GameClock::default();
        let mut ctx = EventContext::new(&clock, None, None);

        Game::event(
            app,
            &mut ctx,
            moho_app::Event::Window(WindowEvent::CloseRequested),
        );

        assert!(ctx.exit_requested(), "closing the window leaves the loop");
    }

    /// Exit chosen in the pause menu; the next frame carries it out.
    fn exit_from_the_menu(app: &mut App) {
        app.exit_requested = true;
        let clock = GameClock::default();
        let mut ctx = FrameContext::new(tick_length(), &clock, None, None);

        Game::frame(app, &mut ctx, 0.0);

        assert!(ctx.exit_requested(), "Exit leaves the loop");
    }

    /// Loads the latest save, walks to the block and checks it is gone.
    fn assert_absent_after_loading(saves: &Path, world: &str, block: BlockPos) {
        let mut app = open_world(saves, world);
        let mut sim = new_loop();

        walk_to(
            &mut app,
            &mut sim,
            block.x as f32 + 0.5,
            block.z as f32 + 0.5,
        );

        assert!(
            has_chunk_of(&app, block),
            "the block's chunk is loaded, so its absence is real"
        );
        let grid = app.light_system.as_ref().expect("light system").grid();
        assert!(!grid.is_solid_at(block), "the mined block came back");
    }

    /// A save holding one block, as a player's earlier session left it; returns its bytes.
    fn previous_save(saves: &Path) -> Vec<u8> {
        let mut previous = App::headless();
        previous.generation.last_spec = Some(spec("autosave_unit_previous_world"));
        previous
            .light_system
            .as_mut()
            .expect("App starts with a light system")
            .grid_mut()
            .mutator()
            .place(BlockPos::new(3, 70, 3), 1, None);
        auto_save_on_shutdown(&mut previous, &GameClock::default(), saves, &[])
            .expect("previous save");
        std::fs::read(saves.join("scene.bin")).expect("previous save is readable")
    }

    #[test]
    fn mined_block_is_absent_after_staying_near_then_closing() {
        let temp = tempfile::tempdir().expect("temp dir");
        let saves = temp.path().join("saves");
        let world = &unique("autosave_unit_near_close");
        let mut app = new_world(&saves, world);
        let mut sim = new_loop();
        let block = mine_surface(&mut app, 8, 8);
        play(&mut app, &mut sim, 1);

        close_window(&mut app);

        assert_absent_after_loading(&saves, world, block);
    }

    #[test]
    fn mined_block_is_absent_after_staying_near_then_menu_exit() {
        let temp = tempfile::tempdir().expect("temp dir");
        let saves = temp.path().join("saves");
        let world = &unique("autosave_unit_near_menu_exit");
        let mut app = new_world(&saves, world);
        let mut sim = new_loop();
        let block = mine_surface(&mut app, 8, 8);
        play(&mut app, &mut sim, 1);
        app.game_state = GameState::Paused;

        exit_from_the_menu(&mut app);

        assert_absent_after_loading(&saves, world, block);
    }

    #[test]
    fn mined_block_is_absent_after_walking_back_then_closing() {
        let temp = tempfile::tempdir().expect("temp dir");
        let saves = temp.path().join("saves");
        let world = &unique("autosave_unit_walk_back_close");
        let mut app = new_world(&saves, world);
        let mut sim = new_loop();
        let block = mine_surface(&mut app, 8, 8);
        play(&mut app, &mut sim, 1);
        walk_to(&mut app, &mut sim, 8.5, AWAY_Z);
        assert!(
            !has_chunk_of(&app, block),
            "out of range, the chunk is evicted"
        );
        walk_to(&mut app, &mut sim, 8.5, 8.5);
        assert!(
            has_chunk_of(&app, block),
            "back in range, the chunk is loaded again"
        );

        close_window(&mut app);

        assert_absent_after_loading(&saves, world, block);
    }

    #[test]
    fn mined_block_is_absent_after_staying_away_then_closing() {
        let temp = tempfile::tempdir().expect("temp dir");
        let saves = temp.path().join("saves");
        let world = &unique("autosave_unit_stay_away_close");
        let mut app = new_world(&saves, world);
        let mut sim = new_loop();
        let block = mine_surface(&mut app, 8, 8);
        play(&mut app, &mut sim, 1);
        walk_to(&mut app, &mut sim, 8.5, AWAY_Z);
        assert!(
            !has_chunk_of(&app, block),
            "out of range, the chunk is evicted"
        );

        close_window(&mut app);

        assert_absent_after_loading(&saves, world, block);
    }

    #[test]
    fn closing_with_no_world_keeps_the_previous_save() {
        let temp = tempfile::tempdir().expect("temp dir");
        let saves = temp.path().join("saves");
        let before = previous_save(&saves);
        let mut app = App::headless();
        app.saves_dir = saves.clone();

        close_window(&mut app);

        let after = std::fs::read(saves.join("scene.bin")).expect("the save is still there");
        assert!(
            after == before,
            "closing from the start menu rewrote the save"
        );
    }

    #[test]
    fn menu_exit_with_no_world_keeps_the_previous_save() {
        let temp = tempfile::tempdir().expect("temp dir");
        let saves = temp.path().join("saves");
        let before = previous_save(&saves);
        let mut app = App::headless();
        app.saves_dir = saves.clone();

        exit_from_the_menu(&mut app);

        let after = std::fs::read(saves.join("scene.bin")).expect("the save is still there");
        assert!(after == before, "Exit from the start menu rewrote the save");
    }

    #[test]
    fn returning_to_the_main_menu_with_no_world_keeps_the_previous_save() {
        let temp = tempfile::tempdir().expect("temp dir");
        let saves = temp.path().join("saves");
        let before = previous_save(&saves);
        let mut app = App::headless();
        app.saves_dir = saves.clone();
        app.main_menu_requested = true;
        let clock = GameClock::default();
        let mut ctx = FrameContext::new(tick_length(), &clock, None, None);

        Game::frame(&mut app, &mut ctx, 0.0);

        let after = std::fs::read(saves.join("scene.bin")).expect("the save is still there");
        assert!(after == before, "returning to the menu rewrote the save");
    }

    #[test]
    fn killed_session_keeps_every_edit_before_the_last_autosave() {
        let temp = tempfile::tempdir().expect("temp dir");
        let saves = temp.path().join("saves");
        let world = &unique("autosave_unit_killed");
        let mut app = new_world(&saves, world);
        app.autosave_interval = Duration::from_secs(10);
        let mut sim = new_loop();
        let first = mine_surface(&mut app, 8, 8);
        play(&mut app, &mut sim, 11 * TICK_HZ);
        let second = mine_surface(&mut app, 10, 10);
        play(&mut app, &mut sim, 11 * TICK_HZ);
        mine_surface(&mut app, 12, 12);
        play(&mut app, &mut sim, 5 * TICK_HZ);

        drop(app);

        assert_absent_after_loading(&saves, world, first);
        assert_absent_after_loading(&saves, world, second);
    }

    #[test]
    fn interval_of_30_seconds_saves_the_edit_after_30_seconds() {
        let temp = tempfile::tempdir().expect("temp dir");
        let saves = temp.path().join("saves");
        let world = &unique("autosave_unit_interval_30s");
        let mut app = new_world(&saves, world);
        app.autosave_interval = Duration::from_secs(30);
        let unmined = std::fs::read(saves.join("scene.bin")).expect("seed save is readable");
        let mut sim = new_loop();
        let block = mine_surface(&mut app, 8, 8);

        play(&mut app, &mut sim, 1794);
        let at_29_9_seconds = std::fs::read(saves.join("scene.bin")).expect("save is readable");
        play(&mut app, &mut sim, 6);
        let at_30_seconds = std::fs::read(saves.join("scene.bin")).expect("save is readable");

        assert!(
            at_29_9_seconds == unmined,
            "saved before the interval ended"
        );
        assert!(at_30_seconds != unmined, "nothing saved at 30 seconds");
        assert_absent_after_loading(&saves, world, block);
    }

    #[test]
    fn schedule_is_due_only_after_the_interval_and_an_edit() {
        let mut schedule = AutosaveSchedule::new(3);
        for _ in 0..10 {
            schedule.tick();
        }
        assert!(!schedule.due(), "an unedited world is never due");

        let mut schedule = AutosaveSchedule::new(3);
        schedule.mark_edited();
        schedule.tick();
        schedule.tick();
        assert!(!schedule.due(), "one tick short of the interval");
        schedule.tick();
        assert!(schedule.due(), "edited and a full interval passed");

        let mut late_edit = AutosaveSchedule::new(3);
        for _ in 0..5 {
            late_edit.tick();
        }
        late_edit.mark_edited();
        assert!(late_edit.due(), "an edit after the interval is due at once");
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
        assert!(!schedule.due(), "a saved world is not due");
        for _ in 0..10 {
            schedule.tick();
        }
        assert!(!schedule.due(), "the save cleared the edit");
        schedule.mark_edited();
        assert!(schedule.due(), "the interval passed since the save");

        schedule.saved();
        schedule.mark_edited();
        schedule.tick();
        schedule.tick();
        assert!(!schedule.due(), "the interval starts over at the save");
        schedule.tick();
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
        assert!(!schedule.due(), "no retry on the very next frame");
        schedule.tick();
        schedule.tick();
        assert!(!schedule.due(), "one tick short of the next interval");
        schedule.tick();

        assert!(
            schedule.due(),
            "the edit is still unsaved, so the retry is due"
        );
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
