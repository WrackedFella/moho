use glam::Vec3;

#[test]
fn snapshot_roundtrip_and_restore() {
    // create a controller, set some state
    let mut sim = moho_sim::SimulationController::new(Vec3::new(1.0, 2.0, 3.0));
    sim.player_controller.yaw = 0.75;
    sim.player_controller.pitch = -0.125;
    sim.controller_input.forward = 0.9;
    sim.controller_input.right = -0.4;

    let bytes = sim.snapshot_bytes();
    let restored =
        moho_sim::SimulationController::restore_from_bytes(&bytes).expect("restore should succeed");

    assert_eq!(
        sim.player_controller.position,
        restored.player_controller.position
    );
    assert_eq!(sim.player_controller.yaw, restored.player_controller.yaw);
    assert_eq!(
        sim.player_controller.pitch,
        restored.player_controller.pitch
    );
    assert_eq!(
        sim.controller_input.forward,
        restored.controller_input.forward
    );
    assert_eq!(sim.controller_input.right, restored.controller_input.right);
}

#[test]
fn snapshot_corruption_is_detected() {
    let sim = moho_sim::SimulationController::new(Vec3::new(0.0, 0.0, 0.0));
    let mut bytes = sim.snapshot_bytes();

    // Flip one payload byte (payload starts at offset 14)
    if bytes.len() > 14 {
        bytes[14] ^= 0xff;
    }

    let res = moho_sim::SimulationController::restore_from_bytes(&bytes);
    assert!(res.is_err(), "corrupted snapshot should produce an error");
}
