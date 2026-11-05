//! Slice-stack renderer.
//!
//! Ports `draw.ts`. Constants are the TypeScript values. `eye_y` is the
//! `EYE_Y` table. Caches match the TS maps (palette, whirl ink, eye and
//! mouth paths), including the clear-when-over-limit rule.
//!
//! - 15–84 `DrawConfig`
//! - 88–91 `OVERSCAN`, `RISE`
//! - 94–103 `SLICES`, `HALF_DEPTH`, `CAP`, `profile`, `EYE_GAP`, `EYE_RX`, `EYE_Y`
//! - 105–158 `Palette`, `palette`, `hslNums`, `mixCss`
//! - 170–251 whirl: `WHIRL_*`, `WhirlInk`, `whirlInk`, `withAlpha`, `drawWhirl`
//! - 259–263 `backOf`
//! - 268–271 `LIGHT_DEFAULTS`
//! - 273–510 `draw`
//! - 513–529 `FACE_R`, `onSphere`
//! - 535–553 `EYE_STEPS`, `eyePath`
//! - 560–586 `Mouth`, `MOUTH_SIN`, `MOUTH_COS`, `mouthPath`
//! - 588–677 `drawFace`
#![allow(dead_code, unused_variables, unused_imports)]

use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex};

use tiny_skia::{BlendMode, FillRule, LineCap, LineJoin, Path, PathBuilder, PathSegment};

use crate::canvas::{Canvas2D, PaintStyle, RgbaColor, ellipse_path};
use crate::color::shade;
use crate::engine::Pose;
use crate::plastic::{
    FurStyle, Material, Rig, draw_plastic_cap, mul_affine, surface_at_tier, tier_for,
};
use crate::types::{BotAvatarFace, BotAvatarShading, BotAvatarTheme};
use crate::wear::{
    EyeSpot, FaceAnchor, GlassesPass, HeadTurn, ScreenLight, Wear, WearRig, draw_glasses,
    draw_wear_behind, draw_wear_front,
};

/// The canvas is larger than the layout box so a hop or a flip is not clipped.
pub const OVERSCAN: f64 = 1.5;
/// The body centre sits this fraction of the box below the canvas centre.
pub const RISE: f64 = 0.1;

pub(crate) const SLICES: usize = 17;
pub(crate) const HALF_DEPTH: f64 = 15.0;
/// Cap scale at the ends of the pillow, at the stock rim width.
pub(crate) const CAP: f64 = 0.9;

pub(crate) const EYE_GAP: f64 = 25.0;
pub(crate) const EYE_RX: f64 = 6.3;
pub(crate) const EYE_Y_EYES: f64 = 1.0;
pub(crate) const EYE_Y_MOUTH: f64 = -3.5;

pub(crate) const WHIRL_SEGMENTS: usize = 34;
pub(crate) const WHIRL_SPAN: f64 = std::f64::consts::PI * 1.55;
pub(crate) const WHIRL_RX: f64 = 57.0;
pub(crate) const WHIRL_RATIO: f64 = 0.4;
pub(crate) const WHIRL_TILT: f64 = -0.28;

pub(crate) const FACE_R: f64 = 30.0;
pub(crate) const EYE_STEPS: usize = 8;
/// `sin(0.684)` and `cos(0.684)`, the mouth corner slope (`draw.ts` 561).
pub(crate) const MOUTH_SIN: f64 = 0.6318982762618849;
pub(crate) const MOUTH_COS: f64 = 0.7750513327885183;

/// One outline, or one solid per piece.
///
/// A single path caches as `{type_key}:parts`. A list caches each piece as
/// `{type_key}:parts{i}/{len}` so two antennae do not inflate toward the gap.
#[derive(Clone)]
pub enum OutlineParts {
    Single(Arc<Path>),
    Many(Arc<Vec<Arc<Path>>>),
}

/// The whirl's knobs. 1 everywhere is the stock look; strength 0 draws nothing.
#[derive(Debug, Clone, Copy)]
pub struct WhirlKnobs {
    pub strength: f64,
    pub size: f64,
    pub width: f64,
    pub length: f64,
    pub tilt: f64,
}

/// Plastic side slices. `Auto` blits sprites on WebKit; this port has no WebKit,
/// and [`crate::plastic_cap::WEBKIT`] is false.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SideMode {
    #[default]
    Auto,
    Vector,
    Sprite,
}

/// One frame's inputs. Omitted `Option` fields mean the stock look inside [`draw`].
pub struct DrawConfig {
    pub path: Arc<Path>,
    pub face: BotAvatarFace,
    pub face_x: f64,
    pub face_y: f64,
    pub face_scale: f64,
    pub color: String,
    pub ink: String,
    pub shading: BotAvatarShading,
    pub shadow: Option<f64>,
    pub highlight: Option<f64>,
    pub depth: Option<f64>,
    /// Degrees clockwise from the top.
    pub light: Option<f64>,
    pub rim: Option<f64>,
    pub spread: Option<f64>,
    /// 1 a cushion, toward 0 a slab. Default 1.
    pub roundness: Option<f64>,
    pub fur: Option<FurStyle>,
    pub vivid: Option<f64>,
    /// Degrees clockwise from the top. Default: above the toy, away from the key.
    pub back_light: Option<f64>,
    /// Degrees the key sits toward the camera. Default 32.
    pub light_front: Option<f64>,
    /// Fibre highlights. Default 0.
    pub shine: Option<f64>,
    /// 0 a small back source, 1 an even glow. Default 1.
    pub back_softness: Option<f64>,
    /// Satin sheen over the edge. Default 0.
    pub sheen: Option<f64>,
    pub type_key: Option<String>,
    /// Build materials in this call. There is no idle queue in the sync bake.
    pub still: bool,
    /// With `still`, still allow the pile bake to miss and land later.
    pub fur_on_idle: bool,
    pub parts: Option<OutlineParts>,
    pub parts_depth: Option<f64>,
    pub parts_colors: Option<Vec<Option<String>>>,
    pub theme: Option<BotAvatarTheme>,
    /// Device pixel ratio the context is already scaled by.
    pub dpr: Option<f64>,
    pub sides: SideMode,
    pub whirl: Option<WhirlKnobs>,
    pub wear: Option<Wear>,
}

/// Numbers shared by the fabric and the other materials (`LIGHT_DEFAULTS`, 268–271).
#[derive(Debug, Clone, Copy)]
pub struct MaterialLight {
    pub shadow: f64,
    pub highlight: f64,
    /// Degrees clockwise from the top.
    pub light: f64,
    pub rim: f64,
    pub spread: f64,
}

#[derive(Debug, Clone, Copy)]
pub struct LightDefaults {
    pub fabric: MaterialLight,
    pub other: MaterialLight,
}

pub const LIGHT_DEFAULTS: LightDefaults = LightDefaults {
    fabric: MaterialLight {
        shadow: 1.15,
        highlight: 1.45,
        light: 295.0,
        rim: 0.6,
        spread: 1.6,
    },
    other: MaterialLight {
        shadow: 0.35,
        highlight: 1.3,
        light: 300.0,
        rim: 0.5,
        spread: 1.55,
    },
};

/// `EYE_Y[face]`.
pub(crate) fn eye_y(face: BotAvatarFace) -> f64 {
    match face {
        BotAvatarFace::Eyes => EYE_Y_EYES,
        BotAvatarFace::Mouth => EYE_Y_MOUTH,
    }
}

/// Pillow profile (`profile`, 98). `cap` is the scale at the ends.
pub(crate) fn profile(z: f64, cap: f64) -> f64 {
    cap + (1.0 - cap) * js_max(0.0, 1.0 - z * z).sqrt()
}

/// Lit and shaded stops for one body colour (`Palette`, 105–118).
///
/// `palette` caches by `color|shadow|highlight` and drops the cache past 200
/// entries. `grad` is filled lazily for the crisp light direction.
#[derive(Clone)]
pub(crate) struct Palette {
    pub base: String,
    pub far: String,
    pub near: String,
    pub light: String,
    pub dark: String,
    pub cap_top: String,
    pub cap_bottom: String,
    pub crisp_mix: Vec<String>,
    pub smooth_mix: Vec<String>,
    pub grad: Option<SliceGrad>,
}

#[derive(Clone)]
pub(crate) struct SliceGrad {
    pub lx: f64,
    pub ly: f64,
    pub lit: PaintStyle,
    pub cap: PaintStyle,
}

static PALETTE_CACHE: LazyLock<Mutex<HashMap<String, Palette>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn palette_key(color: &str, shadow: f64, highlight: f64) -> String {
    format!("{color}|{shadow}|{highlight}")
}

