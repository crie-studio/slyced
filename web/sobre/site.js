'use strict';
const LANG = document.documentElement.lang.startsWith('en') ? 'en' : 'pt';
const T = {
  pt: { axial: 'Corte axial', coronal: 'Corte coronal', sagittal: 'Corte sagital', pos: 'Posição', phantom: 'fantoma sintético', slice: 'corte', slices: 'cortes empilhados', volume: 'volume na placa de vídeo', computer: 'SEU COMPUTADOR', cloud: 'NUVEM', modes: ['osso', 'angio', 'pele', 'MIP'], cut: ' + corte', locale: 'pt-BR', noGL: 'WebGL2 indisponível' },
  en: { axial: 'Axial slice', coronal: 'Coronal slice', sagittal: 'Sagittal slice', pos: 'Position', phantom: 'synthetic phantom', slice: 'slice', slices: 'stacked slices', volume: 'volume on the GPU', computer: 'YOUR COMPUTER', cloud: 'CLOUD', modes: ['bone', 'angio', 'skin', 'MIP'], cut: ' + cutaway', locale: 'en-US', noGL: 'WebGL2 unavailable' },
}[LANG];
const $ = (s, el = document) => el.querySelector(s);
const $$ = (s, el = document) => [...el.querySelectorAll(s)];
const reduced = matchMedia('(prefers-reduced-motion: reduce)').matches;
const clamp = (x, a = 0, b = 1) => Math.min(b, Math.max(a, x));
const lerp = (a, b, t) => a + (b - a) * t;
const ease = t => t < .5 ? 4 * t * t * t : 1 - Math.pow(-2 * t + 2, 3) / 2;

/* ════════ WebGL: a synthetic head phantom in Hounsfield units ════════ */
const PHANTOM = `
float hash13(vec3 p3) { p3 = fract(p3 * 0.1031); p3 += dot(p3, p3.zyx + 31.32); return fract((p3.x + p3.y) * p3.z); }
float sdE(vec3 p, vec3 r) { float k0 = length(p / r); float k1 = length(p / (r * r)); return k0 * (k0 - 1.0) / max(k1, 1e-5); }
float sdCyl(vec3 p, float r, float y0, float y1) { vec2 d = vec2(length(p.xz) - r, max(y0 - p.y, p.y - y1)); return min(max(d.x, d.y), 0.0) + length(max(d, 0.0)); }
float sdSeg(vec3 p, vec3 a, vec3 b, float r) { vec3 pa = p - a, ba = b - a; float h = clamp(dot(pa, ba) / dot(ba, ba), 0.0, 1.0); return length(pa - ba * h) - r; }
float sdTorus(vec3 p, float R, float r) { return length(vec2(length(p.xz) - R, p.y)) - r; }
float inside(float d, float w) { return smoothstep(w, -w, d); }
const vec3 HEAD = vec3(0.0, 0.12, 0.0);
float bodySdf(vec3 p) {
  float head = sdE(p - HEAD, vec3(0.60, 0.66, 0.70));
  float neck = sdCyl(p - vec3(0.0, 0.0, -0.06), 0.30, -1.02, -0.30);
  return min(head, neck);
}
float hu(vec3 p) {
  float body = bodySdf(p);
  if (body > 0.02) return -1000.0;
  vec3 q = p - HEAD;
  vec3 qm = vec3(abs(q.x), q.y, q.z);
  vec3 pm = vec3(abs(p.x), p.y, p.z);
  float v = mix(-1000.0, 38.0, inside(body, 0.008));
  float outer = sdE(q, vec3(0.565, 0.625, 0.665));
  float inner = sdE(q, vec3(0.515, 0.575, 0.615));
  v = mix(v, 1350.0, inside(max(outer, -inner), 0.007));
  v = mix(v, 36.0, inside(inner, 0.007));
  v -= 7.0 * inside(sdE(q - vec3(0.0, 0.03, 0.0), vec3(0.37, 0.40, 0.46)), 0.03);
  float vent = sdE(qm - vec3(0.075, 0.07, -0.02), vec3(0.045, 0.085, 0.2));
  v = mix(v, 6.0, inside(vent, 0.008));
  float lesion = length(q - vec3(-0.21, 0.2, 0.16)) - 0.065;
  v = mix(v, 95.0, inside(lesion, 0.01));
  float ring = sdTorus(q - vec3(0.0, -0.22, 0.04), 0.11, 0.016);
  float mca = sdSeg(qm, vec3(0.10, -0.22, 0.07), vec3(0.42, -0.06, 0.10), 0.014);
  float aca = sdSeg(q, vec3(0.0, -0.20, 0.15), vec3(0.0, 0.38, 0.32), 0.012);
  v = mix(v, 330.0, inside(min(ring, min(mca, aca)), 0.006));
  float orbit = length(qm - vec3(0.21, -0.03, 0.55)) - 0.12;
  v = mix(v, 15.0, inside(orbit, 0.008));
  float carotid = sdSeg(pm, vec3(0.13, -1.02, 0.02), vec3(0.11, -0.10, 0.05), 0.026);
  v = mix(v, 330.0, inside(carotid, 0.006));
  float spine = sdCyl(p - vec3(0.0, 0.0, -0.15), 0.085, -1.02, -0.30);
  float disc = smoothstep(0.78, 0.84, fract(p.y * 9.0));
  v = mix(v, mix(780.0, 80.0, disc), inside(spine, 0.006));
  float airway = sdCyl(p - vec3(0.0, 0.0, 0.11), 0.05, -1.02, -0.40);
  v = mix(v, -950.0, inside(airway, 0.008));
  return v;
}
float huN(vec3 p) { return hu(p) + (hash13(floor(p * 480.0)) - 0.5) * 15.0 + (hash13(floor(p * 160.0) + 7.0) - 0.5) * 6.0; }
float win(float h, float w, float l) { return clamp((h - (l - 0.5 * w)) / w, 0.0, 1.0); }
`;
const VS = `#version 300 es
void main() { vec2 p = vec2(float((gl_VertexID << 1) & 2), float(gl_VertexID & 2)); gl_Position = vec4(p * 2.0 - 1.0, 0.0, 1.0); }`;

