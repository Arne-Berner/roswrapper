struct Dimensions {
    columns: f32,
    rows: f32,
    _pad0: u32,
    _pad1: u32,
}

struct FullscreenVertexOutput {
    @builtin(position)
    position: vec4<f32>,
    @location(0)
    uv: vec2<f32>,

};

@group(0) @binding(0) var<uniform> u_screen: vec2<f32>; // width,height

@group(1) @binding(0) var<storage, read> 
labels: array<u32>;

@group(1) @binding(1) var<uniform> 
dims : Dimensions;

// top left:    0,0
// top right:   2,0
// bottom left: 0,2
@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32)-> FullscreenVertexOutput {
    let uv = vec2<f32>(f32(vertex_index >> 1u), f32(vertex_index & 1u)) * 2.0;
    let clip_position = vec4<f32>(uv * vec2<f32>(2.0, -2.0) + vec2<f32>(-1.0, 1.0), 0.0, 1.0);

    return FullscreenVertexOutput(clip_position, uv);
}

@fragment
fn fs_main(@location(0) position: vec2<f32>)-> @location(0) vec4<f32> {
    var color = vec3<f32>(1.0);
    /*  
    if 0.1 < position.x || 0.1 < position.y {
        color = vec3<f32>(0.0);
    }
    */
    let max_x = dims.columns ;
    let max_y = dims.rows;
    let idx: u32 = u32(position.x * max_x) + u32(position.y * dims.rows) * u32(dims.columns);
    let pixel = f32(labels[idx]);
    color = vec3<f32>(pixel);

    return vec4<f32>(color, 1.0);

}
