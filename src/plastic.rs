//! Core of `plastic.ts`: the form, the matcap, and a synchronous bake cache.
//!
//! Ports these `plastic.ts` ranges:
//! - 26–38 `PAD`, `SPAN`, `M`, `MATCAP_SIZE`, `MM`, `CONIC_STOPS`, `INF`, `EL`, `E_XY`, `E_Z`
//! - 40 `V3`
//! - 44–58 `Form`; 62–88 `edt1d`; 91–111 `edt2d`; 114–128 `blur5`; 133–185 `poisson`
//! - 187–188 `DX`, `DY`, `DL`; 196–288 `buildForm`
//! - 292–319 `makeCanvas`, `ctx2d`, `rasterize`
//! - 325–455 form cache: `pathId`, `pump`, `schedule`, `hurry`, `bakesPending`, `idle`,
//!   `land`, `onLanded`, `takeMissed`, `formFor`, `warmPlastic`, `tierFor`
//!   (idle queue collapsed into a synchronous cache: work runs inside the call,
//!   `bakesPending` is false once it returns, `takeMissed` stays clear)
//! - 467–558 `Relief`, `reliefFor`, `surfaceAt`
//! - 562–666 `linearColor`, `tone`, `srgb`, `powLut`, `Material`, `vivify`, `Frame`, `ENV`, `WARM`, `smooth`, `soft`
//! - 669–797 `buildMatcap`, `sampleMatcap`, `Lift`, `edgeLift`, `shadeTexels`, `FRONT_FADE`
//! - 2841–2844 `norm3` (shared with fur and the cap)
#![allow(dead_code, unused_variables)]

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use tiny_skia::{Path, PathSegment, Point};

use crate::canvas::Canvas2D;
use crate::color::parse_color;
use crate::plastic_fur::fur_ready;

pub use crate::mesh::V3;
pub use crate::plastic_cap::{Rig, build_fabric_matcap, cap_frame, draw_plastic_cap, mul_affine};
pub use crate::plastic_fur::{
    FUR_STOCK, Fur, FurLights, FurStyle, back_from_key, fur_for, stock_lights,
};

/// Texture covers design units `[-PAD, 100 + PAD]`. `plastic.ts` line 26.
pub const PAD: f64 = 3.0;
/// `plastic.ts` line 27.
pub const SPAN: f64 = 100.0 + 2.0 * PAD;
/// Matcap side. `plastic.ts` lines 29–31.
pub const MATCAP_SIZE: u32 = 64;
pub(crate) const M: u32 = MATCAP_SIZE;
pub(crate) const MM: usize = (MATCAP_SIZE as usize) * (MATCAP_SIZE as usize);
/// Stops on the conic side gradients. `plastic.ts` line 34.
pub(crate) const CONIC_STOPS: usize = 24;
pub(crate) const INF: f64 = 1e12;
/// Light elevation off the screen plane, radians. `plastic.ts` lines 37–38.
pub(crate) const EL: f64 = (48.0 * std::f64::consts::PI) / 180.0;
pub(crate) const E_XY: f64 = 0.6691306063588582;
pub(crate) const E_Z: f64 = 0.7431448254773942;
/// Forced idle slice and hurry slice, kept for the port. Sync bakes do not slice on them.
pub(crate) const FORCED_MS: f64 = 6.0;
pub(crate) const HURRY_MS: f64 = 20.0;
/// How wide the front texture fades toward the outline, in design units. Line 769.
pub(crate) const FRONT_FADE: f64 = 5.0;

pub(crate) const DX: [i32; 8] = [1, 1, 0, -1, -1, -1, 0, 1];
pub(crate) const DY: [i32; 8] = [0, 1, 1, 1, 0, -1, -1, -1];
pub(crate) const DL: [f64; 8] = [
    1.0,
    std::f64::consts::SQRT_2,
    1.0,
    std::f64::consts::SQRT_2,
    1.0,
    std::f64::consts::SQRT_2,
    1.0,
    std::f64::consts::SQRT_2,
];

/// Environment and warm specular tints. `plastic.ts` lines 659–660.
pub(crate) const ENV: V3 = [0.92, 0.96, 1.0];
pub(crate) const WARM: V3 = [1.0, 0.98, 0.95];

pub(crate) const TONE_N: usize = 2048;
pub(crate) const TONE_MAX: f64 = 2.5;
pub(crate) const TONE_SCALE: f64 = (TONE_N as f64) / TONE_MAX;
pub(crate) const POW_N: usize = 1024;

/// Pillow height field, matcap cells, and baked occlusion for one outline.
#[derive(Debug, Clone)]
pub struct Form {
    /// Stable id from [`build_form`]. `0` is a hand-built form and falls back to a content hash.
    pub id: u64,
    pub n: u32,
    /// Index of the matcap's top-left cell.
    pub i00: Vec<u16>,
    /// Weights inside that cell, 0–255.
    pub wx: Vec<u8>,
    pub wy: Vec<u8>,
    /// Baked occlusion × edge darkening, gamma-compensated, 0–255. 0 = not drawn.
    pub ao: Vec<u8>,
    /// Signed distance to the outline in design units (positive inside).
    pub sd: Vec<f32>,
    /// Pillow height.
    pub h: Vec<f32>,
}

/// Inflated profile of a [`Form`]: level sets through the depth.
#[derive(Debug, Clone)]
pub struct Relief {
    /// Highest point, in design units from the centre plane.
    pub top: f64,
    /// Each level's depth from the centre plane. Level 0 is the outline.
    pub z: Vec<f32>,
    /// Scale of each level's region (by area).
    pub scale: Vec<f32>,
    pub cx: Vec<f32>,
    pub cy: Vec<f32>,
    /// How far in from the outline each level's edge lies, in design units.
    pub inset: Vec<f32>,
    /// Exponent the levels were cut with.
    pub q: f64,
}

/// Knobs of the clear-coat and fabric matcaps.
#[derive(Debug, Clone, Copy)]
pub struct Material {
    pub shadow: f64,
    pub highlight: f64,
    pub spread: f64,
    pub rim: f64,
    /// 0–1, how much colour the light keeps past full saturation.
    pub vivid: Option<f64>,
    /// Fabric: how far round to the front the key sits, radians.
    pub front: Option<f64>,
    /// Fabric: strength of the light along single fibres.
    pub shine: Option<f64>,
    /// Fabric: 0–1, how soft the back light is.
    pub back_soft: Option<f64>,
    /// Fabric: smooth satin light over the body's edge.
    pub sheen: Option<f64>,
}

/// Light and view in the cap's own frame.
#[derive(Debug, Clone, Copy)]
pub struct Frame {
    pub l: V3,
    pub v: V3,
    pub h: V3,
    pub u: V3,
    pub w: V3,
    pub a: V3,
    pub b: V3,
    /// Down on screen, in the cap's frame.
    pub d: V3,
    /// Toward the back light on screen, in the cap's frame.
    pub k: V3,
}

/// Fabric's lift near the edge: occlusion with the edge darkening divided back out.
#[derive(Debug, Clone)]
pub struct Lift {
    pub ao: Vec<u8>,
    /// Sheen band just inside the outline, a gain.
    pub k: Vec<f32>,
}

type Listener = Arc<dyn Fn() + Send + Sync>;

static LANDED: Mutex<Vec<(u64, Listener)>> = Mutex::new(Vec::new());
static NEXT_LISTENER: AtomicU64 = AtomicU64::new(1);
static HURRYING: Mutex<i32> = Mutex::new(0);
static MISSED: Mutex<bool> = Mutex::new(false);

struct FormStore {
    order: Vec<String>,
    map: HashMap<String, Arc<Form>>,
}

static NEXT_FORM: AtomicU64 = AtomicU64::new(1);

fn forms() -> &'static Mutex<FormStore> {
    static FORMS: OnceLock<Mutex<FormStore>> = OnceLock::new();
    FORMS.get_or_init(|| {
        Mutex::new(FormStore {
            order: Vec::new(),
            map: HashMap::new(),
        })
    })
}

fn reliefs() -> &'static Mutex<HashMap<u64, Vec<(String, Relief)>>> {
    static RELIEFS: OnceLock<Mutex<HashMap<u64, Vec<(String, Relief)>>>> = OnceLock::new();
    RELIEFS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn lifts() -> &'static Mutex<HashMap<u64, Lift>> {
    static LIFTS: OnceLock<Mutex<HashMap<u64, Lift>>> = OnceLock::new();
    LIFTS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn lin_cache() -> &'static Mutex<HashMap<String, V3>> {
    static LIN: OnceLock<Mutex<HashMap<String, V3>>> = OnceLock::new();
    LIN.get_or_init(|| Mutex::new(HashMap::new()))
}

fn pow_cache() -> &'static Mutex<HashMap<u64, Vec<f32>>> {
    static POW: OnceLock<Mutex<HashMap<u64, Vec<f32>>>> = OnceLock::new();
    POW.get_or_init(|| Mutex::new(HashMap::new()))
}

/// `Math.min(1, Math.max(0, x))` without panicking on NaN.
fn js_clamp01(x: f64) -> f64 {
    let x = if x < 0.0 { 0.0 } else { x };
    if x > 1.0 { 1.0 } else { x }
}

/// `Math.round`: half toward +∞.
fn js_round(x: f64) -> f64 {
    if !x.is_finite() {
        return x;
    }
    let f = x.floor();
    if x - f >= 0.5 { f + 1.0 } else { f }
}

/// `x | 0` for values that fit in an i32. Non-finite inputs become 0, as in JS.
fn js_trunc_i32(x: f64) -> i32 {
    if !x.is_finite() {
        return 0;
    }
    let t = x.trunc();
    if t >= i32::MAX as f64 {
        i32::MAX
    } else if t <= i32::MIN as f64 {
        i32::MIN
    } else {
        t as i32
    }
}