pub(crate) fn palette(color: &str, shadow: f64, highlight: f64) -> Palette {
    let key = palette_key(color, shadow, highlight);
    let mut cache = PALETTE_CACHE.lock().expect("palette cache");
    if let Some(found) = cache.get(&key) {
        return found.clone();
    }
    let far = shade(color, -0.3 * shadow, 0.05 * shadow);
    let near = shade(color, -0.12 * shadow, 0.03 * shadow);
    let mut crisp_mix = Vec::with_capacity(SLICES);
    let mut smooth_mix = Vec::with_capacity(SLICES);
    for j in 0..SLICES {
        let t = j as f64 / (SLICES - 1) as f64;
        if t > 0.6 {
            crisp_mix.push(String::new());
        } else {
            crisp_mix.push(mix_css(&far, &near, t / 0.6));
        }
        if t >= 0.5 {
            smooth_mix.push(color.to_string());
        } else {
            smooth_mix.push(mix_css(&far, color, t / 0.5));
        }
    }
    let built = Palette {
        base: color.to_string(),
        far: far.clone(),
        near,
        light: shade(color, 0.04 * highlight, 0.0),
        dark: shade(color, -0.3 * shadow, 0.05 * shadow),
        cap_top: shade(color, 0.035 * highlight, 0.0),
        cap_bottom: shade(color, -0.035 * shadow, 0.0),
        crisp_mix,
        smooth_mix,
        grad: None,
    };
    if cache.len() > 200 {
        cache.clear();
    }
    cache.insert(key, built.clone());
    built
}

fn store_palette_grad(color: &str, shadow: f64, highlight: f64, grad: SliceGrad) {
    let key = palette_key(color, shadow, highlight);
    if let Some(slot) = PALETTE_CACHE.lock().expect("palette cache").get_mut(&key) {
        slot.grad = Some(grad);
    }
}

/// Numbers of an `hsl()` string. Any other colour goes through `shade` first (`hslNums`, 151).
pub(crate) fn hsl_nums(c: &str) -> Vec<f64> {
    if c.starts_with("hsl(") {
        extract_nums(c)
    } else {
        extract_nums(&shade(c, 0.0, 0.0))
    }
}

fn extract_nums(src: &str) -> Vec<f64> {
    let bytes = src.as_bytes();
    let mut out = Vec::with_capacity(3);
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i].is_ascii_digit() || bytes[i] == b'.' {
            let start = i;
            i += 1;
            while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'.') {
                i += 1;
            }
            if let Ok(v) = src[start..i].parse::<f64>() {
                out.push(v);
            }
        } else {
            i += 1;
        }
    }
    out
}

pub(crate) fn mix_css(a: &str, b: &str, t: f64) -> String {
    let pa = hsl_nums(a);
    let pb = hsl_nums(b);
    let n = pa.len().min(pb.len()).min(3);
    let mut m = [0.0; 3];
    for i in 0..n {
        m[i] = pa[i] + (pb[i] - pa[i]) * t;
    }
    format!(
        "hsl({} {}% {}%)",
        js_to_fixed(m[0], 1),
        js_to_fixed(m[1], 1),
        js_to_fixed(m[2], 1)
    )
}

#[derive(Clone)]
pub(crate) struct WhirlInk {
    pub base: String,
    pub light: String,
    pub dark: String,
    pub halo: String,
}

static WHIRL_INK_CACHE: LazyLock<Mutex<HashMap<String, WhirlInk>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

pub(crate) fn whirl_ink(color: &str) -> WhirlInk {
    let mut cache = WHIRL_INK_CACHE.lock().expect("whirl ink cache");
    if let Some(found) = cache.get(color) {
        return found.clone();
    }
    let built = WhirlInk {
        base: shade(color, 0.1, 0.02),
        light: shade(color, 0.3, 0.04),
        dark: shade(color, -0.22, 0.08),
        halo: shade(color, 0.2, 0.0),
    };
    if cache.len() > 200 {
        cache.clear();
    }
    cache.insert(color.to_string(), built.clone());
    built
}

/// An `hsl()` from `shade` with an alpha (`withAlpha`, 192).
pub(crate) fn with_alpha(hsl: &str, a: f64) -> String {
    let aa = js_max(0.0, js_min(1.0, a));
    let repl = format!(" / {})", js_to_fixed(aa, 3));
    match hsl.find(')') {
        Some(i) => {
            let mut s = String::with_capacity(hsl.len() + repl.len());
            s.push_str(&hsl[..i]);
            s.push_str(&repl);
            s.push_str(&hsl[i + 1..]);
            s
        }
        None => hsl.to_string(),
    }
}

pub(crate) fn draw_whirl(
    ctx: &mut Canvas2D,
    pose: &Pose,
    color: &str,
    lx: f64,
    ly: f64,
    near: bool,
    knobs: Option<&WhirlKnobs>,
) {
    let strength = knobs.map(|k| k.strength).unwrap_or(0.0);
    let k = js_min(1.0, pose.whirl * strength);
    if k <= 0.01 {
        return;
    }
    let (size_k, width_k, length_k, tilt_k) = match knobs {
        Some(n) => (n.size, n.width, n.length, n.tilt),
        None => (1.0, 1.0, 1.0, 1.0),
    };
    let span = WHIRL_SPAN * length_k;
    let ink = whirl_ink(color);
    let head = -pose.whirl_angle;
    let rx = WHIRL_RX * size_k;
    let ry = rx * WHIRL_RATIO * tilt_k * if near { 1.14 } else { 0.86 };
    let light_a = ly.atan2(lx) - WHIRL_TILT;
    ctx.save();
    ctx.rotate(WHIRL_TILT);
    ctx.translate(0.0, 5.0);
    ctx.set_line_cap(LineCap::Butt);
    let mut seg = |a0: f64, a1: f64, width: f64, style: &str, dy: f64| {
        // Canvas throws on a non-positive ellipse radius. A non-positive
        // line width is ignored by the setter, and the previous width is stroked.
        if !(rx > 0.0) || !(ry > 0.0) {
            return;
        }
        ctx.set_stroke_css(style);
        ctx.set_line_width(width);
        if let Some(path) = ellipse_path(0.0, dy, rx, ry, 0.0, a0, a1, false, false) {
            ctx.stroke_path(&path);
        }
    };
    if near {
        for i in 0..WHIRL_SEGMENTS {
            let f = i as f64 / WHIRL_SEGMENTS as f64;
            let a1 = head + f * span;
            let a0 = a1 + span / WHIRL_SEGMENTS as f64 + 0.012;
            if ((a0 + a1) / 2.0).sin() <= 0.0 {
                continue;
            }
            let fade = (1.0 - f).powf(1.3);
            let alpha = js_to_fixed(0.2 * k * fade, 3);
            let style = format!("rgba(0,0,0,{alpha})");
            seg(a1, a0, (2.0 + 8.0 * fade) * 1.5 * width_k, &style, 3.5);
        }
    }
    for i in 0..WHIRL_SEGMENTS {
        let f = i as f64 / WHIRL_SEGMENTS as f64;
        let a1 = head + f * span;
        let a0 = a1 + span / WHIRL_SEGMENTS as f64 + 0.012;
        let mid = (a0 + a1) / 2.0;
        if (mid.sin() > 0.0) != near {
            continue;
        }
        let depth = 0.6 + 0.4 * mid.sin();
        let fade = (1.0 - f).powf(1.3);
        let puff = 1.0 + 0.18 * (f * 9.0 + 1.2).sin();
        let width = (2.0 + 8.0 * fade) * depth * width_k * puff;
        let a = k * (0.3 + 0.7 * fade) * depth;
        let facing = 0.5 + 0.5 * (mid - light_a).cos();
        seg(a1, a0, width * 2.6, &with_alpha(&ink.halo, a * 0.2), 0.0);
        seg(
            a1,
            a0,
            width * 0.8,
            &with_alpha(&ink.dark, a * 0.45),
            width * 0.32,
        );
        seg(a1, a0, width, &with_alpha(&ink.base, a * 0.72), 0.0);
        seg(
            a1,
            a0,
            width * 0.62,
            &with_alpha(&ink.light, a * 0.78 * (0.4 + 0.6 * facing)),
            -width * 0.16,
        );
        let spec = js_to_fixed(a * 0.9 * (0.15 + 0.85 * facing * facing), 3);
        let spec_style = format!("rgba(255,255,255,{spec})");
        seg(a1, a0, width * 0.24, &spec_style, -width * 0.3);
    }
    ctx.restore();
}

/// Back-light direction on screen (`backOf`, 259–263). Missing or non-finite is empty.
pub(crate) struct BackDir {
    pub bx: Option<f64>,
    pub by: Option<f64>,
}

pub(crate) fn back_of(deg: Option<f64>) -> BackDir {
    match deg {
        Some(deg) if deg.is_finite() => {
            let a = deg * std::f64::consts::PI / 180.0;
            BackDir {
                bx: Some(a.sin()),
                by: Some(-a.cos()),
            }
        }
        _ => BackDir { bx: None, by: None },
    }
}

/// A feature's place on the face sphere (`onSphere`, 518–529).
pub(crate) struct SpherePlace {
    pub x: f64,
    pub y: f64,
    pub sx: f64,
    pub sy: f64,
    pub z: f64,
}