const VR_FS = `#version 300 es
precision highp float;
out vec4 outColor;
uniform vec2 uRes; uniform vec3 uEye; uniform mat3 uCam; uniform float uFov;
uniform int uPreset; uniform float uClip; uniform vec3 uCut;
${PHANTOM}
// Cutaway: the octant x > cut.x, y > cut.y, z > cut.z is removed, exposing three MPR faces.
bool removed(vec3 p) { return uClip > 0.5 && p.x > uCut.x && p.y > uCut.y && p.z > uCut.z; }
vec2 boxHit(vec3 ro, vec3 rd) {
  vec3 inv = 1.0 / rd;
  vec3 t0 = (vec3(-0.68, -1.04, -0.76) - ro) * inv, t1 = (vec3(0.68, 0.80, 0.76) - ro) * inv;
  vec3 a = min(t0, t1), b = max(t0, t1);
  return vec2(max(max(a.x, a.y), a.z), min(min(b.x, b.y), b.z));
}
vec3 grad(vec3 p) {
  const vec2 k = vec2(1.0, -1.0); const float e = 0.006;
  return k.xyy * hu(p + k.xyy * e) + k.yyx * hu(p + k.yyx * e) + k.yxy * hu(p + k.yxy * e) + k.xxx * hu(p + k.xxx * e);
}
vec4 tf(float h) {
  if (uPreset == 0) {
    float a = smoothstep(180.0, 520.0, h);
    vec3 c = mix(vec3(0.93, 0.74, 0.58), vec3(1.0, 0.96, 0.89), smoothstep(220.0, 700.0, h));
    return vec4(c, a * 0.85);
  }
  if (uPreset == 1) {
    float vessel = smoothstep(200.0, 290.0, h) * (1.0 - smoothstep(480.0, 650.0, h));
    float bone = smoothstep(650.0, 1100.0, h);
    vec3 c = vessel > bone ? vec3(0.97, 0.22, 0.26) : vec3(0.93, 0.90, 0.84);
    return vec4(c, max(vessel * 0.95, bone * 0.09));
  }
  float a = smoothstep(-600.0, -150.0, h) * 0.9;
  return vec4(0.95, 0.73, 0.62, a);
}
void main() {
  vec2 uv = (gl_FragCoord.xy * 2.0 - uRes) / uRes.y;
  vec3 rd = normalize(uCam * vec3(uv * uFov, -1.0));
  vec3 ro = uEye;
  vec2 tb = boxHit(ro, rd);
  vec4 acc = vec4(0.0);
  float mip = -1000.0;
  if (tb.x < tb.y && tb.y > 0.0) {
    float t = max(tb.x, 0.0), tEnd = tb.y;
    const float dt = 0.008;
    // Per-pixel jitter where the march reaches the body turns step banding into fine grain.
    float jit = hash13(vec3(gl_FragCoord.xy, 1.7));
    vec3 L = normalize(-rd + vec3(0.35, 0.55, 0.0));
    bool cutTissue = false, near = false;
    for (int i = 0; i < 340; i++) {
      if (t > tEnd || acc.a > 0.97) break;
      vec3 p = ro + rd * t;
      float d = bodySdf(p);
      if (d > 0.03) { t += max(d * 0.8 - 0.01, dt); cutTissue = false; near = false; continue; }
      if (!near) { near = true; t += dt * jit; p = ro + rd * t; }
      if (removed(p)) { cutTissue = hu(p) > -600.0; t += dt; continue; }
      float h = hu(p);
      // Leaving the removed octant inside the body: this is a cut face, drawn like a CT slice.
      if (cutTissue && uPreset != 3 && h > -600.0) {
        vec3 e = abs(p - uCut);
        float g = win(hu(p) + (hash13(floor(p * 300.0)) - 0.5) * 7.0, 110.0, 40.0);
        vec3 c = vec3(g) * vec3(0.94, 0.98, 1.04);
        // Where two faces meet, the line takes the other plane's MPR color
        // (red axial, green coronal, yellow sagittal).
        const vec3 RED = vec3(1.0, 0.36, 0.42), GREEN = vec3(0.24, 0.86, 0.52), YELLOW = vec3(1.0, 0.79, 0.3);
        vec3 ec; float s2;
        if (e.x <= e.y && e.x <= e.z) { ec = e.y < e.z ? RED : GREEN; s2 = min(e.y, e.z); }
        else if (e.y <= e.z) { ec = e.x < e.z ? YELLOW : GREEN; s2 = min(e.x, e.z); }
        else { ec = e.x < e.y ? YELLOW : RED; s2 = min(e.x, e.y); }
        c = mix(c, ec, smoothstep(0.013, 0.005, s2) * 0.9);
        acc.rgb += (1.0 - acc.a) * c; acc.a = 1.0;
        break;
      }
      cutTissue = false;
      if (uPreset == 3) { mip = max(mip, h); t += dt * 1.5; continue; }
      vec4 c = tf(h);
      if (c.a > 0.003) {
        vec3 n = -normalize(grad(p) + vec3(1e-5));
        float diff = max(dot(n, L), 0.0);
        float spec = pow(max(dot(n, normalize(L - rd)), 0.0), 30.0) * 0.32;
        float rim = pow(1.0 - max(dot(n, -rd), 0.0), 3.0) * 0.25;
        vec3 col = c.rgb * (0.30 + 0.80 * diff) + spec + rim * vec3(0.55, 0.62, 1.0);
        float a = 1.0 - pow(1.0 - c.a, dt / 0.01);
        acc.rgb += (1.0 - acc.a) * a * col;
        acc.a += (1.0 - acc.a) * a;
      }
      t += dt;
    }
  }
  if (uPreset == 3) {
    float g = win(mip, 1500.0, 330.0);
    float a = smoothstep(0.0, 0.25, g);
    acc = vec4(vec3(pow(g, 0.9)) * vec3(0.92, 0.97, 1.05) * a, a);
  }
  outColor = acc;
}`;

