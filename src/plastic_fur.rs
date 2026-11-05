//! Fabric pile from `plastic.ts` (fur).
//!
//! Ports `plastic.ts` 892–1210, 1234–2539, and 2683–2812: the pile cache,
//! fibre bake, halo cut, and gap / tip / sheen films.
#![allow(dead_code, unused_variables)]

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use tiny_skia::{BlendMode, LineCap, LineJoin, Path, PathBuilder, Pixmap};

use crate::canvas::{self, Canvas2D, upload_straight_rgba};
use crate::plastic::{
    Form, Material, PAD, POW_N, SPAN, V3, hurrying, idle, land, linear_color, make_canvas, norm3,
    note_missed, pow_lut as pow_table, smooth, srgb as srgb_byte,
};

/// The pile's neutral grey. Line 960.
pub(crate) const FUR_MID: f64 = 128.0;
pub(crate) const LOCK_COVER: f64 = 3.2;
pub(crate) const LOCK_DENSE: f64 = 1.0 / 2.56;
/// `1 / ((2/3) * sqrt(1/3))`. Written out because `sqrt` is not const here.
pub(crate) const LOCK_TAPER: f64 = 2.598076211353316;
pub(crate) const LOCK_PART: f64 = 0.6;
pub(crate) const PILE_AMB: f64 = 0.07;
pub(crate) const PILE_FILL: f64 = 0.14;
pub(crate) const PILE_WRAP: f64 = 0.3;
pub(crate) const PILE_SHADOW: f64 = 0.6;
pub(crate) const PILE_AO: f64 = 0.35;
pub(crate) const PILE_ROOT: f64 = 0.3;
pub(crate) const PILE_KK: f64 = 0.3;
pub(crate) const PILE_GAIN: f64 = 1.26;
pub(crate) const PILE_LIFT: f64 = 1.6;
pub(crate) const PILE_BIAS: f64 = 0.045;
/// Fibres stand about this far out of the surface, radians. Line 1002.
pub(crate) const PILE_STAND: f64 = 0.5;
pub(crate) const PILE_COS: f64 = 0.8775825618903728;
pub(crate) const PILE_SIN: f64 = 0.479425538604203;
pub(crate) const PILE_CREASE: f64 = 0.22;
pub(crate) const PILE_H: f64 = 0.5;
pub(crate) const GAP_REF: f64 = 0.36;
pub(crate) const PILE_LIT: f64 = 1.1;
pub(crate) const TIP_TOP: f64 = 1.625;
pub(crate) const STRAND_TAPER: f64 = 0.5;
pub(crate) const STRAND_RISE: f64 = 0.9;
pub(crate) const STRAND_SHORT: f64 = 0.7;
pub(crate) const STRAND_CAST_H: f64 = 0.15;
pub(crate) const STRAND_CAST: f64 = 1.3;
/// How far past the body's square the pile's film reaches. Lines 1033–1034.
pub(crate) const FILM_M: f64 = 20.0;
pub(crate) const FILM_SPAN: f64 = SPAN + 2.0 * FILM_M;
pub(crate) const HALO_A: f64 = 0.8;
pub(crate) const HALO_STREAK: f64 = 0.3;
pub(crate) const HALO_HAIR: f64 = 0.8;
pub(crate) const HALO_LIFT: f64 = 0.7;
pub(crate) const HALO_SHADE: f64 = 0.5;
pub(crate) const HALO_FIT: f64 = 2.5;
pub(crate) const HALO_CORE: f64 = 1.2;
pub(crate) const HALO_INSET: f64 = 3.0;
pub(crate) const HALO_VEIL: f64 = 0.8;
pub(crate) const HALO_BLEND: f64 = 0.3;
pub(crate) const HALO_EASE: f64 = 0.1;
/// Key elevation for fabric, radians. Line 1075.
pub(crate) const KEY_EL: f64 = (32.0 * std::f64::consts::PI) / 180.0;
/// Default back-light softness. Line 1077.
pub(crate) const BACK_SOFT: f64 = 1.0;
pub(crate) const GAP_DEEP: f64 = 0.28;
pub(crate) const GAP_SCATTER: f64 = 0.42;
pub(crate) const TIP_TINT: f64 = 0.6;

const PI: f64 = std::f64::consts::PI;
const LP: usize = 16;
const W_MEAN: f64 = 0.25;

/// Plush pile baked for one outline, resolution, and light.
#[derive(Clone)]
pub struct Fur {
    /// The pile's own resolution.
    pub r: u32,
    pub mask: Option<Pixmap>,
    pub haze: Option<Pixmap>,
    pub cut: Option<Pixmap>,
    pub core: Option<Pixmap>,
    pub dark: Option<Pixmap>,
    pub light: Option<Pixmap>,
    pub glint: Option<Pixmap>,
    pub back: Option<Pixmap>,
    pub back_soft: Option<Pixmap>,
    /// Identity shared by clones so gap / tip / sheen / cut caches hit.
    /// `0` means "do not share". Assigned when a bake is filed.
    pub cache_key: u64,
}

impl Fur {
    /// Empty pile (`furFor`'s fallback, line 1241).
    pub(crate) fn empty() -> Self {
        Self {
            r: 0,
            mask: None,
            haze: None,
            cut: None,
            core: None,
            dark: None,
            light: None,
            glint: None,
            back: None,
            back_soft: None,
            cache_key: 0,
        }
    }
}

/// Length, density, edge fuzz, tufts, curl, and how much the pile hangs.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FurStyle {
    pub length: f64,
    pub density: f64,
    pub fuzz: f64,
    pub clumps: f64,
    pub curl: f64,
    /// 0–1: how much the pile hangs down.
    pub gravity: f64,
}

/// Stock pile. `plastic.ts` line 949.
pub const FUR_STOCK: FurStyle = FurStyle {
    length: 1.0,
    density: 1.6,
    fuzz: 0.9,
    clumps: 0.4,
    curl: 0.7,
    gravity: 0.9,
};

/// Lights the pile's fibres are baked for.
#[derive(Debug, Clone, Copy)]
pub struct FurLights {
    pub bx: f64,
    pub by: f64,
    /// How far round to the front the key sits, radians.
    pub front: f64,
}

/// One bake of [`fur_job`], as steps. The synchronous cache runs them inline.
pub struct FurJob {
    pub steps: Vec<Box<dyn FnMut() + Send>>,
}

/// `lightBin` (932): 15° bins. `Math.round` is half toward +∞.
pub(crate) fn light_bin(lx: f64, ly: f64) -> i32 {
    js_round_i32(ly.atan2(lx) / (PI / 12.0))
}

/// Pile resolution for a cap `cap_px` device pixels across (`furTier`, 934).
pub(crate) fn fur_tier(cap_px: f64) -> u32 {
    if cap_px <= 200.0 {
        192
    } else if cap_px <= 360.0 {
        320
    } else {
        512
    }
}

/// `styleId` (950–951).
pub(crate) fn style_id(style: &FurStyle) -> String {
    format!(
        "{},{},{},{},{},{}",
        js_to_fixed(style.length, 2),
        js_to_fixed(style.density, 2),
        js_to_fixed(style.fuzz, 2),
        js_to_fixed(style.clumps, 2),
        js_to_fixed(style.curl, 2),
        js_to_fixed(style.gravity, 2),
    )
}

/// `furKey` (952).
///
/// `n` is the tier of the form the fur grows on: two sizes in one fur tier still bake from
/// their own form, so neither depends on which was drawn first.
#[allow(clippy::too_many_arguments)]
pub(crate) fn fur_key(
    key: &str,
    half_depth: f64,
    r: u32,
    n: u32,
    bin: i32,
    style: &FurStyle,
    lights: &FurLights,
) -> String {
    format!(
        "{key}|{}|{r}|{n}|{bin}|{}|{}",
        js_round_i32(half_depth),
        style_id(style),
        lights_id(lights)
    )
}

/// `lightsId` (1106).
pub(crate) fn lights_id(lights: &FurLights) -> String {
    format!(
        "{}|{}",
        js_round_i32(lights.by.atan2(lights.bx) / (PI / 36.0)),
        js_round_i32(lights.front / (PI / 90.0))
    )
}

/// `fabricKey` (1078–1081). `front` defaults to [`KEY_EL`].
pub(crate) fn fabric_key(u: V3, v: V3, front: f64) -> V3 {
    let (s, c) = front.sin_cos();
    norm3([
        u[0] * c + v[0] * s,
        u[1] * c + v[1] * s,
        u[2] * c + v[2] * s,
    ])
}

/// `fabricBack` (1082–1084).
pub(crate) fn fabric_back(k: V3, v: V3) -> V3 {
    norm3([
        0.94 * k[0] - 0.34 * v[0],
        0.94 * k[1] - 0.34 * v[1],
        0.94 * k[2] - 0.34 * v[2],
    ])
}

/// Unit back-light on screen from the key (`backFromKey`, 1088–1092). Y is down.
pub fn back_from_key(lx: f64, ly: f64) -> [f64; 2] {
    let x = -0.5 * lx;
    let y = -0.5 * ly - 0.87;
    let l = or1(x.hypot(y));
    [x / l, y / l]
}

/// Stock lights for a key at `(lx, ly)` (`stockLights`, 1102–1105).
pub fn stock_lights(lx: f64, ly: f64) -> FurLights {
    let [bx, by] = back_from_key(lx, ly);
    FurLights {
        bx,
        by,
        front: KEY_EL,
    }
}

/// `hash2` (1109–1113).
pub(crate) fn hash2(x: f64, y: f64) -> f64 {
    let mut h = js_to_int32(x * 374761393.0 + y * 668265263.0);
    h = imul(h ^ u_shr(h, 13), 1274126177);
    ((h ^ u_shr(h, 16)) as u32) as f64 / 4294967296.0
}

/// Bilinear sample of a form field (`bilerp`, 1116–1122).
pub(crate) fn bilerp(field: &[f32], n: usize, x: f64, y: f64) -> f64 {
    if n < 2 || field.len() < n * n {
        return 0.0;
    }
    let last = (n - 1) as f64;
    let fx = x.clamp(0.0, last);
    let fy = y.clamp(0.0, last);
    let x0 = (js_to_int32(fx).min(n as i32 - 2)).max(0) as usize;
    let y0 = (js_to_int32(fy).min(n as i32 - 2)).max(0) as usize;
    let tx = fx - x0 as f64;
    let ty = fy - y0 as f64;
    let i = y0 * n + x0;
    let a = field[i] as f64;
    let b = field[i + 1] as f64;
    let c = field[i + n] as f64;
    let d = field[i + n + 1] as f64;
    (a * (1.0 - tx) + b * tx) * (1.0 - ty) + (c * (1.0 - tx) + d * tx) * ty
}

/// `furReady` (1126–1163). `sync` builds on this thread.
///
/// `idle` in this port runs the task immediately, so a miss finishes before
/// returning. The lookup afterwards still walks the cache in insertion order
/// and may hand back an older pile of the same shape for that one call.
pub(crate) fn fur_ready(
    form: &Form,
    key: &str,
    half_depth: f64,
    cap_px: f64,
    sync: bool,
    lx: f64,
    ly: f64,
    style: &FurStyle,
    lights: &FurLights,
) -> Option<Arc<Fur>> {
    let r = fur_tier(cap_px);
    let bin = light_bin(lx, ly);
    let id = fur_key(key, half_depth, r, form.n, bin, style, lights);
    if let Some(hit) = cache_get(&id) {
        return Some(hit);
    }
    if sync {
        if let Some(mut job) = fur_job(form, &id, r, lx, ly, style, lights) {
            for step in job.steps.iter_mut() {
                step();
            }
        }
        return cache_get(&id);
    }
    let prefix = format!("{key}|{}|", js_round_i32(half_depth));
    if r > 192 && hurrying() == 0 && !pending_has(&id) && !prefix_busy(&prefix) {
        fur_ready(form, key, half_depth, 1.0, false, lx, ly, style, lights);
    }
    if !pending_has(&id) {
        {
            let mut store = store().lock().expect("fur store");
            store.pending.insert(id.clone());
            store.dropped.remove(&id);
            let shape = format!("{key}|{}|{r}|{}|{bin}", js_round_i32(half_depth), form.n);
            let olds = {
                let line = store.queued.entry(shape).or_default();
                line.push(id.clone());
                let mut olds = Vec::new();
                while line.len() > 2 {
                    olds.push(line.remove(0));
                }
                olds
            };
            for old in olds {
                store.dropped.insert(old.clone());
                store.pending.remove(&old);
            }
        }
        if let Some(mut job) = fur_job(form, &id, r, lx, ly, style, lights) {
            for step in job.steps.iter_mut() {
                let drop_id = id.clone();
                idle(|| {
                    if !is_dropped(&drop_id) {
                        step();
                    }
                });
            }
        }
    }
    note_missed();
    // The bake above ran in place, so its own fur is ready. Only a dropped bake falls back
    // to another fur of the same shape, which depends on what was drawn before.
    cache_get(&id).or_else(|| first_with_prefix(&prefix))
}

/// `rng` (1166–1174). Returns values in `0..1`.
pub(crate) fn rng(seed: u32) -> Box<dyn FnMut() -> f64 + Send> {
    let mut g = Rng::new(seed);
    Box::new(move || g.next())
}

/// `valueNoise` (1178–1188).
pub(crate) fn value_noise(scale: f64, seed: f64) -> Box<dyn Fn(f64, f64) -> f64 + Send> {
    let n = Noise2 { scale, seed };
    Box::new(move |x, y| n.at(x, y))
}

/// `hash3` (1192–1196).
pub(crate) fn hash3(x: f64, y: f64, z: f64) -> f64 {
    let mut h = js_to_int32(x * 374761393.0 + y * 668265263.0 + z * 1274126177.0);
    h = imul(h ^ u_shr(h, 13), 1274126177);
    ((h ^ u_shr(h, 16)) as u32) as f64 / 4294967296.0
}

/// `valueNoise3` (1197–1209).
pub(crate) fn value_noise3(scale: f64, seed: f64) -> Box<dyn Fn(f64, f64, f64) -> f64 + Send> {
    let n = Noise3 { scale, seed };
    Box::new(move |x, y, z| n.at(x, y, z))
}

/// The pile for a form (`furFor`, 1234–1242).
///
/// TS defaults: `key = "custom"`, `half_depth = 9.75`, `cap_px = 320`, `lx = -1`, `ly = 0`,
/// `style = FUR_STOCK`, `lights = stock_lights(lx, ly)`.
pub fn fur_for(
    form: &Form,
    key: &str,
    half_depth: f64,
    cap_px: f64,
    lx: f64,
    ly: f64,
    style: &FurStyle,
    lights: &FurLights,
) -> Fur {
    let r = fur_tier(cap_px);
    let id = fur_key(key, half_depth, r, form.n, light_bin(lx, ly), style, lights);
    if let Some(hit) = cache_get(&id) {
        return (*hit).clone();
    }
    if let Some(mut job) = fur_job(form, &id, r, lx, ly, style, lights) {
        for step in job.steps.iter_mut() {
            step();
        }
    }
    cache_get(&id)
        .map(|hit| (*hit).clone())
        .unwrap_or_else(Fur::empty)
}

/// `furJob` (1246–2527).
pub(crate) fn fur_job(
    form: &Form,
    id: &str,
    r: u32,
    lx: f64,
    ly: f64,
    style: &FurStyle,
    lights: &FurLights,
) -> Option<FurJob> {
    if cache_has(id) {
        return None;
    }
    BAKES.fetch_add(1, Ordering::Relaxed);
    let bake = Bake::prepare(form, id, r, lx, ly, style, lights);
    let n = bake.steps.len();
    let shared = Arc::new(Mutex::new(bake));
    let mut steps: Vec<Box<dyn FnMut() + Send>> = Vec::with_capacity(n);
    for _ in 0..n {
        let shared = Arc::clone(&shared);
        steps.push(Box::new(move || {
            let do_land = {
                let mut bake = shared.lock().expect("fur bake");
                bake.run_next()
            };
            if do_land {
                land();
            }
        }));
    }
    Some(FurJob { steps })
}

fn cut_bank() -> &'static Mutex<HashMap<u64, Arc<Pixmap>>> {
    static C: OnceLock<Mutex<HashMap<u64, Arc<Pixmap>>>> = OnceLock::new();
    C.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Halo complement shared across frames. Does not clone the pile.
pub(crate) fn shared_cut(fur: &Fur) -> Option<Arc<Pixmap>> {
    if fur.cache_key != 0 {
        let bank = cut_bank().lock().expect("cuts");
        if let Some(hit) = bank.get(&fur.cache_key) {
            return Some(Arc::clone(hit));
        }
    }
    if let Some(cut) = &fur.cut {
        let arc = Arc::new(cut.clone());
        if fur.cache_key != 0 {
            cut_bank()
                .lock()
                .expect("cuts")
                .insert(fur.cache_key, Arc::clone(&arc));
        }
        return Some(arc);
    }
    let mask = fur.mask.as_ref()?;
    let arc = Arc::new(cut_from_mask(mask));
    if fur.cache_key != 0 {
        cut_bank()
            .lock()
            .expect("cuts")
            .insert(fur.cache_key, Arc::clone(&arc));
    }
    Some(arc)
}

