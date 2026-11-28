fn step(x: f32, v: Option<f32>) -> f32 {
    let r = x.sqrt().abs().powi(2);
    let d = 90.0_f32.to_radians();
    r + d + v.expect("set")
}
