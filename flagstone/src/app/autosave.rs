/// When the running world is next autosaved: after an edit, once an interval of
/// simulation ticks has passed since the last save.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutosaveSchedule {
    pub(crate) interval_ticks: u64,
    pub(crate) ticks_since_save: u64,
    pub(crate) edited: bool,
}

impl AutosaveSchedule {
    /// A schedule that comes due `interval` after the last save, counted in ticks of `tick_length`.
    ///
    /// The interval rounds up to whole ticks, so a save never comes early, and is at least one
    /// tick, so a zero interval saves at most once per tick instead of every frame.
    pub fn new(interval: std::time::Duration, tick_length: std::time::Duration) -> Self {
        let tick_nanos = tick_length.as_nanos().max(1);
        let ticks = interval.as_nanos().div_ceil(tick_nanos).max(1);
        Self {
            interval_ticks: u64::try_from(ticks).unwrap_or(u64::MAX),
            ticks_since_save: 0,
            edited: false,
        }
    }

    /// Counts one simulation tick.
    pub fn tick(&mut self) {
        self.ticks_since_save = self.ticks_since_save.saturating_add(1);
    }

    /// Records that the world changed since the last save.
    pub fn mark_edited(&mut self) {
        self.edited = true;
    }

    /// Whether the world has an unsaved edit and a full interval has passed since the last save.
    pub fn due(&self) -> bool {
        self.edited && self.ticks_since_save >= self.interval_ticks
    }

    /// A save succeeded: the edit is on disk and the interval starts over.
    pub fn saved(&mut self) {
        self.ticks_since_save = 0;
        self.edited = false;
    }

    /// A save failed: the interval starts over so the retry waits, and the edit stays unsaved.
    pub fn save_failed(&mut self) {
        self.ticks_since_save = 0;
    }
}