const MPR_FS = `#version 300 es
precision highp float;
out vec4 outColor;
uniform vec2 uRes; uniform vec3 uC, uU, uV; uniform float uS; uniform vec2 uWL;
uniform vec2 uLine; uniform vec3 uColX, uColY; uniform float uLines, uMask, uDpr;
${PHANTOM}
void main() {
  float m = min(uRes.x, uRes.y);
  vec2 uv = (gl_FragCoord.xy * 2.0 - uRes) / m;
  vec3 p = uC + uU * uv.x * uS + uV * uv.y * uS;
  float h = huN(p);
  vec3 col = vec3(win(h, uWL.x, uWL.y));
  float alpha = 1.0;
  if (uMask > 0.5) { alpha = smoothstep(-700.0, -450.0, h); col *= alpha; }
  if (uLines > 0.5) {
    float px = 2.0 / m * uDpr;
    vec2 d = abs(uv - uLine) / px;
    float r = length(uv - uLine) / px;
    float gap = smoothstep(10.0, 13.0, r);
    col = mix(col, uColX, (1.0 - smoothstep(0.4, 1.3, d.x)) * gap * 0.9);
    col = mix(col, uColY, (1.0 - smoothstep(0.4, 1.3, d.y)) * gap * 0.9);
    col = mix(col, vec3(1.0), (1.0 - smoothstep(0.5, 1.5, abs(r - 6.5))) * 0.85);
  }
  outColor = vec4(col, alpha);
}`;

function makeGL(canvas, opts = {}) {
  const gl = canvas.getContext('webgl2', { antialias: false, alpha: true, premultipliedAlpha: true, powerPreference: 'high-performance', ...opts });
  return gl || null;
}
function makeProgram(gl, fs) {
  const sh = (type, src) => {
    const s = gl.createShader(type); gl.shaderSource(s, src); gl.compileShader(s);
    if (!gl.getShaderParameter(s, gl.COMPILE_STATUS)) throw new Error(gl.getShaderInfoLog(s));
    return s;
  };
  const p = gl.createProgram();
  gl.attachShader(p, sh(gl.VERTEX_SHADER, VS)); gl.attachShader(p, sh(gl.FRAGMENT_SHADER, fs)); gl.linkProgram(p);
  if (!gl.getProgramParameter(p, gl.LINK_STATUS)) throw new Error(gl.getProgramInfoLog(p));
  const u = {}; const n = gl.getProgramParameter(p, gl.ACTIVE_UNIFORMS);
  for (let i = 0; i < n; i++) { const name = gl.getActiveUniform(p, i).name; u[name] = gl.getUniformLocation(p, name); }
  gl.useProgram(p);
  return { p, u };
}
const v3 = {
  sub: (a, b) => [a[0] - b[0], a[1] - b[1], a[2] - b[2]],
  add: (a, b) => [a[0] + b[0], a[1] + b[1], a[2] + b[2]],
  mul: (a, s) => [a[0] * s, a[1] * s, a[2] * s],
  dot: (a, b) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2],
  cross: (a, b) => [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]],
  norm: a => { const l = Math.hypot(...a) || 1; return [a[0] / l, a[1] / l, a[2] / l]; },
};

/* One render loop for everything animated; each item renders only while on screen. */
const loop = new Set();
let lastNow = performance.now();
function tick(now) {
  const dt = Math.min(0.1, (now - lastNow) / 1000); lastNow = now;
  if (!document.hidden) for (const item of loop) if (item.visible) item.frame(now / 1000, dt);
  requestAnimationFrame(tick);
}
function watch(el, item) {
  item.visible = false;
  new IntersectionObserver(([e]) => { item.visible = e.isIntersecting; if (item.visible && item.wake) item.wake(); }, { rootMargin: '120px' }).observe(el);
  loop.add(item);
}

