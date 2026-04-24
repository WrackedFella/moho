use moho_sim::{PlayerInput, Simulation};

#[test]
fn simulation_is_deterministic_given_seed_and_inputs() {
    let inputs = vec![
        PlayerInput::Move { dx: 1, dy: 0 },
        PlayerInput::Move { dx: 0, dy: 2 },
        PlayerInput::Action(3),
    ];

    let seed = 0x1234_u64;

    let mut sim1 = Simulation::new(seed);
    sim1.tick(&inputs);
    let final1 = (sim1.x, sim1.y);

    let mut sim2 = Simulation::new(seed);
    sim2.tick(&inputs);
    let final2 = (sim2.x, sim2.y);

    assert_eq!(
        final1, final2,
        "Simulations with same seed+inputs must match"
    );

    // Verify snapshot/restore also preserves state
    let snap = sim1.snapshot().expect("snapshot");
    let restored = Simulation::restore(&snap).expect("restore");
    assert_eq!((restored.x, restored.y), final1);
}