/// Save the current scene to `saves_dir/scene.bin`, creating the directory
/// if needed.
///
/// With no world loaded there is nothing to save: nothing is written, so the previous save
/// stays, and the call is not an error.
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
    let started = std::time::Instant::now();
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

    use crate::app::chunk_streamer::ChunkStreamer;
    use crate::app::event_loop::{EventProcessor, FrameProcessor};
    use crate::app::scene_loader::load_scene;
    use crate::input_event::InputEvent;
    use flagstone_game::TICK_HZ;
    use flagstone_game::controller::CameraMode;
    use flagstone_game::scene_builders::{TerrainConfig, WorldSpec};
    use flagstone_voxel::{BlockPos, StreamingConfig};
    use glam::{IVec3, Vec3};
    use moho_app::{EventContext, FrameContext, Game, HeadlessLoop, LoopConfig};
    use moho_core::events::UiEvent;
    use std::path::Path;
    use std::time::Duration;
    use winit::event::WindowEvent;

    const WORLD_SIZE: u32 = 256;
    /// Where the player stands when the world loads, and the column the edits are made in.
    const HOME_X: f32 = 8.5;
    /// Six chunk columns from home: past the streamer's unload radius.
    const AWAY_X: f32 = HOME_X + 6.0 * 16.0;

    fn tick_length() -> Duration {
        LoopConfig::new(TICK_HZ).tick_length()
    }

    fn ticks_in(duration: Duration) -> u32 {
        u32::try_from(duration.as_nanos() / tick_length().as_nanos()).expect("a short duration")
    }

    fn spec(world: &str) -> WorldSpec {
        WorldSpec {
            name: world.to_string(),
            size_xz: WORLD_SIZE,
            ..WorldSpec::default()
        }
    }

    /// Streams the columns next to the player and evicts those more than two away.
    fn small_streamer(world: &str, saves: &Path) -> ChunkStreamer {
        ChunkStreamer::new(
            TerrainConfig {
                world_size: WORLD_SIZE,
                ..TerrainConfig::default()
            },
            StreamingConfig {
                load_radius_chunks: 1,
                unload_radius_chunks: 2,
                chunks_per_frame: 9,
            },
            world,
            saves,
        )
    }

    /// Writes the save of a world with terrain around the origin, as an earlier session left it.
    fn write_world_save(saves: &Path, world: &str) {
        let mut app = App::headless();
        app.generation.last_spec = Some(spec(world));
        app.simulation
            .set_position_yaw_pitch(Vec3::new(HOME_X, 200.0, 8.5), 0.0, 0.0);
        let grid = app
            .light_system
            .as_mut()
            .expect("App starts with a light system")
            .grid_mut();
        small_streamer(world, saves).load_columns_now(grid, (0, 0), 2);

        auto_save_on_shutdown(&mut app, &GameClock::default(), saves, &[]).expect("seed save");
    }

    /// The save in `saves`, loaded as the player loads it.
    fn load_save(saves: &Path) -> App {
        let mut app = App::headless();
        app.saves_dir = saves.to_path_buf();
        load_scene(
            &mut app,
            &mut GameClock::default(),
            &saves.join("scene.bin"),
        )
        .expect("the save loads");
        app
    }

    /// A world loaded from a save and ready to play: first person, a pickaxe in hand, standing still.
    fn playing_world(saves: &Path, world: &str) -> App {
        write_world_save(saves, world);
        let mut app = load_save(saves);
        app.chunk_streamer = Some(small_streamer(world, saves));
        app.simulation.set_camera_mode(CameraMode::FirstPerson);
        app.physics.noclip = true;
        app.pawn.select_slot(flagstone_game::pawn::TOOL_SLOT);
        app
    }

    fn grid(app: &App) -> &flagstone_voxel::VoxelGrid {
        app.light_system
            .as_ref()
            .expect("App has a light system")
            .grid()
    }

    fn surface_block(app: &App, x: i32, z: i32) -> BlockPos {
        let top = grid(app).get_height(x, z).expect("the column has terrain");
        BlockPos::new(x, top, z)
    }

    /// The player aims down at `block` and clicks.
    fn mine(app: &mut App, block: BlockPos) {
        let eye = block.as_vec3() + Vec3::new(0.5, 3.0, 0.5);
        app.camera.0 = glam::camera::rh::view::look_to_mat4(eye, Vec3::NEG_Y, Vec3::Z);
        let (tx, rx) = std::sync::mpsc::channel();
        app.input.unconsumed_rx = Some(rx);
        tx.send(InputEvent::MineRequested).expect("channel open");

        EventProcessor::new().process_input_events(app);

        assert!(!grid(app).is_solid_at(block), "the click mined {block}");
    }

    /// A mined block and an untouched one beside it, to tell an edit from a lost world.
    struct Edit {
        mined: BlockPos,
        kept: BlockPos,
    }

    impl Edit {
        fn chunk(&self) -> IVec3 {
            self.mined.div_euclid(IVec3::splat(16))
        }
    }

    /// Mines the surface at home and picks an untouched block four columns over.
    fn mine_at_home(app: &mut App) -> Edit {
        let edit = Edit {
            mined: surface_block(app, 8, 8),
            kept: surface_block(app, 12, 8),
        };
        mine(app, edit.mined);
        edit
    }

    fn walk_to(app: &mut App, x: f32) {
        app.simulation
            .set_position_yaw_pitch(Vec3::new(x, 100.0, 8.5), 0.0, 0.0);
        FrameProcessor::new().update_chunk_streaming(app);
    }

    struct Saved {
        kept_present: bool,
        mined_present: bool,
    }

    /// Loads the save and walks the player to the edit, so a chunk kept in its own file is read.
    fn saved_blocks(saves: &Path, edit: &Edit) -> Saved {
        let mut app = load_save(saves);
        app.chunk_streamer = Some(small_streamer(
            &app.generation
                .last_spec
                .as_ref()
                .expect("a loaded world has a spec")
                .name,
            saves,
        ));
        walk_to(&mut app, HOME_X);
        let world = grid(&app);
        Saved {
            kept_present: world.is_solid_at(edit.kept),
            mined_present: world.is_solid_at(edit.mined),
        }
    }

    fn assert_save_holds_the_edit(saves: &Path, edit: &Edit) {
        let saved = saved_blocks(saves, edit);

        assert!(saved.kept_present, "the save still holds the world");
        assert!(!saved.mined_present, "the mined block is absent");
    }

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

    /// Exit chosen in a menu: the UI asks, the next frame leaves.
    fn choose_exit_in_menu(app: &mut App) -> bool {
        app.event_bus.publish(UiEvent::ExitRequested);
        app.event_bus.process_deferred();
        EventProcessor::new().process_ui_events(app, &mut GameClock::default());
        run_frame(app)
    }

    fn run_frame(app: &mut App) -> bool {
        let clock = GameClock::default();
        let mut ctx = FrameContext::new(tick_length(), &clock, None, None);
        Game::frame(app, &mut ctx, 0.0);
        ctx.exit_requested()
    }

    fn play(sim: &mut HeadlessLoop<App>, app: &mut App, ticks: u32) {
        let mut ran = 0;
        while ran < ticks {
            ran += sim.advance(app, tick_length());
        }
    }

    fn set_autosave_interval(app: &mut App, interval: Duration) {
        app.autosave = AutosaveSchedule::new(interval, tick_length());
    }

    #[test]
    fn mined_block_is_absent_after_staying_near_then_closing() {
        let saves = tempfile::tempdir().expect("temp dir");
        let mut app = playing_world(saves.path(), "autosave_near_close_world");
        let edit = mine_at_home(&mut app);

        let exit = close_window(&mut app);

        assert!(exit, "closing the window leaves the loop");
        assert_save_holds_the_edit(saves.path(), &edit);
    }

    #[test]
    fn mined_block_is_absent_after_staying_near_then_menu_exit() {
        let saves = tempfile::tempdir().expect("temp dir");
        let mut app = playing_world(saves.path(), "autosave_near_menu_world");
        let edit = mine_at_home(&mut app);
        app.game_state = GameState::Paused;

        let exit = choose_exit_in_menu(&mut app);

        assert!(exit, "Exit leaves the loop");
        assert_save_holds_the_edit(saves.path(), &edit);
    }

    #[test]
    fn mined_block_is_absent_after_walking_back_then_closing() {
        let saves = tempfile::tempdir().expect("temp dir");
        let mut app = playing_world(saves.path(), "autosave_back_close_world");
        let edit = mine_at_home(&mut app);

        walk_to(&mut app, AWAY_X);
        assert!(
            !grid(&app).has_chunk(edit.chunk()),
            "the edited chunk was evicted"
        );
        walk_to(&mut app, HOME_X);
        assert!(
            grid(&app).has_chunk(edit.chunk()),
            "the edited chunk is back"
        );
        assert!(
            !grid(&app).is_solid_at(edit.mined),
            "the chunk came back with the edit"
        );
        close_window(&mut app);

        assert_save_holds_the_edit(saves.path(), &edit);
    }

    #[test]
    fn mined_block_is_absent_after_staying_away_then_closing() {
        let saves = tempfile::tempdir().expect("temp dir");
        let mut app = playing_world(saves.path(), "autosave_away_close_world");
        let edit = mine_at_home(&mut app);

        walk_to(&mut app, AWAY_X);
        assert!(
            !grid(&app).has_chunk(edit.chunk()),
            "the edited chunk was evicted"
        );
        close_window(&mut app);

        assert_save_holds_the_edit(saves.path(), &edit);
    }

    /// A save of an edited world, then a session that never loaded one.
    fn edited_save_and_empty_session(world: &str) -> (tempfile::TempDir, Edit, App) {
        let saves = tempfile::tempdir().expect("temp dir");
        let mut played = playing_world(saves.path(), world);
        let edit = mine_at_home(&mut played);
        close_window(&mut played);
        let saved = saved_blocks(saves.path(), &edit);
        assert!(
            saved.kept_present && !saved.mined_present,
            "setup: the played session saved its edit"
        );
        let mut menu = App::headless();
        menu.saves_dir = saves.path().to_path_buf();
        assert_eq!(menu.game_state, GameState::Menu, "no world is loaded");
        (saves, edit, menu)
    }

    #[test]
    fn closing_with_no_world_keeps_the_previous_save() {
        let (saves, edit, mut menu) = edited_save_and_empty_session("autosave_empty_close_world");

        let exit = close_window(&mut menu);

        assert!(exit, "closing the window leaves the loop");
        assert_save_holds_the_edit(saves.path(), &edit);
    }

    #[test]
    fn menu_exit_with_no_world_keeps_the_previous_save() {
        let (saves, edit, mut menu) = edited_save_and_empty_session("autosave_empty_exit_world");

        let exit = choose_exit_in_menu(&mut menu);

        assert!(exit, "Exit leaves the loop");
        assert_save_holds_the_edit(saves.path(), &edit);
    }

    #[test]
    fn main_menu_with_no_world_keeps_the_previous_save() {
        let (saves, edit, mut menu) = edited_save_and_empty_session("autosave_empty_menu_world");
        menu.main_menu_requested = true;

        run_frame(&mut menu);

        assert_save_holds_the_edit(saves.path(), &edit);
    }

    #[test]
    fn killed_session_keeps_every_edit_before_the_last_autosave() {
        let saves = tempfile::tempdir().expect("temp dir");
        let mut app = playing_world(saves.path(), "autosave_killed_world");
        set_autosave_interval(&mut app, Duration::from_secs(10));
        let mut sim = HeadlessLoop::<App>::new(LoopConfig::new(TICK_HZ));
        let seconds = |s: u64| ticks_in(Duration::from_secs(s));
        let first = surface_block(&app, 8, 8);
        let second = surface_block(&app, 9, 8);
        let third = surface_block(&app, 10, 8);
        let last = surface_block(&app, 11, 8);
        let kept = surface_block(&app, 14, 8);

        mine(&mut app, first);
        play(&mut sim, &mut app, seconds(4));
        mine(&mut app, second);
        play(&mut sim, &mut app, seconds(7));
        mine(&mut app, third);
        play(&mut sim, &mut app, seconds(14));
        mine(&mut app, last);
        play(&mut sim, &mut app, seconds(3));
        drop(app);

        let loaded = load_save(saves.path());
        let world = grid(&loaded);
        assert!(world.is_solid_at(kept), "the save holds the world");
        for (block, age) in [(first, 28), (second, 24), (third, 17)] {
            assert!(
                !world.is_solid_at(block),
                "the block mined {age} s before the kill is absent"
            );
        }
    }

    #[test]
    fn interval_of_30_seconds_saves_the_edit_after_30_seconds() {
        let saves = tempfile::tempdir().expect("temp dir");
        let mut app = playing_world(saves.path(), "autosave_interval_world");
        set_autosave_interval(&mut app, Duration::from_secs(30));
        let mut sim = HeadlessLoop::<App>::new(LoopConfig::new(TICK_HZ));
        let edit = mine_at_home(&mut app);

        play(&mut sim, &mut app, ticks_in(Duration::from_millis(29_900)));
        let early = saved_blocks(saves.path(), &edit);
        play(
            &mut sim,
            &mut app,
            ticks_in(Duration::from_secs(30)) + 1 - ticks_in(Duration::from_millis(29_900)),
        );
        let on_time = saved_blocks(saves.path(), &edit);

        assert!(early.mined_present, "nothing is saved at 29.9 s");
        assert!(on_time.kept_present, "the save holds the world");
        assert!(!on_time.mined_present, "the edit is saved after 30 s");
    }

    #[test]
    fn idle_world_is_not_rewritten_after_an_interval() {
        let saves = tempfile::tempdir().expect("temp dir");
        let mut app = playing_world(saves.path(), "autosave_idle_world");
        set_autosave_interval(&mut app, Duration::from_secs(10));
        let mut sim = HeadlessLoop::<App>::new(LoopConfig::new(TICK_HZ));
        let scene = saves.path().join("scene.bin");
        std::fs::remove_file(&scene).expect("the seed save exists");

        play(&mut sim, &mut app, ticks_in(Duration::from_secs(25)));
        let after_idle = scene.exists();
        let edit = mine_at_home(&mut app);
        play(&mut sim, &mut app, ticks_in(Duration::from_secs(11)));

        assert!(!after_idle, "an idle world is not rewritten");
        assert!(scene.is_file(), "the loop runs: an edit is saved");
        assert_save_holds_the_edit(saves.path(), &edit);
    }

    #[test]
    fn failed_interval_save_retries_one_interval_later() {
        let saves = tempfile::tempdir().expect("temp dir");
        let mut app = playing_world(saves.path(), "autosave_failing_world");
        set_autosave_interval(&mut app, Duration::from_secs(10));
        let mut sim = HeadlessLoop::<App>::new(LoopConfig::new(TICK_HZ));
        let scene = saves.path().join("scene.bin");
        std::fs::remove_file(&scene).expect("the seed save exists");
        let not_a_dir = saves.path().join("a_file");
        std::fs::write(&not_a_dir, b"x").expect("write file");
        app.saves_dir = not_a_dir.clone();
        let edit = mine_at_home(&mut app);

        play(
            &mut sim,
            &mut app,
            ticks_in(Duration::from_secs(10)) + ticks_in(Duration::from_secs(1)),
        );

        assert!(!scene.exists(), "nothing is saved");
        assert!(app.autosave.edited, "the edit stays unsaved");
        assert!(
            app.autosave.ticks_since_save < app.autosave.interval_ticks,
            "the failed save reset the interval: {} ticks",
            app.autosave.ticks_since_save
        );
        assert!(
            app.autosave.ticks_since_save <= u64::from(ticks_in(Duration::from_secs(2))),
            "no retry every frame: {} ticks since the failure",
            app.autosave.ticks_since_save
        );

        app.saves_dir = saves.path().to_path_buf();
        play(&mut sim, &mut app, ticks_in(Duration::from_secs(10)));

        assert!(scene.is_file(), "the retry saved");
        assert_save_holds_the_edit(saves.path(), &edit);
    }

    fn schedule() -> AutosaveSchedule {
        AutosaveSchedule::new(Duration::from_secs(1), Duration::from_millis(100))
    }

    fn tick_times(schedule: &mut AutosaveSchedule, n: u32) {
        for _ in 0..n {
            schedule.tick();
        }
    }

    #[test]
    fn schedule_is_due_only_after_the_interval_and_an_edit() {
        let mut unedited = schedule();
        let mut edited = schedule();
        let mut edited_late = schedule();

        tick_times(&mut unedited, 30);
        edited.mark_edited();
        tick_times(&mut edited, 9);
        let one_tick_short = edited.due();
        tick_times(&mut edited, 1);
        tick_times(&mut edited_late, 25);
        edited_late.mark_edited();

        assert!(!unedited.due(), "no edit, nothing to save");
        assert!(!one_tick_short, "an interval is ten ticks, not nine");
        assert!(edited.due(), "an edit and ten ticks");
        assert!(
            edited_late.due(),
            "an edit after a long idle is due at once"
        );
    }

    #[test]
    fn schedule_resets_on_save() {
        let mut schedule = schedule();
        schedule.mark_edited();
        tick_times(&mut schedule, 10);
        assert!(schedule.due());

        schedule.saved();

        assert!(!schedule.due());
        assert_eq!(schedule.ticks_since_save, 0);
        assert!(!schedule.edited, "the edit is on disk");
        schedule.mark_edited();
        tick_times(&mut schedule, 9);
        assert!(!schedule.due(), "the interval starts over from the save");
        tick_times(&mut schedule, 1);
        assert!(schedule.due());
    }

    #[test]
    fn schedule_failed_save_retries_next_interval() {
        let mut schedule = schedule();
        schedule.mark_edited();
        tick_times(&mut schedule, 10);
        assert!(schedule.due());

        schedule.save_failed();

        assert!(!schedule.due(), "no retry on the next frame");
        assert!(schedule.edited, "the edit is still unsaved");
        tick_times(&mut schedule, 9);
        assert!(!schedule.due(), "the retry waits a full interval");
        tick_times(&mut schedule, 1);
        assert!(schedule.due(), "then it retries");
    }

    #[test]
    fn app_autosaves_every_two_minutes_by_default() {
        let app = App::headless();

        let ticks = u64::from(ticks_in(Duration::from_secs(120)));

        assert!(
            (ticks..=ticks + 1).contains(&app.autosave.interval_ticks),
            "{} ticks vs {ticks}",
            app.autosave.interval_ticks
        );
    }
}