/* ── volume renderer (hero and the end of the scroll story) ── */
class VolumeView {
  constructor(canvas, { interactive = false, preset = 0, clip = false, quality = 0.75, hud = null } = {}) {
    this.canvas = canvas; this.gl = makeGL(canvas);
    if (!this.gl) throw new Error(T.noGL);
    this.prog = makeProgram(this.gl, VR_FS);
    this.preset = preset; this.clip = clip; this.q = quality; this.qMax = quality;
    this.yaw = 0.55; this.pitch = 0.12; this.vYaw = 0; this.vPitch = 0; this.dragging = false; this.auto = !reduced;
    this.hud = hud; this.fpsAcc = 0; this.fpsN = 0; this.slow = 0; this.fast = 0; this.dirty = true; this.time = 0;
    if (interactive) this.bindDrag();
  }
  bindDrag() {
    const c = this.canvas; let lx = 0, ly = 0, lt = 0;
    c.addEventListener('pointerdown', e => { this.dragging = true; c.setPointerCapture(e.pointerId); c.classList.add('drag'); lx = e.clientX; ly = e.clientY; lt = performance.now(); this.vYaw = this.vPitch = 0; });
    c.addEventListener('pointermove', e => {
      if (!this.dragging) return;
      const dx = e.clientX - lx, dy = e.clientY - ly, now = performance.now(), dts = Math.max(1, now - lt) / 1000;
      this.yaw -= dx * 0.008; this.pitch = clamp(this.pitch + dy * 0.006, -1.2, 1.2);
      this.vYaw = -dx * 0.008 / dts; this.vPitch = dy * 0.006 / dts; lx = e.clientX; ly = e.clientY; lt = now; this.dirty = true;
    });
    const up = () => { this.dragging = false; c.classList.remove('drag'); };
    c.addEventListener('pointerup', up); c.addEventListener('pointercancel', up);
  }
  resize() {
    const w = this.canvas.clientWidth, h = this.canvas.clientHeight, s = Math.min(devicePixelRatio || 1, 2) * this.q;
    const W = Math.max(2, Math.round(w * s)), H = Math.max(2, Math.round(h * s));
    if (this.canvas.width !== W || this.canvas.height !== H) { this.canvas.width = W; this.canvas.height = H; this.dirty = true; }
  }
  wake() { this.dirty = true; }
  frame(t, dt) {
    const animating = this.auto || this.dragging || Math.abs(this.vYaw) > 0.01;
    if (!animating && !this.dirty) return;
    this.time += reduced ? 0 : dt;
    if (!this.dragging) {
      this.yaw += this.vYaw * dt; this.pitch = clamp(this.pitch + this.vPitch * dt, -1.2, 1.2);
      const k = Math.exp(-dt * 3); this.vYaw *= k; this.vPitch *= k;
      if (this.auto) this.yaw += dt * 0.32;
    }
    this.resize(); this.render(); this.dirty = false;
    // Adaptive resolution: keep the animation fluid on modest GPUs.
    if (dt > 0) {
      this.fpsAcc += dt; this.fpsN++;
      // Only real struggle counts: some screens and power modes cap the page at 30 fps.
      if (dt > 1 / 24) this.slow++; else if (dt < 1 / 50) this.fast++;
      if (this.fpsAcc > 0.5) {
        if (this.hud) this.hud.textContent = Math.round(this.fpsN / this.fpsAcc);
        if (this.slow > this.fpsN * 0.5 && this.q > 0.45) this.q *= 0.88;
        else if (this.fast > this.fpsN * 0.9 && this.q < this.qMax) this.q = Math.min(this.qMax, this.q * 1.08);
        this.fpsAcc = 0; this.fpsN = 0; this.slow = 0; this.fast = 0;
      }
    }
  }
  render() {
    const { gl, prog: { u } } = this, W = this.canvas.width, H = this.canvas.height;
    const target = [0, -0.12, 0], dist = 3.25;
    const eye = [target[0] + dist * Math.cos(this.pitch) * Math.sin(this.yaw), target[1] + dist * Math.sin(this.pitch), target[2] + dist * Math.cos(this.pitch) * Math.cos(this.yaw)];
    const f = v3.norm(v3.sub(target, eye)), r = v3.norm(v3.cross(f, [0, 1, 0])), up = v3.cross(r, f);
    gl.viewport(0, 0, W, H);
    gl.uniform2f(u.uRes, W, H);
    gl.uniform3fv(u.uEye, eye);
    gl.uniformMatrix3fv(u.uCam, false, [...r, ...up, ...v3.mul(f, -1)]);
    gl.uniform1f(u.uFov, 0.4);
    gl.uniform1i(u.uPreset, this.preset);
    gl.uniform1f(u.uClip, this.clip ? 1 : 0);
    // The corner of the cutaway drifts slowly, like moving the MPR crosshair.
    const k = this.time;
    gl.uniform3fv(u.uCut, [0.02 + 0.07 * Math.sin(k * 0.5), 0.14 + 0.1 * Math.sin(k * 0.37 + 1), 0.02 + 0.07 * Math.sin(k * 0.43 + 2)]);
    gl.drawArrays(gl.TRIANGLES, 0, 3);
  }
}

