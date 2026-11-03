//! Plastic and fabric cap: relief drawing, halo, and the fabric matcap.
//!
//! Ports these `plastic.ts` ranges:
//! - 799–881 `softMask`, `rimOf`, `ringOf`, `ringOrder`, `reach`
//! - 2554–2669 `buildFabricMatcap`
//! - 2816–2874 `Rig`, `capFrame` (`norm3` lives in `plastic`)
//! - 2878–2988 `State`, `stateFor`, `BIN`, `moved`
//! - 2990–2996 `mulAffine`
//! - 3001–3211 `Rays`, `raysOf`, `sliceGradient`, `hazeColor`, `brightest`, `Reach`,
//!   `stackReach`, `haloStretch`, `haloRays`, `haloMiss`, `measureMiss`, `MISS_N`, `MISS_STEP`
//! - 3223–3231 `WEBKIT`, `GECKO` (neither engine exists here, so both are false)
//! - 3233–3962 `drawPlasticCap`
#![allow(dead_code, unused_variables)]

use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet};
use std::f64::consts::{PI, TAU};
use std::hash::{Hash, Hasher};
use std::mem;
use std::sync::{Arc, LazyLock, Mutex, MutexGuard};

use tiny_skia::{BlendMode, FillRule, FilterQuality, LineJoin, Path, Pixmap};

use crate::canvas::{Canvas2D, PaintStyle, RgbaColor, upload_straight_rgba};
use crate::draw::DrawConfig;
use crate::plastic::{
    CONIC_STOPS, E_XY, E_Z, FRONT_FADE, Form, Frame, INF, M, MM, Material, PAD, Relief, SPAN, V3,
    build_matcap, edge_lift, edt_2d, form_for, idle, linear_color, make_canvas, norm3, path_id,
    relief_for, shade_texels, smooth, srgb, tier_for, vivify,
};
use crate::plastic_fur::{
    BACK_SOFT, FILM_M, FILM_SPAN, FUR_STOCK, Fur, FurLights, HALO_BLEND, HALO_EASE, HALO_FIT,
    HALO_INSET, HALO_SHADE, HALO_VEIL, KEY_EL, PILE_H, back_from_key, bilerp, fabric_back,
    fabric_key, fur_ready, gap_film, shared_cut, sheen_film, tip_film,
};
use crate::types::BotAvatarShading;

/// `BIN` (2987): rebuild the matcap once a direction has moved this far.
pub(crate) const BIN: f64 = 1.0 / 48.0;
pub(crate) const MISS_N: u32 = 96;
pub(crate) const MISS_STEP: f64 = 0.1;
/// No WebKit and no Firefox in this port. `plastic.ts` lines 3223–3231.
pub(crate) const WEBKIT: bool = false;
pub(crate) const GECKO: bool = false;

/// Rig the cap is drawn in. Screen Y of `lx` / `ly` / `bx` / `by` is down.
#[derive(Debug, Clone, Copy)]
pub struct Rig {
    pub cy: f64,
    pub sy: f64,
    pub cp: f64,
    pub sp: f64,
    /// `cos(yaw)·cos(pitch)`, unfloored.
    pub facing: f64,
    pub roll: f64,
    pub half_depth: f64,
    pub cap: f64,
    pub lx: f64,
    pub ly: f64,
    pub bx: Option<f64>,
    pub by: Option<f64>,
    /// Avatar box in device pixels.
    pub dev: f64,
    /// Context transform for body space `(a, b, c, d, e, f)`.
    pub ctm: [f64; 6],
    pub still: bool,
    /// 1 a cushion, toward 0 a slab. The draw site passes `roundness` or 1.
    pub round: f64,
}

/// Rim points thinned to about a hundred, in order round the outline (`ringOf`, 848–866).
#[derive(Debug, Clone)]
pub(crate) struct Ring {
    pub x: Vec<f32>,
    pub y: Vec<f32>,
    pub d: Vec<f32>,
}

/// Front colour sampled along 24 rays (`Rays`, 3001–3005).
#[derive(Debug, Clone)]
pub struct Rays {
    pub d: Vec<Vec<f32>>,
    pub c: Vec<Vec<f32>>,
}

/// Which slice forms the stack's reach along a direction (`Reach`, 3098–3101).
#[derive(Debug, Clone, Copy)]
pub struct Reach {
    pub k: u32,
    pub far: bool,
}

/// Per-ray halo colour (`haloRays`, 3125).
#[derive(Debug, Clone)]
pub struct HaloRays {
    pub inset: Vec<f32>,
    pub dark: Vec<f32>,
}

/// Per-canvas drawing state (`State`, 2878–2960). Keyed by canvas identity and outline.
pub struct CapState {
    pub n: u32,
    pub img: Option<Pixmap>,
    pub mc: Vec<f32>,
    pub l: Option<V3>,
    pub v: Option<V3>,
    pub lx: f64,
    pub ly: f64,
    pub base: String,
    pub shadow: f64,
    pub highlight: f64,
    pub spread: f64,
    pub rim: f64,
    pub vivid: f64,
    pub front: f64,
    pub sheen: f64,
    pub back_soft: f64,
    pub k: Option<V3>,
    pub version: u32,
    pub mc_prev: Vec<f32>,
    pub mc_mix: Vec<f32>,
    pub mix_version: u32,
    pub blend_t: f64,
    pub blend_frames: u32,
    pub since_build: u32,
    pub img_version: i32,
    pub img_ao_k: f64,
    pub img_form: Option<Arc<Form>>,
    pub ao_k: f64,
    pub ao_mul: Vec<f32>,
    pub scratch: [Option<Pixmap>; 2],
    pub scratch_idx: usize,
    pub scratch_n: u32,
    pub scratch_stale: bool,
    pub sprites: [Option<Pixmap>; 5],
    pub slice_g: Vec<PaintStyle>,
    pub slice_ins: Vec<f64>,
    /// Inset quantized with [`f64::to_bits`], matching `sliceSpr`'s number key.
    pub slice_spr: HashMap<u64, Pixmap>,
    pub slice_spr_key: String,
    pub slice_spr_made: HashSet<u64>,
    pub slice_key: String,
    pub body: [Option<BodyBuf>; 2],
    pub body_idx: usize,
    pub sprite_version: i32,
    pub sprite_px: f64,
    pub sprite_fur: Option<Arc<Fur>>,
    /// Front-view body, design space, reused while shading inputs stay put.
    pub card: Option<Pixmap>,
    pub card_stamp: u64,
    /// Opaque pixel bounds of `card`: x0, y0, x1, y1.
    pub card_box: [i32; 4],
    pub miss: [f64; 2],
    pub miss_at: [f64; 2],
    pub turn_at: [f64; 2],
    pub fade: Option<[f64; 2]>,
    pub lay: f64,
    pub lay_tmp: Option<Pixmap>,
    pub halo_tmp: Option<Pixmap>,
}

/// Fabric slice buffer (`State.body` entries, 2938).
pub struct BodyBuf {
    pub pixmap: Pixmap,
    pub w: u32,
    pub h: u32,
}

impl CapState {
    fn fresh() -> Self {
        let mc = vec![0.0; MM * 3];
        Self {
            n: 0,
            img: None,
            mc: mc.clone(),
            l: None,
            v: None,
            lx: f64::NAN,
            ly: f64::NAN,
            base: String::new(),
            shadow: f64::NAN,
            highlight: f64::NAN,
            spread: f64::NAN,
            rim: f64::NAN,
            vivid: f64::NAN,
            front: f64::NAN,
            sheen: f64::NAN,
            back_soft: f64::NAN,
            k: None,
            version: 0,
            mc_prev: mc.clone(),
            mc_mix: mc,
            mix_version: 0,
            blend_t: 1.0,
            blend_frames: 1,
            since_build: 0,
            img_version: -1,
            img_ao_k: f64::NAN,
            img_form: None,
            ao_k: -1.0,
            ao_mul: vec![0.0; 256],
            scratch: [None, None],
            scratch_idx: 0,
            scratch_n: 0,
            scratch_stale: true,
            sprites: [None, None, None, None, None],
            slice_g: Vec::new(),
            slice_ins: Vec::new(),
            slice_spr: HashMap::new(),
            slice_spr_key: String::new(),
            slice_spr_made: HashSet::new(),
            slice_key: String::new(),
            body: [None, None],
            body_idx: 0,
            sprite_version: -1,
            sprite_px: 0.0,
            sprite_fur: None,
            card: None,
            card_stamp: 0,
            card_box: [0, 0, 0, 0],
            miss: [0.0, 0.0],
            miss_at: [0.0, 0.0],
            turn_at: [0.0, 0.0],
            fade: None,
            lay: 1.0,
            lay_tmp: None,
            halo_tmp: None,
        }
    }
}

static STATES: LazyLock<Mutex<HashMap<String, CapState>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
static RIMS: LazyLock<Mutex<HashMap<u64, Vec<f32>>>> = LazyLock::new(|| Mutex::new(HashMap::new()));
static RINGS: LazyLock<Mutex<HashMap<u64, Ring>>> = LazyLock::new(|| Mutex::new(HashMap::new()));
static SOFTS: LazyLock<Mutex<HashMap<(u64, u32), Option<Pixmap>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
static MISSES: LazyLock<Mutex<HashMap<String, Option<[f64; 2]>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Holds the cap-state map so [`state_for`] can hand back one record.
pub(crate) struct StateGuard {
    guard: MutexGuard<'static, HashMap<String, CapState>>,
    key: String,
}

impl StateGuard {
    pub(crate) fn get(&mut self) -> &mut CapState {
        self.guard.get_mut(&self.key).expect("cap state")
    }
}

/// `Math.round`: half away from −∞, so half toward +∞.
#[inline]
fn js_round(x: f64) -> f64 {
    (x + 0.5).floor()
}

/// Content stamp for the form. `form_for` returns an owned [`Form`], so the
/// TS `WeakMap` identity is the field values the rim and the miss read.
fn form_stamp(form: &Form) -> u64 {
    if form.id != 0 {
        return form.id;
    }
    let mut h = DefaultHasher::new();
    form.n.hash(&mut h);
    form.sd.len().hash(&mut h);
    for v in &form.sd {
        v.to_bits().hash(&mut h);
    }
    h.finish()
}

fn form_same(a: &Form, b: &Form) -> bool {
    if a.id != 0 && a.id == b.id {
        return true;
    }
    a.n == b.n
        && a.sd == b.sd
        && a.ao == b.ao
        && a.h == b.h
        && a.i00 == b.i00
        && a.wx == b.wx
        && a.wy == b.wy
}

fn pixmap_same(a: &Option<Pixmap>, b: &Option<Pixmap>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => {
            a.width() == b.width() && a.height() == b.height() && a.data() == b.data()
        }
        _ => false,
    }
}

/// `fur_ready` returns an owned pile, so the sprite cache compares bytes.
fn fur_same(a: &Option<Arc<Fur>>, b: &Option<Arc<Fur>>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => {
            if a.cache_key != 0 && a.cache_key == b.cache_key {
                return true;
            }
            a.r == b.r
                && pixmap_same(&a.mask, &b.mask)
                && pixmap_same(&a.haze, &b.haze)
                && pixmap_same(&a.cut, &b.cut)
                && pixmap_same(&a.core, &b.core)
                && pixmap_same(&a.dark, &b.dark)
                && pixmap_same(&a.light, &b.light)
                && pixmap_same(&a.glint, &b.glint)
                && pixmap_same(&a.back, &b.back)
                && pixmap_same(&a.back_soft, &b.back_soft)
        }
        _ => false,
    }
}

fn engine_is_webkit() -> bool {
    WEBKIT
}

fn engine_is_gecko() -> bool {
    GECKO
}

fn set_smoothing(g: &mut Canvas2D, high: bool) {
    g.set_image_smoothing(true);
    g.set_image_smoothing_quality(if high {
        FilterQuality::Bicubic
    } else {
        FilterQuality::Bilinear
    });
}

fn canvas_from(pm: Pixmap) -> Canvas2D {
    let mut c = Canvas2D::new(1, 1);
    c.pixmap = pm;
    c
}

fn into_pixmap(mut c: Canvas2D) -> Pixmap {
    mem::replace(&mut c.pixmap, Pixmap::new(1, 1).expect("pixmap"))
}

fn fixed3(v: f64) -> f32 {
    ((v * 1000.0).round() / 1000.0) as f32
}

/// `softMask` (802–825).
pub(crate) fn soft_mask(form: &Form, r: u32) -> Option<Pixmap> {
    let stamp = form_stamp(form);
    let cached = {
        let cache = SOFTS.lock().expect("soft masks");
        cache.get(&(stamp, r)).cloned()
    };
    if let Some(hit) = cached {
        return hit;
    }
    let pm = raster_soft(form, r);
    let mut cache = SOFTS.lock().expect("soft masks");
    if cache.len() > 64 {
        cache.clear();
    }
    cache.insert((stamp, r), pm.clone());
    pm
}

fn raster_soft(form: &Form, r: u32) -> Option<Pixmap> {
    if r == 0 {
        return None;
    }
    let Some(mut pm) = Pixmap::new(r, r) else {
        return None;
    };
    let n = form.n as f64;
    let px = r as f64 / SPAN;
    let mut rgba = vec![0u8; (r as usize) * (r as usize) * 4];
    for y in 0..r {
        for x in 0..r {
            let d = bilerp(
                &form.sd,
                form.n as usize,
                (((x as f64 + 0.5) / px) / SPAN) * n - 0.5,
                (((y as f64 + 0.5) / px) / SPAN) * n - 0.5,
            );
            let t = (d + 0.2) / 0.6;
            let k = ((y * r + x) * 4) as usize;
            rgba[k] = 255;
            rgba[k + 1] = 255;
            rgba[k + 2] = 255;
            rgba[k + 3] = if t >= 1.0 {
                255
            } else if t <= 0.0 {
                0
            } else {
                js_round(255.0 * t * t * (3.0 - 2.0 * t)) as u8
            };
        }
    }
    upload_straight_rgba(&mut pm, &rgba);
    Some(pm)
}

