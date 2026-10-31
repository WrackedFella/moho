//! The baked arena: its meshes become world colliders, its named markers become
//! spawn, dummy, enemy, waypoint and cover points.

use std::path::Path;
use std::sync::Arc;

use glam::{Quat, Vec3};
use moho_level::{AgentProfile, Level, LevelError, LevelMarker, NavBuildError, NavMesh};
use moho_physics::PhysicsWorld;
use moho_render_api::{WorldMesh, WorldMeshId};

use crate::control::ControlAction;
use crate::sim::{CAPSULE_RADIUS, STAND_HEIGHT};
use crate::tag::SurfaceTag;

/// One nominal tick, enough for a step to index freshly added colliders.
pub(crate) const SETTLE_DT: f32 = 1.0 / 60.0;

const LEVEL_MARKER: &str = "searchlight_level";
const PLAYER_SPAWN: &str = "player_spawn";
const DUMMY_PREFIX: &str = "dummy_";
const ENEMY_SPAWN_PREFIX: &str = "enemy_spawn_";
const WAYPOINT_PREFIX: &str = "waypoint_";
const COVER_PREFIX: &str = "cover_";
const CONTROL_PREFIX: &str = "control_";
const CONTROL_ACTION_KEY: &str = "action";
/// First control mesh id; see [`control_mesh_id`].
const CONTROL_MESH_BASE: u64 = 3 << 32;
/// Width, height and depth of a control's post, in metres.
const CONTROL_POST_SIZE: Vec3 = Vec3::new(0.4, 1.2, 0.4);

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
    /// The walkable area could not be built from the level's meshes.
    #[error(transparent)]
    Nav(#[from] NavBuildError),
    /// The arena has enemy spawns, but no enemy spawn has a path to any waypoint.
    #[error("no path from any enemy spawn reaches a waypoint")]
    NoReachableWaypoint,
    /// A `control_NN` marker names no action, or one not on the list.
    #[error("arena control `{marker}` needs an `action` on the list")]
    BadControl { marker: String },
    /// The `searchlight_level` marker's `mode` is missing or is neither `arena` nor `sandbox`.
    #[error("level mode `{value}` is not `arena` or `sandbox`")]
    BadMode { value: String },
}

/// What a level plays as: waves and a score, or a sandbox with neither.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LevelMode {
    Arena,
    Sandbox,
}

/// The arena's markers, grouped by kind. Within a kind, markers keep file order.
#[derive(Debug, Clone, PartialEq)]
pub struct ArenaMarkers {
    /// How the level plays, from its `searchlight_level` marker.
    pub mode: LevelMode,
    /// Where the player starts; exactly one in an arena, at most one in a sandbox.
    pub player_spawn: Option<LevelMarker>,
    /// `dummy_NN`: training dummy positions; at least one in an arena.
    pub dummies: Vec<LevelMarker>,
    /// `enemy_spawn_NN`.
    pub enemy_spawns: Vec<LevelMarker>,
    /// `waypoint_NN`.
    pub waypoints: Vec<LevelMarker>,
    /// `cover_NN`.
    pub covers: Vec<LevelMarker>,
    /// `control_NN`: placed posts the player can use.
    pub controls: Vec<LevelMarker>,
}

impl ArenaMarkers {
    /// Sorts `level`'s markers by kind. A file with no `searchlight_level` marker is a sandbox.
    ///
    /// An arena needs a `player_spawn`, a `dummy_NN`, and a `waypoint_NN` when it has an enemy
    /// spawn, and its waypoint suffixes must be unique numbers. A sandbox needs none of these:
    /// its markers are only what a borrowed map happens to name.
    ///
    /// # Errors
    /// [`ArenaError::DuplicateMarker`] naming `searchlight_level` or `player_spawn`;
    /// [`ArenaError::BadMode`] when the `searchlight_level` marker's `mode` is missing or unknown.
    /// In an arena also: [`ArenaError::MissingMarker`] naming `player_spawn`, `dummy_NN` or
    /// `waypoint_NN`; [`ArenaError::BadWaypoint`] naming a waypoint whose suffix is not
    /// a number or repeats an earlier one's value.
    pub fn from_level(level: &Level) -> Result<Self, ArenaError> {
        let mode = level_mode(level)?;
        let mut spawns = Vec::new();
        let mut dummies = Vec::new();
        let mut enemy_spawns = Vec::new();
        let mut waypoints = Vec::new();
        let mut covers = Vec::new();
        let mut controls = Vec::new();
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
            } else if name.starts_with(CONTROL_PREFIX) {
                &mut controls
            } else {
                continue;
            };
            kind.push(marker.clone());
        }
        let marker = PLAYER_SPAWN.to_owned();
        let mut spawns = spawns.into_iter();
        let player_spawn = spawns.next();
        if spawns.next().is_some() {
            return Err(ArenaError::DuplicateMarker { marker });
        }
        if mode == LevelMode::Arena {
            if player_spawn.is_none() {
                return Err(ArenaError::MissingMarker { marker });
            }
            validate_arena_rules(&dummies, &enemy_spawns, &waypoints)?;
        }
        Ok(Self {
            mode,
            player_spawn,
            dummies,
            enemy_spawns,
            waypoints,
            covers,
            controls,
        })
    }
}

