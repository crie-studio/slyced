// GPU ray casting of a scalar volume plus a blit pass that composites the
// offscreen result into egui's render pass.
//
// Three lightings: simple (headlight, as in workstations of the 2000s),
// realistic (key light with soft shadows, ambient occlusion, rim light and
// tone mapping, in one pass) and cinematic (the same model with random light
// positions, occlusion rays and sub-pixel offsets, averaged over many frames).

struct Uniforms {
    // Clip space -> texture space ([0,1]^3 over the volume box).
    inv_mvp: mat4x4<f32>,
    crop_min: vec4<f32>,
    crop_max: vec4<f32>,
    // xyz: physical box size (mm). w: reference distance (mm) for opacity correction.
    extent: vec4<f32>,
    // xyz: voxel counts. w: samples per voxel along the ray.
    dims: vec4<f32>,
    // x: real value at texel 0. y: real span of texel range [0,1].
    // z: transfer function / window low. w: 1 / (transfer function / window width).
    value: vec4<f32>,
    // ambient, diffuse, specular, shininess
    light: vec4<f32>,
    // x: mode (0 composite, 1 MIP, 2 MinIP, 3 isosurface). y: iso level (real).
    // z: opacity multiplier. w: shading on/off.
    params: vec4<f32>,
    bg_top: vec4<f32>,
    bg_bottom: vec4<f32>,
    iso_color: vec4<f32>,
    // x: lighting (0 simple, 1 realistic, 2 cinematic). y: index of the frame being averaged.
    look: vec4<f32>,
};

@group(0) @binding(0) var<uniform> u: Uniforms;
@group(0) @binding(1) var vol_tex: texture_3d<f32>;
@group(0) @binding(2) var tf_tex: texture_2d<f32>;
@group(0) @binding(3) var lin: sampler;

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) ndc: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) i: u32) -> VsOut {
    var corners = array<vec2<f32>, 3>(vec2<f32>(-1.0, -1.0), vec2<f32>(3.0, -1.0), vec2<f32>(-1.0, 3.0));
    var o: VsOut;
    o.pos = vec4<f32>(corners[i], 0.0, 1.0);
    o.ndc = corners[i];
    return o;
}

fn value_at(p: vec3<f32>) -> f32 {
    return textureSampleLevel(vol_tex, lin, p, 0.0).r * u.value.y + u.value.x;
}

// Gradient in real units per mm. A 1.5 texel stencil smooths the normals of
// voxelized surfaces (fewer contour rings) at no extra cost.
fn gradient(p: vec3<f32>) -> vec3<f32> {
    return gradient_over(p, 1.5);
}

fn gradient_over(p: vec3<f32>, texels: f32) -> vec3<f32> {
    let d = texels / u.dims.xyz;
    let gx = value_at(p + vec3<f32>(d.x, 0.0, 0.0)) - value_at(p - vec3<f32>(d.x, 0.0, 0.0));
    let gy = value_at(p + vec3<f32>(0.0, d.y, 0.0)) - value_at(p - vec3<f32>(0.0, d.y, 0.0));
    let gz = value_at(p + vec3<f32>(0.0, 0.0, d.z)) - value_at(p - vec3<f32>(0.0, 0.0, d.z));
    return vec3<f32>(gx, gy, gz) / (2.0 * d * u.extent.xyz);
}

fn shade(p: vec3<f32>, base: vec3<f32>, view_dir: vec3<f32>) -> vec3<f32> {
    let g = gradient(p);
    let glen = length(g);
    if (glen < 1e-3) {
        return base * (u.light.x + u.light.y * 0.6);
    }
    let n = -g / glen;
    // Headlight: light comes from the camera, so N.L == N.V == N.H.
    let ndl = abs(dot(n, -view_dir));
    let spec = u.light.z * pow(ndl, u.light.w);
    return base * (u.light.x + u.light.y * ndl) + vec3<f32>(spec);
}

// PCG integer hash: unlike sin()-based hashes it stays well distributed on
// GPUs with low-precision transcendental functions.
fn pcg(v: u32) -> u32 {
    let state = v * 747796405u + 2891336453u;
    let word = ((state >> ((state >> 28u) + 4u)) ^ state) * 277803737u;
    return (word >> 22u) ^ word;
}

fn hash(p: vec2<f32>) -> f32 {
    let h = pcg(u32(p.x) ^ pcg(u32(p.y)));
    return f32(h) * (1.0 / 4294967296.0);
}