/// The halo's complement (`furCut`, 2530–2539).
pub(crate) fn fur_cut(fur: &mut Fur) -> Option<Pixmap> {
    if let Some(cut) = &fur.cut {
        return Some(cut.clone());
    }
    if fur.mask.is_none() {
        return None;
    }
    if fur.cache_key != 0 {
        if let Some(cut) = cached_cut(fur.cache_key) {
            fur.cut = Some(cut.clone());
            return Some(cut);
        }
    }
    let mask = fur.mask.as_ref()?;
    let cut = cut_from_mask(mask);
    if fur.cache_key != 0 {
        store_set_cut(fur.cache_key, cut.clone());
    }
    fur.cut = Some(cut.clone());
    Some(cut)
}

/// Gaps between locks, in a deep shade of `color` (`gapFilm`, 2685–2725). `vivid` defaults to 0.
pub(crate) fn gap_film(fur: &Fur, color: &str, vivid: f64) -> Option<Arc<Pixmap>> {
    let src = fur.dark.as_ref()?;
    let vq = js_round(vivid.min(1.0) * 10.0) / 10.0;
    let key = format!("{color}|{vq}");
    if fur.cache_key == 0 {
        return Some(Arc::new(build_gap(src, color, vq)));
    }
    film_cached(
        &mut store().lock().expect("fur store").gaps,
        fur.cache_key,
        key,
        4,
        || build_gap(src, color, vq),
    )
}

/// Lit tips (`tipFilm`, 2735–2758).
pub(crate) fn tip_film(fur: &Fur, color: &str, vivid: f64) -> Option<Arc<Pixmap>> {
    let src = fur.light.as_ref()?;
    let v = js_round((TIP_TINT + (1.0 - TIP_TINT) * vivid.clamp(0.0, 1.0)) * 10.0) / 10.0;
    let key = format!("{color}|{v}");
    if fur.cache_key == 0 {
        return Some(Arc::new(build_tip(src, color, v)));
    }
    film_cached(
        &mut store().lock().expect("fur store").tips,
        fur.cache_key,
        key,
        6,
        || build_tip(src, color, v),
    )
}

/// Fibre highlights and the back light as one film (`sheenFilm`, 2769–2812).
pub(crate) fn sheen_film(fur: &Fur, color: &str, material: &Material) -> Option<Arc<Pixmap>> {
    if fur.glint.is_none() || fur.back.is_none() {
        return None;
    }
    let hk = (0.45 * material.highlight * material.shine.unwrap_or(0.0)).min(1.0);
    let rk = (1.1 * material.rim).min(1.0);
    if hk <= 0.01 && rk <= 0.01 {
        return None;
    }
    let vivid = material.vivid.unwrap_or(0.0).min(1.0);
    let soft = (material.back_soft.unwrap_or(BACK_SOFT)).clamp(0.0, 1.0);
    let soft = js_round(soft * 20.0) / 20.0;
    let key = format!(
        "{color}|{}|{}|{}|{soft}",
        js_to_fixed(hk, 2),
        js_to_fixed(rk, 2),
        js_to_fixed(vivid, 1)
    );
    let build = || build_sheen(fur, color, hk, rk, vivid, soft).map(Arc::new);
    if fur.cache_key == 0 {
        return build();
    }
    {
        let mut store = store().lock().expect("fur store");
        let map = store.sheens.entry(fur.cache_key).or_default();
        if let Some(hit) = map.get(&key) {
            return hit.clone();
        }
    }
    let built = build();
    let mut store = store().lock().expect("fur store");
    let map = store.sheens.entry(fur.cache_key).or_default();
    if map.len() >= 6 {
        map.clear();
    }
    map.insert(key, built.clone());
    built
}

fn build_gap(src: &Pixmap, color: &str, vq: f64) -> Pixmap {
    GAP_BUILDS.fetch_add(1, Ordering::Relaxed);
    let lin = linear_color(color);
    let e = [
        srgb_byte(lin[0]) / 255.0,
        srgb_byte(lin[1]) / 255.0,
        srgb_byte(lin[2]) / 255.0,
    ];
    let mx = e[0].max(e[1]).max(e[2]).max(1e-3);
    let lu = 0.2126 * e[0] + 0.7152 * e[1] + 0.0722 * e[2];
    let pale = e[0].min(e[1]).min(e[2]) / mx;
    let kd = GAP_DEEP + GAP_SCATTER * lu * lu * pale;
    let ch = |v: f64| -> i32 { js_round_i32(255.0 * kd * v * (v / mx).powf(3.2 + 3.0 * vq)) };
    let r = ch(e[0]);
    let g = ch(e[1]);
    let b = ch(e[2]);
    let deep = (0.2126 * r as f64 + 0.7152 * g as f64 + 0.0722 * b as f64) / 255.0 / lu.max(1e-3);
    let alpha = (1.0f64).min(((1.0 - GAP_REF) / (1.0 - deep).max(0.05)).sqrt());
    let mut dst = Pixmap::new(src.width(), src.height()).expect("gap film");
    canvas::composite_source_over_subrect(&mut dst, src, src.width(), src.height(), alpha);
    source_in_fill(&mut dst, sat_u8(r), sat_u8(g), sat_u8(b));
    dst
}

fn build_tip(src: &Pixmap, color: &str, v: f64) -> Pixmap {
    TIP_BUILDS.fetch_add(1, Ordering::Relaxed);
    let lin = linear_color(color);
    let e = [srgb_byte(lin[0]), srgb_byte(lin[1]), srgb_byte(lin[2])];
    let mx = e[0].max(e[1]).max(e[2]).max(1.0);
    let ch = |u: f64| -> i32 { js_round_i32(255.0 + ((255.0 * u) / mx - 255.0) * v.min(1.0)) };
    let mut dst = Pixmap::new(src.width(), src.height()).expect("tip film");
    canvas::copy_subrect(&mut dst, src, src.width(), src.height());
    source_in_fill(
        &mut dst,
        sat_u8(ch(e[0])),
        sat_u8(ch(e[1])),
        sat_u8(ch(e[2])),
    );
    dst
}

fn build_sheen(fur: &Fur, color: &str, hk: f64, rk: f64, vivid: f64, soft: f64) -> Option<Pixmap> {
    SHEEN_BUILDS.fetch_add(1, Ordering::Relaxed);
    let gl = fur.glint.as_ref()?;
    let bk = fur.back.as_ref()?;
    let w = gl.width();
    let h = gl.height();
    let lin = linear_color(color);
    let e = [srgb_byte(lin[0]), srgb_byte(lin[1]), srgb_byte(lin[2])];
    let mx = e[0].max(e[1]).max(e[2]).max(1.0);
    let mix = |k: f64| -> [u8; 3] {
        let ch =
            |i: usize| -> u8 { sat_u8(js_round_i32(255.0 + ((255.0 * e[i]) / mx - 255.0) * k)) };
        [ch(0), ch(1), ch(2)]
    };
    let mut dst = Pixmap::new(w, h).expect("sheen film");
    let mut tmp = Pixmap::new(w, h).expect("sheen scratch");
    let layers: [(&Pixmap, [u8; 3], f64); 3] = [
        (bk, mix(0.65 + 0.35 * vivid), rk * (1.0 - 0.85 * soft)),
        (
            fur.back_soft.as_ref().unwrap_or(bk),
            mix(0.7 + 0.3 * vivid),
            if fur.back_soft.is_some() {
                rk * soft
            } else {
                0.0
            },
        ),
        (gl, mix(0.45 + 0.55 * vivid), hk),
    ];
    // The middle slot stands in only so the array stays sized; a missing
    // back-soft layer is skipped by the alpha test below.
    let use_soft = fur.back_soft.is_some();
    for (i, (src, fill, a)) in layers.iter().enumerate() {
        if i == 1 && !use_soft {
            continue;
        }
        if *a <= 0.01 {
            continue;
        }
        canvas::copy_subrect(&mut tmp, src, w, h);
        source_in_fill(&mut tmp, fill[0], fill[1], fill[2]);
        canvas::composite_source_over_subrect(&mut dst, &tmp, w, h, *a);
    }
    Some(dst)
}

/// Opaque black, then `destination-out` by the mask. Integer 1:1, so alpha is `255 - mask.a`.
fn cut_from_mask(mask: &Pixmap) -> Pixmap {
    let mut dst = Pixmap::new(mask.width(), mask.height()).expect("fur cut");
    let dw = dst.width() as usize;
    let sw = mask.width() as usize;
    let h = dst.height().min(mask.height()) as usize;
    let d = dst.data_mut();
    let s = mask.data();
    for y in 0..h {
        for x in 0..dw.min(sw) {
            let sa = s[y * sw * 4 + x * 4 + 3];
            let o = y * dw * 4 + x * 4;
            d[o] = 0;
            d[o + 1] = 0;
            d[o + 2] = 0;
            d[o + 3] = 255 - sa;
        }
    }
    dst
}

fn source_in_fill(dst: &mut Pixmap, r: u8, g: u8, b: u8) {
    for px in dst.data_mut().chunks_exact_mut(4) {
        let a = px[3] as u32;
        if a == 0 {
            px.copy_from_slice(&[0, 0, 0, 0]);
        } else if a == 255 {
            px[0] = r;
            px[1] = g;
            px[2] = b;
        } else {
            px[0] = ((r as u32 * a + 127) / 255) as u8;
            px[1] = ((g as u32 * a + 127) / 255) as u8;
            px[2] = ((b as u32 * a + 127) / 255) as u8;
        }
    }
}

fn film_cached(
    maps: &mut HashMap<u64, HashMap<String, Arc<Pixmap>>>,
    id: u64,
    key: String,
    limit: usize,
    build: impl FnOnce() -> Pixmap,
) -> Option<Arc<Pixmap>> {
    if let Some(hit) = maps.get(&id).and_then(|m| m.get(&key)) {
        return Some(Arc::clone(hit));
    }
    let built = Arc::new(build());
    let map = maps.entry(id).or_default();
    if map.len() >= limit {
        map.clear();
    }
    map.insert(key, Arc::clone(&built));
    Some(built)
}

// --- integer / numeric helpers matching JS ---------------------------------

fn js_to_int32(n: f64) -> i32 {
    if !n.is_finite() {
        return 0;
    }
    let wrapped = n.trunc().rem_euclid(4294967296.0);
    if wrapped >= 2147483648.0 {
        (wrapped - 4294967296.0) as i32
    } else {
        wrapped as i32
    }
}

fn imul(a: i32, b: i32) -> i32 {
    a.wrapping_mul(b)
}

fn u_shr(h: i32, s: u32) -> i32 {
    ((h as u32) >> s) as i32
}

/// `Math.round`: half toward +∞.
fn js_round(v: f64) -> f64 {
    if !v.is_finite() {
        return v;
    }
    (v + 0.5).floor()
}

fn js_round_i32(v: f64) -> i32 {
    let r = js_round(v);
    if !r.is_finite() { 0 } else { r as i32 }
}

fn u8_round(v: f64) -> u8 {
    let r = js_round(v);
    if !(r > 0.0) {
        0
    } else if r >= 255.0 {
        255
    } else {
        r as u8
    }
}

fn sat_u8(v: i32) -> u8 {
    v.clamp(0, 255) as u8
}

/// `Number.prototype.toFixed`: half away from zero on the exact f64 value.
fn js_to_fixed(v: f64, digits: u32) -> String {
    if v.is_nan() {
        return "NaN".to_string();
    }
    if v.is_infinite() {
        return if v.is_sign_positive() {
            "Infinity".to_string()
        } else {
            "-Infinity".to_string()
        };
    }
    let neg = v.is_sign_negative() && v != 0.0;
    let x = v.abs();
    if x >= 1e21 {
        let s = format!("{x}");
        return if neg { format!("-{s}") } else { s };
    }
    if x == 0.0 {
        return if digits == 0 {
            "0".to_string()
        } else {
            format!(
                "{}0.{}",
                if neg { "-" } else { "" },
                "0".repeat(digits as usize)
            )
        };
    }
    let bits = x.to_bits();
    let biased = ((bits >> 52) & 0x7ff) as i32;
    let frac = bits & ((1u64 << 52) - 1);
    let (mant, e2): (u128, i32) = if biased == 0 {
        (frac as u128, -1074)
    } else {
        (((1u64 << 52) | frac) as u128, biased - 1023 - 52)
    };
    let mut p5 = 1u128;
    for _ in 0..digits {
        p5 *= 5;
    }
    let num = mant * p5;
    let p = e2 + digits as i32;
    let n: u128 = if p >= 0 {
        num << p.min(127)
    } else {
        let k = (-p) as u32;
        if k > 127 {
            0
        } else if k == 0 {
            num
        } else {
            let half = 1u128 << (k - 1);
            let rem = num & ((1u128 << k) - 1);
            let q = num >> k;
            if rem >= half { q + 1 } else { q }
        }
    };
    let mut digits_str = n.to_string();
    if digits > 0 {
        if digits_str.len() <= digits as usize {
            let pad = digits as usize + 1 - digits_str.len();
            digits_str = format!("{}{digits_str}", "0".repeat(pad));
        }
        let split = digits_str.len() - digits as usize;
        let (a, b) = digits_str.split_at(split);
        digits_str = format!("{a}.{b}");
    }
    if neg {
        format!("-{digits_str}")
    } else {
        digits_str
    }
}

fn or1(v: f64) -> f64 {
    if v == 0.0 || v.is_nan() { 1.0 } else { v }
}

fn hypot3(x: f64, y: f64, z: f64) -> f64 {
    x.hypot(y).hypot(z)
}

struct Rng {
    a: u32,
}

impl Rng {
    fn new(seed: u32) -> Self {
        Self { a: seed }
    }

    fn next(&mut self) -> f64 {
        self.a = self.a.wrapping_add(0x6d2b79f5);
        let a_i = self.a as i32;
        let mut t = imul(a_i ^ u_shr(a_i, 15), 1 | a_i);
        t = t.wrapping_add(imul(t ^ u_shr(t, 7), 61 | t)) ^ t;
        ((t ^ u_shr(t, 14)) as u32) as f64 / 4294967296.0
    }
}

#[derive(Clone, Copy)]
struct Noise2 {
    scale: f64,
    seed: f64,
}

impl Noise2 {
    fn at(self, x: f64, y: f64) -> f64 {
        let fx = x / self.scale;
        let fy = y / self.scale;
        let x0 = fx.floor();
        let y0 = fy.floor();
        let tx = fx - x0;
        let ty = fy - y0;
        let sx = tx * tx * (3.0 - 2.0 * tx);
        let sy = ty * ty * (3.0 - 2.0 * ty);
        let h = |i: f64, j: f64| hash2(i + self.seed * 131.0, j - self.seed * 71.0);
        let a = h(x0, y0);
        let b = h(x0 + 1.0, y0);
        let c = h(x0, y0 + 1.0);
        let d = h(x0 + 1.0, y0 + 1.0);
        (a + (b - a) * sx) * (1.0 - sy) + (c + (d - c) * sx) * sy
    }
}

#[derive(Clone, Copy)]
struct Noise3 {
    scale: f64,
    seed: f64,
}

impl Noise3 {
    fn at(self, x: f64, y: f64, z: f64) -> f64 {
        let fx = x / self.scale;
        let fy = y / self.scale;
        let fz = z / self.scale;
        let x0 = fx.floor();
        let y0 = fy.floor();
        let z0 = fz.floor();
        let tx = fx - x0;
        let ty = fy - y0;
        let tz = fz - z0;
        let sx = tx * tx * (3.0 - 2.0 * tx);
        let sy = ty * ty * (3.0 - 2.0 * ty);
        let sz = tz * tz * (3.0 - 2.0 * tz);
        let h = |i: f64, j: f64, k: f64| {
            hash3(
                i + self.seed * 131.0,
                j - self.seed * 71.0,
                k + self.seed * 17.0,
            )
        };
        let lerp = |a: f64, b: f64, t: f64| a + (b - a) * t;
        let c00 = lerp(h(x0, y0, z0), h(x0 + 1.0, y0, z0), sx);
        let c10 = lerp(h(x0, y0 + 1.0, z0), h(x0 + 1.0, y0 + 1.0, z0), sx);
        let c01 = lerp(h(x0, y0, z0 + 1.0), h(x0 + 1.0, y0, z0 + 1.0), sx);
        let c11 = lerp(
            h(x0, y0 + 1.0, z0 + 1.0),
            h(x0 + 1.0, y0 + 1.0, z0 + 1.0),
            sx,
        );
        lerp(lerp(c00, c10, sy), lerp(c01, c11, sy), sz)
    }
}

// --- cache -----------------------------------------------------------------