/// The marker rules only an arena enforces: a dummy, waypoints for any enemy spawn, and
/// numeric, distinct waypoint suffixes.
fn validate_arena_rules(
    dummies: &[LevelMarker],
    enemy_spawns: &[LevelMarker],
    waypoints: &[LevelMarker],
) -> Result<(), ArenaError> {
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
    for marker in waypoints {
        let number = waypoint_number(marker).filter(|n| !numbers.contains(n));
        numbers.push(number.ok_or_else(|| ArenaError::BadWaypoint {
            marker: marker.name.clone(),
        })?);
    }
    Ok(())
}

/// The mode `level`'s `searchlight_level` marker names; a sandbox when it has none.
fn level_mode(level: &Level) -> Result<LevelMode, ArenaError> {
    let mut found = level.markers.iter().filter(|m| m.name == LEVEL_MARKER);
    let Some(marker) = found.next() else {
        return Ok(LevelMode::Sandbox);
    };
    if found.next().is_some() {
        return Err(ArenaError::DuplicateMarker {
            marker: LEVEL_MARKER.to_owned(),
        });
    }
    match marker.properties.get("mode").map(String::as_str) {
        Some("arena") => Ok(LevelMode::Arena),
        Some("sandbox") => Ok(LevelMode::Sandbox),
        other => Err(ArenaError::BadMode {
            value: other.unwrap_or_default().to_owned(),
        }),
    }
}

/// The `NN` of a `waypoint_NN` marker, `None` when it is not a number.
fn waypoint_number(marker: &LevelMarker) -> Option<u32> {
    marker.name[WAYPOINT_PREFIX.len()..].parse().ok()
}

impl ArenaMarkers {
    /// The patrol loop: every `waypoint_NN` position, in numeric order of `NN`.
    pub fn patrol_route(&self) -> Vec<Vec3> {
        self.ordered_waypoints()
            .into_iter()
            .map(|marker| marker.position)
            .collect()
    }

    fn ordered_waypoints(&self) -> Vec<&LevelMarker> {
        let mut waypoints: Vec<&LevelMarker> = self.waypoints.iter().collect();
        waypoints.sort_by_key(|marker| waypoint_number(marker));
        waypoints
    }
}

/// A post the player can use, standing on a `control_NN` marker.
#[derive(Debug, Clone)]
pub struct ArenaControl {
    pub action: ControlAction,
    pub mesh: WorldMesh,
}

/// Everything a sim needs to start, and to start again: static geometry plus spawn points.
/// The sim holds it behind an `Arc`, so a restart rebuilds from the same data.
#[derive(Debug, Clone)]
pub struct Arena {
    pub meshes: Vec<WorldMesh>,
    /// The glTF material name of each of `meshes`, in the same order.
    pub material_names: Vec<Option<String>>,
    /// Floor point the player starts on.
    pub player_spawn: Vec3,
    /// Floor points of the training dummies.
    pub dummy_spawns: Vec<Vec3>,
    /// Floor points of the enemies.
    pub enemy_spawns: Vec<Vec3>,
    /// The loop every enemy patrols, in order.
    pub route: Vec<Vec3>,
    pub covers: Vec<Vec3>,
    /// The usable posts, in file order; an index is the control's id.
    pub controls: Vec<ArenaControl>,
    /// Where an enemy of [`nav_profile`] size can walk.
    pub nav: Arc<NavMesh>,
    /// Whether the navmesh could not be built and `nav` is empty (sandbox levels only).
    pub nav_fallback: bool,
}

/// Extra metres of clearance on top of the capsule radius, so paths do not graze corners.
const NAV_CLEARANCE: f32 = 0.1;

/// The agent the arena's navmesh is built for: the player-sized capsule.
pub fn nav_profile() -> AgentProfile {
    AgentProfile {
        radius: CAPSULE_RADIUS + NAV_CLEARANCE,
        height: 2.0 * STAND_HEIGHT,
        max_climb: 0.5,
        max_slope: std::f32::consts::FRAC_PI_4,
    }
}

