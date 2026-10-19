//! The baked arena: its meshes become world colliders, its named markers become
//! spawn, dummy, enemy, waypoint and cover points.

use std::path::Path;

use glam::Vec3;
use moho_level::{Level, LevelError, LevelMarker};
use moho_physics::PhysicsWorld;
use moho_render_api::{WorldMesh, WorldMeshId};

use crate::tag::SurfaceTag;

/// One nominal tick, enough for a step to index freshly added colliders.
pub(crate) const SETTLE_DT: f32 = 1.0 / 60.0;

const PLAYER_SPAWN: &str = "player_spawn";
const DUMMY_PREFIX: &str = "dummy_";
const ENEMY_SPAWN_PREFIX: &str = "enemy_spawn_";
const WAYPOINT_PREFIX: &str = "waypoint_";
const COVER_PREFIX: &str = "cover_";

/// Why an arena level cannot be used.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ArenaError {
    /// The level file did not load.
    #[error(transparent)]
    Level(#[from] LevelError),
    /// A marker the game cannot start without is absent.
    #[error("arena has no `{marker}` marker")]
    MissingMarker { marker: String },
    /// A marker that must be unique appears more than once.
    #[error("arena has more than one `{marker}` marker")]
    DuplicateMarker { marker: String },
    /// A waypoint's suffix is not a number, or repeats an earlier waypoint's number.
    #[error("arena waypoint `{marker}` needs a unique numeric suffix")]
    BadWaypoint { marker: String },
}

/// The arena's markers, grouped by kind. Within a kind, markers keep file order.
#[derive(Debug, Clone, PartialEq)]
pub struct ArenaMarkers {
    /// Where the player starts; exactly one.
    pub player_spawn: LevelMarker,
    /// `dummy_NN`: training dummy positions; at least one.
    pub dummies: Vec<LevelMarker>,
    /// `enemy_spawn_NN`.
    pub enemy_spawns: Vec<LevelMarker>,
    /// `waypoint_NN`.
    pub waypoints: Vec<LevelMarker>,
    /// `cover_NN`.
    pub covers: Vec<LevelMarker>,
}

impl ArenaMarkers {
    /// Sorts `level`'s markers by kind.
    ///
    /// # Errors
    /// [`ArenaError::MissingMarker`] naming `player_spawn` or `dummy_NN`;
    /// [`ArenaError::DuplicateMarker`] naming `player_spawn`;
    /// [`ArenaError::MissingMarker`] naming `waypoint_NN` when there is an enemy spawn and
    /// no waypoint to patrol; [`ArenaError::BadWaypoint`] naming a waypoint whose suffix is not
    /// a number or repeats an earlier one's value.
    pub fn from_level(level: &Level) -> Result<Self, ArenaError> {
        let mut spawns = Vec::new();
        let mut dummies = Vec::new();
        let mut enemy_spawns = Vec::new();
        let mut waypoints = Vec::new();
        let mut covers = Vec::new();
        for marker in &level.markers {
            let name = marker.name.as_str();
            let kind = if name == PLAYER_SPAWN {
                &mut spawns
            } else if name.starts_with(DUMMY_PREFIX) {
                &mut dummies
            } else if name.starts_with(ENEMY_SPAWN_PREFIX) {
                &mut enemy_spawns
            } else if name.starts_with(WAYPOINT_PREFIX) {
                &mut waypoints
            } else if name.starts_with(COVER_PREFIX) {
                &mut covers
            } else {
                continue;
            };
            kind.push(marker.clone());
        }
        let marker = PLAYER_SPAWN.to_owned();
        let mut spawns = spawns.into_iter();
        let player_spawn = spawns.next().ok_or(ArenaError::MissingMarker {
            marker: marker.clone(),
        })?;
        if spawns.next().is_some() {
            return Err(ArenaError::DuplicateMarker { marker });
        }
        if dummies.is_empty() {
            return Err(ArenaError::MissingMarker {
                marker: format!("{DUMMY_PREFIX}NN"),
            });
        }
        if !enemy_spawns.is_empty() && waypoints.is_empty() {
            return Err(ArenaError::MissingMarker {
                marker: format!("{WAYPOINT_PREFIX}NN"),
            });
        }
        let mut numbers = Vec::new();
        for marker in &waypoints {
            let number = waypoint_number(marker).filter(|n| !numbers.contains(n));
            numbers.push(number.ok_or_else(|| ArenaError::BadWaypoint {
                marker: marker.name.clone(),
            })?);
        }
        Ok(Self {
            player_spawn,
            dummies,
            enemy_spawns,
            waypoints,
            covers,
        })
    }
}

