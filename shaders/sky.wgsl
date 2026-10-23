// Sky gradient shared by the skybox and by surface reflections, so a mirror shows the sky
// the player sees. Sun and moon disks are not part of it.
fn sky_gradient(dir: vec3<f32>, zenith: vec3<f32>, horizon: vec3<f32>, ground: vec3<f32>) -> vec3<f32> {
    if (dir.y > 0.0) {
        return mix(horizon, zenith, smoothstep(0.0, 0.6, dir.y));
    }
    return mix(horizon, ground, smoothstep(0.0, 0.3, -dir.y));
}