struct Store {
    order: Vec<String>,
    furs: HashMap<String, Arc<Fur>>,
    pending: HashSet<String>,
    dropped: HashSet<String>,
    queued: HashMap<String, Vec<String>>,
    gaps: HashMap<u64, HashMap<String, Arc<Pixmap>>>,
    tips: HashMap<u64, HashMap<String, Arc<Pixmap>>>,
    sheens: HashMap<u64, HashMap<String, Option<Arc<Pixmap>>>>,
}

fn store() -> &'static Mutex<Store> {
    static S: OnceLock<Mutex<Store>> = OnceLock::new();
    S.get_or_init(|| {
        Mutex::new(Store {
            order: Vec::new(),
            furs: HashMap::new(),
            pending: HashSet::new(),
            dropped: HashSet::new(),
            queued: HashMap::new(),
            gaps: HashMap::new(),
            tips: HashMap::new(),
            sheens: HashMap::new(),
        })
    })
}

static BAKES: AtomicU64 = AtomicU64::new(0);
static GAP_BUILDS: AtomicU64 = AtomicU64::new(0);
static TIP_BUILDS: AtomicU64 = AtomicU64::new(0);
static SHEEN_BUILDS: AtomicU64 = AtomicU64::new(0);
static NEXT_FUR: AtomicU64 = AtomicU64::new(1);

/// A hit moves to the back of the eviction order, so furs in use stay while unused ones go.
fn cache_get(id: &str) -> Option<Arc<Fur>> {
    let mut s = store().lock().expect("fur store");
    let hit = s.furs.get(id).map(Arc::clone)?;
    if s.order.last().map(String::as_str) != Some(id)
        && let Some(at) = s.order.iter().position(|k| k == id)
    {
        let k = s.order.remove(at);
        s.order.push(k);
    }
    Some(hit)
}

fn cache_has(id: &str) -> bool {
    store().lock().expect("fur store").furs.contains_key(id)
}

fn pending_has(id: &str) -> bool {
    store().lock().expect("fur store").pending.contains(id)
}

fn is_dropped(id: &str) -> bool {
    store().lock().expect("fur store").dropped.contains(id)
}

fn prefix_busy(prefix: &str) -> bool {
    let store = store().lock().expect("fur store");
    store.furs.keys().any(|k| k.starts_with(prefix))
        || store.pending.iter().any(|k| k.starts_with(prefix))
}

fn first_with_prefix(prefix: &str) -> Option<Arc<Fur>> {
    let store = store().lock().expect("fur store");
    for k in &store.order {
        if k.starts_with(prefix) {
            if let Some(fur) = store.furs.get(k) {
                return Some(Arc::clone(fur));
            }
        }
    }
    None
}

const FURS_MAX: usize = 96;

fn file_fur(id: &str, fur: Fur) {
    let mut s = store().lock().expect("fur store");
    // Room for every body on a full gallery page; past it the least recently drawn goes.
    if s.furs.len() >= FURS_MAX {
        if let Some(old) = s.order.first().cloned() {
            s.order.remove(0);
            if let Some(old_fur) = s.furs.remove(&old) {
                let k = old_fur.cache_key;
                s.gaps.remove(&k);
                s.tips.remove(&k);
                s.sheens.remove(&k);
            }
        }
    }
    if !s.furs.contains_key(id) {
        s.order.push(id.to_string());
    }
    s.pending.remove(id);
    s.furs.insert(id.to_string(), Arc::new(fur));
}

fn cached_cut(key: u64) -> Option<Pixmap> {
    let s = store().lock().expect("fur store");
    s.furs
        .values()
        .find(|f| f.cache_key == key)
        .and_then(|f| f.cut.clone())
}

fn store_set_cut(key: u64, cut: Pixmap) {
    let mut s = store().lock().expect("fur store");
    if let Some(fur) = s.furs.values_mut().find(|f| f.cache_key == key) {
        Arc::make_mut(fur).cut = Some(cut);
    }
}

// --- bake ------------------------------------------------------------------

#[derive(Clone, Copy)]
enum Step {
    Grid(u32, u32),
    Crease,
    Seed,
    Splat(u32),
    BlurWideH,
    BlurWideV,
    Soften,
    White,
    Strand(u32, u32),
    Light(u32, u32),
    NormLocks,
    Upload,
    Hairs { kind: u8, part: u32, parts: u32 },
    Fringe(u32),
    Tufts,
    StrokeEdges,
    Halo(u32, u32),
    Finish,
}

#[derive(Clone, Copy)]
struct Kind {
    count: f64,
    len: f64,
    vary: f64,
    end_on: f64,
    width: f64,
    spread: f64,
    alpha: f64,
    deep: f64,
    lift: f64,
    light: f64,
    dark: f64,
    bend: f64,
    clump0: f64,
    clump1: f64,
    clump_w: f64,
    shadow: f64,
    shadow_w: f64,
    over: f64,
    crowd: bool,
    short: f64,
}

struct StrokePath {
    b: PathBuilder,
    any: bool,
}

impl StrokePath {
    fn new() -> Self {
        Self {
            b: PathBuilder::new(),
            any: false,
        }
    }
    fn move_to(&mut self, x: f64, y: f64) {
        self.b.move_to(x as f32, y as f32);
        self.any = true;
    }
    fn line_to(&mut self, x: f64, y: f64) {
        self.b.line_to(x as f32, y as f32);
    }
    fn quad_to(&mut self, cx: f64, cy: f64, x: f64, y: f64) {
        self.b.quad_to(cx as f32, cy as f32, x as f32, y as f32);
    }
    fn finish(self) -> Option<Path> {
        if self.any { self.b.finish() } else { None }
    }
}

struct Bake {
    id: String,
    form: Form,
    r: u32,
    rb: usize,
    rl: usize,
    g: usize,
    px: f64,
    o: f64,
    gu: f64,
    pl: f64,
    lx: f64,
    ly: f64,
    k_len: f64,
    k_den: f64,
    fuzz: f64,
    clumps: f64,
    curl: f64,
    gravity: f64,
    tuft_k: f64,
    h_max: f64,
    lock_wd: f64,
    lock_ld: f64,
    tuft_h: f64,
    stand0: f64,
    depth_k: f64,
    strand_k: f64,
    strand_half: usize,
    nsh: usize,
    r_box: usize,
    fine: f64,
    crown_x: f64,
    crown_y: f64,
    k3: V3,
    b3: V3,
    h3: V3,
    f3: V3,
    rand: Rng,
    lean: Noise2,
    tone: Noise2,
    swirl: Noise2,
    bump: Noise2,
    swell: Noise2,
    tuft_n: Noise3,
    size_n: Noise2,
    flow_x: Vec<f32>,
    flow_y: Vec<f32>,
    tilt_g: Vec<f32>,
    nx_g: Vec<f32>,
    ny_g: Vec<f32>,
    nz_g: Vec<f32>,
    on_g: Vec<f32>,
    tone_g: Vec<f32>,
    h_g: Vec<f32>,
    crease_g: Vec<f32>,
    pf_x: Vec<f32>,
    pf_y: Vec<f32>,
    fore_g: Vec<f32>,
    lock: Vec<f32>,
    lock_lit: Vec<f32>,
    pile_h: Vec<f32>,
    pile_b: Vec<f32>,
    own: Vec<i32>,
    locks: Vec<f32>,
    n_locks: usize,
    line: Vec<f32>,
    white: Vec<f32>,
    strand_buf: Vec<f32>,
    strand_w: Vec<Vec<f32>>,
    strand_mean: Vec<f32>,
    strand_sd: Vec<f32>,
    sh_x: Vec<i32>,
    sh_y: Vec<i32>,
    sh_d: Vec<f32>,
    dif_lut: Vec<f32>,
    gam_lut: Vec<f32>,
    dark_img: Vec<u8>,
    light_img: Vec<u8>,
    glint_img: Vec<u8>,
    back_img: Vec<u8>,
    soft_img: Vec<u8>,
    mask_img: Vec<u8>,
    haze_img: Vec<u8>,
    core_img: Vec<u8>,
    fib: [f32; 11],
    seen: [f32; 4],
    kinds: Vec<Kind>,
    cells: Vec<usize>,
    edge_tries: usize,
    fringe: StrokePath,
    fringe_back: Vec<StrokePath>,
    tufts: StrokePath,
    tuft_tips: Vec<StrokePath>,
    tuft_back: Vec<StrokePath>,
    fringe_p: Option<Path>,
    tufts_p: Option<Path>,
    dark_c: Canvas2D,
    light_c: Canvas2D,
    mask_c: Canvas2D,
    glint_c: Canvas2D,
    back_c: Canvas2D,
    soft_c: Canvas2D,
    steps: Vec<Step>,
    cursor: usize,
}

fn push_rows(steps: &mut Vec<Step>, n: u32, chunk: u32, ctor: fn(u32, u32) -> Step) {
    if chunk == 0 {
        return;
    }
    let mut y0 = 0u32;
    while y0 < n {
        let y1 = n.min(y0.saturating_add(chunk));
        steps.push(ctor(y0, y1));
        if y1 <= y0 {
            break;
        }
        y0 = y1;
    }
}

fn lock_along(s: f64) -> f64 {
    (if s < 0.25 { 4.0 * s } else { 1.0 }) * (1.0 - 0.5 * s)
}

fn paint_stroke(canvas: &mut Canvas2D, path: &Option<Path>) {
    if let Some(p) = path {
        canvas.stroke_path(p);
    }
}

fn shade_of(v: f64) -> usize {
    let c = v.clamp(-1.0, 1.0);
    js_round(((c + 1.0) / 2.0) * 15.0).clamp(0.0, 15.0) as usize
}

fn glint_of(v: f64) -> usize {
    let i = js_to_int32(v * 6.0);
    if i <= 0 { 0 } else { (i as usize).min(5) }
}

impl Bake {
    fn prepare(
        form: &Form,
        id: &str,
        r: u32,
        lx: f64,
        ly: f64,
        style: &FurStyle,
        lights: &FurLights,
    ) -> Self {
        let n = form.n;
        let px = r as f64 / SPAN;
        let o = PAD + FILM_M;
        let rb = js_round((r as f64 * FILM_SPAN) / SPAN).max(1.0) as usize;
        let g = 112usize;
        let gu = FILM_SPAN / g as f64;
        let rl = rb >> 1;
        let pl = rl as f64 / FILM_SPAN;
        let k_len = style.length;
        let k_den = style.density;
        let fuzz = style.fuzz;
        let clumps = style.clumps;
        let curl = style.curl;
        let gravity = style.gravity;
        let ll = or1(lx.hypot(ly));
        let lux = lx / ll;
        let luy = ly / ll;
        let uf: V3 = [lux, luy, 0.0];
        let vf: V3 = [0.0, 0.0, 1.0];
        let df: V3 = [0.0, 1.0, 0.0];
        let k3 = fabric_key(uf, vf, lights.front);
        let b3 = fabric_back([lights.bx, lights.by, 0.0], vf);
        let h3 = norm3([k3[0], k3[1], k3[2] + 1.0]);
        let f3 = norm3([
            -0.55 * uf[0] + 0.85 * vf[0] + 0.15 * df[0],
            -0.55 * uf[1] + 0.85 * vf[1] + 0.15 * df[1],
            -0.55 * uf[2] + 0.85 * vf[2] + 0.15 * df[2],
        ]);
        let gg = g * g;
        let pix = rb * rb;
        let lock_w = 0.8 + 0.45 * k_len;
        let lock_wd = lock_w * (0.65 + 0.4 * clumps);
        let lock_ld = 1.5 + 1.6 * k_len;
        let h_max = 0.32 * lock_wd * (0.15 + 0.85 * clumps);
        let cell = ((0.6 * lock_wd * lock_ld) / LOCK_COVER / LOCK_DENSE).sqrt();
        let gn = (FILM_SPAN / cell).ceil().max(1.0) as usize;
        let tuft_k = (0.2 + 0.5 * fuzz) * 1.8_f64.min(k_len);
        let fine = if r >= 512 {
            1.0
        } else if r >= 320 {
            0.45
        } else {
            0.0
        };
        let strand_half = ((0.35 + 0.4 * k_len) * px).round().max(1.0) as usize;
        let mut strand_w = Vec::with_capacity(2 * strand_half + 1);
        let mut strand_mean = vec![0f32; 2 * strand_half + 1];
        let mut strand_sd = vec![0f32; 2 * strand_half + 1];
        for len in 0..=(2 * strand_half) {
            let mut w = vec![0f32; len + 1];
            let mut m1 = 0.0;
            let mut v2 = 0.0;
            for t in 0..=len {
                let tau = if len == 0 { 0.0 } else { t as f64 / len as f64 };
                let p = if tau <= STRAND_SHORT {
                    1.0
                } else {
                    (1.0 - tau) / (1.0 - STRAND_SHORT)
                };
                // Float32Array store, then the mean is accumulated from the stored value.
                let wt = (1.0 - STRAND_TAPER * tau) as f32;
                w[t] = wt;
                let wd = wt as f64;
                m1 += wd * p * W_MEAN;
                v2 += wd * wd * (p / 7.0 - W_MEAN * W_MEAN * p * p);
            }
            strand_w.push(w);
            strand_mean[len] = m1 as f32;
            strand_sd[len] = v2.max(1e-6).sqrt() as f32;
        }
        let reach = (1.5 * lock_wd * px).max(1.0);
        let nsh = (js_round(reach) as i32).clamp(1, 5) as usize;
        let mut sh_x = vec![0i32; nsh];
        let mut sh_y = vec![0i32; nsh];
        let mut sh_d = vec![0f32; nsh];
        for t in 0..nsh {
            let d = ((t + 1) as f64 / nsh as f64) * reach;
            sh_x[t] = js_round_i32(lux * d);
            sh_y[t] = js_round_i32(luy * d);
            sh_d[t] = ((sh_x[t] as f64).hypot(sh_y[t] as f64) / px) as f32;
        }
        let r_box = (0.3 * lock_wd * px).round().max(1.0) as usize;
        let mut crown_y = 12.0;
        {
            let mut y = -PAD;
            let nn = n as usize;
            while y < 60.0 {
                let sx = ((50.0 + PAD) / SPAN) * nn as f64 - 0.5;
                let sy = ((y + PAD) / SPAN) * nn as f64 - 0.5;
                if bilerp(&form.sd, nn, sx, sy) > 0.5 {
                    crown_y = y + 1.0;
                    break;
                }
                y += 0.5;
            }
            crown_y -= 18.0;
        }
        let area = FILM_SPAN * FILM_SPAN;
        let kinds_src = [
            Kind {
                count: 0.12,
                len: 1.2,
                vary: 1.0,
                end_on: 0.6,
                width: 0.09,
                spread: 30.0,
                alpha: 0.4,
                deep: 1.0,
                lift: 0.25,
                light: 0.46,
                dark: 0.34,
                bend: 0.93,
                clump0: 1.0,
                clump1: 0.0,
                clump_w: 0.0,
                shadow: 0.0,
                shadow_w: 1.0,
                over: 0.0,
                crowd: false,
                short: 0.0,
            },
            Kind {
                count: 0.4 * fine,
                len: 0.9,
                vary: 1.2,
                end_on: 0.35,
                width: 0.13,
                spread: 100.0,
                alpha: 0.26,
                deep: 0.2,
                lift: 0.18,
                light: 0.55,
                dark: 0.06,
                bend: 1.2,
                clump0: 1.0,
                clump1: js_round(1.0 + 2.0 * clumps),
                clump_w: 0.08 + 0.07 * clumps,
                shadow: 0.25,
                shadow_w: 2.0,
                over: 0.25,
                crowd: true,
                short: 1.0,
            },
        ];
        let mut steps = Vec::new();
        push_rows(&mut steps, g as u32, 28, Step::Grid);
        steps.push(Step::Crease);
        steps.push(Step::Seed);
        for part in 0..6 {
            steps.push(Step::Splat(part));
        }
        steps.push(Step::BlurWideH);
        steps.push(Step::BlurWideV);
        steps.push(Step::Soften);
        steps.push(Step::White);
        push_rows(&mut steps, rb as u32, 32, Step::Strand);
        push_rows(&mut steps, rb as u32, 16, Step::Light);
        steps.push(Step::NormLocks);
        steps.push(Step::Upload);
        let mut kinds = Vec::new();
        for (ki, kind) in kinds_src.into_iter().enumerate() {
            if kind.count <= 0.0 {
                continue;
            }
            let per_root = kind.clump0 + 0.5 * kind.clump1;
            let crowd = if kind.crowd { 2.0 } else { 1.0 };
            let parts = ((area * kind.count * k_den * crowd * (0.5 + 0.5 * per_root)) / 9000.0)
                .ceil()
                .max(1.0) as u32;
            kinds.push(kind);
            let kind_i = (kinds.len() - 1) as u8;
            let _ = ki;
            for part in 0..parts {
                steps.push(Step::Hairs {
                    kind: kind_i,
                    part,
                    parts,
                });
            }
        }
        steps.push(Step::Fringe(0));
        steps.push(Step::Fringe(1));
        steps.push(Step::Tufts);
        steps.push(Step::StrokeEdges);
        push_rows(&mut steps, r, 64, Step::Halo);
        steps.push(Step::Finish);
        let mut fringe_back = Vec::with_capacity(6);
        let mut tuft_back = Vec::with_capacity(6);
        for _ in 0..6 {
            fringe_back.push(StrokePath::new());
            tuft_back.push(StrokePath::new());
        }
        let mut tuft_tips = Vec::with_capacity(3);
        for _ in 0..3 {
            tuft_tips.push(StrokePath::new());
        }
        let pile_h = vec![(-0.25 * h_max) as f32; pix];
        let own = vec![-1i32; pix];
        let _ = pile_h.len();
        Self {
            id: id.to_string(),
            form: form.clone(),
            r,
            rb,
            rl,
            g,
            px,
            o,
            gu,
            pl,
            lx: lux,
            ly: luy,
            k_len,
            k_den,
            fuzz,
            clumps,
            curl,
            gravity,
            tuft_k,
            h_max,
            lock_wd,
            lock_ld,
            tuft_h: 1.5 * h_max * (0.25 + clumps),
            stand0: 0.45 - 0.2 * gravity,
            depth_k: if r >= 512 {
                1.0
            } else if r >= 320 {
                0.8
            } else {
                0.55
            },
            strand_k: 0.12
                * (if r >= 512 {
                    1.0
                } else if r >= 320 {
                    0.75
                } else {
                    0.4
                })
                * (1.4_f64).min(0.55 + 0.35 * k_den),
            strand_half,
            nsh,
            r_box,
            fine,
            crown_x: 50.0,
            crown_y,
            k3,
            b3,
            h3,
            f3,
            rand: Rng::new(n.wrapping_mul(7919).wrapping_add(17)),
            lean: Noise2 {
                scale: 3.4,
                seed: 21.0,
            },
            tone: Noise2 {
                scale: 22.0,
                seed: 9.0,
            },
            swirl: Noise2 {
                scale: 13.0,
                seed: 57.0,
            },
            bump: Noise2 {
                scale: 0.7 + 0.35 * k_len,
                seed: 61.0,
            },
            swell: Noise2 {
                scale: 2.2 + 0.6 * k_len,
                seed: 67.0,
            },
            tuft_n: Noise3 {
                scale: 1.6 * lock_ld,
                seed: 79.0,
            },
            size_n: Noise2 {
                scale: 9.0,
                seed: 73.0,
            },
            flow_x: vec![0.0; gg],
            flow_y: vec![0.0; gg],
            tilt_g: vec![0.0; gg],
            nx_g: vec![0.0; gg],
            ny_g: vec![0.0; gg],
            nz_g: vec![0.0; gg],
            on_g: vec![0.0; gg],
            tone_g: vec![0.0; gg],
            h_g: vec![0.0; gg],
            crease_g: vec![0.0; gg],
            pf_x: vec![0.0; gg],
            pf_y: vec![0.0; gg],
            fore_g: vec![0.0; gg],
            lock: vec![0.0; rl * rl],
            lock_lit: vec![0.0; rl * rl],
            pile_h,
            pile_b: vec![0.0; pix],
            own,
            locks: vec![0.0; gn * gn * LP],
            n_locks: 0,
            line: vec![0.0; rb],
            white: vec![0.0; pix],
            strand_buf: vec![0.0; pix],
            strand_w,
            strand_mean,
            strand_sd,
            sh_x,
            sh_y,
            sh_d,
            dif_lut: pow_table(1.35),
            gam_lut: pow_table(1.0 / 2.2),
            dark_img: vec![0u8; pix * 4],
            light_img: vec![0u8; pix * 4],
            glint_img: vec![0u8; pix * 4],
            back_img: vec![0u8; pix * 4],
            soft_img: vec![0u8; pix * 4],
            mask_img: vec![0u8; (r as usize) * (r as usize) * 4],
            haze_img: vec![0u8; (r as usize) * (r as usize) * 4],
            core_img: vec![0u8; (r as usize) * (r as usize) * 4],
            fib: [0.0; 11],
            seen: [0.0; 4],
            kinds,
            cells: Vec::new(),
            edge_tries: 0,
            fringe: StrokePath::new(),
            fringe_back,
            tufts: StrokePath::new(),
            tuft_tips,
            tuft_back,
            fringe_p: None,
            tufts_p: None,
            dark_c: make_canvas(rb as u32),
            light_c: make_canvas(rb as u32),
            mask_c: make_canvas(r),
            glint_c: make_canvas(rb as u32),
            back_c: make_canvas(rb as u32),
            soft_c: make_canvas(rb as u32),
            steps,
            cursor: 0,
        }
    }

