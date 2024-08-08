// Vertex shader
struct MetaInfo {
    window_size: vec2<u32>,
};
@group(0) @binding(0)
var<uniform> meta_info: MetaInfo;

struct VertexInput {
    @location(0) position: vec2<f32>,
}
struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
};

@vertex
fn vs_main(
    input: VertexInput,
) -> VertexOutput {
    var out: VertexOutput;
    let x = input.position.x / f32(meta_info.window_size.x) * 2.0 - 1.0;
    let y = 1.0 - input.position.y / f32(meta_info.window_size.y) * 2.0;
    out.clip_position = vec4<f32>(x, y, 0.0, 1.0);
    return out;
}

// Fragment shader
@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return vec4<f32>(1.0, 1.0, 1.0, 1.0);
}
 