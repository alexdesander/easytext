// Vertex shader
struct MetaInfo {
    window_size: vec2<u32>,
};
@group(1) @binding(0)
var<uniform> meta_info: MetaInfo;

struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) tex_coords: vec2<f32>,
};
struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) tex_coords: vec2<f32>,
};

@vertex
fn vs_main(
    input: VertexInput,
) -> VertexOutput {
    var out: VertexOutput;
    let x = input.position.x / f32(meta_info.window_size.x) * 2.0 - 1.0;
    let y = 1.0 - input.position.y / f32(meta_info.window_size.y) * 2.0;
    out.clip_position = vec4<f32>(x, y, 0.0, 1.0);
    out.tex_coords = input.tex_coords;
    return out;
}

// Fragment shader
@group(0) @binding(0)
var t_diffuse: texture_2d<f32>;
@group(0) @binding(1)
var s_diffuse: sampler;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let sample = textureSample(t_diffuse, s_diffuse, in.tex_coords);
    if sample.x < 0.00001 {
        discard;
    }
    return vec4<f32>(1.0, 1.0, 1.0, sample.x);
}