    /// One idle step. `true` means the pile was filed and the caller should `land()`.
    fn run_next(&mut self) -> bool {
        if self.cursor >= self.steps.len() {
            return false;
        }
        let step = self.steps[self.cursor];
        self.cursor += 1;
        match step {
            Step::Grid(a, b) => self.step_grid(a, b),
            Step::Crease => self.step_crease(),
            Step::Seed => self.step_seed(),
            Step::Splat(p) => self.step_splat(p),
            Step::BlurWideH => {
                self.box_blur(true, false, self.r_box, true);
                self.box_blur(false, false, self.r_box, true);
            }
            Step::BlurWideV => {
                self.box_blur(false, false, self.r_box, false);
                self.box_blur(false, false, self.r_box, false);
            }
            Step::Soften => {
                self.box_blur(true, true, 1, true);
                self.box_blur(true, true, 1, false);
            }
            Step::White => self.step_white(),
            Step::Strand(a, b) => self.step_strand(a, b),
            Step::Light(a, b) => self.step_light(a, b),
            Step::NormLocks => self.step_norm(),
            Step::Upload => self.step_upload(),
            Step::Hairs { kind, part, parts } => self.step_hairs(kind, part, parts),
            Step::Fringe(p) => self.step_fringe(p),
            Step::Tufts => self.step_tufts(),
            Step::StrokeEdges => self.step_stroke_edges(),
            Step::Halo(a, b) => self.step_halo(a, b),
            Step::Finish => return self.step_finish(),
        }
        false
    }

    fn at(&self, field: &[f32], x: f64, y: f64) -> f64 {
        let n = self.form.n as usize;
        bilerp(
            field,
            n,
            ((x + PAD) / SPAN) * n as f64 - 0.5,
            ((y + PAD) / SPAN) * n as f64 - 0.5,
        )
    }
    fn sd_at(&self, x: f64, y: f64) -> f64 {
        self.at(&self.form.sd, x, y)
    }
    fn h_at(&self, x: f64, y: f64) -> f64 {
        self.at(&self.form.h, x, y)
    }
    fn grid_at(&self, field: &[f32], x: f64, y: f64) -> f64 {
        bilerp(
            field,
            self.g,
            (x + self.o) / self.gu - 0.5,
            (y + self.o) / self.gu - 0.5,
        )
    }
    fn lock_at(&self, field: &[f32], x: f64, y: f64) -> f64 {
        bilerp(
            field,
            self.rl,
            (x + self.o) * self.pl - 0.5,
            (y + self.o) * self.pl - 0.5,
        )
    }
    fn dir_at(&self, x: f64, y: f64) -> (f64, f64) {
        let ux = self.grid_at(&self.pf_x, x, y);
        let uy = self.grid_at(&self.pf_y, x, y);
        let ul = (ux * ux + uy * uy).sqrt();
        if ul > 1e-4 {
            (ux / ul, uy / ul)
        } else {
            (0.0, 1.0)
        }
    }
    fn free_dir_at(&self, x: f64, y: f64) -> (f64, f64) {
        let ux = self.grid_at(&self.flow_x, x, y);
        let uy = self.grid_at(&self.flow_y, x, y);
        let ul = (ux * ux + uy * uy).sqrt();
        if ul > 1e-4 {
            (ux / ul, uy / ul)
        } else {
            (0.0, 1.0)
        }
    }
    fn outward(&self, x: f64, y: f64) -> (f64, f64) {
        let e = 0.8;
        let ox = self.sd_at(x - e, y) - self.sd_at(x + e, y);
        let oy = self.sd_at(x, y - e) - self.sd_at(x, y + e);
        let ol = or1(ox.hypot(oy));
        (ox / ol, oy / ol)
    }
    fn slope_at(&self, x: f64, y: f64) -> (f64, f64, f64) {
        let e = 0.6;
        let gx = (self.h_at(x + e, y) - self.h_at(x - e, y)) / (2.0 * e);
        let gy = (self.h_at(x, y + e) - self.h_at(x, y - e)) / (2.0 * e);
        let m = gx.hypot(gy);
        let tilt = m / (1.0 + m * m).sqrt();
        if m > 1e-6 {
            (-gx / m, -gy / m, tilt)
        } else {
            (0.0, 0.0, tilt)
        }
    }
    fn flow(&self, x: f64, y: f64) -> (f64, f64) {
        let mut rx = x - self.crown_x;
        let mut ry = y - self.crown_y;
        let rl = or1(rx.hypot(ry));
        rx /= rl;
        ry /= rl;
        let fall = self.gravity * (1.0 - 0.6 * (-rl / 8.0).exp());
        let mut dx = rx * (1.0 - 0.8 * fall);
        let mut dy = ry * (1.0 - 0.8 * fall) + fall;
        let d0 = self.sd_at(x, y);
        if d0 < 5.0 {
            let (ox, oy) = self.outward(x, y);
            let up = 0.0_f64.max(-oy);
            let ex = ox * (1.0 - self.gravity * up);
            let ey = oy * (1.0 - self.gravity * up) + 1.2 * self.gravity;
            let w = (1.0 - d0 / 5.0).clamp(0.0, 1.0) * 0.75;
            dx = dx * (1.0 - w) + ex * w * 1.6;
            dy = dy * (1.0 - w) + ey * w * 1.6;
        }
        let a = (self.lean.at(x, y) - 0.5) * (0.5 + 1.6 * self.curl)
            + (self.swirl.at(x, y) - 0.5) * (0.6 + self.curl);
        let ca = a.cos();
        let sa = a.sin();
        let qx = dx * ca - dy * sa;
        let qy = dx * sa + dy * ca;
        let m = or1(qx.hypot(qy));
        (qx / m, qy / m)
    }
    fn tuft_at(&self, x: f64, y: f64) -> f64 {
        let v = 0.3 * self.lock_at(&self.lock, x, y).clamp(-1.6, 1.6) / 1.6
            + 0.25 * (self.bump.at(x, y) - 0.5) * 2.4
            + 0.45 * (self.swell.at(x, y) - 0.5) * 2.4;
        self.tuft_k * v.clamp(-1.0, 1.0)
    }
    fn key_on(&self, nk: f64) -> f64 {
        let q = (nk + PILE_WRAP) / (1.0 + PILE_WRAP);
        if q <= 0.0 {
            0.0
        } else if q >= 1.0 {
            1.0
        } else {
            lut_index(&self.dif_lut, q)
        }
    }
    fn gam_of(&self, lq: f64) -> f64 {
        if lq >= 1.0 {
            1.0
        } else {
            lut_index(&self.gam_lut, lq)
        }
    }

    fn step_grid(&mut self, y0: u32, y1: u32) {
        let g = self.g;
        let gu = self.gu;
        let o = self.o;
        let gravity = self.gravity;
        for j in y0..y1 {
            for x in 0..g as u32 {
                let i = j as usize * g + x as usize;
                let design_x = (x as f64 + 0.5) * gu - o;
                let design_y = (j as f64 + 0.5) * gu - o;
                let (mut fx, mut fy) = self.flow(design_x, design_y);
                let (dx, dy, tilt_s) = self.slope_at(design_x, design_y);
                let w =
                    tilt_s * (0.25 + 0.5 * (1.0 - gravity)) * (1.0 - gravity * 0.0_f64.max(-dy));
                fx = fx * (1.0 - w) + dx * w;
                fy = fy * (1.0 - w) + (dy + 0.35 * gravity) * w;
                let fl = or1(fx.hypot(fy));
                fx /= fl;
                fy /= fl;
                let mut nx = dx * tilt_s;
                let mut ny = dy * tilt_s;
                let mut nz = (1.0 - tilt_s * tilt_s).max(0.0).sqrt();
                let d0 = self.sd_at(design_x, design_y);
                if d0 < 2.0 {
                    let (ox, oy) = self.outward(design_x, design_y);
                    let bend = if d0 >= 0.0 {
                        0.7 * (1.0 - d0 / 2.0)
                    } else {
                        1.0_f64.min(0.7 - 0.2 * d0)
                    };
                    nx += bend * (ox - nx);
                    ny += bend * (oy - ny);
                    nz -= bend * nz;
                    let nl = or1(hypot3(nx, ny, nz));
                    nx /= nl;
                    ny /= nl;
                    nz /= nl;
                }
                let s = (nx * nx + ny * ny).sqrt();
                let (gx, gy) = if s > 1e-4 {
                    (nx / s, ny / s)
                } else {
                    (0.0, 0.0)
                };
                let a = fx * gx + fy * gy;
                let k = (1.0 - 0.0_f64.max(nz)) * a;
                let mut qx = PILE_COS * (fx - k * gx) + PILE_SIN * s * gx;
                let mut qy = PILE_COS * (fy - k * gy) + PILE_SIN * s * gy;
                let att = smooth(-0.5, 1.5, d0);
                qx = fx + (qx - fx) * att;
                qy = fy + (qy - fy) * att;
                let ql = or1((qx * qx + qy * qy).sqrt());
                self.flow_x[i] = fx as f32;
                self.flow_y[i] = fy as f32;
                self.tilt_g[i] = tilt_s as f32;
                self.nx_g[i] = nx as f32;
                self.ny_g[i] = ny as f32;
                self.nz_g[i] = nz as f32;
                self.on_g[i] = smooth(-3.5, -1.5, d0) as f32;
                self.tone_g[i] = self.tone.at(design_x, design_y) as f32;
                self.h_g[i] = if d0 > 0.0 {
                    self.h_at(design_x, design_y) as f32
                } else {
                    0.0
                };
                self.pf_x[i] = (qx / ql) as f32;
                self.pf_y[i] = (qy / ql) as f32;
                self.fore_g[i] = (1.0 + (1.0_f64.min(ql) - 1.0) * att) as f32;
            }
        }
    }

    fn step_crease(&mut self) {
        let g = self.g;
        let mut tmp = vec![0f32; g * g];
        let mut avg = self.h_g.clone();
        for _ in 0..3 {
            for y in 0..g {
                for x in 0..g {
                    let mut acc = 0.0;
                    for t in -2i32..=2 {
                        let xx = (x as i32 + t).clamp(0, g as i32 - 1) as usize;
                        acc += avg[y * g + xx] as f64;
                    }
                    tmp[y * g + x] = (acc / 5.0) as f32;
                }
            }
            for y in 0..g {
                for x in 0..g {
                    let mut acc = 0.0;
                    for t in -2i32..=2 {
                        let yy = (y as i32 + t).clamp(0, g as i32 - 1) as usize;
                        acc += tmp[yy * g + x] as f64;
                    }
                    avg[y * g + x] = (acc / 5.0) as f32;
                }
            }
        }
        let gu = self.gu;
        let o = self.o;
        for j in 0..g {
            for x in 0..g {
                let i = j * g + x;
                let inside = smooth(
                    0.2,
                    2.0,
                    self.sd_at((x as f64 + 0.5) * gu - o, (j as f64 + 0.5) * gu - o),
                );
                let valley = smooth(0.3, 3.5, avg[i] as f64 - self.h_g[i] as f64) * inside;
                let under = smooth(0.15, 0.85, self.ny_g[i] as f64) * (1.0 - self.nz_g[i] as f64);
                self.crease_g[i] =
                    (1.0_f64.min(valley + 0.45 * under) * self.on_g[i] as f64) as f32;
            }
        }
    }