/// The `NN` of a `waypoint_NN` marker, `None` when it is not a number.
fn waypoint_number(marker: &LevelMarker) -> Option<u32> {
    marker.name[WAYPOINT_PREFIX.len()..].parse().ok()
}

impl ArenaMarkers {
    /// The patrol loop: every `waypoint_NN` position, in numeric order of `NN`.
    pub fn patrol_route(&self) -> Vec<Vec3> {
        let mut waypoints: Vec<&LevelMarker> = self.waypoints.iter().collect();
        waypoints.sort_by_key(|marker| waypoint_number(marker));
        waypoints
            .into_iter()
            .map(|marker| marker.position)
            .collect()
    }
}

/// Everything a sim needs to start, and to start again: static geometry plus spawn points.
/// The sim holds it behind an `Arc`, so a restart rebuilds from the same data.
#[derive(Debug, Clone)]
pub struct Arena {
    pub meshes: Vec<WorldMesh>,
    /// Floor point the player starts on.
    pub player_spawn: Vec3,
    /// Floor points of the training dummies.
    pub dummy_spawns: Vec<Vec3>,
    /// Floor points of the enemies.
    pub enemy_spawns: Vec<Vec3>,
    /// The loop every enemy patrols, in order.
    pub route: Vec<Vec3>,
    pub covers: Vec<Vec3>,
}

impl Arena {
    /// The arena `level` and its `markers` describe.
    pub fn from_level(level: &Level, markers: &ArenaMarkers) -> Self {
        let positions = |list: &[LevelMarker]| list.iter().map(|marker| marker.position).collect();
        Self {
            meshes: level.meshes.iter().map(|m| m.mesh.clone()).collect(),
            player_spawn: markers.player_spawn.position,
            dummy_spawns: positions(&markers.dummies),
            enemy_spawns: positions(&markers.enemy_spawns),
            route: markers.patrol_route(),
            covers: positions(&markers.covers),
        }
    }

    /// A physics world whose static colliders are this arena's meshes, tagged
    /// [`SurfaceTag::World`], stepped once so they are queryable.
    pub fn physics_world(&self) -> PhysicsWorld {
        let mut world = PhysicsWorld::new();
        for (index, mesh) in self.meshes.iter().enumerate() {
            world.set_world_mesh(arena_mesh_id(index), mesh, SurfaceTag::World.into());
        }
        world.step(SETTLE_DT);
        world
    }
}

/// Reads the level at `path` and its markers.
///
/// # Errors
/// [`ArenaError::Level`] when the file does not load, else as [`ArenaMarkers::from_level`].
pub fn load(path: &Path) -> Result<(Level, ArenaMarkers), ArenaError> {
    let level = moho_level::load(path)?;
    let markers = ArenaMarkers::from_level(&level)?;
    Ok((level, markers))
}

/// A physics world whose static colliders are `level`'s meshes, all tagged
/// [`SurfaceTag::World`](crate::tag::SurfaceTag::World).
///
/// The world is stepped once so the colliders are queryable before the first tick.
pub fn physics_world(level: &Level) -> PhysicsWorld {
    let mut world = PhysicsWorld::new();
    for (index, level_mesh) in level.meshes.iter().enumerate() {
        world.set_world_mesh(
            arena_mesh_id(index),
            &level_mesh.mesh,
            SurfaceTag::World.into(),
        );
    }
    world.step(SETTLE_DT);
    world
}

