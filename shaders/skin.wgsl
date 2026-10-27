// Skins one mesh placement into the frame vertex arena: every source vertex is copied word
// for word, then position (words 0-2) and normal (words 3-5) are rewritten from the blended
// joint matrix. Any other vertex field rides along untouched, whatever the vertex layout.

// Size of one `Vertex` in 32-bit words; the pipeline sets it from `size_of::<Vertex>() / 4`.
override VERTEX_WORDS: u32;

struct SkinParams {
    vertex_count: u32,
    // Arena index of this placement's first vertex.
    first_vertex: u32,
    // Index in `palette` of this placement's joint 0.
    first_joint: u32,
    pad: u32,
}

struct SkinVertex {
    joints: vec4<u32>,
    weights: vec4<f32>,
}

@group(0) @binding(0) var<storage, read> source: array<u32>;
@group(0) @binding(1) var<storage, read> skin: array<SkinVertex>;

// Three rows per joint: `[m_r0, m_r1, m_r2, t_r]`.
@group(1) @binding(0) var<storage, read> palette: array<vec4<f32>>;
@group(1) @binding(1) var<storage, read_write> arena: array<u32>;
@group(1) @binding(2) var<uniform> params: SkinParams;

@compute @workgroup_size(64, 1, 1)
fn cs_main(@builtin(global_invocation_id) id: vec3<u32>) {
    let v = id.x;
    if (v >= params.vertex_count) {
        return;
    }
    let src = v * VERTEX_WORDS;
    let dst = (params.first_vertex + v) * VERTEX_WORDS;

    let position = vec3<f32>(
        bitcast<f32>(source[src]),
        bitcast<f32>(source[src + 1u]),
        bitcast<f32>(source[src + 2u]),
    );
    let normal = vec3<f32>(
        bitcast<f32>(source[src + 3u]),
        bitcast<f32>(source[src + 4u]),
        bitcast<f32>(source[src + 5u]),
    );

    let sv = skin[v];
    var r0 = vec4<f32>(0.0);
    var r1 = vec4<f32>(0.0);
    var r2 = vec4<f32>(0.0);
    for (var k = 0u; k < 4u; k++) {
        let base = (params.first_joint + sv.joints[k]) * 3u;
        let w = sv.weights[k];
        r0 += w * palette[base];
        r1 += w * palette[base + 1u];
        r2 += w * palette[base + 2u];
    }

    let p = vec4<f32>(position, 1.0);
    let skinned_position = vec3<f32>(dot(r0, p), dot(r1, p), dot(r2, p));
    let m = mat3x3<f32>(
        vec3<f32>(r0.x, r1.x, r2.x),
        vec3<f32>(r0.y, r1.y, r2.y),
        vec3<f32>(r0.z, r1.z, r2.z),
    );
    let skinned_normal = normal_to_world(m, normal);

    arena[dst] = bitcast<u32>(skinned_position.x);
    arena[dst + 1u] = bitcast<u32>(skinned_position.y);
    arena[dst + 2u] = bitcast<u32>(skinned_position.z);
    arena[dst + 3u] = bitcast<u32>(skinned_normal.x);
    arena[dst + 4u] = bitcast<u32>(skinned_normal.y);
    arena[dst + 5u] = bitcast<u32>(skinned_normal.z);
    for (var w = 6u; w < VERTEX_WORDS; w++) {
        arena[dst + w] = source[src + w];
    }
}