    fn step_seed(&mut self) {
        let cell = ((0.6 * self.lock_wd * self.lock_ld) / LOCK_COVER / LOCK_DENSE).sqrt();
        let gn = (FILM_SPAN / cell).ceil().max(1.0) as usize;
        let px = self.px;
        let o = self.o;
        let curl = self.curl;
        for gy in 0..gn {
            for gx in 0..gn {
                let design_x = (gx as f64 + self.rand.next()) * cell - o;
                let design_y = (gy as f64 + self.rand.next()) * cell - o;
                let r1 = self.rand.next();
                let r2 = self.rand.next();
                let r3 = self.rand.next();
                let r4 = self.rand.next();
                let r5 = self.rand.next();
                let r6 = self.rand.next();
                let r7 = self.rand.next();
                let mut nx = self.grid_at(&self.nx_g, design_x, design_y);
                let mut ny = self.grid_at(&self.ny_g, design_x, design_y);
                let mut nz = self.grid_at(&self.nz_g, design_x, design_y);
                let nl = or1(hypot3(nx, ny, nz));
                nx /= nl;
                ny /= nl;
                nz /= nl;
                let (fx, fy) = self.free_dir_at(design_x, design_y);
                let ja = (r1 - 0.5) * (0.2 + 0.45 * curl);
                let cj = ja.cos();
                let sj = ja.sin();
                let ux = fx * cj - fy * sj;
                let uy = fx * sj + fy * cj;
                let a = ux * nx + uy * ny;
                let mut tx = ux - a * nx;
                let mut ty = uy - a * ny;
                let mut tz = -a * nz;
                let mut tl = hypot3(tx, ty, tz);
                if tl < 0.05 {
                    tx = nz * nx;
                    ty = nz * ny;
                    tz = nz * nz - 1.0;
                    tl = or1(hypot3(tx, ty, tz));
                }
                tx /= tl;
                ty /= tl;
                tz /= tl;
                let mut bx = ny * tz - nz * ty;
                let mut by = nz * tx - nx * tz;
                let mut bz = nx * ty - ny * tx;
                let st = self.stand0 + 0.2 * r2;
                let cs = st.cos();
                let sn = st.sin();
                let grow = 0.8 + 0.45 * self.size_n.at(design_x, design_y);
                let wd = self.lock_wd * grow * (0.8 + 0.4 * r3);
                let ld = self.lock_ld * grow * (0.75 + 0.5 * r4);
                let mut ax = (tx * cs + nx * sn) * ld * px;
                let mut ay = (ty * cs + ny * sn) * ld * px;
                let mut la = ax.hypot(ay);
                if la < 1.5 {
                    ax = ux;
                    ay = uy;
                    la = 1.5;
                } else {
                    ax /= la;
                    ay /= la;
                }
                let mut across = -bx * ay + by * ax;
                if across < 0.0 {
                    bx = -bx;
                    by = -by;
                    bz = -bz;
                    across = -across;
                }
                let wa = 0.6_f64.max(0.5 * wd * px * 0.15_f64.max(across));
                let seen = (la / (ld * px)) * (wa / (0.5 * wd * px));
                let tn = self
                    .tuft_n
                    .at(design_x, design_y, self.h_at(design_x, design_y));
                let gate = (LOCK_DENSE / LOCK_DENSE.max(seen))
                    * (1.0 - LOCK_PART * self.clumps * (1.0 - smooth(0.25, 0.65, tn)));
                if r5 > gate {
                    continue;
                }
                let b = self.n_locks * LP;
                if b + LP > self.locks.len() {
                    continue;
                }
                self.locks[b] = ((design_x + o) * px) as f32;
                self.locks[b + 1] = ((design_y + o) * px) as f32;
                self.locks[b + 2] = ax as f32;
                self.locks[b + 3] = ay as f32;
                self.locks[b + 4] = la as f32;
                self.locks[b + 5] = wa as f32;
                self.locks[b + 6] = (0.1 * self.h_max * r6 + self.tuft_h * tn) as f32;
                self.locks[b + 7] = ((r7 - 0.5) * 1.2 * curl) as f32;
                self.locks[b + 8] = tx as f32;
                self.locks[b + 9] = ty as f32;
                self.locks[b + 10] = tz as f32;
                self.locks[b + 11] = bx as f32;
                self.locks[b + 12] = by as f32;
                self.locks[b + 13] = bz as f32;
                self.locks[b + 14] = ld as f32;
                self.locks[b + 15] = wd as f32;
                self.n_locks += 1;
            }
        }
    }

    fn step_splat(&mut self, part: u32) {
        let n = self.n_locks;
        let k0 = (n * part as usize) / 6;
        let k1 = (n * (part as usize + 1)) / 6;
        let rb = self.rb as i32;
        let h_max = self.h_max;
        for k in k0..k1 {
            let b = k * LP;
            let ox = self.locks[b] as f64;
            let oy = self.locks[b + 1] as f64;
            let ax = self.locks[b + 2] as f64;
            let ay = self.locks[b + 3] as f64;
            let la = self.locks[b + 4] as f64;
            let wa = self.locks[b + 5] as f64;
            let z0 = self.locks[b + 6] as f64;
            let bend = self.locks[b + 7] as f64;
            if la == 0.0 || wa == 0.0 {
                continue;
            }
            let ext = wa * (1.0 + bend.abs()) + 1.0;
            let ex = ox + ax * la;
            let ey = oy + ay * la;
            let xa = (ox.min(ex) - ext).floor().max(0.0) as i32;
            let xb = (ox.max(ex) + ext).ceil().min((rb - 1) as f64) as i32;
            let ya = (oy.min(ey) - ext).floor().max(0.0) as i32;
            let yb = (oy.max(ey) + ext).ceil().min((rb - 1) as f64) as i32;
            if xa > xb || ya > yb {
                continue;
            }
            let ila = 1.0 / la;
            let iwa = 1.0 / wa;
            for y in ya..=yb {
                let dy = y as f64 + 0.5 - oy;
                let mut i = y as usize * self.rb + xa as usize;
                for x in xa..=xb {
                    let dx = x as f64 + 0.5 - ox;
                    let s = (dx * ax + dy * ay) * ila;
                    if s > 0.0 && s < 1.0 {
                        let w = LOCK_TAPER * s.sqrt() * (1.0 - s);
                        let v = (dy * ax - dx * ay) * iwa - bend * s * s;
                        if v < w && v > -w {
                            let q = v / w;
                            let h = z0 + h_max * lock_along(s) * (1.0 - q * q);
                            if h > self.pile_h[i] as f64 {
                                self.pile_h[i] = h as f32;
                                self.own[i] = k as i32;
                            }
                        }
                    }
                    i += 1;
                }
            }
        }
    }

    fn box_blur(&mut self, from_h: bool, to_h: bool, radius: usize, horizontal: bool) {
        let rb = self.rb;
        if rb == 0 {
            return;
        }
        let stride = if horizontal { 1 } else { rb };
        let step = if horizontal { rb } else { 1 };
        let inv = 1.0 / (2 * radius + 1) as f64;
        let mut line = std::mem::take(&mut self.line);
        if line.len() < rb {
            line.resize(rb, 0.0);
        }
        let r = radius as i32;
        for k in 0..rb {
            let o = k * step;
            for t in 0..rb {
                let idx = o + t * stride;
                line[t] = if from_h {
                    self.pile_h[idx]
                } else {
                    self.pile_b[idx]
                };
            }
            let at = |t: i32| -> f64 {
                let i = if t < 0 {
                    0
                } else if t >= rb as i32 {
                    rb - 1
                } else {
                    t as usize
                };
                line[i] as f64
            };
            let mut acc = 0.0;
            for t in -r..=r {
                acc += at(t);
            }
            for t in 0..rb {
                let idx = o + t * stride;
                let v = (acc * inv) as f32;
                if to_h {
                    self.pile_h[idx] = v;
                } else {
                    self.pile_b[idx] = v;
                }
                let ti = t as i32;
                acc += at(ti + r + 1) - at(ti - r);
            }
        }
        self.line = line;
    }

    fn fibre_light(&mut self, x: f64, y: f64, ux: f64, uy: f64, stand: f64) {
        let g = self.g;
        let last = (g - 1) as f64;
        let fx = ((x + self.o) / self.gu - 0.5).clamp(0.0, last);
        let fy = ((y + self.o) / self.gu - 0.5).clamp(0.0, last);
        let x0 = js_to_int32(fx).clamp(0, g as i32 - 2) as usize;
        let y0 = js_to_int32(fy).clamp(0, g as i32 - 2) as usize;
        let u = fx - x0 as f64;
        let v = fy - y0 as f64;
        let j = y0 * g + x0;
        let w00 = (1.0 - u) * (1.0 - v);
        let w10 = u * (1.0 - v);
        let w01 = (1.0 - u) * v;
        let w11 = u * v;
        let read = |f: &[f32]| -> f64 {
            f[j] as f64 * w00
                + f[j + 1] as f64 * w10
                + f[j + g] as f64 * w01
                + f[j + g + 1] as f64 * w11
        };
        let mut nx = read(&self.nx_g);
        let mut ny = read(&self.ny_g);
        let mut nz = read(&self.nz_g);
        let nn = or1((nx * nx + ny * ny + nz * nz).sqrt());
        nx /= nn;
        ny /= nn;
        nz /= nn;
        let nk = nx * self.k3[0] + ny * self.k3[1] + nz * self.k3[2];
        let crease = read(&self.crease_g);
        let on = read(&self.on_g);
        let tone = read(&self.tone_g);
        self.fib[4] = nx as f32;
        self.fib[5] = ny as f32;
        self.fib[6] = nz as f32;
        self.fib[7] = nk as f32;
        self.fib[8] = crease as f32;
        self.fib[9] = on as f32;
        self.fib[10] = tone as f32;
        if on <= 0.0 {
            self.fib[0] = 0.0;
            self.fib[1] = 0.0;
            self.fib[3] = 0.0;
            self.fib[2] = 0.7;
            return;
        }
        let key = smooth(-0.2, 0.5, nk);
        let along = ux * nx + uy * ny;
        let mut tx = ux - along * nx;
        let mut ty = uy - along * ny;
        let mut tz = -along * nz;
        let tl = (tx * tx + ty * ty + tz * tz).sqrt();
        if tl > 1e-3 {
            tx /= tl;
            ty /= tl;
            tz /= tl;
        } else {
            tx = 0.0;
            ty = 0.0;
            tz = 1.0;
        }
        let cs = stand.cos();
        let sn = stand.sin();
        tx = tx * cs + nx * sn;
        ty = ty * cs + ny * sn;
        tz = tz * cs + nz * sn;
        let h = self.h3;
        let th = tx * h[0] + ty * h[1] + tz * h[2];
        let nh = nx * h[0] + ny * h[1] + nz * h[2];
        let a1 = th - 0.12 * nh;
        let a2 = th + 0.2 * nh;
        let b1 = (1.0 - a1 * a1).max(0.0);
        let b2 = b1 * b1;
        let b4 = b2 * b2;
        let b8 = b4 * b4;
        let b16 = b8 * b8;
        let c1 = (1.0 - a2 * a2).max(0.0);
        let c2 = c1 * c1;
        let mut glint = key * (0.85 * b16 * b16 * b4 + 0.15 * c2 * c2);
        glint *= (0.4 + 0.6 * (1.0 - smooth(0.35, 0.85, nk))) * on;
        let side = (1.0 - tz * tz).max(0.0).sqrt();
        let bdot = nx * self.b3[0] + ny * self.b3[1] + nz * self.b3[2];
        let ny_up = 0.0_f64.max(ny);
        let back = smooth(-0.3, 0.6, bdot)
            * (1.0 - nz).powf(1.6)
            * (0.4 + 0.6 * side)
            * (1.0 - 0.6 * ny_up)
            * on;
        let soft = smooth(-0.5, 0.75, bdot) * (1.0 - nz).powf(0.85) * (1.0 - 0.6 * ny_up) * on;
        self.fib[0] = glint as f32;
        self.fib[1] = back as f32;
        self.fib[2] = (key * on + 0.7 * (1.0 - on)) as f32;
        self.fib[3] = soft as f32;
    }

    fn lock_seen(&mut self, x: usize, y: usize, design_x: f64, design_y: f64, k: i32) {
        if k < 0 {
            let (ux, uy) = self.dir_at(design_x, design_y);
            self.seen = [ux as f32, uy as f32, 0.5, 0.0];
            return;
        }
        let b = k as usize * LP;
        let ax = self.locks[b + 2] as f64;
        let ay = self.locks[b + 3] as f64;
        let la = self.locks[b + 4] as f64;
        let wa = self.locks[b + 5] as f64;
        let bend = self.locks[b + 7] as f64;
        let dx = x as f64 + 0.5 - self.locks[b] as f64;
        let dy = y as f64 + 0.5 - self.locks[b + 1] as f64;
        let s = ((dx * ax + dy * ay) / la).clamp(0.005, 0.995);
        let ss = s.sqrt();
        let w = LOCK_TAPER * ss * (1.0 - s);
        let dw = LOCK_TAPER * ((0.5 * (1.0 - s)) / ss - ss);
        let q = (((dy * ax - dx * ay) / wa - bend * s * s) / w).clamp(-1.0, 1.0);
        let sl = (q * dw + 2.0 * bend * s).clamp(-1.2, 1.2);
        let ux = la * ax - sl * wa * ay;
        let uy = la * ay + sl * wa * ax;
        let ul = or1((ux * ux + uy * uy).sqrt());
        self.seen[0] = (ux / ul) as f32;
        self.seen[1] = (uy / ul) as f32;
        self.seen[2] = s as f32;
        self.seen[3] = (lock_along(s) * (1.0 - q * q)) as f32;
    }

    fn step_white(&mut self) {
        let rb = self.rb;
        for y in 0..rb {
            for x in 0..rb {
                let h = hash2(x as f64 + 911.0, y as f64 + 37.0);
                self.white[y * rb + x] = (h * h * h) as f32;
            }
        }
    }

    fn step_strand(&mut self, y0: u32, y1: u32) {
        let rb = self.rb;
        let g = self.g;
        let px = self.px;
        let o = self.o;
        let half = self.strand_half;
        for y in y0 as usize..y1 as usize {
            let design_y = (y as f64 + 0.5) / px - o;
            for x in 0..rb {
                let i = y * rb + x;
                let design_x = (x as f64 + 0.5) / px - o;
                self.lock_seen(x, y, design_x, design_y, self.own[i]);
                let ux = self.seen[0] as f64;
                let uy = self.seen[1] as f64;
                let gx = ((design_x + o) / self.gu - 0.5).clamp(0.0, (g - 1) as f64);
                let gy = ((design_y + o) / self.gu - 0.5).clamp(0.0, (g - 1) as f64);
                let gx0 = js_to_int32(gx).clamp(0, g as i32 - 2) as usize;
                let gy0 = js_to_int32(gy).clamp(0, g as i32 - 2) as usize;
                let gtx = gx - gx0 as f64;
                let gty = gy - gy0 as f64;
                let gj = gy0 * g + gx0;
                let tilt = (self.tilt_g[gj] as f64 * (1.0 - gtx)
                    + self.tilt_g[gj + 1] as f64 * gtx)
                    * (1.0 - gty)
                    + (self.tilt_g[gj + g] as f64 * (1.0 - gtx)
                        + self.tilt_g[gj + g + 1] as f64 * gtx)
                        * gty;
                let fore = (self.fore_g[gj] as f64 * (1.0 - gtx)
                    + self.fore_g[gj + 1] as f64 * gtx)
                    * (1.0 - gty)
                    + (self.fore_g[gj + g] as f64 * (1.0 - gtx)
                        + self.fore_g[gj + g + 1] as f64 * gtx)
                        * gty;
                let raw = js_round((half as f64 * (0.5 + 0.5 * tilt) * fore) / PILE_COS);
                let len = (2.0 * raw.max(1.0).min(half as f64)) as usize;
                let mean = self.strand_mean[len] as f64;
                let sd = self.strand_sd[len] as f64;
                let mut acc = 0.0;
                let mut acc_r = 0.0;
                for t in 0..=len {
                    let sx = js_round_i32(x as f64 - ux * t as f64);
                    let sy = js_round_i32(y as f64 - uy * t as f64);
                    if sx < 0 || sy < 0 || sx >= rb as i32 || sy >= rb as i32 {
                        acc += mean / (len as f64 + 1.0);
                        continue;
                    }
                    let m = self.white[sy as usize * rb + sx as usize] as f64;
                    let f = m * 53.7;
                    let ls = len as f64 * (STRAND_SHORT + (1.0 - STRAND_SHORT) * (f - f.floor()));
                    if (t as f64) <= ls {
                        let wt = self.strand_w[len][t] as f64;
                        acc += m * wt;
                        acc_r += (m * wt * t as f64) / if ls == 0.0 { 1.0 } else { ls };
                    }
                }
                let strand = (acc - mean) / sd;
                let along = if acc > 1e-4 { acc_r / acc } else { 0.5 };
                self.strand_buf[i] =
                    (strand + STRAND_RISE * strand.clamp(0.0, 2.5) * (along - 0.45)) as f32;
            }
        }
    }

