fn step(yaw: f32) -> (f32, f32) {
    let x = yaw;
    x.sin_cos()
}