/// Outline rim as `(x, y, distance)` triples (`rimOf`, 833–843).
pub(crate) fn rim_of(form: &Form) -> Vec<f32> {
    let stamp = form_stamp(form);
    if let Some(hit) = {
        let cache = RIMS.lock().expect("rims");
        cache.get(&stamp).cloned()
    } {
        return hit;
    }
    let n = form.n as usize;
    let u = SPAN / form.n as f64;
    let limit = 2.0 * u;
    let mut pts = Vec::new();
    for j in 0..n {
        for x in 0..n {
            let d = form.sd[j * n + x];
            if d >= 0.0 && (d as f64) < limit {
                pts.push(((x as f64 + 0.5) * u - PAD) as f32);
                pts.push(((j as f64 + 0.5) * u - PAD) as f32);
                pts.push(d);
            }
        }
    }
    let mut cache = RIMS.lock().expect("rims");
    if cache.len() > 48 {
        cache.clear();
    }
    cache.insert(stamp, pts.clone());
    pts
}

/// `ringOf` (848–867).
pub(crate) fn ring_of(form: &Form) -> Ring {
    let stamp = form_stamp(form);
    if let Some(hit) = {
        let cache = RINGS.lock().expect("rings");
        cache.get(&stamp).cloned()
    } {
        return hit;
    }
    let rim = rim_of(form);
    let mut mx = 0.0;
    let mut my = 0.0;
    let thirds = rim.len() / 3;
    let mut i = 0;
    while i + 2 < rim.len() {
        mx += rim[i] as f64;
        my += rim[i + 1] as f64;
        i += 3;
    }
    let denom = if thirds == 0 { 1.0 } else { thirds as f64 };
    mx /= denom;
    my /= denom;
    let mut all = Vec::with_capacity(thirds);
    i = 0;
    while i + 2 < rim.len() {
        let x = rim[i];
        let y = rim[i + 1];
        let ang = (y as f64 - my).atan2(x as f64 - mx);
        all.push((x, y, ang, rim[i + 2]));
        i += 3;
    }
    all.sort_by(|p, q| p.2.partial_cmp(&q.2).unwrap_or(std::cmp::Ordering::Equal));
    let step = (all.len() / 96).max(1);
    let mut x = Vec::new();
    let mut y = Vec::new();
    let mut d = Vec::new();
    for (i, p) in all.iter().enumerate() {
        if i % step == 0 {
            x.push(p.0);
            y.push(p.1);
            d.push(p.3);
        }
    }
    let ring = Ring { x, y, d };
    let mut cache = RINGS.lock().expect("rings");
    if cache.len() > 48 {
        cache.clear();
    }
    cache.insert(stamp, ring.clone());
    ring
}

/// `ringOrder` (869–873).
#[inline]
pub(crate) fn ring_order(n: usize, a: f64, k: usize) -> usize {
    if n == 0 {
        return 0;
    }
    let start = js_round(((a + PI) / TAU) * n as f64) as i64;
    let off = if k & 1 == 1 {
        ((k + 1) >> 1) as i64
    } else {
        -((k >> 1) as i64)
    };
    let n_i = n as i64;
    let mut m = (start + off) % n_i;
    if m < 0 {
        m += n_i;
    }
    m as usize
}

/// How far the rim reaches along `(ex, ey)` (`reach`, 874–881).
#[inline]
pub(crate) fn reach(rim: &[f32], ex: f64, ey: f64) -> f64 {
    let mut m = -INF;
    let mut i = 0;
    while i + 2 < rim.len() {
        let v = rim[i] as f64 * ex + rim[i + 1] as f64 * ey + rim[i + 2] as f64;
        if v > m {
            m = v;
        }
        i += 3;
    }
    m
}

/// Fabric matcap (`buildFabricMatcap`, 2554–2669). `out` is `M × M × rgb`, sRGB 0–255.
///
/// Texels with `r² > 1.14` are left as they were, matching the TS `continue`.
pub fn build_fabric_matcap(out: &mut [f32], c: V3, frame: &Frame, material: &Material) {
    let vivid = material.vivid.unwrap_or(0.0);
    let l = fabric_key(frame.u, frame.v, material.front.unwrap_or(KEY_EL));
    let lb = fabric_back(frame.k, frame.v);
    let amb = (0.03_f64).max(0.2 - 0.15 * material.shadow);
    let fill_k = (0.04_f64).max(0.24 - 0.13 * material.shadow);
    let wrap = (0.7_f64).min(0.05 + 0.12 * material.spread);
    let kd = 1.3;
    let floor_k = 0.34 * (1.6_f64).min(material.shadow / 0.35);
    let fl = norm3([
        -0.55 * frame.u[0] + 0.85 * frame.v[0] + 0.15 * frame.d[0],
        -0.55 * frame.u[1] + 0.85 * frame.v[1] + 0.15 * frame.d[1],
        -0.55 * frame.u[2] + 0.85 * frame.v[2] + 0.15 * frame.d[2],
    ]);
    let satin = material.sheen.unwrap_or(0.0);
    let back_soft = (1.0_f64).min((0.0_f64).max(material.back_soft.unwrap_or(BACK_SOFT)));
    let sheen_k = 0.16 * material.highlight * satin;
    let rim_k = 1.15 * material.rim * satin;
    let sss_k = 0.2 + 0.25 * vivid;
    let mx = c[0].max(c[1]).max(c[2]).max(0.02);
    let cb = [c[0] / mx, c[1] / mx, c[2] / mx];
    let rim_in = cb;
    let w_r = 0.45 * (1.0 - vivid);
    let w_s = 0.25 * (1.0 - vivid);
    let rim_out = [
        w_r + (1.0 - w_r) * cb[0],
        w_r + (1.0 - w_r) * cb[1],
        w_r + (1.0 - w_r) * cb[2],
    ];
    let sheen_c = [
        w_s + (1.0 - w_s) * cb[0],
        w_s + (1.0 - w_s) * cb[1],
        w_s + (1.0 - w_s) * cb[2],
    ];
    let y_of = |v: V3| 0.2126 * v[0] + 0.7152 * v[1] + 0.0722 * v[2];
    let de = 1.8 + 2.2 * vivid;
    let d0 = [c[0].powf(de), c[1].powf(de), c[2].powf(de)];
    let dk = y_of(c) / (1e-4_f64).max(y_of(d0));
    let deep = [d0[0] * dk, d0[1] * dk, d0[2] * dk];
    let sc = [
        cb[0] * cb[0] * c[0],
        cb[1] * cb[1] * c[1],
        cb[2] * cb[2] * c[2],
    ];
    let denom = (M - 1) as f64;
    for j in 0..M {
        for i in 0..M {
            let mut nx = (i as f64 / denom) * 2.0 - 1.0;
            let mut ny = (j as f64 / denom) * 2.0 - 1.0;
            let mut r2 = nx * nx + ny * ny;
            if r2 > 1.14 {
                continue;
            }
            if r2 > 1.0 {
                let s = 1.0 / r2.sqrt();
                nx *= s;
                ny *= s;
                r2 = 1.0;
            }
            let nz = (1.0 - r2).sqrt();
            let nl = nx * l[0] + ny * l[1] + nz * l[2];
            let nv = (0.0_f64).max(nx * frame.v[0] + ny * frame.v[1] + nz * frame.v[2]);
            let nd = nx * frame.d[0] + ny * frame.d[1] + nz * frame.d[2];
            let nb = nx * lb[0] + ny * lb[1] + nz * lb[2];
            let q = 1.0 - nv;
            let w = wrap + (0.45 - wrap) * q * q;
            let mut dif = (1.0_f64).min((0.0_f64).max((nl + w) / (1.0 + w)));
            dif = dif.powf(1.35);
            let fill = fill_k * (0.0_f64).max(nx * fl[0] + ny * fl[1] + nz * fl[2]);
            let floor = 1.0 - floor_k * (0.0_f64).max(nd) * (0.6 + 0.4 * q);
            let light = (amb + kd * dif + fill) * floor;
            let t = (1.0_f64).min((0.0_f64).max(1.0 - light));
            let sb = (0.0_f64).max(1.0 - ((nl + 0.08) * (nl + 0.08)) / 0.18);
            let sss = sss_k * sb * sb * floor;
            let lit = (1.0_f64).min((0.0_f64).max((nl + 0.2) / 0.8));
            let sheen = sheen_k * q * q * lit;
            let bl = smooth(-0.3, 0.6, nb);
            let rq = smooth(0.18 - 0.12 * back_soft, 0.62 + 0.18 * back_soft, q);
            let rim = rim_k * rq * rq.sqrt() * (0.3 + 0.7 * bl) * (1.0 - 0.7 * (0.0_f64).max(nd));
            let hot = rq * rq * rq.sqrt() * (1.0 - 0.5 * back_soft);
            let mut rgb = [0.0; 3];
            for ch in 0..3 {
                let base = c[ch] + (deep[ch] - c[ch]) * t * 0.9;
                rgb[ch] = base * light
                    + sss * sc[ch]
                    + sheen * sheen_c[ch]
                    + rim * (rim_in[ch] + (rim_out[ch] - rim_in[ch]) * hot);
            }
            let mut r = rgb[0];
            let mut g = rgb[1];
            let mut b = rgb[2];
            let peak = r.max(g).max(b);
            if peak > 0.8 {
                let kk = (0.8 + 0.2 * (1.0 - (-(peak - 0.8) / 0.2).exp())) / peak;
                r *= kk;
                g *= kk;
                b *= kk;
            }
            if vivid > 0.0 {
                let mut px = [r, g, b];
                vivify(&mut px, vivid, 1.0);
                r = px[0];
                g = px[1];
                b = px[2];
            }
            let k = ((j * M + i) * 3) as usize;
            out[k] = srgb(r) as f32;
            out[k + 1] = srgb(g) as f32;
            out[k + 2] = srgb(b) as f32;
        }
    }
}

/// Screen-space vectors into the cap's frame (`capFrame`, 2848–2874).
pub fn cap_frame(rig: &Rig) -> Frame {
    let cr = rig.roll.cos();
    let sr = rig.roll.sin();
    let lx = cr * rig.lx + sr * rig.ly;
    let ly = -sr * rig.lx + cr * rig.ly;
    let mirror = if rig.facing < 0.0 { -1.0 } else { 1.0 };
    let zf = mirror * (1.0_f64).min(rig.facing.abs() / 0.16);
    let cy = rig.cy;
    let sy = rig.sy;
    let cp = rig.cp;
    let sp = rig.sp;
    let local = |x: f64, y: f64, z: f64, zk: f64| -> V3 {
        norm3([
            cy * x + sy * sp * y - sy * cp * z,
            cp * y + sp * z,
            zk * (sy * x - cy * sp * y + cy * cp * z),
        ])
    };
    let l = local(E_XY * lx, E_XY * ly, E_Z, zf);
    let v = local(0.0, 0.0, 1.0, mirror);
    let h = norm3([l[0] + v[0], l[1] + v[1], l[2] + v[2]]);
    let u = local(lx, ly, 0.0, mirror);
    let cw = ((80.0 * PI) / 180.0).cos();
    let sw = ((80.0 * PI) / 180.0).sin();
    let wx = cw * lx - sw * ly;
    let wy = sw * lx + cw * ly;
    let ws = norm3([0.55 * wx, 0.55 * wy, 0.83]);
    let as_ = norm3([ws[1], -ws[0], 0.0]);
    let bs = [
        ws[1] * as_[2] - ws[2] * as_[1],
        ws[2] * as_[0] - ws[0] * as_[2],
        ws[0] * as_[1] - ws[1] * as_[0],
    ];
    let w = local(ws[0], ws[1], ws[2], mirror);
    let a = local(as_[0], as_[1], as_[2], mirror);
    let b = local(bs[0], bs[1], bs[2], mirror);
    let d = local(sr, cr, 0.0, mirror);
    let (kx0, ky0) = match (rig.bx, rig.by) {
        (Some(bx), Some(by)) => (bx, by),
        _ => {
            let p = back_from_key(rig.lx, rig.ly);
            (p[0], p[1])
        }
    };
    let k = local(cr * kx0 + sr * ky0, -sr * kx0 + cr * ky0, 0.0, mirror);
    Frame {
        l,
        v,
        h,
        u,
        w,
        a,
        b,
        d,
        k,
    }
}

/// Cached cap state for this canvas and outline (`stateFor`, 2962–2983).
/// The key is the pixmap address plus `outline`, so two avatars do not share a scratch.
pub(crate) fn state_for(ctx: &Canvas2D, outline: &str) -> StateGuard {
    let addr = ctx.id;
    let key = format!("{addr}|{outline}");
    let mut guard = STATES.lock().expect("cap states");
    if !guard.contains_key(&key) {
        let prefix = format!("{addr}|");
        let n = guard.keys().filter(|k| k.starts_with(&prefix)).count();
        if n > 16 {
            guard.retain(|k, _| !k.starts_with(&prefix));
        }
        guard.insert(key.clone(), CapState::fresh());
    }
    StateGuard { guard, key }
}

/// Drop every state a canvas keeps, once that canvas is gone.
pub(crate) fn forget_states(ctx: &Canvas2D) {
    let prefix = format!("{}|", ctx.id);
    STATES
        .lock()
        .expect("cap states")
        .retain(|k, _| !k.starts_with(&prefix));
}

/// `moved` (2988).
#[inline]
pub(crate) fn moved(a: V3, b: Option<V3>) -> bool {
    match b {
        None => true,
        Some(b) => {
            (a[0] - b[0]).abs() >= BIN || (a[1] - b[1]).abs() >= BIN || (a[2] - b[2]).abs() >= BIN
        }
    }
}

/// `A · B` for canvas affines `(a, b, c, d, e, f)` (`mulAffine`, 2990–2996).
pub fn mul_affine(a: [f64; 6], b: [f64; 6]) -> [f64; 6] {
    crate::canvas::mul_affine(a, b)
}