    fn step_light(&mut self, y0: u32, y1: u32) {
        let rb = self.rb;
        let px = self.px;
        let o = self.o;
        let h_max = self.h_max;
        let rl = self.rl;
        for y in y0 as usize..y1 as usize {
            let design_y = (y as f64 + 0.5) / px - o;
            for x in 0..rb {
                let design_x = (x as f64 + 0.5) / px - o;
                let i = y * rb + x;
                let k = self.own[i];
                self.lock_seen(x, y, design_x, design_y, k);
                let ux = self.seen[0] as f64;
                let uy = self.seen[1] as f64;
                let s = self.seen[2] as f64;
                let hgt = self.seen[3] as f64;
                self.fibre_light(design_x, design_y, ux, uy, 0.25 + 0.2 * hgt);
                let nx = self.fib[4] as f64;
                let ny = self.fib[5] as f64;
                let nz = self.fib[6] as f64;
                let nkb = self.fib[7] as f64;
                let xl0 = if x > 0 { i - 1 } else { i };
                let xr0 = if x + 1 < rb { i + 1 } else { i };
                let yu0 = if y > 0 { i - rb } else { i };
                let yd0 = if y + 1 < rb { i + rb } else { i };
                let gx = ((self.pile_h[xr0] as f64 - self.pile_h[xl0] as f64) * px)
                    / ((xr0 as i32 - xl0 as i32).max(1) as f64);
                let gy = ((self.pile_h[yd0] as f64 - self.pile_h[yu0] as f64) * px * rb as f64)
                    / ((yd0 as i32 - yu0 as i32).max(1) as f64);
                let l1 = (nx * nx + ny * ny).sqrt();
                let (t1x, t1y, t2x, t2y, t2z) = if l1 > 1e-3 {
                    (-ny / l1, nx / l1, (-nz * nx) / l1, (-nz * ny) / l1, l1)
                } else {
                    (1.0, 0.0, 0.0, 1.0, 0.0)
                };
                let g1 = (gx * t1x + gy * t1y).clamp(-1.5, 1.5);
                let g2 = (gx * t2x + gy * t2y).clamp(-1.5, 1.5);
                let mut mx = nx - g1 * t1x - g2 * t2x;
                let mut my = ny - g1 * t1y - g2 * t2y;
                let mut mz = nz - g2 * t2z;
                let ml = or1((mx * mx + my * my + mz * mz).sqrt());
                mx /= ml;
                my /= ml;
                mz /= ml;
                let mut kk = 1.0;
                if k >= 0 {
                    let b = k as usize * LP;
                    let tx = self.locks[b + 8] as f64;
                    let ty = self.locks[b + 9] as f64;
                    let tz = self.locks[b + 10] as f64;
                    let fx = 0.9 * tx + 0.44 * nx;
                    let fy = 0.9 * ty + 0.44 * ny;
                    let fz = 0.9 * tz + 0.44 * nz;
                    let fk = (fx * self.k3[0] + fy * self.k3[1] + fz * self.k3[2])
                        / or1((fx * fx + fy * fy + fz * fz).sqrt());
                    kk = 1.0 - PILE_KK
                        + PILE_KK * 1.2_f64.min((1.0 - fk * fk).max(0.0).sqrt() / 0.85);
                }
                let nkp = mx * self.k3[0] + my * self.k3[1] + mz * self.k3[2];
                let h0 = self.pile_h[i] as f64;
                let mut sh = 1.0;
                if nkb > -0.25 {
                    let sin_e = nkb.clamp(0.05, 0.97);
                    let tan_e = sin_e / (1.0 - sin_e * sin_e).sqrt();
                    let mut occ = 0.0;
                    for t in 0..self.nsh {
                        let sx = x as i32 + self.sh_x[t];
                        let sy = y as i32 + self.sh_y[t];
                        if sx < 0 || sy < 0 || sx >= rb as i32 || sy >= rb as i32 {
                            break;
                        }
                        let o = self.pile_h[sy as usize * rb + sx as usize] as f64
                            - h0
                            - self.sh_d[t] as f64 * tan_e;
                        if o > occ {
                            occ = o;
                        }
                    }
                    sh = 1.0 - PILE_SHADOW * smooth(0.0, 0.35 * h_max, occ);
                }
                let mut ao = 1.0 - PILE_AO * smooth(0.0, 0.6 * h_max, self.pile_b[i] as f64 - h0);
                if k >= 0 {
                    ao *= 1.0 - PILE_ROOT * (1.0 - smooth(0.0, 0.35, s));
                }
                let fill_p = PILE_AMB
                    + PILE_FILL * 0.0_f64.max(mx * self.f3[0] + my * self.f3[1] + mz * self.f3[2]);
                let fill_b = PILE_AMB
                    + PILE_FILL * 0.0_f64.max(nx * self.f3[0] + ny * self.f3[1] + nz * self.f3[2]);
                let mut r = (ao * (fill_p + self.key_on(nkp) * sh * kk) * PILE_GAIN)
                    / (fill_b + self.key_on(nkb));
                if k >= 0 {
                    r *= (1.0 + 0.15 * s * s) * (1.0 + 0.08 * (hash2(k as f64, 5.0) - 0.5));
                }
                let crease = self.fib[8] as f64;
                r *= 1.0
                    - PILE_CREASE
                        * crease
                        * (0.6 + 0.4 * smooth(0.0, 0.6 * h_max, self.pile_b[i] as f64 - h0));
                r = 1.0 + (r - 1.0) * self.depth_k;
                let on = self.fib[9] as f64;
                let lq = (fill_b + self.key_on(nkb)) / PILE_LIT;
                let lbs = self.gam_of(lq) * on + 0.75 * (1.0 - on);
                let mut m0 = (if r > 2.4 { 2.4 } else { r }).sqrt();
                if m0 > 1.0 {
                    m0 = 1.0 + (m0 - 1.0) / (1.0 + 2.5 * lbs * (m0 - 1.0));
                }
                let strand = self.strand_buf[i] as f64;
                let mut cast = 0.0;
                if nkb > 0.0 && rb >= 2 {
                    let kx = self.k3[0] - nkb * nx;
                    let ky = self.k3[1] - nkb * ny;
                    let kt = (1.0 - nkb * nkb).sqrt();
                    let reach_s = if kt > 1e-3 {
                        (STRAND_CAST_H * px * 3.0_f64.min(kt / 0.2_f64.max(nkb))) / kt
                    } else {
                        0.0
                    };
                    let hi = rb as f64 - 1.001;
                    let cx = (x as f64 + kx * reach_s).max(0.0).min(hi);
                    let cy = (y as f64 + ky * reach_s).max(0.0).min(hi);
                    let cx0 = js_to_int32(cx).max(0) as usize;
                    let cy0 = js_to_int32(cy).max(0) as usize;
                    if cx0 + 1 < rb && cy0 + 1 < rb {
                        let tx = cx - cx0 as f64;
                        let ty = cy - cy0 as f64;
                        let ci = cy0 * rb + cx0;
                        let caster = (self.strand_buf[ci] as f64 * (1.0 - tx)
                            + self.strand_buf[ci + 1] as f64 * tx)
                            * (1.0 - ty)
                            + (self.strand_buf[ci + rb] as f64 * (1.0 - tx)
                                + self.strand_buf[ci + rb + 1] as f64 * tx)
                                * ty;
                        cast = 0.0_f64.max(caster - 0.0_f64.max(strand)) * smooth(0.0, 0.3, nkb);
                    }
                }
                let fib2 = self.fib[2] as f64;
                let mut m = m0
                    * (1.0
                        + self.strand_k
                            * (strand * (0.45 + 0.55 * fib2) * (0.75 + 0.5 * s)
                                - STRAND_CAST * cast))
                    + 0.04 * (self.fib[10] as f64 - 0.5)
                    + PILE_BIAS * self.depth_k * on;
                let edge_d = (design_x + o)
                    .min(FILM_SPAN - o - design_x)
                    .min(design_y + o)
                    .min(FILM_SPAN - o - design_y);
                let fade_e = if edge_d >= 8.0 {
                    1.0
                } else if edge_d <= 0.0 {
                    0.0
                } else {
                    let t = edge_d / 8.0;
                    t * t * (3.0 - 2.0 * t)
                };
                m = 1.0 + (m - 1.0) * fade_e;
                let k4 = i * 4;
                if m < 1.0 {
                    let a = 255.0 * (((1.0 - m) * lbs) / (lbs - GAP_REF).max(0.4)).min(1.0);
                    self.dark_img[k4 + 3] = u8_round(a);
                } else {
                    self.light_img[k4] = 255;
                    self.light_img[k4 + 1] = 255;
                    self.light_img[k4 + 2] = 255;
                    let lift = (PILE_LIFT * (TIP_TOP - 1.0) * lbs) / (TIP_TOP - lbs);
                    let a = 255.0 * (lift * (m - 1.0) * (1.0 - 0.7 * crease)).min(1.0);
                    self.light_img[k4 + 3] = u8_round(a);
                }
                let lit = (4.0 * (m0 - 1.0)).clamp(-2.0, 2.0);
                let xl = x >> 1;
                let yl = y >> 1;
                if xl < rl && yl < rl {
                    let j = yl * rl + xl;
                    self.lock[j] = (self.lock[j] as f64 + 0.25 * h0) as f32;
                    self.lock_lit[j] = (self.lock_lit[j] as f64 + 0.25 * lit) as f32;
                }
                let tip_k = smooth(-0.4, 1.0, lit) * (0.65 + 0.25 * strand).clamp(0.0, 1.4);
                self.glint_img[k4] = 255;
                self.glint_img[k4 + 1] = 255;
                self.glint_img[k4 + 2] = 255;
                self.back_img[k4] = 255;
                self.back_img[k4 + 1] = 255;
                self.back_img[k4 + 2] = 255;
                self.glint_img[k4 + 3] = u8_round(255.0 * (self.fib[0] as f64 * tip_k).min(1.0));
                self.back_img[k4 + 3] = u8_round(
                    255.0 * (self.fib[1] as f64 * (0.5 + 0.4 * strand).clamp(0.0, 1.6)).min(1.0),
                );
                let on_pile = (0.85 + 0.15 * strand + 0.12 * lit).clamp(0.55, 1.3);
                self.soft_img[k4] = 255;
                self.soft_img[k4 + 1] = 255;
                self.soft_img[k4 + 2] = 255;
                self.soft_img[k4 + 3] =
                    u8_round(255.0 * (0.85 * self.fib[3] as f64 * on_pile).min(1.0));
            }
        }
    }

    fn step_norm(&mut self) {
        let n = self.rl * self.rl;
        if n == 0 {
            return;
        }
        let mut sum = 0.0;
        let mut sum2 = 0.0;
        for v in &self.lock {
            let v = *v as f64;
            sum += v;
            sum2 += v * v;
        }
        let nf = n as f64;
        let mean = sum / nf;
        let sdv = (sum2 / nf - mean * mean).max(1e-9).sqrt();
        for v in &mut self.lock {
            *v = (((*v as f64) - mean) / sdv).clamp(-2.5, 2.5) as f32;
        }
    }

    fn step_upload(&mut self) {
        upload_straight_rgba(&mut self.dark_c.pixmap, &self.dark_img);
        upload_straight_rgba(&mut self.light_c.pixmap, &self.light_img);
        upload_straight_rgba(&mut self.glint_c.pixmap, &self.glint_img);
        upload_straight_rgba(&mut self.soft_c.pixmap, &self.soft_img);
        upload_straight_rgba(&mut self.back_c.pixmap, &self.back_img);
        let e = FILM_M * self.px;
        for canvas in [
            &mut self.dark_c,
            &mut self.light_c,
            &mut self.glint_c,
            &mut self.back_c,
        ] {
            canvas.set_transform(1.0, 0.0, 0.0, 1.0, e, e);
            canvas.set_line_cap(LineCap::Round);
            canvas.set_line_join(LineJoin::Round);
        }
        self.dark_img = Vec::new();
        self.light_img = Vec::new();
        self.glint_img = Vec::new();
        self.back_img = Vec::new();
        self.soft_img = Vec::new();
        self.strand_buf = Vec::new();
    }

    fn step_hairs(&mut self, kind_i: u8, part: u32, parts: u32) {
        let kind = self.kinds[kind_i as usize];
        let mut buckets: Vec<StrokePath> = (0..16).map(|_| StrokePath::new()).collect();
        let mut glints: Vec<StrokePath> = (0..6).map(|_| StrokePath::new()).collect();
        let mut backs: Vec<StrokePath> = (0..6).map(|_| StrokePath::new()).collect();
        let mut shades: Vec<StrokePath> = (0..3).map(|_| StrokePath::new()).collect();
        let mut over = StrokePath::new();
        let area = FILM_SPAN * FILM_SPAN;
        let crowd = if kind.crowd { 2.0 } else { 1.0 };
        let count =
            js_round((area * kind.count * self.k_den * crowd) / parts as f64).max(0.0) as usize;
        let curl = self.curl;
        let px = self.px;
        let k_len = self.k_len;
        for n in 0..count {
            let design_x = self.rand.next() * FILM_SPAN - self.o;
            let design_y = self.rand.next() * FILM_SPAN - self.o;
            let r0 = self.rand.next();
            let r1 = self.rand.next();
            let r2 = self.rand.next();
            let r3 = self.rand.next();
            let r4 = self.rand.next();
            let r5 = self.rand.next();
            let edge = (design_x + self.o)
                .min(FILM_SPAN - self.o - design_x)
                .min(design_y + self.o)
                .min(FILM_SPAN - self.o - design_y);
            if r5 * 8.0 > edge {
                continue;
            }
            if kind.crowd {
                let keep = if self.sd_at(design_x, design_y) > 0.0 {
                    0.5 / self.grid_at(&self.nz_g, design_x, design_y).max(0.25)
                } else {
                    0.5
                };
                if r5 > keep {
                    continue;
                }
            }
            let (fx, fy) = self.dir_at(design_x, design_y);
            let tilt_s = self.grid_at(&self.tilt_g, design_x, design_y);
            let vl = self.grid_at(&self.fore_g, design_x, design_y) / PILE_COS;
            let ang = (r1 - 0.5) * (0.6 + 0.8 * curl) * (1.0 - 0.5 * tilt_s);
            let ca = ang.cos();
            let sa = ang.sin();
            let ux = fx * ca - fy * sa;
            let uy = fx * sa + fy * ca;
            let longer = if r2 < 0.1 { 1.35 + 0.6 * r3 } else { 1.0 };
            let len = (kind.len + kind.vary * r3 * r3)
                * px
                * k_len
                * longer
                * vl
                * (1.0 - kind.end_on + 1.25 * kind.end_on * tilt_s);
            self.fibre_light(
                design_x,
                design_y,
                ux,
                uy,
                0.1 + 0.5 * hash2(n as f64, 7.0 + part as f64),
            );
            let fib2 = self.fib[2] as f64;
            let fib8 = self.fib[8] as f64;
            let fib0 = self.fib[0] as f64;
            let fib1 = self.fib[1] as f64;
            let mut v = 0.22 * self.lock_at(&self.lock, design_x, design_y)
                + 0.3 * self.lock_at(&self.lock_lit, design_x, design_y);
            let lit = r0 < kind.light * (0.4 + 0.6 * fib2);
            if lit {
                v += 0.2 + 0.28 * r4;
            } else if r0 < kind.light + kind.dark {
                v -= 0.2 + 0.28 * r4;
            }
            v += kind.lift;
            v += 0.45 * (fib2 - 0.5) - 0.35 * fib8;
            if v > 0.0 {
                v *= 0.3 + 0.7 * fib2;
            }
            let deepen = 0.15 * (1.0 - fib2);
            let x0 = (design_x + PAD) * px;
            let y0 = (design_y + PAD) * px;
            let b1 = (self.rand.next() - 0.5) * kind.bend * curl * len;
            let b2 = if r4 < 0.3 {
                -0.6 * b1
            } else {
                b1 * (0.6 + 0.8 * self.rand.next())
            };
            let hairs = kind.clump0 + js_to_int32(self.rand.next() * (kind.clump1 + 1.0)) as f64;
            let hairs_n = if hairs <= 0.0 {
                0
            } else {
                hairs.ceil() as usize
            };
            let wc = kind.clump_w * px * 1.5_f64.min(k_len);
            for hk in 0..hairs_n {
                let (off, f, j) = if hairs_n > 1 {
                    (
                        self.rand.next() - 0.5,
                        0.7 + 0.3 * self.rand.next(),
                        (self.rand.next() - 0.5) * 0.3,
                    )
                } else {
                    (0.0, 1.0, 0.0)
                };
                let rx = x0 - uy * off * wc;
                let ry = y0 + ux * off * wc;
                let lk = len * f;
                let e1 = b1 + j * lk;
                let e2 = b2 + j * lk - off * wc * 0.7;
                let c1x = rx + ux * lk * 0.33 - uy * e1;
                let c1y = ry + uy * lk * 0.33 + ux * e1;
                let c2x = rx + ux * lk * 0.67 - uy * e2;
                let c2y = ry + uy * lk * 0.67 + ux * e2;
                let x1 = rx + ux * lk - uy * (b2 - off * wc * 0.75);
                let y1 = ry + uy * lk + ux * (b2 - off * wc * 0.75);
                let sg = if lk < 4.0 {
                    2
                } else if lk < 10.0 {
                    3
                } else {
                    4
                };
                let mut hx = [0.0; 5];
                let mut hy = [0.0; 5];
                for s in 0..=sg {
                    let t = s as f64 / sg as f64;
                    let q = 1.0 - t;
                    let a = q * q * q;
                    let b = 3.0 * q * q * t;
                    let c = 3.0 * q * t * t;
                    let d = t * t * t;
                    hx[s] = a * rx + b * c1x + c * c2x + d * x1;
                    hy[s] = a * ry + b * c1y + c * c2y + d * y1;
                }
                let vh = v + if hairs_n > 1 {
                    (self.rand.next() - 0.5) * 0.25
                } else {
                    0.0
                };
                let rise = 0.3 + 0.15 * fib2;
                for s in 0..sg {
                    let bk = shade_of(
                        vh - 0.3 - deepen * (1.0 - s as f64 / sg as f64)
                            + (rise * (s as f64 + 0.5)) / sg as f64,
                    );
                    buckets[bk].move_to(hx[s], hy[s]);
                    buckets[bk].line_to(hx[s + 1], hy[s + 1]);
                }
                if lit && kind.over > 0.0 && sg >= 1 {
                    over.move_to(hx[1], hy[1]);
                    for s in 2..=sg {
                        over.line_to(hx[s], hy[s]);
                    }
                }
                if hk == 0 && kind.shadow > 0.0 && fib2 > 0.25 && v > -0.2 {
                    let kk = kind.shadow * px * 1.0_f64.min(k_len);
                    let si = {
                        let idx = js_to_int32((fib2 - 0.25) * 4.0);
                        if idx <= 0 { 0 } else { (idx as usize).min(2) }
                    };
                    for s in 1..=sg {
                        let t = s as f64 / sg as f64;
                        let ox = -self.lx * kk * t;
                        let oy = -self.ly * kk * t + 0.3 * kk * t;
                        if s == 1 {
                            shades[si].move_to(hx[0], hy[0]);
                        }
                        shades[si].line_to(hx[s] + ox, hy[s] + oy);
                    }
                }
                let h = sg >> 1;
                if fib0 > 0.3 {
                    let gb = glint_of(fib0);
                    glints[gb].move_to(hx[h], hy[h]);
                    for s in (h + 1)..=sg {
                        glints[gb].line_to(hx[s], hy[s]);
                    }
                }
                if fib1 > 0.1 {
                    let bb = glint_of(fib1);
                    backs[bb].move_to(hx[h], hy[h]);
                    for s in (h + 1)..=sg {
                        backs[bb].line_to(hx[s], hy[s]);
                    }
                }
            }
        }
        let short_k = 1.0_f64.min(k_len).powf(kind.short);
        let lw = 0.45_f64.max(kind.width * px * (0.5 + 0.5 * short_k));
        let buckets: Vec<Option<Path>> = buckets.into_iter().map(StrokePath::finish).collect();
        let glints: Vec<Option<Path>> = glints.into_iter().map(StrokePath::finish).collect();
        let backs: Vec<Option<Path>> = backs.into_iter().map(StrokePath::finish).collect();
        let shades: Vec<Option<Path>> = shades.into_iter().map(StrokePath::finish).collect();
        let over = over.finish();
        if kind.over > 0.0 {
            self.dark_c.set_blend_mode(BlendMode::DestinationOut);
            self.dark_c.set_line_width(lw);
            self.dark_c.set_global_alpha(kind.over * short_k);
            self.dark_c.set_stroke_css("#000");
            paint_stroke(&mut self.dark_c, &over);
            self.dark_c.set_blend_mode(BlendMode::SourceOver);
        }
        if kind.shadow > 0.0 {
            self.dark_c.set_line_width(lw * kind.shadow_w);
            self.dark_c.set_stroke_css("#000");
            for i in 0..3 {
                self.dark_c
                    .set_global_alpha(0.03 * (i as f64 + 1.0) * short_k);
                paint_stroke(&mut self.dark_c, &shades[i]);
            }
        }
        for i in 0..16 {
            let vv = FUR_MID + ((i as f64 / 15.0) * 2.0 - 1.0) * kind.spread;
            let alpha = kind.alpha
                * short_k
                * if vv < FUR_MID {
                    kind.deep * (1.0 - vv / FUR_MID)
                } else {
                    (vv - FUR_MID) / (255.0 - FUR_MID)
                };
            if vv < FUR_MID {
                self.dark_c.set_line_width(lw);
                self.dark_c.set_global_alpha(alpha);
                self.dark_c.set_stroke_css("#000");
                paint_stroke(&mut self.dark_c, &buckets[i]);
            } else {
                self.light_c.set_line_width(lw);
                self.light_c.set_global_alpha(alpha);
                self.light_c.set_stroke_css("#fff");
                paint_stroke(&mut self.light_c, &buckets[i]);
            }
        }
        self.glint_c.set_line_width(lw);
        self.glint_c.set_stroke_css("#fff");
        self.back_c.set_line_width(lw);
        self.back_c.set_stroke_css("#fff");
        for i in 0..6 {
            self.glint_c
                .set_global_alpha((0.4 * (i as f64 + 0.5)) / 6.0);
            self.back_c.set_global_alpha((0.8 * (i as f64 + 0.5)) / 6.0);
            paint_stroke(&mut self.glint_c, &glints[i]);
            paint_stroke(&mut self.back_c, &backs[i]);
        }
        self.dark_c.set_global_alpha(1.0);
        self.light_c.set_global_alpha(1.0);
        self.glint_c.set_global_alpha(1.0);
        self.back_c.set_global_alpha(1.0);
    }

