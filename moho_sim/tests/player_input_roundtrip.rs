use moho_sim::PlayerInputType as PlayerInput;

#[test]
fn player_input_bincode_roundtrip() {
    let inputs = vec![PlayerInput::Move { dx: 1, dy: -1 }, PlayerInput::Action(7)];

    let bytes = serde_json::to_vec(&inputs).expect("serialize inputs");
    let decoded: Vec<PlayerInput> = serde_json::from_slice(&bytes).expect("deserialize inputs");
    assert_eq!(inputs, decoded);
}