/// `raysOf` (3006–3028). `img` holds straight RGBA, the TS `ImageData`.
pub(crate) fn rays_of(img: &Pixmap, form: &Form, c0: [f64; 2]) -> Rays {
    let n = form.n;
    let nf = n as f64;
    let data = img.data();
    let to_g = |v: f64| ((v + PAD) / SPAN) * nf - 0.5;
    let mut d_out = Vec::with_capacity(CONIC_STOPS);
    let mut c_out = Vec::with_capacity(CONIC_STOPS);
    for s in 0..CONIC_STOPS {
        let phi = (s as f64 / CONIC_STOPS as f64) * TAU;
        let (uy, ux) = phi.sin_cos();
        let mut ds = Vec::with_capacity(160);
        let mut cs = Vec::with_capacity(160 * 3);
        let mut r = 0.0;
        while r < 80.0 {
            let x = c0[0] + ux * r;
            let y = c0[1] + uy * r;
            let d = bilerp(&form.sd, n as usize, to_g(x), to_g(y));
            if d < 0.0 {
                break;
            }
            let gx = js_round(to_g(x)) as i32;
            let gy = js_round(to_g(y)) as i32;
            if gx < 0 || gy < 0 || gx >= n as i32 || gy >= n as i32 {
                break;
            }
            let k = ((gy as u32 * n + gx as u32) * 4) as usize;
            ds.push(d as f32);
            cs.push(data[k] as f32);
            cs.push(data[k + 1] as f32);
            cs.push(data[k + 2] as f32);
            r += 0.5;
        }
        d_out.push(ds);
        c_out.push(cs);
    }
    Rays { d: d_out, c: c_out }
}

fn ray_index(a: f64) -> usize {
    let wrapped = {
        let m = a % TAU;
        if m < 0.0 { m + TAU } else { m }
    };
    let step = TAU / CONIC_STOPS as f64;
    let r = js_round(wrapped / step) as i64;
    r.rem_euclid(CONIC_STOPS as i64) as usize
}

fn trunc_channel(v: f64) -> u8 {
    let t = v.trunc();
    if t <= 0.0 {
        0
    } else if t >= 255.0 {
        255
    } else {
        t as u8
    }
}

/// One slice's side light (`sliceGradient`, 3033–3054).
/// Conic gradients are not in [`PaintStyle`]; the linear fallback is the portable fill.
pub(crate) fn slice_gradient(
    ctx: &mut Canvas2D,
    rays: &Rays,
    inset: SliceInset<'_>,
    dark: SliceDark<'_>,
    c0: [f64; 2],
    lxy: [f64; 2],
    linear: bool,
) -> PaintStyle {
    let _ = (ctx, linear);
    let at = |s: usize| -> RgbaColor {
        let s = s % CONIC_STOPS;
        let d = &rays.d[s];
        let c = &rays.c[s];
        let ins = match inset {
            SliceInset::Uniform(v) => v,
            SliceInset::PerRay(a) => a.get(s).copied().unwrap_or(0.0) as f64,
        };
        let dark_v = match dark {
            SliceDark::Uniform(v) => v,
            SliceDark::PerRay(a) => a.get(s).copied().unwrap_or(0.0) as f64,
        };
        let k = 1.0 - dark_v;
        if c.len() < 3 {
            return RgbaColor::BLACK;
        }
        let mut j = 0;
        for i in 0..d.len() {
            if d[i] as f64 >= ins {
                j = i;
            }
        }
        let base = j * 3;
        if base + 2 >= c.len() {
            return RgbaColor::BLACK;
        }
        RgbaColor::from_rgba8(
            trunc_channel(c[base] as f64 * k),
            trunc_channel(c[base + 1] as f64 * k),
            trunc_channel(c[base + 2] as f64 * k),
            1.0,
        )
    };
    let stops = vec![
        (0.0, at(ray_index(lxy[1].atan2(lxy[0])))),
        (0.5, at(ray_index(lxy[0].atan2(-lxy[1])))),
        (1.0, at(ray_index((-lxy[1]).atan2(-lxy[0])))),
    ];
    PaintStyle::LinearGradient {
        x0: c0[0] + lxy[0] * 50.0,
        y0: c0[1] + lxy[1] * 50.0,
        x1: c0[0] - lxy[0] * 50.0,
        y1: c0[1] - lxy[1] * 50.0,
        stops,
    }
}

/// `inset` is one distance or one per ray (`sliceGradient`).
pub enum SliceInset<'a> {
    Uniform(f64),
    PerRay(&'a [f32]),
}

/// `dark` is one amount or one per ray.
pub enum SliceDark<'a> {
    Uniform(f64),
    PerRay(&'a [f32]),
}

fn parse_rgb_triplet(s: &str) -> [f64; 3] {
    let Some(rest) = s.strip_prefix("rgb(") else {
        return [255.0, 255.0, 255.0];
    };
    let Some(inner) = rest.strip_suffix(')') else {
        return [255.0, 255.0, 255.0];
    };
    let parts: Vec<&str> = inner.split(' ').collect();
    if parts.len() != 3
        || parts
            .iter()
            .any(|p| p.is_empty() || !p.bytes().all(|b| b.is_ascii_digit()))
    {
        return [255.0, 255.0, 255.0];
    }
    [
        parts[0].parse().unwrap_or(255.0),
        parts[1].parse().unwrap_or(255.0),
        parts[2].parse().unwrap_or(255.0),
    ]
}

/// `hazeColor` (3060–3071).
pub(crate) fn haze_color(rays: &Rays, base: &str, rim: f64, vivid: f64) -> String {
    let b = parse_rgb_triplet(&brightest(rays));
    let lin = linear_color(base);
    let e = [srgb(lin[0]), srgb(lin[1]), srgb(lin[2])];
    let mx = e[0].max(e[1]).max(e[2]).max(1.0);
    let k = (0.85_f64).min(0.75 * rim);
    let pale = 0.45 + 0.55 * (1.0_f64).min(vivid);
    let q = |i: usize| js_round(b[i] + (255.0 + ((255.0 * e[i]) / mx - 255.0) * pale - b[i]) * k);
    format!("rgb({} {} {})", q(0) as i64, q(1) as i64, q(2) as i64)
}

/// The front's brightest colour (`brightest`, 3073–3084).
pub(crate) fn brightest(rays: &Rays) -> String {
    let mut best = -1.0;
    let mut r = 0.0f32;
    let mut g = 0.0f32;
    let mut b = 0.0f32;
    for c in &rays.c {
        let mut i = 0;
        while i + 2 < c.len() {
            let l = 0.2126 * c[i] as f64 + 0.7152 * c[i + 1] as f64 + 0.0722 * c[i + 2] as f64;
            if l > best {
                best = l;
                r = c[i];
                g = c[i + 1];
                b = c[i + 2];
            }
            i += 3;
        }
    }
    let q = |v: f32| (255.0f64).min(js_round(v as f64 / 4.0) * 4.0) as i64;
    format!("rgb({} {} {})", q(r), q(g), q(b))
}

/// `stackReach` (3102–3112). `at` receives which slice won, when given.
pub(crate) fn stack_reach(
    rel: &Relief,
    k: u32,
    ex: f64,
    ey: f64,
    dl1: f64,
    vx: f64,
    vy: f64,
    h: f64,
    at: Option<&mut Reach>,
) -> f64 {
    let v0 = vx * rel.cx[0] as f64 + vy * rel.cy[0] as f64;
    let ve = (vx * ex + vy * ey) * dl1;
    let mut best = h;
    let mut kb = 0u32;
    let mut far = false;
    for i in 1..k as usize {
        let r = vx * rel.cx[i] as f64 + vy * rel.cy[i] as f64 + rel.scale[i] as f64 * (h - v0);
        let z = rel.z[i] as f64;
        if r + z * ve > best {
            best = r + z * ve;
            kb = i as u32;
            far = false;
        }
        if r - z * ve > best {
            best = r - z * ve;
            kb = i as u32;
            far = true;
        }
    }
    if let Some(at) = at {
        at.k = kb;
        at.far = far;
    }
    best
}

/// Stretch affine in design units (`haloStretch`, 3115–3122).
pub(crate) fn halo_stretch(
    form: &Form,
    rel: &Relief,
    k: u32,
    ex: f64,
    ey: f64,
    dl1: f64,
) -> [f64; 6] {
    if dl1 <= 1e-4 {
        return [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];
    }
    let rim = rim_of(form);
    let hp = reach(&rim, ex, ey);
    let hn = reach(&rim, -ex, -ey);
    let up = stack_reach(rel, k, ex, ey, dl1, ex, ey, hp, None);
    let un = stack_reach(rel, k, ex, ey, dl1, -ex, -ey, hn, None);
    let s1 = (up + un) / (1.0_f64).max(hp + hn) - 1.0;
    let t = up - (s1 + 1.0) * hp;
    [
        1.0 + s1 * ex * ex,
        s1 * ex * ey,
        s1 * ex * ey,
        1.0 + s1 * ey * ey,
        ex * t,
        ey * t,
    ]
}

/// `haloRays` (3125–3146).
pub(crate) fn halo_rays(
    form: &Form,
    rel: &Relief,
    k: u32,
    ex: f64,
    ey: f64,
    dl1: f64,
    stretch: [f64; 6],
    back_shade: f64,
) -> HaloRays {
    let rim = rim_of(form);
    let mut inset = vec![0.0f32; CONIC_STOPS];
    let mut dark = vec![0.0f32; CONIC_STOPS];
    let s1 = stretch[0] + stretch[3] - 2.0;
    let mut at = Reach { k: 0, far: false };
    for s in 0..CONIC_STOPS {
        let phi = (s as f64 / CONIC_STOPS as f64) * TAU;
        let (uy, ux) = phi.sin_cos();
        let ue = ux * ex + uy * ey;
        let q = s1 / (1.0 + s1);
        let mut vx = ux - q * ue * ex;
        let mut vy = uy - q * ue * ey;
        let vl = vx.hypot(vy);
        vx /= vl;
        vy /= vl;
        stack_reach(
            rel,
            k,
            ex,
            ey,
            dl1,
            vx,
            vy,
            reach(&rim, vx, vy),
            Some(&mut at),
        );
        inset[s] = HALO_INSET.max(rel.inset[at.k as usize] as f64) as f32;
        dark[s] = if at.far {
            (HALO_SHADE * back_shade * (1.0 - (-2.4 * (at.k as f64 / k as f64)).exp())) as f32
        } else {
            0.0
        };
    }
    HaloRays { inset, dark }
}

fn miss_cache_key(stamp: u64, k: u32, q: f64, dxs: f64, dys: f64) -> String {
    format!(
        "{stamp}|{k}|{q}|{}|{}",
        js_round(dxs / MISS_STEP) as i64,
        js_round(dys / MISS_STEP) as i64
    )
}

/// `haloMiss` (3162–3179). `None` while a deferred measure has not run; this port measures when `sync`.
pub(crate) fn halo_miss(
    form: &Form,
    path: &Path,
    rel: &Relief,
    k: u32,
    stretch: [f64; 6],
    dxs: f64,
    dys: f64,
    sync: bool,
) -> Option<[f64; 2]> {
    let stamp = form_stamp(form);
    let key = miss_cache_key(stamp, k, rel.q, dxs, dys);
    {
        let mut map = MISSES.lock().expect("halo misses");
        if let Some(hit) = map.get(&key) {
            if hit.is_some() || !sync {
                return *hit;
            }
        }
        let prefix = format!("{stamp}|");
        let count = map.keys().filter(|k| k.starts_with(&prefix)).count();
        if count > 256 {
            map.retain(|k, _| !k.starts_with(&prefix));
        }
        if !sync {
            map.insert(key.clone(), None);
            drop(map);
            let form = form.clone();
            let path = path.clone();
            let rel = rel.clone();
            let key2 = key;
            idle(move || {
                let r = measure_miss(&form, &path, &rel, k, stretch, dxs, dys);
                if let Ok(mut map) = MISSES.lock() {
                    map.insert(key2, Some(r));
                }
            });
            return None;
        }
    }
    let r = measure_miss(form, path, rel, k, stretch, dxs, dys);
    MISSES.lock().expect("halo misses").insert(key, Some(r));
    Some(r)
}

/// `measureMiss` (3180–3212).
pub(crate) fn measure_miss(
    form: &Form,
    path: &Path,
    rel: &Relief,
    k: u32,
    stretch: [f64; 6],
    dxs: f64,
    dys: f64,
) -> [f64; 2] {
    let mut g = make_canvas(MISS_N);
    let u = SPAN / MISS_N as f64;
    let det = stretch[0] * stretch[3] - stretch[1] * stretch[2];
    let ia = stretch[3] / det;
    let ib = -stretch[1] / det;
    let ic = -stretch[2] / det;
    let id = stretch[0] / det;
    let ie = -(ia * stretch[4] + ic * stretch[5]);
    let jf = -(ib * stretch[4] + id * stretch[5]);
    g.set_fill_css("#fff");
    for ki in 0..k {
        let z0 = if ki == 0 {
            0.0
        } else {
            rel.z[ki as usize] as f64
        };
        let z1 = -z0;
        let nzs = if ki == 0 { 1 } else { 2 };
        for zi in 0..nzs {
            let z = if zi == 0 { z0 } else { z1 };
            g.set_transform(1.0 / u, 0.0, 0.0, 1.0 / u, PAD / u, PAD / u);
            g.transform(ia, ib, ic, id, ie, jf);
            g.translate(z * dxs, z * dys);
            if ki != 0 {
                let sk = rel.scale[ki as usize] as f64;
                g.transform(
                    sk,
                    0.0,
                    0.0,
                    sk,
                    rel.cx[ki as usize] as f64 - sk * rel.cx[0] as f64,
                    rel.cy[ki as usize] as f64 - sk * rel.cy[0] as f64,
                );
            }
            g.fill_path(path);
        }
    }
    // `getImageData` alpha. Opaque white fills stay unpremultiplied in alpha.
    let px = g.pixmap.data();
    let cells = (MISS_N * MISS_N) as usize;
    let mut on = vec![0u8; cells];
    for i in 0..cells {
        on[i] = if px[i * 4 + 3] > 127 { 1 } else { 0 };
    }
    let mut dist = vec![0.0f32; cells];
    edt_2d(&on, 1, MISS_N as usize, &mut dist, None);
    let mut cut = 0.0_f64;
    let mut over = 0.0_f64;
    let mut i = 0;
    for y in 0..MISS_N {
        for x in 0..MISS_N {
            let d = bilerp(
                &form.sd,
                form.n as usize,
                ((x as f64 + 0.5) / MISS_N as f64) * form.n as f64 - 0.5,
                ((y as f64 + 0.5) / MISS_N as f64) * form.n as f64 - 0.5,
            );
            if on[i] == 1 && d < 0.0 {
                cut = cut.max(-d);
            }
            if on[i] == 0 && d > 0.0 {
                over = over.max((dist[i] as f64).sqrt() * u);
            }
            i += 1;
        }
    }
    [cut, over]
}

