use moho_sim::{
    ContinuousState, PlayerInputType as PlayerInput, Simulation, map_to_player_inputs, stamp_inputs,
};

#[test]
fn map_single_direction_key_to_move() {
    let prev = ContinuousState {
        up: false,
        down: false,
        left: false,
        right: false,
        action: false,
        mouse_dx: 0,
        mouse_dy: 0,
    };
    let curr = ContinuousState {
        up: false,
        down: false,
        left: false,
        right: true,
        action: false,
        mouse_dx: 0,
        mouse_dy: 0,
    };

    let inputs = map_to_player_inputs(&prev, &curr);
    assert_eq!(inputs, vec![PlayerInput::Move { dx: 1, dy: 0 }]);
}

#[test]
fn map_action_press_to_action() {
    let prev = ContinuousState {
        up: false,
        down: false,
        left: false,
        right: false,
        action: false,
        mouse_dx: 0,
        mouse_dy: 0,
    };
    let curr = ContinuousState {
        up: false,
        down: false,
        left: false,
        right: false,
        action: true,
        mouse_dx: 0,
        mouse_dy: 0,
    };

    let inputs = map_to_player_inputs(&prev, &curr);
    assert_eq!(inputs, vec![PlayerInput::Action(1)]);
}

#[test]
fn stamp_and_apply_timed_inputs_via_simulation() {
    let prev = ContinuousState {
        up: false,
        down: false,
        left: false,
        right: false,
        action: false,
        mouse_dx: 0,
        mouse_dy: 0,
    };
    let curr = ContinuousState {
        up: false,
        down: false,
        left: false,
        right: true,
        action: true,
        mouse_dx: 0,
        mouse_dy: 0,
    };

    let inputs = map_to_player_inputs(&prev, &curr);
    let timed = stamp_inputs(42, inputs);

    let mut sim = Simulation::new(0x10);
    sim.tick_timed(&timed);
    // right move adds dx=1, seed is added in tick: seed low bits=0x10 -> +16
    assert_eq!(sim.x, 1 + (0x10 & 0xffff));
}