/// The id under which the physics world and the renderer both know `level.meshes[index]`.
pub fn arena_mesh_id(index: usize) -> WorldMeshId {
    WorldMeshId(index as u64)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    use glam::{Quat, Vec3};
    use moho_level::LevelMesh;
    use moho_physics::RayQuery;

    use super::*;
    use crate::tag::SurfaceTag;
    use crate::test_support::box_mesh;

    fn marker(name: &str, position: Vec3) -> LevelMarker {
        LevelMarker {
            name: name.to_owned(),
            position,
            rotation: Quat::IDENTITY,
            properties: BTreeMap::new(),
        }
    }

    fn level_of(names: &[&str]) -> Level {
        Level {
            meshes: Vec::new(),
            markers: names
                .iter()
                .enumerate()
                .map(|(i, n)| marker(n, Vec3::new(i as f32, 0.0, 0.0)))
                .collect(),
        }
    }

    fn shipped_arena() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/arena.glb")
    }

    #[test]
    fn the_shipped_arena_has_one_player_spawn_and_dummies() {
        let (level, markers) = load(&shipped_arena()).expect("the shipped arena loads");

        assert_eq!(markers.player_spawn.name, "player_spawn");
        assert!(!markers.dummies.is_empty(), "no dummies");
        assert!(!markers.enemy_spawns.is_empty(), "no enemy spawns");
        assert!(!markers.waypoints.is_empty(), "no waypoints");
        assert!(!markers.covers.is_empty(), "no cover points");
        assert!(!level.meshes.is_empty(), "no geometry");
        let player_spawns = level
            .markers
            .iter()
            .filter(|m| m.name == "player_spawn")
            .count();
        assert_eq!(player_spawns, 1);
    }

    #[test]
    fn the_shipped_arena_has_enough_enemy_spawns_for_the_first_wave() {
        let (_, markers) = load(&shipped_arena()).expect("the shipped arena loads");

        let first_wave = crate::wave::WaveTuning::default().first_size() as usize;

        assert!(
            markers.enemy_spawns.len() >= first_wave,
            "{} enemy spawns for a first wave of {first_wave}",
            markers.enemy_spawns.len()
        );
    }

    #[test]
    fn the_shipped_arena_has_an_enemy_spawn_and_a_waypoint_loop() {
        let (_, markers) = load(&shipped_arena()).expect("the shipped arena loads");

        let route = markers.patrol_route();

        assert!(!markers.enemy_spawns.is_empty(), "no enemy spawn");
        assert!(route.len() >= 3, "a loop needs three waypoints: {route:?}");
        let numbered: Vec<Vec3> = (1..=route.len())
            .map(|n| {
                let name = format!("waypoint_{n:02}");
                markers
                    .waypoints
                    .iter()
                    .find(|m| m.name == name)
                    .unwrap_or_else(|| panic!("no {name}"))
                    .position
            })
            .collect();
        assert_eq!(route, numbered);
    }

    #[test]
    fn an_enemy_spawn_without_waypoints_is_an_error_naming_it() {
        let level = level_of(&["player_spawn", "dummy_01", "enemy_spawn_01"]);

        let error = ArenaMarkers::from_level(&level).unwrap_err();

        assert!(
            matches!(&error, ArenaError::MissingMarker { marker } if marker.starts_with("waypoint_")),
            "{error:?}"
        );
        assert!(error.to_string().contains("waypoint_"));
    }

    #[test]
    fn waypoints_without_enemy_spawns_are_not_an_error() {
        let level = level_of(&["player_spawn", "dummy_01", "waypoint_01"]);

        let markers = ArenaMarkers::from_level(&level);

        assert!(markers.is_ok(), "{markers:?}");
    }

    #[test]
    fn the_patrol_route_follows_waypoint_numbers_not_file_order() {
        let mut level = level_of(&["player_spawn", "dummy_01", "enemy_spawn_01"]);
        for (name, x) in [
            ("waypoint_10", 10.0),
            ("waypoint_02", 2.0),
            ("waypoint_01", 1.0),
        ] {
            level.markers.push(marker(name, Vec3::new(x, 0.0, 0.0)));
        }

        let markers = ArenaMarkers::from_level(&level).expect("a valid arena");

        let xs: Vec<f32> = markers.patrol_route().iter().map(|p| p.x).collect();
        assert_eq!(xs, [1.0, 2.0, 10.0]);
    }

    #[test]
    fn a_waypoint_with_a_non_numeric_suffix_is_an_error_naming_it() {
        let level = level_of(&[
            "player_spawn",
            "dummy_01",
            "enemy_spawn_01",
            "waypoint_01",
            "waypoint_a",
        ]);

        let error = ArenaMarkers::from_level(&level).unwrap_err();

        assert!(error.to_string().contains("waypoint_a"), "{error}");
    }

    #[test]
    fn two_waypoints_with_the_same_number_are_an_error_naming_the_later_one() {
        let level = level_of(&[
            "player_spawn",
            "dummy_01",
            "enemy_spawn_01",
            "waypoint_1",
            "waypoint_01",
        ]);

        let error = ArenaMarkers::from_level(&level).unwrap_err();

        assert!(error.to_string().contains("waypoint_01"), "{error}");
    }

    #[test]
    fn missing_or_duplicate_spawn_is_an_error_naming_it() {
        let none = level_of(&["dummy_01"]);
        let twice = level_of(&["player_spawn", "player_spawn", "dummy_01"]);

        let missing = ArenaMarkers::from_level(&none).unwrap_err();
        let duplicate = ArenaMarkers::from_level(&twice).unwrap_err();

        assert!(
            matches!(&missing, ArenaError::MissingMarker { marker } if marker == "player_spawn"),
            "{missing:?}"
        );
        assert!(missing.to_string().contains("player_spawn"));
        assert!(
            matches!(&duplicate, ArenaError::DuplicateMarker { marker } if marker == "player_spawn"),
            "{duplicate:?}"
        );
        assert!(duplicate.to_string().contains("player_spawn"));
    }

    #[test]
    fn an_arena_without_dummies_is_an_error_naming_them() {
        let level = level_of(&["player_spawn", "enemy_spawn_01"]);

        let error = ArenaMarkers::from_level(&level).unwrap_err();

        assert!(
            matches!(&error, ArenaError::MissingMarker { marker } if marker.starts_with("dummy_")),
            "{error:?}"
        );
        assert!(error.to_string().contains("dummy_"));
    }

    #[test]
    fn markers_are_kept_by_kind_in_file_order() {
        let level = level_of(&[
            "waypoint_02",
            "dummy_02",
            "player_spawn",
            "cover_01",
            "enemy_spawn_01",
            "dummy_01",
            "waypoint_01",
            "some_decoration",
            "enemy_spawn_02",
        ]);

        let markers = ArenaMarkers::from_level(&level).expect("a valid arena");

        let names =
            |list: &[LevelMarker]| -> Vec<String> { list.iter().map(|m| m.name.clone()).collect() };
        assert_eq!(markers.player_spawn.name, "player_spawn");
        assert_eq!(names(&markers.dummies), ["dummy_02", "dummy_01"]);
        assert_eq!(names(&markers.waypoints), ["waypoint_02", "waypoint_01"]);
        assert_eq!(
            names(&markers.enemy_spawns),
            ["enemy_spawn_01", "enemy_spawn_02"]
        );
        assert_eq!(names(&markers.covers), ["cover_01"]);
    }

    #[test]
    fn level_meshes_become_colliders_tagged_world() {
        let level = Level {
            meshes: vec![LevelMesh {
                mesh: box_mesh(Vec3::new(-1.0, 0.0, 4.0), Vec3::new(1.0, 2.0, 5.0)),
                material: None,
            }],
            markers: Vec::new(),
        };

        let mut world = physics_world(&level);
        world.step(1.0 / 60.0);
        let hit = world.cast_ray(&RayQuery {
            origin: Vec3::new(0.0, 1.0, 0.0),
            direction: Vec3::Z,
            max_distance: 20.0,
            ignore: None,
        });

        let hit = hit.expect("the box stops the ray");
        assert_eq!(SurfaceTag::try_from(hit.tag), Ok(SurfaceTag::World));
        assert!((hit.distance - 4.0).abs() < 1e-3, "{hit:?}");
    }
}