struct DrawGeom {
    k_count: usize,
    near: f64,
    dxs: f64,
    dys: f64,
    dl1: f64,
    ex: f64,
    ey: f64,
    back_shade: f64,
    ox: f64,
    oy: f64,
    bw: f64,
    bh: f64,
    buffered: bool,
    ta: f64,
    tb: f64,
    tc: f64,
    td: f64,
    ca: f64,
    cb: f64,
    cc: f64,
    cd: f64,
    ce: f64,
    cf: f64,
    sy: f64,
    cy: f64,
    sp: f64,
    m1: f64,
    cp: f64,
    cap_m: [f64; 6],
    halo_s: Option<[f64; 6]>,
    spx: u32,
    front_a: f64,
    fringe_a: f64,
    still: bool,
}

fn place_at(g: &mut Canvas2D, geom: &DrawGeom, z: f64) {
    let e = z * geom.sy - 50.0 * geom.cy;
    let fo = -z * geom.cy * geom.sp - 50.0 * geom.m1 - 50.0 * geom.cp;
    g.set_transform(
        geom.ta,
        geom.tb,
        geom.tc,
        geom.td,
        geom.ca * e + geom.cc * fo + geom.ce - geom.ox,
        geom.cb * e + geom.cd * fo + geom.cf - geom.oy,
    );
}

fn bake_gecko_sprite(st: &mut CapState, path: &Path, k: usize, spx: u32) -> Option<Pixmap> {
    let ins = *st.slice_ins.get(k)?;
    let key = ins.to_bits();
    let ready = st.slice_spr_made.contains(&key)
        && st
            .slice_spr
            .get(&key)
            .is_some_and(|p| p.width() == spx && p.height() == spx);
    if ready {
        return st.slice_spr.get(&key).cloned();
    }
    let style = st.slice_g.get(k)?.clone();
    let mut canvas = Canvas2D::new(spx.max(1), spx.max(1));
    canvas.set_transform(1.0, 0.0, 0.0, 1.0, 0.0, 0.0);
    canvas.clear_rect(0.0, 0.0, spx as f64, spx as f64);
    let kk = spx as f64 / SPAN;
    canvas.set_transform(kk, 0.0, 0.0, kk, PAD * kk, PAD * kk);
    canvas.set_fill_style(style);
    canvas.fill_path(path);
    let pm = into_pixmap(canvas);
    st.slice_spr.insert(key, pm.clone());
    st.slice_spr_made.insert(key);
    Some(pm)
}

fn blit_slice(
    g: &mut Canvas2D,
    geom: &DrawGeom,
    st: &mut CapState,
    sprites: &[Option<Pixmap>; 5],
    path: &Path,
    rel: &Relief,
    fur: bool,
    k: usize,
    z: f64,
    dark: f64,
) {
    place_at(g, geom, z);
    let sk = rel.scale[k] as f64;
    if k > 0 {
        g.transform(
            sk,
            0.0,
            0.0,
            sk,
            rel.cx[k] as f64 - sk * rel.cx[0] as f64,
            rel.cy[k] as f64 - sk * rel.cy[0] as f64,
        );
        let mut used = false;
        if engine_is_gecko() {
            if let Some(pm) = bake_gecko_sprite(st, path, k, geom.spx) {
                g.draw_image(&pm, -PAD, -PAD, SPAN, SPAN);
                used = true;
            }
        }
        if !used {
            if let Some(style) = st.slice_g.get(k).cloned() {
                g.set_fill_style(style);
                g.fill_path(path);
            }
        }
        if dark > 0.01 {
            g.set_global_alpha(dark);
            g.set_fill_css("#000");
            g.fill_path(path);
            g.set_global_alpha(1.0);
        }
        return;
    }
    if fur {
        if let Some(side) = sprites[3].as_ref() {
            g.draw_image(side, -PAD, -PAD, SPAN, SPAN);
        }
    } else if let Some(style) = st.slice_g.first().cloned() {
        g.set_fill_style(style);
        g.fill_path(path);
    }
    if fur && !geom.buffered && geom.fringe_a > 0.01 {
        if let Some(fringed) = sprites[0].as_ref() {
            g.set_global_alpha(geom.fringe_a);
            g.draw_image(fringed, -PAD, -PAD, SPAN, SPAN);
            g.set_global_alpha(1.0);
        }
    }
    if dark > 0.01 {
        if let Some(black) = sprites[1].as_ref() {
            g.set_global_alpha(dark);
            g.draw_image(black, -PAD, -PAD, SPAN, SPAN);
            g.set_global_alpha(1.0);
        }
    }
}

fn composite_halo(
    d: &mut Canvas2D,
    halo_tmp: &mut Option<Pixmap>,
    fringed: &Pixmap,
    band: Option<&Pixmap>,
    veil: f64,
    bw: f64,
    bh: f64,
) {
    if engine_is_webkit() {
        let n = ((bw.max(bh) / 64.0).ceil() * 64.0).max(1.0) as u32;
        let existing = halo_tmp.take();
        let mut canvas = match existing {
            Some(pm) if pm.width() as f64 >= bw && pm.height() as f64 >= bh => canvas_from(pm),
            _ => Canvas2D::new(n, n),
        };
        canvas.set_transform(1.0, 0.0, 0.0, 1.0, 0.0, 0.0);
        canvas.set_blend_mode(BlendMode::SourceOver);
        canvas.set_global_alpha(1.0);
        canvas.clear_rect(0.0, 0.0, bw, bh);
        set_smoothing(&mut canvas, false);
        let m = d.get_transform();
        canvas.set_transform(m[0], m[1], m[2], m[3], m[4], m[5]);
        canvas.draw_image(fringed, -PAD, -PAD, SPAN, SPAN);
        let pm = into_pixmap(canvas);
        d.save();
        d.set_transform(1.0, 0.0, 0.0, 1.0, 0.0, 0.0);
        d.set_blend_mode(BlendMode::DestinationAtop);
        d.draw_image_sub(&pm, 0.0, 0.0, bw, bh, 0.0, 0.0, bw, bh);
        d.restore();
        *halo_tmp = Some(pm);
    } else {
        d.set_blend_mode(BlendMode::DestinationAtop);
        d.draw_image(fringed, -PAD, -PAD, SPAN, SPAN);
    }
    if veil > 0.01 {
        if let Some(band) = band {
            d.set_blend_mode(BlendMode::SourceAtop);
            d.set_global_alpha(veil);
            d.draw_image(band, -PAD, -PAD, SPAN, SPAN);
            d.set_global_alpha(1.0);
        }
    }
    d.set_blend_mode(BlendMode::SourceOver);
}

fn paint_sprite(
    g: &mut Canvas2D,
    i: usize,
    spx: u32,
    k_scale: f64,
    form: &Form,
    rel: &Relief,
    fur: Option<&Fur>,
    soft: Option<&Pixmap>,
    rays: &Rays,
    path: &Path,
    levels: u32,
    ex: f64,
    ey: f64,
    dl1: f64,
    halo_s: Option<[f64; 6]>,
    back_shade: f64,
    base: &str,
    rim: f64,
    vivid: f64,
    lxy: [f64; 2],
) {
    let spx_f = spx as f64;
    g.set_transform(1.0, 0.0, 0.0, 1.0, 0.0, 0.0);
    g.set_blend_mode(BlendMode::SourceOver);
    g.set_global_alpha(1.0);
    g.clear_rect(0.0, 0.0, spx_f, spx_f);
    set_smoothing(g, true);
    let halo = fur.filter(|f| i == 0 && f.mask.is_some());
    if let Some(h) = halo {
        if let Some(haze) = h.haze.as_ref() {
            set_smoothing(g, false);
            g.draw_image(haze, 0.0, 0.0, spx_f, spx_f);
            set_smoothing(g, true);
            g.set_blend_mode(BlendMode::SourceIn);
            g.set_fill_css(&haze_color(rays, base, rim, vivid));
            g.fill_rect(0.0, 0.0, spx_f, spx_f);
            g.set_blend_mode(BlendMode::DestinationOver);
        }
    }
    g.set_transform(k_scale, 0.0, 0.0, k_scale, PAD * k_scale, PAD * k_scale);
    let edge = if halo.is_some() {
        halo_s.map(|s| halo_rays(form, rel, levels, ex, ey, dl1, s, back_shade))
    } else {
        None
    };
    let c0 = [rel.cx[0] as f64, rel.cy[0] as f64];
    let inset = if let Some(edge) = &edge {
        SliceInset::PerRay(&edge.inset)
    } else if fur.is_some() {
        SliceInset::Uniform(HALO_INSET.max(rel.inset[0] as f64))
    } else {
        SliceInset::Uniform(rel.inset[0] as f64)
    };
    let dark = if let Some(edge) = &edge {
        SliceDark::PerRay(&edge.dark)
    } else {
        SliceDark::Uniform(0.0)
    };
    let style = if i == 1 {
        PaintStyle::css("#000")
    } else {
        slice_gradient(g, rays, inset, dark, c0, lxy, engine_is_webkit())
    };
    g.set_fill_style(style);
    if fur.is_some_and(|f| f.mask.is_some()) {
        g.fill_rect(-PAD, -PAD, SPAN, SPAN);
        g.set_transform(1.0, 0.0, 0.0, 1.0, 0.0, 0.0);
        g.set_blend_mode(BlendMode::DestinationIn);
        let mask = if i == 0 {
            fur.and_then(|f| f.mask.as_ref())
        } else {
            soft.or(fur.and_then(|f| f.mask.as_ref()))
        };
        if let Some(mask) = mask {
            g.draw_image(mask, 0.0, 0.0, spx_f, spx_f);
        }
        g.set_blend_mode(BlendMode::SourceOver);
    } else {
        g.fill_path(path);
    }
}

