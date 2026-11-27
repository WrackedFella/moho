//! The levels the game ships, chosen by name at launch, and the notices a loaded level earns.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use moho_core::assets::{AssetError, AssetRoots};

use crate::arena::Arena;

/// A shipped level: the name the tester launches it by and its file under the assets folder.
#[derive(Debug, PartialEq, Eq)]
pub struct NamedLevel {
    pub name: &'static str,
    pub file: &'static str,
}

/// Every shipped level. The first is the default.
pub const LEVELS: &[NamedLevel] = &[
    NamedLevel {
        name: "arena",
        file: "arena.glb",
    },
    NamedLevel {
        name: "proving_ground",
        file: "proving_ground.glb",
    },
    NamedLevel {
        name: "parkour",
        file: "parkour.glb",
    },
];

/// The level a launch argument asks for.
#[derive(Debug, PartialEq, Eq)]
pub enum LevelChoice {
    Named(&'static NamedLevel),
    Path(PathBuf),
}

impl LevelChoice {
    /// The file to load: a named level resolved through `assets`, a path as given.
    ///
    /// # Errors
    /// [`AssetError`] when a named level's file is in no asset folder.
    pub fn path(&self, assets: &AssetRoots) -> Result<PathBuf, AssetError> {
        match self {
            Self::Named(level) => assets.resolve(Path::new(level.file)),
            Self::Path(path) => Ok(path.clone()),
        }
    }
}

/// A launch argument that is neither a level name nor a glTF path.
#[derive(Debug, thiserror::Error)]
#[error("no level named `{name}`; levels: {}", level_names())]
pub struct UnknownLevel {
    pub name: String,
}

/// The level `arg` asks for: none is the first of [`LEVELS`], a `.glb` or `.gltf` file name
/// (any case) is a path, anything else is looked up by exact name.
///
/// # Errors
/// [`UnknownLevel`] when `arg` is neither, listing the level names in its message.
pub fn choose(arg: Option<&OsStr>) -> Result<LevelChoice, UnknownLevel> {
    let Some(arg) = arg else {
        return Ok(LevelChoice::Named(&LEVELS[0]));
    };
    let path = Path::new(arg);
    let is_gltf = path
        .extension()
        .and_then(OsStr::to_str)
        .is_some_and(|ext| ext.eq_ignore_ascii_case("glb") || ext.eq_ignore_ascii_case("gltf"));
    if is_gltf {
        return Ok(LevelChoice::Path(path.to_owned()));
    }
    let name = arg.to_string_lossy();
    LEVELS
        .iter()
        .find(|level| level.name == name)
        .map(LevelChoice::Named)
        .ok_or_else(|| UnknownLevel {
            name: name.into_owned(),
        })
}

fn level_names() -> String {
    let names: Vec<&str> = LEVELS.iter().map(|level| level.name).collect();
    names.join(", ")
}

/// The lines to show the tester about a loaded level, in the order they apply.
pub fn load_notices(arena: &Arena) -> Vec<&'static str> {
    let mut notices = Vec::new();
    if arena.nav_fallback {
        notices.push("No walkable area; enemies cannot path here");
    }
    notices
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::sync::Arc;

    use glam::Vec3;
    use moho_core::content::ContentRoots;
    use moho_level::{Level, LevelMesh};
    use moho_physics::RayQuery;
    use searchlight_rules::{BodyPart, MIN_SPEED};

    use super::*;
    use crate::arena::{self, ArenaMarkers};
    use crate::input::SearchlightCommand;
    use crate::kit::{self, START_KIT_ID};
    use crate::mode::{ArenaMode, Tally};
    use crate::sim::{STAND_HEIGHT, dummy_hit_volumes};
    use crate::tag::SurfaceTag;
    use crate::test_support::{
        DT, box_mesh, level_marker, mag, placeholder_assets, rifle_from, test_config,
    };
    use crate::tuning::SHIPPED_BODY;
    use crate::wave::WaveTuning;
    use crate::{assets_root, content_root};

    fn chosen(arg: &str) -> Result<LevelChoice, UnknownLevel> {
        choose(Some(OsStr::new(arg)))
    }