    fn step_fringe(&mut self, part: u32) {
        if part == 0 {
            let n = self.form.n as usize;
            self.cells.clear();
            let sd = &self.form.sd;
            let n2 = n.saturating_mul(n).min(sd.len());
            for i in 0..n2 {
                let v = sd[i] as f64;
                if v > -1.6 && v < 2.2 {
                    self.cells.push(i);
                }
            }
            let cu = if n == 0 { 0.0 } else { SPAN / n as f64 };
            let tries = self.cells.len() as f64
                * cu
                * cu
                * 24.0
                * (0.3 + 1.4 * self.fuzz)
                * self.k_den.sqrt()
                * (0.5 + 0.5 * 1.0_f64.min(self.r as f64 / 512.0))
                * (1.0 - 0.55 * self.fine);
            self.edge_tries = js_round(tries).max(0.0) as usize;
        }
        if self.cells.is_empty() || self.form.n == 0 {
            return;
        }
        let n = self.form.n as usize;
        let cu = SPAN / n as f64;
        let n0 = self.edge_tries.saturating_mul(part as usize) / 2;
        let n1 = self.edge_tries.saturating_mul(part as usize + 1) / 2;
        for _ in n0..n1 {
            let idx = js_to_int32(self.rand.next() * self.cells.len() as f64).max(0) as usize;
            if idx >= self.cells.len() {
                continue;
            }
            let c = self.cells[idx];
            let design_x = ((c % n) as f64 + self.rand.next()) * cu - PAD;
            let design_y = ((c / n) as f64 + self.rand.next()) * cu - PAD;
            let d0 = self.sd_at(design_x, design_y);
            if d0 < -1.2 || d0 > 1.8 {
                continue;
            }
            let d = d0 + self.tuft_at(design_x, design_y);
            if d < -0.2 || d > 1.2 {
                continue;
            }
            let (fx, fy) = self.flow(design_x, design_y);
            let (ox, oy) = self.outward(design_x, design_y);
            let ang = (self.rand.next() - 0.5) * 1.1;
            let ca = ang.cos();
            let sa = ang.sin();
            let mut ux = fx * ca - fy * sa;
            let mut uy = fx * sa + fy * ca;
            let r1 = self.rand.next();
            let hang =
                1.0 + 0.5 * self.gravity * 0.0_f64.max(oy) - 0.35 * self.gravity * 0.0_f64.max(-oy);
            let mut fl =
                (0.3 + 1.5 * r1 * r1 * r1) * self.px * self.k_len * (0.5 + self.fuzz) * hang;
            let x0 = (design_x + PAD) * self.px;
            let y0 = (design_y + PAD) * self.px;
            let mut bend = (self.rand.next() - 0.5) * 1.17 * self.curl * fl;
            if oy < -0.3 && self.rand.next() < 0.06 * (0.3 + self.fuzz) {
                ux = ox * 0.8 + (self.rand.next() - 0.5) * 0.6;
                uy = oy * 0.8;
                fl *= 1.2;
                let sign = if self.rand.next() < 0.5 { -1.0 } else { 1.0 };
                bend = sign * (0.25 + 0.3 * self.gravity) * fl;
            }
            let qx = x0 + ux * fl * 0.5 - uy * bend;
            let qy = y0 + uy * fl * 0.5 + ux * bend;
            let ex = x0 + ux * fl;
            let ey = y0 + uy * fl + self.gravity * fl * 0.25;
            self.fringe.move_to(x0, y0);
            self.fringe.quad_to(qx, qy, ex, ey);
            self.fibre_light(design_x, design_y, ux, uy, 0.3);
            if (self.fib[1] as f64) > 0.08 {
                let bi = glint_of(self.fib[1] as f64);
                self.fringe_back[bi].move_to(x0, y0);
                self.fringe_back[bi].quad_to(qx, qy, ex, ey);
            }
        }
    }

    fn step_tufts(&mut self) {
        let n = self.form.n as usize;
        if n == 0 || self.cells.is_empty() {
            return;
        }
        let cu = SPAN / n as f64;
        let band = self.cells.len() as f64 * cu * cu;
        let around = band / 3.8;
        let count = js_round(
            (around / (0.45 + 0.3 * self.k_len))
                * (0.3 + 0.8 * self.fuzz)
                * self.k_den.sqrt()
                * (1.0 + 0.6 * self.fine),
        )
        .max(0.0) as i32;
        let mut n_ok = 0i32;
        let mut tries = 0i32;
        let max_tries = count.saturating_mul(6);
        while n_ok < count && tries < max_tries {
            tries += 1;
            let idx = js_to_int32(self.rand.next() * self.cells.len() as f64).max(0) as usize;
            if idx >= self.cells.len() {
                continue;
            }
            let c = self.cells[idx];
            let design_x = ((c % n) as f64 + self.rand.next()) * cu - PAD;
            let design_y = ((c / n) as f64 + self.rand.next()) * cu - PAD;
            let r0 = self.rand.next();
            let r1 = self.rand.next();
            let r2 = self.rand.next();
            let r3 = self.rand.next();
            let d0 = self.sd_at(design_x, design_y);
            if d0 < -0.5 || d0 > 1.6 {
                continue;
            }
            let d = d0 + self.tuft_at(design_x, design_y);
            if d < 0.0 || d > 0.9 {
                continue;
            }
            if self.lock_at(&self.lock, design_x, design_y) < -0.4 + 0.8 * r3 {
                continue;
            }
            n_ok += 1;
            let (fx, fy) = self.flow(design_x, design_y);
            let (ox, oy) = self.outward(design_x, design_y);
            let a = (r0 - 0.5) * 0.8 * (0.5 + self.curl);
            let ca = a.cos();
            let sa = a.sin();
            let sx0 = 0.75 * ox + 0.25 * fx;
            let sy0 = 0.75 * oy + 0.25 * fy;
            let ex0 = 0.55 * fx + 0.45 * ox;
            let ey0 = 0.55 * fy + 0.45 * oy + 0.35 * self.gravity;
            let sl = or1(sx0.hypot(sy0));
            let el = or1(ex0.hypot(ey0));
            let sx = (sx0 * ca - sy0 * sa) / sl;
            let sy = (sx0 * sa + sy0 * ca) / sl;
            let ex = (ex0 * ca - ey0 * sa) / el;
            let ey = (ex0 * sa + ey0 * ca) / el;
            let hang =
                1.0 + 0.5 * self.gravity * 0.0_f64.max(oy) - 0.35 * self.gravity * 0.0_f64.max(-oy);
            let lt = (0.5 + 1.1 * r1 * r1) * self.px * self.k_len * (0.5 + self.fuzz) * hang;
            let wt = (0.3 + 0.35 * r2) * self.px * 1.5_f64.min(self.k_len);
            let x0 = (design_x + PAD) * self.px;
            let y0 = (design_y + PAD) * self.px;
            let cx = x0 + sx * lt * 0.5;
            let cy = y0 + sy * lt * 0.5;
            let tx = x0 + (sx * 0.4 + ex * 0.6) * lt;
            let ty = y0 + (sy * 0.4 + ey * 0.6) * lt;
            let qx = -sy;
            let qy = sx;
            self.fibre_light(design_x, design_y, ex, ey, 0.4);
            let fib1 = self.fib[1] as f64;
            let fib2 = self.fib[2] as f64;
            let back_i = if fib1 > 0.08 {
                Some(glint_of(fib1))
            } else {
                None
            };
            let tip_i = {
                let idx = js_to_int32(fib2 * 3.0);
                if idx <= 0 { 0 } else { (idx as usize).min(2) }
            };
            let hairs = 4 + js_to_int32(self.rand.next() * 5.0).max(0) as usize;
            for _k in 0..hairs {
                let off = self.rand.next() - 0.5;
                let f = 0.7 + 0.3 * self.rand.next();
                let w = (self.rand.next() - 0.5) * 0.35;
                let hx0 = x0 + qx * off * wt;
                let hy0 = y0 + qy * off * wt;
                let hcx = cx + qx * off * wt * 0.6;
                let hcy = cy + qy * off * wt * 0.6;
                let htx = hx0 + (tx + qx * (off * 0.25 + w) * wt - hx0) * f;
                let hty = hy0 + (ty + qy * (off * 0.25 + w) * wt - hy0) * f;
                self.tufts.move_to(hx0, hy0);
                self.tufts.quad_to(hcx, hcy, htx, hty);
                let mx = 0.25 * hx0 + 0.5 * hcx + 0.25 * htx;
                let my = 0.25 * hy0 + 0.5 * hcy + 0.25 * hty;
                let cqx = 0.5 * (hcx + htx);
                let cqy = 0.5 * (hcy + hty);
                self.tuft_tips[tip_i].move_to(mx, my);
                self.tuft_tips[tip_i].quad_to(cqx, cqy, htx, hty);
                if let Some(bi) = back_i {
                    self.tuft_back[bi].move_to(mx, my);
                    self.tuft_back[bi].quad_to(cqx, cqy, htx, hty);
                }
            }
        }
    }

    fn step_stroke_edges(&mut self) {
        let fringe = std::mem::replace(&mut self.fringe, StrokePath::new()).finish();
        let tufts = std::mem::replace(&mut self.tufts, StrokePath::new()).finish();
        let fringe_back: Vec<Option<Path>> = std::mem::take(&mut self.fringe_back)
            .into_iter()
            .map(StrokePath::finish)
            .collect();
        let tuft_tips: Vec<Option<Path>> = std::mem::take(&mut self.tuft_tips)
            .into_iter()
            .map(StrokePath::finish)
            .collect();
        let tuft_back: Vec<Option<Path>> = std::mem::take(&mut self.tuft_back)
            .into_iter()
            .map(StrokePath::finish)
            .collect();
        let tw = 0.4_f64.max(0.11 * self.px);
        self.dark_c.set_line_width(tw);
        self.light_c.set_line_width(tw);
        self.back_c.set_line_width(tw);
        self.dark_c.set_blend_mode(BlendMode::DestinationOut);
        self.dark_c.set_global_alpha(0.6);
        self.dark_c.set_stroke_css("#000");
        self.light_c.set_stroke_css("#fff");
        for i in 0..tuft_tips.len() {
            paint_stroke(&mut self.dark_c, &tuft_tips[i]);
            self.light_c.set_global_alpha(0.08 + 0.11 * i as f64);
            paint_stroke(&mut self.light_c, &tuft_tips[i]);
        }
        self.dark_c.set_blend_mode(BlendMode::SourceOver);
        self.back_c.set_stroke_css("#fff");
        for i in 0..tuft_back.len() {
            self.back_c.set_global_alpha((i as f64 + 0.5) / 6.0);
            paint_stroke(&mut self.back_c, &tuft_back[i]);
        }
        self.dark_c.set_global_alpha(1.0);
        self.light_c.set_global_alpha(1.0);
        self.back_c.set_global_alpha(1.0);
        let fw = 0.35_f64.max(0.08 * self.px);
        self.light_c.set_line_width(fw);
        self.dark_c.set_line_width(fw);
        self.light_c
            .set_global_alpha(0.5 * ((140.0 - FUR_MID) / (255.0 - FUR_MID)));
        self.light_c.set_stroke_css("#fff");
        paint_stroke(&mut self.light_c, &fringe);
        self.light_c.set_global_alpha(1.0);
        self.back_c.set_line_width(fw);
        self.back_c.set_stroke_css("#fff");
        for i in 0..fringe_back.len() {
            self.back_c.set_global_alpha((i as f64 + 0.5) / 6.0);
            paint_stroke(&mut self.back_c, &fringe_back[i]);
        }
        self.back_c.set_global_alpha(1.0);
        self.dark_c.set_blend_mode(BlendMode::DestinationOut);
        self.dark_c.set_global_alpha(0.7);
        self.dark_c.set_stroke_css("#000");
        paint_stroke(&mut self.dark_c, &fringe);
        self.dark_c.set_global_alpha(1.0);
        self.dark_c.set_blend_mode(BlendMode::SourceOver);
        self.fringe_p = fringe;
        self.tufts_p = tufts;
    }