fn paint_stack(
    g: &mut Canvas2D,
    st: &mut CapState,
    sprites: &[Option<Pixmap>; 5],
    form: &Form,
    rel: &Relief,
    path: &Path,
    fur: Option<&Fur>,
    geom: &DrawGeom,
    scratch_idx: usize,
) {
    let ring = ring_of(form);
    let rn = ring.x.len();
    let nf = form.n;
    let texel = SPAN / nf as f64;
    let det_s = (geom.ta * geom.td - geom.tb * geom.tc).abs().sqrt();
    let hide_by = 1.5 / if det_s == 0.0 { 1.0 } else { det_s } + 0.4;
    let to_g = |v: f64| ((v + PAD) / SPAN) * nf as f64 - 0.5;
    let lean_far = (-geom.near * geom.dys).atan2(-geom.near * geom.dxs);
    let lean_near = (geom.near * geom.dys).atan2(geom.near * geom.dxs);
    let fur_on = fur.is_some();
    for k in (1..geom.k_count).rev() {
        let z = -geom.near * rel.z[k] as f64;
        let sk = rel.scale[k] as f64;
        if fur_on {
            let bx = rel.cx[k] as f64 - sk * rel.cx[0] as f64 + z * geom.dxs;
            let by = rel.cy[k] as f64 - sk * rel.cy[0] as f64 + z * geom.dys;
            let mut hidden = bx.is_finite() && by.is_finite();
            let mut j = 0;
            while hidden && j < rn {
                let i = ring_order(rn, lean_far, j);
                let d = bilerp(
                    &form.sd,
                    nf as usize,
                    to_g(sk * ring.x[i] as f64 + bx),
                    to_g(sk * ring.y[i] as f64 + by),
                );
                if d < hide_by + sk * (ring.d[i] as f64 + texel) {
                    hidden = false;
                }
                j += 1;
            }
            if hidden {
                continue;
            }
        }
        let dark = if fur_on {
            0.0
        } else {
            geom.back_shade * (1.0 - (-2.4 * (k as f64 / geom.k_count as f64)).exp())
        };
        blit_slice(g, geom, st, sprites, path, rel, fur_on, k, z, dark);
    }
    if fur_on && geom.dl1 > 1e-3 {
        let hn = reach(&rim_of(form), -geom.ex, -geom.ey);
        let span = 1.3 * rel.top * geom.dl1;
        place_at(g, geom, 0.0);
        let mut stops = Vec::with_capacity(9);
        for s in 0..=8 {
            let t = s as f64 / 8.0;
            let a = fixed3(geom.back_shade * (1.0 - (-2.4 * t).exp()) * t * t * (3.0 - 2.0 * t));
            stops.push((
                t as f32,
                RgbaColor {
                    r: 0.0,
                    g: 0.0,
                    b: 0.0,
                    a,
                },
            ));
        }
        g.set_blend_mode(BlendMode::SourceAtop);
        g.set_fill_style(PaintStyle::LinearGradient {
            x0: -geom.ex * hn,
            y0: -geom.ey * hn,
            x1: -geom.ex * (hn + span),
            y1: -geom.ey * (hn + span),
            stops,
        });
        g.fill_rect(-PAD - 60.0, -PAD - 60.0, SPAN + 120.0, SPAN + 120.0);
        g.set_blend_mode(BlendMode::SourceOver);
    }
    blit_slice(g, geom, st, sprites, path, rel, fur_on, 0, 0.0, 0.0);

    let cdet = geom.cap_m[0] * geom.cap_m[3] - geom.cap_m[1] * geom.cap_m[2];
    let cover_by = FRONT_FADE
        + 1.5
            / if cdet.abs().sqrt() == 0.0 {
                1.0
            } else {
                cdet.abs().sqrt()
            };
    for k in 1..geom.k_count {
        let z = geom.near * rel.z[k] as f64;
        let sk = rel.scale[k] as f64;
        if geom.front_a >= 1.0 && cdet.abs() > 1e-9 {
            let e = z * geom.sy - 50.0 * geom.cy;
            let fo = -z * geom.cy * geom.sp - 50.0 * geom.m1 - 50.0 * geom.cp;
            let ee = geom.ca * e + geom.cc * fo + geom.ce - geom.cap_m[4];
            let ff = geom.cb * e + geom.cd * fo + geom.cf - geom.cap_m[5];
            let ux = rel.cx[k] as f64 - sk * rel.cx[0] as f64;
            let uy = rel.cy[k] as f64 - sk * rel.cy[0] as f64;
            let mut covered = true;
            let mut j = 0;
            while covered && j < rn {
                let i = ring_order(rn, lean_near, j);
                let u = sk * ring.x[i] as f64 + ux;
                let v = sk * ring.y[i] as f64 + uy;
                let x = geom.ta * u + geom.tc * v + ee;
                let y = geom.tb * u + geom.td * v + ff;
                let qx = (geom.cap_m[3] * x - geom.cap_m[2] * y) / cdet;
                let qy = (geom.cap_m[0] * y - geom.cap_m[1] * x) / cdet;
                if bilerp(&form.sd, nf as usize, to_g(qx), to_g(qy))
                    < cover_by + sk * (ring.d[i] as f64 + texel)
                {
                    covered = false;
                }
                j += 1;
            }
            if covered {
                continue;
            }
        }
        blit_slice(g, geom, st, sprites, path, rel, fur_on, k, z, 0.0);
    }

    if geom.front_a > 0.01 {
        if let Some(src) = st.scratch[scratch_idx].as_ref() {
            g.save();
            let m = geom.cap_m;
            g.set_transform(m[0], m[1], m[2], m[3], m[4] - geom.ox, m[5] - geom.oy);
            g.set_global_alpha(geom.front_a);
            g.draw_image(src, -PAD, -PAD, SPAN, SPAN);
            g.restore();
        }
    }

    let Some(halo_s) = geom.halo_s else {
        return;
    };
    if !(geom.buffered && fur.as_ref().is_some_and(|f| f.mask.is_some())) {
        return;
    }
    let Some(fringed) = sprites[0].as_ref() else {
        return;
    };
    let Some(fur) = fur else {
        return;
    };
    let mx = geom.near * geom.dxs;
    let my = geom.near * geom.dys;
    let stale = (mx - st.miss_at[0]).abs() > 1.5 * MISS_STEP
        || (my - st.miss_at[1]).abs() > 1.5 * MISS_STEP;
    let measured = if geom.dl1 > 1e-4 {
        halo_miss(
            form,
            path,
            rel,
            geom.k_count as u32,
            halo_s,
            mx,
            my,
            geom.still || stale,
        )
    } else {
        Some([0.0, 0.0])
    };
    if let Some(m) = measured {
        st.miss = m;
        st.miss_at = [mx, my];
    }
    let cut = st.miss[0];
    let over = st.miss[1];
    let miss = cut.max(over);
    let mut back = smooth(HALO_FIT / 2.0, 2.0 * HALO_FIT, miss);
    let mut gone = smooth(HALO_FIT / 2.0, HALO_FIT, over);
    let quick = (mx - st.turn_at[0]).abs() > MISS_STEP || (my - st.turn_at[1]).abs() > MISS_STEP;
    st.turn_at = [mx, my];
    let settle = !geom.still && !quick;
    if settle {
        if let Some(prev) = st.fade {
            let ease = |a: f64, b: f64| {
                if (b - a).abs() < 0.02 {
                    b
                } else {
                    a + (b - a) * 0.2
                }
            };
            back = ease(prev[0], back);
            gone = ease(prev[1], gone);
        }
    }
    st.fade = Some([back, gone]);
    let lay_to = (1.0 - smooth(0.0, HALO_BLEND, back)) * (1.0 - gone);
    let lay = if !settle {
        lay_to
    } else {
        st.lay + (lay_to - st.lay).clamp(-HALO_EASE, HALO_EASE)
    };
    st.lay = lay;
    let band = sprites[4].as_ref();
    set_smoothing(g, false);
    place_at(g, geom, 0.0);
    g.transform(
        halo_s[0], halo_s[1], halo_s[2], halo_s[3], halo_s[4], halo_s[5],
    );
    let mut lay_canvas = None;
    if lay > 0.0 && lay < 1.0 {
        let n = ((geom.bw.max(geom.bh) / 64.0).ceil() * 64.0).max(1.0) as u32;
        let existing = st.lay_tmp.take();
        let mut c = match existing {
            Some(pm) if pm.width() as f64 >= geom.bw && pm.height() as f64 >= geom.bh => {
                canvas_from(pm)
            }
            _ => Canvas2D::new(n, n),
        };
        c.set_transform(1.0, 0.0, 0.0, 1.0, 0.0, 0.0);
        c.set_global_alpha(1.0);
        c.set_blend_mode(BlendMode::Source);
        c.draw_image_sub(
            &g.pixmap, 0.0, 0.0, geom.bw, geom.bh, 0.0, 0.0, geom.bw, geom.bh,
        );
        set_smoothing(&mut c, false);
        let m = g.get_transform();
        c.set_transform(m[0], m[1], m[2], m[3], m[4], m[5]);
        lay_canvas = Some(c);
    }
    let veil = HALO_VEIL * (1.0_f64).min(geom.dl1 / 0.2);
    if let Some(h) = lay_canvas.as_mut() {
        composite_halo(h, &mut st.halo_tmp, fringed, band, veil, geom.bw, geom.bh);
    }
    if lay >= 1.0 || (lay_canvas.is_none() && lay >= 0.5) {
        composite_halo(g, &mut st.halo_tmp, fringed, band, veil, geom.bw, geom.bh);
    } else {
        if back < 1.0 {
            if let Some(cut_away) = shared_cut(fur) {
                let f = 1.0 + (back * (miss + HALO_FIT)) / 25.0;
                let cx = rel.cx[0] as f64;
                let cy = rel.cy[0] as f64;
                g.save();
                g.transform(f, 0.0, 0.0, f, cx * (1.0 - f), cy * (1.0 - f));
                g.set_blend_mode(BlendMode::DestinationOut);
                g.draw_image(&cut_away, -PAD, -PAD, SPAN, SPAN);
                g.restore();
            }
        }
        if gone < 1.0 {
            g.set_global_alpha(1.0 - gone);
            g.set_blend_mode(BlendMode::DestinationOver);
            g.draw_image(fringed, -PAD, -PAD, SPAN, SPAN);
            g.set_global_alpha(1.0);
        }
        if let Some(h) = lay_canvas.as_ref() {
            g.set_transform(1.0, 0.0, 0.0, 1.0, 0.0, 0.0);
            g.set_blend_mode(BlendMode::DestinationIn);
            g.set_global_alpha(1.0 - lay);
            g.set_fill_css("#000");
            g.fill_rect(0.0, 0.0, geom.bw, geom.bh);
            g.set_blend_mode(BlendMode::Plus);
            g.set_global_alpha(lay);
            g.draw_image_sub(
                &h.pixmap, 0.0, 0.0, geom.bw, geom.bh, 0.0, 0.0, geom.bw, geom.bh,
            );
            g.set_global_alpha(1.0);
        }
    }
    g.set_blend_mode(BlendMode::SourceOver);
    if let Some(c) = lay_canvas {
        st.lay_tmp = Some(into_pixmap(c));
    }
}

fn lay_pile(
    g: &mut Canvas2D,
    fur: &Fur,
    base: &str,
    material: &Material,
    rig: &Rig,
    cfg: &DrawConfig,
    geom: &DrawGeom,
    cap_m: [f64; 6],
    cap_t: [f64; 6],
    face: f64,
) {
    g.set_blend_mode(BlendMode::SourceAtop);
    set_smoothing(g, false);
    let lin = linear_color(base);
    let lum = (0.2126 * lin[0] + 0.7152 * lin[1] + 0.0722 * lin[2]).powf(1.0 / 2.2);
    let vivid = material.vivid.unwrap_or(0.0);
    let gap = gap_film(fur, base, vivid);
    let tip = tip_film(fur, base, vivid);
    let Some(dark) = gap.as_deref().or(fur.dark.as_ref()) else {
        return;
    };
    let Some(tips) = tip.as_deref().or(fur.light.as_ref()) else {
        return;
    };
    let sheen_a = if rig.dev < 160.0 {
        0.0
    } else if face >= 0.88 {
        1.0
    } else if face <= 0.55 {
        0.0
    } else {
        (face - 0.55) / 0.33
    };
    let sheen = if sheen_a > 0.01 {
        sheen_film(fur, base, material)
    } else {
        None
    };
    let side_u = if face >= 0.62 {
        0.0
    } else if face <= 0.36 {
        1.0
    } else {
        (0.62 - face) / 0.26
    };
    let draw_layers = |g: &mut Canvas2D, m: [f64; 6], a: f64, with_sheen: bool, t: [f64; 6]| {
        g.set_transform(m[0], m[1], m[2], m[3], m[4] - geom.ox, m[5] - geom.oy);
        g.set_global_alpha(a);
        g.draw_image(dark, -PAD - FILM_M, -PAD - FILM_M, FILM_SPAN, FILM_SPAN);
        if t != m {
            g.set_transform(t[0], t[1], t[2], t[3], t[4] - geom.ox, t[5] - geom.oy);
        }
        g.set_global_alpha(a * (0.25 + 0.75 * lum));
        g.draw_image(tips, -PAD - FILM_M, -PAD - FILM_M, FILM_SPAN, FILM_SPAN);
        if with_sheen {
            if let Some(sh) = sheen.as_deref() {
                g.set_global_alpha(a * sheen_a);
                g.draw_image(sh, -PAD - FILM_M, -PAD - FILM_M, FILM_SPAN, FILM_SPAN);
            }
        }
        g.set_global_alpha(1.0);
    };
    if side_u < 1.0 {
        draw_layers(g, cap_m, 1.0 - side_u, true, cap_t);
    }
    if side_u > 0.0 {
        let k = rig.dev / 100.0;
        let cxs = geom.ox + geom.bw / 2.0;
        let cys = geom.oy + geom.bh / 2.0;
        let m = [k, 0.0, 0.0, k, cxs - 50.0 * k, cys - 50.0 * k];
        draw_layers(g, m, side_u, false, m);
    }
    g.set_blend_mode(BlendMode::SourceOver);
    let _ = cfg;
}

fn reuse_body(st: &mut CapState, bw: f64, bh: f64) -> (usize, Canvas2D) {
    st.body_idx ^= 1;
    let idx = st.body_idx;
    let w = ((bw / 64.0).ceil() * 64.0).max(1.0) as u32;
    let h = ((bh / 64.0).ceil() * 64.0).max(1.0) as u32;
    let existing = st.body[idx].take();
    let canvas = if let Some(buf) = existing {
        let too_small = (buf.w as f64) < bw || (buf.h as f64) < bh;
        let too_big = (buf.w as f64) > 2.0 * bw + 64.0 || (buf.h as f64) > 2.0 * bh + 64.0;
        if !too_small && !too_big {
            canvas_from(buf.pixmap)
        } else {
            Canvas2D::new(w, h)
        }
    } else {
        Canvas2D::new(w, h)
    };
    (idx, canvas)
}

fn rebuild_sprites(
    st: &mut CapState,
    form: &Form,
    rel: &Relief,
    fur: Option<&Fur>,
    path: &Path,
    levels: u32,
    spx: u32,
    ex: f64,
    ey: f64,
    dl1: f64,
    halo_s: Option<[f64; 6]>,
    back_shade: f64,
    base: &str,
    material: &Material,
    lxy: [f64; 2],
) {
    let k_scale = spx as f64 / SPAN;
    let soft = fur.map(|f| soft_mask(form, f.r));
    let c0 = [rel.cx[0] as f64, rel.cy[0] as f64];
    let rays = rays_of(st.img.as_ref().expect("front image"), form, c0);
    let vivid = material.vivid.unwrap_or(0.0);
    for i in 0..4 {
        if i == 2 {
            continue;
        }
        let existing = st.sprites[i].take();
        let mut canvas = match existing {
            Some(pm) if pm.width() == spx && pm.height() == spx => canvas_from(pm),
            _ => Canvas2D::new(spx.max(1), spx.max(1)),
        };
        paint_sprite(
            &mut canvas,
            i,
            spx,
            k_scale,
            form,
            rel,
            fur,
            soft.as_ref().and_then(|s| s.as_ref()),
            &rays,
            path,
            levels,
            ex,
            ey,
            dl1,
            halo_s,
            back_shade,
            base,
            material.rim,
            vivid,
            lxy,
        );
        st.sprites[i] = Some(into_pixmap(canvas));
    }
    if fur.is_some_and(|f| f.core.is_some()) {
        let existing = st.sprites[4].take();
        let mut band = match existing {
            Some(pm) if pm.width() == spx && pm.height() == spx => canvas_from(pm),
            _ => Canvas2D::new(spx.max(1), spx.max(1)),
        };
        band.set_transform(1.0, 0.0, 0.0, 1.0, 0.0, 0.0);
        band.set_blend_mode(BlendMode::Source);
        if let Some(src) = st.sprites[0].as_ref() {
            band.draw_image_at(src, 0.0, 0.0);
        }
        band.set_blend_mode(BlendMode::DestinationOut);
        if let Some(core) = fur.and_then(|f| f.core.as_ref()) {
            band.draw_image(core, 0.0, 0.0, spx as f64, spx as f64);
        }
        band.set_blend_mode(BlendMode::SourceOver);
        st.sprites[4] = Some(into_pixmap(band));
    }
}

fn front_rig(rig: &Rig) -> Rig {
    let mut rig = *rig;
    rig.cy = 1.0;
    rig.sy = 0.0;
    rig.cp = 1.0;
    rig.sp = 0.0;
    rig.facing = 1.0;
    rig.roll = 0.0;
    rig
}

/// Design point `(x, y)` to device pixels for a slice at depth `z`.
fn slice_affine(rig: &Rig, z: f64) -> [f64; 6] {
    let [ca, cb, cc, cd, ce, cf] = rig.ctm;
    let m1 = rig.sy * rig.sp;
    let e = z * rig.sy - 50.0 * rig.cy;
    let fo = -z * rig.cy * rig.sp - 50.0 * m1 - 50.0 * rig.cp;
    [
        ca * rig.cy + cc * m1,
        cb * rig.cy + cd * m1,
        cc * rig.cp,
        cd * rig.cp,
        ca * e + cc * fo + ce,
        cb * e + cd * fo + cf,
    ]
}

fn mix_stamp(h: &mut u64, bytes: impl AsRef<[u8]>) {
    for &b in bytes.as_ref() {
        *h ^= b as u64;
        *h = h.wrapping_mul(0x100000001b3);
    }
}