/// `Math.round` then `ToUint8` (wrap, not clamp). Used by `Uint8Array` stores.
fn round_u8(v: f64) -> u8 {
    let r = js_round(v);
    if !r.is_finite() {
        return 0;
    }
    r.trunc() as i64 as u8
}

/// `ToUint8Clamp`: round half to even, then clamp to 0–255. `Uint8ClampedArray` stores.
fn to_u8_clamp(v: f64) -> u8 {
    if !v.is_finite() || v <= 0.0 {
        return 0;
    }
    if v >= 255.0 {
        return 255;
    }
    let f = v.floor();
    let frac = v - f;
    let mut n = f as i32;
    if frac > 0.5 || (frac == 0.5 && (n & 1) == 1) {
        n += 1;
    }
    n as u8
}

/// `Number.prototype.toFixed(2)` for finite values, half away from zero via `* 100`.
fn to_fixed_2(x: f64) -> String {
    if !x.is_finite() {
        return if x.is_nan() {
            "NaN".to_string()
        } else if x.is_sign_negative() {
            "-Infinity".to_string()
        } else {
            "Infinity".to_string()
        };
    }
    let neg = x.is_sign_negative() && x != 0.0;
    let n = js_round(x.abs() * 100.0);
    let n = if n.is_finite() { n as i64 } else { 0 };
    let whole = n / 100;
    let frac = (n % 100).unsigned_abs();
    if neg {
        format!("-{whole}.{frac:02}")
    } else {
        format!("{whole}.{frac:02}")
    }
}

fn form_fp(form: &Form) -> u64 {
    if form.id != 0 {
        return form.id;
    }
    let mut h = 0xcbf29ce484222325u64;
    let mix = |h: &mut u64, b: u8| {
        *h ^= b as u64;
        *h = h.wrapping_mul(0x100000001b3);
    };
    for b in form.n.to_le_bytes() {
        mix(&mut h, b);
    }
    for v in &form.h {
        for b in v.to_le_bytes() {
            mix(&mut h, b);
        }
    }
    for v in &form.sd {
        for b in v.to_le_bytes() {
            mix(&mut h, b);
        }
    }
    for &v in &form.ao {
        mix(&mut h, v);
    }
    h
}

/// CSS named colours the browser canvas resolves when `parseColor` misses.
fn css_named_rgb(name: &str) -> Option<[f64; 3]> {
    let key = name.trim().to_ascii_lowercase();
    if key == "transparent" {
        return Some([0.0, 0.0, 0.0]);
    }
    let hex: u32 = match key.as_str() {
        "aliceblue" => 0xF0F8FF,
        "antiquewhite" => 0xFAEBD7,
        "aqua" => 0x00FFFF,
        "aquamarine" => 0x7FFFD4,
        "azure" => 0xF0FFFF,
        "beige" => 0xF5F5DC,
        "bisque" => 0xFFE4C4,
        "black" => 0x000000,
        "blanchedalmond" => 0xFFEBCD,
        "blue" => 0x0000FF,
        "blueviolet" => 0x8A2BE2,
        "brown" => 0xA52A2A,
        "burlywood" => 0xDEB887,
        "cadetblue" => 0x5F9EA0,
        "chartreuse" => 0x7FFF00,
        "chocolate" => 0xD2691E,
        "coral" => 0xFF7F50,
        "cornflowerblue" => 0x6495ED,
        "cornsilk" => 0xFFF8DC,
        "crimson" => 0xDC143C,
        "cyan" => 0x00FFFF,
        "darkblue" => 0x00008B,
        "darkcyan" => 0x008B8B,
        "darkgoldenrod" => 0xB8860B,
        "darkgray" => 0xA9A9A9,
        "darkgreen" => 0x006400,
        "darkgrey" => 0xA9A9A9,
        "darkkhaki" => 0xBDB76B,
        "darkmagenta" => 0x8B008B,
        "darkolivegreen" => 0x556B2F,
        "darkorange" => 0xFF8C00,
        "darkorchid" => 0x9932CC,
        "darkred" => 0x8B0000,
        "darksalmon" => 0xE9967A,
        "darkseagreen" => 0x8FBC8F,
        "darkslateblue" => 0x483D8B,
        "darkslategray" => 0x2F4F4F,
        "darkslategrey" => 0x2F4F4F,
        "darkturquoise" => 0x00CED1,
        "darkviolet" => 0x9400D3,
        "deeppink" => 0xFF1493,
        "deepskyblue" => 0x00BFFF,
        "dimgray" => 0x696969,
        "dimgrey" => 0x696969,
        "dodgerblue" => 0x1E90FF,
        "firebrick" => 0xB22222,
        "floralwhite" => 0xFFFAF0,
        "forestgreen" => 0x228B22,
        "fuchsia" => 0xFF00FF,
        "gainsboro" => 0xDCDCDC,
        "ghostwhite" => 0xF8F8FF,
        "gold" => 0xFFD700,
        "goldenrod" => 0xDAA520,
        "gray" => 0x808080,
        "green" => 0x008000,
        "greenyellow" => 0xADFF2F,
        "grey" => 0x808080,
        "honeydew" => 0xF0FFF0,
        "hotpink" => 0xFF69B4,
        "indianred" => 0xCD5C5C,
        "indigo" => 0x4B0082,
        "ivory" => 0xFFFFF0,
        "khaki" => 0xF0E68C,
        "lavender" => 0xE6E6FA,
        "lavenderblush" => 0xFFF0F5,
        "lawngreen" => 0x7CFC00,
        "lemonchiffon" => 0xFFFACD,
        "lightblue" => 0xADD8E6,
        "lightcoral" => 0xF08080,
        "lightcyan" => 0xE0FFFF,
        "lightgoldenrodyellow" => 0xFAFAD2,
        "lightgray" => 0xD3D3D3,
        "lightgreen" => 0x90EE90,
        "lightgrey" => 0xD3D3D3,
        "lightpink" => 0xFFB6C1,
        "lightsalmon" => 0xFFA07A,
        "lightseagreen" => 0x20B2AA,
        "lightskyblue" => 0x87CEFA,
        "lightslategray" => 0x778899,
        "lightslategrey" => 0x778899,
        "lightsteelblue" => 0xB0C4DE,
        "lightyellow" => 0xFFFFE0,
        "lime" => 0x00FF00,
        "limegreen" => 0x32CD32,
        "linen" => 0xFAF0E6,
        "magenta" => 0xFF00FF,
        "maroon" => 0x800000,
        "mediumaquamarine" => 0x66CDAA,
        "mediumblue" => 0x0000CD,
        "mediumorchid" => 0xBA55D3,
        "mediumpurple" => 0x9370DB,
        "mediumseagreen" => 0x3CB371,
        "mediumslateblue" => 0x7B68EE,
        "mediumspringgreen" => 0x00FA9A,
        "mediumturquoise" => 0x48D1CC,
        "mediumvioletred" => 0xC71585,
        "midnightblue" => 0x191970,
        "mintcream" => 0xF5FFFA,
        "mistyrose" => 0xFFE4E1,
        "moccasin" => 0xFFE4B5,
        "navajowhite" => 0xFFDEAD,
        "navy" => 0x000080,
        "oldlace" => 0xFDF5E6,
        "olive" => 0x808000,
        "olivedrab" => 0x6B8E23,
        "orange" => 0xFFA500,
        "orangered" => 0xFF4500,
        "orchid" => 0xDA70D6,
        "palegoldenrod" => 0xEEE8AA,
        "palegreen" => 0x98FB98,
        "paleturquoise" => 0xAFEEEE,
        "palevioletred" => 0xDB7093,
        "papayawhip" => 0xFFEFD5,
        "peachpuff" => 0xFFDAB9,
        "peru" => 0xCD853F,
        "pink" => 0xFFC0CB,
        "plum" => 0xDDA0DD,
        "powderblue" => 0xB0E0E6,
        "purple" => 0x800080,
        "rebeccapurple" => 0x663399,
        "red" => 0xFF0000,
        "rosybrown" => 0xBC8F8F,
        "royalblue" => 0x4169E1,
        "saddlebrown" => 0x8B4513,
        "salmon" => 0xFA8072,
        "sandybrown" => 0xF4A460,
        "seagreen" => 0x2E8B57,
        "seashell" => 0xFFF5EE,
        "sienna" => 0xA0522D,
        "silver" => 0xC0C0C0,
        "skyblue" => 0x87CEEB,
        "slateblue" => 0x6A5ACD,
        "slategray" => 0x708090,
        "slategrey" => 0x708090,
        "snow" => 0xFFFAFA,
        "springgreen" => 0x00FF7F,
        "steelblue" => 0x4682B4,
        "tan" => 0xD2B48C,
        "teal" => 0x008080,
        "thistle" => 0xD8BFD8,
        "tomato" => 0xFF6347,
        "turquoise" => 0x40E0D0,
        "violet" => 0xEE82EE,
        "wheat" => 0xF5DEB3,
        "white" => 0xFFFFFF,
        "whitesmoke" => 0xF5F5F5,
        "yellow" => 0xFFFF00,
        "yellowgreen" => 0x9ACD32,
        _ => return None,
    };
    Some([
        ((hex >> 16) & 255) as f64,
        ((hex >> 8) & 255) as f64,
        (hex & 255) as f64,
    ])
}

/// `norm3` (`plastic.ts` 2841–2844).
pub(crate) fn norm3(v: V3) -> V3 {
    let l = v[0].hypot(v[1]).hypot(v[2]);
    let l = if l > 0.0 { l } else { 1.0 };
    [v[0] / l, v[1] / l, v[2] / l]
}