#[cfg(test)]
mod shipped_cover_tests {
    use std::path::PathBuf;

    use glam::Vec3;
    use moho_physics::RayQuery;

    use super::*;

    /// A standing body's centre, and the eye of a 2.3 m tall character.
    const BODY_HEIGHT: f32 = 1.15;
    const EYE_HEIGHT: f32 = 1.85;
    /// How far out on the shielded side the test rays start.
    const REACH: f32 = 7.0;

    fn shielded_from(marker: &LevelMarker) -> Vec3 {
        let direction = marker
            .properties
            .get("shielded_from")
            .unwrap_or_else(|| panic!("{} has no `shielded_from`", marker.name));
        match direction.as_str() {
            "+x" => Vec3::X,
            "-x" => Vec3::NEG_X,
            "+z" => Vec3::Z,
            "-z" => Vec3::NEG_Z,
            other => panic!("{}: unknown direction `{other}`", marker.name),
        }
    }

    /// The distance at which a ray from `height` above the point `REACH` out on the shielded
    /// side of `cover` first meets geometry on its way to `cover`.
    fn first_obstacle(world: &PhysicsWorld, cover: &LevelMarker, height: f32) -> Option<f32> {
        let toward = shielded_from(cover);
        let origin = cover.position + toward * REACH + Vec3::Y * height;
        world
            .cast_ray(&RayQuery {
                origin,
                direction: -toward,
                max_distance: REACH,
                ignore: None,
            })
            .map(|hit| hit.distance)
    }

