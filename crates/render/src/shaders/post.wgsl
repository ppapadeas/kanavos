// Screen-space ambient occlusion and final composite.
//
// The ambient occlusion estimate follows the "alchemy" family of obscurance estimators (McGuire et al., HPG 2011
// and 2012): a few depth samples on a spiral around each pixel, weighted by how much of each sample vector lies
// above the surface. A 4×4 rotation pattern decorrelates neighbouring pixels and a matching 4×4 depth-aware blur
// in the composite removes the pattern.
//
// `DEPTH_TEXTURE` is replaced by `texture_depth_multisampled_2d` or `texture_depth_2d` when the module is built;
// both take the sample index or mip level as the last `textureLoad` argument, and sample 0 / level 0 is used.

@group(0) @binding(0) var<uniform> frame: Frame;
@group(0) @binding(1) var depth_tex: DEPTH_TEXTURE;
@group(0) @binding(2) var color_tex: texture_2d<f32>;
@group(0) @binding(3) var ao_tex: texture_2d<f32>;

const AO_SAMPLES: i32 = 12;
const AO_TURNS: f32 = 7.0;
const TAU: f32 = 6.28318530718;

struct FullscreenOut {
    @builtin(position) position: vec4<f32>,
};

@vertex
fn vs_fullscreen(@builtin(vertex_index) vi: u32) -> FullscreenOut {
    let uv = vec2<f32>(f32((vi << 1u) & 2u), f32(vi & 2u));
    var out: FullscreenOut;
    out.position = vec4<f32>(uv * 2.0 - 1.0, 0.0, 1.0);
    return out;
}

fn viewport_max() -> vec2<i32> {
    return vec2<i32>(frame.viewport.xy) - vec2<i32>(1, 1);
}

fn load_depth(p: vec2<i32>) -> f32 {
    return textureLoad(depth_tex, clamp(p, vec2<i32>(0, 0), viewport_max()), 0);
}

// View-space position of a pixel centre at a given depth.
fn view_position(p: vec2<i32>, depth: f32) -> vec3<f32> {
    let uv = (vec2<f32>(p) + 0.5) * frame.viewport.zw;
    let ndc = vec4<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0, depth, 1.0);
    let v = frame.inv_proj * ndc;
    return v.xyz / v.w;
}

fn view_z(depth: f32) -> f32 {
    let v = frame.inv_proj * vec4<f32>(0.0, 0.0, depth, 1.0);
    return v.z / v.w;
}

fn in_bounds(p: vec2<i32>) -> bool {
    return all(p >= vec2<i32>(0, 0)) && all(p <= viewport_max());
}

// Tangent along a pixel step, using the neighbour on the side with the smaller depth jump so that tangents do
// not bridge silhouettes. Zero when neither neighbour is on a surface.
fn tangent(p: vec3<f32>, pix: vec2<i32>, step: vec2<i32>) -> vec3<f32> {
    let qa = pix + step;
    let qb = pix - step;
    var da = 0.0;
    var db = 0.0;
    if in_bounds(qa) {
        da = load_depth(qa);
    }
    if in_bounds(qb) {
        db = load_depth(qb);
    }
    if da <= 0.0 && db <= 0.0 {
        return vec3<f32>(0.0);
    }
    if db <= 0.0 {
        return view_position(qa, da) - p;
    }
    if da <= 0.0 {
        return p - view_position(qb, db);
    }
    let ta = view_position(qa, da) - p;
    let tb = p - view_position(qb, db);
    return select(ta, tb, abs(ta.z) > abs(tb.z));
}

fn bayer4(p: vec2<i32>) -> f32 {
    var m = array<f32, 16>(0.0, 8.0, 2.0, 10.0, 12.0, 4.0, 14.0, 6.0, 3.0, 11.0, 1.0, 9.0, 15.0, 7.0, 13.0, 5.0);
    let i = (p.y & 3) * 4 + (p.x & 3);
    return m[i];
}

struct AoOut {
    @location(0) ao: vec4<f32>,
};

@fragment
fn fs_ao(in: FullscreenOut) -> AoOut {
    var out: AoOut;
    out.ao = vec4<f32>(1.0);
    let pix = vec2<i32>(in.position.xy);
    let d = load_depth(pix);
    if d <= 0.0 {
        return out;
    }
    let p = view_position(pix, d);

    // Surface normal from screen-space tangents (x right, y down on screen).
    let nc = cross(tangent(p, pix, vec2<i32>(0, 1)), tangent(p, pix, vec2<i32>(1, 0)));
    let nl = length(nc);
    if nl < 1e-12 {
        return out;
    }
    let n = nc / nl;

    let radius = frame.ao_params.x;
    var radius_px = radius * frame.ao_params.z;
    if frame.eye.w > 0.5 {
        radius_px = radius_px / max(-p.z, 1e-3);
    }
    radius_px = min(radius_px, 0.12 * frame.viewport.y);
    if radius_px < 1.5 {
        return out;
    }

    let r2 = radius * radius;
    let bias = 0.01 * radius;
    let rotation = (bayer4(pix) + 0.5) / 16.0 * TAU;
    var sum = 0.0;
    for (var i = 0; i < AO_SAMPLES; i = i + 1) {
        let alpha = (f32(i) + 0.5) / f32(AO_SAMPLES);
        let angle = alpha * AO_TURNS * TAU + rotation;
        let offset = vec2<f32>(cos(angle), sin(angle)) * (alpha * radius_px);
        let q_pix = pix + vec2<i32>(round(offset));
        if any(q_pix < vec2<i32>(0, 0)) || any(q_pix > viewport_max()) {
            continue;
        }
        let qd = load_depth(q_pix);
        if qd <= 0.0 {
            continue;
        }
        let v = view_position(q_pix, qd) - p;
        let vv = dot(v, v);
        let vn = dot(v, n);
        let f = max(r2 - vv, 0.0);
        sum = sum + f * f * f * max((vn - bias) / (vv + 0.01 * r2), 0.0);
    }
    let a = max(0.0, 1.0 - sum * 5.0 / (r2 * r2 * r2 * f32(AO_SAMPLES)));
    out.ao = vec4<f32>(a, a, a, 1.0);
    return out;
}

@fragment
fn fs_composite(in: FullscreenOut) -> @location(0) vec4<f32> {
    let pix = vec2<i32>(in.position.xy);
    let c = textureLoad(color_tex, pix, 0);
    let d0 = load_depth(pix);
    if d0 <= 0.0 || frame.ao_params.w < 0.5 {
        return c;
    }
    let z0 = view_z(d0);
    let tolerance = 0.04 * abs(z0) + 0.05;
    var sum = 0.0;
    var weight = 0.0;
    for (var y = -2; y <= 1; y = y + 1) {
        for (var x = -2; x <= 1; x = x + 1) {
            let q = clamp(pix + vec2<i32>(x, y), vec2<i32>(0, 0), viewport_max());
            let qd = textureLoad(depth_tex, q, 0);
            var w = 0.0;
            if qd > 0.0 {
                w = max(0.0, 1.0 - abs(view_z(qd) - z0) / tolerance);
            }
            sum = sum + textureLoad(ao_tex, q, 0).r * w;
            weight = weight + w;
        }
    }
    var ao = 1.0;
    if weight > 1e-4 {
        ao = sum / weight;
    }
    let k = mix(1.0, ao, frame.ao_params.y);
    return vec4<f32>(c.rgb * k, c.a);
}
