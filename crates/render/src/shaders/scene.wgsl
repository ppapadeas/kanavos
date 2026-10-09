// Kanavos 3D viewport shaders.
//
// Coordinates: render space = world metres minus the scene origin, Z up. Clip space uses reverse-Z (near = 1,
// far = 0), so depth tests compare with "greater". Colours are linear light; the colour targets are sRGB and
// encode on write. `struct Frame` comes from frame.wgsl, which is prepended to this file.

@group(0) @binding(0) var<uniform> frame: Frame;
@group(0) @binding(1) var object_state: texture_2d<u32>;
@group(0) @binding(2) var shadow_map: texture_depth_2d;
@group(0) @binding(3) var shadow_sampler: sampler_comparison;

const ST_HIDDEN: u32 = 1u;
const ST_GHOST: u32 = 2u;
const ST_SELECTED: u32 = 4u;
const ST_HOVER: u32 = 8u;

const FLAG_EDGES: u32 = 1u;
const FLAG_SILHOUETTES: u32 = 2u;

const EDGE_SMOOTH: u32 = 1u;
const EDGE_CUT: u32 = 2u;

const MODE_OPAQUE: u32 = 0u;
const MODE_GHOST: u32 = 1u;
const MODE_TRANSPARENT: u32 = 2u;
const MODE_PICK: u32 = 3u;
const MODE_SHADOW: u32 = 4u;

// A clip-space position outside the view volume; all three corners of a triangle collapse onto it.
const OFFSCREEN: vec4<f32> = vec4<f32>(2.0, 2.0, 2.0, 1.0);

fn state_of(slot: u32) -> u32 {
    let w = max(frame.counts.z, 1u);
    return textureLoad(object_state, vec2<i32>(i32(slot % w), i32(slot / w)), 0).r;
}

fn is_clipped(p: vec3<f32>) -> bool {
    return is_clipped_by(p, 0.0);
}

// Clip test with a tolerance (metres) that keeps points lying on a plane, such as cut outlines.
fn is_clipped_by(p: vec3<f32>, tolerance: f32) -> bool {
    let n = min(frame.counts.x, 6u);
    for (var i = 0u; i < n; i = i + 1u) {
        let pl = frame.clip[i];
        if dot(pl.xyz, p) + pl.w < -tolerance {
            return true;
        }
    }
    return false;
}

fn linear_from_srgb(c: vec3<f32>) -> vec3<f32> {
    let lo = c / 12.92;
    let hi = pow((c + vec3<f32>(0.055)) / 1.055, vec3<f32>(2.4));
    return select(hi, lo, c <= vec3<f32>(0.04045));
}

// Unit vector from a point towards the viewer.
fn to_viewer(p: vec3<f32>) -> vec3<f32> {
    if frame.eye.w > 0.5 {
        let d = frame.eye.xyz - p;
        let l = length(d);
        if l > 1e-6 {
            return d / l;
        }
    }
    return -frame.forward.xyz;
}

fn safe_normal(n: vec3<f32>) -> vec3<f32> {
    let l = length(n);
    if l > 1e-6 {
        return n / l;
    }
    return vec3<f32>(0.0, 0.0, 1.0);
}

// ---------------------------------------------------------------------------------------------------------------
// Background

struct BackgroundOut {
    @builtin(position) position: vec4<f32>,
    @location(0) t: f32,
};

@vertex
fn vs_background(@builtin(vertex_index) vi: u32) -> BackgroundOut {
    let uv = vec2<f32>(f32((vi << 1u) & 2u), f32(vi & 2u));
    var out: BackgroundOut;
    out.position = vec4<f32>(uv * 2.0 - 1.0, 0.0, 1.0);
    out.t = uv.y;
    return out;
}

@fragment
fn fs_background(in: BackgroundOut) -> @location(0) vec4<f32> {
    let t = clamp(in.t, 0.0, 1.0);
    let s = t * t * (3.0 - 2.0 * t);
    return vec4<f32>(mix(frame.bg_bottom.rgb, frame.bg_top.rgb, s), 1.0);
}