fn fast_stamp(
    form: &Form,
    fur: Option<&Fur>,
    base: &str,
    material: &Material,
    rig: &Rig,
    fabric: bool,
) -> u64 {
    let mut h = 0xcbf29ce484222325u64;
    mix_stamp(&mut h, form.id.to_le_bytes());
    mix_stamp(&mut h, form.n.to_le_bytes());
    mix_stamp(&mut h, fur.map(|f| f.cache_key).unwrap_or(0).to_le_bytes());
    mix_stamp(&mut h, base.as_bytes());
    let q = |h: &mut u64, v: f64| mix_stamp(h, ((v * 1000.0).round() as i64).to_le_bytes());
    q(&mut h, material.shadow);
    q(&mut h, material.highlight);
    q(&mut h, material.spread);
    q(&mut h, material.rim);
    q(&mut h, material.vivid.unwrap_or(0.0));
    q(&mut h, material.front.unwrap_or(0.0));
    q(&mut h, material.shine.unwrap_or(0.0));
    q(&mut h, material.sheen.unwrap_or(0.0));
    q(&mut h, material.back_soft.unwrap_or(0.0));
    q(&mut h, rig.lx);
    q(&mut h, rig.ly);
    q(&mut h, rig.half_depth);
    q(&mut h, rig.dev);
    q(&mut h, rig.bx.unwrap_or(f64::NAN));
    q(&mut h, rig.by.unwrap_or(f64::NAN));
    mix_stamp(&mut h, [u8::from(fabric)]);
    h
}

fn opaque_box(card: &Pixmap) -> [i32; 4] {
    let w = card.width() as i32;
    let h = card.height() as i32;
    let data = card.data();
    let mut x0 = w;
    let mut y0 = h;
    let mut x1 = 0;
    let mut y1 = 0;
    for y in 0..h {
        let row = (y as usize) * (w as usize) * 4;
        for x in 0..w {
            if data[row + (x as usize) * 4 + 3] != 0 {
                x0 = x0.min(x);
                y0 = y0.min(y);
                x1 = x1.max(x + 1);
                y1 = y1.max(y + 1);
            }
        }
    }
    if x0 >= x1 || y0 >= y1 {
        [0, 0, 0, 0]
    } else {
        [x0, y0, x1, y1]
    }
}

#[derive(Clone, Copy)]
struct CardMap {
    fx_dx: f32,
    fx_dy: f32,
    fy_dx: f32,
    fy_dy: f32,
    fx0: f32,
    fy0: f32,
}

fn card_map(m: [f64; 6], sw: f32) -> Option<CardMap> {
    let (a, b, c, d, e, f) = (
        m[0] as f32,
        m[1] as f32,
        m[2] as f32,
        m[3] as f32,
        m[4] as f32,
        m[5] as f32,
    );
    let det = a * d - c * b;
    if det.abs() < 1e-12 {
        return None;
    }
    let inv = 1.0 / det;
    let ia = d * inv;
    let ib = -c * inv;
    let ic = -b * inv;
    let id = a * inv;
    let origin = -(PAD as f32 + FILM_M as f32);
    let inv_scale = sw / FILM_SPAN as f32;
    let ox = 0.5 - e;
    let oy = 0.5 - f;
    Some(CardMap {
        fx_dx: ia * inv_scale,
        fx_dy: ib * inv_scale,
        fy_dx: ic * inv_scale,
        fy_dy: id * inv_scale,
        fx0: (ia * ox + ib * oy - origin) * inv_scale - 0.5,
        fy0: (ic * ox + id * oy - origin) * inv_scale - 0.5,
    })
}

fn dest_box(
    m: [f64; 6],
    sw: f64,
    rect: [i32; 4],
    dw: i32,
    dh: i32,
) -> Option<(i32, i32, i32, i32)> {
    if rect[0] >= rect[2] || rect[1] >= rect[3] {
        return None;
    }
    let (a, b, c, d, e, f) = (m[0], m[1], m[2], m[3], m[4], m[5]);
    let origin = -(PAD + FILM_M);
    let scale = FILM_SPAN / sw;
    let mut minx = f64::INFINITY;
    let mut miny = f64::INFINITY;
    let mut maxx = f64::NEG_INFINITY;
    let mut maxy = f64::NEG_INFINITY;
    let corners = [
        (rect[0] - 1, rect[1] - 1),
        (rect[2] + 1, rect[1] - 1),
        (rect[0] - 1, rect[3] + 1),
        (rect[2] + 1, rect[3] + 1),
    ];
    for (px, py) in corners {
        let dx = origin + px as f64 * scale;
        let dy = origin + py as f64 * scale;
        let x = a * dx + c * dy + e;
        let y = b * dx + d * dy + f;
        minx = minx.min(x);
        maxx = maxx.max(x);
        miny = miny.min(y);
        maxy = maxy.max(y);
    }
    let x0 = (minx.floor() as i32).clamp(0, dw);
    let y0 = (miny.floor() as i32).clamp(0, dh);
    let x1 = (maxx.ceil() as i32 + 1).clamp(0, dw);
    let y1 = (maxy.ceil() as i32 + 1).clamp(0, dh);
    if x0 >= x1 || y0 >= y1 {
        None
    } else {
        Some((x0, y0, x1, y1))
    }
}

#[inline(always)]
fn sample_card(src: &[u8], stride: usize, sw: i32, sh: i32, fx: f32, fy: f32) -> [u32; 4] {
    if fx < -1.0 || fy < -1.0 || fx >= sw as f32 || fy >= sh as f32 {
        return [0; 4];
    }
    let sx = fx.floor() as i32;
    let sy = fy.floor() as i32;
    let tx = fx - sx as f32;
    let ty = fy - sy as f32;
    let texel = |ix: i32, iy: i32| -> [u32; 4] {
        if ix < 0 || iy < 0 || ix >= sw || iy >= sh {
            return [0; 4];
        }
        let o = iy as usize * stride + ix as usize * 4;
        [
            src[o] as u32,
            src[o + 1] as u32,
            src[o + 2] as u32,
            src[o + 3] as u32,
        ]
    };
    let c00 = texel(sx, sy);
    let c10 = texel(sx + 1, sy);
    let c01 = texel(sx, sy + 1);
    let c11 = texel(sx + 1, sy + 1);
    let mut px = [0u32; 4];
    for ch in 0..4 {
        let top = c00[ch] as f32 + (c10[ch] as f32 - c00[ch] as f32) * tx;
        let bot = c01[ch] as f32 + (c11[ch] as f32 - c01[ch] as f32) * tx;
        px[ch] = (top + (bot - top) * ty).round() as u32;
    }
    px
}

#[inline(always)]
fn over_premul(dst: &mut [u8], src: [u32; 4]) {
    let sa = src[3].min(255);
    if sa == 0 {
        return;
    }
    if sa == 255 {
        dst[0] = src[0].min(255) as u8;
        dst[1] = src[1].min(255) as u8;
        dst[2] = src[2].min(255) as u8;
        dst[3] = 255;
        return;
    }
    let inv_a = 255 - sa;
    dst[0] = (src[0] + (dst[0] as u32 * inv_a + 127) / 255).min(255) as u8;
    dst[1] = (src[1] + (dst[1] as u32 * inv_a + 127) / 255).min(255) as u8;
    dst[2] = (src[2] + (dst[2] as u32 * inv_a + 127) / 255).min(255) as u8;
    dst[3] = (sa + (dst[3] as u32 * inv_a + 127) / 255).min(255) as u8;
}