// Random number in [0, 1) from a running seed.
fn rand(seed: ptr<function, u32>) -> f32 {
    *seed = pcg(*seed);
    return f32(*seed) * (1.0 / 4294967296.0);
}

fn safe_inv(v: vec3<f32>) -> vec3<f32> {
    let e = 1e-8;
    let s = select(vec3<f32>(e), vec3<f32>(-e), v < vec3<f32>(0.0));
    let w = select(v, s, abs(v) < vec3<f32>(e));
    return 1.0 / w;
}

// ---------------------------------------------------------------- realistic light

// Opacity per reference length at a point: what blocks light (the transfer
// function, or the iso level for surfaces).
fn occluder(p: vec3<f32>) -> f32 {
    let v = value_at(p);
    if (i32(u.params.x + 0.5) == 3) {
        return min(smoothstep(u.params.y - 30.0, u.params.y + 30.0, v), 0.9999);
    }
    let t = clamp((v - u.value.z) * u.value.w, 0.0, 1.0);
    return min(textureSampleLevel(tf_tex, lin, vec2<f32>(t, 0.5), 0.0).a * u.params.z, 0.9999);
}

fn inside_box(q: vec3<f32>) -> bool {
    return all(q >= u.crop_min.xyz) && all(q <= u.crop_max.xyz);
}

// Fraction of light that reaches `p` along direction `l` (mm space), over `len_mm`.
fn transmittance(p: vec3<f32>, l: vec3<f32>, steps: i32, len_mm: f32, offset: f32) -> f32 {
    let dt = len_mm / f32(steps);
    let d = l * dt / u.extent.xyz;
    var tr = 1.0;
    for (var i = 0; i < steps; i = i + 1) {
        // Start a little away from the point so a surface does not shadow itself.
        let q = p + d * (f32(i) + 0.6 + offset);
        if (!inside_box(q)) {
            break;
        }
        tr = tr * pow(1.0 - occluder(q), dt / u.extent.w);
        if (tr < 0.02) {
            return 0.0;
        }
    }
    return tr;
}

struct Lights {
    key: vec3<f32>,
    right: vec3<f32>,
    up: vec3<f32>,
};

// Key light from the upper left of the viewer, in mm space (it follows the camera).
fn lights(view_dir: vec3<f32>) -> Lights {
    let c0 = u.inv_mvp * vec4<f32>(0.0, 0.0, 0.5, 1.0);
    let cx = u.inv_mvp * vec4<f32>(1.0, 0.0, 0.5, 1.0);
    let cy = u.inv_mvp * vec4<f32>(0.0, 1.0, 0.5, 1.0);
    let o = c0.xyz / c0.w;
    let right = normalize((cx.xyz / cx.w - o) * u.extent.xyz);
    let up = normalize((cy.xyz / cy.w - o) * u.extent.xyz);
    var out: Lights;
    out.key = normalize(-view_dir * 0.55 + up * 0.65 - right * 0.55);
    out.right = right;
    out.up = up;
    return out;
}

// Light that reaches a point from its surroundings: deterministic probes
// (realistic) or one random ray per frame (cinematic, averaged over frames).
fn ambient_light(p: vec3<f32>, n: vec3<f32>, lt: Lights, cinematic: bool, seed: ptr<function, u32>) -> f32 {
    if (cinematic) {
        // Cosine-weighted direction around the normal.
        let r1 = rand(seed);
        let r2 = rand(seed);
        let phi = 6.2831853 * r1;
        let s = sqrt(r2);
        let t = normalize(cross(n, select(vec3<f32>(1.0, 0.0, 0.0), vec3<f32>(0.0, 1.0, 0.0), abs(n.x) > 0.7)));
        let b = cross(n, t);
        let dir = normalize(t * cos(phi) * s + b * sin(phi) * s + n * sqrt(1.0 - r2));
        return transmittance(p, dir, 6, 24.0, rand(seed));
    }
    var open = 0.0;
    var dirs = array<vec3<f32>, 5>(n, normalize(n + lt.right * 0.8), normalize(n - lt.right * 0.8), normalize(n + lt.up * 0.8), normalize(n - lt.up * 0.8));
    for (var i = 0; i < 5; i = i + 1) {
        let a = occluder(p + dirs[i] * 3.0 / u.extent.xyz);
        let b = occluder(p + dirs[i] * 7.0 / u.extent.xyz);
        open = open + (1.0 - a) * (1.0 - 0.6 * b);
    }
    return clamp(open / 5.0, 0.0, 1.0);
}