/// Felzenszwalb–Huttenlocher 1-D squared distance transform (`edt1d`, 62–87).
pub(crate) fn edt_1d(
    f: &[f32],
    n: usize,
    d: &mut [f32],
    s: &mut [i32],
    v: &mut [i32],
    z: &mut [f32],
) {
    if n == 0 {
        return;
    }
    let mut k: i32 = 0;
    v[0] = 0;
    z[0] = -1e30;
    z[1] = 1e30;
    for q in 1..n {
        let qf = q as f64;
        let fq = f[q] as f64;
        let mut x;
        loop {
            let vk = v[k as usize];
            let vkf = vk as f64;
            x = (fq + qf * qf - f[vk as usize] as f64 - vkf * vkf) / (2.0 * (qf - vkf));
            if x > z[k as usize] as f64 || k == 0 {
                break;
            }
            k -= 1;
        }
        k += 1;
        v[k as usize] = q as i32;
        z[k as usize] = x as f32;
        z[(k + 1) as usize] = 1e30;
    }
    k = 0;
    for q in 0..n {
        let qf = q as f64;
        while (z[(k + 1) as usize] as f64) < qf {
            k += 1;
        }
        let vk = v[k as usize];
        let diff = q as f64 - vk as f64;
        d[q] = (diff * diff + f[vk as usize] as f64) as f32;
        s[q] = vk;
    }
}

/// Squared texel distance to the nearest texel whose mask equals `site` (`edt2d`, 91–111).
pub(crate) fn edt_2d(
    mask: &[u8],
    site: u8,
    n: usize,
    out: &mut [f32],
    mut near: Option<&mut [i32]>,
) {
    if n == 0 {
        return;
    }
    let mut f = vec![0.0f32; n];
    let mut d = vec![0.0f32; n];
    let mut s = vec![0i32; n];
    let mut v = vec![0i32; n];
    let mut z = vec![0.0f32; n + 1];
    let mut g = vec![0.0f32; n * n];
    let mut row = vec![0i32; n * n];
    let inf = INF as f32;
    for x in 0..n {
        for y in 0..n {
            f[y] = if mask[y * n + x] == site { 0.0 } else { inf };
        }
        edt_1d(&f, n, &mut d, &mut s, &mut v, &mut z);
        for y in 0..n {
            g[y * n + x] = d[y];
            row[y * n + x] = s[y];
        }
    }
    for y in 0..n {
        let o = y * n;
        for x in 0..n {
            f[x] = g[o + x];
        }
        edt_1d(&f, n, &mut d, &mut s, &mut v, &mut z);
        for x in 0..n {
            out[o + x] = d[x];
            if let Some(near) = near.as_mut() {
                let sx = s[x];
                near[o + x] = row[o + sx as usize] * n as i32 + sx;
            }
        }
    }
}

/// Separable binomial blur `[1 4 6 4 1]/16` (`blur5`, 114–128).
pub(crate) fn blur5(a: &mut [f32], n: usize, tmp: &mut [f32]) {
    if n == 0 {
        return;
    }
    for y in 0..n {
        let o = y * n;
        for x in 0..n {
            let x0 = if x < 2 { 0 } else { x - 2 };
            let x1 = if x < 1 { 0 } else { x - 1 };
            let x3 = if x + 2 > n { n - 1 } else { x + 1 };
            let x4 = if x + 3 > n { n - 1 } else { x + 2 };
            let sum = (a[o + x0] as f64)
                + 4.0 * (a[o + x1] as f64)
                + 6.0 * (a[o + x] as f64)
                + 4.0 * (a[o + x3] as f64)
                + (a[o + x4] as f64);
            tmp[o + x] = (sum * 0.0625) as f32;
        }
    }
    for x in 0..n {
        for y in 0..n {
            let y0 = if y < 2 { 0 } else { y - 2 };
            let y1 = if y < 1 { 0 } else { y - 1 };
            let y3 = if y + 2 > n { n - 1 } else { y + 1 };
            let y4 = if y + 3 > n { n - 1 } else { y + 2 };
            let sum = (tmp[y0 * n + x] as f64)
                + 4.0 * (tmp[y1 * n + x] as f64)
                + 6.0 * (tmp[y * n + x] as f64)
                + 4.0 * (tmp[y3 * n + x] as f64)
                + (tmp[y4 * n + x] as f64);
            a[y * n + x] = (sum * 0.0625) as f32;
        }
    }
}

fn sweep_poisson(phi: &mut [f32], mask: &[u8], n: usize, s: f64, iters: usize, om: f64) {
    if n < 3 {
        return;
    }
    let s2 = s * s;
    for _ in 0..iters {
        for y in 1..n - 1 {
            let o = y * n;
            for x in 1..n - 1 {
                let i = o + x;
                if mask[i] == 0 {
                    continue;
                }
                let v = (phi[i - 1] as f64
                    + phi[i + 1] as f64
                    + phi[i - n] as f64
                    + phi[i + n] as f64
                    + s2)
                    * 0.25;
                let cur = phi[i] as f64;
                phi[i] = (cur + om * (v - cur)) as f32;
            }
        }
    }
}

struct PoissonLevel {
    n: usize,
    mask: Vec<u8>,
    phi: Vec<f32>,
}

/// Gauss–Seidel pillow solve (`poisson`, 133–185).
pub(crate) fn poisson(cov: &[u8], n: usize, u: f64) -> Vec<f32> {
    let mut levels: Vec<PoissonLevel> = Vec::new();
    let mut n_l = n as i32;
    let mut f = 1i32;
    loop {
        // `f <= 4 && n % 2 === 0 || f === 1` — `&&` binds tighter than `||`.
        if !((f <= 4 && n_l % 2 == 0) || f == 1) {
            break;
        }
        let nu = if n_l > 0 { n_l as usize } else { 0 };
        let fu = if f > 0 { f as usize } else { 0 };
        let mut mask = vec![0u8; nu.saturating_mul(nu)];
        if fu > 0 && n > 0 {
            let thresh = 128 * (fu as i32) * (fu as i32);
            for y in 0..nu {
                for x in 0..nu {
                    let mut sum = 0i32;
                    for j in 0..fu {
                        for i in 0..fu {
                            sum += cov[(y * fu + j) * n + x * fu + i] as i32;
                        }
                    }
                    mask[y * nu + x] = if sum >= thresh { 1 } else { 0 };
                }
            }
        }
        levels.push(PoissonLevel {
            n: nu,
            mask,
            phi: vec![0.0; nu.saturating_mul(nu)],
        });
        if f == 4 {
            break;
        }
        n_l >>= 1;
        f <<= 1;
    }
    for l in (0..levels.len()).rev() {
        let f_scale = 1i32 << l;
        if l + 1 < levels.len() {
            let cn = levels[l + 1].n;
            let n_fine = levels[l].n;
            for y in 0..n_fine {
                let fy = js_min_max((y as f64 + 0.5) / 2.0 - 0.5, cn);
                let y0 = js_trunc_i32(fy);
                let y1 = js_trunc_i32(js_min_pair(cn as f64 - 1.0, y0 as f64 + 1.0));
                let ty = fy - y0 as f64;
                for x in 0..n_fine {
                    let i = y * n_fine + x;
                    if levels[l].mask[i] == 0 {
                        continue;
                    }
                    let fx = js_min_max((x as f64 + 0.5) / 2.0 - 0.5, cn);
                    let x0 = js_trunc_i32(fx);
                    let x1 = js_trunc_i32(js_min_pair(cn as f64 - 1.0, x0 as f64 + 1.0));
                    let tx = fx - x0 as f64;
                    let v = {
                        let coarse = &levels[l + 1];
                        let c00 = coarse_phi(coarse, x0, y0);
                        let c10 = coarse_phi(coarse, x1, y0);
                        let c01 = coarse_phi(coarse, x0, y1);
                        let c11 = coarse_phi(coarse, x1, y1);
                        (c00 * (1.0 - tx) + c10 * tx) * (1.0 - ty)
                            + (c01 * (1.0 - tx) + c11 * tx) * ty
                    };
                    levels[l].phi[i] = v as f32;
                }
            }
        }
        let n_here = levels[l].n;
        let om =
            (2.0 / (1.0 + (std::f64::consts::PI / n_here.max(1) as f64).sin()) - 0.05).min(1.9);
        let s = u * f_scale as f64;
        let iters_sor = if l == 2 {
            100
        } else if l == 1 {
            30
        } else {
            16
        };
        let level = &mut levels[l];
        sweep_poisson(&mut level.phi, &level.mask, n_here, s, 4, 1.0);
        sweep_poisson(&mut level.phi, &level.mask, n_here, s, iters_sor, om);
        sweep_poisson(&mut level.phi, &level.mask, n_here, s, 8, 1.0);
    }
    levels.into_iter().next().map(|l| l.phi).unwrap_or_default()
}

/// `Math.min(cn - 1, Math.max(0, v))`.
fn js_min_max(v: f64, cn: usize) -> f64 {
    let lo = if v < 0.0 { 0.0 } else { v };
    let hi = cn as f64 - 1.0;
    if lo < hi { lo } else { hi }
}

fn js_min_pair(a: f64, b: f64) -> f64 {
    if a < b { a } else { b }
}

fn coarse_phi(level: &PoissonLevel, x: i32, y: i32) -> f64 {
    if x < 0 || y < 0 {
        return f64::NAN;
    }
    let (x, y) = (x as usize, y as usize);
    if level.n == 0 || x >= level.n || y >= level.n {
        return f64::NAN;
    }
    level.phi[y * level.n + x] as f64
}