/* ── MPR: three linked planes ── */
class Mpr {
  constructor(grid) {
    this.cross = [0, 0.16, 0.02]; this.wl = [80, 40]; this.demo = !reduced; this.views = [];
    const RED = [1, .36, .42], GREEN = [.24, .86, .52], YELLOW = [1, .79, .3];
    const cfg = {
      axial: { U: [1, 0, 0], V: [0, 0, 1], s: 0.8, cx: YELLOW, cy: GREEN, center: c => [0, c[1], 0], set: (p, c) => { c[0] = p[0]; c[2] = p[2]; }, pos: c => c[1], name: T.axial },
      coronal: { U: [1, 0, 0], V: [0, 1, 0], s: 1.0, cx: YELLOW, cy: RED, center: c => [0, -0.12, c[2]], set: (p, c) => { c[0] = p[0]; c[1] = p[1]; }, pos: c => c[2], name: T.coronal },
      sagittal: { U: [0, 0, -1], V: [0, 1, 0], s: 1.0, cx: GREEN, cy: RED, center: c => [c[0], -0.12, 0], set: (p, c) => { c[2] = p[2]; c[1] = p[1]; }, pos: c => c[0], name: T.sagittal },
    };
    for (const el of $$('.view', grid)) {
      const canvas = $('canvas', el), gl = makeGL(canvas);
      if (!gl) throw new Error(T.noGL);
      const v = { el, canvas, gl, prog: makeProgram(gl, MPR_FS), ...cfg[el.dataset.view], meta: $('.meta', el), meta2: $('.meta2', el) };
      this.views.push(v); this.bind(v);
    }
    this.dirty = true; this.t = 0;
    $$('[data-wl]').forEach(b => b.addEventListener('click', () => {
      this.wl = b.dataset.wl.split(',').map(Number); this.dirty = true;
      $$('[data-wl]').forEach(x => x.setAttribute('aria-pressed', x === b));
    }));
    addEventListener('resize', () => this.dirty = true);
  }
  bind(v) {
    let mode = 0, lx = 0, ly = 0;
    const pick = e => {
      const r = v.canvas.getBoundingClientRect(), m = Math.min(r.width, r.height);
      const uv = [((e.clientX - r.left) * 2 - r.width) / m, -((e.clientY - r.top) * 2 - r.height) / m];
      const p = v3.add(v.center(this.cross), v3.add(v3.mul(v.U, uv[0] * v.s), v3.mul(v.V, uv[1] * v.s)));
      v.set(p, this.cross);
      this.cross = [clamp(this.cross[0], -0.66, 0.66), clamp(this.cross[1], -1.0, 0.78), clamp(this.cross[2], -0.74, 0.74)];
      this.dirty = true;
    };
    v.el.addEventListener('contextmenu', e => e.preventDefault());
    v.el.addEventListener('pointerdown', e => {
      this.demo = false; v.el.setPointerCapture(e.pointerId); lx = e.clientX; ly = e.clientY;
      mode = e.button === 2 ? 2 : 1; if (mode === 1) pick(e);
      this.views.forEach(x => x.el.classList.toggle('active', x === v));
    });
    v.el.addEventListener('pointermove', e => {
      if (mode === 1) pick(e);
      else if (mode === 2) {
        this.wl = [clamp(this.wl[0] + (e.clientX - lx) * 4, 1, 4000), clamp(this.wl[1] - (e.clientY - ly) * 2, -1000, 2000)];
        lx = e.clientX; ly = e.clientY; this.dirty = true;
        $$('[data-wl]').forEach(x => x.setAttribute('aria-pressed', 'false'));
      }
    });
    const up = () => mode = 0;
    v.el.addEventListener('pointerup', up); v.el.addEventListener('pointercancel', up);
    v.el.addEventListener('dblclick', () => { this.cross = [0, 0.16, 0.02]; this.dirty = true; });
  }
  wake() { this.dirty = true; }
  frame(t, dt) {
    if (this.demo) {
      this.t += dt;
      const k = this.t * 0.45;
      this.cross = [0.28 * Math.sin(k * 1.3), 0.12 + 0.42 * Math.sin(k * 0.7), 0.3 * Math.sin(k + 1.2)];
      this.dirty = true;
    }
    if (!this.dirty) return;
    this.dirty = false;
    for (const v of this.views) this.draw(v);
  }
  draw(v) {
    const { gl, canvas, prog: { u } } = v, dpr = Math.min(devicePixelRatio || 1, 2);
    const W = Math.round(canvas.clientWidth * dpr), H = Math.round(canvas.clientHeight * dpr);
    if (canvas.width !== W || canvas.height !== H) { canvas.width = W; canvas.height = H; }
    const c = v.center(this.cross), d = v3.sub(this.cross, c);
    gl.viewport(0, 0, W, H);
    gl.uniform2f(u.uRes, W, H); gl.uniform3fv(u.uC, c); gl.uniform3fv(u.uU, v.U); gl.uniform3fv(u.uV, v.V);
    gl.uniform1f(u.uS, v.s); gl.uniform2f(u.uWL, this.wl[0], this.wl[1]);
    gl.uniform2f(u.uLine, v3.dot(d, v.U) / v.s, v3.dot(d, v.V) / v.s);
    gl.uniform3fv(u.uColX, v.cx); gl.uniform3fv(u.uColY, v.cy);
    gl.uniform1f(u.uLines, 1); gl.uniform1f(u.uMask, 0); gl.uniform1f(u.uDpr, dpr);
    gl.drawArrays(gl.TRIANGLES, 0, 3);
    const mm = x => (x * 150).toLocaleString(T.locale, { minimumFractionDigits: 1, maximumFractionDigits: 1 });
    v.meta.innerHTML = `${v.name}<br>${T.pos} ${mm(v.pos(this.cross))} mm`;
    v.meta2.innerHTML = `W ${Math.round(this.wl[0])} L ${Math.round(this.wl[1])}<br>${T.phantom}`;
  }
}

