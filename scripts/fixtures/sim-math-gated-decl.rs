#[cfg(test)]
mod helpers;

fn step(x: f32) -> (f32, f32) {
    x.sin_cos()
}