    fn step_halo(&mut self, y0: u32, y1: u32) {
        let r = self.r as usize;
        if r == 0 {
            return;
        }
        let px = self.px;
        let h_out = 0.5 + 1.2 * self.fuzz;
        let reach_out = h_out + self.tuft_k;
        let halo_a = HALO_A * (1.0 - 0.6 * self.fine);
        let halo_streak = HALO_STREAK * (1.0 + 1.2 * self.fine);
        let hn = js_round(HALO_HAIR * self.k_len * px).max(1.0) as i32;
        let hs1 = hn + 1;
        let w_sd = (1.0_f64 / 7.0 - 1.0 / 16.0).sqrt();
        let hsd = (w_sd
            * (1.0 + (hn as f64 * (2.0 * hn as f64 + 1.0)) / (3.0 * hs1 as f64)).sqrt())
            / hs1 as f64;
        let off = js_round(FILM_M * px);
        let rb = self.rb as i32;
        for y in y0 as usize..y1 as usize {
            let design_y = (y as f64 + 0.5) / px - PAD;
            for x in 0..r {
                let design_x = (x as f64 + 0.5) / px - PAD;
                let k = (y * r + x) * 4;
                self.mask_img[k] = 255;
                self.mask_img[k + 1] = 255;
                self.mask_img[k + 2] = 255;
                self.haze_img[k] = 255;
                self.haze_img[k + 1] = 255;
                self.haze_img[k + 2] = 255;
                let d0 = self.sd_at(design_x, design_y);
                if d0 >= self.tuft_k + HALO_CORE {
                    self.mask_img[k + 3] = 255;
                    self.core_img[k + 3] = 255;
                    continue;
                }
                let d = if d0 <= -reach_out {
                    d0
                } else {
                    d0 + self.tuft_at(design_x, design_y)
                };
                if d >= 0.0 {
                    self.mask_img[k + 3] = 255;
                    self.core_img[k + 3] = u8_round(255.0 * smooth(0.25 * HALO_CORE, HALO_CORE, d));
                    continue;
                }
                let z = 1.0_f64.min(-d / (2.0 * h_out));
                let nb = self.grid_at(&self.nx_g, design_x, design_y) * self.b3[0]
                    + self.grid_at(&self.ny_g, design_x, design_y) * self.b3[1];
                let za = 255.0
                    * HALO_LIFT
                    * z
                    * z
                    * (3.0 - 2.0 * z)
                    * (0.45 + 0.55 * smooth(-0.3, 0.6, nb));
                self.haze_img[k + 3] = u8_round(za);
                if d <= -h_out {
                    continue;
                }
                let o = 1.0 + d / h_out;
                let f = o * o * (3.0 - 2.0 * o);
                let (ux, uy) = self.free_dir_at(design_x, design_y);
                let mut acc = 0.0;
                for t in -hn..=hn {
                    let sx = js_round_i32(x as f64 + off + ux * t as f64);
                    let sy = js_round_i32(y as f64 + off + uy * t as f64);
                    let sample = if sx < 0 || sy < 0 || sx >= rb || sy >= rb {
                        W_MEAN
                    } else {
                        self.white[sy as usize * self.rb + sx as usize] as f64
                    };
                    acc += sample * (1.0 - (t.abs() as f64) / hs1 as f64);
                }
                let strand = (acc / hs1 as f64 - W_MEAN) / hsd;
                let m = (f * (halo_a + halo_streak * strand)).clamp(0.0, 1.0);
                self.mask_img[k + 3] = u8_round(255.0 * m);
            }
        }
    }

    fn step_finish(&mut self) -> bool {
        let fringe = self.fringe_p.take();
        let tufts = self.tufts_p.take();
        let mut haze_c = make_canvas(self.r);
        upload_straight_rgba(&mut haze_c.pixmap, &self.haze_img);
        let haze = Some(take_pm(&mut haze_c));
        upload_straight_rgba(&mut self.mask_c.pixmap, &self.mask_img);
        self.mask_c.set_line_cap(LineCap::Round);
        self.mask_c.set_line_width(0.35_f64.max(0.08 * self.px));
        self.mask_c.set_stroke_css("rgba(255,255,255,0.45)");
        paint_stroke(&mut self.mask_c, &fringe);
        self.mask_c.set_line_width(0.4_f64.max(0.11 * self.px));
        self.mask_c.set_stroke_css("rgba(255,255,255,0.8)");
        paint_stroke(&mut self.mask_c, &tufts);
        let mut core_c = make_canvas(self.r);
        upload_straight_rgba(&mut core_c.pixmap, &self.core_img);
        let core = Some(take_pm(&mut core_c));
        let cache_key = NEXT_FUR.fetch_add(1, Ordering::Relaxed);
        if cache_key == 0 {
            // 0 means "do not share". Skip it if the counter wraps.
            let _ = NEXT_FUR.fetch_add(1, Ordering::Relaxed);
        }
        let cache_key = if cache_key == 0 { 1 } else { cache_key };
        let fur = Fur {
            r: self.r,
            mask: Some(take_pm(&mut self.mask_c)),
            haze,
            cut: None,
            core,
            dark: Some(take_pm(&mut self.dark_c)),
            light: Some(take_pm(&mut self.light_c)),
            glint: Some(take_pm(&mut self.glint_c)),
            back: Some(take_pm(&mut self.back_c)),
            back_soft: Some(take_pm(&mut self.soft_c)),
            cache_key,
        };
        file_fur(&self.id, fur);
        true
    }
}

fn lut_index(lut: &[f32], q: f64) -> f64 {
    let i = js_to_int32(q * (POW_N as f64));
    let i = if i <= 0 { 0 } else { (i as usize).min(POW_N) };
    lut.get(i).copied().unwrap_or(0.0) as f64
}

fn take_pm(canvas: &mut Canvas2D) -> Pixmap {
    std::mem::replace(&mut canvas.pixmap, Pixmap::new(1, 1).expect("pixmap"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::BotAvatarType;

    fn near(a: f64, b: f64) {
        let d = (a - b).abs();
        assert!(d <= 1e-12, "{a} vs {b} (diff {d})");
    }

    fn disk_form() -> Form {
        let n = 24u32;
        let mut sd = vec![0f32; (n * n) as usize];
        let mut h = vec![0f32; (n * n) as usize];
        let cu = SPAN / n as f64;
        for y in 0..n {
            for x in 0..n {
                let px = (x as f64 + 0.5) * cu - PAD;
                let py = (y as f64 + 0.5) * cu - PAD;
                let d = 40.0 - (px - 50.0).hypot(py - 50.0);
                let i = (y * n + x) as usize;
                sd[i] = d as f32;
                h[i] = d.max(0.0) as f32;
            }
        }
        Form {
            id: 0,
            n,
            i00: Vec::new(),
            wx: Vec::new(),
            wy: Vec::new(),
            ao: Vec::new(),
            sd,
            h,
        }
    }

    fn sample_material(highlight: f64, rim: f64, shine: f64) -> Material {
        Material {
            shadow: 0.5,
            highlight,
            spread: 0.4,
            rim,
            vivid: Some(0.2),
            front: None,
            shine: Some(shine),
            back_soft: Some(0.5),
            sheen: None,
        }
    }

    #[test]
    fn two_form_tiers_bake_two_furs() {
        let lights = stock_lights(-1.0, 0.0);
        let bin = light_bin(-1.0, 0.0);
        let small = fur_key("blob", 9.75, 192, 64, bin, &FUR_STOCK, &lights);
        let large = fur_key("blob", 9.75, 192, 96, bin, &FUR_STOCK, &lights);
        assert_ne!(small, large);
        assert!(small.starts_with("blob|10|") && large.starts_with("blob|10|"));
    }

    #[test]
    fn fur_tier_matches_thresholds() {
        assert_eq!(fur_tier(0.0), 192);
        assert_eq!(fur_tier(200.0), 192);
        assert_eq!(fur_tier(200.1), 320);
        assert_eq!(fur_tier(360.0), 320);
        assert_eq!(fur_tier(360.1), 512);
    }

    #[test]
    fn js_numbers_match_the_reference() {
        assert_eq!(hash2(0.0, 0.0), 0.0);
        near(hash2(1.0, -2.0), 0.96651594201102853);
        near(hash2(10.5, 3.2), 0.53729687142185867);
        near(hash2(914.0, 42.0), 0.065625235904008150);
        near(hash3(1.0, 2.0, 3.0), 0.5932219938840717);
        near(hash3(-4.0, 0.5, 8.0), 0.5727988907601684);
        let mut g = Rng::new((8u32).wrapping_mul(7919).wrapping_add(17));
        let got = [g.next(), g.next(), g.next(), g.next(), g.next()];
        let expect = [
            0.13338673650287092,
            0.9342705719172955,
            0.9939278818201274,
            0.6533331435639411,
            0.5664242869243026,
        ];
        for (a, b) in got.iter().zip(expect) {
            near(*a, b);
        }
        let n2 = Noise2 {
            scale: 3.4,
            seed: 21.0,
        };
        near(n2.at(10.0, 20.0), 0.10061892480082987);
        near(n2.at(-3.0, 0.2), 0.4432759098879347);
        near(n2.at(50.0, 50.0), 0.23437703655085584);
        let n3 = Noise3 {
            scale: 1.6 * (1.5 + 1.6),
            seed: 79.0,
        };
        near(n3.at(10.0, 20.0, 1.0), 0.10225405524875414);
        near(n3.at(-1.0, 4.0, 0.3), 0.9302543071000279);
        let field = [0f32, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0];
        near(bilerp(&field, 3, 0.5, 0.5), 2.0);
        near(bilerp(&field, 3, -1.0, 5.0), 6.0);
        near(bilerp(&field, 3, 2.0, 1.25), 5.75);
        assert_eq!(js_to_fixed(0.25, 1), "0.3");
        assert_eq!(js_to_fixed(-0.25, 1), "-0.3");
        assert_eq!(js_to_fixed(1.005, 2), "1.00");
        assert_eq!(js_to_fixed(0.0, 2), "0.00");
        assert_eq!(style_id(&FUR_STOCK), "1.00,1.60,0.90,0.40,0.70,0.90");
        let styled = FurStyle {
            length: 1.005,
            density: 0.1,
            fuzz: 2.0 / 3.0,
            clumps: -0.5,
            curl: 1.225,
            gravity: 0.0,
        };
        assert_eq!(style_id(&styled), "1.00,0.10,0.67,-0.50,1.23,0.00");
        assert_eq!(light_bin(-1.0, 0.0), 12);
        assert_eq!(light_bin(0.0, 1.0), 6);
        assert_eq!(light_bin(1.0, 1.0), 3);
        assert_eq!(light_bin(0.0, -1.0), -6);
        assert_eq!(light_bin(-0.2, 0.9), 7);
        let back = back_from_key(-1.0, 0.0);
        near(back[0], 0.49828387585345851);
        near(back[1], -0.86701394398501774);
        assert_eq!(lights_id(&stock_lights(-1.0, 0.0)), "-12|16");
        let key = fabric_key([-1.0, 0.0, 0.0], [0.0, 0.0, 1.0], KEY_EL);
        near(key[0], -0.84804809615642596);
        near(key[1], 0.0);
        near(key[2], 0.52991926423320490);
        let fb = fabric_back([back[0], back[1], 0.0], [0.0, 0.0, 1.0]);
        near(fb[0], 0.46857431052740856);
        near(fb[1], -0.81531930031769084);
        near(fb[2], -0.34013608165443804);
        assert_eq!(smooth(0.0, 1.0, 0.5), 0.5);
        let nrm = norm3([0.0, 3.0, 4.0]);
        near(nrm[0], 0.0);
        near(nrm[1], 0.6);
        near(nrm[2], 0.8);
    }

    #[test]
    fn films_cache_and_cut_and_every_kind() {
        let key = NEXT_FUR.fetch_add(1, Ordering::Relaxed);
        let mut px = Pixmap::new(4, 4).expect("pixmap");
        px.data_mut()[0] = 255;
        px.data_mut()[3] = 200;
        let mut fur = Fur::empty();
        fur.cache_key = key;
        fur.dark = Some(px.clone());
        fur.light = Some(px.clone());
        fur.glint = Some(px.clone());
        fur.back = Some(px.clone());
        fur.back_soft = Some(px);
        let gaps0 = GAP_BUILDS.load(Ordering::Relaxed);
        let a = gap_film(&fur, "#336699", 0.25).expect("gap");
        let b = gap_film(&fur, "#336699", 0.25).expect("gap");
        assert_eq!(a.data(), b.data());
        assert_eq!(GAP_BUILDS.load(Ordering::Relaxed), gaps0 + 1);
        // Gap map holds 4. The fifth distinct colour clears it, so the first colour bakes again.
        for i in 1..4 {
            gap_film(&fur, &format!("#11223{i}"), 0.0).expect("gap");
        }
        assert_eq!(GAP_BUILDS.load(Ordering::Relaxed), gaps0 + 4);
        gap_film(&fur, "#445566", 0.0).expect("gap");
        let again = gap_film(&fur, "#336699", 0.25).expect("gap");
        assert_eq!(again.data(), a.data());
        assert_eq!(GAP_BUILDS.load(Ordering::Relaxed), gaps0 + 6);
        let tips0 = TIP_BUILDS.load(Ordering::Relaxed);
        let t1 = tip_film(&fur, "#cc8844", 0.4).expect("tip");
        let t2 = tip_film(&fur, "#cc8844", 0.4).expect("tip");
        assert_eq!(t1.data(), t2.data());
        assert_eq!(TIP_BUILDS.load(Ordering::Relaxed), tips0 + 1);
        let sheen0 = SHEEN_BUILDS.load(Ordering::Relaxed);
        assert!(sheen_film(&fur, "#cc8844", &sample_material(0.0, 0.0, 0.0)).is_none());
        assert_eq!(SHEEN_BUILDS.load(Ordering::Relaxed), sheen0);
        let s1 = sheen_film(&fur, "#cc8844", &sample_material(0.8, 0.4, 1.0)).expect("sheen");
        let s2 = sheen_film(&fur, "#cc8844", &sample_material(0.8, 0.4, 1.0)).expect("sheen");
        assert_eq!(s1.data(), s2.data());
        assert_eq!(SHEEN_BUILDS.load(Ordering::Relaxed), sheen0 + 1);
        let mut mask = Pixmap::new(2, 2).expect("mask");
        mask.data_mut()[3] = 255;
        mask.data_mut()[7] = 0;
        let mut cut_fur = Fur::empty();
        cut_fur.mask = Some(mask);
        let cut = fur_cut(&mut cut_fur).expect("cut");
        assert_eq!(cut.data()[3], 0);
        assert_eq!(cut.data()[7], 255);
        assert_eq!(cut.data()[4], 0);
        let again = fur_cut(&mut cut_fur).expect("cut");
        assert_eq!(cut.data(), again.data());
        let mut empty = Fur::empty();
        assert!(gap_film(&empty, "#888", 0.0).is_none());
        assert!(tip_film(&empty, "#888", 0.0).is_none());
        assert!(sheen_film(&empty, "#888", &sample_material(1.0, 1.0, 1.0)).is_none());
        assert!(fur_cut(&mut empty).is_none());
        for kind in BotAvatarType::ALL {
            let lights = stock_lights(-1.0, 0.0);
            let id = fur_key(
                kind.as_str(),
                9.75,
                192,
                96,
                light_bin(-1.0, 0.0),
                &FUR_STOCK,
                &lights,
            );
            assert!(id.starts_with(kind.as_str()));
            let _ = style_id(&FUR_STOCK);
            let mut bare = Fur::empty();
            assert!(gap_film(&bare, "#888888", 0.0).is_none());
            assert!(tip_film(&bare, "#888888", 0.2).is_none());
            assert!(sheen_film(&bare, "#888888", &sample_material(0.2, 0.2, 0.2)).is_none());
            assert!(fur_cut(&mut bare).is_none());
        }
    }

    #[test]
    fn fur_for_caches_one_bake() {
        let form = disk_form();
        let lights = stock_lights(-1.0, 0.0);
        let before = BAKES.load(Ordering::Relaxed);
        let fur = fur_for(
            &form,
            "disk-cache",
            9.75,
            64.0,
            -1.0,
            0.0,
            &FUR_STOCK,
            &lights,
        );
        assert_eq!(fur.r, 192);
        assert_eq!(fur.dark.as_ref().map(|p| p.width()), Some(264));
        assert!(fur.mask.is_some());
        assert!(fur.cache_key != 0);
        let again = fur_for(
            &form,
            "disk-cache",
            9.75,
            64.0,
            -1.0,
            0.0,
            &FUR_STOCK,
            &lights,
        );
        assert_eq!(BAKES.load(Ordering::Relaxed), before + 1);
        assert_eq!(again.cache_key, fur.cache_key);
        assert_eq!(
            again.dark.as_ref().unwrap().data(),
            fur.dark.as_ref().unwrap().data()
        );
        assert_eq!(
            again.mask.as_ref().unwrap().data(),
            fur.mask.as_ref().unwrap().data()
        );
        let mut owned = again;
        assert!(fur_cut(&mut owned).is_some());
    }
}
