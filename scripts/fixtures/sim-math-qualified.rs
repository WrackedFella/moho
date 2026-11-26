fn a(x: f32, y: f64) -> (f32, f64) {
    let s = f32::sin_cos(x);
    let t = f64::sin(y);
    let u = sim_math::sin_cos(x);
    (s.0 + u.0, t)
}
