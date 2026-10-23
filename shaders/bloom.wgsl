// Bloom chain (Jimenez 2014, "Next Generation Post Processing in Call of Duty: Advanced
// Warfare"): a thresholded 13-tap prefilter from the HDR scene into mip 0, 13-tap
// downsamples down the chain, then 3x3 tent upsamples blended additively back up it.
// Each pass reads one texture (`source`) and writes another; texel size comes from the
// source's dimensions.

// Brightness above which a pixel blooms, and the width of the soft knee above it.
override bloom_threshold: f32;
override bloom_knee: f32;

@group(0) @binding(0)
var source: texture_2d<f32>;

@group(0) @binding(1)
var source_sampler: sampler;

struct VertexOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOut {
    // One oversized triangle covering the viewport: (-1,-1), (3,-1), (-1,3).
    let x = f32(i32(vertex_index & 1u) * 4 - 1);
    let y = f32(i32(vertex_index >> 1u) * 4 - 1);
    var out: VertexOut;
    out.position = vec4<f32>(x, y, 0.0, 1.0);
    out.uv = vec2<f32>(x * 0.5 + 0.5, 0.5 - y * 0.5);
    return out;
}

fn tap(uv: vec2<f32>, texel: vec2<f32>, offset: vec2<f32>) -> vec3<f32> {
    return max(textureSampleLevel(source, source_sampler, uv + offset * texel, 0.0).rgb, vec3<f32>(0.0));
}

fn luma(color: vec3<f32>) -> f32 {
    return dot(color, vec3<f32>(0.2126, 0.7152, 0.0722));
}

// The 13 taps grouped into five 2x2 boxes: four overlapping corner boxes weighted 1/8 and
// the centre box weighted 1/2. With `karis`, each box is further weighted by
// 1 / (1 + luma) so a single very bright texel can't flicker as the camera moves.
fn downsample_13(uv: vec2<f32>, karis: bool) -> vec3<f32> {
    let texel = 1.0 / vec2<f32>(textureDimensions(source));
    // a b c
    //  j k
    // d e f
    //  l m
    // g h i
    let a = tap(uv, texel, vec2<f32>(-2.0, -2.0));
    let b = tap(uv, texel, vec2<f32>(0.0, -2.0));
    let c = tap(uv, texel, vec2<f32>(2.0, -2.0));
    let d = tap(uv, texel, vec2<f32>(-2.0, 0.0));
    let e = tap(uv, texel, vec2<f32>(0.0, 0.0));
    let f = tap(uv, texel, vec2<f32>(2.0, 0.0));
    let g = tap(uv, texel, vec2<f32>(-2.0, 2.0));
    let h = tap(uv, texel, vec2<f32>(0.0, 2.0));
    let i = tap(uv, texel, vec2<f32>(2.0, 2.0));
    let j = tap(uv, texel, vec2<f32>(-1.0, -1.0));
    let k = tap(uv, texel, vec2<f32>(1.0, -1.0));
    let l = tap(uv, texel, vec2<f32>(-1.0, 1.0));
    let m = tap(uv, texel, vec2<f32>(1.0, 1.0));

    let centre = (j + k + l + m) * 0.25;
    let top_left = (a + b + d + e) * 0.25;
    let top_right = (b + c + e + f) * 0.25;
    let bottom_left = (d + e + g + h) * 0.25;
    let bottom_right = (e + f + h + i) * 0.25;

    let w0 = box_weight(centre, 0.5, karis);
    let w1 = box_weight(top_left, 0.125, karis);
    let w2 = box_weight(top_right, 0.125, karis);
    let w3 = box_weight(bottom_left, 0.125, karis);
    let w4 = box_weight(bottom_right, 0.125, karis);
    let sum = centre * w0 + top_left * w1 + top_right * w2 + bottom_left * w3
        + bottom_right * w4;
    return sum / (w0 + w1 + w2 + w3 + w4);
}

fn box_weight(color: vec3<f32>, base: f32, karis: bool) -> f32 {
    if (karis) {
        return base / (1.0 + luma(color));
    }
    return base;
}

// Soft-knee threshold: 0 at or below the threshold, a quadratic ramp across the knee,
// then linear. Scales the colour so its peak channel keeps only the part above it.
fn threshold(color: vec3<f32>) -> vec3<f32> {
    let peak = max(color.r, max(color.g, color.b));
    let q = clamp(peak - bloom_threshold, 0.0, 2.0 * bloom_knee);
    let w = max(q * q / (4.0 * bloom_knee), peak - bloom_threshold - bloom_knee)
        / max(peak, 1e-4);
    return color * w;
}

@fragment
fn fs_prefilter(in: VertexOut) -> @location(0) vec4<f32> {
    return vec4<f32>(threshold(downsample_13(in.uv, true)), 1.0);
}

@fragment
fn fs_downsample(in: VertexOut) -> @location(0) vec4<f32> {
    return vec4<f32>(downsample_13(in.uv, false), 1.0);
}

@fragment
fn fs_upsample(in: VertexOut) -> @location(0) vec4<f32> {
    let texel = 1.0 / vec2<f32>(textureDimensions(source));
    var sum = tap(in.uv, texel, vec2<f32>(0.0, 0.0)) * 4.0;
    sum += (tap(in.uv, texel, vec2<f32>(-1.0, 0.0)) + tap(in.uv, texel, vec2<f32>(1.0, 0.0))
        + tap(in.uv, texel, vec2<f32>(0.0, -1.0)) + tap(in.uv, texel, vec2<f32>(0.0, 1.0))) * 2.0;
    sum += tap(in.uv, texel, vec2<f32>(-1.0, -1.0)) + tap(in.uv, texel, vec2<f32>(1.0, -1.0))
        + tap(in.uv, texel, vec2<f32>(-1.0, 1.0)) + tap(in.uv, texel, vec2<f32>(1.0, 1.0));
    return vec4<f32>(sum / 16.0, 0.0);
}