// Key light with soft shadow, wrapped diffuse (softer on tissue), specular,
// ambient occlusion and a cool rim on silhouettes. Tissue with weak gradients
// is lit like a scattering medium instead of a surface.
fn shade_real(p: vec3<f32>, base: vec3<f32>, view_dir: vec3<f32>, lt: Lights, cinematic: bool, seed: ptr<function, u32>) -> vec3<f32> {
    // Wider stencil: the side light shows the relief, so voxel steps would show too.
    let g = gradient_over(p, 2.0);
    let glen = length(g);
    var n = -view_dir;
    if (glen > 1e-3) {
        n = -g / glen;
    }
    // Faces turned away (inside a cavity seen through tissue) are lit from the front; on the
    // silhouette the normal only loses its backward part, so it does not flip between pixels.
    let back = dot(n, view_dir);
    if (back > 0.3) {
        n = -n;
    } else if (back > 0.0) {
        n = normalize(n - view_dir * back);
    }
    let surf = 1.0 - exp(-glen * 0.012);
    var l = lt.key;
    var shadow_len = 45.0;
    var shadow_steps = 14;
    var offset = 0.0;
    if (cinematic) {
        // Area light: a random point of a disc around the key direction gives soft shadows.
        let a = 6.2831853 * rand(seed);
        let r = 0.22 * sqrt(rand(seed));
        l = normalize(l + (lt.right * cos(a) + lt.up * sin(a)) * r);
        // Fewer, longer steps with a random start: the average over frames fills the gaps.
        shadow_len = 60.0;
        shadow_steps = 12;
        offset = rand(seed);
    }
    // Start just outside the surface: samples a fraction of a voxel deep would otherwise
    // be shadowed by the surface itself, giving a grainy look.
    let ps = p + n * 2.0 / u.extent.xyz;
    let shadow = transmittance(ps, l, shadow_steps, shadow_len, offset);
    let ambient = ambient_light(ps, n, lt, cinematic, seed);
    let ndl = dot(n, l);
    let diffuse = mix(0.5, max((ndl + 0.15) / 1.15, 0.0), surf);
    let h = normalize(l - view_dir);
    let spec = pow(max(dot(n, h), 0.0), max(u.light.w, 16.0)) * (u.light.z * 0.8 + 0.1) * surf;
    let rim = pow(1.0 - abs(dot(n, view_dir)), 3.0) * 0.16 * surf;
    let key = vec3<f32>(1.0, 0.95, 0.88) * 1.05;
    let sky = vec3<f32>(0.55, 0.62, 0.75) * (0.2 + 0.5 * u.light.x);
    return base * (sky * ambient + key * diffuse * shadow) + key * spec * shadow + vec3<f32>(0.5, 0.6, 0.8) * rim * ambient;
}

// Soft shoulder: colors up to 0.75 stay as they are (hue and background
// included) and brighter ones roll off instead of burning out.
fn tonemap(c: vec3<f32>) -> vec3<f32> {
    let k = 0.75;
    let over = max(c - vec3<f32>(k), vec3<f32>(0.0));
    return min(c, vec3<f32>(k)) + (1.0 - k) * (1.0 - exp(-over / (1.0 - k)));
}

// ---------------------------------------------------------------- main pass

