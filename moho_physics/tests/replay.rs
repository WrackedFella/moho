//! A recorded run of commands and effects replays bit-for-bit in a fresh world, using only
//! the public API (ADR-0011).
//!
//! The tick loop is fixed: queue the tick's effects, `move_character` for each character in
//! handle-creation order, then `PhysicsWorld::step`.

use glam::{Quat, Vec3};
use moho_physics::{
    CharacterBody, CharacterCommand, CharacterEffect, CharacterHandle, CharacterSpawn,
    CharacterState, HitTag, MoveSettings, PhysicsWorld, RigidBodyHandle,
};
use moho_render_api::{WorldMesh, WorldMeshId};
use serde::{Deserialize, Serialize};

const TICKS: usize = 600;
const DT: f32 = 1.0 / 60.0;
const SEED: u64 = 0x5EED_0502;

/// Effects queued before the tick's moves, then each character's command, in creation order.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct TickInput {
    effects: Vec<(usize, CharacterEffect)>,
    commands: [CharacterCommand; 2],
}

/// Everything the replay compares after one tick.
#[derive(Clone, Copy, Debug)]
struct Snapshot {
    characters: [CharacterState; 2],
    box_translation: Vec3,
    box_rotation: Quat,
}

#[derive(Debug, PartialEq)]
struct Divergence {
    tick: usize,
    entity: &'static str,
    field: &'static str,
}

/// Seeded linear congruential generator, so the script needs no `rand` dependency.
struct Lcg(u64);

impl Lcg {
    fn next_u32(&mut self) -> u32 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (self.0 >> 32) as u32
    }

    /// Uniform in [-1, 1).
    fn signed(&mut self) -> f32 {
        (self.next_u32() >> 8) as f32 / (1u32 << 23) as f32 - 1.0
    }

    fn chance(&mut self, one_in: u32) -> bool {
        self.next_u32().is_multiple_of(one_in)
    }
}

fn settings() -> MoveSettings {
    MoveSettings {
        max_speed: 4.0,
        ground_accel: 40.0,
        friction: 4.0,
        stop_speed: 1.0,
        air_accel: 10.0,
        air_speed_cap: 0.8,
        gravity: None,
        jump_speed: 8.0,
        max_velocity: 90.0,
        autohop: false,
    }
}

fn mesh(positions: Vec<[f32; 3]>, indices: Vec<u32>) -> WorldMesh {
    let n = positions.len();
    WorldMesh::new(
        positions,
        vec![[0.0, 1.0, 0.0]; n],
        vec![1.0; n],
        vec![[0.0; 3]; n],
        vec![1.0; n],
        vec![0; n],
        indices,
    )
    .expect("the fixed test meshes are well formed")
}

fn floor() -> WorldMesh {
    mesh(
        vec![
            [-12.0, 0.0, -12.0],
            [12.0, 0.0, -12.0],
            [12.0, 0.0, 12.0],
            [-12.0, 0.0, 12.0],
        ],
        vec![0, 2, 1, 0, 3, 2],
    )
}

/// A 0.3 m high slab across the floor at z in [1, 2].
fn step_slab() -> WorldMesh {
    mesh(
        vec![
            [-12.0, 0.0, 1.0],
            [12.0, 0.0, 1.0],
            [12.0, 0.3, 1.0],
            [-12.0, 0.3, 1.0],
            [-12.0, 0.0, 2.0],
            [12.0, 0.0, 2.0],
            [12.0, 0.3, 2.0],
            [-12.0, 0.3, 2.0],
        ],
        vec![
            0, 2, 1, 0, 3, 2, 4, 5, 6, 4, 6, 7, 3, 7, 6, 3, 6, 2, 0, 4, 7, 0, 7, 3, 1, 2, 6, 1, 6,
            5, 0, 1, 5, 0, 5, 4,
        ],
    )
}

/// A wall at `z`, y in [0, 6], facing -Z, from `x_from` to `x_to`.
fn wall_x(z: f32, x_from: f32, x_to: f32) -> WorldMesh {
    mesh(
        vec![
            [x_from, 0.0, z],
            [x_to, 0.0, z],
            [x_to, 6.0, z],
            [x_from, 6.0, z],
        ],
        vec![0, 2, 1, 0, 3, 2],
    )
}