pub(crate) fn on_sphere(x: f64, y: f64, yaw: f64, pitch: f64) -> SpherePlace {
    let lon = (x / FACE_R).clamp(-1.0, 1.0).asin() + yaw;
    let lat = (-y / FACE_R).clamp(-1.0, 1.0).asin() + pitch;
    let cl = lat.cos();
    SpherePlace {
        x: FACE_R * lon.sin() * cl,
        y: -FACE_R * lat.sin(),
        sx: lon.cos(),
        sy: cl,
        z: lon.cos() * cl,
    }
}

static EYE_PATHS: LazyLock<Mutex<HashMap<u64, Arc<Path>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

pub(crate) fn eye_path(x0: f64, y0: f64, cy: f64) -> Path {
    let qx = js_round(x0 * 50.0);
    let qy = js_round(y0 * 50.0);
    let qc = js_round(cy * 50.0);
    let key = (qx + 2000.0 * qy + 4.0e6 * qc).to_bits();
    let mut cache = EYE_PATHS.lock().expect("eye path cache");
    if let Some(found) = cache.get(&key) {
        return clone_path(found);
    }
    let ax = qx / 50.0;
    let ay = qy / 50.0;
    let ac = qc / 50.0;
    let built = Arc::new(build_eye_path(ax, ay, ac));
    if cache.len() > 256 {
        cache.clear();
    }
    cache.insert(key, Arc::clone(&built));
    clone_path(&built)
}

fn build_eye_path(ax: f64, ay: f64, ac: f64) -> Path {
    let mut pb = PathBuilder::new();
    pb.move_to((-ax) as f32, ay as f32);
    for i in 1..=EYE_STEPS {
        let t = i as f64 / EYE_STEPS as f64;
        let mt = 1.0 - t;
        let x = mt * mt * -ax + t * t * ax;
        let y = (mt * mt + t * t) * ay + 2.0 * mt * t * ac;
        pb.line_to(fixed_f32(x, 3), fixed_f32(y, 3));
    }
    finish_path(pb)
}

static MOUTH_PATHS: LazyLock<Mutex<HashMap<(i64, i64, i64, i64, i64, i64), Arc<Path>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Mouth outline (`Mouth`, 560).
pub(crate) struct Mouth {
    pub hw: f64,
    pub t0: f64,
    pub a: f64,
    pub yt: f64,
    pub ab: f64,
    pub yb: f64,
}

pub(crate) fn mouth_path(m: &Mouth) -> Path {
    let key = (
        q50_i(m.hw),
        q50_i(m.t0),
        q50_i(m.a),
        q50_i(m.yt),
        q50_i(m.ab),
        q50_i(m.yb),
    );
    let mut cache = MOUTH_PATHS.lock().expect("mouth path cache");
    if let Some(found) = cache.get(&key) {
        return clone_path(found);
    }
    let hw = key.0 as f64 / 50.0;
    let t0 = key.1 as f64 / 50.0;
    let a = key.2 as f64 / 50.0;
    let yt = key.3 as f64 / 50.0;
    let ab = key.4 as f64 / 50.0;
    let yb = key.5 as f64 / 50.0;
    let built = Arc::new(build_mouth_path(hw, t0, a, yt, ab, yb));
    if cache.len() > 256 {
        cache.clear();
    }
    cache.insert(key, Arc::clone(&built));
    clone_path(&built)
}

fn build_mouth_path(hw: f64, t0: f64, a: f64, yt: f64, ab: f64, yb: f64) -> Path {
    let nx = t0 * MOUTH_SIN;
    let ny = t0 * MOUTH_COS;
    let ltx = -hw + nx;
    let lty = -ny;
    let rtx = hw - nx;
    let rty = -ny;
    let lbx = -hw - nx;
    let lby = ny;
    let rbx = hw + nx;
    let rby = ny;
    let cx = (4.0 / 3.0) * t0 * MOUTH_COS;
    let cy = (4.0 / 3.0) * t0 * MOUTH_SIN;
    let p = |v: f64| fixed_f32(v, 3);
    let mut pb = PathBuilder::new();
    pb.move_to(p(ltx), p(lty));
    pb.cubic_to(
        p(-hw + a * hw),
        p(yt - t0),
        p(hw - a * hw),
        p(yt - t0),
        p(rtx),
        p(rty),
    );
    pb.cubic_to(
        p(rtx + cx),
        p(rty - cy),
        p(rbx + cx),
        p(rby - cy),
        p(rbx),
        p(rby),
    );
    pb.cubic_to(
        p(hw - ab * hw),
        p(yb + t0),
        p(-hw + ab * hw),
        p(yb + t0),
        p(lbx),
        p(lby),
    );
    pb.cubic_to(
        p(lbx - cx),
        p(lby - cy),
        p(ltx - cx),
        p(lty - cy),
        p(ltx),
        p(lty),
    );
    pb.close();
    finish_path(pb)
}

pub(crate) fn draw_face(ctx: &mut Canvas2D, pose: &Pose, cfg: &DrawConfig) -> [EyeSpot; 2] {
    let wd = pose.w[0];
    let ww = pose.w[1];
    let ws = pose.w[2];
    let ink = cfg.ink.as_str();
    let ey = eye_y(cfg.face);
    let half = EYE_GAP / 2.0;
    let look_x = pose.look_x;
    let look_y = pose.look_y;
    let yaw = pose.yaw;
    let pitch = pose.pitch;

    let past = |v: f64, d: f64| -> f64 {
        if v.abs() <= d {
            0.0
        } else {
            v.signum() * (v.abs() - d) / (1.0 - d)
        }
    };
    let clamp1 = |v: f64| js_max(-1.0, js_min(1.0, v));
    let up = past(clamp1(-pose.pitch / 0.26 - pose.look_y / 7.0), 0.34);
    let side_amt = past(clamp1(pose.look_x / 4.5), 0.4).abs();
    let tall = js_max(0.3, 1.0 + 0.55 * up - 0.1 * side_amt);
    let wide = 1.0 - 0.05 * up + 0.12 * side_amt;
    let open = wd + ww * (1.0 - pose.laugh);
    let laugh = ww * pose.laugh;
    let lift = js_max(0.0, -pose.y) / 26.0;
    let sag = 0.5 + 0.5 * pose.breath;

    for side in [-1.0_f64, 1.0] {
        let lid = if side < 0.0 {
            pose.blink_l
        } else {
            pose.blink_r
        };
        let e = js_max(0.0, js_min(1.0, pose.eye_open * (1.0 - lid)));
        let k_open = open * e;
        let k_shut = open * (1.0 - e);
        let k_laugh = laugh;
        let k_sleep = ws;
        let x0 = k_open * 0.01 + k_shut * 5.4 + k_laugh * 6.2 + k_sleep * 6.0;
        let y0 = k_open * 1.1 * tall
            + k_shut * 0.6
            + k_laugh * (2.2 - lift * 1.5)
            + k_sleep * (-1.4 + sag);
        let cy = k_open * -3.3 * tall
            + k_shut * 0.6
            + k_laugh * (-11.4 - 4.0 * lift)
            + k_sleep * (5.4 + 2.0 * sag);
        let w = k_open * EYE_RX * 2.0 * wide + k_shut * 2.8 + k_laugh * 4.4 + k_sleep * 4.0;
        let dx = look_x * (k_open + 0.5 * (k_shut + k_laugh));
        let dy = look_y * (k_open + 0.5 * k_shut);
        if pose.eyes_mix > 0.0 {
            let m = pose.eyes_mix.min(1.0);
            let classic = crate::expression::EyeShape {
                x0,
                y0,
                cy,
                w,
                dx,
                dy,
                tilt: 0.0,
                cross: 0.0,
            };
            let mut e = pose.eyes[usize::from(side > 0.0)].with_lid(lid);
            e.dx += look_x;
            e.dy += look_y;
            let shape = classic.lerp(e, m);
            draw_expression_eye(ctx, ink, side * half, ey, yaw, pitch, &shape);
            continue;
        }
        place_feature(ctx, side * half + dx, ey + dy, yaw, pitch, 1.0, |ctx| {
            ctx.set_stroke_css(ink);
            ctx.set_line_width(w);
            let path = eye_path(x0, y0, cy);
            ctx.stroke_path(&path);
        });
    }

    if cfg.face == BotAvatarFace::Mouth {
        let mx = look_x * 0.35;
        let kd = (0.6 + 0.4 * wd) * (1.0 + 0.06 * pose.breath);
        let kw = (0.6 + 0.4 * ww) * (1.0 + (0.25 * js_max(0.0, -pose.y)) / 26.0);
        let r = 2.7 * ws * (1.0 + 0.25 * pose.breath);
        let b = |d: f64, w: f64, s: f64| wd * d + ww * w + ws * s;
        let mouth = Mouth {
            hw: b(6.5 * kd, 9.5 * kw, r),
            t0: b(1.9, 0.0, 0.0),
            a: b(2.0 / 3.0, 2.0 / 3.0, 0.0),
            yt: b(3.53 * kd, 1.6 * kw, (-4.0 * r) / 3.0),
            ab: b(2.0 / 3.0, 0.0, 0.0),
            yb: b(3.53 * kd, 17.3 * kw, (4.0 * r) / 3.0),
        };
        place_feature(ctx, mx, b(12.5, 11.6, 15.5), yaw, pitch, 1.0, |ctx| {
            ctx.set_fill_css(ink);
            let path = mouth_path(&mouth);
            ctx.fill_path(&path);
        });
    }

    let spot = |x: f64| -> EyeSpot {
        let q = on_sphere(x, ey, yaw, pitch);
        EyeSpot {
            x: q.x,
            y: q.y,
            sx: q.sx,
            sy: q.sy,
            z: q.z,
            ox: x,
            oy: ey,
            fz: js_max(0.0, FACE_R * FACE_R - x * x - ey * ey).sqrt(),
        }
    };
    [spot(-half), spot(half)]
}