    fn named(choice: LevelChoice) -> &'static NamedLevel {
        match choice {
            LevelChoice::Named(level) => level,
            LevelChoice::Path(path) => panic!("expected a named level, got the path {path:?}"),
        }
    }

    #[test]
    fn the_name_arena_chooses_the_arena_file() {
        let choice = chosen("arena").expect("`arena` is a level");

        assert_eq!(
            choice.path(&placeholder_assets()).unwrap(),
            assets_root().join("arena.glb")
        );
        assert_eq!(named(choice).name, "arena");
    }

    #[test]
    fn the_name_proving_ground_chooses_its_file() {
        let choice = chosen("proving_ground").expect("`proving_ground` is a level");

        assert_eq!(
            choice.path(&placeholder_assets()).unwrap(),
            assets_root().join("proving_ground.glb")
        );
        assert_eq!(named(choice).name, "proving_ground");
    }

    #[test]
    fn no_argument_chooses_the_arena() {
        let choice = choose(None).expect("no argument is the default level");

        assert_eq!(named(choice).name, "arena");
        assert_eq!(LEVELS[0].name, "arena");
    }

    #[test]
    fn an_unknown_name_is_an_error_listing_every_level() {
        let error = chosen("nowhere").unwrap_err();

        assert_eq!(error.name, "nowhere");
        assert_eq!(
            error.to_string(),
            "no level named `nowhere`; levels: arena, proving_ground, parkour"
        );
    }

    #[test]
    fn a_name_with_the_wrong_case_is_unknown() {
        let error = chosen("Arena").unwrap_err();

        assert_eq!(error.name, "Arena");
    }

    #[test]
    fn a_glb_or_gltf_argument_is_a_path() {
        for arg in ["maps/local/de_x.glb", "borrowed.gltf", "arena.glb"] {
            let choice = chosen(arg).unwrap_or_else(|e| panic!("{arg}: {e}"));

            assert_eq!(choice, LevelChoice::Path(PathBuf::from(arg)), "{arg}");
            assert_eq!(
                choice.path(&placeholder_assets()).unwrap(),
                Path::new(arg),
                "a path is loaded as given, not under the assets folder"
            );
        }
    }

    #[test]
    fn a_named_level_found_in_no_folder_is_not_found_naming_where_it_looked() {
        let empty = tempfile::tempdir().unwrap();
        let roots = AssetRoots::placeholders(empty.path()).unwrap();
        let choice = chosen("arena").unwrap();

        let error = choice.path(&roots).unwrap_err();

        assert!(
            matches!(
                &error,
                AssetError::NotFound { name, searched }
                    if name == Path::new("arena.glb") && searched == &[empty.path().to_owned()]
            ),
            "{error:?}"
        );
    }

    #[test]
    fn a_path_argument_is_returned_as_given() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("arena.glb"), "x").unwrap();
        let folder = AssetRoots::placeholders(&assets_root())
            .unwrap()
            .with_folder(dir.path())
            .unwrap();
        let choice = chosen("arena.glb").unwrap();

        let path = choice.path(&folder).unwrap();

        assert_eq!(path, Path::new("arena.glb"));
    }

    #[test]
    fn an_upper_case_extension_is_a_path() {
        let choice = chosen("MAP.GLB").expect("an upper-case extension is still a glTF file");

        assert_eq!(choice, LevelChoice::Path(PathBuf::from("MAP.GLB")));
    }

    #[test]
    fn names_are_unique() {
        let names: BTreeSet<&str> = LEVELS.iter().map(|level| level.name).collect();
        let files: BTreeSet<&str> = LEVELS.iter().map(|level| level.file).collect();

        assert_eq!(names.len(), LEVELS.len());
        assert_eq!(files.len(), LEVELS.len());
    }

    fn shipped(file: &str) -> (Level, ArenaMarkers) {
        arena::load(&assets_root().join(file)).unwrap_or_else(|e| panic!("{file}: {e}"))
    }

    #[test]
    fn every_listed_level_file_loads() {
        for level in LEVELS {
            let (loaded, _) = shipped(level.file);

            assert!(!loaded.meshes.is_empty(), "{}: no geometry", level.file);
        }
    }

    #[test]
    fn no_shipped_level_names_a_mode() {
        for level in LEVELS {
            let (loaded, _) = shipped(level.file);

            let stale: Vec<&str> = loaded
                .markers
                .iter()
                .filter(|marker| marker.name == "searchlight_level")
                .map(|marker| marker.name.as_str())
                .collect();

            assert!(stale.is_empty(), "{}: {stale:?}", level.file);
        }
    }

    /// The mode a shipped level plays as, built with the tuning the tests share.
    fn shipped_mode(file: &str) -> ArenaMode {
        let (level, markers) = shipped(file);
        let arena = Arc::new(Arena::from_level(&level, &markers).expect("the level builds"));
        ArenaMode::for_level(
            arena,
            test_config(rifle_from(mag(30, 30)), vec![]),
            WaveTuning::default(),
        )
    }

    #[test]
    fn the_shipped_arena_starts_on_wave_one_with_enemies() {
        let mode = shipped_mode("arena.glb");

        assert_eq!(mode.tally(), Some(Tally { wave: 1, score: 0 }));
        assert!(mode.sim().enemy_count() > 0, "no enemy spawned");
    }

    #[test]
    fn the_proving_ground_spawns_no_enemy_in_sixty_seconds() {
        let mut mode = shipped_mode("proving_ground.glb");

        for _ in 0..3600 {
            mode.tick(&SearchlightCommand::default(), DT);
        }

        assert_eq!(mode.tally(), None);
        assert_eq!(mode.sim().enemy_count(), 0);
        assert_eq!(mode.sim().enemies_alive(), 0);
    }

    #[test]
    fn a_level_with_only_a_player_spawn_loads_there_without_waves() {
        let spawn = Vec3::new(2.0, 0.0, -3.0);
        let level = Level {
            meshes: vec![LevelMesh {
                mesh: box_mesh(Vec3::new(-5.0, -1.0, -5.0), Vec3::new(5.0, 0.0, 5.0)),
                material: None,
            }],
            markers: vec![level_marker("player_spawn", spawn)],
        };
        let markers = ArenaMarkers::from_level(&level).expect("a floor and a spawn load");
        let arena = Arc::new(Arena::from_level(&level, &markers).expect("the level builds"));

        let notices = load_notices(&arena);
        let mode = ArenaMode::for_level(
            arena.clone(),
            test_config(rifle_from(mag(30, 30)), vec![]),
            WaveTuning::default(),
        );

        assert_eq!(arena.player_spawn, spawn);
        assert!(notices.is_empty(), "{notices:?}");
        assert_eq!(mode.tally(), None);
        assert_eq!(mode.sim().enemy_count(), 0);
    }

    #[test]
    fn the_proving_ground_has_dummies_at_25_50_100_and_200_m() {
        let (_, markers) = shipped("proving_ground.glb");
        let spawn = &markers.player_spawn;

        let mut distances: Vec<f32> = markers
            .dummies
            .iter()
            .map(|dummy| dummy.position.z - spawn.position.z)
            .collect();
        distances.sort_by(f32::total_cmp);

        assert_eq!(distances.len(), 4, "{distances:?}");
        for (actual, expected) in distances.iter().zip([25.0, 50.0, 100.0, 200.0]) {
            assert!((actual - expected).abs() < 0.01, "{distances:?}");
        }
    }

    #[test]
    fn the_proving_ground_cover_stands_0_6_1_0_and_1_4_m() {
        let (level, _) = shipped("proving_ground.glb");
        let world = arena::physics_world(&level);
        let top_at = |x: f32, z: f32| {
            world
                .cast_ray(&RayQuery {
                    origin: Vec3::new(x, 5.0, z),
                    direction: Vec3::NEG_Y,
                    max_distance: 10.0,
                    ignore: None,
                })
                .map(|hit| 5.0 - hit.distance)
        };
        let is = |top: Option<f32>, height: f32| top.is_some_and(|t| (t - height).abs() < 0.01);
        // A cover block is a flat top at least 0.8 m across; a stair tread is 0.3 m deep.
        let flat_patch_at = |height: f32| {
            (-75..=75)
                .flat_map(|i| (-75..=100).map(move |j| (i as f32 * 0.2, j as f32 * 0.2)))
                .any(|(x, z)| {
                    is(top_at(x, z), height)
                        && [(0.4, 0.0), (-0.4, 0.0), (0.0, 0.4), (0.0, -0.4)]
                            .iter()
                            .all(|&(dx, dz)| is(top_at(x + dx, z + dz), height))
                })
        };

        let found: Vec<(f32, bool)> = [0.6, 1.0, 1.4]
            .into_iter()
            .map(|height| (height, flat_patch_at(height)))
            .collect();

        assert_eq!(found, [(0.6, true), (1.0, true), (1.4, true)]);
    }

    #[test]
    fn the_200_m_dummy_is_within_the_start_kits_round_reach_from_the_firing_line() {
        let roots = ContentRoots::base(&content_root()).expect("the shipped content folder");
        let ballistics = kit::load(&roots, START_KIT_ID)
            .expect("the shipped start kit loads")
            .rounds
            .into_values()
            .next()
            .expect("the start kit names a round")
            .ballistics;
        // Linear drag sheds speed at `drag` m/s per metre, so a round is spent after this far.
        let range = (ballistics.muzzle_velocity - MIN_SPEED) / ballistics.drag;
        let (_, markers) = shipped("proving_ground.glb");
        let spawn = &markers.player_spawn;
        let far = markers
            .dummies
            .iter()
            .max_by(|a, b| a.position.z.total_cmp(&b.position.z))
            .expect("the proving ground has dummies");
        let torso = dummy_hit_volumes(0)
            .into_iter()
            .find(|v| {
                v.tag
                    == SurfaceTag::Part {
                        target: 0,
                        part: BodyPart::Torso,
                    }
                    .into()
            })
            .expect("a dummy has a torso");

        let eye = spawn.position + Vec3::Y * SHIPPED_BODY.standing.eye_height;
        let centre = far.position + Vec3::Y * STAND_HEIGHT + torso.offset;
        let near_face = centre - Vec3::Z * torso.half_extents.z;

        let distance = eye.distance(near_face);

        assert!(
            distance <= range,
            "the torso's near face is {distance} m from the eye; the round reaches {range} m"
        );
    }

    /// A 10 m floor, plus `names` as markers at the origin.
    fn floor_level(names: &[&str]) -> Level {
        Level {
            meshes: vec![LevelMesh {
                mesh: box_mesh(Vec3::new(-5.0, -1.0, -5.0), Vec3::new(5.0, 0.0, 5.0)),
                material: Some("floor".to_owned()),
            }],
            markers: names.iter().map(|n| level_marker(n, Vec3::ZERO)).collect(),
        }
    }

    fn notices_of(level: &Level) -> Vec<&'static str> {
        let markers = ArenaMarkers::from_level(level).expect("valid markers");
        let arena = Arena::from_level(level, &markers).expect("the level builds");
        load_notices(&arena)
    }

    #[test]
    fn a_level_with_a_spawn_and_a_navmesh_has_no_notices() {
        let level = floor_level(&["player_spawn"]);

        let notices = notices_of(&level);

        assert!(notices.is_empty(), "{notices:?}");
    }

    #[test]
    fn a_navmesh_fallback_has_its_notice() {
        let mut level = floor_level(&["player_spawn"]);
        level.meshes.push(LevelMesh {
            mesh: box_mesh(
                Vec3::new(-500.0, -1.0, -500.0),
                Vec3::new(500.0, 0.0, 500.0),
            ),
            material: None,
        });

        let notices = notices_of(&level);

        assert_eq!(notices, ["No walkable area; enemies cannot path here"]);
    }

    #[test]
    fn parkour_loads_by_name_with_a_spawn_and_no_waves() {
        let choice = chosen("parkour").expect("`parkour` is a level");
        let file = named(choice).file;

        let (_, markers) = shipped(file);
        let mode = shipped_mode(file);

        assert_eq!(file, "parkour.glb");
        assert!(
            markers.enemy_spawns.is_empty(),
            "{:?}",
            markers.enemy_spawns
        );
        assert!(markers.dummies.is_empty(), "{:?}", markers.dummies);
        assert_eq!(mode.tally(), None);
        assert_eq!(mode.sim().enemy_count(), 0);
    }

    #[test]
    fn every_parkour_marker_has_its_extras() {
        let (level, _) = shipped("parkour.glb");

        let gaps = parkour::markers_named(&level, "gap_");
        let pieces = parkour::markers_named(&level, "course_");

        let mut widths: Vec<f32> = gaps.iter().map(|g| parkour::number(g, "width")).collect();
        widths.sort_by(f32::total_cmp);
        let expected: Vec<f32> = (1..=10).map(|i| i as f32 * 0.5).collect();
        assert_eq!(widths, expected, "one gap per width, 0.5 to 5.0 m");
        for gap in &gaps {
            let clears = parkour::extra(gap, "clears");
            assert!(
                ["stand", "run", "none"].contains(&clears),
                "{}: clears `{clears}`",
                gap.name
            );
        }
        let mut kinds: Vec<(&str, f32)> = pieces
            .iter()
            .map(|p| (parkour::extra(p, "kind"), parkour::number(p, "value")))
            .collect();
        kinds.sort_by(|a, b| a.0.cmp(b.0).then(a.1.total_cmp(&b.1)));
        assert_eq!(
            kinds,
            [
                ("ledge", 1.0),
                ("ledge", 1.2),
                ("ledge", 1.4),
                ("slope", 30.0),
                ("slope", 45.0),
                ("slope", 50.0),
                ("step", 0.3),
                ("step", 0.5),
                ("step", 0.6),
            ]
        );
        for piece in &pieces {
            let passable = parkour::extra(piece, "passable");
            assert!(
                ["walk", "jump", "none"].contains(&passable),
                "{}: passable `{passable}`",
                piece.name
            );
        }
    }

    /// The headless jump and climb trials on the shipped parkour level, with the shipped
    /// movement tuning. A marker stands on the floor at the take-off edge of a gap, or at the
    /// foot of a course piece, and faces across or up it (its rotation turns +Z that way).
    mod parkour {
        use glam::Vec3;
        use moho_level::LevelMarker;
        use moho_physics::CharacterCommand;

        use super::*;
        use crate::sim::{SearchlightSim, SimConfig};
        use crate::tuning::tuning;

        /// Ticks to wait for a trial to end before calling it failed: far longer than a jump.
        const TRIAL_TICKS: usize = 300;
        /// How far below its floor a character's feet are when it is in the pit.
        const PIT_DROP: f32 = 0.5;

        pub(super) fn markers_named<'a>(level: &'a Level, prefix: &str) -> Vec<&'a LevelMarker> {
            level
                .markers
                .iter()
                .filter(|marker| marker.name.starts_with(prefix))
                .collect()
        }

        pub(super) fn extra<'a>(marker: &'a LevelMarker, key: &str) -> &'a str {
            marker
                .properties
                .get(key)
                .unwrap_or_else(|| panic!("{}: no `{key}` extra", marker.name))
        }

        pub(super) fn number(marker: &LevelMarker, key: &str) -> f32 {
            let text = extra(marker, key);
            text.parse()
                .unwrap_or_else(|_| panic!("{}: `{key}` is `{text}`, not a number", marker.name))
        }

        fn facing(marker: &LevelMarker) -> Vec3 {
            let across = marker.rotation * Vec3::Z;
            Vec3::new(across.x, 0.0, across.z).normalize()
        }

        fn shipped_config() -> SimConfig {
            let roots = ContentRoots::base(&content_root()).expect("the shipped content");
            let kit = kit::load(&roots, START_KIT_ID).expect("the shipped start kit");
            tuning(kit)
        }

        /// A player standing on the floor point `start`, turned to face `facing`, settled.
        struct Trial {
            sim: SearchlightSim,
            origin: Vec3,
            facing: Vec3,
            floor: f32,
        }

        impl Trial {
            /// `start_back` metres behind `origin` along `facing`, on the marker's floor.
            fn new(
                level: &Level,
                config: &SimConfig,
                marker: &LevelMarker,
                start_back: f32,
            ) -> Self {
                let facing = facing(marker);
                let start = marker.position - facing * start_back;
                let mut sim =
                    SearchlightSim::new(arena::physics_world(level), start, &[], config.clone());
                let turn = SearchlightCommand {
                    movement: CharacterCommand {
                        yaw_delta: facing.x.atan2(facing.z),
                        ..CharacterCommand::default()
                    },
                    ..SearchlightCommand::default()
                };
                sim.tick(&turn, DT);
                for _ in 0..30 {
                    sim.tick(&SearchlightCommand::default(), DT);
                }
                assert!(
                    sim.player().grounded,
                    "{}: the player never landed",
                    marker.name
                );
                Self {
                    sim,
                    origin: marker.position,
                    facing,
                    floor: marker.position.y,
                }
            }

            fn tick(&mut self, forward: f32, jump: bool) {
                let command = SearchlightCommand {
                    movement: CharacterCommand {
                        forward,
                        jump,
                        ..CharacterCommand::default()
                    },
                    ..SearchlightCommand::default()
                };
                self.sim.tick(&command, DT);
            }

            /// How far past the marker the player's centre is, along the marker's facing.
            fn along(&self) -> f32 {
                (self.sim.player().position - self.origin).dot(self.facing)
            }

            fn feet_above_floor(&self) -> f32 {
                self.sim.player().position.y - STAND_HEIGHT - self.floor
            }

            fn run_to_the_marker(&mut self) {
                for _ in 0..TRIAL_TICKS {
                    if self.along() >= 0.0 {
                        return;
                    }
                    self.tick(1.0, false);
                }
                panic!(
                    "never reached the marker at {}, stopped at {}",
                    self.origin,
                    self.along(),
                );
            }
        }

        /// Whether a jump from the marker lands on the far platform of a gap `width` wide: the
        /// jump is taken at the marker with the run-up behind it, or from standing on it.
        fn jump_clears(
            level: &Level,
            config: &SimConfig,
            gap: &LevelMarker,
            running: bool,
        ) -> bool {
            let width = number(gap, "width");
            let mut trial = Trial::new(level, config, gap, if running { 5.0 } else { 0.0 });
            if running {
                trial.run_to_the_marker();
                trial.tick(1.0, true);
            } else {
                trial.tick(0.0, true);
            }
            for tick in 0..TRIAL_TICKS {
                trial.tick(1.0, false);
                if trial.feet_above_floor() < -PIT_DROP {
                    return false;
                }
                if tick > 3 && trial.sim.player().grounded {
                    return trial.along() >= width;
                }
            }
            false
        }

        /// Whether the player reaches the top of the course piece at `piece`: by walking at it,
        /// or by walking at it and jumping as the foot of it comes within a body's reach.
        fn climbs(level: &Level, config: &SimConfig, piece: &LevelMarker, jumping: bool) -> bool {
            let mut trial = Trial::new(level, config, piece, 3.0);
            let mut jumped = false;
            for _ in 0..TRIAL_TICKS {
                let jump = jumping && !jumped && trial.along() >= -1.0;
                jumped |= jump;
                trial.tick(1.0, jump);
                if trial.sim.player().grounded
                    && trial.along() >= 1.0
                    && trial.feet_above_floor() >= 0.25
                {
                    return true;
                }
            }
            false
        }

        #[test]
        fn every_gap_is_cleared_by_exactly_the_movement_it_names() {
            let (level, _) = shipped("parkour.glb");
            let config = shipped_config();
            let gaps = markers_named(&level, "gap_");
            assert!(!gaps.is_empty(), "the parkour level has no gaps");

            let mut wrong = Vec::new();
            for gap in gaps {
                let clears = extra(gap, "clears");
                let expected_running = clears != "none";
                let expected_standing = clears == "stand";
                let running = jump_clears(&level, &config, gap, true);
                let standing = jump_clears(&level, &config, gap, false);
                if (running, standing) != (expected_running, expected_standing) {
                    wrong.push(format!(
                        "{} ({} m) is marked `{clears}`; a running jump clears it: {running}, \
                         a standing jump: {standing}",
                        gap.name,
                        extra(gap, "width")
                    ));
                }
            }

            assert!(wrong.is_empty(), "{wrong:#?}");
        }

        #[test]
        fn the_ladder_has_a_running_gap_and_an_uncleared_gap() {
            let (level, _) = shipped("parkour.glb");

            let marks: BTreeSet<&str> = markers_named(&level, "gap_")
                .into_iter()
                .map(|gap| extra(gap, "clears"))
                .collect();

            assert!(
                marks.contains("run"),
                "no gap needs a running jump: {marks:?}"
            );
            assert!(marks.contains("none"), "no gap is out of reach: {marks:?}");
        }

        #[test]
        fn every_course_piece_is_passable_exactly_as_labelled() {
            let (level, _) = shipped("parkour.glb");
            let config = shipped_config();
            let pieces = markers_named(&level, "course_");
            assert!(!pieces.is_empty(), "the parkour level has no course");

            let mut wrong = Vec::new();
            for piece in pieces {
                let actual = if climbs(&level, &config, piece, false) {
                    "walk"
                } else if climbs(&level, &config, piece, true) {
                    "jump"
                } else {
                    "none"
                };
                let label = extra(piece, "passable");
                if actual != label {
                    wrong.push(format!(
                        "{} ({} {}) is labelled `{label}` but is `{actual}`",
                        piece.name,
                        extra(piece, "kind"),
                        extra(piece, "value")
                    ));
                }
            }

            assert!(wrong.is_empty(), "{wrong:#?}");
        }
    }
}