// ---------------------------------------------------------------------------------------------------------------
// Surfaces

struct SurfaceIn {
    @location(0) position: vec3<f32>,
    // xyz: unit normal; w: 1 when the mesh is a closed solid.
    @location(1) normal: vec4<f32>,
    // sRGB colour, linear alpha.
    @location(2) color: vec4<f32>,
    @location(3) slot: u32,
};

struct SurfaceOut {
    @builtin(position) position: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) color: vec4<f32>,
    @location(3) @interpolate(flat) state: u32,
    @location(4) @interpolate(flat) closed: u32,
    @location(5) @interpolate(flat) slot: u32,
};

fn surface_vertex(v: SurfaceIn, mode: u32) -> SurfaceOut {
    var out: SurfaceOut;
    let state = state_of(v.slot);
    let ghost = (state & ST_GHOST) != 0u;
    var visible = (state & ST_HIDDEN) == 0u;
    if mode == MODE_OPAQUE || mode == MODE_PICK || mode == MODE_SHADOW {
        visible = visible && !ghost;
    } else if mode == MODE_GHOST {
        visible = visible && ghost;
    }
    if !visible {
        out.position = OFFSCREEN;
        return out;
    }
    let p = vec4<f32>(v.position, 1.0);
    if mode == MODE_SHADOW {
        out.position = frame.light_view_proj * p;
    } else {
        out.position = frame.view_proj * p;
    }
    out.world = v.position;
    out.normal = v.normal.xyz;
    out.color = vec4<f32>(linear_from_srgb(v.color.rgb), v.color.a);
    out.state = state;
    out.closed = select(0u, 1u, v.normal.w > 0.5);
    out.slot = v.slot;
    return out;
}

@vertex
fn vs_opaque(v: SurfaceIn) -> SurfaceOut {
    return surface_vertex(v, MODE_OPAQUE);
}

@vertex
fn vs_ghost(v: SurfaceIn) -> SurfaceOut {
    return surface_vertex(v, MODE_GHOST);
}

@vertex
fn vs_transparent(v: SurfaceIn) -> SurfaceOut {
    return surface_vertex(v, MODE_TRANSPARENT);
}

@vertex
fn vs_pick(v: SurfaceIn) -> SurfaceOut {
    return surface_vertex(v, MODE_PICK);
}

@vertex
fn vs_shadow(v: SurfaceIn) -> SurfaceOut {
    return surface_vertex(v, MODE_SHADOW);
}

// Fraction of sunlight reaching a point (1 = fully lit), with 3×3 percentage-closer filtering.
fn sun_visibility(world: vec3<f32>, n: vec3<f32>) -> f32 {
    if frame.shadow_params.w < 0.5 {
        return 1.0;
    }
    let lp = frame.light_view_proj * vec4<f32>(world + n * frame.shadow_params.x, 1.0);
    let ndc = lp.xyz / lp.w;
    let uv = vec2<f32>(ndc.x * 0.5 + 0.5, 0.5 - ndc.y * 0.5);
    if uv.x <= 0.0 || uv.x >= 1.0 || uv.y <= 0.0 || uv.y >= 1.0 || ndc.z >= 1.0 || ndc.z <= 0.0 {
        return 1.0;
    }
    let t = frame.shadow_params.z;
    var lit = 0.0;
    for (var y = -1; y <= 1; y = y + 1) {
        for (var x = -1; x <= 1; x = x + 1) {
            let o = vec2<f32>(f32(x), f32(y)) * t;
            lit = lit + textureSampleCompareLevel(shadow_map, shadow_sampler, uv + o, ndc.z);
        }
    }
    return mix(1.0, lit / 9.0, frame.shadow_params.y);
}