    fn shipped() -> (PhysicsWorld, ArenaMarkers) {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/arena.glb");
        let (level, markers) = load(&path).expect("the shipped arena loads");
        (physics_world(&level), markers)
    }

    #[test]
    fn a_cover_point_stops_a_body_height_ray_and_lets_an_eye_height_ray_over() {
        let (world, markers) = shipped();

        for cover in &markers.covers {
            let at_body = first_obstacle(&world, cover, BODY_HEIGHT);
            let at_eye = first_obstacle(&world, cover, EYE_HEIGHT);

            assert!(
                at_body.is_some(),
                "{}: body height ray is not stopped",
                cover.name
            );
            assert_eq!(at_eye, None, "{}: eye height ray is stopped", cover.name);
        }
    }

    #[test]
    fn every_marker_stands_on_the_floor() {
        let (world, markers) = shipped();
        let all = markers
            .dummies
            .iter()
            .chain(&markers.enemy_spawns)
            .chain(&markers.waypoints)
            .chain(&markers.covers)
            .chain(std::iter::once(&markers.player_spawn));

        for marker in all {
            let down = world.cast_ray(&RayQuery {
                origin: marker.position + Vec3::Y,
                direction: Vec3::NEG_Y,
                max_distance: 2.0,
                ignore: None,
            });

            let hit = down.unwrap_or_else(|| panic!("{}: no floor below", marker.name));
            assert!(
                (hit.distance - 1.0).abs() < 1e-3,
                "{}: {hit:?}",
                marker.name
            );
        }
    }
}