/// `buildForm` (196–288).
pub fn build_form(cov: &[u8], n: u32, half_depth: f64) -> Form {
    let n = n as usize;
    let nn = n * n;
    if n == 0 {
        return Form {
            id: 0,
            n: 0,
            i00: Vec::new(),
            wx: Vec::new(),
            wy: Vec::new(),
            ao: Vec::new(),
            sd: Vec::new(),
            h: Vec::new(),
        };
    }
    let u = SPAN / n as f64;
    let mut mask = vec![0u8; nn];
    for i in 0..nn {
        mask[i] = if cov[i] >= 128 { 1 } else { 0 };
    }
    let mut d_in = vec![0.0f32; nn];
    let mut d_out = vec![0.0f32; nn];
    let mut near_in = vec![0i32; nn];
    edt_2d(&mask, 0, n, &mut d_in, None);
    edt_2d(&mask, 1, n, &mut d_out, Some(&mut near_in));
    let mut sd = vec![0.0f32; nn];
    let mut tmp = vec![0.0f32; nn];
    for i in 0..nn {
        let a = cov[i] as f64 / 255.0;
        let signed = if a > 0.0 && a < 1.0 {
            a - 0.5
        } else if mask[i] == 1 {
            (d_in[i] as f64).sqrt() - 0.5
        } else {
            0.5 - (d_out[i] as f64).sqrt()
        };
        sd[i] = (u * signed) as f32;
    }
    blur5(&mut sd, n, &mut tmp);
    blur5(&mut sd, n, &mut tmp);

    let phi = poisson(cov, n, u);
    let mut phi_max = 0.0f64;
    for &p in &phi {
        if p as f64 > phi_max {
            phi_max = p as f64;
        }
    }
    let r_in = 2.0 * phi_max.sqrt();
    let h_max = (1.5 * half_depth + 0.32 * r_in).min(1.4 * r_in);
    let kh = if phi_max > 0.0 {
        h_max / phi_max.sqrt()
    } else {
        0.0
    };
    let mut h = vec![0.0f32; nn];
    for i in 0..nn {
        h[i] = if phi[i] > 0.0 {
            (kh * (phi[i] as f64).sqrt()) as f32
        } else {
            0.0
        };
    }
    blur5(&mut h, n, &mut tmp);

    let mut i00 = vec![0u16; nn];
    let mut wx = vec![0u8; nn];
    let mut wy = vec![0u8; nn];
    let mut ao = vec![0u8; nn];
    let steps: &[i32] = if n <= 64 {
        &[1, 2, 3, 5, 8]
    } else if n <= 96 {
        &[1, 2, 4, 7, 11]
    } else {
        &[1, 2, 4, 7, 11, 15]
    };
    let halo = 64.0;
    let last = n - 1;
    for y in 0..n {
        for x in 0..n {
            let i = y * n + x;
            let src = if mask[i] == 1 {
                i as i32
            } else if d_out[i] as f64 <= halo {
                near_in[i]
            } else {
                -1
            };
            if src < 0 {
                continue;
            }
            let src_u = src as usize;
            let sx = src_u % n;
            let sy = src_u / n;
            let xl = if sx > 0 { sx - 1 } else { 0 };
            let xr = if sx < last { sx + 1 } else { last };
            let yu = if sy > 0 { sy - 1 } else { 0 };
            let yd = if sy < last { sy + 1 } else { last };
            let mut nx = -((h[sy * n + xr] as f64) - (h[sy * n + xl] as f64)) / (2.0 * u);
            let mut ny = -((h[yd * n + sx] as f64) - (h[yu * n + sx] as f64)) / (2.0 * u);
            let mut nz = 1.0;
            let mut len = (nx * nx + ny * ny + 1.0).sqrt();
            nx /= len;
            ny /= len;
            nz /= len;
            let dd = (sd[src_u] as f64).max(0.0);
            if dd < 2.0 {
                let mut gx = (sd[sy * n + xr] as f64) - (sd[sy * n + xl] as f64);
                let mut gy = (sd[yd * n + sx] as f64) - (sd[yu * n + sx] as f64);
                let gl = gx.hypot(gy);
                let gl = if gl > 0.0 { gl } else { 1.0 };
                gx /= gl;
                gy /= gl;
                let w = 0.7 * (1.0 - dd / 2.0);
                nx += w * (-gx - nx);
                ny += w * (-gy - ny);
                nz += w * (0.0 - nz);
                len = (nx * nx + ny * ny + nz * nz).sqrt();
                let len = if len > 0.0 { len } else { 1.0 };
                nx /= len;
                ny /= len;
                nz /= len;
                let _ = nz;
            }
            let h0 = h[src_u] as f64;
            let mut occ = 0.0;
            for d in 0..8 {
                let mut m = 0.0;
                for &r in steps {
                    let mut qx = sx as i32 + DX[d] * r;
                    let mut qy = sy as i32 + DY[d] * r;
                    if qx < 0 {
                        qx = 0;
                    } else if qx > last as i32 {
                        qx = last as i32;
                    }
                    if qy < 0 {
                        qy = 0;
                    } else if qy > last as i32 {
                        qy = last as i32;
                    }
                    let t =
                        ((h[qy as usize * n + qx as usize] as f64) - h0) / (r as f64 * u * DL[d]);
                    if t > m {
                        m = t;
                    }
                }
                occ += m / (1.0 + m * m).sqrt();
            }
            let e = 1.0 - (dd / 3.0).min(1.0);
            let edge = 1.0 - 0.2 * e * e;
            let ao_lin = (1.0 - 0.9 * occ / 8.0).powf(1.5) * edge;
            ao[i] = round_u8((255.0 * ao_lin.powf(1.0 / 2.2)).max(1.0));
            let fx = (nx * 0.5 + 0.5) * (M as f64 - 1.0);
            let fy = (ny * 0.5 + 0.5) * (M as f64 - 1.0);
            let cx = js_trunc_i32(fx).max(0).min(M as i32 - 2);
            let cy = js_trunc_i32(fy).max(0).min(M as i32 - 2);
            i00[i] = (cy * M as i32 + cx) as u16;
            wx[i] = round_u8(255.0 * js_clamp01(fx - cx as f64));
            wy[i] = round_u8(255.0 * js_clamp01(fy - cy as f64));
        }
    }
    Form {
        id: NEXT_FORM.fetch_add(1, Ordering::Relaxed),
        n: n as u32,
        i00,
        wx,
        wy,
        ao,
        sd,
        h,
    }
}

/// Scratch surface. The DOM canvas is [`Canvas2D`]; this port always has one (`makeCanvas`, 293–301).
pub(crate) fn make_canvas(n: u32) -> Canvas2D {
    Canvas2D::new(n.max(1), n.max(1))
}

/// `ctx2d` (302–304). [`Canvas2D`] is already the 2D context; `read_back` is unused.
pub(crate) fn ctx_2d(canvas: &mut Canvas2D, read_back: bool) -> &mut Canvas2D {
    let _ = read_back;
    canvas
}

/// Coverage raster, `N × N` alpha over `[-PAD, 100 + PAD]` (`rasterize`, 307–319).
pub fn rasterize(path: &Path, n: u32) -> Option<Vec<u8>> {
    if n == 0 {
        return Some(Vec::new());
    }
    let mut canvas = make_canvas(n);
    let g = ctx_2d(&mut canvas, true);
    let u = SPAN / n as f64;
    let s = 1.0 / u;
    g.set_transform(s, 0.0, 0.0, s, PAD / u, PAD / u);
    g.set_fill_css("#fff");
    g.fill_path(path);
    let data = g.pixmap.data();
    let w = g.pixmap.width() as usize;
    let n = n as usize;
    let mut cov = vec![0u8; n * n];
    for i in 0..cov.len() {
        let x = i % n;
        let y = i / n;
        cov[i] = data[(y * w + x) * 4 + 3];
    }
    Some(cov)
}

/// Stable identity of a path. Point and verb bytes, not the allocation address.
pub(crate) fn path_fingerprint(path: &Path) -> u64 {
    let mut h = 0xcbf29ce484222325u64;
    let mix = |h: &mut u64, b: u8| {
        *h ^= b as u64;
        *h = h.wrapping_mul(0x100000001b3);
    };
    let mix_pt = |h: &mut u64, point: Point| {
        for b in point.x.to_le_bytes() {
            mix(h, b);
        }
        for b in point.y.to_le_bytes() {
            mix(h, b);
        }
    };
    for seg in path.segments() {
        match seg {
            PathSegment::MoveTo(point) => {
                mix(&mut h, 1);
                mix_pt(&mut h, point);
            }
            PathSegment::LineTo(point) => {
                mix(&mut h, 2);
                mix_pt(&mut h, point);
            }
            PathSegment::QuadTo(a, b) => {
                mix(&mut h, 3);
                mix_pt(&mut h, a);
                mix_pt(&mut h, b);
            }
            PathSegment::CubicTo(a, b, c) => {
                mix(&mut h, 4);
                mix_pt(&mut h, a);
                mix_pt(&mut h, b);
                mix_pt(&mut h, c);
            }
            PathSegment::Close => mix(&mut h, 5),
        }
    }
    h
}

/// Identity of a path for the form cache (`pathId`, 329–333).
pub(crate) fn path_id(path: &Path) -> String {
    format!("p{:x}", path_fingerprint(path))
}

/// `clock` (345). The synchronous cache never reads it. A real clock would
/// make bakes non-deterministic, so this stays at 0.
pub(crate) fn clock() -> f64 {
    0.0
}

/// `pump` (352–363). Synchronous bakes do not drain an idle queue.
pub(crate) fn pump() {}

/// `schedule` (364–374). No idle callback is queued.
pub(crate) fn schedule() {}

/// Balance of avatars waiting on a bake (`hurry`, 377–380). Scheduling does not change.
pub fn hurry(on: bool) {
    let mut n = HURRYING.lock().expect("hurry lock");
    if on {
        *n += 1;
    } else if *n > 0 {
        *n -= 1;
    }
}

/// Avatars currently inside [`hurry`]`(true)`.
pub(crate) fn hurrying() -> i32 {
    *HURRYING.lock().expect("hurry lock")
}

/// Whether a form or pile is still being made (`bakesPending`, 382–384).
/// Synchronous bakes finish before they return, so this is false.
pub fn bakes_pending() -> bool {
    false
}

