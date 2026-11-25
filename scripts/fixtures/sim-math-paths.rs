fn a(yaw: Vec<f32>, x: f32) {
    let s: Vec<f32> = yaw.iter().copied().map(f32::sin).collect();
    let c: Vec<(f32, f32)> = yaw.iter().copied().map(f64::sin_cos).collect();
    let q = <f32>::sin_cos(x);
    let p = f32::consts::PI;
    let e = f32::EPSILON;
    let t = f32::sinusoid_table;
    let v = Some(x).expect("x");
}
