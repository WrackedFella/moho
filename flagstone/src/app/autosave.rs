/// When the world next needs an autosave: `interval_ticks` simulation ticks after the last
/// save, and only if the world was edited since.
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

    /// One simulation tick has passed.
    pub fn tick(&mut self) {}

    /// A block was placed, removed or modified.
    pub fn mark_edited(&mut self) {}

    /// Whether an edited world has waited a full interval since the last save attempt.
    pub fn due(&self) -> bool {
        false
    }

    /// The world was written: nothing is pending and the interval restarts.
    pub fn saved(&mut self) {}

    /// The write failed: the interval restarts but the edits stay pending.
    pub fn save_failed(&mut self) {}
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
    use crate::app::event_loop::FrameProcessor;
    use crate::app::scene_loader::load_scene;
    use crate::game_state::GameState;
    use flagstone_game::TICK_HZ;
    use flagstone_game::scene_builders::{TerrainConfig, WorldSpec};
    use flagstone_voxel::{BlockChangeReason, BlockPos, StreamingConfig, WorldEvent};
    use moho_app::{Event, EventContext, Game, GameClock, HeadlessLoop, LoopConfig};
    use moho_core::events::UiEvent;
    use std::path::Path;
    use std::time::Duration;
    use winit::event::WindowEvent;

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

    #[test]
    fn schedule_is_due_only_after_the_interval_and_an_edit() {
        let mut unedited = AutosaveSchedule::new(3);
        for _ in 0..10 {
            unedited.tick();
        }
        let mut edited = AutosaveSchedule::new(3);
        edited.mark_edited();
        edited.tick();
        edited.tick();
        let one_tick_short = edited.due();
        edited.tick();
        let mut edited_late = AutosaveSchedule::new(3);
        for _ in 0..5 {
            edited_late.tick();
        }
        let before_the_edit = edited_late.due();
        edited_late.mark_edited();

        assert!(!unedited.due(), "an idle world is never due");
        assert!(!one_tick_short, "due one tick early");
        assert!(edited.due(), "due once a full interval has passed");
        assert!(!before_the_edit);
        assert!(
            edited_late.due(),
            "an edit after the interval is due at once"
        );
    }

    #[test]
    fn schedule_resets_on_save() {
        let mut schedule = AutosaveSchedule::new(3);
        schedule.mark_edited();
        for _ in 0..3 {
            schedule.tick();
        }
        let due_before_save = schedule.due();

        schedule.saved();
        let due_after_save = schedule.due();
        for _ in 0..3 {
            schedule.tick();
        }
        let due_without_new_edit = schedule.due();
        schedule.mark_edited();
        let due_with_new_edit = schedule.due();

        assert!(due_before_save, "due before the save");
        assert!(!due_after_save, "a save clears the pending edit");
        assert!(
            !due_without_new_edit,
            "an unedited world stays clean after a save"
        );
        assert!(due_with_new_edit, "an edit after the save is due again");
    }

    #[test]
    fn schedule_restarts_the_interval_on_save() {
        let mut schedule = AutosaveSchedule::new(3);
        schedule.mark_edited();
        for _ in 0..3 {
            schedule.tick();
        }
        schedule.saved();
        schedule.mark_edited();
        schedule.tick();
        schedule.tick();
        let one_tick_short = schedule.due();
        schedule.tick();

        assert!(!one_tick_short, "the interval counts from the save");
        assert!(schedule.due());
    }

    #[test]
    fn schedule_failed_save_retries_next_interval() {
        let mut schedule = AutosaveSchedule::new(3);
        schedule.mark_edited();
        for _ in 0..3 {
            schedule.tick();
        }
        let due_before_save = schedule.due();

        schedule.save_failed();
        let due_after_failure = schedule.due();
        schedule.tick();
        schedule.tick();
        let one_tick_short = schedule.due();
        schedule.tick();

        assert!(due_before_save, "due before the failed save");
        assert!(
            !due_after_failure,
            "a failed save is not retried on the next frame"
        );
        assert!(!one_tick_short, "the retry waits a full interval");
        assert!(
            schedule.due(),
            "the edit is still pending, so the retry is due"
        );
    }

    #[test]
    fn new_app_autosaves_every_two_minutes() {
        let mut app = App::headless();
        app.autosave.mark_edited();
        for _ in 0..120 * flagstone_game::TICK_HZ - 1 {
            app.autosave.tick();
        }
        let one_tick_short = app.autosave.due();
        app.autosave.tick();

        assert!(!one_tick_short, "due before two minutes");
        assert!(app.autosave.due(), "not due after two minutes");
    }

    /// Terrain column (2, 2): where the player starts and mines.
    const NEAR: glam::Vec3 = glam::Vec3::new(40.5, 100.0, 40.5);
    /// Terrain column (12, 12): far enough that every column near `NEAR` is unloaded.
    const FAR: glam::Vec3 = glam::Vec3::new(200.5, 100.0, 200.5);

    struct Session {
        app: App,
        sim: HeadlessLoop<App>,
    }

    /// A playing app in a streamed 256-block world named `world`, saving under `saves`.
    fn session(saves: &Path, world: &str) -> Session {
        let mut app = App::headless();
        app.saves_dir = saves.to_path_buf();
        app.generation.last_spec = Some(WorldSpec {
            name: world.to_string(),
            size_xz: 256,
            ..WorldSpec::default()
        });
        app.chunk_streamer = Some(ChunkStreamer::new(
            TerrainConfig {
                world_size: 256,
                ..TerrainConfig::default()
            },
            StreamingConfig {
                load_radius_chunks: 1,
                unload_radius_chunks: 1,
                chunks_per_frame: 9,
            },
            world,
            saves,
        ));
        app.game_state = GameState::Playing;
        app.simulation.set_position_yaw_pitch(NEAR, 0.0, 0.0);
        FrameProcessor::new().update_chunk_streaming(&mut app);
        Session {
            app,
            sim: HeadlessLoop::new(LoopConfig::new(TICK_HZ)),
        }
    }

    /// Runs `ticks` ticks, each followed by a frame.
    fn play(session: &mut Session, ticks: u32) {
        let frame = Duration::from_nanos(1_000_000_000u64.div_ceil(u64::from(TICK_HZ)));
        let mut done = 0;
        while done < ticks {
            done += session.sim.advance(&mut session.app, frame);
        }
    }

    fn walk_to(session: &mut Session, position: glam::Vec3) {
        session
            .app
            .simulation
            .set_position_yaw_pitch(position, 0.0, 0.0);
        play(session, 2);
    }

    fn grid(app: &App) -> &flagstone_voxel::VoxelGrid {
        app.light_system.as_ref().expect("light system").grid()
    }

    fn surface_block(app: &App, x: i32, z: i32) -> BlockPos {
        let height = grid(app).get_height(x, z).expect("the column has terrain");
        BlockPos::new(x, height, z)
    }

    /// Removes a block the way mining does: out of the grid, then announced.
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

    fn close_window(app: &mut App) {
        let clock = GameClock::default();
        let mut ctx = EventContext::new(&clock, None, None);

        Game::event(app, &mut ctx, Event::Window(WindowEvent::CloseRequested));

        assert!(ctx.exit_requested());
    }

    fn frame_length() -> Duration {
        LoopConfig::new(TICK_HZ).tick_length()
    }

    /// Chooses Exit in the menu and lets the loop handle it.
    fn exit_from_the_menu(app: &mut App) {
        app.event_bus.publish(UiEvent::ExitRequested);
        app.event_bus.process_deferred();
        let mut sim = HeadlessLoop::<App>::new(LoopConfig::new(TICK_HZ));

        sim.step(app, 1);
        assert!(app.exit_requested, "the tick took the exit request");
        sim.advance(app, Duration::ZERO);

        assert!(!app.exit_requested, "the frame handled the exit request");
    }

    /// The next session: a fresh app loading the save in `saves`.
    fn load_save(saves: &Path) -> App {
        let scene = saves.join("scene.bin");
        assert!(scene.is_file(), "a save exists at {}", scene.display());
        let mut app = App::headless();
        app.saves_dir = saves.to_path_buf();
        load_scene(&mut app, &mut GameClock::default(), &scene).expect("the save loads");
        app
    }

    /// Streams in the column of `pos`, as walking to it would, then asserts it is mined
    /// and the block under it is not.
    fn assert_mined(app: &mut App, pos: BlockPos) {
        let column = (pos.x.div_euclid(16), pos.z.div_euclid(16));
        let streamer = app.chunk_streamer.as_ref().expect("a loaded world streams");
        let grid = app.light_system.as_mut().expect("light system").grid_mut();
        streamer.load_columns_now(grid, column, 0);

        assert!(
            !grid.is_solid_at(pos),
            "{pos} is still there after the load"
        );
        assert!(
            grid.is_solid_at(pos - BlockPos::Y),
            "the terrain under {pos} is loaded"
        );
    }

    fn assert_evicted(app: &App, pos: BlockPos) {
        let chunk = glam::IVec3::new(pos.x, pos.y, pos.z).div_euclid(glam::IVec3::splat(16));
        assert!(
            !grid(app).has_chunk(chunk),
            "chunk {chunk} is still loaded away from the block"
        );
    }

    #[test]
    fn mined_block_is_absent_after_staying_near_then_closing() {
        let temp = tempfile::tempdir().expect("temp dir");
        let mut session = session(temp.path(), "autosave_unit_near_close");
        let pos = surface_block(&session.app, 40, 40);
        mine(&mut session.app, pos);
        play(&mut session, 30);

        close_window(&mut session.app);

        assert_mined(&mut load_save(temp.path()), pos);
    }

    #[test]
    fn mined_block_is_absent_after_staying_near_then_menu_exit() {
        let temp = tempfile::tempdir().expect("temp dir");
        let mut session = session(temp.path(), "autosave_unit_near_menu");
        let pos = surface_block(&session.app, 40, 40);
        mine(&mut session.app, pos);
        play(&mut session, 30);
        session.app.escape_pressed();
        assert_eq!(session.app.game_state, GameState::Paused);

        exit_from_the_menu(&mut session.app);

        assert_mined(&mut load_save(temp.path()), pos);
    }

    #[test]
    fn mined_block_is_absent_after_walking_back_then_closing() {
        let temp = tempfile::tempdir().expect("temp dir");
        let mut session = session(temp.path(), "autosave_unit_back_close");
        let pos = surface_block(&session.app, 40, 40);
        mine(&mut session.app, pos);
        play(&mut session, 30);
        walk_to(&mut session, FAR);
        assert_evicted(&session.app, pos);
        walk_to(&mut session, NEAR);

        close_window(&mut session.app);

        assert_mined(&mut load_save(temp.path()), pos);
    }

    #[test]
    fn mined_block_is_absent_after_staying_away_then_closing() {
        let temp = tempfile::tempdir().expect("temp dir");
        let mut session = session(temp.path(), "autosave_unit_away_close");
        let pos = surface_block(&session.app, 40, 40);
        mine(&mut session.app, pos);
        play(&mut session, 30);
        walk_to(&mut session, FAR);
        assert_evicted(&session.app, pos);

        close_window(&mut session.app);

        assert_mined(&mut load_save(temp.path()), pos);
    }

    /// A save of a world with one block mined, and its bytes.
    fn saved_world(saves: &Path, world: &str) -> Vec<u8> {
        let mut session = session(saves, world);
        let pos = surface_block(&session.app, 40, 40);
        mine(&mut session.app, pos);
        play(&mut session, 2);
        auto_save_on_shutdown(&mut session.app, &GameClock::default(), saves, &[])
            .expect("autosave");
        std::fs::read(saves.join("scene.bin")).expect("the save was written")
    }

    fn menu_app(saves: &Path) -> App {
        let mut app = App::headless();
        app.saves_dir = saves.to_path_buf();
        assert_eq!(app.game_state, GameState::Menu);
        app
    }

    #[test]
    fn closing_with_no_world_keeps_the_previous_save() {
        let temp = tempfile::tempdir().expect("temp dir");
        let previous = saved_world(temp.path(), "autosave_unit_keep_close");
        let mut app = menu_app(temp.path());

        close_window(&mut app);

        let after = std::fs::read(temp.path().join("scene.bin")).expect("the save is there");
        assert!(after == previous, "closing from the menu rewrote the save");
    }

    #[test]
    fn menu_exit_with_no_world_keeps_the_previous_save() {
        let temp = tempfile::tempdir().expect("temp dir");
        let previous = saved_world(temp.path(), "autosave_unit_keep_exit");
        let mut app = menu_app(temp.path());

        exit_from_the_menu(&mut app);

        let after = std::fs::read(temp.path().join("scene.bin")).expect("the save is there");
        assert!(after == previous, "Exit from the menu rewrote the save");
    }

    #[test]
    fn main_menu_with_no_world_keeps_the_previous_save() {
        let temp = tempfile::tempdir().expect("temp dir");
        let previous = saved_world(temp.path(), "autosave_unit_keep_main_menu");
        let mut app = menu_app(temp.path());
        app.main_menu_requested = true;
        let mut sim = HeadlessLoop::<App>::new(LoopConfig::new(TICK_HZ));

        sim.advance(&mut app, frame_length());

        let after = std::fs::read(temp.path().join("scene.bin")).expect("the save is there");
        assert!(
            after == previous,
            "Main menu from the menu rewrote the save"
        );
    }

    #[test]
    fn killed_session_keeps_every_edit_before_the_last_autosave() {
        const INTERVAL: u32 = 600;
        let temp = tempfile::tempdir().expect("temp dir");
        let mut session = session(temp.path(), "autosave_unit_killed");
        session.app.autosave = AutosaveSchedule::new(INTERVAL);
        let first = surface_block(&session.app, 40, 40);
        let second = surface_block(&session.app, 41, 40);
        let third = surface_block(&session.app, 42, 40);

        mine(&mut session.app, first);
        play(&mut session, INTERVAL + 5);
        mine(&mut session.app, second);
        play(&mut session, INTERVAL + 5);
        mine(&mut session.app, third);
        play(&mut session, INTERVAL / 2);
        drop(session);

        let mut loaded = load_save(temp.path());
        assert_mined(&mut loaded, first);
        assert_mined(&mut loaded, second);
    }

    #[test]
    fn interval_of_30_seconds_saves_the_edit_after_30_seconds() {
        let temp = tempfile::tempdir().expect("temp dir");
        let mut session = session(temp.path(), "autosave_unit_thirty_seconds");
        session.app.autosave = AutosaveSchedule::new(30 * TICK_HZ);
        let pos = surface_block(&session.app, 40, 40);
        mine(&mut session.app, pos);

        play(&mut session, 299 * TICK_HZ / 10);
        let saved_at_29_9_seconds = temp.path().join("scene.bin").exists();
        play(&mut session, 30 * TICK_HZ - 299 * TICK_HZ / 10);

        assert!(!saved_at_29_9_seconds, "saved before the interval passed");
        assert_mined(&mut load_save(temp.path()), pos);
    }
}
