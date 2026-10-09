// Per-frame uniforms shared by every Kanavos viewport shader. This file is prepended to each shader module and
// must match `FrameUniforms` in gpu.rs field for field.

struct Frame {
    view_proj: mat4x4<f32>,
    view: mat4x4<f32>,
    proj: mat4x4<f32>,
    inv_proj: mat4x4<f32>,
    light_view_proj: mat4x4<f32>,
    // xyz: eye position; w: 1 for perspective, 0 for parallel projection.
    eye: vec4<f32>,
    // xyz: viewing direction; w: near plane distance (perspective).
    forward: vec4<f32>,
    // width, height, 1/width, 1/height in physical pixels.
    viewport: vec4<f32>,
    // xyz: direction towards the sun; w: sun intensity.
    sun_dir: vec4<f32>,
    sun_color: vec4<f32>,
    sky_color: vec4<f32>,
    ground_color: vec4<f32>,
    bg_top: vec4<f32>,
    bg_bottom: vec4<f32>,
    edge_color: vec4<f32>,
    select_color: vec4<f32>,
    hover_color: vec4<f32>,
    cut_color: vec4<f32>,
    grid_color: vec4<f32>,
    // xy: grid centre, z: half size, w: elevation (render space).
    grid_a: vec4<f32>,
    // xy: offset that aligns lines with world coordinates, z: minor spacing, w: minor line strength.
    grid_b: vec4<f32>,
    // x: width in pixels, y: perspective pull towards the eye (fraction of distance),
    // z: parallel pull towards the eye (metres), w: dash length in pixels.
    edge_params: vec4<f32>,
    // x: normal offset (metres), y: strength, z: texel size (uv), w: 1 when enabled.
    shadow_params: vec4<f32>,
    // x: ambient strength, y: fill strength, z: ghost opacity, w: selection tint.
    light_params: vec4<f32>,
    // x: radius (metres), y: strength, z: pixels per metre at unit depth, w: 1 when enabled.
    ao_params: vec4<f32>,
    clip: array<vec4<f32>, 6>,
    // x: clip plane count, y: flags, z: state texture width, w: unused.
    counts: vec4<u32>,
};