fn shade(base: vec3<f32>, n: vec3<f32>, world: vec3<f32>) -> vec3<f32> {
    let hemi = mix(frame.ground_color.rgb, frame.sky_color.rgb, n.z * 0.5 + 0.5);
    let ndl = dot(n, frame.sun_dir.xyz);
    var sun = 0.0;
    if ndl > 0.0 && frame.sun_dir.w > 0.0 {
        sun = ndl * sun_visibility(world, n) * frame.sun_dir.w;
    }
    let fill = max(dot(n, to_viewer(world)), 0.0) * frame.light_params.y;
    let light = hemi * frame.light_params.x + frame.sun_color.rgb * sun + vec3<f32>(fill);
    return base * light;
}

fn highlight(rgb: vec3<f32>, state: u32) -> vec3<f32> {
    if (state & ST_SELECTED) != 0u {
        return mix(rgb, frame.select_color.rgb, frame.light_params.w);
    }
    if (state & ST_HOVER) != 0u {
        return mix(rgb, frame.hover_color.rgb, 0.6 * frame.light_params.w);
    }
    return rgb;
}

@fragment
fn fs_opaque(in: SurfaceOut, @builtin(front_facing) front: bool) -> @location(0) vec4<f32> {
    if is_clipped(in.world) {
        discard;
    }
    var rgb: vec3<f32>;
    if !front && in.closed == 1u {
        // Looking into a solid that a clip plane has opened: show a flat cut colour.
        rgb = frame.cut_color.rgb;
    } else {
        var n = safe_normal(in.normal);
        if !front {
            n = -n;
        }
        rgb = shade(in.color.rgb, n, in.world);
    }
    return vec4<f32>(highlight(rgb, in.state), 1.0);
}

@fragment
fn fs_ghost(in: SurfaceOut, @builtin(front_facing) front: bool) -> @location(0) vec4<f32> {
    if is_clipped(in.world) {
        discard;
    }
    var n = safe_normal(in.normal);
    if !front {
        n = -n;
    }
    let grey = dot(in.color.rgb, vec3<f32>(0.2126, 0.7152, 0.0722));
    let base = mix(in.color.rgb, vec3<f32>(grey), 0.7);
    let hemi = mix(frame.ground_color.rgb, frame.sky_color.rgb, n.z * 0.5 + 0.5);
    let rgb = base * hemi * (frame.light_params.x + 0.25);
    return vec4<f32>(highlight(rgb, in.state), frame.light_params.z);
}

@fragment
fn fs_transparent(in: SurfaceOut, @builtin(front_facing) front: bool) -> @location(0) vec4<f32> {
    if is_clipped(in.world) {
        discard;
    }
    var n = safe_normal(in.normal);
    if !front {
        n = -n;
    }
    let v = to_viewer(in.world);
    let rim = pow(1.0 - clamp(abs(dot(n, v)), 0.0, 1.0), 3.0);
    var rgb = shade(in.color.rgb, n, in.world);
    rgb = mix(rgb, frame.sky_color.rgb * frame.light_params.x * 1.2, rim * 0.6);
    var alpha = in.color.a + (1.0 - in.color.a) * rim * 0.45;
    if (in.state & ST_GHOST) != 0u {
        alpha = min(alpha, frame.light_params.z);
    }
    return vec4<f32>(highlight(rgb, in.state), clamp(alpha, 0.0, 1.0));
}

struct PickTargets {
    @location(0) id: u32,
    @location(1) depth: f32,
};

@fragment
fn fs_pick(in: SurfaceOut) -> PickTargets {
    if is_clipped(in.world) {
        discard;
    }
    var out: PickTargets;
    out.id = in.slot + 1u;
    out.depth = in.position.z;
    return out;
}