/// One eye from the motion layer: the classic stroke, turned by `tilt`, split into an X by
/// `cross`.
fn draw_expression_eye(
    ctx: &mut Canvas2D,
    ink: &str,
    x: f64,
    y: f64,
    yaw: f64,
    pitch: f64,
    e: &crate::expression::EyeShape,
) {
    place_feature(ctx, x + e.dx, y + e.dy, yaw, pitch, 1.0, |ctx| {
        ctx.set_stroke_css(ink);
        ctx.set_line_width(js_max(0.4, e.w));
        let path = eye_path(e.x0, e.y0, e.cy);
        let spread = e.cross.clamp(0.0, 1.0) * std::f64::consts::FRAC_PI_4;
        if e.tilt == 0.0 && spread == 0.0 {
            ctx.stroke_path(&path);
            return;
        }
        ctx.rotate(e.tilt + spread);
        ctx.stroke_path(&path);
        if spread > 1e-3 {
            ctx.rotate(-2.0 * spread);
            ctx.stroke_path(&path);
        }
    });
}

/// Draw one frame (`draw`, 273–510).
///
/// `box_px` is the layout size in CSS px (`box` is a keyword). The canvas is
/// `box_px * OVERSCAN` square, the body centre is `RISE * box_px` below the
/// middle, and the context is already scaled by the device pixel ratio.
///
/// Order: clear; body transform via [`mul_affine`]; far [`draw_whirl`];
/// [`draw_wear_behind`]; parts then the body (`draw_plastic_cap` for plastic
/// and fabric, otherwise the slice stack, falling back through [`surface_at`]);
/// face clip, [`draw_face`], glasses shadow then frame; [`draw_wear_front`];
/// near whirl.
pub fn draw(ctx: &mut Canvas2D, box_px: f64, pose: &Pose, cfg: &DrawConfig) {
    let full = box_px * OVERSCAN;
    ctx.clear_rect(0.0, 0.0, full, full);
    let s = box_px / 100.0;
    let (dpr, base) = if let Some(dpr) = cfg.dpr {
        (dpr, [dpr, 0.0, 0.0, dpr, 0.0, 0.0])
    } else {
        let t = ctx.get_transform();
        let dpr = if t[0] == 0.0 || t[0].is_nan() {
            1.0
        } else {
            t[0]
        };
        (dpr, t)
    };
    let lit = if cfg.shading == BotAvatarShading::Fabric {
        LIGHT_DEFAULTS.fabric
    } else {
        LIGHT_DEFAULTS.other
    };
    let shadow = cfg.shadow.unwrap_or(lit.shadow);
    let highlight = cfg.highlight.unwrap_or(lit.highlight);
    let half_depth = HALF_DEPTH * cfg.depth.unwrap_or(0.65);
    let rim_k = cfg.rim.unwrap_or(lit.rim);
    let cap = 1.0 - (1.0 - CAP) * rim_k;
    let spread = cfg.spread.unwrap_or(lit.spread);
    let la = cfg.light.unwrap_or(lit.light) * std::f64::consts::PI / 180.0;
    let lx = la.sin();
    let ly = -la.cos();
    let mut body_pal = palette(&cfg.color, shadow, highlight);

    let cy0 = pose.yaw.cos();
    let sy = pose.yaw.sin();
    let cp0 = pose.pitch.cos();
    let sp = pose.pitch.sin();
    let facing = cy0 * cp0;
    let foreshorten = |v: f64| -> f64 {
        if v.abs() < 0.22 {
            if v < 0.0 { -0.22 } else { 0.22 }
        } else {
            v
        }
    };
    let cy = foreshorten(cy0);
    let cp = foreshorten(cp0);

    let cr = pose.roll.cos();
    let sr = pose.roll.sin();
    let kx = pose.sx * s;
    let ky = pose.sy * s;
    let lift = 50.0 * (1.0 - pose.sy) * s;
    let body = mul_affine(
        base,
        [
            cr * kx,
            sr * kx,
            -sr * ky,
            cr * ky,
            full / 2.0 + pose.x * s - sr * lift,
            full / 2.0 + RISE * box_px + pose.y * s + cr * lift,
        ],
    );
    ctx.save();
    ctx.set_transform(body[0], body[1], body[2], body[3], body[4], body[5]);
    ctx.set_line_cap(LineCap::Round);
    ctx.set_line_join(LineJoin::Round);

    let mode = cfg.shading;
    draw_whirl(ctx, pose, &cfg.color, lx, ly, false, cfg.whirl.as_ref());

    let wear = cfg.wear.as_ref();
    let worn = wear.is_some_and(is_worn);
    let wr = WearRig {
        cy,
        sy,
        cp,
        sp,
        facing,
        half_depth,
        lx,
        ly,
    };
    if worn {
        draw_wear_behind(ctx, cfg.path.as_ref(), &wr, wear.expect("worn"));
    }

    let type_key_fallback;
    let type_key: &str = if let Some(key) = cfg.type_key.as_deref() {
        key
    } else {
        type_key_fallback = "custom".to_string();
        &type_key_fallback
    };
    let mut part_pals: HashMap<String, Palette> = HashMap::new();
    let part_depth = half_depth * cfg.parts_depth.unwrap_or(0.4);
    match &cfg.parts {
        None => {}
        Some(OutlineParts::Single(path)) => {
            let key = format!("{type_key}:parts");
            let mut pal = part_palette(cfg, 0, shadow, highlight, &body_pal, &mut part_pals);
            draw_solid(
                ctx, path, &key, part_depth, false, &mut pal, cfg, mode, body, cy, sy, cp, sp,
                facing, cap, lx, ly, shadow, highlight, spread, rim_k, box_px, dpr, pose.roll,
            );
        }
        Some(OutlineParts::Many(paths)) => {
            let n = paths.len();
            for (i, path) in paths.iter().enumerate() {
                let key = format!("{type_key}:parts{i}/{n}");
                let mut pal = part_palette(cfg, i, shadow, highlight, &body_pal, &mut part_pals);
                draw_solid(
                    ctx, path, &key, part_depth, false, &mut pal, cfg, mode, body, cy, sy, cp, sp,
                    facing, cap, lx, ly, shadow, highlight, spread, rim_k, box_px, dpr, pose.roll,
                );
            }
        }
    }
    let plastic_done = draw_solid(
        ctx,
        &cfg.path,
        &type_key,
        half_depth,
        true,
        &mut body_pal,
        cfg,
        mode,
        body,
        cy,
        sy,
        cp,
        sp,
        facing,
        cap,
        lx,
        ly,
        shadow,
        highlight,
        spread,
        rim_k,
        box_px,
        dpr,
        pose.roll,
    );

    let zf = if facing >= 0.0 { 1.0 } else { -1.0 };
    let round = cfg.roundness.unwrap_or(1.0);
    let cushion = plastic_done;
    let sf = if cushion { 1.0 } else { profile(zf, cap) };
    let sample_surface = |x: f64, y: f64| -> f64 {
        if cushion {
            surface_at_tier(&type_key, tier_for(box_px * dpr), half_depth, x, y, round)
                .unwrap_or(0.8 * half_depth)
        } else {
            half_depth * sf
        }
    };
    let z_face = if cushion {
        0.55 * sample_surface(cfg.face_x, cfg.face_y)
    } else {
        half_depth
    };
    let fm0 = cy * sf;
    let fm1 = sy * sp * sf;
    let fm3 = cp * sf;
    let fe = zf * sy * z_face - 50.0 * fm0;
    let ffo = -zf * cy * sp * z_face - 50.0 * fm1 - 50.0 * fm3;
    let front_clip = if worn {
        transform_path(cfg.path.as_ref(), [fm0, fm1, 0.0, fm3, fe, ffo])
    } else {
        None
    };

    if facing > -0.2 {
        ctx.save();
        {
            let (ca, cb, cc, cd, ce, cf) = (body[0], body[1], body[2], body[3], body[4], body[5]);
            ctx.set_transform(
                ca * fm0 + cc * fm1,
                cb * fm0 + cd * fm1,
                cc * fm3,
                cd * fm3,
                ca * fe + cc * ffo + ce,
                cb * fe + cd * ffo + cf,
            );
            ctx.clip_path(cfg.path.as_ref(), FillRule::Winding);
            ctx.set_transform(ca, cb, cc, cd, ce, cf);
        }
        ctx.translate(cfg.face_x - 50.0, cfg.face_y - 50.0);
        ctx.scale(cfg.face_scale, cfg.face_scale);
        if mode == BotAvatarShading::Plastic && plastic_done {
            ctx.set_global_alpha(0.93);
        }
        let eyes = draw_face(ctx, pose, cfg);
        ctx.set_global_alpha(1.0);
        let glasses = wear.and_then(|w| {
            if w.glasses == crate::types::BotAvatarGlasses::None {
                None
            } else {
                Some(w.glasses)
            }
        });
        let turn = HeadTurn {
            yaw: pose.yaw,
            pitch: pose.pitch,
        };
        let light = ScreenLight { lx, ly };
        if let Some(glasses) = glasses {
            draw_glasses(ctx, glasses, &eyes, light, GlassesPass::Shadow, turn);
        }
        ctx.restore();
        if let Some(glasses) = glasses {
            ctx.save();
            ctx.translate(cfg.face_x - 50.0, cfg.face_y - 50.0);
            ctx.scale(cfg.face_scale, cfg.face_scale);
            draw_glasses(ctx, glasses, &eyes, light, GlassesPass::Frame, turn);
            ctx.restore();
        }
    }
    if worn {
        draw_wear_front(
            ctx,
            cfg.path.as_ref(),
            &wr,
            wear.expect("worn"),
            front_clip.as_ref(),
            &sample_surface,
            FaceAnchor {
                y: cfg.face_y,
                scale: cfg.face_scale,
            },
        );
    }
    draw_whirl(ctx, pose, &cfg.color, lx, ly, true, cfg.whirl.as_ref());
    ctx.restore();
}