/// Run `task` now (`idle`, 385–388).
pub(crate) fn idle(task: impl FnOnce()) {
    task();
}

/// Invoke landed listeners now (`land`, 396–403). The TS `setTimeout` is dropped.
pub(crate) fn land() {
    let listeners: Vec<Listener> = LANDED
        .lock()
        .expect("landed lock")
        .iter()
        .map(|(_, listener)| Arc::clone(listener))
        .collect();
    for listener in listeners {
        listener();
    }
}

/// Call `listener` each time a form or a pile lands, until the returned function runs
/// (`onLanded`, 405–410).
pub fn on_landed(listener: impl Fn() + Send + Sync + 'static) -> impl FnOnce() + Send {
    let id = NEXT_LISTENER.fetch_add(1, Ordering::Relaxed);
    LANDED
        .lock()
        .expect("landed lock")
        .push((id, Arc::new(listener)));
    move || {
        if let Ok(mut listeners) = LANDED.lock() {
            listeners.retain(|(slot, _)| *slot != id);
        }
    }
}

/// Whether a draw stood in for a bake still under way, then clear the flag (`takeMissed`, 412–416).
pub fn take_missed() -> bool {
    let mut missed = MISSED.lock().expect("missed lock");
    let was = *missed;
    *missed = false;
    was
}

/// Set the flag [`take_missed`] reports. Synchronous builds do not need it.
pub(crate) fn note_missed() {
    *MISSED.lock().expect("missed lock") = true;
}

fn form_cache_id(key: &str, n: u32, half_depth: f64) -> String {
    format!("{key}|{n}|{}", js_round(half_depth) as i64)
}

/// The form for an outline (`formFor`, 418–441). `sync` is kept; this port builds on the calling thread.
pub(crate) fn form_for(
    key: &str,
    path: &Path,
    n: u32,
    half_depth: f64,
    sync: bool,
) -> Option<Arc<Form>> {
    let _ = sync;
    let id = form_cache_id(key, n, half_depth);
    {
        let store = forms().lock().expect("forms");
        if let Some(hit) = store.map.get(&id) {
            return Some(Arc::clone(hit));
        }
    }
    let cov = rasterize(path, n)?;
    let built = Arc::new(build_form(&cov, n, half_depth));
    {
        let mut store = forms().lock().expect("forms");
        if let Some(hit) = store.map.get(&id) {
            return Some(Arc::clone(hit));
        }
        // Every tier of every body and part on a full gallery page fits before it starts over.
        if store.map.len() >= 192 {
            store.map.clear();
            store.order.clear();
        }
        store.order.push(id.clone());
        store.map.insert(id, Arc::clone(&built));
    }
    land();
    Some(built)
}

/// Build a form ahead of time (`warmPlastic`, 443–451).
///
/// TS defaults: `device_px = 192`, `depth = 0.65`, `fabric = false`, `style = FUR_STOCK`, `light = 295`.
pub fn warm_plastic(
    key: &str,
    path: &Path,
    device_px: f64,
    depth: f64,
    fabric: bool,
    style: &FurStyle,
    light: f64,
    lights: Option<&FurLights>,
) {
    let half = 15.0 * depth;
    let form = form_for(key, path, tier_for(device_px), half, true);
    if fabric && let Some(form) = form {
        let a = (light * std::f64::consts::PI) / 180.0;
        let sx = a.sin();
        let sy = -a.cos();
        let cap_px = (SPAN * device_px / 100.0).ceil();
        if let Some(lights) = lights {
            fur_ready(&form, key, half, cap_px, false, sx, sy, style, lights);
        } else {
            let lights = stock_lights(sx, sy);
            fur_ready(&form, key, half, cap_px, false, sx, sy, style, &lights);
        }
    }
}

/// Texture size for an avatar `device_px` wide (`tierFor`, 453–455).
pub fn tier_for(device_px: f64) -> u32 {
    if device_px <= 100.0 {
        64
    } else if device_px <= 224.0 {
        96
    } else {
        128
    }
}

fn measure_level(n: usize, u: f64, mut inside: impl FnMut(usize) -> bool) -> (f64, f64, f64) {
    let mut count = 0.0;
    let mut sx = 0.0;
    let mut sy = 0.0;
    let mut i = 0;
    for j in 0..n {
        for x in 0..n {
            if inside(i) {
                count += 1.0;
                sx += x as f64;
                sy += j as f64;
            }
            i += 1;
        }
    }
    if count > 0.0 {
        (
            count,
            ((sx / count + 0.5) * u) - PAD,
            ((sy / count + 0.5) * u) - PAD,
        )
    } else {
        (0.0, 50.0, 50.0)
    }
}

/// `reliefFor` (488–538).
pub fn relief_for(form: &Form, k: u32, round: f64) -> Relief {
    let q = round.clamp(0.05, 1.0);
    let cache_key = format!("{k}|{}", to_fixed_2(q));
    let fp = form_fp(form);
    {
        let cache = reliefs().lock().expect("reliefs");
        if let Some(bucket) = cache.get(&fp)
            && let Some((_, hit)) = bucket.iter().find(|(key, _)| key == &cache_key)
        {
            return hit.clone();
        }
    }
    let n = form.n as usize;
    let mut top = 0.0f64;
    for &v in &form.h {
        if v as f64 > top {
            top = v as f64;
        }
    }
    let u = if n == 0 { 0.0 } else { SPAN / n as f64 };
    let mut z = vec![0.0f32; k as usize];
    let mut scale = vec![0.0f32; k as usize];
    let mut cx = vec![0.0f32; k as usize];
    let mut cy = vec![0.0f32; k as usize];
    let mut inset = vec![0.0f32; k as usize];
    if k > 0 {
        inset[0] = 1.2;
    }
    let (a0, x0, y0) = measure_level(n, u, |i| form.sd[i] >= 0.0);
    if k > 0 {
        scale[0] = 1.0;
        cx[0] = x0 as f32;
        cy[0] = y0 as f32;
    }
    for level in 1..k as usize {
        let s = level as f64 / k as f64;
        z[level] = (top * s) as f32;
        let tau = top * s.powf(1.0 / q);
        let (ak, xk, yk) = measure_level(n, u, |i| form.sd[i] >= 0.0 && form.h[i] as f64 >= tau);
        let grown = if a0 > 0.0 { (ak / a0).sqrt() } else { 1.0 };
        let prev = scale[level - 1] as f64;
        scale[level] = prev.min(grown) as f32;
        cx[level] = xk as f32;
        cy[level] = yk as f32;
        let band = top / (2.0 * k as f64);
        let mut count = 0.0;
        let mut sum = 0.0;
        for i in 0..n * n {
            if form.sd[i] > 0.0 && ((form.h[i] as f64) - tau).abs() < band {
                sum += form.sd[i] as f64;
                count += 1.0;
            }
        }
        let inset_k = if count > 0.0 {
            sum / count
        } else {
            inset[level - 1] as f64
        };
        inset[level] = (inset[level - 1] as f64).max(inset_k) as f32;
    }
    let rel = Relief {
        top,
        z,
        scale,
        cx,
        cy,
        inset,
        q,
    };
    {
        let mut cache = reliefs().lock().expect("reliefs");
        let bucket = cache.entry(fp).or_default();
        if bucket.len() > 6 {
            bucket.clear();
        }
        bucket.push((cache_key, rel.clone()));
    }
    rel
}

/// Height of the surface at a design point (`surfaceAt`, 543–558). `round` defaults to 1.
pub fn surface_at(key: &str, half_depth: f64, x: f64, y: f64, round: f64) -> Option<f64> {
    let prefix = format!("{key}|");
    let suffix = format!("|{}", js_round(half_depth) as i64);
    let store = forms().lock().expect("forms");
    let mut form: Option<&Form> = None;
    for id in &store.order {
        if id.starts_with(&prefix) && id.ends_with(&suffix) {
            form = store.map.get(id).map(Arc::as_ref);
        }
    }
    surface_of(form?, x, y, round)
}

/// [`surface_at`] on the form of tier `n`, the one the body at this size is drawn from, so
/// the answer does not depend on which sizes of the outline were drawn before. Falls back to
/// the last form made for the outline while that tier is not built.
pub(crate) fn surface_at_tier(
    key: &str,
    n: u32,
    half_depth: f64,
    x: f64,
    y: f64,
    round: f64,
) -> Option<f64> {
    let form = forms()
        .lock()
        .expect("forms")
        .map
        .get(&form_cache_id(key, n, half_depth))
        .cloned();
    match form {
        Some(form) => surface_of(&form, x, y, round),
        None => surface_at(key, half_depth, x, y, round),
    }
}

fn surface_of(form: &Form, x: f64, y: f64, round: f64) -> Option<f64> {
    let n = form.n as usize;
    if n < 2 {
        return Some(0.0);
    }
    let u = SPAN / n as f64;
    let fx = ((x + PAD) / u - 0.5).clamp(0.0, (n - 1) as f64);
    let fy = ((y + PAD) / u - 0.5).clamp(0.0, (n - 1) as f64);
    let x0 = js_trunc_i32(fx).min(n as i32 - 2).max(0) as usize;
    let y0 = js_trunc_i32(fy).min(n as i32 - 2).max(0) as usize;
    let tx = fx - x0 as f64;
    let ty = fy - y0 as f64;
    let i = y0 * n + x0;
    let v = (form.h[i] as f64 * (1.0 - tx) + form.h[i + 1] as f64 * tx) * (1.0 - ty)
        + (form.h[i + n] as f64 * (1.0 - tx) + form.h[i + n + 1] as f64 * tx) * ty;
    let mut top = 0.0f64;
    for &hv in &form.h {
        if hv as f64 > top {
            top = hv as f64;
        }
    }
    if top <= 0.0 {
        return Some(0.0);
    }
    let q = round.clamp(0.05, 1.0);
    Some(top * (v.max(0.0) / top).powf(q))
}