impl Arena {
    /// The arena `level` and its `markers` describe.
    ///
    /// In a sandbox a navmesh that cannot be built is a warning and an empty navmesh
    /// ([`Arena::nav_fallback`]), and an unreachable route is not an error.
    ///
    /// # Errors
    /// [`ArenaError::BadControl`] naming a control marker with no `action` on the list;
    /// [`ArenaError::Nav`] when the walkable area cannot be built (arena only);
    /// [`ArenaError::NoReachableWaypoint`] when there are enemy spawns but no waypoint is
    /// reachable from any of them (arena only). A waypoint no enemy spawn reaches is dropped
    /// from the route with a warning.
    pub fn from_level(level: &Level, markers: &ArenaMarkers) -> Result<Self, ArenaError> {
        let positions = |list: &[LevelMarker]| list.iter().map(|marker| marker.position).collect();
        let meshes: Vec<WorldMesh> = level.meshes.iter().map(|m| m.mesh.clone()).collect();
        let material_names = level.meshes.iter().map(|m| m.material.clone()).collect();
        let controls = markers
            .controls
            .iter()
            .map(|marker| {
                let action = marker
                    .properties
                    .get(CONTROL_ACTION_KEY)
                    .and_then(|name| ControlAction::parse(name))
                    .ok_or_else(|| ArenaError::BadControl {
                        marker: marker.name.clone(),
                    })?;
                Ok(ArenaControl {
                    action,
                    mesh: control_post(marker.position),
                })
            })
            .collect::<Result<Vec<_>, ArenaError>>()?;
        let solids: Vec<WorldMesh> = meshes
            .iter()
            .cloned()
            .chain(controls.iter().map(|control| control.mesh.clone()))
            .collect();
        let (nav, nav_fallback) = match NavMesh::build(&solids, nav_profile()) {
            Ok(nav) => (nav, false),
            Err(error) if markers.mode == LevelMode::Sandbox => {
                tracing::warn!(%error, "no walkable area; the sandbox runs without a navmesh");
                (NavMesh::default(), true)
            }
            Err(error) => return Err(error.into()),
        };
        let nav = Arc::new(nav);
        let enemy_spawns: Vec<Vec3> = positions(&markers.enemy_spawns);
        let mut route = Vec::new();
        for waypoint in markers.ordered_waypoints() {
            let reached = enemy_spawns.is_empty()
                || enemy_spawns
                    .iter()
                    .any(|&spawn| nav.path(spawn, waypoint.position).is_some());
            if reached {
                route.push(waypoint.position);
            } else {
                tracing::warn!(
                    waypoint = waypoint.name,
                    "no path reaches the waypoint; the patrol skips it"
                );
            }
        }
        if markers.mode == LevelMode::Arena && !enemy_spawns.is_empty() && route.is_empty() {
            return Err(ArenaError::NoReachableWaypoint);
        }
        Ok(Self {
            meshes,
            material_names,
            nav,
            nav_fallback,
            player_spawn: markers
                .player_spawn
                .as_ref()
                .map_or(Vec3::ZERO, |marker| marker.position),
            dummy_spawns: positions(&markers.dummies),
            enemy_spawns,
            route,
            covers: positions(&markers.covers),
            controls,
        })
    }