#[allow(clippy::too_many_arguments)]
fn draw_solid(
    ctx: &mut Canvas2D,
    path: &Arc<Path>,
    key: &str,
    half_depth: f64,
    plush: bool,
    pal: &mut Palette,
    cfg: &DrawConfig,
    mode: BotAvatarShading,
    body: [f64; 6],
    cy: f64,
    sy: f64,
    cp: f64,
    sp: f64,
    facing: f64,
    cap: f64,
    lx: f64,
    ly: f64,
    shadow: f64,
    highlight: f64,
    spread: f64,
    rim_k: f64,
    box_px: f64,
    dpr: f64,
    roll: f64,
) -> bool {
    if mode == BotAvatarShading::Crisp {
        let stale = match &pal.grad {
            Some(g) => g.lx != lx || g.ly != ly,
            None => true,
        };
        if stale {
            let lit = PaintStyle::linear_gradient(
                lx * 56.0,
                ly * 56.0,
                -lx * 56.0,
                -ly * 56.0,
                &[
                    (0.0, pal.light.as_str()),
                    (0.45, pal.near.as_str()),
                    (1.0, pal.dark.as_str()),
                ],
            );
            let cap_g = PaintStyle::linear_gradient(
                lx * 46.0,
                ly * 46.0,
                -lx * 46.0,
                -ly * 46.0,
                &[(0.0, pal.cap_top.as_str()), (1.0, pal.cap_bottom.as_str())],
            );
            let grad = SliceGrad {
                lx,
                ly,
                lit,
                cap: cap_g,
            };
            store_palette_grad(&pal.base, shadow, highlight, grad.clone());
            pal.grad = Some(grad);
        }
    }

    let mut plastic_done = false;
    if mode == BotAvatarShading::Plastic || mode == BotAvatarShading::Fabric {
        let shading = if plush {
            mode
        } else {
            BotAvatarShading::Plastic
        };
        let solid = solid_cfg(cfg, Arc::clone(path), key.to_string(), shading);
        let back = back_of(cfg.back_light);
        let rig = Rig {
            cy,
            sy,
            cp,
            sp,
            facing,
            roll,
            half_depth,
            cap,
            lx,
            ly,
            bx: back.bx,
            by: back.by,
            dev: box_px * dpr,
            ctm: body,
            still: cfg.still,
            round: cfg.roundness.unwrap_or(1.0),
        };
        let front = js_min(85.0, js_max(0.0, cfg.light_front.unwrap_or(32.0)))
            * std::f64::consts::PI
            / 180.0;
        let material = Material {
            shadow,
            highlight,
            spread,
            rim: rim_k,
            vivid: Some(cfg.vivid.unwrap_or(0.0)),
            front: Some(front),
            shine: Some(cfg.shine.unwrap_or(0.0)),
            sheen: Some(cfg.sheen.unwrap_or(0.0)),
            back_soft: Some(cfg.back_softness.unwrap_or(1.0)),
        };
        // The Rust signature takes `pal.base`, not the whole palette. `union_path`
        // is the TS `null` (the slice union is local to the non-plastic fallback).
        plastic_done = draw_plastic_cap(ctx, &solid, &rig, pal.base.as_str(), None, &material);
    }

    let mode2 = if (mode == BotAvatarShading::Plastic || mode == BotAvatarShading::Fabric)
        && !plastic_done
    {
        BotAvatarShading::Smooth
    } else {
        mode
    };
    let soft = mode2 == BotAvatarShading::Smooth;
    let order = if facing >= 0.0 { 1.0 } else { -1.0 };
    let (ca, cb, cc, cd, ce, cf) = (body[0], body[1], body[2], body[3], body[4], body[5]);
    let lit_fill = if mode == BotAvatarShading::Crisp {
        pal.grad.as_ref().expect("crisp grad").lit.clone()
    } else {
        PaintStyle::css(&pal.near)
    };
    let cap_fill = if mode == BotAvatarShading::Crisp {
        pal.grad.as_ref().expect("crisp grad").cap.clone()
    } else {
        PaintStyle::css(&pal.base)
    };

    let mut pa = 1.0;
    let mut pb = 0.0;
    let mut pc = 0.0;
    let mut pd = 1.0;
    let mut pe = 0.0;
    let mut pf = 0.0;
    let mut union_pb = if soft { Some(PathBuilder::new()) } else { None };
    let mut j = 0;
    while j < SLICES && !plastic_done {
        let k = if order > 0.0 { j } else { SLICES - 1 - j };
        let z = -1.0 + (2.0 * k as f64) / (SLICES - 1) as f64;
        let scale = profile(z, cap);
        let near = j as f64 / (SLICES - 1) as f64;
        let m0 = cy * scale;
        let m1 = sy * sp * scale;
        let m3 = cp * scale;
        let e = z * sy * half_depth - 50.0 * m0;
        let fo = -z * cy * sp * half_depth - 50.0 * m1 - 50.0 * m3;
        let det = pa * pd - pb * pc;
        let ia = pd / det;
        let ib = -pb / det;
        let ic = -pc / det;
        let id = pa / det;
        let ie = (pc * pf - pd * pe) / det;
        let jf = (pb * pe - pa * pf) / det;
        ctx.transform(
            ia * m0 + ic * m1,
            ib * m0 + id * m1,
            ic * m3,
            id * m3,
            ia * e + ic * fo + ie,
            ib * e + id * fo + jf,
        );
        pa = m0;
        pb = m1;
        pc = 0.0;
        pd = m3;
        pe = e;
        pf = fo;
        let style = if soft {
            PaintStyle::css(&pal.smooth_mix[j])
        } else if j == SLICES - 1 {
            cap_fill.clone()
        } else if near > 0.6 {
            lit_fill.clone()
        } else {
            PaintStyle::css(&pal.crisp_mix[j])
        };
        ctx.set_fill_style(style);
        ctx.fill_path(path);
        if let Some(pb) = union_pb.as_mut() {
            append_path_transformed(pb, path, [m0, m1, 0.0, m3, e, fo]);
        }
        j += 1;
    }
    if !plastic_done {
        ctx.set_transform(ca, cb, cc, cd, ce, cf);
    }

    if soft && mode2 == BotAvatarShading::Smooth {
        if let Some(union_path) = union_pb.and_then(PathBuilder::finish) {
            ctx.save();
            ctx.clip_path(&union_path, FillRule::Winding);
            let sa = js_min(1.0, 0.34 * shadow);
            let sg = PaintStyle::RadialGradient {
                x0: -lx * 45.0,
                y0: -ly * 45.0,
                r0: 4.0 * spread,
                x1: -lx * 45.0,
                y1: -ly * 45.0,
                r1: 84.0 * spread,
                stops: vec![
                    (0.0, RgbaColor::from_rgba8(0, 0, 0, sa)),
                    (0.5, RgbaColor::from_rgba8(0, 0, 0, sa * 0.35)),
                    (1.0, RgbaColor::from_rgba8(0, 0, 0, 0.0)),
                ],
            };
            ctx.set_blend_mode(BlendMode::Multiply);
            ctx.set_fill_style(sg);
            ctx.fill_rect(-120.0, -120.0, 240.0, 240.0);
            ctx.set_blend_mode(BlendMode::SourceOver);
            let ha = js_min(1.0, 0.22 * highlight);
            let hg = PaintStyle::RadialGradient {
                x0: lx * 37.0,
                y0: ly * 37.0,
                r0: 0.0,
                x1: lx * 37.0,
                y1: ly * 37.0,
                r1: 62.0 * spread,
                stops: vec![
                    (0.0, RgbaColor::from_rgba8(255, 255, 255, ha)),
                    (0.6, RgbaColor::from_rgba8(255, 255, 255, ha * 0.23)),
                    (1.0, RgbaColor::from_rgba8(255, 255, 255, 0.0)),
                ],
            };
            ctx.set_fill_style(hg);
            ctx.fill_rect(-120.0, -120.0, 240.0, 240.0);
            ctx.restore();
        }
    }

    plastic_done
}