/* ── slices for the scroll story: rendered once with the MPR shader ── */
function renderSlices(n, size) {
  const c = document.createElement('canvas'); c.width = c.height = size;
  const gl = makeGL(c, { preserveDrawingBuffer: true }); if (!gl) return [];
  const { u } = makeProgram(gl, MPR_FS), out = [];
  gl.viewport(0, 0, size, size);
  const slice = (y, w, l) => {
    gl.uniform2f(u.uRes, size, size); gl.uniform3fv(u.uC, [0, y, 0]); gl.uniform3fv(u.uU, [1, 0, 0]); gl.uniform3fv(u.uV, [0, 0, 1]);
    gl.uniform1f(u.uS, 0.78); gl.uniform2f(u.uWL, w, l); gl.uniform1f(u.uLines, 0); gl.uniform1f(u.uMask, 1); gl.uniform1f(u.uDpr, 1);
    gl.clearColor(0, 0, 0, 0); gl.clear(gl.COLOR_BUFFER_BIT); gl.drawArrays(gl.TRIANGLES, 0, 3);
    const s = document.createElement('canvas'); s.width = s.height = size;
    const x = s.getContext('2d'); x.drawImage(c, 0, 0);
    return [s, x];
  };
  for (let i = 0; i < n; i++) {
    const [s, x] = slice(lerp(0.74, -0.98, i / (n - 1)), 1500, 380);
    x.globalCompositeOperation = 'source-atop';
    x.fillStyle = `hsla(${lerp(188, 268, i / (n - 1))}, 95%, 62%, .38)`; x.fillRect(0, 0, size, size);
    out.push(s);
  }
  // One untinted slice through the ventricles, in the brain window, for the "one slice" moment.
  out.key = slice(0.19, 110, 40)[0];
  gl.getExtension('WEBGL_lose_context')?.loseContext();
  return out;
}

class Story {
  constructor(section) {
    this.section = section; this.stack = $('#stack'); this.steps = $$('.steps li'); this.count = $('#stageCount'); this.word = $('#stageWord');
    this.slices = renderSlices(40, innerWidth < 700 ? 200 : 280);
    this.slices.forEach(s => { s.setAttribute('aria-hidden', 'true'); this.stack.append(s); });
    this.key = this.slices.key;
    if (this.key) { this.key.setAttribute('aria-hidden', 'true'); this.stack.append(this.key); }
    try { this.vr = new VolumeView($('#stageVr'), { preset: 0, quality: 0.6 }); this.vr.yaw = -0.4; } catch { this.vr = null; }
    this.p = -1;
  }
  wake() { this.p = -1; }
  frame(t, dt) {
    const r = this.section.getBoundingClientRect();
    const p = reduced ? 0.5 : clamp(-r.top / Math.max(1, r.height - innerHeight));
    if (this.vr && p > 0.62) this.vr.frame(t, dt);
    if (Math.abs(p - this.p) < 1e-4) return;
    this.p = p;
    const explode = ease(clamp(p / 0.34)), turn = ease(clamp((p - 0.28) / 0.42)), merge = ease(clamp((p - 0.66) / 0.26));
    const n = this.slices.length, gap = lerp(0, 9.5, explode) * (1 - merge * 0.9);
    const rx = lerp(0, 64, explode), rz = lerp(0, -28, explode) - 70 * turn;
    this.stack.style.transform = `rotateX(${rx}deg) rotateZ(${rz}deg) scale(${lerp(1, 0.8, explode) * (1 - merge * 0.25)})`;
    this.slices.forEach((s, i) => {
      const k = i - (n - 1) / 2;
      s.style.transform = `translateZ(${-k * gap}px)`;
      s.style.opacity = clamp(explode * 1.8) * 0.62 * (1 - merge);
    });
    if (this.key) {
      this.key.style.transform = `translateZ(${(n - 1) / 2 * gap * 0.35}px)`;
      this.key.style.opacity = clamp(1 - explode * 2.2);
    }
    $('#stageVr').style.opacity = merge;
    const step = p < 0.3 ? 0 : p < 0.64 ? 1 : 2;
    this.steps.forEach((li, i) => li.classList.toggle('on', i === step));
    const shown = Math.max(1, Math.round(lerp(1, n, explode)));
    this.count.textContent = step === 2 ? '1' : shown;
    this.word.textContent = step === 2 ? T.volume : shown === 1 ? T.slice : T.slices;
  }
}

