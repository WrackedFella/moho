// Tone-map pass: reads the HDR scene texel 1:1, adds the bloom chain's mip 0, and writes
// the swapchain.
// The curve is Khronos PBR Neutral's highlight compression without its toe offset:
// identity below START_COMPRESSION, hue-preserving roll-off above it.

struct ToneMap {
    exposure: f32,
    enabled: u32,
    bloom_intensity: f32,
    pad0: u32,
}

@group(0) @binding(0)
var hdr_texture: texture_2d<f32>;

@group(0) @binding(1)
var<uniform> tone_map: ToneMap;

@group(0) @binding(2)
var bloom_texture: texture_2d<f32>;

@group(0) @binding(3)
var bloom_sampler: sampler;

const START_COMPRESSION: f32 = 0.76;
const DESATURATION: f32 = 0.15;

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> @builtin(position) vec4<f32> {
    // One oversized triangle covering the viewport: (-1,-1), (3,-1), (-1,3).
    let x = f32(i32(vertex_index & 1u) * 4 - 1);
    let y = f32(i32(vertex_index >> 1u) * 4 - 1);
    return vec4<f32>(x, y, 0.0, 1.0);
}

fn neutral_shoulder(color: vec3<f32>) -> vec3<f32> {
    let peak = max(color.r, max(color.g, color.b));
    if (peak < START_COMPRESSION) {
        return color;
    }
    let d = 1.0 - START_COMPRESSION;
    let new_peak = 1.0 - d * d / (peak + d - START_COMPRESSION);
    let scaled = color * (new_peak / peak);
    let g = 1.0 - 1.0 / (DESATURATION * (peak - new_peak) + 1.0);
    return mix(scaled, vec3<f32>(new_peak), g);
}

@fragment
fn fs_main(@builtin(position) frag_coord: vec4<f32>) -> @location(0) vec4<f32> {
    var hdr = textureLoad(hdr_texture, vec2<i32>(frag_coord.xy), 0);
    // Skipped at zero so bloom Off leaves the scene bit-identical.
    if (tone_map.bloom_intensity > 0.0) {
        let uv = frag_coord.xy / vec2<f32>(textureDimensions(hdr_texture));
        let bloom = textureSampleLevel(bloom_texture, bloom_sampler, uv, 0.0).rgb;
        hdr = vec4<f32>(hdr.rgb + tone_map.bloom_intensity * bloom, hdr.a);
    }
    if (tone_map.enabled == 0u) {
        return hdr;
    }
    let mapped = neutral_shoulder(max(hdr.rgb, vec3<f32>(0.0)) * tone_map.exposure);
    return vec4<f32>(mapped, hdr.a);
}