@fragment
fn fs_shadow(in: SurfaceOut) {
    if is_clipped(in.world) {
        discard;
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Edges: each instance is a segment, expanded into a screen-space quad of constant pixel width.

struct EdgeIn {
    @location(0) p0: vec3<f32>,
    @location(1) p1: vec3<f32>,
    @location(2) n0: vec4<f32>,
    @location(3) n1: vec4<f32>,
    @location(4) slot: u32,
    @location(5) kind: u32,
};

struct EdgeOut {
    @builtin(position) position: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) color: vec4<f32>,
    @location(2) @interpolate(linear) along: f32,
    @location(3) @interpolate(flat) cut: u32,
};

// Moves a point slightly towards the viewer so that lines win the depth test against their own faces.
fn pull_towards_eye(p: vec3<f32>) -> vec3<f32> {
    if frame.eye.w > 0.5 {
        return p + (frame.eye.xyz - p) * frame.edge_params.y;
    }
    return p - frame.forward.xyz * frame.edge_params.z;
}

fn edge_vertex(vi: u32, e: EdgeIn, occluded_pass: bool) -> EdgeOut {
    var out: EdgeOut;
    out.position = OFFSCREEN;
    let state = state_of(e.slot);
    if (state & ST_HIDDEN) != 0u {
        return out;
    }
    let selected = (state & ST_SELECTED) != 0u;
    let hovered = (state & ST_HOVER) != 0u;
    let ghost = (state & ST_GHOST) != 0u;
    if occluded_pass && !selected {
        return out;
    }
    // Selected and hovered objects always show their outline, whatever the display settings.
    let emphasised = selected || hovered;
    let cut = e.kind == EDGE_CUT;
    if cut {
        // Cut outlines are uploaded only while clipping and always shown then.
        if occluded_pass {
            return out;
        }
    } else if e.kind == EDGE_SMOOTH {
        if (frame.counts.y & FLAG_SILHOUETTES) == 0u && !emphasised {
            return out;
        }
        let v = to_viewer(0.5 * (e.p0 + e.p1));
        if dot(e.n0.xyz, v) * dot(e.n1.xyz, v) > 0.0 {
            return out;
        }
    } else if (frame.counts.y & FLAG_EDGES) == 0u && !emphasised {
        return out;
    }

    // Corner of the quad: t along the segment, side across it.
    let corner = vi % 6u;
    var t = 0.0;
    var side = -1.0;
    if corner == 1u {
        t = 1.0;
    } else if corner == 2u || corner == 4u {
        t = 1.0;
        side = 1.0;
    } else if corner == 5u {
        side = 1.0;
    }

    var c0 = frame.view_proj * vec4<f32>(pull_towards_eye(e.p0), 1.0);
    var c1 = frame.view_proj * vec4<f32>(pull_towards_eye(e.p1), 1.0);
    var w0 = e.p0;
    var w1 = e.p1;
    // Clip against the near plane so that both ends project sensibly.
    let near = select(1e-6, frame.forward.w, frame.eye.w > 0.5);
    if c0.w < near && c1.w < near {
        return out;
    }
    if c0.w < near {
        let s = (near - c0.w) / (c1.w - c0.w);
        c0 = mix(c0, c1, s);
        w0 = mix(w0, w1, s);
    } else if c1.w < near {
        let s = (near - c1.w) / (c0.w - c1.w);
        c1 = mix(c1, c0, s);
        w1 = mix(w1, w0, s);
    }

    let half_vp = 0.5 * frame.viewport.xy;
    let s0 = c0.xy / c0.w * half_vp;
    let s1 = c1.xy / c1.w * half_vp;
    let d = s1 - s0;
    let len = length(d);
    var dir = vec2<f32>(1.0, 0.0);
    if len > 1e-4 {
        dir = d / len;
    }
    let perp = vec2<f32>(-dir.y, dir.x);
    var width = frame.edge_params.x;
    if selected {
        width = width * 1.8;
    } else if hovered || cut {
        width = width * 1.5;
    }
    let half_w = 0.5 * width;
    var c = select(c0, c1, t > 0.5);
    let offset = perp * (side * half_w) + dir * ((t * 2.0 - 1.0) * half_w);
    c = vec4<f32>(c.xy + offset / half_vp * c.w, c.z, c.w);

    var color = frame.edge_color;
    if selected {
        color = vec4<f32>(frame.select_color.rgb, 1.0);
    } else if hovered {
        color = vec4<f32>(frame.hover_color.rgb, 1.0);
    } else if ghost {
        color = vec4<f32>(color.rgb, color.a * min(1.0, frame.light_params.z * 2.0));
    } else if cut {
        color = vec4<f32>(color.rgb, 1.0);
    }
    if occluded_pass {
        color = vec4<f32>(frame.select_color.rgb, 0.7);
    }
    out.position = c;
    out.world = mix(w0, w1, t);
    out.color = color;
    out.along = t * len;
    out.cut = select(0u, 1u, cut);
    return out;
}

@vertex
fn vs_edge(@builtin(vertex_index) vi: u32, e: EdgeIn) -> EdgeOut {
    return edge_vertex(vi, e, false);
}

@vertex
fn vs_edge_occluded(@builtin(vertex_index) vi: u32, e: EdgeIn) -> EdgeOut {
    return edge_vertex(vi, e, true);
}

@fragment
fn fs_edge(in: EdgeOut) -> @location(0) vec4<f32> {
    // Cut outlines lie on a clip plane; a millimetre of tolerance keeps rounding from breaking them up.
    if is_clipped_by(in.world, select(0.0, 0.001, in.cut == 1u)) {
        discard;
    }
    return in.color;
}

@fragment
fn fs_edge_dashed(in: EdgeOut) -> @location(0) vec4<f32> {
    if is_clipped(in.world) {
        discard;
    }
    if fract(in.along / frame.edge_params.w) > 0.55 {
        discard;
    }
    return in.color;
}

// ---------------------------------------------------------------------------------------------------------------
// Ground grid

struct GridOut {
    @builtin(position) position: vec4<f32>,
    @location(0) world: vec3<f32>,
};

@vertex
fn vs_grid(@builtin(vertex_index) vi: u32) -> GridOut {
    var corners = array<vec2<f32>, 6>(
        vec2<f32>(-1.0, -1.0), vec2<f32>(1.0, -1.0), vec2<f32>(1.0, 1.0),
        vec2<f32>(-1.0, -1.0), vec2<f32>(1.0, 1.0), vec2<f32>(-1.0, 1.0),
    );
    let c = corners[vi % 6u];
    let p = vec3<f32>(frame.grid_a.xy + c * frame.grid_a.z, frame.grid_a.w);
    var out: GridOut;
    var clip = frame.view_proj * vec4<f32>(p, 1.0);
    // Push the grid very slightly back so that floors lying on it win the depth test.
    clip.z = clip.z * 0.9995;
    out.position = clip;
    out.world = p;
    return out;
}

// Anti-aliased grid lines (1 on a line, 0 between lines), faded out where lines get closer than a few pixels
// so that distant parts of the grid turn into a quiet tone instead of moiré.
fn grid_lines(coord: vec2<f32>) -> f32 {
    let w = max(fwidth(coord), vec2<f32>(1e-6));
    let g = abs(fract(coord - 0.5) - 0.5) / w;
    let line = 1.0 - min(min(g.x, g.y), 1.0);
    let density = max(w.x, w.y);
    return line * (1.0 - smoothstep(0.12, 0.35, density));
}

@fragment
fn fs_grid(in: GridOut) -> @location(0) vec4<f32> {
    let coord = (in.world.xy + frame.grid_b.xy) / frame.grid_b.z;
    let minor = grid_lines(coord);
    let major = grid_lines(coord * 0.1);
    let r = distance(in.world.xy, frame.grid_a.xy) / frame.grid_a.z;
    let fade = 1.0 - smoothstep(0.25, 1.0, r);
    // Fade out when the grid is seen edge-on.
    let facing = abs(dot(vec3<f32>(0.0, 0.0, 1.0), to_viewer(in.world)));
    let grazing = smoothstep(0.03, 0.25, facing);
    let a = max(major, minor * frame.grid_b.w * 0.45) * fade * grazing * frame.grid_color.a;
    if a < 0.002 {
        discard;
    }
    return vec4<f32>(frame.grid_color.rgb, a);
}
