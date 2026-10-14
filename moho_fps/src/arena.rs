//! The baked arena: its meshes become world colliders, its named markers become
//! spawn, dummy, enemy, waypoint and cover points.

use std::path::Path;

use moho_level::{Level, LevelError, LevelMarker};
use moho_physics::PhysicsWorld;

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
    /// [`ArenaError::DuplicateMarker`] naming `player_spawn`.
    pub fn from_level(_level: &Level) -> Result<Self, ArenaError> {
        unimplemented!("sort the level's markers by kind and validate the required ones")
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
pub fn physics_world(_level: &Level) -> PhysicsWorld {
    unimplemented!("register each level mesh as a world-tagged collider")
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