/// sRGB 0–1 to linear (`toLin`, 562).
pub(crate) fn to_lin(v: f64) -> f64 {
    if v <= 0.04045 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    }
}

fn resolve_rgb(color: &str) -> [f64; 3] {
    if let Some(rgb) = parse_color(color) {
        return rgb;
    }
    if let Some(rgb) = css_named_rgb(color) {
        return rgb;
    }
    // A name the canvas does not know keeps the default fill, black.
    let mut canvas = make_canvas(1);
    let g = ctx_2d(&mut canvas, true);
    g.set_fill_css(color);
    g.fill_rect(0.0, 0.0, 1.0, 1.0);
    match g.pixmap.pixel(0, 0) {
        Some(px) => [px.red() as f64, px.green() as f64, px.blue() as f64],
        None => [0.0, 0.0, 0.0],
    }
}

/// `linearColor` (564–584).
pub(crate) fn linear_color(color: &str) -> V3 {
    {
        let cache = lin_cache().lock().expect("lin cache");
        if let Some(c) = cache.get(color) {
            return *c;
        }
    }
    let rgb = resolve_rgb(color);
    let c = [
        to_lin(rgb[0] / 255.0),
        to_lin(rgb[1] / 255.0),
        to_lin(rgb[2] / 255.0),
    ];
    let mut cache = lin_cache().lock().expect("lin cache");
    if let Some(hit) = cache.get(color) {
        return *hit;
    }
    if cache.len() > 200 {
        cache.clear();
    }
    cache.insert(color.to_string(), c);
    c
}

fn tone_lut() -> &'static [f32; TONE_N] {
    static LUT: OnceLock<[f32; TONE_N]> = OnceLock::new();
    LUT.get_or_init(|| {
        let mut lut = [0.0; TONE_N];
        for i in 0..TONE_N {
            let v = (i as f64 + 0.5) / TONE_SCALE;
            let y = if v <= 0.75 {
                v
            } else {
                0.75 + 0.25 * (1.0 - (-(v - 0.75) / 0.25).exp())
            };
            let enc = if y <= 0.0031308 {
                12.92 * y
            } else {
                1.055 * y.powf(1.0 / 2.4) - 0.055
            };
            lut[i] = (255.0 * enc) as f32;
        }
        lut
    })
}

/// Tone-mapped sRGB byte (`tone`, 594).
pub(crate) fn tone(v: f64) -> f32 {
    let lut = tone_lut();
    if !(v > 0.0) {
        lut[0]
    } else if v >= TONE_MAX {
        lut[TONE_N - 1]
    } else {
        let i = (v * TONE_SCALE).trunc() as usize;
        lut[i.min(TONE_N - 1)]
    }
}

/// Linear to 0–255 sRGB, no shoulder (`srgb`, 596).
pub(crate) fn srgb(v: f64) -> f64 {
    if v <= 0.0 {
        0.0
    } else if v >= 1.0 {
        255.0
    } else if v <= 0.0031308 {
        255.0 * 12.92 * v
    } else {
        255.0 * (1.055 * v.powf(1.0 / 2.4) - 0.055)
    }
}

/// `x^e` over `[0, 1]` (`powLut`, 601–610).
pub(crate) fn pow_lut(e: f64) -> Vec<f32> {
    let key = e.to_bits();
    let mut cache = pow_cache().lock().expect("pow lut");
    if let Some(t) = cache.get(&key) {
        return t.clone();
    }
    let mut t = vec![0.0f32; POW_N + 1];
    for i in 0..=POW_N {
        t[i] = ((i as f64) / POW_N as f64).powf(e) as f32;
    }
    if cache.len() > 16 {
        cache.clear();
    }
    cache.insert(key, t.clone());
    t
}

/// Pull a linear triple away from grey (`vivify`, 641–651). `cap` defaults to +∞.
pub(crate) fn vivify(c: &mut V3, v: f64, cap: f64) {
    if !(v > 0.0) {
        return;
    }
    let y = 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
    let mn = c[0].min(c[1]).min(c[2]);
    let mx = c[0].max(c[1]).max(c[2]);
    let mut k = 1.0 + 3.0 * v;
    if mn < y {
        k = k.min(y / (y - mn));
    }
    if mx > y {
        k = k.min((1.0_f64).max((cap - y) / (mx - y)));
    }
    c[0] = y + (c[0] - y) * k;
    c[1] = y + (c[1] - y) * k;
    c[2] = y + (c[2] - y) * k;
}

/// Smoothstep (`smooth`, 661–664).
pub(crate) fn smooth(a: f64, b: f64, v: f64) -> f64 {
    let t = if v <= a {
        0.0
    } else if v >= b {
        1.0
    } else {
        (v - a) / (b - a)
    };
    t * t * (3.0 - 2.0 * t)
}

/// 1 inside `|x| < w`, falling to 0 over `±s` (`soft`, 666).
pub(crate) fn soft(w: f64, s: f64, x: f64) -> f64 {
    1.0 - smooth(w - s, w + s, x)
}

/// Fill `out` (`M × M × rgb`, sRGB 0–255) with the lit sphere (`buildMatcap`, 669–721).
pub fn build_matcap(out: &mut [f32], c: V3, frame: &Frame, material: &Material) {
    let Frame {
        l: lit,
        v: view,
        h: half,
        u: up,
        w: win_dir,
        a: axis_a,
        b: axis_b,
        ..
    } = *frame;
    let vivid = material.vivid.unwrap_or(0.0);
    let mut px = [0.0, 0.0, 0.0];
    let mx = c[0].max(c[1]).max(c[2]).max(0.05);
    let tint = [c[0] / mx, c[1] / mx, c[2] / mx];
    let amb = (0.30 - 0.15 * material.shadow).max(0.03);
    let wrap = 0.15 + 0.14 * material.spread;
    let kd = 0.85;
    let e1 = js_round(110.0 / material.spread.powf(1.3)).min(90.0);
    let e2 = js_round(8.0 / material.spread).max(2.0);
    let lut1 = pow_lut(e1);
    let lut2 = pow_lut(e2);
    let ks1 = 0.45 * material.highlight;
    let ks2 = 0.10 * material.highlight;
    let win_k = 0.11 * material.highlight;
    let rim_k = 0.30 * material.rim;
    let amb_t = [amb * tint[0], amb * tint[1], amb * tint[2]];
    let m = M as usize;
    for j in 0..m {
        for i in 0..m {
            let mut nx = (i as f64 / (m as f64 - 1.0)) * 2.0 - 1.0;
            let mut ny = (j as f64 / (m as f64 - 1.0)) * 2.0 - 1.0;
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
            let nl = nx * lit[0] + ny * lit[1] + nz * lit[2];
            let nv = (nx * view[0] + ny * view[1] + nz * view[2]).max(0.0);
            let nh = (nx * half[0] + ny * half[1] + nz * half[2]).max(0.0);
            let dif = js_clamp01((nl + wrap) / (1.0 + wrap));
            let q = 1.0 - nv;
            let q2 = q * q;
            let f3 = q2 * q;
            let f5 = f3 * q2;
            let ni = ((nh * POW_N as f64).trunc() as isize).clamp(0, POW_N as isize) as usize;
            let spec = (ks1 * lut1[ni] as f64 + ks2 * lut2[ni] as f64) * (1.0 + 3.0 * f5);
            let rx = 2.0 * nv * nx - view[0];
            let ry = 2.0 * nv * ny - view[1];
            let rz = 2.0 * nv * nz - view[2];
            let sky = 0.45 + 0.55 * smooth(-0.4, 0.6, rx * up[0] + ry * up[1] + rz * up[2]);
            let rw = rx * win_dir[0] + ry * win_dir[1] + rz * win_dir[2];
            let mut win = 0.0;
            if rw > 0.5 {
                let ra = (rx * axis_a[0] + ry * axis_a[1] + rz * axis_a[2]) / rw;
                let rb = (rx * axis_b[0] + ry * axis_b[1] + rz * axis_b[2]) / rw;
                win = soft(0.34, 0.12, ra.abs()) * soft(0.12, 0.06, rb.abs());
            }
            let env = rim_k * f3 * sky + win_k * win;
            let k = (j * m + i) * 3;
            px[0] = c[0] * (amb_t[0] + kd * dif);
            px[1] = c[1] * (amb_t[1] + kd * dif);
            px[2] = c[2] * (amb_t[2] + kd * dif);
            vivify(&mut px, vivid, f64::INFINITY);
            out[k] = tone(px[0] + spec * WARM[0] + env * ENV[0]);
            out[k + 1] = tone(px[1] + spec * WARM[1] + env * ENV[1]);
            out[k + 2] = tone(px[2] + spec * WARM[2] + env * ENV[2]);
        }
    }
}

/// Bilinear read of the matcap at `(nx, ny)` (`sampleMatcap`, 724–730).
pub(crate) fn sample_matcap(mc: &[f32], nx: f64, ny: f64, out: &mut V3) {
    let m = M as f64;
    let fx = (nx * 0.5 + 0.5) * (m - 1.0);
    let fy = (ny * 0.5 + 0.5) * (m - 1.0);
    let cx = js_trunc_i32(fx).clamp(0, M as i32 - 2) as usize;
    let cy = js_trunc_i32(fy).clamp(0, M as i32 - 2) as usize;
    let x = fx - cx as f64;
    let y = fy - cy as f64;
    let b = (cy * M as usize + cx) * 3;
    let r = M as usize * 3;
    let w00 = (1.0 - x) * (1.0 - y);
    let w10 = x * (1.0 - y);
    let w01 = (1.0 - x) * y;
    let w11 = x * y;
    for ch in 0..3 {
        out[ch] = mc[b + ch] as f64 * w00
            + mc[b + 3 + ch] as f64 * w10
            + mc[b + r + ch] as f64 * w01
            + mc[b + r + 3 + ch] as f64 * w11;
    }
}

