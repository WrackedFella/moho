//! The levels the game ships, chosen by name at launch, and the notices a loaded level earns.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use crate::arena::{Arena, ArenaMarkers};

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
];

/// The level a launch argument asks for.
#[derive(Debug, PartialEq, Eq)]
pub enum LevelChoice {
    Named(&'static NamedLevel),
    Path(PathBuf),
}

impl LevelChoice {
    /// The file to load: a named level under `assets`, a path as given.
    pub fn path(&self, assets: &Path) -> PathBuf {
        let _ = assets;
        todo!("a named level is under the assets folder; a path is as given")
    }
}

/// A launch argument that is neither a level name nor a glTF path.
#[derive(Debug, thiserror::Error)]
#[error("no level named `{name}`")]
pub struct UnknownLevel {
    pub name: String,
}

/// The level `arg` asks for: none is the first of [`LEVELS`], a `.glb` or `.gltf` file name
/// (any case) is a path, anything else is looked up by exact name.
///
/// # Errors
/// [`UnknownLevel`] when `arg` is neither, listing the level names in its message.
pub fn choose(arg: Option<&OsStr>) -> Result<LevelChoice, UnknownLevel> {
    let _ = arg;
    todo!("choose the level from the launch argument")
}

/// The lines to show the tester about a loaded level, in the order they apply.
pub fn load_notices(markers: &ArenaMarkers, arena: &Arena) -> Vec<&'static str> {
    let _ = (markers, arena);
    todo!("notice a missing spawn marker and a missing navmesh")
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use glam::Vec3;
    use moho_core::content::ContentRoots;
    use moho_level::{Level, LevelMesh};
    use moho_physics::RayQuery;
    use searchlight_rules::BodyPart;

    use super::*;
    use crate::arena::{self, LevelMode};
    use crate::kit::{self, START_KIT_ID};
    use crate::sim::{STAND_HEIGHT, dummy_hit_volumes};
    use crate::tag::SurfaceTag;
    use crate::test_support::{box_mesh, level_marker, mode_marker};
    use crate::tuning::SHIPPED_BODY;
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

        assert_eq!(choice.path(Path::new("assets")), Path::new("assets/arena.glb"));
        assert_eq!(named(choice).name, "arena");
    }

    #[test]
    fn the_name_proving_ground_chooses_its_file() {
        let choice = chosen("proving_ground").expect("`proving_ground` is a level");

        assert_eq!(
            choice.path(Path::new("assets")),
            Path::new("assets/proving_ground.glb")
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
            "no level named `nowhere`; levels: arena, proving_ground"
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
                choice.path(Path::new("assets")),
                Path::new(arg),
                "a path is loaded as given, not under the assets folder"
            );
        }
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

            let mode_markers: Vec<&str> = loaded
                .markers
                .iter()
                .filter(|marker| marker.name == "searchlight_level")
                .map(|marker| marker.properties.get("mode").map_or("", String::as_str))
                .collect();
            assert_eq!(mode_markers.len(), 1, "{}: one mode marker", level.file);
            assert!(
                ["arena", "sandbox"].contains(&mode_markers[0]),
                "{}: mode {:?}",
                level.file,
                mode_markers[0]
            );
        }
    }

    #[test]
    fn the_proving_ground_has_dummies_at_25_50_100_and_200_m() {
        let (_, markers) = shipped("proving_ground.glb");
        let spawn = markers
            .player_spawn
            .as_ref()
            .expect("the proving ground has a firing line spawn");

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
    fn the_200_m_dummy_is_in_rifle_range_from_the_firing_line() {
        let roots = ContentRoots::base(&content_root()).expect("the shipped content folder");
        let range = kit::load(&roots, START_KIT_ID)
            .expect("the shipped start kit loads")
            .weapon
            .range;
        let (_, markers) = shipped("proving_ground.glb");
        let spawn = markers.player_spawn.as_ref().expect("a firing line spawn");
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
            "the torso's near face is {distance} m from the eye; the rifle reaches {range} m"
        );
    }

    /// A 10 m floor and a sandbox marker, plus `names` as markers at the origin.
    fn sandbox_level(names: &[&str]) -> Level {
        let mut markers = vec![mode_marker("sandbox")];
        markers.extend(names.iter().map(|n| level_marker(n, Vec3::ZERO)));
        Level {
            meshes: vec![LevelMesh {
                mesh: box_mesh(Vec3::new(-5.0, -1.0, -5.0), Vec3::new(5.0, 0.0, 5.0)),
                material: Some("floor".to_owned()),
            }],
            markers,
        }
    }

    fn notices_of(level: &Level) -> Vec<&'static str> {
        let markers = ArenaMarkers::from_level(level).expect("a valid sandbox");
        let arena = Arena::from_level(level, &markers).expect("the sandbox builds");
        assert_eq!(markers.mode, LevelMode::Sandbox);
        load_notices(&markers, &arena)
    }

    #[test]
    fn a_level_without_a_spawn_has_the_origin_notice() {
        let level = sandbox_level(&[]);

        let notices = notices_of(&level);

        assert_eq!(notices, ["No player_spawn marker; starting at the origin"]);
    }

    #[test]
    fn a_level_with_a_spawn_and_a_navmesh_has_no_notices() {
        let level = sandbox_level(&["player_spawn"]);

        let notices = notices_of(&level);

        assert!(notices.is_empty(), "{notices:?}");
    }

    #[test]
    fn a_navmesh_fallback_has_its_notice() {
        let mut level = sandbox_level(&["player_spawn"]);
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
}