/// A wall at `x`, y in [0, 6], facing -X, from `z_from` to `z_to`.
fn wall_z(x: f32, z_from: f32, z_to: f32) -> WorldMesh {
    mesh(
        vec![
            [x, 0.0, z_from],
            [x, 0.0, z_to],
            [x, 6.0, z_to],
            [x, 6.0, z_from],
        ],
        vec![0, 1, 2, 0, 2, 3],
    )
}

struct Scene {
    world: PhysicsWorld,
    characters: [CharacterHandle; 2],
    cuboid: RigidBodyHandle,
}

/// Floor, a 0.3 m step, a wall corner, two characters and a dynamic cuboid in their path.
fn build_scene() -> Scene {
    let mut world = PhysicsWorld::new();
    let tag = HitTag(1);
    world.set_world_mesh(WorldMeshId(1), &floor(), tag);
    world.set_world_mesh(WorldMeshId(2), &step_slab(), tag);
    world.set_world_mesh(WorldMeshId(3), &wall_x(8.0, -12.0, 4.0), tag);
    world.set_world_mesh(WorldMeshId(4), &wall_z(4.0, 3.0, 8.0), tag);

    let first = world.add_character(CharacterSpawn {
        position: Vec3::new(-0.6, 1.2, -6.0),
        yaw: 0.0,
        pitch: 0.0,
        body: CharacterBody::default(),
    });
    let second = world.add_character(CharacterSpawn {
        position: Vec3::new(2.5, 1.2, -4.0),
        yaw: -0.4,
        pitch: 0.2,
        body: CharacterBody::default(),
    });
    let cuboid = world.add_dynamic_cuboid(Vec3::new(0.0, 0.6, 4.0), 0.5, 0.5, 0.5);
    let body = &mut world.rigid_body_set[cuboid];
    body.set_linvel(Vec3::new(0.8, 0.0, -1.5), true);
    body.set_angvel(Vec3::new(0.0, 1.0, 0.3), true);

    Scene {
        world,
        characters: [first, second],
        cuboid,
    }
}

fn random_effect(rng: &mut Lcg) -> CharacterEffect {
    let kind = rng.next_u32() % 5;
    let vector = |rng: &mut Lcg, scale: f32| {
        Vec3::new(
            rng.signed() * scale,
            rng.signed().abs() * scale,
            rng.signed() * scale,
        )
    };
    match kind {
        0 => CharacterEffect::AddVelocity(vector(rng, 6.0)),
        1 => CharacterEffect::SetVelocity(vector(rng, 5.0)),
        2 | 3 => CharacterEffect::Teleport {
            position: Vec3::new(rng.signed() * 4.0, 1.5, -3.0 + rng.signed() * 4.0),
            keep_velocity: kind == 3,
        },
        _ => CharacterEffect::SetLook {
            yaw: rng.signed() * 1.0,
            pitch: rng.signed() * 2.0,
        },
    }
}

/// Ticks of straight walking into the cuboid before the noise starts.
const APPROACH_TICKS: usize = 200;

/// 600 ticks: both characters first walk forward into the cuboid (one jumping the step), then
/// steer noisily with jump presses, a held-jump stretch and an effect now and then.
fn make_script() -> Vec<TickInput> {
    let mut rng = Lcg(SEED);
    (0..TICKS)
        .map(|tick| {
            let mut commands = [CharacterCommand::default(); 2];
            let mut effects = Vec::new();
            for command in &mut commands {
                if tick < APPROACH_TICKS {
                    command.forward = 1.0;
                    command.jump = tick == 60;
                } else {
                    command.forward = 0.6 + 0.4 * rng.signed();
                    command.right = rng.signed() * 0.5;
                    command.yaw_delta = rng.signed() * 0.03;
                    command.pitch_delta = rng.signed() * 0.02;
                    command.jump = (300..340).contains(&tick) || rng.chance(40);
                }
            }
            if tick >= APPROACH_TICKS && rng.chance(12) {
                effects.push(((rng.next_u32() >> 16) as usize % 2, random_effect(&mut rng)));
            }
            TickInput { effects, commands }
        })
        .collect()
}

fn snapshot(scene: &Scene) -> Snapshot {
    let [first, second] = scene.characters.map(|handle| {
        scene
            .world
            .character(handle)
            .expect("characters are never removed")
    });
    let body = &scene.world.rigid_body_set[scene.cuboid];
    Snapshot {
        characters: [first, second],
        box_translation: body.position().translation,
        box_rotation: body.position().rotation,
    }
}

