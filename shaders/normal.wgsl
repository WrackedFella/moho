// Cofactor of `m` (inverse-transpose times det): exact under non-uniform scale without an
// inverse; flipping on a negative determinant keeps mirrored transforms' normals facing out.
// `sign` is not used because it is 0 for a flattened `m`, whose cofactor still gives the
// plane's normal.
fn normal_to_world(m: mat3x3<f32>, n: vec3<f32>) -> vec3<f32> {
    let c0 = m[0];
    let c1 = m[1];
    let c2 = m[2];
    let cofactor = mat3x3<f32>(cross(c1, c2), cross(c2, c0), cross(c0, c1));
    let det = dot(c0, cofactor[0]);
    return normalize(cofactor * n * select(1.0, -1.0, det < 0.0));
}
