use moho_sim::PlayerInput;

#[test]
fn player_input_bincode_roundtrip() {
    let inputs = vec![PlayerInput::Move { dx: 1, dy: -1 }, PlayerInput::Action(7)];

    let bytes =
        bincode::encode_to_vec(&inputs, bincode::config::standard()).expect("encode inputs");
    let (decoded, _): (Vec<PlayerInput>, _) =
        bincode::decode_from_slice(&bytes, bincode::config::standard()).expect("decode inputs");
    assert_eq!(inputs, decoded);
}
