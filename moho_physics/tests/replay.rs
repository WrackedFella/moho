//! A recorded run of commands and effects replays bit-for-bit in a fresh world, using only
//! the public API (ADR-0011).
//!
//! The tick loop is fixed: queue the tick's effects, `move_character` for each character in
//! handle-creation order, then `PhysicsWorld::step`.

use std::fmt::Write as _;

use glam::{Quat, Vec3};
use moho_physics::{
    CharacterBody, CharacterCommand, CharacterEffect, CharacterHandle, CharacterSpawn,
    CharacterState, HitTag, Hull, HullShape, Lean, LeanReach, LooseId, LooseShape, LooseSpawn,
    LooseState, MoveSettings, PhysicsWorld, RigidBodyHandle, Stance, Stature,
};
use moho_render_api::{WorldMesh, WorldMeshId};
use serde::{Deserialize, Serialize};

const TICKS: usize = 600;
const DT: f32 = 1.0 / 60.0;
/// The cuboid's launch velocity; it slides to rest well before the noisy phase.
const BOX_VELOCITY: Vec3 = Vec3::new(0.5, 0.0, 0.3);
/// Ticks a stand-or-crouch leg takes.
const CROUCH_TICKS: u16 = 12;
/// Ticks a crouch-or-prone leg takes.
const PRONE_TICKS: u16 = 20;
/// Ticks a lean takes from upright to full.
const LEAN_TICKS: u16 = 6;
/// A full upright lean's roll and a full prone lean's, in radians.
const UPRIGHT_ROLL: f32 = 12.0 * std::f32::consts::PI / 180.0;
const PRONE_ROLL: f32 = 20.0 * std::f32::consts::PI / 180.0;
const SEED: u64 = 0x5EED_0502;

/// A loose-object input: a drop, or the removal of an earlier drop. A script cannot name an id
/// the world has not issued yet, so a removal names the index of the spawn, counted in the
/// order the script spawns.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
enum LooseInput {
    Spawn(LooseSpawn),
    Remove(usize),
}

/// Loose inputs and effects applied before the tick's moves, then each character's command, in
/// creation order.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct TickInput {
    loose: Vec<LooseInput>,
    effects: Vec<(usize, CharacterEffect)>,
    commands: [CharacterCommand; 2],
}

#[derive(Clone, Debug)]
struct Snapshot {
    characters: [CharacterState; 2],
    box_translation: Vec3,
    box_rotation: Quat,
    loose: Vec<(LooseId, LooseState)>,
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
        crouch_ticks: CROUCH_TICKS,
        prone_ticks: PRONE_TICKS,
        lean_ticks: LEAN_TICKS,
    }
}