/// `edgeLift` (744–762).
pub fn edge_lift(form: &Form) -> Lift {
    let fp = form_fp(form);
    {
        let cache = lifts().lock().expect("lifts");
        if let Some(hit) = cache.get(&fp) {
            return hit.clone();
        }
    }
    let n = form.n as usize;
    let mut ao = vec![0u8; n * n];
    let mut k = vec![0.0f32; n * n];
    for i in 0..n * n {
        if form.ao[i] == 0 {
            continue;
        }
        let dd = (form.sd[i] as f64).max(0.0);
        let e = 1.0 - (dd / 3.0).min(1.0);
        let edge = 1.0 - 0.2 * e * e;
        let lin = (1.0_f64).min((form.ao[i] as f64 / 255.0).powf(2.2) / edge);
        ao[i] = round_u8((255.0 * lin.powf(1.0 / 2.2)).max(1.0));
        k[i] = 1.0;
    }
    let lift = Lift { ao, k };
    {
        let mut cache = lifts().lock().expect("lifts");
        if cache.len() >= 64 && !cache.contains_key(&fp) {
            cache.clear();
        }
        cache.insert(fp, lift.clone());
    }
    lift
}

/// Matcap lookup × baked AO into straight RGBA texels (`shadeTexels`, 770–797).
pub fn shade_texels(form: &Form, mc: &[f32], px: &mut [u8], ao_mul: &[f32], lift: Option<&Lift>) {
    let n = form.n as usize;
    let r = M as usize * 3;
    for i in 0..n * n {
        let k = i * 4;
        let a = form.ao[i];
        if a == 0 {
            px[k + 3] = 0;
            continue;
        }
        let mut m = if let Some(lift) = lift {
            ao_mul[lift.ao[i] as usize] as f64 * lift.k[i] as f64
        } else {
            ao_mul[a as usize] as f64
        };
        let b = form.i00[i] as usize * 3;
        let x = form.wx[i] as f64 * (1.0 / 255.0);
        let y = form.wy[i] as f64 * (1.0 / 255.0);
        if lift.is_some() {
            let g = (mc[b + 1] as f64 * (1.0 - x) + mc[b + 4] as f64 * x) * (1.0 - y)
                + (mc[b + r + 1] as f64 * (1.0 - x) + mc[b + r + 4] as f64 * x) * y;
            let ratio = g / 235.0;
            let clamped = if ratio < 1.0 { ratio } else { 1.0 };
            m = 1.0 - (1.0 - m) * (1.1 - 0.75 * clamped);
        }
        let w00 = (1.0 - x) * (1.0 - y) * m;
        let w10 = x * (1.0 - y) * m;
        let w01 = (1.0 - x) * y * m;
        let w11 = x * y * m;
        px[k] = to_u8_clamp(
            mc[b] as f64 * w00
                + mc[b + 3] as f64 * w10
                + mc[b + r] as f64 * w01
                + mc[b + r + 3] as f64 * w11,
        );
        px[k + 1] = to_u8_clamp(
            mc[b + 1] as f64 * w00
                + mc[b + 4] as f64 * w10
                + mc[b + r + 1] as f64 * w01
                + mc[b + r + 4] as f64 * w11,
        );
        px[k + 2] = to_u8_clamp(
            mc[b + 2] as f64 * w00
                + mc[b + 5] as f64 * w10
                + mc[b + r + 2] as f64 * w01
                + mc[b + r + 5] as f64 * w11,
        );
        let t = form.sd[i] as f64 / FRONT_FADE;
        px[k + 3] = if t >= 1.0 {
            255
        } else if t <= 0.0 {
            0
        } else {
            round_u8(255.0 * t * t * (3.0 - 2.0 * t))
        };
    }
}

#[cfg(test)]
mod tests {
    use super::{
        FRONT_FADE, Frame, M, Material, POW_N, bakes_pending, blur5, build_form, build_matcap,
        clock, edge_lift, edt_1d, edt_2d, form_for, hurry, linear_color, norm3, on_landed, poisson,
        pow_lut, rasterize, relief_for, sample_matcap, shade_texels, smooth, soft, srgb,
        surface_at, take_missed, tier_for, to_lin, tone, vivify, warm_plastic,
    };
    use crate::shapes::body_path_for;
    use crate::types::BotAvatarType;
    use std::sync::Mutex;
    use tiny_skia::PathBuilder;

    static GATE: Mutex<()> = Mutex::new(());

    fn rect(x0: f32, y0: f32, x1: f32, y1: f32) -> tiny_skia::Path {
        let mut pb = PathBuilder::new();
        pb.move_to(x0, y0);
        pb.line_to(x1, y0);
        pb.line_to(x1, y1);
        pb.line_to(x0, y1);
        pb.close();
        pb.finish().unwrap()
    }

    #[test]
    fn tier_for_matches_plastic_ts() {
        assert_eq!(tier_for(100.0), 64);
        assert_eq!(tier_for(100.1), 96);
        assert_eq!(tier_for(224.0), 96);
        assert_eq!(tier_for(224.1), 128);
    }

    #[test]
    fn synchronous_bake_is_not_pending() {
        assert!(!bakes_pending());
        assert!(!take_missed());
        hurry(true);
        hurry(false);
        let _off = on_landed(|| {});
        assert!(!bakes_pending());
        assert!(!take_missed());
    }

    #[test]
    fn clock_is_deterministic() {
        assert_eq!(clock(), 0.0);
    }

    #[test]
    fn norm_smooth_soft_and_colour_match_ts() {
        assert_eq!(norm3([0.0, 0.0, 0.0]), [0.0, 0.0, 0.0]);
        let n = norm3([3.0, 4.0, 12.0]);
        assert!((n[0] - 3.0 / 13.0).abs() < 1e-12);
        assert!((n[1] - 4.0 / 13.0).abs() < 1e-12);
        assert!((n[2] - 12.0 / 13.0).abs() < 1e-12);
        assert!((smooth(0.0, 1.0, 0.25) - 0.15625).abs() < 1e-15);
        assert!((soft(0.34, 0.12, 0.0) - 1.0).abs() < 1e-15);
        assert!((soft(0.34, 0.12, 0.34) - 0.5).abs() < 1e-15);
        assert!((to_lin(0.04045) - 0.0031308049535603713).abs() < 1e-15);
        assert!((to_lin(0.5) - 0.21404114048223255).abs() < 1e-12);
        assert!((srgb(0.2) - 123.55494714283527).abs() < 1e-9);
        assert!((tone(0.0) - 2.0108642578125).abs() < 1e-5);
        assert!((tone(0.5) - 187.4955291748047).abs() < 1e-4);
        assert!((tone(1.0) - 244.4158935546875).abs() < 1e-4);
        assert!((tone(2.5) - 254.97438049316406).abs() < 1e-4);
        let lut = pow_lut(2.0);
        assert_eq!(lut.len(), POW_N + 1);
        assert_eq!(lut[0], 0.0);
        assert_eq!(lut[256], 0.0625);
        assert_eq!(lut[512], 0.25);
        assert_eq!(lut[POW_N], 1.0);
        assert_eq!(pow_lut(2.0), lut);
        assert_eq!(linear_color("#ff0000"), [1.0, 0.0, 0.0]);
        assert_eq!(linear_color("red"), [1.0, 0.0, 0.0]);
        assert_eq!(linear_color("white"), [1.0, 1.0, 1.0]);
        let grey = linear_color("#808080");
        assert!((grey[0] - 0.21586050011389926).abs() < 1e-12);
        assert_eq!(grey[0], grey[1]);
        assert_eq!(grey[1], grey[2]);
        assert_eq!(linear_color("not-a-color"), [0.0, 0.0, 0.0]);
        let mut c = [0.2, 0.5, 0.8];
        vivify(&mut c, 0.5, f64::INFINITY);
        assert!(c[0].abs() < 1e-12);
        assert!((c[1] - 0.5326663564448582).abs() < 1e-12);
        assert!((c[2] - 1.0653327128897163).abs() < 1e-12);
        let mut held = [0.2, 0.4, 0.6];
        vivify(&mut held, 0.0, f64::INFINITY);
        assert_eq!(held, [0.2, 0.4, 0.6]);
    }

    #[test]
    fn distance_blur_and_poisson_match_hand_values() {
        let f = [0.0f32, 1e12f32, 0.0];
        let mut d = [0.0; 3];
        let mut s = [0; 3];
        let mut v = [0; 3];
        let mut z = [0.0; 4];
        edt_1d(&f, 3, &mut d, &mut s, &mut v, &mut z);
        assert_eq!(d, [0.0, 1.0, 0.0]);
        assert_eq!(s, [0, 0, 2]);

        let mask = [1u8, 1, 1, 1, 0, 1, 1, 1, 1];
        let mut dist = [0.0f32; 9];
        let mut near = [0i32; 9];
        edt_2d(&mask, 0, 3, &mut dist, Some(&mut near));
        assert_eq!(dist, [2.0, 1.0, 2.0, 1.0, 0.0, 1.0, 2.0, 1.0, 2.0]);
        assert_eq!(near, [4; 9]);

        let mut field = [1.0f32, 2.0, 3.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0];
        let mut tmp = [0.0f32; 9];
        blur5(&mut field, 3, &mut tmp);
        let expect = [
            0.9453125, 1.375, 1.8046875, 0.4296875, 0.625, 0.8203125, 0.0859375, 0.125, 0.1640625,
        ];
        for (got, exp) in field.iter().zip(expect) {
            assert!((got - exp).abs() < 1e-6, "{got} vs {exp}");
        }

        let phi = poisson(&[255u8; 64], 8, 106.0 / 8.0);
        assert_eq!(phi[0], 0.0);
        assert!(phi[4 * 8 + 4] > phi[1 * 8 + 1]);
        assert!(phi[4 * 8 + 4].is_finite());
        assert!(poisson(&[0u8; 16], 4, 1.0).iter().all(|v| *v == 0.0));
    }