fn solid_cfg(
    cfg: &DrawConfig,
    path: Arc<Path>,
    type_key: String,
    shading: BotAvatarShading,
) -> DrawConfig {
    DrawConfig {
        path,
        face: cfg.face,
        face_x: cfg.face_x,
        face_y: cfg.face_y,
        face_scale: cfg.face_scale,
        color: cfg.color.clone(),
        ink: cfg.ink.clone(),
        shading,
        shadow: cfg.shadow,
        highlight: cfg.highlight,
        depth: cfg.depth,
        light: cfg.light,
        rim: cfg.rim,
        spread: cfg.spread,
        roundness: cfg.roundness,
        fur: cfg.fur,
        vivid: cfg.vivid,
        back_light: cfg.back_light,
        light_front: cfg.light_front,
        shine: cfg.shine,
        back_softness: cfg.back_softness,
        sheen: cfg.sheen,
        type_key: Some(type_key),
        still: cfg.still,
        fur_on_idle: cfg.fur_on_idle,
        parts: cfg.parts.clone(),
        parts_depth: cfg.parts_depth,
        parts_colors: cfg.parts_colors.clone(),
        theme: cfg.theme,
        dpr: cfg.dpr,
        sides: cfg.sides,
        whirl: cfg.whirl,
        wear: cfg.wear.clone(),
    }
}

fn part_palette(
    cfg: &DrawConfig,
    i: usize,
    shadow: f64,
    highlight: f64,
    body_pal: &Palette,
    part_pals: &mut HashMap<String, Palette>,
) -> Palette {
    let color = cfg
        .parts_colors
        .as_ref()
        .and_then(|cols| cols.get(i))
        .and_then(|c| c.as_deref());
    let Some(color) = color else {
        return body_pal.clone();
    };
    if let Some(found) = part_pals.get(color) {
        return found.clone();
    }
    let built = palette(color, shadow, highlight);
    part_pals.insert(color.to_string(), built.clone());
    built
}

fn is_worn(wear: &Wear) -> bool {
    wear.hat != crate::types::BotAvatarHat::None
        || wear.glasses != crate::types::BotAvatarGlasses::None
        || wear.headphones
        || wear.bow_tie
}

fn place_feature(
    ctx: &mut Canvas2D,
    x: f64,
    y: f64,
    yaw: f64,
    pitch: f64,
    alpha: f64,
    draw_fn: impl FnOnce(&mut Canvas2D),
) {
    let q = on_sphere(x, y, yaw, pitch);
    if q.z <= 0.02 || alpha <= 0.01 {
        return;
    }
    ctx.save();
    ctx.set_global_alpha(alpha * js_min(1.0, q.z * 5.0));
    ctx.translate(q.x, q.y);
    ctx.scale(js_max(0.02, q.sx), js_max(0.02, q.sy));
    draw_fn(ctx);
    ctx.restore();
}

fn append_path_transformed(pb: &mut PathBuilder, path: &Path, m: [f64; 6]) {
    let map = |x: f32, y: f32| -> (f32, f32) {
        let x = x as f64;
        let y = y as f64;
        (
            (m[0] * x + m[2] * y + m[4]) as f32,
            (m[1] * x + m[3] * y + m[5]) as f32,
        )
    };
    for seg in path.segments() {
        match seg {
            PathSegment::MoveTo(p) => {
                let (x, y) = map(p.x, p.y);
                pb.move_to(x, y);
            }
            PathSegment::LineTo(p) => {
                let (x, y) = map(p.x, p.y);
                pb.line_to(x, y);
            }
            PathSegment::QuadTo(c, p) => {
                let (cx, cy) = map(c.x, c.y);
                let (x, y) = map(p.x, p.y);
                pb.quad_to(cx, cy, x, y);
            }
            PathSegment::CubicTo(c1, c2, p) => {
                let (x1, y1) = map(c1.x, c1.y);
                let (x2, y2) = map(c2.x, c2.y);
                let (x, y) = map(p.x, p.y);
                pb.cubic_to(x1, y1, x2, y2, x, y);
            }
            PathSegment::Close => pb.close(),
        }
    }
}

fn transform_path(path: &Path, m: [f64; 6]) -> Option<Path> {
    let mut pb = PathBuilder::new();
    append_path_transformed(&mut pb, path, m);
    pb.finish()
}