    /// A physics world whose static colliders are this arena's meshes, tagged
    /// [`SurfaceTag::World`], and its control posts, tagged [`SurfaceTag::Control`] with their
    /// index, stepped once so they are queryable.
    pub fn physics_world(&self) -> PhysicsWorld {
        let mut world = PhysicsWorld::new();
        for (index, mesh) in self.meshes.iter().enumerate() {
            world.set_world_mesh(arena_mesh_id(index), mesh, SurfaceTag::World.into());
        }
        for (index, control) in self.controls.iter().enumerate() {
            let tag = SurfaceTag::Control(
                u16::try_from(index).expect("an arena has fewer than 65536 controls"),
            );
            world.set_world_mesh(control_mesh_id(index), &control.mesh, tag.into());
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

/// The id under which the physics world and the renderer both know control `index`, clear of
/// the arena's, the dummies' and the enemies' ids.
pub fn control_mesh_id(index: usize) -> WorldMeshId {
    WorldMeshId(CONTROL_MESH_BASE + index as u64)
}

/// A closed box with outward-facing triangles and a normal per face.
pub(crate) fn box_mesh(centre: Vec3, half: Vec3) -> WorldMesh {
    oriented_box_mesh(centre, half, Quat::IDENTITY)
}

/// [`box_mesh`] with its axes turned by `rotation` about its centre.
pub(crate) fn oriented_box_mesh(centre: Vec3, half: Vec3, rotation: Quat) -> WorldMesh {
    // (normal, u, v) with u x v = normal, so (-u-v, +u-v, +u+v, -u+v) winds counter-clockwise
    // seen from outside.
    const FACES: [(Vec3, Vec3, Vec3); 6] = [
        (Vec3::X, Vec3::Y, Vec3::Z),
        (Vec3::NEG_X, Vec3::Z, Vec3::Y),
        (Vec3::Y, Vec3::Z, Vec3::X),
        (Vec3::NEG_Y, Vec3::X, Vec3::Z),
        (Vec3::Z, Vec3::X, Vec3::Y),
        (Vec3::NEG_Z, Vec3::Y, Vec3::X),
    ];
    let mut positions = Vec::with_capacity(24);
    let mut normals = Vec::with_capacity(24);
    let mut indices = Vec::with_capacity(36);
    for (normal, u, v) in FACES {
        let first = positions.len() as u32;
        let (du, dv) = (u * half, v * half);
        let face_centre = centre + rotation * (normal * half);
        for corner in [-du - dv, du - dv, du + dv, -du + dv] {
            positions.push((face_centre + rotation * corner).to_array());
            normals.push((rotation * normal).to_array());
        }
        indices.extend([first, first + 1, first + 2, first, first + 2, first + 3]);
    }
    let count = positions.len();
    WorldMesh::new(
        positions,
        normals,
        vec![1.0; count],
        vec![[0.0; 3]; count],
        vec![1.0; count],
        vec![0; count],
        indices,
    )
    .expect("a box has matching channels and whole triangles")
}

/// The post standing on a control's marker.
fn control_post(floor_point: Vec3) -> WorldMesh {
    let half = CONTROL_POST_SIZE / 2.0;
    box_mesh(floor_point + Vec3::Y * half.y, half)
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
    use crate::test_support::{box_mesh, mode_marker};

    fn marker(name: &str, position: Vec3) -> LevelMarker {
        LevelMarker {
            name: name.to_owned(),
            position,
            rotation: Quat::IDENTITY,
            properties: BTreeMap::new(),
        }
    }

    /// A level file whose `searchlight_level` marker says `mode`, with `names` as markers.
    fn level_in_mode(mode: &str, names: &[&str]) -> Level {
        let mut markers = vec![mode_marker(mode)];
        markers.extend(
            names
                .iter()
                .enumerate()
                .map(|(i, n)| marker(n, Vec3::new(i as f32, 0.0, 0.0))),
        );
        Level {
            meshes: Vec::new(),
            markers,
        }
    }

    /// An arena-mode level file with `names` as markers.
    fn level_of(names: &[&str]) -> Level {
        level_in_mode("arena", names)
    }

    fn shipped_arena() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/arena.glb")
    }

    #[test]
    fn the_nav_profile_is_the_player_capsule_with_clearance_and_the_specified_limits() {
        let profile = nav_profile();

        assert!(
            (profile.radius - (CAPSULE_RADIUS + 0.1)).abs() < 1e-6,
            "{profile:?}"
        );
        assert!((profile.height - 1.83).abs() < 1e-6, "{profile:?}");
        assert!((profile.max_climb - 0.5).abs() < 1e-6, "{profile:?}");
        assert!(
            (profile.max_slope - std::f32::consts::FRAC_PI_4).abs() < 1e-6,
            "{profile:?}"
        );
    }

    #[test]
    fn the_shipped_arena_navmesh_joins_every_spawn_and_waypoint() {
        let (level, markers) = load(&shipped_arena()).expect("the shipped arena loads");
        let arena = Arena::from_level(&level, &markers).expect("the shipped arena builds");
        let places: Vec<(String, Vec3)> =
            std::iter::once(("player spawn".to_owned(), arena.player_spawn))
                .chain(
                    arena
                        .enemy_spawns
                        .iter()
                        .enumerate()
                        .map(|(i, p)| (format!("enemy spawn {i}"), *p)),
                )
                .chain(
                    arena
                        .route
                        .iter()
                        .enumerate()
                        .map(|(i, p)| (format!("waypoint {i}"), *p)),
                )
                .collect();

        let unjoined: Vec<&str> = places
            .iter()
            .filter(|(_, place)| arena.nav.path(arena.player_spawn, *place).is_none())
            .map(|(name, _)| name.as_str())
            .collect();
        let legs: Vec<usize> = (0..arena.route.len())
            .filter(|&i| {
                let next = arena.route[(i + 1) % arena.route.len()];
                arena.nav.path(arena.route[i], next).is_none()
            })
            .collect();

        assert_eq!(
            arena.route,
            markers.patrol_route(),
            "load dropped a shipped waypoint no enemy spawn reaches"
        );
        assert!(places.len() > 2, "the arena has too few places to join");
        assert_eq!(
            unjoined,
            Vec::<&str>::new(),
            "no path from the player spawn"
        );
        assert_eq!(
            legs,
            Vec::<usize>::new(),
            "no path from waypoint to the next"
        );
    }

    #[test]
    fn the_shipped_arena_has_one_player_spawn_and_dummies() {
        let (level, markers) = load(&shipped_arena()).expect("the shipped arena loads");

        let spawn = markers.player_spawn.as_ref().expect("a player spawn");
        assert_eq!(spawn.name, "player_spawn");
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
        let spawn = markers.player_spawn.as_ref().expect("a player spawn");
        assert_eq!(spawn.name, "player_spawn");
        assert_eq!(names(&markers.dummies), ["dummy_02", "dummy_01"]);
        assert_eq!(names(&markers.waypoints), ["waypoint_02", "waypoint_01"]);
        assert_eq!(
            names(&markers.enemy_spawns),
            ["enemy_spawn_01", "enemy_spawn_02"]
        );
        assert_eq!(names(&markers.covers), ["cover_01"]);
    }

    fn the_level_marker_of(path: &Path) -> Option<String> {
        let level = moho_level::load(path).expect("the shipped level loads");
        level
            .markers
            .iter()
            .find(|m| m.name == "searchlight_level")
            .and_then(|m| m.properties.get("mode").cloned())
    }

    #[test]
    fn the_shipped_arena_is_in_arena_mode() {
        let (_, markers) = load(&shipped_arena()).expect("the shipped arena loads");

        assert_eq!(
            the_level_marker_of(&shipped_arena()).as_deref(),
            Some("arena")
        );
        assert_eq!(markers.mode, LevelMode::Arena);
    }

    #[test]
    fn the_shipped_proving_ground_is_a_sandbox() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/proving_ground.glb");

        let (level, markers) = load(&path).expect("the shipped proving ground loads");

        assert_eq!(markers.mode, LevelMode::Sandbox);
        assert!(!level.meshes.is_empty(), "no geometry");
        assert!(markers.player_spawn.is_some(), "no firing line spawn");
        assert!(markers.enemy_spawns.is_empty(), "a sandbox attacks no one");
        assert!(markers.waypoints.is_empty());
        assert!(markers.covers.is_empty());
    }

    #[test]
    fn a_file_without_a_level_marker_is_a_sandbox() {
        let level = Level {
            meshes: Vec::new(),
            markers: vec![marker("player_spawn", Vec3::ZERO)],
        };

        let markers = ArenaMarkers::from_level(&level).expect("an unmarked file loads");

        assert_eq!(markers.mode, LevelMode::Sandbox);
    }

    #[test]
    fn a_sandbox_without_player_spawn_starts_at_the_origin() {
        let mut level = level_in_mode("sandbox", &[]);
        level.meshes.push(LevelMesh {
            mesh: box_mesh(Vec3::new(5.0, -1.0, 5.0), Vec3::new(15.0, 0.0, 15.0)),
            material: None,
        });

        let markers = ArenaMarkers::from_level(&level).expect("a sandbox needs no spawn");
        let arena = Arena::from_level(&level, &markers).expect("the sandbox builds");

        assert_eq!(markers.mode, LevelMode::Sandbox);
        assert_eq!(markers.player_spawn, None);
        assert_eq!(arena.player_spawn, Vec3::ZERO);
        assert!(markers.dummies.is_empty());
    }

    #[test]
    fn a_sandbox_with_two_player_spawns_is_an_error() {
        let level = level_in_mode("sandbox", &["player_spawn", "player_spawn"]);

        let error = ArenaMarkers::from_level(&level).unwrap_err();

        assert!(
            matches!(&error, ArenaError::DuplicateMarker { marker } if marker == "player_spawn"),
            "{error:?}"
        );
    }

    #[test]
    fn a_sandbox_with_a_waypoint_a_node_loads() {
        let level = level_in_mode(
            "sandbox",
            &["player_spawn", "enemy_spawn_01", "waypoint_a", "waypoint_b"],
        );

        let markers = ArenaMarkers::from_level(&level).expect("a borrowed node name is fine");

        assert_eq!(markers.mode, LevelMode::Sandbox);
        assert_eq!(markers.waypoints.len(), 2);
    }

    #[test]
    fn a_sandbox_enemy_spawn_without_waypoints_loads_with_an_empty_route() {
        let mut level = level_in_mode("sandbox", &["player_spawn", "enemy_spawn_01"]);
        level.meshes.push(LevelMesh {
            mesh: box_mesh(Vec3::new(-10.0, -1.0, -10.0), Vec3::new(10.0, 0.0, 10.0)),
            material: None,
        });

        let markers = ArenaMarkers::from_level(&level).expect("a sandbox needs no waypoint");
        let arena = Arena::from_level(&level, &markers).expect("the sandbox builds");

        assert_eq!(markers.enemy_spawns.len(), 1);
        assert!(markers.waypoints.is_empty());
        assert!(arena.route.is_empty(), "{:?}", arena.route);
    }

    #[test]
    fn a_sandbox_whose_waypoints_no_spawn_reaches_builds_with_an_empty_route() {
        let mut level = island_level(&[("waypoint_01", ON_ISLAND)]);
        level.markers[0] = mode_marker("sandbox");

        let arena = build(&level).expect("a sandbox does not need a reachable waypoint");

        assert!(
            arena.nav.path(arena.enemy_spawns[0], ON_ISLAND).is_none(),
            "the island must be unreachable for this test to mean anything"
        );
        assert!(arena.route.is_empty(), "{:?}", arena.route);
    }

    #[test]
    fn a_node_named_level_is_not_the_mode_marker() {
        let mut borrowed = marker("level", Vec3::ZERO);
        borrowed
            .properties
            .insert("mode".to_owned(), "arena".to_owned());
        let level = Level {
            meshes: Vec::new(),
            markers: vec![borrowed],
        };

        let markers = ArenaMarkers::from_level(&level).expect("a borrowed group node loads");

        assert_eq!(markers.mode, LevelMode::Sandbox);
    }

    #[test]
    fn two_level_markers_are_an_error() {
        let mut level = level_of(&["player_spawn", "dummy_01"]);
        level.markers.push(mode_marker("sandbox"));

        let error = ArenaMarkers::from_level(&level).unwrap_err();

        assert!(
            matches!(&error, ArenaError::DuplicateMarker { marker } if marker == "searchlight_level"),
            "{error:?}"
        );
    }

    #[test]
    fn an_unknown_mode_is_an_error() {
        let level = level_in_mode("creative", &["player_spawn", "dummy_01"]);

        let error = ArenaMarkers::from_level(&level).unwrap_err();

        assert!(
            matches!(&error, ArenaError::BadMode { value } if value == "creative"),
            "{error:?}"
        );
        assert!(error.to_string().contains("creative"), "{error}");
    }

    #[test]
    fn a_level_marker_without_a_mode_is_an_error() {
        let mut level = level_of(&["player_spawn", "dummy_01"]);
        level.markers[0].properties.clear();

        let error = ArenaMarkers::from_level(&level).unwrap_err();

        assert!(matches!(&error, ArenaError::BadMode { .. }), "{error:?}");
    }

    /// A level whose floor is wider than the navmesh's cell budget allows.
    fn too_wide_for_a_navmesh(mode: &str, names: &[&str]) -> Level {
        let mut level = level_in_mode(mode, names);
        level.meshes.push(LevelMesh {
            mesh: box_mesh(
                Vec3::new(-500.0, -1.0, -500.0),
                Vec3::new(500.0, 0.0, 500.0),
            ),
            material: None,
        });
        level
    }

    #[test]
    fn a_sandbox_whose_navmesh_fails_loads_with_no_walkable_area() {
        let level = too_wide_for_a_navmesh("sandbox", &["player_spawn"]);
        let markers = ArenaMarkers::from_level(&level).expect("a valid sandbox");

        let mut arena = None;
        let log = logged(|| arena = Some(Arena::from_level(&level, &markers)));

        let arena = arena
            .expect("from_level ran")
            .expect("a sandbox survives a navmesh failure");
        assert!(arena.nav_fallback);
        assert!(
            arena
                .nav
                .path(Vec3::new(-5.0, 0.0, 0.0), Vec3::new(5.0, 0.0, 0.0))
                .is_none(),
            "the fallback navmesh walks nowhere"
        );
        assert_eq!(arena.meshes.len(), 1, "the geometry still loads");
        assert!(log.contains("WARN"), "{log}");
    }

    #[test]
    fn an_arena_whose_navmesh_fails_is_an_error() {
        let level = too_wide_for_a_navmesh("arena", &["player_spawn", "dummy_01"]);
        let markers = ArenaMarkers::from_level(&level).expect("valid arena markers");

        let error = Arena::from_level(&level, &markers).unwrap_err();

        assert!(matches!(error, ArenaError::Nav(_)), "{error:?}");
    }

    #[test]
    fn a_level_whose_navmesh_builds_does_not_fall_back() {
        let level = island_level(&[("waypoint_01", ON_FLOOR)]);

        let arena = build(&level).expect("the level builds");

        assert!(!arena.nav_fallback);
    }

    /// A 20 m floor with a 3 m tall island, too high to climb, in one corner.
    /// `waypoints` are `(name, position)` pairs; the enemy spawns on the floor.
    fn island_level(waypoints: &[(&str, Vec3)]) -> Level {
        let solid = |min: Vec3, max: Vec3| LevelMesh {
            mesh: box_mesh(min, max),
            material: None,
        };
        let mut markers = vec![
            mode_marker("arena"),
            marker("player_spawn", Vec3::new(-5.0, 0.0, -5.0)),
            marker("dummy_01", Vec3::new(-5.0, 0.0, 5.0)),
            marker("enemy_spawn_01", Vec3::new(0.0, 0.0, 0.0)),
        ];
        markers.extend(waypoints.iter().map(|&(name, at)| marker(name, at)));
        Level {
            meshes: vec![
                solid(Vec3::new(-10.0, -1.0, -10.0), Vec3::new(10.0, 0.0, 10.0)),
                solid(Vec3::new(4.0, 0.0, -8.0), Vec3::new(8.0, 3.0, -4.0)),
            ],
            markers,
        }
    }

    const ON_FLOOR: Vec3 = Vec3::new(-5.0, 0.0, 5.0);
    const OTHER_ON_FLOOR: Vec3 = Vec3::new(5.0, 0.0, 5.0);
    const ON_ISLAND: Vec3 = Vec3::new(6.0, 3.0, -6.0);

    fn build(level: &Level) -> Result<Arena, ArenaError> {
        let markers = ArenaMarkers::from_level(level).expect("a valid marker set");
        Arena::from_level(level, &markers)
    }

    #[test]
    fn arena_meshes_keep_their_gltf_material_names() {
        let mut level = island_level(&[("waypoint_01", ON_FLOOR)]);
        level.meshes[0].material = Some("floor".to_owned());

        let arena = build(&level).expect("the level builds");

        assert_eq!(
            arena.material_names,
            [Some("floor".to_owned()), None],
            "one name per mesh, in mesh order"
        );
    }

    #[test]
    fn a_waypoint_no_spawn_reaches_is_dropped_from_the_route() {
        let level = island_level(&[
            ("waypoint_03", OTHER_ON_FLOOR),
            ("waypoint_02", ON_ISLAND),
            ("waypoint_01", ON_FLOOR),
        ]);

        let arena = build(&level).expect("two waypoints are reachable");

        let spawn = arena.enemy_spawns[0];
        assert!(
            arena.nav.path(spawn, ON_ISLAND).is_none(),
            "the island must be unreachable for this test to mean anything"
        );
        assert_eq!(arena.route, [ON_FLOOR, OTHER_ON_FLOOR]);
    }

    #[test]
    fn dropping_a_waypoint_warns_with_its_name() {
        let level = island_level(&[
            ("waypoint_01", ON_FLOOR),
            ("waypoint_02", ON_ISLAND),
            ("waypoint_03", OTHER_ON_FLOOR),
        ]);

        let log = logged(|| {
            build(&level).expect("two waypoints are reachable");
        });

        assert!(log.contains("WARN"), "{log}");
        assert!(log.contains("waypoint_02"), "{log}");
        assert!(!log.contains("waypoint_01"), "{log}");
        assert!(!log.contains("waypoint_03"), "{log}");
    }

    #[test]
    fn a_route_with_every_waypoint_reachable_is_kept_whole_and_silent() {
        let level = island_level(&[("waypoint_02", OTHER_ON_FLOOR), ("waypoint_01", ON_FLOOR)]);

        let mut arena = None;
        let log = logged(|| arena = Some(build(&level).expect("both are reachable")));

        assert_eq!(arena.expect("built").route, [ON_FLOOR, OTHER_ON_FLOOR]);
        assert!(!log.contains("WARN"), "{log}");
    }

    #[test]
    fn an_arena_whose_waypoints_no_spawn_reaches_is_an_error() {
        let level = island_level(&[("waypoint_01", ON_ISLAND)]);

        let error = build(&level).unwrap_err();

        assert!(
            matches!(error, ArenaError::NoReachableWaypoint),
            "{error:?}"
        );
    }

    #[test]
    fn waypoints_are_not_checked_when_there_are_no_enemy_spawns() {
        let mut level = island_level(&[("waypoint_01", ON_ISLAND)]);
        level.markers.retain(|m| m.name != "enemy_spawn_01");

        let arena = build(&level).expect("nothing walks the route");

        assert_eq!(arena.route, [ON_ISLAND]);
    }

    /// Collects what `tracing` writes while `run` executes.
    fn logged(run: impl FnOnce()) -> String {
        use std::io::Write;
        use std::sync::Mutex;

        struct Sink(Arc<Mutex<Vec<u8>>>);
        impl Write for Sink {
            fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
                self.0.lock().expect("log sink").extend_from_slice(buf);
                Ok(buf.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }

        let buffer = Arc::new(Mutex::new(Vec::new()));
        let writer = Arc::clone(&buffer);
        let subscriber = tracing_subscriber::fmt()
            .with_writer(move || Sink(Arc::clone(&writer)))
            .with_ansi(false)
            .finish();
        tracing::subscriber::with_default(subscriber, run);
        let bytes = buffer.lock().expect("log sink").clone();
        String::from_utf8(bytes).expect("log output is utf-8")
    }

    /// A 20 m floor, a player spawn, a dummy and one `control_NN` marker per entry of
    /// `controls`: its name, its `action` property (if any) and where it stands.
    fn control_level(controls: &[(&str, Option<&str>, Vec3)]) -> Level {
        let mut markers = vec![
            marker("player_spawn", Vec3::new(-5.0, 0.0, -5.0)),
            marker("dummy_01", Vec3::new(-5.0, 0.0, 5.0)),
        ];
        for &(name, action, at) in controls {
            let mut control = marker(name, at);
            if let Some(action) = action {
                control
                    .properties
                    .insert("action".to_owned(), action.to_owned());
            }
            markers.push(control);
        }
        Level {
            meshes: vec![LevelMesh {
                mesh: box_mesh(Vec3::new(-10.0, -1.0, -10.0), Vec3::new(10.0, 0.0, 10.0)),
                material: None,
            }],
            markers,
        }
    }

    /// The error loading `level` ends in, from either stage.
    fn refusal(level: &Level) -> ArenaError {
        match ArenaMarkers::from_level(level) {
            Err(error) => error,
            Ok(markers) => match Arena::from_level(level, &markers) {
                Err(error) => error,
                Ok(_) => panic!("the level loaded, but it should have been refused"),
            },
        }
    }

    #[test]
    fn a_control_naming_an_unknown_action_is_refused() {
        let level = control_level(&[
            ("control_01", Some("next_wave"), Vec3::new(3.0, 0.0, 3.0)),
            ("control_02", Some("open_door"), Vec3::new(-3.0, 0.0, 3.0)),
        ]);

        let error = refusal(&level);

        assert!(
            matches!(&error, ArenaError::BadControl { marker } if marker == "control_02"),
            "{error:?}"
        );
        assert!(error.to_string().contains("control_02"), "{error}");
    }

    #[test]
    fn a_control_with_no_action_is_refused() {
        let level = control_level(&[
            ("control_01", Some("next_wave"), Vec3::new(3.0, 0.0, 3.0)),
            ("control_02", None, Vec3::new(-3.0, 0.0, 3.0)),
        ]);

        let error = refusal(&level);

        assert!(
            matches!(&error, ArenaError::BadControl { marker } if marker == "control_02"),
            "{error:?}"
        );
        assert!(error.to_string().contains("control_02"), "{error}");
    }

    #[test]
    fn a_control_marker_gives_a_collider_tagged_with_its_index() {
        let level = control_level(&[
            ("control_01", Some("next_wave"), Vec3::new(3.0, 0.0, 3.0)),
            ("control_02", Some("next_wave"), Vec3::new(-3.0, 0.0, 3.0)),
        ]);
        let markers = ArenaMarkers::from_level(&level).expect("a valid marker set");
        let arena = Arena::from_level(&level, &markers).expect("the controls are valid");

        let world = arena.physics_world();
        let strike = |x: f32| {
            world.cast_ray(&RayQuery {
                origin: Vec3::new(x, 0.6, 0.0),
                direction: Vec3::Z,
                max_distance: 20.0,
                ignore: None,
            })
        };
        let (first, second) = (strike(3.0), strike(-3.0));

        assert_eq!(arena.controls.len(), 2);
        assert!(
            arena
                .controls
                .iter()
                .all(|control| control.action == ControlAction::NextWave)
        );
        let first = first.expect("the first post stops the ray");
        let second = second.expect("the second post stops the ray");
        assert_eq!(SurfaceTag::try_from(first.tag), Ok(SurfaceTag::Control(0)));
        assert_eq!(SurfaceTag::try_from(second.tag), Ok(SurfaceTag::Control(1)));
        assert!(
            (first.distance - 2.8).abs() < 1e-2,
            "a 0.4 m post stands on the marker: {first:?}"
        );
    }

    #[test]
    fn the_shipped_arena_has_a_next_wave_control() {
        let (level, markers) = load(&shipped_arena()).expect("the shipped arena loads");

        let arena = Arena::from_level(&level, &markers).expect("the shipped arena builds");

        assert!(
            arena
                .controls
                .iter()
                .any(|control| control.action == ControlAction::NextWave),
            "{} controls",
            arena.controls.len()
        );
    }

    #[test]
    fn the_shipped_arena_has_a_god_mode_switch() {
        let (level, markers) = load(&shipped_arena()).expect("the shipped arena loads");

        let arena = Arena::from_level(&level, &markers).expect("the shipped arena builds");

        let actions: Vec<ControlAction> = arena.controls.iter().map(|c| c.action).collect();
        assert!(
            actions.contains(&ControlAction::ToggleGodMode),
            "{actions:?}"
        );
        assert!(actions.contains(&ControlAction::NextWave), "{actions:?}");
        assert_eq!(actions.len(), 2);
    }

    #[test]
    fn control_mesh_ids_are_distinct_per_control() {
        let ids: Vec<WorldMeshId> = (0..4).map(control_mesh_id).collect();

        for (i, a) in ids.iter().enumerate() {
            for b in &ids[i + 1..] {
                assert_ne!(a, b);
            }
        }
        assert_ne!(control_mesh_id(0), arena_mesh_id(0));
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

    /// A standing body's centre, and the eye of a 1.83 m tall character.
    const BODY_HEIGHT: f32 = 0.915;
    const EYE_HEIGHT: f32 = 1.63;
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
            .chain(&markers.player_spawn);

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