    #[test]
    fn relief_of_a_step_matches_the_level_formula() {
        let form = super::Form {
            id: 0,
            n: 2,
            i00: vec![0; 4],
            wx: vec![0; 4],
            wy: vec![0; 4],
            ao: vec![1; 4],
            sd: vec![1.0; 4],
            h: vec![0.0, 0.0, 0.0, 4.0],
        };
        let rel = relief_for(&form, 2, 1.0);
        assert!((rel.top - 4.0).abs() < 1e-9);
        assert!((rel.q - 1.0).abs() < 1e-12);
        assert_eq!(rel.z[0], 0.0);
        assert_eq!(rel.z[1], 2.0);
        assert_eq!(rel.scale[0], 1.0);
        assert_eq!(rel.scale[1], 0.5);
        assert_eq!(rel.cx[0], 50.0);
        assert_eq!(rel.cy[0], 50.0);
        assert_eq!(rel.cx[1], 76.5);
        assert_eq!(rel.cy[1], 76.5);
        assert_eq!(rel.inset[0], 1.2);
        assert_eq!(rel.inset[1], 1.2);
        let again = relief_for(&form, 2, 1.0);
        assert_eq!(again.z, rel.z);
        assert_eq!(again.scale, rel.scale);
        for i in 0..10 {
            let round = 1.0 - i as f64 * 0.1;
            let q = round.clamp(0.05, 1.0);
            let got = relief_for(&form, 3, round);
            assert!((got.q - q).abs() < 1e-12, "round {round}");
        }
    }

    #[test]
    fn edge_lift_undoes_the_baked_darkening() {
        let form = super::Form {
            id: 0,
            n: 2,
            i00: vec![0; 4],
            wx: vec![0; 4],
            wy: vec![0; 4],
            ao: vec![0, 200, 200, 200],
            sd: vec![0.0, 0.0, 3.0, 10.0],
            h: vec![0.0; 4],
        };
        let lift = edge_lift(&form);
        assert_eq!(lift.ao[0], 0);
        assert_eq!(lift.k[0], 0.0);
        assert_eq!(lift.ao[1], 221);
        assert_eq!(lift.k[1], 1.0);
        assert_eq!(lift.ao[2], 200);
        assert_eq!(lift.ao[3], 200);
        assert_eq!(edge_lift(&form).ao, lift.ao);
    }

    #[test]
    fn matcap_sample_and_shade_match_one_cell() {
        let frame = Frame {
            l: norm3([0.2, -0.4, 0.9]),
            v: [0.0, 0.0, 1.0],
            h: norm3([
                norm3([0.2, -0.4, 0.9])[0] + 0.0,
                norm3([0.2, -0.4, 0.9])[1] + 0.0,
                norm3([0.2, -0.4, 0.9])[2] + 1.0,
            ]),
            u: [0.0, 1.0, 0.0],
            w: norm3([0.3, 0.2, 0.9]),
            a: {
                let w = norm3([0.3, 0.2, 0.9]);
                norm3([w[1], -w[0], 0.0])
            },
            b: {
                let w = norm3([0.3, 0.2, 0.9]);
                let a = norm3([w[1], -w[0], 0.0]);
                [
                    w[1] * a[2] - w[2] * a[1],
                    w[2] * a[0] - w[0] * a[2],
                    w[0] * a[1] - w[1] * a[0],
                ]
            },
            d: [0.0, 1.0, 0.0],
            k: [0.0, -1.0, 0.0],
        };
        let material = Material {
            shadow: 0.35,
            highlight: 1.3,
            spread: 1.55,
            rim: 0.5,
            vivid: Some(0.0),
            front: None,
            shine: None,
            back_soft: None,
            sheen: None,
        };
        let body = [to_lin(0.2), to_lin(0.6), to_lin(0.9)];
        let mut mc = vec![0.0f32; (M as usize) * (M as usize) * 3];
        build_matcap(&mut mc, body, &frame, &material);
        let k = (32 * M as usize + 32) * 3;
        let expect = [132.15367126464844, 184.3863067626953, 244.6263885498047];
        for ch in 0..3 {
            assert!(
                (mc[k + ch] - expect[ch]).abs() < 0.05,
                "ch {ch}: {} vs {}",
                mc[k + ch],
                expect[ch]
            );
        }
        assert_eq!(mc[0], 0.0, "corner outside the disc stays unwritten");
        let mut sample = [0.0; 3];
        mc.fill(0.0);
        let b = (31 * M as usize + 31) * 3;
        let r = M as usize * 3;
        mc[b] = 10.0;
        mc[b + 3] = 20.0;
        mc[b + r] = 30.0;
        mc[b + r + 3] = 40.0;
        sample_matcap(&mc, 0.0, 0.0, &mut sample);
        assert!((sample[0] - 25.0).abs() < 1e-4);

        let form = super::Form {
            id: 0,
            n: 2,
            i00: vec![0; 4],
            wx: vec![0; 4],
            wy: vec![0; 4],
            ao: vec![0, 10, 0, 0],
            sd: vec![0.0, (FRONT_FADE / 2.0) as f32, 0.0, 0.0],
            h: vec![0.0; 4],
        };
        let mut px = vec![7u8; 16];
        let mut ao_mul = vec![0.0f32; 256];
        ao_mul[10] = 0.5;
        mc.fill(0.0);
        mc[0] = 100.0;
        mc[1] = 50.0;
        mc[2] = 10.0;
        shade_texels(&form, &mc, &mut px, &ao_mul, None);
        assert_eq!(&px[0..4], &[7, 7, 7, 0]);
        assert_eq!(&px[4..8], &[50, 25, 5, 128]);

        let lift = super::Lift {
            ao: vec![0, 100, 0, 0],
            k: vec![0.0, 1.0, 0.0, 0.0],
        };
        ao_mul[100] = 0.4;
        mc[1] = 235.0;
        let mut px = vec![7u8; 16];
        shade_texels(&form, &mc, &mut px, &ao_mul, Some(&lift));
        assert_eq!(px[4], 79);
        assert_eq!(px[7], 128);
    }

    #[test]
    fn form_cache_hits_and_square_pillow_is_finite() {
        let _gate = GATE.lock().unwrap();
        let big = rect(10.0, 10.0, 90.0, 90.0);
        let small = rect(40.0, 40.0, 60.0, 60.0);
        let key = "cache-square";
        let first = form_for(key, &big, 32, 9.75, true).unwrap();
        let second = form_for(key, &small, 32, 9.75, false).unwrap();
        assert_eq!(first.h, second.h);
        assert_eq!(first.ao, second.ao);
        assert!(!bakes_pending());
        assert!(!take_missed());
        let center = surface_at(key, 9.75, 50.0, 50.0, 1.0).unwrap();
        let off = surface_at(key, 9.75, 30.0, 50.0, 1.0).unwrap();
        let off_flat = surface_at(key, 9.75, 30.0, 50.0, 0.0).unwrap();
        assert!(center > off);
        assert!(off_flat + 1e-6 >= off);
        assert_eq!(surface_at(key, 9.75, -10.0, -10.0, 1.0), Some(0.0));
        assert!(surface_at("missing-outline", 9.75, 50.0, 50.0, 1.0).is_none());
        warm_plastic(
            key,
            &small,
            64.0,
            0.65,
            false,
            &super::FUR_STOCK,
            295.0,
            None,
        );
        assert!(surface_at(key, 9.75, 50.0, 50.0, 1.0).is_some());

        let cov = rasterize(&big, 32).unwrap();
        assert_eq!(cov.len(), 32 * 32);
        assert!(cov[16 * 32 + 16] > 200);
        assert_eq!(cov[0], 0);
        // A small interior block, so the texture corner is past the 8-texel halo.
        let mut dot = vec![0u8; 32 * 32];
        for y in 12..20 {
            for x in 12..20 {
                dot[y * 32 + x] = 255;
            }
        }
        let built = build_form(&dot, 32, 9.75);
        assert!(built.h.iter().all(|v| v.is_finite()));
        assert!(built.ao.iter().any(|v| *v >= 1));
        assert_eq!(built.ao[0], 0);
        assert!(built.sd[0] < 0.0);
        assert!(built.sd[16 * 32 + 16] > 0.0);
        assert!(built.h[16 * 32 + 16] > 0.0);
    }

    #[test]
    fn every_avatar_outline_bakes_without_panic() {
        for kind in BotAvatarType::ALL {
            let path = body_path_for(kind);
            let cov = rasterize(path.as_ref(), 64).expect("canvas");
            let form = build_form(&cov, 64, 9.75);
            assert_eq!(form.ao.len(), 64 * 64, "{kind}");
            assert!(form.h.iter().all(|v| v.is_finite()), "{kind}");
            assert!(form.sd.iter().all(|v| v.is_finite()), "{kind}");
            assert!(
                form.ao.iter().any(|v| *v >= 1),
                "{kind} has no covered texel"
            );
            let rel = relief_for(&form, 4, 1.0);
            assert!(rel.top.is_finite() && rel.top > 0.0, "{kind}");
            assert!(rel.scale.iter().all(|s| s.is_finite()));
            let lift = edge_lift(&form);
            assert_eq!(lift.ao.len(), form.ao.len());
            let mc = vec![10.0f32; (M as usize) * (M as usize) * 3];
            let mut px = vec![0u8; 64 * 64 * 4];
            let ao_mul = vec![1.0f32; 256];
            shade_texels(&form, &mc, &mut px, &ao_mul, None);
            shade_texels(&form, &mc, &mut px, &ao_mul, Some(&lift));
        }
    }
}