fn clone_path(path: &Path) -> Path {
    let mut pb = PathBuilder::new();
    append_path_transformed(&mut pb, path, [1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);
    finish_path(pb)
}

fn finish_path(pb: PathBuilder) -> Path {
    pb.finish().unwrap_or_else(|| {
        let mut pb = PathBuilder::new();
        pb.move_to(0.0, 0.0);
        pb.line_to(0.01, 0.0);
        pb.finish().expect("degenerate path")
    })
}

/// `Math.round`: half towards +∞.
fn js_round(v: f64) -> f64 {
    if !v.is_finite() {
        return v;
    }
    (v + 0.5).floor()
}

/// `Math.min`, propagating NaN.
fn js_min(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else {
        a.min(b)
    }
}

/// `Math.max`, propagating NaN.
fn js_max(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else {
        a.max(b)
    }
}

fn q50_i(v: f64) -> i64 {
    js_round(v * 50.0) as i64
}

/// ECMAScript `Number.prototype.toFixed`: nearest `n / 10^digits` to the exact
/// f64, ties away from zero. The sign is kept when a negative value rounds to 0.
fn js_to_fixed(x: f64, digits: u32) -> String {
    if x.is_nan() {
        return "NaN".to_string();
    }
    if x.is_infinite() {
        return if x.is_sign_negative() {
            "-Infinity".to_string()
        } else {
            "Infinity".to_string()
        };
    }
    let neg = x < 0.0;
    let ax = x.abs();
    let n = round_scaled(ax, digits);
    let mut s = format_fixed(n, digits);
    if neg {
        s.insert(0, '-');
    }
    s
}

fn round_scaled(x: f64, digits: u32) -> u128 {
    if x == 0.0 {
        return 0;
    }
    let bits = x.to_bits();
    let mut exp = ((bits >> 52) & 0x7ff) as i32;
    let mut mant = bits & ((1_u64 << 52) - 1);
    if exp == 0 {
        if mant == 0 {
            return 0;
        }
        exp = -1022 - 52;
    } else if exp == 0x7ff {
        return 0;
    } else {
        mant |= 1_u64 << 52;
        exp = exp - 1023 - 52;
    }
    let mut pow10: u128 = 1;
    for _ in 0..digits {
        pow10 = pow10.saturating_mul(10);
    }
    let mant = mant as u128;
    if exp >= 0 {
        if exp >= 128 {
            return 0;
        }
        mant.saturating_mul(pow10).saturating_mul(1_u128 << exp)
    } else {
        let shift = (-exp) as u32;
        if shift >= 128 {
            return 0;
        }
        let den = 1_u128 << shift;
        let num = mant.saturating_mul(pow10);
        let q = num / den;
        let r = num % den;
        if r >= den / 2 { q + 1 } else { q }
    }
}

fn format_fixed(n: u128, digits: u32) -> String {
    let mut s = n.to_string();
    if digits == 0 {
        return s;
    }
    let d = digits as usize;
    while s.len() <= d {
        s.insert(0, '0');
    }
    let split = s.len() - d;
    s.insert(split, '.');
    s
}

fn fixed_f32(v: f64, digits: u32) -> f32 {
    js_to_fixed(v, digits).parse::<f64>().unwrap_or(v) as f32
}

#[cfg(test)]
fn palette_cache_len() -> usize {
    PALETTE_CACHE.lock().expect("palette cache").len()
}

#[cfg(test)]
fn reset_draw_caches() {
    PALETTE_CACHE.lock().expect("palette cache").clear();
    WHIRL_INK_CACHE.lock().expect("whirl ink cache").clear();
    EYE_PATHS.lock().expect("eye path cache").clear();
    MOUTH_PATHS.lock().expect("mouth path cache").clear();
}

#[cfg(test)]
fn eye_cache_len() -> usize {
    EYE_PATHS.lock().expect("eye path cache").len()
}

#[cfg(test)]
fn mouth_cache_len() -> usize {
    MOUTH_PATHS.lock().expect("mouth path cache").len()
}

#[cfg(test)]
fn whirl_cache_len() -> usize {
    WHIRL_INK_CACHE.lock().expect("whirl ink cache").len()
}

#[cfg(test)]
mod tests {
    use super::{
        LIGHT_DEFAULTS, Mouth, OVERSCAN, RISE, eye_cache_len, eye_path, eye_y, hsl_nums, js_round,
        js_to_fixed, mix_css, mouth_cache_len, mouth_path, on_sphere, palette, palette_cache_len,
        profile, reset_draw_caches, whirl_cache_len, whirl_ink, with_alpha,
    };
    use crate::canvas::Canvas2D;
    use crate::draw::{
        DrawConfig, EYE_GAP, FACE_R, OutlineParts, SideMode, WhirlKnobs, back_of, draw, draw_face,
        draw_whirl,
    };
    use crate::engine::{Pose, rest_pose};
    use crate::shapes::{body_path_for, part_paths_for};
    use crate::types::{BotAvatarFace, BotAvatarShading, BotAvatarState, BotAvatarType};
    use std::sync::{Mutex, MutexGuard};

    static TEST_LOCK: Mutex<()> = Mutex::new(());

    fn lock() -> MutexGuard<'static, ()> {
        TEST_LOCK.lock().expect("test lock")
    }

    fn close(a: f64, b: f64) {
        assert!((a - b).abs() < 1e-9, "{a} != {b}");
    }

    #[test]
    fn light_defaults_and_layout_match_draw_ts() {
        assert_eq!(OVERSCAN, 1.5);
        assert_eq!(RISE, 0.1);
        assert_eq!(LIGHT_DEFAULTS.fabric.shadow, 1.15);
        assert_eq!(LIGHT_DEFAULTS.fabric.highlight, 1.45);
        assert_eq!(LIGHT_DEFAULTS.fabric.light, 295.0);
        assert_eq!(LIGHT_DEFAULTS.fabric.rim, 0.6);
        assert_eq!(LIGHT_DEFAULTS.fabric.spread, 1.6);
        assert_eq!(LIGHT_DEFAULTS.other.shadow, 0.35);
        assert_eq!(LIGHT_DEFAULTS.other.highlight, 1.3);
        assert_eq!(LIGHT_DEFAULTS.other.light, 300.0);
        assert_eq!(LIGHT_DEFAULTS.other.rim, 0.5);
        assert_eq!(LIGHT_DEFAULTS.other.spread, 1.55);
        assert_eq!(eye_y(BotAvatarFace::Eyes), 1.0);
        assert_eq!(eye_y(BotAvatarFace::Mouth), -3.5);
    }

    #[test]
    fn js_round_and_to_fixed_match_ecmascript() {
        assert_eq!(js_round(-1.5), -1.0);
        assert_eq!(js_round(-0.5), 0.0);
        assert_eq!(js_round(0.5), 1.0);
        assert_eq!(js_round(1.5), 2.0);
        assert_eq!(js_round(2.5), 3.0);
        assert_eq!(js_round(-5.5), -5.0);
        assert_eq!(js_round(-5.6), -6.0);
        assert_eq!(js_to_fixed(1.25, 1), "1.3");
        assert_eq!(js_to_fixed(1.35, 1), "1.4");
        assert_eq!(js_to_fixed(-1.25, 1), "-1.3");
        assert_eq!(js_to_fixed(2.55, 1), "2.5");
        assert_eq!(js_to_fixed(9.95, 1), "9.9");
        assert_eq!(js_to_fixed(1.15, 1), "1.1");
        assert_eq!(js_to_fixed(0.35, 1), "0.3");
        assert_eq!(js_to_fixed(99.95, 1), "100.0");
        assert_eq!(js_to_fixed(10.125, 1), "10.1");
        assert_eq!(js_to_fixed(1.005, 1), "1.0");
        assert_eq!(js_to_fixed(-0.0004, 3), "-0.000");
        assert_eq!(js_to_fixed(-0.0005, 3), "-0.001");
        assert_eq!(js_to_fixed(0.0005, 3), "0.001");
        assert_eq!(js_to_fixed(-0.0, 3), "0.000");
        assert_eq!(js_to_fixed(1.0334017770513577, 3), "1.033");
        assert_eq!(js_to_fixed(1.2006067248975811, 3), "1.201");
    }

    #[test]
    fn profile_on_sphere_mix_and_back_light_match_ts() {
        close(profile(0.0, 0.95), 1.0);
        close(profile(1.0, 0.95), 0.95);
        close(profile(0.5, 0.95), 0.9933012701892219);
        close(profile(2.0, 0.9), 0.9);
        let q = on_sphere(12.5, 1.0, 0.3, -0.2);
        close(q.x, 19.459031991211813);
        close(q.y, 6.93683442590307);
        close(q.sx, 0.7453241415976126);
        close(q.sy, 0.97289963405368);
        close(q.z, 0.7251255846116905);
        let rest = on_sphere(-12.5, -3.5, 0.0, 0.0);
        close(rest.x, -12.41463909619965);
        close(rest.y, -3.5);
        close(rest.z, 0.9028514927169553);
        assert_eq!(
            mix_css("hsl(200.0 50.0% 40.0%)", "hsl(20.0 80.0% 70.0%)", 0.25),
            "hsl(155.0 57.5% 47.5%)"
        );
        assert_eq!(
            mix_css("hsl(10.0 20.0% 30.0%)", "hsl(350.0 10.0% 90.0%)", 0.0),
            "hsl(10.0 20.0% 30.0%)"
        );
        assert_eq!(
            mix_css("hsl(10.0 20.0% 30.0%)", "hsl(350.0 10.0% 90.0%)", 1.0),
            "hsl(350.0 10.0% 90.0%)"
        );
        let nums = hsl_nums("#35B8FF");
        assert_eq!(nums.len(), 3);
        close(nums[0], 201.1);
        close(nums[1], 100.0);
        close(nums[2], 60.4);
        assert_eq!(
            with_alpha("hsl(1.0 2.0% 3.0%)", 0.5),
            "hsl(1.0 2.0% 3.0% / 0.500)"
        );
        assert_eq!(
            with_alpha("hsl(1.0 2.0% 3.0%)", 2.0),
            "hsl(1.0 2.0% 3.0% / 1.000)"
        );
        assert_eq!(
            with_alpha("hsl(1.0 2.0% 3.0%)", -1.0),
            "hsl(1.0 2.0% 3.0% / 0.000)"
        );
        let back = back_of(Some(295.0));
        close(back.bx.unwrap(), -0.9063077870366498);
        close(back.by.unwrap(), -0.4226182617406996);
        assert!(back_of(None).bx.is_none());
        assert!(back_of(Some(f64::NAN)).by.is_none());
        assert!(back_of(Some(f64::INFINITY)).bx.is_none());
    }

    #[test]
    fn palette_and_paths_cache_and_match_ts_strings() {
        let _g = lock();
        reset_draw_caches();
        let color = "#35B8FF";
        let p = palette(color, 0.35, 1.3);
        assert_eq!(p.far, "hsl(201.1 100.0% 49.9%)");
        assert_eq!(p.near, "hsl(201.1 100.0% 56.2%)");
        assert_eq!(p.light, "hsl(201.1 100.0% 65.6%)");
        assert_eq!(p.dark, "hsl(201.1 100.0% 49.9%)");
        assert_eq!(p.cap_top, "hsl(201.1 100.0% 64.9%)");
        assert_eq!(p.cap_bottom, "hsl(201.1 100.0% 59.2%)");
        assert_eq!(p.crisp_mix.len(), 17);
        assert_eq!(p.smooth_mix.len(), 17);
        assert_eq!(p.crisp_mix[0], "hsl(201.1 100.0% 49.9%)");
        assert_eq!(p.crisp_mix[9], "hsl(201.1 100.0% 55.8%)");
        assert!(p.crisp_mix[10].is_empty());
        assert!(p.crisp_mix[16].is_empty());
        assert_eq!(p.smooth_mix[0], mix_css(&p.far, color, 0.0));
        // j = 4 → t = 0.25 → mix(far, color, 0.5). j = 7 → t = 7/16 → mix at 0.875.
        assert_eq!(p.smooth_mix[4], "hsl(201.1 100.0% 55.1%)");
        assert_eq!(p.smooth_mix[7], "hsl(201.1 100.0% 59.1%)");
        assert_eq!(p.smooth_mix[8], color);
        assert_eq!(p.smooth_mix[16], color);
        assert!(p.grad.is_none());
        let again = palette(color, 0.35, 1.3);
        assert_eq!(again.crisp_mix, p.crisp_mix);
        assert_eq!(again.smooth_mix, p.smooth_mix);
        assert_eq!(palette_cache_len(), 1);

        let ink = whirl_ink(color);
        assert_eq!(ink.base, "hsl(201.1 100.0% 70.4%)");
        assert_eq!(ink.light, "hsl(201.1 100.0% 90.4%)");
        assert_eq!(ink.dark, "hsl(201.1 100.0% 38.4%)");
        assert_eq!(ink.halo, "hsl(201.1 100.0% 80.4%)");
        let _ink2 = whirl_ink(color);
        assert_eq!(whirl_cache_len(), 1);

        let eye = eye_path(0.01, 1.1, -3.3);
        let pts = path_points(&eye);
        let expect = [
            (-0.02, 1.1),
            (-0.015, 0.138),
            (-0.010, -0.550),
            (-0.005, -0.962),
            (0.0, -1.100),
            (0.005, -0.962),
            (0.010, -0.550),
            (0.015, 0.138),
            (0.020, 1.100),
        ];
        assert_eq!(pts.len(), expect.len());
        for (got, want) in pts.iter().zip(expect) {
            assert!((got.0 as f64 - want.0).abs() < 5e-4, "{got:?} {want:?}");
            assert!((got.1 as f64 - want.1).abs() < 5e-4, "{got:?} {want:?}");
        }
        let _eye2 = eye_path(0.01, 1.1, -3.3);
        assert_eq!(eye_cache_len(), 1);

        let mouth = mouth_path(&Mouth {
            hw: 6.5,
            t0: 1.9,
            a: 2.0 / 3.0,
            yt: 3.53,
            ab: 2.0 / 3.0,
            yb: 3.53,
        });
        let mpts = path_points(&mouth);
        let want = [
            (-5.299, -1.473),
            (-2.210, 1.640),
            (2.210, 1.640),
            (5.299, -1.473),
            (7.263, -3.073),
            (9.664, -0.128),
            (7.701, 1.473),
            (2.210, 5.440),
            (-2.210, 5.440),
            (-7.701, 1.473),
            (-9.664, -0.128),
            (-7.263, -3.073),
            (-5.299, -1.473),
        ];
        assert_eq!(mpts.len(), want.len());
        for (got, exp) in mpts.iter().zip(want) {
            assert!((got.0 as f64 - exp.0).abs() < 5e-4, "{got:?} {exp:?}");
            assert!((got.1 as f64 - exp.1).abs() < 5e-4, "{got:?} {exp:?}");
        }
        let _mouth2 = mouth_path(&Mouth {
            hw: 6.5,
            t0: 1.9,
            a: 2.0 / 3.0,
            yt: 3.53,
            ab: 2.0 / 3.0,
            yb: 3.53,
        });
        assert_eq!(mouth_cache_len(), 1);

        reset_draw_caches();
        for i in 0..201 {
            palette(&format!("#{i:06x}"), 0.1, 0.1);
        }
        assert_eq!(palette_cache_len(), 201);
        palette("#ffffff", 0.1, 0.1);
        assert_eq!(palette_cache_len(), 1);
    }

    #[test]
    fn crisp_draw_caches_gradient_and_every_type_paints() {
        let _g = lock();
        reset_draw_caches();
        let color = "#35B8FF";
        assert!(palette(color, 0.35, 1.3).grad.is_none());
        for kind in BotAvatarType::ALL {
            for shading in [
                BotAvatarShading::Crisp,
                BotAvatarShading::Smooth,
                BotAvatarShading::Flat,
            ] {
                let mut ctx = Canvas2D::new(160, 160);
                let mut pose = rest_pose(BotAvatarState::Default);
                pose.yaw = 0.45;
                pose.pitch = -0.15;
                pose.whirl = 0.8;
                pose.whirl_angle = 0.4;
                let mut cfg = test_cfg(kind, shading, BotAvatarFace::Eyes);
                cfg.whirl = Some(WhirlKnobs {
                    strength: 0.6,
                    size: 1.0,
                    width: 1.0,
                    length: 1.0,
                    tilt: 1.0,
                });
                if kind == BotAvatarType::Ghost {
                    cfg.face = BotAvatarFace::Mouth;
                }
                draw(&mut ctx, 64.0, &pose, &cfg);
                assert!(painted(&ctx), "{kind} {shading:?}");
            }
        }
        let cached = palette(color, 0.35, 1.3);
        let grad = cached.grad.expect("crisp grad cached");
        close(grad.lx, -0.8660254037844386);
        close(grad.ly, -0.5);

        let mut pose = Pose::default();
        pose.yaw = 0.3;
        pose.pitch = -0.2;
        pose.w = [1.0, 0.0, 0.0];
        pose.eye_open = 1.0;
        let cfg = test_cfg(
            BotAvatarType::Circle,
            BotAvatarShading::Flat,
            BotAvatarFace::Eyes,
        );
        let mut ctx = Canvas2D::new(80, 80);
        let spots = draw_face(&mut ctx, &pose, &cfg);
        let half = EYE_GAP / 2.0;
        let left = on_sphere(-half, eye_y(BotAvatarFace::Eyes), pose.yaw, pose.pitch);
        close(spots[0].x, left.x);
        close(spots[0].y, left.y);
        close(spots[0].z, left.z);
        close(spots[0].ox, -half);
        close(spots[0].oy, 1.0);
        let fz = (FACE_R * FACE_R - half * half - 1.0).max(0.0).sqrt();
        close(spots[0].fz, fz);
        close(spots[1].ox, half);

        let mut blank = Canvas2D::new(220, 220);
        blank.translate(110.0, 110.0);
        let mut spin = Pose::default();
        spin.whirl = 1.0;
        spin.whirl_angle = 0.6;
        let knobs = WhirlKnobs {
            strength: 0.0,
            size: 1.0,
            width: 1.0,
            length: 1.0,
            tilt: 1.0,
        };
        draw_whirl(&mut blank, &spin, color, -0.8, -0.5, true, Some(&knobs));
        assert!(!painted(&blank));
        let knobs = WhirlKnobs {
            strength: 1.0,
            ..knobs
        };
        draw_whirl(&mut blank, &spin, color, -0.8, -0.5, true, Some(&knobs));
        draw_whirl(&mut blank, &spin, color, -0.8, -0.5, false, Some(&knobs));
        assert!(painted(&blank));

        let mut edge = Canvas2D::new(160, 160);
        let mut side = rest_pose(BotAvatarState::Sleeping);
        side.yaw = std::f64::consts::FRAC_PI_2;
        side.pitch = 1.2;
        let mut cfg = test_cfg(
            BotAvatarType::Droid,
            BotAvatarShading::Smooth,
            BotAvatarFace::Mouth,
        );
        cfg.parts = part_paths_for(BotAvatarType::Droid).map(OutlineParts::Many);
        draw(&mut edge, 64.0, &side, &cfg);
        assert!(painted(&edge));
    }

    fn test_cfg(kind: BotAvatarType, shading: BotAvatarShading, face: BotAvatarFace) -> DrawConfig {
        DrawConfig {
            path: body_path_for(kind),
            face,
            face_x: 50.0,
            face_y: 52.0,
            face_scale: 1.0,
            color: "#35B8FF".to_string(),
            ink: "#1E1A33".to_string(),
            shading,
            shadow: None,
            highlight: None,
            depth: None,
            light: None,
            rim: None,
            spread: None,
            roundness: None,
            fur: None,
            vivid: None,
            back_light: None,
            light_front: None,
            shine: None,
            back_softness: None,
            sheen: None,
            type_key: Some(kind.as_str().to_string()),
            still: true,
            fur_on_idle: false,
            parts: part_paths_for(kind).map(OutlineParts::Many),
            parts_depth: None,
            parts_colors: None,
            theme: None,
            dpr: Some(1.0),
            sides: SideMode::Vector,
            whirl: None,
            wear: None,
        }
    }

    fn painted(ctx: &Canvas2D) -> bool {
        ctx.pixmap.data().chunks_exact(4).any(|px| px[3] > 0)
    }

    fn path_points(path: &tiny_skia::Path) -> Vec<(f32, f32)> {
        let mut pts = Vec::new();
        for seg in path.segments() {
            match seg {
                tiny_skia::PathSegment::MoveTo(p) | tiny_skia::PathSegment::LineTo(p) => {
                    pts.push((p.x, p.y));
                }
                tiny_skia::PathSegment::QuadTo(c, p) => {
                    pts.push((c.x, c.y));
                    pts.push((p.x, p.y));
                }
                tiny_skia::PathSegment::CubicTo(c1, c2, p) => {
                    pts.push((c1.x, c1.y));
                    pts.push((c2.x, c2.y));
                    pts.push((p.x, p.y));
                }
                tiny_skia::PathSegment::Close => {}
            }
        }
        pts
    }
}
