fn step(x: f32) -> f32 {
    x.abs()
}

#[cfg(test)]
mod tests {
    #[test]
    fn rotates() {
        let _ = 1.0_f32.sin_cos();
    }
}