@fragment
fn fs_volume(in: VsOut) -> @location(0) vec4<f32> {
    let lighting = i32(u.look.x + 0.5);
    let frame = u32(u.look.y + 0.5);
    // The first cinematic frame uses the realistic light (it is also what shows while the
    // volume moves); the next ones add random samples to the running average.
    let cinematic = lighting == 2 && frame > 0u;
    var seed = pcg(u32(in.pos.x) ^ pcg(u32(in.pos.y) ^ pcg(frame * 9781u + 17u)));
    // Cinematic: each frame looks through a slightly different point of the pixel (anti-aliasing).
    let px = vec2<f32>(dpdx(in.ndc.x), dpdy(in.ndc.y));
    var ndc = in.ndc;
    if (cinematic) {
        ndc = ndc + (vec2<f32>(rand(&seed), rand(&seed)) - 0.5) * px;
    }
    let bg = mix(u.bg_bottom.rgb, u.bg_top.rgb, clamp(ndc.y * 0.5 + 0.5, 0.0, 1.0));
    // Weight of this frame in the running average (cinematic).
    let weight = select(1.0, 1.0 / (f32(frame) + 1.0), cinematic);

    let n4 = u.inv_mvp * vec4<f32>(ndc, 0.0, 1.0);
    let f4 = u.inv_mvp * vec4<f32>(ndc, 1.0, 1.0);
    let ro = n4.xyz / n4.w;
    let rd = f4.xyz / f4.w - ro;

    let inv = safe_inv(rd);
    let ta = (u.crop_min.xyz - ro) * inv;
    let tb = (u.crop_max.xyz - ro) * inv;
    let tmin = min(ta, tb);
    let tmax = max(ta, tb);
    let t0 = max(max(max(tmin.x, tmin.y), tmin.z), 0.0);
    let t1 = min(min(min(tmax.x, tmax.y), tmax.z), 1.0);
    if (t1 <= t0) {
        var out = bg;
        if (lighting > 0) {
            out = tonemap(bg);
        }
        return vec4<f32>(out, weight);
    }

    let entry = ro + rd * t0;
    let seg = rd * (t1 - t0);
    let len_vox = length(seg * u.dims.xyz);
    let n = clamp(i32(ceil(len_vox * u.dims.w)), 1, 4096);
    let step = seg / f32(n);
    let step_mm = length(step * u.extent.xyz);
    let view_dir = normalize(rd * u.extent.xyz);
    var jitter = hash(in.pos.xy);
    if (cinematic) {
        jitter = rand(&seed);
    }
    let mode = i32(u.params.x + 0.5);
    let lt = lights(view_dir);

    if (mode == 1 || mode == 2) {
        var m = select(3.0e38, -3.0e38, mode == 1);
        for (var i = 0; i < n; i = i + 1) {
            let v = value_at(entry + step * (f32(i) + jitter));
            m = select(min(m, v), max(m, v), mode == 1);
        }
        let g = clamp((m - u.value.z) * u.value.w, 0.0, 1.0);
        return vec4<f32>(vec3<f32>(g), 1.0);
    }

    if (mode == 3) {
        var prev = entry;
        for (var i = 0; i < n; i = i + 1) {
            let p = entry + step * (f32(i) + jitter);
            if (value_at(p) >= u.params.y) {
                // Refine the crossing by bisection between the previous and current sample.
                var a = prev;
                var b = p;
                for (var k = 0; k < 6; k = k + 1) {
                    let mid = (a + b) * 0.5;
                    if (value_at(mid) >= u.params.y) { b = mid; } else { a = mid; }
                }
                if (lighting == 0) {
                    return vec4<f32>(shade(b, u.iso_color.rgb, view_dir), 1.0);
                }
                return vec4<f32>(tonemap(shade_real(b, u.iso_color.rgb, view_dir, lt, cinematic, &seed)), weight);
            }
            prev = p;
        }
        var out = bg;
        if (lighting > 0) {
            out = tonemap(bg);
        }
        return vec4<f32>(out, weight);
    }

    var acc = vec4<f32>(0.0);
    for (var i = 0; i < n; i = i + 1) {
        let p = entry + step * (f32(i) + jitter);
        let v = value_at(p);
        let t = clamp((v - u.value.z) * u.value.w, 0.0, 1.0);
        let c = textureSampleLevel(tf_tex, lin, vec2<f32>(t, 0.5), 0.0);
        var a = min(c.a * u.params.z, 1.0);
        if (a > 0.002) {
            a = 1.0 - pow(1.0 - min(a, 0.9999), step_mm / u.extent.w);
            var col = c.rgb;
            if (u.params.w > 0.5) {
                // The costly light only where the sample still shows.
                if (lighting > 0 && (1.0 - acc.a) * a > 0.01) {
                    col = shade_real(p, col, view_dir, lt, cinematic, &seed);
                } else {
                    col = shade(p, col, view_dir);
                }
            }
            acc = vec4<f32>(acc.rgb + (1.0 - acc.a) * a * col, acc.a + (1.0 - acc.a) * a);
            if (acc.a > 0.985) {
                break;
            }
        }
    }
    var out = acc.rgb + (1.0 - acc.a) * bg;
    if (lighting > 0) {
        out = tonemap(out);
    }
    return vec4<f32>(out, weight);
}