/// Runs `script` in a fresh scene; element `t` is the state after tick `t`.
fn run(script: &[TickInput]) -> Vec<Snapshot> {
    let mut scene = build_scene();
    let settings = settings();
    script
        .iter()
        .map(|tick| {
            for &(who, effect) in &tick.effects {
                scene
                    .world
                    .queue_effect(scene.characters[who], effect)
                    .expect("characters are never removed");
            }
            for (handle, command) in scene.characters.iter().zip(&tick.commands) {
                scene
                    .world
                    .move_character(*handle, command, &settings, DT)
                    .expect("characters are never removed");
            }
            scene.world.step(DT);
            snapshot(&scene)
        })
        .collect()
}

fn bits3(v: Vec3) -> [u32; 3] {
    v.to_array().map(f32::to_bits)
}

/// Every compared field of one character, as raw bits.
fn character_fields(state: &CharacterState) -> [(&'static str, [u32; 3]); 7] {
    [
        ("position", bits3(state.position)),
        ("velocity", bits3(state.velocity)),
        ("yaw", [state.yaw.to_bits(), 0, 0]),
        ("pitch", [state.pitch.to_bits(), 0, 0]),
        ("grounded", [u32::from(state.grounded), 0, 0]),
        ("jump_held", [u32::from(state.jump_held), 0, 0]),
        ("eye", bits3(state.eye)),
    ]
}

fn first_divergence(recorded: &[Snapshot], replayed: &[Snapshot]) -> Option<Divergence> {
    assert_eq!(recorded.len(), replayed.len(), "the runs differ in length");
    for (tick, (a, b)) in recorded.iter().zip(replayed).enumerate() {
        let entities = [("character 0", 0), ("character 1", 1)];
        for (entity, index) in entities {
            let left = character_fields(&a.characters[index]);
            let right = character_fields(&b.characters[index]);
            for ((field, x), (_, y)) in left.iter().zip(&right) {
                if x != y {
                    return Some(Divergence {
                        tick,
                        entity,
                        field,
                    });
                }
            }
        }
        if bits3(a.box_translation) != bits3(b.box_translation) {
            return Some(Divergence {
                tick,
                entity: "box",
                field: "translation",
            });
        }
        if a.box_rotation.to_array().map(f32::to_bits)
            != b.box_rotation.to_array().map(f32::to_bits)
        {
            return Some(Divergence {
                tick,
                entity: "box",
                field: "rotation",
            });
        }
    }
    None
}

fn encode(script: &[TickInput]) -> Vec<u8> {
    postcard::to_allocvec(script).expect("the script serialises")
}

fn decode(bytes: &[u8]) -> Vec<TickInput> {
    postcard::from_bytes(bytes).expect("the encoded script reads back")
}

fn flip_lowest_bit(value: f32) -> f32 {
    f32::from_bits(value.to_bits() ^ 1)
}

fn flip_vec(v: Vec3) -> Vec3 {
    Vec3::new(
        flip_lowest_bit(v.x),
        flip_lowest_bit(v.y),
        flip_lowest_bit(v.z),
    )
}

/// The same effect with the lowest mantissa bit of every component flipped; one component
/// alone can vanish in the rounding of the sum it joins.
fn one_ulp_off(effect: CharacterEffect) -> CharacterEffect {
    match effect {
        CharacterEffect::AddVelocity(v) => CharacterEffect::AddVelocity(flip_vec(v)),
        CharacterEffect::SetVelocity(v) => CharacterEffect::SetVelocity(flip_vec(v)),
        CharacterEffect::Teleport {
            position,
            keep_velocity,
        } => CharacterEffect::Teleport {
            position: flip_vec(position),
            keep_velocity,
        },
        CharacterEffect::SetLook { yaw, pitch } => CharacterEffect::SetLook {
            yaw: flip_lowest_bit(yaw),
            pitch: flip_lowest_bit(pitch),
        },
    }
}

fn effects_of(script: &[TickInput]) -> impl Iterator<Item = &CharacterEffect> {
    script.iter().flat_map(|t| t.effects.iter().map(|(_, e)| e))
}

#[test]
fn a_recorded_run_replays_bit_for_bit_on_every_tick() {
    let script = make_script();
    let recorded = run(&script);
    let decoded = decode(&encode(&script));

    let replayed = run(&decoded);

    assert_eq!(decoded, script);
    assert_eq!(
        first_divergence(&recorded, &replayed),
        None,
        "the replay diverged from the recording"
    );
    assert_run_exercises_the_world(&script, &recorded);
}

/// Guards the comparison against passing because nothing happened.
fn assert_run_exercises_the_world(script: &[TickInput], run: &[Snapshot]) {
    assert_eq!(run.len(), TICKS);
    let kinds = |pick: fn(&CharacterEffect) -> bool| effects_of(script).any(pick);
    assert!(kinds(|e| matches!(e, CharacterEffect::AddVelocity(_))));
    assert!(kinds(|e| matches!(e, CharacterEffect::SetVelocity(_))));
    assert!(kinds(|e| matches!(
        e,
        CharacterEffect::Teleport {
            keep_velocity: true,
            ..
        }
    )));
    assert!(kinds(|e| matches!(
        e,
        CharacterEffect::Teleport {
            keep_velocity: false,
            ..
        }
    )));
    assert!(kinds(|e| matches!(e, CharacterEffect::SetLook { .. })));

    for index in 0..2 {
        let positions: Vec<Vec3> = run.iter().map(|s| s.characters[index].position).collect();
        let travelled: f32 = positions.windows(2).map(|w| w[0].distance(w[1])).sum();
        assert!(
            travelled > 10.0,
            "character {index} barely moved: {travelled}"
        );
        assert!(
            run.iter()
                .any(|s| !s.characters[index].grounded && s.characters[index].velocity.y > 1.0),
            "character {index} never jumped"
        );
        assert!(
            run.iter().any(|s| s.characters[index].grounded),
            "character {index} never stood on the floor"
        );
    }

    let start = run[0].box_translation;
    let moved = run
        .iter()
        .map(|s| (s.box_translation - start).length())
        .fold(0.0, f32::max);
    assert!(moved > 0.1, "the box never moved: {moved}");
    assert!(
        run.iter().any(|s| s.box_rotation != run[0].box_rotation),
        "the box never turned"
    );
}

#[test]
fn a_tick_of_commands_and_effects_round_trips_through_postcard() {
    let tick = TickInput {
        effects: vec![
            (0, CharacterEffect::AddVelocity(Vec3::new(1.0, -2.5, 0.125))),
            (1, CharacterEffect::SetVelocity(Vec3::new(-3.0, 4.0, 5.5))),
            (
                0,
                CharacterEffect::Teleport {
                    position: Vec3::new(1.5, 2.0, -3.25),
                    keep_velocity: true,
                },
            ),
            (
                1,
                CharacterEffect::Teleport {
                    position: Vec3::new(-1.5, 0.5, 9.0),
                    keep_velocity: false,
                },
            ),
            (
                0,
                CharacterEffect::SetLook {
                    yaw: -0.75,
                    pitch: 1.6,
                },
            ),
        ],
        commands: [
            CharacterCommand {
                forward: 1.0,
                right: -0.5,
                yaw_delta: 0.01,
                pitch_delta: -0.02,
                jump: true,
            },
            CharacterCommand::default(),
        ],
    };

    let back = decode(&encode(std::slice::from_ref(&tick)));

    assert_eq!(back, vec![tick]);
}

#[test]
fn a_one_ulp_change_to_one_effect_breaks_the_replay() {
    let script = make_script();
    let recorded = run(&script);
    let (tick, slot) = script
        .iter()
        .enumerate()
        .skip(100)
        .find_map(|(tick, t)| {
            let slot = t
                .effects
                .iter()
                .position(|(_, e)| matches!(e, CharacterEffect::AddVelocity(_)))?;
            Some((tick, slot))
        })
        .expect("the script has an AddVelocity effect after tick 100");
    let mut altered = script.clone();
    altered[tick].effects[slot].1 = one_ulp_off(altered[tick].effects[slot].1);

    let replayed = run(&altered);

    let divergence = first_divergence(&recorded, &replayed)
        .expect("a one-ulp change to a recorded effect must show in the replay");
    assert!(
        divergence.tick >= tick,
        "diverged at tick {} before the altered effect at tick {tick}",
        divergence.tick
    );
}