/// The searchlight-sized capsule that can also crouch to a 1.1 m capsule and lie down as a
/// 1.8 m capsule.
fn crouching_body() -> CharacterBody {
    let capsule = |length, eye_height| Stature {
        hull: Hull {
            shape: HullShape::Capsule,
            radius: 0.3,
            length,
        },
        eye_height,
    };
    CharacterBody {
        standing: capsule(1.83, 1.63),
        crouched: Some(capsule(1.1, 0.95)),
        prone: Some(capsule(1.8, 0.35)),
        lean: Lean {
            upright: LeanReach {
                distance: 0.35,
                roll: UPRIGHT_ROLL,
            },
            prone: LeanReach {
                distance: 0.15,
                roll: PRONE_ROLL,
            },
            head_radius: 0.1,
        },
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
fn build_scene(box_velocity: Vec3) -> Scene {
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
        body: crouching_body(),
    });
    let second = world.add_character(CharacterSpawn {
        position: Vec3::new(3.6, 1.2, -4.0),
        yaw: 0.0,
        pitch: 0.2,
        body: crouching_body(),
    });
    let cuboid = world.add_dynamic_cuboid(Vec3::new(0.0, 0.6, 4.0), 0.5, 0.5, 0.5);
    let body = &mut world.rigid_body_set[cuboid];
    body.set_linvel(box_velocity, true);

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

/// Drops character 0 onto the resting cuboid, so contact pushes it.
const BOX_HIT_TICK: usize = 250;
const BOX_HIT: CharacterEffect = CharacterEffect::Teleport {
    position: Vec3::new(0.05, 1.2, 4.03),
    keep_velocity: false,
};

/// Ticks of straight walking into the cuboid before the noise starts.
const APPROACH_TICKS: usize = 200;

/// Crouch and lie down in bursts that overlap the jumps and the walk, with legs cut short so
/// some reverse mid-way; the two characters are out of step. Fixed by tick so the rng draws
/// stay put.
fn wanted_stance(who: usize, tick: usize) -> Stance {
    // Character 0 asks for prone for 24 ticks, long enough to start a prone leg before the
    // crouch burst at 40; character 1 lies down on open ground and stays long enough to settle.
    let prone = match who {
        0 => (16..40).contains(&tick),
        _ => (110..160).contains(&tick),
    };
    let crouching = match who {
        0 => (40..46).contains(&tick) || (63..110).contains(&tick) || (300..420).contains(&tick),
        _ => (tick / 37) % 3 == 1,
    };
    if prone {
        Stance::Prone
    } else if crouching {
        Stance::Crouch
    } else {
        Stance::Stand
    }
}

/// Lean both ways in every stance, in bursts that overlap the stance legs, with a half lean
/// and a mid-lean reversal. Fixed by tick so the rng draws stay put.
fn wanted_lean(who: usize, tick: usize) -> f32 {
    match who {
        0 => match tick {
            20..70 => -1.0,
            120..180 => 1.0,
            310..330 => 0.5,
            _ => 0.0,
        },
        _ => match tick {
            80..130 => 1.0,
            130..150 => -1.0,
            250..300 => 1.0,
            _ => 0.0,
        },
    }
}

/// The dropped capsule's launch velocity; it rolls clear of the cuboid and never leaves the run.
const CAPSULE_VELOCITY: Vec3 = Vec3::new(1.0, 0.0, 0.5);

/// The loose objects of tick `tick`: five drops, four of them on the characters' paths, and two
/// removals of earlier drops.
fn loose_inputs(tick: usize) -> Vec<LooseInput> {
    let drop = |shape, position: Vec3, velocity: Vec3| {
        vec![LooseInput::Spawn(LooseSpawn {
            position,
            rotation: Quat::from_rotation_y(0.4),
            linear_velocity: velocity,
            angular_velocity: Vec3::new(0.0, 0.0, 1.5),
            ..LooseSpawn::new(shape, 0.5)
        })]
    };
    match tick {
        10 => drop(
            LooseShape::Box {
                half_extents: Vec3::new(0.2, 0.15, 0.1),
            },
            Vec3::new(-0.6, 0.6, -3.0),
            Vec3::new(0.4, 0.0, 0.0),
        ),
        30 => drop(
            LooseShape::Capsule {
                half_height: 0.2,
                radius: 0.1,
            },
            Vec3::new(3.6, 0.8, -1.0),
            CAPSULE_VELOCITY,
        ),
        50 => drop(
            LooseShape::Cylinder {
                half_height: 0.15,
                radius: 0.12,
            },
            Vec3::new(-0.3, 0.6, -1.5),
            Vec3::new(0.5, 0.0, 0.2),
        ),
        70 => drop(
            LooseShape::Sphere { radius: 0.2 },
            Vec3::new(0.5, 0.5, -0.5),
            Vec3::new(-0.8, 0.0, 0.3),
        ),
        120 => drop(
            LooseShape::Box {
                half_extents: Vec3::splat(0.15),
            },
            Vec3::new(1.2, 2.0, -2.0),
            Vec3::new(0.0, -1.0, 0.0),
        ),
        200 => vec![LooseInput::Remove(0)],
        350 => vec![LooseInput::Remove(3)],
        _ => Vec::new(),
    }
}

/// Walk toward the cuboid, then steer noisily with jump presses, a held jump and occasional effects.
fn make_script() -> Vec<TickInput> {
    let mut rng = Lcg(SEED);
    (0..TICKS)
        .map(|tick| {
            let mut commands = [CharacterCommand::default(); 2];
            let mut effects = Vec::new();
            for (who, command) in commands.iter_mut().enumerate() {
                command.stance = wanted_stance(who, tick);
                command.lean = wanted_lean(who, tick);
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
            if tick == BOX_HIT_TICK {
                effects.push((0, BOX_HIT));
            }
            TickInput {
                loose: loose_inputs(tick),
                effects,
                commands,
            }
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
        loose: scene.world.loose_states().collect(),
    }
}

/// Runs `script` in a fresh scene; element `t` is the state after tick `t`.
fn run(script: &[TickInput]) -> Vec<Snapshot> {
    run_in(build_scene(BOX_VELOCITY), script)
}

fn run_in(mut scene: Scene, script: &[TickInput]) -> Vec<Snapshot> {
    let settings = settings();
    let mut spawned: Vec<LooseId> = Vec::new();
    script
        .iter()
        .map(|tick| {
            for input in &tick.loose {
                match input {
                    LooseInput::Spawn(spawn) => spawned.push(scene.world.spawn_loose(spawn)),
                    LooseInput::Remove(index) => scene
                        .world
                        .remove_loose(spawned[*index])
                        .expect("a script removes each drop once"),
                }
            }
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
fn character_fields(state: &CharacterState) -> [(&'static str, [u32; 3]); 13] {
    let CharacterState {
        position,
        velocity,
        yaw,
        pitch,
        grounded,
        jump_held,
        eye,
        stance,
        from,
        progress,
        anchor,
        lean,
        roll,
    } = *state;
    [
        ("position", bits3(position)),
        ("velocity", bits3(velocity)),
        ("yaw", [yaw.to_bits(), 0, 0]),
        ("pitch", [pitch.to_bits(), 0, 0]),
        ("grounded", [u32::from(grounded), 0, 0]),
        ("jump_held", [u32::from(jump_held), 0, 0]),
        ("eye", bits3(eye)),
        ("stance", [stance as u32, 0, 0]),
        ("from", [from as u32, 0, 0]),
        ("progress", [u32::from(progress), 0, 0]),
        ("anchor", [anchor as u32, 0, 0]),
        ("lean", [lean.to_bits(), 0, 0]),
        ("roll", [roll.to_bits(), 0, 0]),
    ]
}

/// Every compared field of one loose object but its id and shape, as raw bits.
fn loose_fields(state: &LooseState) -> [(&'static str, [u32; 4]); 5] {
    let vec4 = |v: Vec3| {
        let [x, y, z] = bits3(v);
        [x, y, z, 0]
    };
    [
        ("position", vec4(state.position)),
        ("rotation", state.rotation.to_array().map(f32::to_bits)),
        ("linear_velocity", vec4(state.linear_velocity)),
        ("angular_velocity", vec4(state.angular_velocity)),
        ("asleep", [u32::from(state.asleep), 0, 0, 0]),
    ]
}

/// A shape as raw words: a variant tag, then each dimension's bits.
fn shape_words(shape: LooseShape) -> Vec<u32> {
    match shape {
        LooseShape::Box { half_extents } => [vec![0], bits3(half_extents).to_vec()].concat(),
        LooseShape::Capsule {
            half_height,
            radius,
        } => vec![1, half_height.to_bits(), radius.to_bits()],
        LooseShape::Cylinder {
            half_height,
            radius,
        } => vec![2, half_height.to_bits(), radius.to_bits()],
        LooseShape::Sphere { radius } => vec![3, radius.to_bits()],
    }
}

fn loose_divergence(
    tick: usize,
    recorded: &[(LooseId, LooseState)],
    replayed: &[(LooseId, LooseState)],
) -> Option<Divergence> {
    let at = |field| Divergence {
        tick,
        entity: "loose",
        field,
    };
    if recorded.len() != replayed.len() {
        return Some(at("count"));
    }
    for ((a_id, a), (b_id, b)) in recorded.iter().zip(replayed) {
        if a_id != b_id {
            return Some(at("id"));
        }
        if shape_words(a.shape) != shape_words(b.shape) {
            return Some(at("shape"));
        }
        for ((field, x), (_, y)) in loose_fields(a).iter().zip(&loose_fields(b)) {
            if x != y {
                return Some(at(field));
            }
        }
    }
    None
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
        if let Some(divergence) = loose_divergence(tick, &a.loose, &b.loose) {
            return Some(divergence);
        }
    }
    None
}

/// FNV-1a 64 over the bits `first_divergence` compares: both characters' fields, then the
/// box's translation and rotation, then the loose list: its length, and per object its id,
/// shape and fields. Hand-written so the hash is the same on every platform.
fn state_hash(snapshot: &Snapshot) -> u64 {
    const OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    let loose_words = std::iter::once(snapshot.loose.len() as u32).chain(
        snapshot.loose.iter().flat_map(|(id, state)| {
            let id = id.get();
            [id as u32, (id >> 32) as u32]
                .into_iter()
                .chain(shape_words(state.shape))
                .chain(loose_fields(state).into_iter().flat_map(|(_, bits)| bits))
        }),
    );
    let box_words = bits3(snapshot.box_translation)
        .into_iter()
        .chain(snapshot.box_rotation.to_array().map(f32::to_bits))
        .chain(loose_words);
    let words = snapshot
        .characters
        .iter()
        .flat_map(|state| {
            character_fields(state)
                .into_iter()
                .flat_map(|(_, bits)| bits)
        })
        .chain(box_words);
    words
        .flat_map(u32::to_le_bytes)
        .fold(OFFSET_BASIS, |hash, byte| {
            (hash ^ u64::from(byte)).wrapping_mul(PRIME)
        })
}

fn hashes(snapshots: &[Snapshot]) -> Vec<u64> {
    snapshots.iter().map(state_hash).collect()
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
fn assert_run_exercises_the_world(script: &[TickInput], recorded: &[Snapshot]) {
    assert_eq!(recorded.len(), TICKS);
    let kinds = |pick: fn(&CharacterEffect) -> bool, kind: &str| {
        assert!(effects_of(script).any(pick), "script has no {kind}");
    };
    kinds(
        |e| matches!(e, CharacterEffect::AddVelocity(_)),
        "AddVelocity",
    );
    kinds(
        |e| matches!(e, CharacterEffect::SetVelocity(_)),
        "SetVelocity",
    );
    kinds(
        |e| {
            matches!(
                e,
                CharacterEffect::Teleport {
                    keep_velocity: true,
                    ..
                }
            )
        },
        "Teleport keeping velocity",
    );
    kinds(
        |e| {
            matches!(
                e,
                CharacterEffect::Teleport {
                    keep_velocity: false,
                    ..
                }
            )
        },
        "Teleport zeroing velocity",
    );
    kinds(|e| matches!(e, CharacterEffect::SetLook { .. }), "SetLook");

    let loose_inputs = || script.iter().flat_map(|t| &t.loose);
    let drops = loose_inputs()
        .filter(|input| matches!(input, LooseInput::Spawn(_)))
        .count();
    let removals = loose_inputs()
        .filter(|input| matches!(input, LooseInput::Remove(_)))
        .count();
    assert_eq!(
        (drops, removals),
        (5, 2),
        "the script drops five and removes two"
    );
    let drops_of = |pick: fn(&LooseShape) -> bool| {
        loose_inputs().any(|input| matches!(input, LooseInput::Spawn(s) if pick(&s.shape)))
    };
    assert!(
        drops_of(|s| matches!(s, LooseShape::Box { .. })),
        "no box drop"
    );
    assert!(
        drops_of(|s| matches!(s, LooseShape::Capsule { .. })),
        "no capsule drop"
    );
    assert!(
        drops_of(|s| matches!(s, LooseShape::Cylinder { .. })),
        "no cylinder drop"
    );
    assert!(
        drops_of(|s| matches!(s, LooseShape::Sphere { .. })),
        "no sphere drop"
    );
    assert_eq!(
        recorded.iter().map(|s| s.loose.len()).max(),
        Some(5),
        "five objects were alive at once"
    );
    assert_eq!(
        recorded[TICKS - 1].loose.len(),
        3,
        "two of the five were removed by the end"
    );
    let horizontal = |v: Vec3| Vec3::new(v.x, 0.0, v.z);
    assert!(
        recorded.iter().any(|s| s.characters.iter().any(|c| {
            s.loose
                .iter()
                .any(|(_, l)| horizontal(c.position).distance(horizontal(l.position)) < 0.3)
        })),
        "no character ever stood within 0.3 m of a live loose object, so passing through one was never exercised"
    );
    assert!(
        recorded[..TICKS - 100]
            .iter()
            .any(|s| s.loose.iter().any(|(_, state)| state.asleep)),
        "no loose object fell asleep before the end"
    );

    for index in 0..2 {
        let positions: Vec<Vec3> = recorded
            .iter()
            .map(|s| s.characters[index].position)
            .collect();
        let travelled: f32 = positions.windows(2).map(|w| w[0].distance(w[1])).sum();
        assert!(
            travelled > 10.0,
            "character {index} barely moved: {travelled}"
        );
        assert!(
            recorded
                .iter()
                .any(|s| !s.characters[index].grounded && s.characters[index].velocity.y > 1.0),
            "character {index} never jumped"
        );
        assert!(
            recorded.iter().any(|s| s.characters[index].grounded),
            "character {index} never stood on the floor"
        );
    }

    for index in 0..2 {
        let states = || recorded.iter().map(|s| s.characters[index]);
        assert!(
            states().any(|c| c.stance == Stance::Crouch && c.progress >= CROUCH_TICKS),
            "character {index} never finished a crouch"
        );
        assert!(
            states().any(|c| c.stance == Stance::Stand && c.from == Stance::Crouch),
            "character {index} never stood up from a crouch"
        );
        assert!(
            states().any(|c| c.stance != c.from && c.progress > 0 && c.progress < CROUCH_TICKS),
            "character {index} was never part-way through a leg"
        );
        assert!(
            states().any(|c| !c.grounded && c.stance == Stance::Crouch),
            "character {index} never crouched in the air"
        );
    }

    let all = || recorded.iter().flat_map(|s| s.characters);
    assert!(
        all().any(|c| c.stance == Stance::Prone && c.progress >= PRONE_TICKS),
        "no character finished lying down"
    );
    assert!(
        all().any(|c| c.stance == Stance::Crouch && c.from == Stance::Prone),
        "no character came back from prone"
    );
    assert!(
        recorded
            .iter()
            .any(|s| s.characters[1].stance == Stance::Prone
                && s.characters[1].from == Stance::Crouch),
        "character 1 never went prone through crouch"
    );

    let pairs = || {
        recorded
            .windows(2)
            .flat_map(|w| (0..2).map(move |i| (w[0].characters[i], w[1].characters[i])))
    };
    assert!(
        pairs().any(|(prev, next)| !prev.grounded && next.stance != prev.stance),
        "no leg started in the air"
    );
    assert!(
        pairs().any(|(prev, next)| prev.stance != prev.from
            && (1..CROUCH_TICKS).contains(&prev.progress)
            && next.stance == prev.from),
        "no leg reversed mid-way"
    );
    assert!(
        pairs().all(|(prev, next)| {
            !matches!(
                (prev.stance, next.stance),
                (Stance::Stand, Stance::Prone) | (Stance::Prone, Stance::Stand)
            )
        }),
        "a character skipped crouch between standing and prone"
    );
    assert!(
        pairs().any(|(prev, next)| prev.stance == Stance::Prone
            && prev.from == Stance::Crouch
            && (1..PRONE_TICKS).contains(&prev.progress)
            && next.stance == Stance::Crouch
            && next.from == Stance::Prone),
        "no prone leg reversed mid-way"
    );

    assert!(
        all().any(|c| c.lean > 0.99 && c.roll > 0.1),
        "no character leaned fully right"
    );
    assert!(
        all().any(|c| c.lean < -0.99 && c.roll < -0.1),
        "no character leaned fully left"
    );
    assert!(
        all().any(|c| c.stance == Stance::Prone && c.lean.abs() > 0.99),
        "no character leaned prone"
    );
    assert!(
        all().any(|c| c.lean.abs() > 0.1 && c.lean.abs() < 0.9),
        "no tick was part-way through a lean"
    );
    assert!(
        all().all(|c| c.lean.abs() <= 1.0 && c.roll.abs() <= PRONE_ROLL + 1e-6),
        "a lean or roll went past full"
    );

    let idle: Vec<TickInput> = script
        .iter()
        .map(|_| TickInput {
            loose: Vec::new(),
            effects: Vec::new(),
            commands: [CharacterCommand::default(); 2],
        })
        .collect();
    let idle_run = run(&idle);
    let pushed_ticks = recorded
        .iter()
        .zip(&idle_run)
        .skip(APPROACH_TICKS)
        .filter(|(a, b)| a.box_translation.distance(b.box_translation) > 0.01)
        .count();
    assert!(
        pushed_ticks > 100,
        "the box differs from an idle run on only {pushed_ticks} ticks"
    );

    let corner = Vec3::new(4.0, 0.0, 8.0);
    let closest = recorded
        .iter()
        .map(|s| {
            let p = s.characters[1].position;
            Vec3::new(p.x, 0.0, p.z).distance(corner)
        })
        .fold(f32::MAX, f32::min);
    assert!(
        closest < 0.7,
        "character 1 never reached the wall corner: {closest}"
    );

    let grounded_heights = |on_slab: bool| {
        recorded
            .iter()
            .flat_map(|s| &s.characters)
            .filter(move |c| c.grounded && (1.0..=2.0).contains(&c.position.z) == on_slab)
            .map(|c| c.position.y)
    };
    let on_floor = grounded_heights(false).fold(f32::MAX, f32::min);
    let on_slab = grounded_heights(true).fold(f32::MIN, f32::max);
    assert!(
        on_slab - on_floor > 0.25,
        "no character stood on the step: floor {on_floor}, slab {on_slab}"
    );
}

#[test]
fn a_tick_of_commands_and_effects_round_trips_through_postcard() {
    let tick = TickInput {
        loose: vec![
            LooseInput::Spawn(LooseSpawn {
                friction: 0.3,
                restitution: 0.2,
                linear_damping: 0.1,
                angular_damping: 0.4,
                position: Vec3::new(1.0, 2.0, -3.0),
                rotation: Quat::from_rotation_y(0.7),
                linear_velocity: Vec3::new(0.5, -1.0, 2.0),
                angular_velocity: Vec3::new(0.1, 0.2, 0.3),
                ..LooseSpawn::new(LooseShape::Sphere { radius: 0.25 }, 3.5)
            }),
            LooseInput::Remove(4),
        ],
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
                stance: Stance::Crouch,
                lean: -0.625,
            },
            CharacterCommand::default(),
        ],
    };

    let back = decode(&encode(std::slice::from_ref(&tick)));

    assert_eq!(back, vec![tick]);
}

/// Effects apply within their tick's move, so the change shows in that same tick.
#[test]
fn a_one_ulp_change_to_one_effect_breaks_the_replay() {
    let script = make_script();
    let recorded = run(&script);
    let (tick, slot) = script
        .iter()
        .enumerate()
        .skip(APPROACH_TICKS)
        .find_map(|(tick, t)| {
            let slot = t
                .effects
                .iter()
                .position(|(_, e)| matches!(e, CharacterEffect::AddVelocity(_)))?;
            Some((tick, slot))
        })
        .expect("the script has an AddVelocity effect after the approach");
    let mut altered = script.clone();
    altered[tick].effects[slot].1 = one_ulp_off(altered[tick].effects[slot].1);

    let replayed = run(&altered);

    let divergence = first_divergence(&recorded, &replayed)
        .expect("a one-ulp change to a recorded effect must show in the replay");
    assert_eq!(divergence.tick, tick, "{divergence:?}");
}

#[test]
fn a_one_ulp_change_to_the_box_launch_velocity_breaks_the_replay() {
    let script = make_script();
    let recorded = run(&script);
    let nudged = Vec3::new(
        flip_lowest_bit(BOX_VELOCITY.x),
        BOX_VELOCITY.y,
        BOX_VELOCITY.z,
    );

    let replayed = run_in(build_scene(nudged), &script);

    let divergence = first_divergence(&recorded, &replayed)
        .expect("a one-ulp change to the box must show in the replay");
    assert_eq!(divergence.entity, "box", "{divergence:?}");
}

/// The script and where its capsule drop sits, with the capsule's launch x velocity raised by
/// 1e-3 m/s.
fn script_with_a_nudged_capsule() -> (Vec<TickInput>, usize) {
    let mut script = make_script();
    let (tick, slot) = script
        .iter()
        .enumerate()
        .find_map(|(tick, t)| {
            let slot = t.loose.iter().position(|input| {
                matches!(
                    input,
                    LooseInput::Spawn(s) if matches!(s.shape, LooseShape::Capsule { .. })
                )
            })?;
            Some((tick, slot))
        })
        .expect("the script drops a capsule");
    let LooseInput::Spawn(spawn) = &mut script[tick].loose[slot] else {
        unreachable!("the slot was found as a spawn");
    };
    spawn.linear_velocity.x += 1e-3;
    (script, tick)
}

#[test]
fn a_one_milli_change_to_a_loose_launch_velocity_breaks_the_replay() {
    let recorded = run(&make_script());
    let (altered, tick) = script_with_a_nudged_capsule();

    let replayed = run(&altered);

    let divergence = first_divergence(&recorded, &replayed)
        .expect("a 1e-3 m/s change to a dropped object must show in the replay");
    assert_eq!(divergence.entity, "loose", "{divergence:?}");
    assert_eq!(divergence.tick, tick, "{divergence:?}");
}

#[test]
fn a_one_milli_change_to_a_loose_launch_velocity_changes_the_state_hash() {
    let recorded = hashes(&run(&make_script()));
    let (altered, tick) = script_with_a_nudged_capsule();

    let replayed = hashes(&run(&altered));

    assert_eq!(recorded.len(), replayed.len());
    assert_eq!(
        &recorded[..tick],
        &replayed[..tick],
        "ticks before the drop match"
    );
    assert_ne!(
        recorded[tick], replayed[tick],
        "a 1e-3 m/s change left the hash of the drop tick unchanged"
    );
}

#[test]
fn the_state_hash_is_identical_across_two_runs() {
    let script = make_script();

    let first = hashes(&run(&script));
    let second = hashes(&run(&script));

    assert_eq!(first.len(), TICKS);
    assert_eq!(first, second);
    let distinct: std::collections::HashSet<u64> = first.iter().copied().collect();
    assert!(
        distinct.len() > TICKS / 2,
        "the hash barely changes across ticks: {} distinct of {TICKS}",
        distinct.len()
    );
}

#[test]
fn a_one_ulp_change_to_the_box_launch_velocity_changes_the_state_hash() {
    let script = make_script();
    let recorded = hashes(&run(&script));
    let nudged = Vec3::new(
        flip_lowest_bit(BOX_VELOCITY.x),
        BOX_VELOCITY.y,
        BOX_VELOCITY.z,
    );

    let replayed = hashes(&run_in(build_scene(nudged), &script));

    assert_eq!(recorded.len(), replayed.len());
    assert!(
        recorded.iter().zip(&replayed).any(|(a, b)| a != b),
        "a one-ulp change to the box left every tick's hash unchanged"
    );
}

/// CI sets `MOHO_REPLAY_HASH_OUT` so each platform leaves its per-tick hashes for comparison.
#[test]
fn the_recorded_run_writes_its_per_tick_hashes_when_asked() {
    let recorded = hashes(&run(&make_script()));

    assert_eq!(recorded.len(), TICKS);
    if let Some(path) = std::env::var_os("MOHO_REPLAY_HASH_OUT") {
        let mut lines = String::new();
        for (tick, hash) in recorded.iter().enumerate() {
            writeln!(lines, "{tick} {hash:016x}").expect("writing to a String cannot fail");
        }
        std::fs::write(&path, lines).expect("the replay hash file is writable");
    }
}