/// Premultiplied source-over of the baked card. `back` is the darkened depth rim,
/// drawn under the front. One walk: an opaque front pixel hides the rim.
fn blit_cards(
    dst: &mut Pixmap,
    card: &Pixmap,
    bounds: [i32; 4],
    front: [f64; 6],
    back: Option<([f64; 6], f64)>,
) {
    let dw = dst.width() as i32;
    let dh = dst.height() as i32;
    let sw_i = card.width() as i32;
    let sh_i = card.height() as i32;
    if dw <= 0 || dh <= 0 || sw_i < 1 || sh_i < 1 {
        return;
    }
    let Some(front_m) = card_map(front, sw_i as f32) else {
        return;
    };
    let back_m = back.and_then(|(m, gain)| card_map(m, sw_i as f32).map(|map| (map, gain)));
    let Some((mut x0, mut y0, mut x1, mut y1)) = dest_box(front, sw_i as f64, bounds, dw, dh)
    else {
        return;
    };
    if let Some((m, _)) = back {
        if let Some((bx0, by0, bx1, by1)) = dest_box(m, sw_i as f64, bounds, dw, dh) {
            x0 = x0.min(bx0);
            y0 = y0.min(by0);
            x1 = x1.max(bx1);
            y1 = y1.max(by1);
        }
    }
    let src = card.data();
    let stride = sw_i as usize * 4;
    let gain_u = back_m
        .map(|(_, gain)| (gain.clamp(0.0, 1.0) * 255.0).round() as u32)
        .unwrap_or(255);
    let out = dst.data_mut();
    let dst_stride = dw as usize * 4;
    let mut fx_row = front_m.fx0 + front_m.fx_dy * y0 as f32;
    let mut fy_row = front_m.fy0 + front_m.fy_dy * y0 as f32;
    let (mut bfx_row, mut bfy_row) = match back_m {
        Some((map, _)) => (
            map.fx0 + map.fx_dy * y0 as f32,
            map.fy0 + map.fy_dy * y0 as f32,
        ),
        None => (0.0, 0.0),
    };
    for y in y0..y1 {
        let row = y as usize * dst_stride;
        let mut fx = fx_row + front_m.fx_dx * x0 as f32;
        let mut fy = fy_row + front_m.fy_dx * x0 as f32;
        let mut bfx = bfx_row;
        let mut bfy = bfy_row;
        if back_m.is_some() {
            let map = &back_m.unwrap().0;
            bfx += map.fx_dx * x0 as f32;
            bfy += map.fy_dx * x0 as f32;
        }
        for x in x0..x1 {
            let px = sample_card(src, stride, sw_i, sh_i, fx, fy);
            fx += front_m.fx_dx;
            fy += front_m.fy_dx;
            let o = row + x as usize * 4;
            if px[3] >= 255 {
                out[o] = px[0].min(255) as u8;
                out[o + 1] = px[1].min(255) as u8;
                out[o + 2] = px[2].min(255) as u8;
                out[o + 3] = 255;
                if back_m.is_some() {
                    let map = &back_m.unwrap().0;
                    bfx += map.fx_dx;
                    bfy += map.fy_dx;
                }
                continue;
            }
            if let Some((map, _)) = back_m {
                let mut under = sample_card(src, stride, sw_i, sh_i, bfx, bfy);
                bfx += map.fx_dx;
                bfy += map.fy_dx;
                if gain_u != 255 {
                    under[0] = under[0] * gain_u / 255;
                    under[1] = under[1] * gain_u / 255;
                    under[2] = under[2] * gain_u / 255;
                }
                if under[3] != 0 {
                    over_premul(&mut out[o..o + 4], under);
                }
            }
            if px[3] != 0 {
                over_premul(&mut out[o..o + 4], px);
            }
        }
        fx_row += front_m.fx_dy;
        fy_row += front_m.fy_dy;
        if let Some((map, _)) = back_m {
            bfx_row += map.fx_dy;
            bfy_row += map.fy_dy;
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn paint_fast(
    ctx: &mut Canvas2D,
    cfg: &DrawConfig,
    rig: &Rig,
    base: &str,
    union_path: Option<&Path>,
    material: &Material,
    form: &Arc<Form>,
    outline_key: &str,
    fur: Option<Arc<Fur>>,
    n: u32,
    cap_px: f64,
) -> bool {
    let fabric = cfg.shading == BotAvatarShading::Fabric;
    let stamp = fast_stamp(form, fur.as_deref(), base, material, rig, fabric);
    let state_key = format!("fast|{base}|{}|{}", form.id, u8::from(fabric));
    let mut states = state_for(ctx, &state_key);
    if states.get().card_stamp != stamp || states.get().card.is_none() {
        // The slice painter keeps its own states, so it bakes with this lock released.
        drop(states);
        let Some(card) = bake_card(
            cfg,
            rig,
            base,
            union_path,
            material,
            form,
            outline_key,
            fur,
            n,
            cap_px,
        ) else {
            return false;
        };
        states = state_for(ctx, &state_key);
        let st = states.get();
        st.card_box = opaque_box(&card);
        st.card = Some(card);
        st.card_stamp = stamp;
    }
    let st = states.get();
    let front = slice_affine(rig, 0.0);
    let sign = if rig.facing < 0.0 { -1.0 } else { 1.0 };
    let back = slice_affine(rig, -sign * rig.half_depth);
    let bounds = st.card_box;
    let card = st.card.as_ref().expect("card");
    let shift = (back[4] - front[4]).hypot(back[5] - front[5]);
    let rim = if shift >= 0.4 {
        Some((back, 0.62))
    } else {
        None
    };
    blit_cards(&mut ctx.pixmap, card, bounds, front, rim);
    true
}

/// The front view as the slice painter draws it, on a card laid out like the fur films
/// (`FILM_SPAN` across, the body at `PAD + FILM_M`), so each pose is one affine blit and
/// keeps the slices' crisp outline and lit side band.
#[allow(clippy::too_many_arguments)]
fn bake_card(
    cfg: &DrawConfig,
    rig: &Rig,
    base: &str,
    union_path: Option<&Path>,
    material: &Material,
    form: &Arc<Form>,
    outline_key: &str,
    fur: Option<Arc<Fur>>,
    n: u32,
    cap_px: f64,
) -> Option<Pixmap> {
    let side = ((cap_px * FILM_SPAN / SPAN).ceil() as u32).clamp(1, 640);
    let s = side as f64 / FILM_SPAN;
    let center = (PAD + FILM_M + 50.0) * s;
    let mut g = Canvas2D::new(side, side);
    let mut front = front_rig(rig);
    front.ctm = [s, 0.0, 0.0, s, center, center];
    front.still = true;
    let drawn = paint_slices(
        &mut g,
        cfg,
        &front,
        base,
        union_path,
        material,
        form.clone(),
        outline_key.to_string(),
        fur,
        n,
        cap_px,
    );
    forget_states(&g);
    drawn.then(|| into_pixmap(g))
}

/// Draw the plastic or fabric body as a relief (`drawPlasticCap`, 3233–3962).
///
/// `base` is `pal.base`. Returns false when the form is not built yet.
/// `union` is a Rust keyword, so the outline union is `union_path`.
///
/// `img` stores straight RGBA (TS `ImageData`). Scratch, sprites and masks are
/// premultiplied pixmaps. Conic fills use the linear ramp: [`PaintStyle`] has
/// no conic gradient.
pub fn draw_plastic_cap(
    ctx: &mut Canvas2D,
    cfg: &DrawConfig,
    rig: &Rig,
    base: &str,
    union_path: Option<&Path>,
    material: &Material,
) -> bool {
    let n = tier_for(rig.dev);
    let outline_key = match &cfg.type_key {
        Some(k) => k.clone(),
        None => path_id(cfg.path.as_ref()),
    };
    let Some(form) = form_for(
        &outline_key,
        cfg.path.as_ref(),
        n,
        rig.half_depth,
        rig.still,
    ) else {
        return false;
    };
    let fabric = cfg.shading == BotAvatarShading::Fabric;
    let cap_px = ((SPAN * rig.dev) / 100.0).ceil();
    let (bx, by) = match (rig.bx, rig.by) {
        (Some(bx), Some(by)) => (bx, by),
        _ => {
            let p = back_from_key(rig.lx, rig.ly);
            (p[0], p[1])
        }
    };
    let lights = FurLights {
        bx,
        by,
        front: material.front.unwrap_or(KEY_EL),
    };
    let style = cfg.fur.unwrap_or(FUR_STOCK);
    let fur = if fabric {
        fur_ready(
            &form,
            &outline_key,
            rig.half_depth,
            cap_px,
            rig.still && !cfg.fur_on_idle,
            rig.lx,
            rig.ly,
            &style,
            &lights,
        )
    } else {
        None
    };
    if fabric && fur.is_none() {
        return false;
    }
    // Sidebar size: one baked card, posed with an affine blit. Larger sizes keep the slice painter.
    if rig.dev <= 224.0 {
        return paint_fast(
            ctx,
            cfg,
            rig,
            base,
            union_path,
            material,
            &form,
            &outline_key,
            fur,
            n,
            cap_px,
        );
    }
    paint_slices(
        ctx,
        cfg,
        rig,
        base,
        union_path,
        material,
        form,
        outline_key,
        fur,
        n,
        cap_px,
    )
}

/// The relief as a stack of filled outlines from back to front under the shaded front view.
#[allow(clippy::too_many_arguments)]
fn paint_slices(
    ctx: &mut Canvas2D,
    cfg: &DrawConfig,
    rig: &Rig,
    base: &str,
    union_path: Option<&Path>,
    material: &Material,
    form: Arc<Form>,
    outline_key: String,
    fur: Option<Arc<Fur>>,
    n: u32,
    cap_px: f64,
) -> bool {
    let fabric = cfg.shading == BotAvatarShading::Fabric;
    let state_key = if fabric {
        format!("{outline_key}|fabric")
    } else {
        outline_key
    };
    let mut states = state_for(ctx, &state_key);
    let st = states.get();
    let frame = cap_frame(rig);
    let lxy = {
        let l = frame.l[0].hypot(frame.l[1]);
        if l < 0.05 {
            [0.0, -1.0]
        } else {
            [frame.l[0] / l, frame.l[1] / l]
        }
    };
    let vivid = material.vivid.unwrap_or(0.0);
    let front = material.front.unwrap_or(KEY_EL);
    let sheen = material.sheen.unwrap_or(0.0);
    let back_soft = material.back_soft.unwrap_or(BACK_SOFT);
    let rebuild = moved(frame.l, st.l)
        || moved(frame.v, st.v)
        || rig.lx != st.lx
        || rig.ly != st.ly
        || base != st.base
        || material.shadow != st.shadow
        || material.highlight != st.highlight
        || material.spread != st.spread
        || material.rim != st.rim
        || vivid != st.vivid
        || front != st.front
        || sheen != st.sheen
        || back_soft != st.back_soft
        || (fabric && moved(frame.k, st.k));
    if rebuild {
        if st.version > 0 {
            let shown = st.mc_mix.clone();
            st.mc_prev.copy_from_slice(&shown);
        }
        let lin = linear_color(base);
        if fabric {
            build_fabric_matcap(&mut st.mc, lin, &frame, material);
        } else {
            build_matcap(&mut st.mc, lin, &frame, material);
        }
        if st.version == 0 || rig.still {
            let built = st.mc.clone();
            st.mc_mix.copy_from_slice(&built);
            st.blend_t = 1.0;
        } else {
            st.blend_frames = st.since_build.clamp(1, 10);
            st.blend_t = 0.0;
        }
        st.since_build = 0;
        st.mix_version = st.mix_version.wrapping_add(1);
        st.l = Some(frame.l);
        st.v = Some(frame.v);
        st.lx = rig.lx;
        st.ly = rig.ly;
        st.base = base.to_string();
        st.shadow = material.shadow;
        st.highlight = material.highlight;
        st.spread = material.spread;
        st.rim = material.rim;
        st.vivid = vivid;
        st.front = front;
        st.sheen = sheen;
        st.back_soft = back_soft;
        st.k = Some(frame.k);
        st.version = st.version.wrapping_add(1);
    }
    st.since_build = st.since_build.wrapping_add(1);
    if st.blend_t < 1.0 {
        st.blend_t = (1.0_f64).min(st.blend_t + 1.0 / st.blend_frames as f64);
        let e = if st.blend_t >= 1.0 {
            1.0
        } else {
            let t = st.blend_t;
            t * t * (3.0 - 2.0 * t)
        };
        let len = st.mc_mix.len().min(st.mc.len()).min(st.mc_prev.len());
        for i in 0..len {
            let a = st.mc_prev[i] as f64;
            let b = st.mc[i] as f64;
            st.mc_mix[i] = (a + (b - a) * e) as f32;
        }
        st.mix_version = st.mix_version.wrapping_add(1);
    }
    let ao_k = if fur.is_some() {
        (1.2_f64).min(0.3 + 0.8 * material.shadow)
    } else {
        (1.3_f64).min(1.2 * material.shadow)
    };
    if ao_k != st.ao_k {
        for a in 0..256 {
            st.ao_mul[a] = (0.0_f64).max(1.0 - ao_k * (1.0 - a as f64 / 255.0)) as f32;
        }
        st.ao_k = ao_k;
    }
    let img_ok = st
        .img
        .as_ref()
        .is_some_and(|p| p.width() == n && p.height() == n);
    if !img_ok {
        st.img = Pixmap::new(n.max(1), n.max(1));
        st.n = n;
        st.img_version = -1;
    }
    let form_changed = st
        .img_form
        .as_ref()
        .map(|f| !form_same(f, &form))
        .unwrap_or(true);
    if st.img_version != st.mix_version as i32 || st.img_ao_k != ao_k || form_changed {
        let lift = if fur.is_some() {
            Some(edge_lift(&form))
        } else {
            None
        };
        let mc = mem::take(&mut st.mc_mix);
        let ao = mem::take(&mut st.ao_mul);
        if let Some(img) = st.img.as_mut() {
            shade_texels(&form, &mc, img.data_mut(), &ao, lift.as_ref());
        }
        st.mc_mix = mc;
        st.ao_mul = ao;
        st.img_version = st.mix_version as i32;
        st.img_ao_k = ao_k;
        if form_changed {
            st.img_form = Some(form.clone());
        }
        st.scratch_stale = true;
    }
    if st.scratch_n != n {
        st.scratch = [None, None];
        st.scratch_n = n;
        st.scratch_stale = true;
    }
    if st.scratch_stale {
        let Some(bytes) = st.img.as_ref().map(|p| p.data().to_vec()) else {
            return false;
        };
        st.scratch_idx ^= 1;
        let idx = st.scratch_idx;
        let existing = st.scratch[idx].take();
        let mut scratch = match existing {
            Some(pm) if pm.width() == n && pm.height() == n => canvas_from(pm),
            _ => Canvas2D::new(n.max(1), n.max(1)),
        };
        upload_straight_rgba(&mut scratch.pixmap, &bytes);
        st.scratch[idx] = Some(into_pixmap(scratch));
        st.scratch_stale = false;
    }
    let scratch_idx = st.scratch_idx;

    let k_count: usize = if rig.dev <= 100.0 {
        14
    } else if rig.dev <= 224.0 {
        20
    } else {
        26
    };
    let rel = relief_for(&form, k_count as u32, rig.round);
    let [ca, cb, cc, cd, ce, cf] = rig.ctm;
    let m1 = rig.sy * rig.sp;
    let ta = ca * rig.cy + cc * m1;
    let tb = cb * rig.cy + cd * m1;
    let tc = cc * rig.cp;
    let td = cd * rig.cp;
    set_smoothing(ctx, true);
    let near = if rig.facing >= 0.0 { 1.0 } else { -1.0 };
    let dxs = rig.sy / rig.cy;
    let dys = -rig.sp / (rig.cy * rig.cp);
    let dl1 = dxs.hypot(dys);
    let mut lead = 50.0_f64;
    for k in 1..k_count {
        lead = lead.max(50.0 * rel.scale[k] as f64 + rel.z[k] as f64 * dl1);
    }
    let ex = if dl1 > 1e-6 { (near * dxs) / dl1 } else { 1.0 };
    let ey = if dl1 > 1e-6 { (near * dys) / dl1 } else { 0.0 };
    let back_shade = (0.7_f64).min(0.55 * (1.6_f64).min(material.shadow / 0.35));
    let halo_s = if fur.is_some() {
        Some(halo_stretch(&form, &rel, k_count as u32, ex, ey, dl1))
    } else {
        None
    };
    let spx = cap_px as u32;
    if st.sprite_px != cap_px {
        st.sprites = [None, None, None, None, None];
        st.sprite_px = cap_px;
        st.sprite_version = -1;
    }
    let fur_changed = !fur_same(&st.sprite_fur, &fur);
    if st.sprite_version != st.version as i32 || fur_changed {
        rebuild_sprites(
            st,
            &form,
            &rel,
            fur.as_deref(),
            cfg.path.as_ref(),
            k_count as u32,
            spx.max(1),
            ex,
            ey,
            dl1,
            halo_s,
            back_shade,
            base,
            material,
            lxy,
        );
        st.sprite_version = st.version as i32;
        if fur_changed {
            st.sprite_fur = fur.clone();
        }
    }
    let stretch = (lead + 50.0) / 100.0;
    let shift = (lead - 50.0) / 2.0;
    let a_m = 1.0 + (stretch - 1.0) * ex * ex;
    let b_m = (stretch - 1.0) * ex * ey;
    let d_m = 1.0 + (stretch - 1.0) * ey * ey;
    let cap_m = mul_affine(
        mul_affine(rig.ctm, [rig.cy, m1, 0.0, rig.cp, 0.0, 0.0]),
        [
            a_m,
            b_m,
            b_m,
            d_m,
            shift * ex - 50.0 * a_m - 50.0 * b_m,
            shift * ey - 50.0 * b_m - 50.0 * d_m,
        ],
    );
    let use_film = fur
        .as_ref()
        .is_some_and(|f| f.dark.is_some() && f.light.is_some());
    let mut ox = 0.0;
    let mut oy = 0.0;
    let mut bw = 0.0;
    let mut bh = 0.0;
    let mut body = None;
    if use_film {
        let mut x0 = f64::INFINITY;
        let mut y0 = f64::INFINITY;
        let mut x1 = f64::NEG_INFINITY;
        let mut y1 = f64::NEG_INFINITY;
        for z in [-rel.top, rel.top] {
            let e = z * rig.sy - 50.0 * rig.cy;
            let fo = -z * rig.cy * rig.sp - 50.0 * m1 - 50.0 * rig.cp;
            let ee = ca * e + cc * fo + ce;
            let ff = cb * e + cd * fo + cf;
            for x in [-PAD, 100.0 + PAD] {
                for y in [-PAD, 100.0 + PAD] {
                    let xp = ta * x + tc * y + ee;
                    let yp = tb * x + td * y + ff;
                    x0 = x0.min(xp);
                    x1 = x1.max(xp);
                    y0 = y0.min(yp);
                    y1 = y1.max(yp);
                }
            }
        }
        if x0.is_finite() && y0.is_finite() && x1.is_finite() && y1.is_finite() {
            ox = x0.floor() - 1.0;
            oy = y0.floor() - 1.0;
            bw = x1.ceil() + 1.0 - ox;
            bh = y1.ceil() + 1.0 - oy;
            if bw > 0.0 && bh > 0.0 {
                body = Some(reuse_body(st, bw, bh));
            } else {
                ox = 0.0;
                oy = 0.0;
            }
        }
    }
    let face = rig.facing.abs();
    let front_a = if face >= 0.72 {
        1.0
    } else if face <= 0.34 {
        0.0
    } else {
        ((face - 0.34) / 0.38).powf(1.5)
    };
    let fringe_a = if face >= 0.8 {
        1.0
    } else if face <= 0.45 {
        0.0
    } else {
        (face - 0.45) / 0.35
    };
    let slice_key = format!("{}|{k_count}|{}", st.version, rel.q);
    if st.slice_key != slice_key {
        let c0 = [rel.cx[0] as f64, rel.cy[0] as f64];
        let rays = rays_of(st.img.as_ref().expect("front image"), &form, c0);
        st.slice_g.clear();
        st.slice_ins.clear();
        for k in 0..k_count {
            let inset = if fur.is_some() {
                HALO_INSET.max(rel.inset[k] as f64)
            } else {
                rel.inset[k] as f64
            };
            st.slice_ins.push(inset);
            st.slice_g.push(slice_gradient(
                ctx,
                &rays,
                SliceInset::Uniform(inset),
                SliceDark::Uniform(0.0),
                c0,
                lxy,
                engine_is_webkit(),
            ));
        }
        st.slice_key = slice_key;
    }
    let spr_key = format!("{}|{spx}", st.slice_key);
    if engine_is_gecko() && st.slice_spr_key != spr_key {
        st.slice_spr_key = spr_key;
        st.slice_spr_made.clear();
        if st.slice_spr.len() > 32 {
            st.slice_spr.clear();
        }
    }
    let geom = DrawGeom {
        k_count,
        near,
        dxs,
        dys,
        dl1,
        ex,
        ey,
        back_shade,
        ox,
        oy,
        bw,
        bh,
        buffered: body.is_some(),
        ta,
        tb,
        tc,
        td,
        ca,
        cb,
        cc,
        cd,
        ce,
        cf,
        sy: rig.sy,
        cy: rig.cy,
        sp: rig.sp,
        m1,
        cp: rig.cp,
        cap_m,
        halo_s,
        spx: spx.max(1),
        front_a,
        fringe_a,
        still: rig.still,
    };
    let sprites = mem::replace(&mut st.sprites, [None, None, None, None, None]);
    if let Some((idx, mut body_canvas)) = body {
        body_canvas.set_transform(1.0, 0.0, 0.0, 1.0, 0.0, 0.0);
        body_canvas.set_blend_mode(BlendMode::SourceOver);
        body_canvas.set_global_alpha(1.0);
        body_canvas.clear_rect(0.0, 0.0, bw, bh);
        set_smoothing(&mut body_canvas, true);
        paint_stack(
            &mut body_canvas,
            st,
            &sprites,
            &form,
            &rel,
            cfg.path.as_ref(),
            fur.as_deref(),
            &geom,
            scratch_idx,
        );
        if use_film {
            if let Some(f) = fur.as_ref() {
                let length = cfg.fur.unwrap_or(FUR_STOCK).length;
                let lift = near * PILE_H * length;
                let cap_t = mul_affine(
                    mul_affine(
                        mul_affine(
                            rig.ctm,
                            [1.0, 0.0, 0.0, 1.0, lift * rig.sy, -lift * rig.cy * rig.sp],
                        ),
                        [rig.cy, m1, 0.0, rig.cp, 0.0, 0.0],
                    ),
                    [
                        a_m,
                        b_m,
                        b_m,
                        d_m,
                        shift * ex - 50.0 * a_m - 50.0 * b_m,
                        shift * ey - 50.0 * b_m - 50.0 * d_m,
                    ],
                );
                lay_pile(
                    &mut body_canvas,
                    f,
                    base,
                    material,
                    rig,
                    cfg,
                    &geom,
                    cap_m,
                    cap_t,
                    face,
                );
            }
        }
        ctx.save();
        ctx.set_transform(1.0, 0.0, 0.0, 1.0, 0.0, 0.0);
        ctx.set_image_smoothing(false);
        ctx.draw_image_sub(&body_canvas.pixmap, 0.0, 0.0, bw, bh, ox, oy, bw, bh);
        ctx.restore();
        let w = body_canvas.width();
        let h = body_canvas.height();
        st.body[idx] = Some(BodyBuf {
            pixmap: into_pixmap(body_canvas),
            w,
            h,
        });
    } else {
        paint_stack(
            ctx,
            st,
            &sprites,
            &form,
            &rel,
            cfg.path.as_ref(),
            fur.as_deref(),
            &geom,
            scratch_idx,
        );
    }
    st.sprites = sprites;

    ctx.set_transform(ca, cb, cc, cd, ce, cf);
    let hair_a = if face >= 0.95 {
        1.0
    } else if face <= 0.8 {
        0.0
    } else {
        (face - 0.8) / 0.15
    };
    if fur.is_none()
        && rig.dev >= 256.0
        && material.rim > 0.0
        && material.highlight > 0.0
        && hair_a > 0.01
    {
        ctx.save();
        if let Some(union_path) = union_path {
            ctx.clip_path(union_path, FillRule::Winding);
        } else {
            ctx.set_blend_mode(BlendMode::SourceAtop);
        }
        ctx.set_transform(cap_m[0], cap_m[1], cap_m[2], cap_m[3], cap_m[4], cap_m[5]);
        let al = hair_a * (0.5_f64).min(0.3 * material.rim * (1.4_f64).min(material.highlight));
        let col = |a: f64| RgbaColor {
            r: 235.0 / 255.0,
            g: 244.0 / 255.0,
            b: 1.0,
            a: fixed3(a),
        };
        ctx.set_stroke_style(PaintStyle::LinearGradient {
            x0: 50.0 + lxy[0] * 50.0,
            y0: 50.0 + lxy[1] * 50.0,
            x1: 50.0 - lxy[0] * 50.0,
            y1: 50.0 - lxy[1] * 50.0,
            stops: vec![(0.0, col(al)), (0.45, col(0.35 * al)), (0.75, col(0.0))],
        });
        ctx.set_line_join(LineJoin::Round);
        ctx.set_line_width(1.3);
        ctx.stroke_path(cfg.path.as_ref());
        ctx.restore();
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::BotAvatarType;

    fn square_form() -> Form {
        let n = 2u32;
        let n2 = (n * n) as usize;
        Form {
            id: 0,
            n,
            i00: vec![0; n2],
            wx: vec![0; n2],
            wy: vec![0; n2],
            ao: vec![255; n2],
            sd: vec![1.0; n2],
            h: vec![1.0; n2],
        }
    }

    fn relief_two() -> Relief {
        Relief {
            top: 10.0,
            z: vec![0.0, 10.0],
            scale: vec![1.0, 1.0],
            cx: vec![50.0, 50.0],
            cy: vec![50.0, 50.0],
            inset: vec![1.2, 4.0],
            q: 1.0,
        }
    }

    #[test]
    fn mul_affine_matches_canvas() {
        let a = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0];
        let b = [0.5, -1.0, 2.0, 0.25, 3.0, -4.0];
        assert_eq!(mul_affine(a, b), crate::canvas::mul_affine(a, b));
    }

    #[test]
    fn constants_match_plastic_ts() {
        assert!((BIN - 1.0 / 48.0).abs() < 1e-15);
        assert_eq!(MISS_N, 96);
        assert!((MISS_STEP - 0.1).abs() < 1e-15);
        assert!(!WEBKIT);
        assert!(!GECKO);
    }

    #[test]
    fn ring_order_walks_both_ways_from_the_nearest_point() {
        // start = round(((0 + π) / 2π) * 8) = 4
        assert_eq!(ring_order(8, 0.0, 0), 4);
        assert_eq!(ring_order(8, 0.0, 1), 5);
        assert_eq!(ring_order(8, 0.0, 2), 3);
        assert_eq!(ring_order(8, 0.0, 10), 7);
        assert_eq!(ring_order(0, 0.0, 0), 0);
    }

    #[test]
    fn reach_is_the_max_support_of_the_rim() {
        let rim = [10.0f32, 0.0, 1.0, 0.0, 10.0, 2.0, -5.0, 0.0, 0.5];
        assert!((reach(&rim, 1.0, 0.0) - 11.0).abs() < 1e-6);
        assert!((reach(&rim, 0.0, 1.0) - 12.0).abs() < 1e-6);
        assert_eq!(reach(&[], 1.0, 0.0), -INF);
    }

    #[test]
    fn moved_uses_the_bin_threshold() {
        assert!(moved([0.0, 0.0, 0.0], None));
        assert!(!moved([0.0, 0.0, 0.0], Some([0.0, 0.0, 0.0])));
        assert!(moved([BIN, 0.0, 0.0], Some([0.0, 0.0, 0.0])));
        assert!(!moved([BIN * 0.5, 0.0, 0.0], Some([0.0, 0.0, 0.0])));
        assert!(moved([0.0, 0.0, -BIN], Some([0.0, 0.0, 0.0])));
    }

    #[test]
    fn square_rim_reach_and_ring_order() {
        let form = square_form();
        let rim = rim_of(&form);
        // N = 2, u = 53. Points (23.5, 23.5, 1) … (76.5, 76.5, 1).
        assert_eq!(rim.len(), 12);
        assert!((reach(&rim, 1.0, 0.0) - 77.5).abs() < 1e-4);
        assert!((reach(&rim, -1.0, 0.0) - (-22.5)).abs() < 1e-4);
        let ring = ring_of(&form);
        let again = ring_of(&form);
        assert_eq!(ring.x, again.x);
        assert_eq!(ring.x.len(), 4);
        let xs: Vec<f64> = ring.x.iter().map(|v| *v as f64).collect();
        for (got, exp) in xs.iter().zip([23.5, 76.5, 76.5, 23.5]) {
            assert!((got - exp).abs() < 1e-3, "{got} != {exp}");
        }
        let stamp = form_stamp(&form);
        assert!(RIMS.lock().unwrap().contains_key(&stamp));
        let before = RIMS.lock().unwrap().len();
        let _ = rim_of(&form);
        assert_eq!(RIMS.lock().unwrap().len(), before);
    }

    #[test]
    fn stack_reach_and_halo_stretch_match_hand_values() {
        let form = square_form();
        let rel = relief_two();
        let mut at = Reach { k: 0, far: true };
        let up = stack_reach(&rel, 2, 1.0, 0.0, 0.25, 1.0, 0.0, 77.5, Some(&mut at));
        assert!((up - 80.0).abs() < 1e-9);
        assert_eq!(at.k, 1);
        assert!(!at.far);
        let un = stack_reach(&rel, 2, 1.0, 0.0, 0.25, -1.0, 0.0, -22.5, Some(&mut at));
        assert!((un - (-20.0)).abs() < 1e-9);
        assert!(at.far);
        assert_eq!(at.k, 1);
        let m = halo_stretch(&form, &rel, 2, 1.0, 0.0, 1e-8);
        assert_eq!(m, [1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);
        let m = halo_stretch(&form, &rel, 2, 1.0, 0.0, 0.25);
        assert!((m[0] - 60.0 / 55.0).abs() < 1e-9);
        assert!(m[1].abs() < 1e-12 && m[2].abs() < 1e-12 && m[5].abs() < 1e-12);
        assert!((m[3] - 1.0).abs() < 1e-12);
        assert!((m[4] - (80.0 - (60.0 / 55.0) * 77.5)).abs() < 1e-9);
        let rays = halo_rays(&form, &rel, 2, 1.0, 0.0, 0.25, m, 0.4);
        assert_eq!(rays.inset.len(), CONIC_STOPS);
        assert!((rays.inset[0] - 4.0).abs() < 1e-5);
        assert_eq!(rays.dark[0], 0.0);
        let expect = 0.5_f64 * 0.4 * (1.0 - (-2.4_f64 * 0.5).exp());
        assert!((rays.dark[12] as f64 - expect).abs() < 1e-4);
        assert!((rays.inset[12] - 4.0).abs() < 1e-5);
    }

    #[test]
    fn brightest_quantizes_in_steps_of_four() {
        let rays = Rays {
            d: vec![vec![1.0]],
            c: vec![vec![10.0, 20.0, 30.0], vec![101.0, 202.0, 7.0]],
        };
        assert_eq!(brightest(&rays), "rgb(100 204 8)");
        assert_eq!(
            brightest(&Rays {
                d: vec![],
                c: vec![]
            }),
            "rgb(0 0 0)"
        );
    }

    #[test]
    fn slice_gradient_darkens_the_last_sample_still_inside() {
        let d = vec![5.0f32, 3.0, 1.0];
        let c = vec![10.0f32, 20.0, 30.0, 40.0, 50.0, 60.0, 70.0, 80.0, 90.0];
        let rays = Rays {
            d: (0..CONIC_STOPS).map(|_| d.clone()).collect(),
            c: (0..CONIC_STOPS).map(|_| c.clone()).collect(),
        };
        let mut ctx = Canvas2D::new(4, 4);
        let style = slice_gradient(
            &mut ctx,
            &rays,
            SliceInset::Uniform(2.5),
            SliceDark::Uniform(0.25),
            [10.0, 20.0],
            [1.0, 0.0],
            false,
        );
        match style {
            PaintStyle::LinearGradient {
                x0,
                y0,
                x1,
                y1,
                stops,
            } => {
                assert!((x0 - 60.0).abs() < 1e-9 && (y0 - 20.0).abs() < 1e-9);
                assert!((x1 - (-40.0)).abs() < 1e-9 && (y1 - 20.0).abs() < 1e-9);
                assert_eq!(stops.len(), 3);
                for (_, col) in &stops {
                    assert!((col.r - 30.0 / 255.0).abs() < 1e-5);
                    assert!((col.g - 37.0 / 255.0).abs() < 1e-5);
                    assert!((col.b - 45.0 / 255.0).abs() < 1e-5);
                }
            }
            other => panic!("expected a linear ramp, got {other:?}"),
        }
    }

    #[test]
    fn state_for_caches_by_canvas_and_outline() {
        let ctx = Canvas2D::new(8, 8);
        {
            let mut g = state_for(&ctx, "one");
            g.get().version = 3;
        }
        {
            let mut g = state_for(&ctx, "two");
            assert_eq!(g.get().version, 0);
            g.get().lay = 0.25;
        }
        {
            let mut g = state_for(&ctx, "one");
            assert_eq!(g.get().version, 3);
            assert!((g.get().lay - 1.0).abs() < 1e-12);
        }
        {
            let mut g = state_for(&ctx, "two");
            assert!((g.get().lay - 0.25).abs() < 1e-12);
        }
    }

    #[test]
    fn pure_cap_math_does_not_panic_for_any_avatar_type() {
        let form = square_form();
        let rel = relief_two();
        let ctx = Canvas2D::new(8, 8);
        let m = halo_stretch(&form, &rel, 2, 1.0, 0.0, 0.25);
        for kind in BotAvatarType::ALL {
            {
                let mut guard = state_for(&ctx, kind.as_str());
                guard.get().base = kind.as_str().to_string();
            }
            let rim = rim_of(&form);
            let ring = ring_of(&form);
            let _ = reach(&rim, kind.as_str().len() as f64, 0.0);
            if !ring.x.is_empty() {
                let _ = ring_order(ring.x.len(), 0.2, 3);
            }
            let _ = stack_reach(&rel, 2, 1.0, 0.0, 0.25, -1.0, 0.0, -22.5, None);
            let _ = halo_rays(&form, &rel, 2, 1.0, 0.0, 0.25, m, 0.2);
            let _ = brightest(&Rays {
                d: vec![],
                c: vec![vec![1.0, 2.0, 3.0]],
            });
            assert!(moved([0.0, 1.0, 0.0], None));
        }
    }
}