/* ── privacy animation: data stays inside the computer ── */
class Vault {
  constructor(canvas) {
    this.c = canvas; this.x = canvas.getContext('2d'); this.t = 0;
    this.parts = Array.from({ length: 70 }, () => ({ x: Math.random(), y: Math.random(), a: Math.random() * 6.28, s: 0.08 + Math.random() * 0.12, h: Math.random() < .5 ? 190 : 250, trail: [] }));
    this.flash = 0; this.font = '600 12px ' + getComputedStyle(document.body).getPropertyValue('--mono');
  }
  wake() {}
  frame(t, dt) {
    const c = this.c, x = this.x, dpr = Math.min(devicePixelRatio || 1, 2), W = c.clientWidth, H = c.clientHeight;
    if (c.width !== Math.round(W * dpr)) { c.width = Math.round(W * dpr); c.height = Math.round(H * dpr); }
    x.setTransform(dpr, 0, 0, dpr, 0, 0); x.clearRect(0, 0, W, H);
    this.t += reduced ? 0 : dt;
    // the computer
    const bw = W * 0.56, bh = H * 0.46, bx = W * 0.1, by = H * 0.3;
    x.lineWidth = 1.5; x.strokeStyle = 'rgba(255,255,255,.22)'; x.fillStyle = 'rgba(61,220,132,.04)';
    round(x, bx, by, bw, bh, 14); x.fill(); x.stroke();
    x.beginPath(); x.moveTo(bx - 14, by + bh + 16); x.lineTo(bx + bw + 14, by + bh + 16); x.stroke();
    x.fillStyle = 'rgba(255,255,255,.5)'; x.font = this.font;
    x.fillText(T.computer, bx + 14, by + 24);
    // the cloud, out of reach
    const cx = W * 0.83, cy = H * 0.24, pulse = 0.5 + 0.5 * Math.sin(this.t * 2.2);
    x.save(); x.translate(cx, cy); x.strokeStyle = 'rgba(255,255,255,.25)'; x.lineWidth = 1.5;
    x.beginPath(); x.arc(-16, 6, 16, Math.PI * .5, Math.PI * 1.5); x.arc(4, -6, 20, Math.PI, Math.PI * 1.85); x.arc(22, 8, 14, Math.PI * 1.5, Math.PI * .5); x.closePath(); x.stroke();
    x.strokeStyle = `rgba(255,92,108,${0.5 + 0.5 * this.flash})`; x.lineWidth = 2.5;
    x.beginPath(); x.moveTo(-30, -24); x.lineTo(36, 30); x.stroke(); x.restore();
    x.setLineDash([4, 6]); x.lineDashOffset = -this.t * 18; x.strokeStyle = `rgba(255,92,108,${0.15 + 0.2 * pulse})`; x.lineWidth = 1.2;
    x.beginPath(); x.moveTo(bx + bw, by + bh * 0.25); x.quadraticCurveTo(W * 0.78, by, cx - 20, cy + 30); x.stroke(); x.setLineDash([]);
    this.flash = Math.max(0, this.flash - dt * 2);
    // data particles bouncing inside
    for (const p of this.parts) {
      p.a += (Math.sin(this.t * 0.7 + p.h) * 0.6) * dt;
      p.x += Math.cos(p.a) * p.s * dt; p.y += Math.sin(p.a) * p.s * dt * 1.2;
      if (p.x < 0.03 || p.x > 0.97) { p.a = Math.PI - p.a; p.x = clamp(p.x, 0.03, 0.97); if (p.x > 0.9 && p.y < 0.4) this.flash = 1; }
      if (p.y < 0.12 || p.y > 0.95) { p.a = -p.a; p.y = clamp(p.y, 0.12, 0.95); }
      const px = bx + p.x * bw, py = by + p.y * bh;
      p.trail.push([px, py]); if (p.trail.length > 10) p.trail.shift();
      x.strokeStyle = `hsla(${p.h},95%,65%,.25)`; x.lineWidth = 1.5; x.beginPath();
      p.trail.forEach(([tx, ty], i) => i ? x.lineTo(tx, ty) : x.moveTo(tx, ty)); x.stroke();
      x.fillStyle = `hsla(${p.h},95%,70%,.95)`; x.beginPath(); x.arc(px, py, 1.8, 0, 6.28); x.fill();
    }
    x.fillStyle = 'rgba(255,255,255,.4)'; x.fillText(T.cloud, cx - 22, cy + 52);
  }
}
function round(x, X, Y, w, h, r) { x.beginPath(); x.roundRect ? x.roundRect(X, Y, w, h, r) : x.rect(X, Y, w, h); }

/* ════════ page wiring ════════ */
const hasGL = !!document.createElement('canvas').getContext('webgl2');
if (!hasGL) document.documentElement.classList.add('no-gl');

if (hasGL) {
  try {
    const hero = new VolumeView($('#vr'), { interactive: true, preset: 0, clip: true, quality: 0.8, hud: $('#fps') });
    watch($('#heroVisual'), hero);
    const names = T.modes;
    const mode = () => $('#vrMode').textContent = names[hero.preset] + (hero.clip ? T.cut : '');
    $$('[data-preset]').forEach(b => b.addEventListener('click', () => {
      hero.preset = +b.dataset.preset; hero.dirty = true; mode();
      $$('[data-preset]').forEach(x => x.setAttribute('aria-pressed', x === b));
    }));
    $('#clipBtn').addEventListener('click', e => { hero.clip = !hero.clip; hero.dirty = true; e.currentTarget.setAttribute('aria-pressed', hero.clip); mode(); });
  } catch (e) { console.warn(e); document.documentElement.classList.add('no-gl'); }
  try { watch($('#mprGrid'), new Mpr($('#mprGrid'))); } catch (e) { console.warn(e); document.documentElement.classList.add('no-gl'); }
}
const story = $('#volume');
if (hasGL) { try { watch(story, new Story(story)); } catch (e) { console.warn(e); } }
watch($('.vault'), new Vault($('#vault')));
requestAnimationFrame(tick);

/* reveal on scroll */
const io = new IntersectionObserver(es => es.forEach(e => { if (e.isIntersecting) { e.target.classList.add('in'); io.unobserve(e.target); } }), { rootMargin: '0px 0px -8% 0px' });
$$('.reveal').forEach(el => io.observe(el));

/* counters */
const cio = new IntersectionObserver(es => es.forEach(e => {
  if (!e.isIntersecting) return; cio.unobserve(e.target);
  const el = e.target, to = +el.dataset.count, t0 = performance.now(), dur = reduced ? 1 : 1600;
  const step = now => { const k = clamp((now - t0) / dur); el.textContent = Math.round(to * (1 - Math.pow(1 - k, 4))); if (k < 1) requestAnimationFrame(step); };
  requestAnimationFrame(step);
}), { threshold: .6 });
$$('[data-count]').forEach(el => { el.textContent = '0'; cio.observe(el); });

/* nav: solid after the hero, hides on the way down, highlights the section */
const nav = $('#nav'), bar = $('.progress'); let lastY = scrollY;
addEventListener('scroll', () => {
  const y = scrollY, max = document.documentElement.scrollHeight - innerHeight;
  bar.style.transform = `scaleX(${max > 0 ? y / max : 0})`;
  nav.classList.toggle('solid', y > 40);
  nav.classList.toggle('hide', y > lastY && y > 600);
  lastY = y;
}, { passive: true });
const links = new Map($$('.nav-links a').map(a => [a.getAttribute('href').slice(1), a]));
const sio = new IntersectionObserver(es => es.forEach(e => { if (e.isIntersecting) links.forEach((a, id) => a.classList.toggle('on', id === e.target.id)); }), { rootMargin: '-45% 0px -50% 0px' });
links.forEach((_, id) => { const s = document.getElementById(id); if (s) sio.observe(s); });

/* cards: spotlight and tilt that follow the pointer */
if (matchMedia('(hover: hover)').matches && !reduced) {
  $$('.tilt').forEach(card => {
    card.addEventListener('pointermove', e => {
      const r = card.getBoundingClientRect(), px = (e.clientX - r.left) / r.width, py = (e.clientY - r.top) / r.height;
      card.style.setProperty('--mx', px * 100 + '%'); card.style.setProperty('--my', py * 100 + '%');
      card.style.setProperty('--rx', (0.5 - py) * 5 + 'deg'); card.style.setProperty('--ry', (px - 0.5) * 6 + 'deg');
    });
    card.addEventListener('pointerleave', () => { card.style.setProperty('--rx', '0deg'); card.style.setProperty('--ry', '0deg'); });
  });
  $$('.magnetic').forEach(b => {
    b.addEventListener('pointermove', e => { const r = b.getBoundingClientRect(); b.style.setProperty('--bx', (e.clientX - r.left - r.width / 2) * 0.18 + 'px'); b.style.setProperty('--by', (e.clientY - r.top - r.height / 2) * 0.28 + 'px'); });
    b.addEventListener('pointerleave', () => { b.style.setProperty('--bx', '0px'); b.style.setProperty('--by', '0px'); });
  });
}

/* lightbox */
const lb = $('#lb');
$$('.shot').forEach(f => f.addEventListener('click', () => {
  $('img', lb).src = f.dataset.full; $('img', lb).alt = f.dataset.cap; $('figcaption', lb).textContent = f.dataset.cap; lb.showModal();
}));
lb.addEventListener('click', e => { if (e.target === lb || e.target.classList.contains('lb-close') || e.target.tagName === 'IMG') lb.close(); });

/* code that types itself (a real excerpt of src/jobs.rs) */
const SRC = `fn spawn(&self, name: &str, f: impl FnOnce(Sender<Msg>) + Send + 'static) {
    let tx = self.tx.clone();
    let ctx = self.ctx.clone();
    // The browser has no threads here: the job runs once the current frame is on screen.
    #[cfg(target_arch = "wasm32")]
    {
        let _ = name;
        crate::web::defer(move || {
            f(tx);
            ctx.request_repaint();
        });
    }
    #[cfg(not(target_arch = "wasm32"))]
    let name = name.to_string();
    #[cfg(not(target_arch = "wasm32"))]
    let _ = std::thread::Builder::new().name(name.clone()).spawn(move || {`;
const esc = s => s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
function highlight(src) {
  const re = /(\/\/[^\n]*)|("(?:[^"\\]|\\.)*")|(#\[[^\]\n]*\]?)|(\b(?:fn|let|impl|move|self|crate|mut|not|pub|use)\b)|(\b[a-z_]+!)|(\b(?:FnOnce|Sender|Msg|Send|str)\b)|('static)/g;
  let out = '', i = 0, m;
  while ((m = re.exec(src))) {
    out += esc(src.slice(i, m.index));
    const cls = m[1] ? 'c' : m[2] ? 's' : m[3] ? 'a' : m[4] ? 'k' : m[5] ? 'm' : m[6] ? 't' : 'k';
    out += `<span class="${cls}">${esc(m[0])}</span>`; i = re.lastIndex;
  }
  return out + esc(src.slice(i));
}
const pre = $('#code');
const typeIO = new IntersectionObserver(([e]) => {
  if (!e.isIntersecting) return; typeIO.disconnect();
  if (reduced) { pre.innerHTML = highlight(SRC); return; }
  let n = 0;
  const step = () => { n = Math.min(SRC.length, n + 3); pre.innerHTML = highlight(SRC.slice(0, n)) + '<span class="caret"></span>'; if (n < SRC.length) setTimeout(step, 16); };
  step();
}, { threshold: .4 });
typeIO.observe(pre);

if (reduced) $$('svg').forEach(s => s.pauseAnimations?.());
