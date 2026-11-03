//! Hats, glasses, headphones, and the bow tie.
//!
//! Ports `wear.ts` (1317 lines). `frame_spec` is the `FRAMES` table. `Phones`
//! does not store `mesh::Layer` (that type borrows a `Proj`); `draw_phones`
//! builds layers for the pass it paints. Mesh and phone projections are cached
//! the way `keep` and `lastPhones` are.
//!
//! - 51–69 `Wear`, `WearRig`
//! - 73–148 `Marks`, `seat`, `marksFor`
//! - 154–188 `project`, `depthOf`, `tilt`, `add`, `along`, `polyPath`, `bounds`
//! - 191–196 `rnd`
//! - 202–312 `grainTile`, `grainOver`, `fuzz`, `contact`
//! - 317–377 `EL`, `lightOf`, `pxOf`, `viewOf`, `keep`, `surface`, `tube`, `rgbOf`, `TAU`, `smooth01`, `spow`
//! - 380–419 `sphereMesh`, `ringMesh`, `offsetPath`
//! - 423–465 `yarnBall`
//! - 467–543 `drawBeret`; 545–643 `drawBeanie`; 645–686 `drawParty`; 688–824 `drawCrown`
//! - 826–986 `Phones`, `phonesOf`; 990–1021 `drawPhones`
//! - 1029–1119 `drawBowTie`
//! - 1123–1142 `EyeSpot`, `FRAMES`
//! - 1150–1272 `drawGlasses`
//! - 1277–1283 `drawWearBehind`; 1288–1317 `drawWearFront`
#![allow(dead_code)]

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

use tiny_skia::{BlendMode, FillRule, LineCap, LineJoin, Path, PathBuilder, Pixmap, SpreadMode};

use crate::canvas::{Canvas2D, PaintStyle, circle_path, pixmap_from_straight_rgba};
use crate::color::{parse_color, shade};
use crate::mesh::{
    Bounds, Inside, Layer, Mesh, P2, Proj, Rgb, SpriteKind, V3, View, bounds_of, cross, cut_gem,
    felt, gold, hull, make_mesh, make_tube, make_view, metal, norm, paint_layers, paper, plastic,
    project_mesh, satin, scale, section, segs, silver, sphere_sprite, sub, velvet,
};
use crate::plastic::{PAD, SPAN};
use crate::types::{BotAvatarGlasses, BotAvatarHat};

const PI: f64 = std::f64::consts::PI;
/// Sentinel owner for glasses meshes (`owner = FRAMES` in `drawGlasses`).
const GLASSES_OWNER: u64 = 1;

thread_local! {
    static OWNER: Cell<u64> = const { Cell::new(0) };
    static MESHES: RefCell<HashMap<u64, HashMap<String, Mesh>>> = RefCell::new(HashMap::new());
    static MARKS_CACHE: RefCell<HashMap<u64, Marks>> = RefCell::new(HashMap::new());
    static LAST_PHONES: RefCell<Option<PhoneBuilt>> = const { RefCell::new(None) };
}

fn owner_id() -> u64 {
    OWNER.with(|c| c.get())
}

fn set_owner(id: u64) {
    OWNER.with(|c| c.set(id));
}

fn path_owner(path: &Path) -> u64 {
    let id = crate::plastic::path_fingerprint(path);
    if id <= 1 { id | (1_u64 << 63) } else { id }
}

fn segs11(view: &View, len: f64, lo: usize, hi: usize) -> usize {
    segs(view, len, lo, hi, 11.0)
}

/// `EL` (317): light elevation off the screen, matching the body material.
pub(crate) const EL: f64 = (48.0 * std::f64::consts::PI) / 180.0;
pub(crate) const TAU: f64 = std::f64::consts::PI * 2.0;
/// Felt grain tile, `grainTile` (202).
pub(crate) const GRAIN_N: u32 = 48;
/// `rgbOf` fallback (373).
pub(crate) const RGB_FALLBACK: Rgb = [40.0, 40.0, 44.0];

/// What the bot wears. `fabric` defaults to false when the caller omits it.
#[derive(Debug, Clone, PartialEq)]
pub struct Wear {
    pub hat: BotAvatarHat,
    pub glasses: BotAvatarGlasses,
    pub headphones: bool,
    pub bow_tie: bool,
    /// Hat, headphones, and bow tie.
    pub color: String,
    /// Plush body: the bow tie is felt rather than satin.
    pub fabric: bool,
}

/// The rig the slices use: floored cos/sin of yaw and pitch, unfloored facing,
/// half-depth, and the light on screen (Y down).
#[derive(Debug, Clone, Copy)]
pub struct WearRig {
    pub cy: f64,
    pub sy: f64,
    pub cp: f64,
    pub sp: f64,
    pub facing: f64,
    pub half_depth: f64,
    pub lx: f64,
    pub ly: f64,
}

/// Landmarks of one outline, in design units (`Marks`, 73–89).
#[derive(Debug, Clone)]
pub struct Marks {
    pub top: f64,
    pub top_l: f64,
    pub top_r: f64,
    pub mid: f64,
    pub left: f64,
    pub right: f64,
    pub bottom: f64,
    /// Row widths, top to bottom, one row every `step` design units.
    pub widths: Vec<f32>,
    pub step: f64,
    pub hull: Vec<P2>,
}

/// Where a feature sits on the face sphere (`EyeSpot`, 1123–1133).
#[derive(Debug, Clone, Copy)]
pub struct EyeSpot {
    pub x: f64,
    pub y: f64,
    pub sx: f64,
    pub sy: f64,
    pub z: f64,
    pub ox: f64,
    pub oy: f64,
    pub fz: f64,
}

/// One lens frame (`FRAMES`, 1138–1142). `n` is the superellipse exponent.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct FrameSpec {
    pub rx: f64,
    pub ry: f64,
    pub n: f64,
    pub tr: f64,
    pub td: f64,
}

pub(crate) const FRAME_ROUND: FrameSpec = FrameSpec {
    rx: 10.8,
    ry: 10.8,
    n: 2.0,
    tr: 1.15,
    td: 1.2,
};
pub(crate) const FRAME_SQUARE: FrameSpec = FrameSpec {
    rx: 12.0,
    ry: 9.6,
    n: 4.2,
    tr: 1.45,
    td: 1.4,
};
pub(crate) const FRAME_SHADES: FrameSpec = FrameSpec {
    rx: 12.6,
    ry: 10.0,
    n: 3.4,
    tr: 1.6,
    td: 1.5,
};

/// `FRAMES[kind]`, or `None` for `none`.
pub(crate) fn frame_spec(kind: BotAvatarGlasses) -> Option<FrameSpec> {
    Some(match kind {
        BotAvatarGlasses::None => return None,
        BotAvatarGlasses::Round => FRAME_ROUND,
        BotAvatarGlasses::Square => FRAME_SQUARE,
        BotAvatarGlasses::Shades => FRAME_SHADES,
    })
}

/// Screen light passed to [`draw_glasses`].
#[derive(Debug, Clone, Copy)]
pub struct ScreenLight {
    pub lx: f64,
    pub ly: f64,
}

/// Head turn for the glasses. The TypeScript default is yaw 0, pitch 0.
#[derive(Debug, Clone, Copy)]
pub struct HeadTurn {
    pub yaw: f64,
    pub pitch: f64,
}

impl Default for HeadTurn {
    fn default() -> Self {
        Self {
            yaw: 0.0,
            pitch: 0.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GlassesPass {
    Shadow,
    Frame,
}

/// Face anchor passed to [`draw_wear_front`]: design `y` and `scale`.
#[derive(Debug, Clone, Copy)]
pub struct FaceAnchor {
    pub y: f64,
    pub scale: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PhonePass {
    Behind,
    Front,
}

#[derive(Debug, Clone, Copy)]
pub struct PhoneSeat {
    pub c: V3,
    pub ry: f64,
}

/// Headphones without stored layers (`Phones`, 826–833, minus `layers`).
#[derive(Debug, Clone)]
pub struct Phones {
    pub pivots: Vec<V3>,
    pub pivot_r: f64,
    pub stitch: Vec<V3>,
    pub seats: Vec<PhoneSeat>,
}

/// `lightOf` (318–322).
pub(crate) struct LightVecs {
    pub l: V3,
    pub h: V3,
}

/// First row from the top where the head is at least `frac` of its full width (`seat`, 94–98).
pub(crate) fn seat(m: &Marks, frac: f64) -> f64 {
    let w = m.right - m.left;
    let need = frac * w;
    for (i, width) in m.widths.iter().enumerate() {
        if (*width as f64) >= need {
            return (i as f64) * m.step - PAD;
        }
    }
    m.top
}

/// Outline landmarks, cached per path (`marksFor`, 100–148). Uses [`crate::plastic::rasterize`].
pub(crate) fn marks_for(path: &Path) -> Option<Marks> {
    let id = path_owner(path);
    if let Some(hit) = MARKS_CACHE.with(|c| c.borrow().get(&id).cloned()) {
        return Some(hit);
    }
    let n: i32 = 128;
    let cov = crate::plastic::rasterize(path, n as u32)?;
    let nu = (n as usize) * (n as usize);
    if cov.len() < nu {
        return None;
    }
    let u = SPAN / (n as f64);
    let to_x = |i: i32| (i as f64 + 0.5) * u - PAD;
    let inside = |x: i32, y: i32| -> bool {
        if x < 0 || y < 0 || x >= n || y >= n {
            return false;
        }
        cov[(y as usize) * (n as usize) + (x as usize)] >= 128
    };
    let row = |y: i32| -> (i32, i32) {
        if y < 0 || y >= n {
            return (-1, -1);
        }
        let mut l = -1;
        let mut r = -1;
        for x in 0..n {
            if inside(x, y) {
                if l < 0 {
                    l = x;
                }
                r = x;
            }
        }
        (l, r)
    };
    let c0 = ((42.0 + PAD) / u).floor() as i32;
    let c1 = ((58.0 + PAD) / u).ceil() as i32;
    let mut top = n;
    let mut bottom = -1;
    for x in c0..=c1 {
        for y in 0..n {
            if inside(x, y) {
                if y < top {
                    top = y;
                }
                break;
            }
        }
        for y in (0..n).rev() {
            if inside(x, y) {
                if y > bottom {
                    bottom = y;
                }
                break;
            }
        }
    }
    let y_t = (n - 1).min(top + ((8.0 / u).round() as i32));
    let (tl, tr) = row(y_t);
    let mut best = -1;
    let mut bl = 0;
    let mut br = 0;
    let mut by = 0;
    let y0 = ((33.0 + PAD) / u).round() as i32;
    let y1 = ((67.0 + PAD) / u).round() as i32;
    for y in y0..=y1 {
        let (l, r) = row(y);
        if l >= 0 && r - l > best {
            best = r - l;
            bl = l;
            br = r;
            by = y;
        }
    }
    let mut widths = vec![0.0_f32; n as usize];
    let mut edge = Vec::new();
    for y in 0..n {
        let (l, r) = row(y);
        widths[y as usize] = if l < 0 {
            0.0
        } else {
            ((r - l + 1) as f64 * u) as f32
        };
        if l >= 0 {
            edge.push([to_x(l) - 0.5 * u, to_x(y)]);
            edge.push([to_x(r) + 0.5 * u, to_x(y)]);
        }
    }
    let m = Marks {
        top: to_x(top) - 0.5 * u,
        top_l: to_x(tl),
        top_r: to_x(tr),
        mid: to_x(by),
        left: to_x(bl) - 0.5 * u,
        right: to_x(br) + 0.5 * u,
        bottom: to_x(bottom) + 0.5 * u,
        widths,
        step: u,
        hull: hull(&edge),
    };
    MARKS_CACHE.with(|c| {
        let mut cache = c.borrow_mut();
        if cache.len() > 512 {
            cache.clear();
        }
        cache.insert(id, m.clone());
    });
    Some(m)
}

/// Orthographic turn (`project`, 154–157).
pub(crate) fn project(r: &WearRig, p: V3) -> P2 {
    let x = p[0] - 50.0;
    let y = p[1] - 50.0;
    [
        r.cy * x + r.sy * p[2],
        r.sy * r.sp * x + r.cp * y - r.cy * r.sp * p[2],
    ]
}

/// Depth toward the viewer (`depthOf`, 159–161).
pub(crate) fn depth_of(r: &WearRig, p: V3) -> f64 {
    -r.sy * r.cp * (p[0] - 50.0) + r.sp * (p[1] - 50.0) + r.cy * r.cp * p[2]
}

/// Local offset: roll about depth, then lean about x (`tilt`, 164–169).
pub(crate) fn tilt(v: V3, roll: f64, lean: f64) -> V3 {
    let (sr, cr) = roll.sin_cos();
    let x = v[0] * cr - v[1] * sr;
    let y = v[0] * sr + v[1] * cr;
    let (sl, cl) = lean.sin_cos();
    [x, y * cl - v[2] * sl, y * sl + v[2] * cl]
}

pub(crate) fn add(a: V3, b: V3) -> V3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

pub(crate) fn along(a: V3, d: V3, k: f64) -> V3 {
    [a[0] + d[0] * k, a[1] + d[1] * k, a[2] + d[2] * k]
}

/// `polyPath` (173–178). `close` defaults to true.
pub(crate) fn poly_path(pts: &[P2], close: bool) -> Path {
    let mut pb = PathBuilder::new();
    if pts.is_empty() {
        pb.move_to(0.0, 0.0);
        return pb.finish().expect("empty poly");
    }
    pb.move_to(pts[0][0] as f32, pts[0][1] as f32);
    for q in &pts[1..] {
        pb.line_to(q[0] as f32, q[1] as f32);
    }
    if close {
        pb.close();
    }
    pb.finish().expect("poly path")
}

/// `bounds` (179–188).
pub(crate) fn bounds(pts: &[P2]) -> Bounds {
    let mut x0 = f64::INFINITY;
    let mut y0 = f64::INFINITY;
    let mut x1 = f64::NEG_INFINITY;
    let mut y1 = f64::NEG_INFINITY;
    for p in pts {
        if p[0] < x0 {
            x0 = p[0];
        }
        if p[1] < y0 {
            y0 = p[1];
        }
        if p[0] > x1 {
            x1 = p[0];
        }
        if p[1] > y1 {
            y1 = p[1];
        }
    }
    Bounds {
        x0,
        y0,
        x1,
        y1,
        cx: (x0 + x1) / 2.0,
        cy: (y0 + y1) / 2.0,
        w: x1 - x0,
        h: y1 - y0,
    }
}

/// JS `ToInt32` for the `rnd` hash.
fn to_int32(n: f64) -> i32 {
    if !n.is_finite() {
        return 0;
    }
    let m = n.trunc().rem_euclid(4294967296.0);
    (m as u32) as i32
}

/// Stable 0..1 hash (`rnd`, 191–196). `k` defaults to 0.
pub(crate) fn rnd(i: i32, k: i32) -> f64 {
    let mut h = to_int32((i as f64) * 2654435761.0 + (k as f64) * 40503.0);
    let u = (h as u32) ^ ((h as u32) >> 15);
    h = (u as i32).wrapping_mul(2246822507u32 as i32);
    let u = (h as u32) ^ ((h as u32) >> 13);
    h = (u as i32).wrapping_mul(3266489909u32 as i32);
    let out = (h as u32) ^ ((h as u32) >> 16);
    (out as f64) / 4294967296.0
}

fn grain_arc() -> &'static Arc<Pixmap> {
    static GRAIN: OnceLock<Arc<Pixmap>> = OnceLock::new();
    GRAIN.get_or_init(|| {
        let n = GRAIN_N as usize;
        let mut rgba = vec![0_u8; n * n * 4];
        for i in 0..(n * n) {
            let x = i % n;
            let y = i / n;
            let mut v = 0.0;
            for k in -2..=2 {
                let xi = (x as i32 + k + n as i32) as usize % n;
                let yi = (y as i32 + k + n as i32) as usize % n;
                v += rnd((xi + yi * n) as i32, 5);
            }
            v /= 5.0;
            let m = ((255.0 * (0.82 + 0.36 * v)).round() as i32).clamp(0, 255) as u8;
            let o = i * 4;
            rgba[o] = m;
            rgba[o + 1] = m;
            rgba[o + 2] = m;
            rgba[o + 3] = 255;
        }
        Arc::new(pixmap_from_straight_rgba(GRAIN_N, GRAIN_N, &rgba))
    })
}

/// Felt grain tile (`grainTile`, 202–222).
pub(crate) fn grain_tile() -> Option<Pixmap> {
    Some((**grain_arc()).clone())
}

/// Multiply the grain over an outline (`grainOver`, 227–247). `scale_k` defaults to 0.18.
///
/// `PaintStyle::Pattern` has no pattern matrix. Scaling the current transform by
/// `scale_k` and filling the rect in that space matches `CanvasPattern.setTransform(scale)`.
pub(crate) fn grain_over(ctx: &mut Canvas2D, outline: &[P2], alpha: f64, scale_k: f64) {
    if outline.len() < 3 || !(scale_k > 0.0) || !scale_k.is_finite() {
        return;
    }
    let b = bounds(outline);
    let path = poly_path(outline, true);
    ctx.save();
    ctx.clip_path(&path, FillRule::Winding);
    ctx.set_blend_mode(BlendMode::Multiply);
    ctx.set_global_alpha(alpha);
    ctx.transform(scale_k, 0.0, 0.0, scale_k, 0.0, 0.0);
    ctx.set_fill_style(PaintStyle::Pattern {
        pixmap: Arc::clone(grain_arc()),
        spread: SpreadMode::Repeat,
    });
    ctx.fill_rect(
        (b.x0 - 1.0) / scale_k,
        (b.y0 - 1.0) / scale_k,
        (b.w + 2.0) / scale_k,
        (b.h + 2.0) / scale_k,
    );
    ctx.restore();
}

fn stroke_built(ctx: &mut Canvas2D, pb: PathBuilder) {
    if let Some(path) = pb.finish() {
        ctx.stroke_path(&path);
    }
}

/// Short hairs off an outline (`fuzz`, 251–294). `px` defaults to 2.4.
pub(crate) fn fuzz(ctx: &mut Canvas2D, outline: &[P2], base: &str, seed: f64, len: f64, px: f64) {
    let n = outline.len();
    if n < 3 {
        return;
    }
    let mut total = 0.0;
    let mut seg = Vec::with_capacity(n);
    for i in 0..n {
        let a = outline[i];
        let c = outline[(i + 1) % n];
        let d = (c[0] - a[0]).hypot(c[1] - a[1]);
        seg.push(d);
        total += d;
    }
    let count = (total * (px * 0.5).min(2.4)).round().clamp(24.0, 360.0) as i32;
    let b = bounds(outline);
    let mut light_p = PathBuilder::new();
    let mut dark_p = PathBuilder::new();
    let mut si = 0_usize;
    let mut acc = 0.0;
    let seed_i = seed as i32;
    for k in 0..count {
        let want = (k as f64 + rnd(k, seed_i) * 0.8) * (total / (count as f64));
        while si < n - 1 && acc + seg[si] < want {
            acc += seg[si];
            si += 1;
        }
        let a = outline[si];
        let c = outline[(si + 1) % n];
        let t = if seg[si] != 0.0 {
            (want - acc) / seg[si]
        } else {
            0.0
        };
        let x = a[0] + (c[0] - a[0]) * t;
        let y = a[1] + (c[1] - a[1]) * t;
        let mut ox = x - b.cx;
        let mut oy = y - b.cy;
        let ol = ox.hypot(oy);
        let ol = if ol == 0.0 { 1.0 } else { ol };
        ox /= ol;
        oy /= ol;
        let ang = (rnd(k, seed_i + 1) - 0.5) * 0.9;
        let (sa, ca) = ang.sin_cos();
        let hx = ox * ca - oy * sa;
        let hy = ox * sa + oy * ca;
        let hair = len * (0.35 + rnd(k, seed_i + 2));
        let pb = if rnd(k, seed_i + 3) > 0.5 {
            &mut light_p
        } else {
            &mut dark_p
        };
        pb.move_to((x - hx * 0.4) as f32, (y - hy * 0.4) as f32);
        pb.line_to((x + hx * hair) as f32, (y + hy * hair) as f32);
    }
    ctx.save();
    ctx.set_line_cap(LineCap::Round);
    ctx.set_line_width(0.4);
    ctx.set_global_alpha(0.55);
    ctx.set_stroke_css(&shade(base, 0.14, 0.0));
    stroke_built(ctx, light_p);
    ctx.set_stroke_css(&shade(base, -0.06, 0.0));
    stroke_built(ctx, dark_p);
    ctx.restore();
}

/// Soft contact shadow (`contact`, 297–312).
pub(crate) fn contact(
    ctx: &mut Canvas2D,
    clip: Option<&Path>,
    cx: f64,
    cy: f64,
    rx: f64,
    ry: f64,
    a: f64,
) {
    if !(rx > 0.0) || !(ry > 0.0) {
        return;
    }
    let c0 = format!("rgba(0,0,0,{a})");
    let c1 = format!("rgba(0,0,0,{})", a * 0.45);
    ctx.save();
    if let Some(clip) = clip {
        ctx.clip_path(clip, FillRule::Winding);
    }
    ctx.translate(cx, cy);
    ctx.scale(1.0, ry / rx);
    ctx.set_fill_style(PaintStyle::radial_gradient(
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        rx,
        &[
            (0.0, c0.as_str()),
            (0.55, c1.as_str()),
            (1.0, "rgba(0,0,0,0)"),
        ],
    ));
    if let Some(path) = circle_path(0.0, 0.0, rx) {
        ctx.fill_path(&path);
    }
    ctx.restore();
}

pub(crate) fn light_of(lx: f64, ly: f64) -> LightVecs {
    let le = EL.cos();
    let lz = EL.sin();
    let l = norm([lx * le, ly * le, lz]);
    let h = norm([l[0], l[1], l[2] + 1.0]);
    LightVecs { l, h }
}

/// Device pixels per unit of the current transform (`pxOf`, 324–327).
pub(crate) fn px_of(ctx: &Canvas2D) -> f64 {
    let t = ctx.get_transform();
    let det = (t[0] * t[3] - t[1] * t[2]).abs().sqrt();
    if det == 0.0 { 1.0 } else { det }
}

/// `viewOf` (328–332).
pub(crate) fn view_of(r: &WearRig, px: f64) -> View {
    let light = light_of(r.lx, r.ly);
    make_view(
        [
            r.cy,
            0.0,
            r.sy,
            r.sy * r.sp,
            r.cp,
            -r.cy * r.sp,
            -r.sy * r.cp,
            r.sp,
            r.cy * r.cp,
        ],
        [50.0, 50.0, 0.0],
        light.l,
        light.h,
        px,
    )
}

fn cache_mesh(key: &str, make: impl FnOnce() -> Mesh) -> bool {
    let owner = owner_id();
    let missing = MESHES.with(|cell| {
        cell.borrow()
            .get(&owner)
            .and_then(|map| map.get(key).map(|_| ()))
            .is_none()
    });
    if !missing {
        return false;
    }
    let mesh = make();
    MESHES.with(|cell| {
        let mut maps = cell.borrow_mut();
        let map = maps.entry(owner).or_default();
        if map.contains_key(key) {
            return;
        }
        if map.len() > 240 {
            map.clear();
        }
        map.insert(key.to_string(), mesh);
    });
    true
}

fn project_cached(view: &View, key: &str) -> Proj {
    let owner = owner_id();
    MESHES.with(|cell| {
        let maps = cell.borrow();
        let mesh = maps
            .get(&owner)
            .and_then(|map| map.get(key))
            .expect("mesh stored");
        project_mesh(view, mesh)
    })
}

/// Mesh cache (`keep`, 339–349). The owner key is internal.
pub(crate) fn keep(key: &str, make: impl FnOnce() -> Mesh) -> Mesh {
    cache_mesh(key, make);
    let owner = owner_id();
    MESHES.with(|cell| {
        cell.borrow()
            .get(&owner)
            .and_then(|map| map.get(key).cloned())
            .expect("mesh stored")
    })
}

/// Cached parametric surface, projected (`surface`, 350–360). `smooth` defaults to true.
pub(crate) fn surface(
    view: &View,
    key: &str,
    surf: &dyn Fn(f64, f64) -> V3,
    nu: usize,
    nv: usize,
    inside: Inside<'_>,
    smooth: bool,
) -> Proj {
    let cache_key = format!("{key}|{nu}x{nv}");
    cache_mesh(&cache_key, || make_mesh(surf, nu, nv, inside, smooth));
    project_cached(view, &cache_key)
}

/// Cached tube, projected (`tube`, 361–372). `closed` defaults to false.
pub(crate) fn tube(
    view: &View,
    key: &str,
    path: &dyn Fn(f64) -> V3,
    up: V3,
    prof: &dyn Fn(f64, f64) -> [f64; 2],
    nu: usize,
    nv: usize,
    closed: bool,
) -> Proj {
    let cache_key = format!("{key}|{nu}x{nv}");
    cache_mesh(&cache_key, || make_tube(path, up, prof, nu, nv, closed));
    project_cached(view, &cache_key)
}

/// `rgbOf` (373).
pub(crate) fn rgb_of(c: &str, fallback: Rgb) -> Rgb {
    parse_color(c).unwrap_or(fallback)
}

pub(crate) fn smooth01(t: f64) -> f64 {
    if t <= 0.0 {
        0.0
    } else if t >= 1.0 {
        1.0
    } else {
        t * t * (3.0 - 2.0 * t)
    }
}

/// JS `Math.sign`: zero stays zero (Rust `signum` maps `0.0` to `1.0`).
fn js_sign(c: f64) -> f64 {
    if c > 0.0 {
        1.0
    } else if c < 0.0 {
        -1.0
    } else {
        c
    }
}

/// Signed power (`spow`, 377).
pub(crate) fn spow(c: f64, e: f64) -> f64 {
    js_sign(c) * c.abs().powf(e)
}

pub(crate) fn sphere_mesh(view: &View, key: &str, c: V3, rad: f64, nu: usize, nv: usize) -> Proj {
    surface(
        view,
        key,
        &|u, v| {
            let th = u * TAU;
            let ph = v * PI;
            [
                c[0] + rad * ph.sin() * th.cos(),
                c[1] - rad * ph.cos(),
                c[2] + rad * ph.sin() * th.sin(),
            ]
        },
        nu,
        nv,
        Inside::Point(c),
        true,
    )
}

pub(crate) fn ring_mesh(
    view: &View,
    key: &str,
    place: &dyn Fn(V3) -> V3,
    ring_r: f64,
    y: f64,
    t: f64,
    z_scale: f64,
    nu: usize,
    nv: usize,
) -> Proj {
    let surf = |u: f64, v: f64| {
        let th = u * TAU;
        let ph = v * TAU;
        let rr = ring_r + t * ph.cos();
        place([rr * th.cos(), y - t * ph.sin(), rr * z_scale * th.sin()])
    };
    let inside = |u: f64, _v: f64| {
        place([
            ring_r * (u * TAU).cos(),
            y,
            ring_r * z_scale * (u * TAU).sin(),
        ])
    };
    surface(view, key, &surf, nu, nv, Inside::Fn(&inside), true)
}

/// Path moved across `up` (`offsetPath`, 412–419).
pub(crate) fn offset_path(
    path: Box<dyn Fn(f64) -> V3 + Send>,
    up: V3,
    off: Box<dyn Fn(f64) -> f64 + Send>,
) -> Box<dyn Fn(f64) -> V3 + Send> {
    Box::new(move |u: f64| {
        let e = 1e-3;
        let t = norm(sub(path((u + e).min(1.0)), path((u - e).max(0.0))));
        let b = cross(t, up);
        along(path(u), b, off(u))
    })
}

pub(crate) fn yarn_ball(ctx: &mut Canvas2D, view: &View, c: V3, rad: f64, base: &str, seed: f64) {
    let rgb = rgb_of(base, RGB_FALLBACK);
    let n = segs11(view, TAU * rad, 12, 20);
    let nv = (n >> 1).max(6);
    let ball = sphere_mesh(view, &format!("ball{seed}"), c, rad * 0.86, n, nv);
    let shade_fn = felt(view, rgb, 0.92);
    let vis = paint_layers(ctx, &[Layer::new(vec![&ball], shade_fn)], view.px);
    let centre = view.to_screen(c);
    let b = bounds(&vis);
    let r = b.w.max(b.h) / 2.0;
    let count_f = 340.0_f64.min(50.0 + r * view.px * 5.0);
    let count = if count_f.is_finite() && count_f > 0.0 {
        count_f.round() as i32
    } else {
        0
    };
    let mut light = PathBuilder::new();
    let mut dark = PathBuilder::new();
    let seed_i = seed as i32;
    for k in 0..count {
        let a = rnd(k, seed_i) * TAU;
        let r0 = r * (0.25 + 0.7 * rnd(k, seed_i + 1).sqrt());
        let len = r * (0.25 + 0.35 * rnd(k, seed_i + 2));
        let x0 = centre[0] + a.cos() * r0;
        let y0 = centre[1] + a.sin() * r0;
        let bend = (rnd(k, seed_i + 3) - 0.5) * 0.6;
        let ex = (a + bend).cos();
        let ey = (a + bend).sin();
        let pb = if rnd(k, seed_i + 4) > 0.45 {
            &mut light
        } else {
            &mut dark
        };
        pb.move_to(x0 as f32, y0 as f32);
        pb.quad_to(
            (x0 + a.cos() * len * 0.5) as f32,
            (y0 + a.sin() * len * 0.5) as f32,
            (x0 + ex * len) as f32,
            (y0 + ey * len) as f32,
        );
    }
    ctx.save();
    ctx.set_line_cap(LineCap::Round);
    let lw = (r * 0.06).max(0.3);
    if lw.is_finite() {
        ctx.set_line_width(lw);
    }
    let l = view.l;
    if r.is_finite() {
        let g0 = shade(base, 0.2, -0.03);
        let g1 = shade(base, -0.12, 0.02);
        ctx.set_stroke_style(PaintStyle::linear_gradient(
            centre[0] + l[0] * r,
            centre[1] + l[1] * r,
            centre[0] - l[0] * r,
            centre[1] - l[1] * r,
            &[(0.0, g0.as_str()), (1.0, g1.as_str())],
        ));
    }
    ctx.set_global_alpha(0.85);
    stroke_built(ctx, light);
    ctx.set_stroke_css(&shade(base, -0.18, 0.03));
    ctx.set_global_alpha(0.6);
    stroke_built(ctx, dark);
    ctx.restore();
}

pub(crate) fn draw_beret(
    ctx: &mut Canvas2D,
    view: &View,
    m: &Marks,
    base: &str,
    body_clip: Option<&Path>,
) {
    let w = m.right - m.left;
    let radius = 0.46 * w;
    let hh = 0.21 * w;
    let rb = 0.33 * w;
    let lift = 0.05 * w;
    let c = [
        50.0 - 0.07 * w,
        (m.top + 0.22 * w).max(seat(m, 0.5) + 0.1 * w),
        0.0,
    ];
    let roll = -0.24;
    let lean = -0.2;
    let place = |p: V3| add(c, tilt(p, roll, lean));
    let nu = segs11(view, TAU * radius, 32, 56);
    let nv = segs(view, 1.2 * radius, 10, 32, 6.0);
    let surf = |u: f64, v: f64| {
        let th = u * TAU;
        let e = 0.74;
        let (mut rr, hh_y) = if v <= e {
            let phi = (v / e) * (PI / 2.0);
            (radius * phi.sin(), -(lift + hh * phi.cos().powf(0.8)))
        } else {
            let f = smooth01((v - e) / (1.0 - e));
            (radius + (rb - radius) * f, -lift * (1.0 - f))
        };
        let band = ((v / e).min(1.0) * PI * 0.9).sin();
        rr *= 1.0 + 0.035 * (th * 7.0 + 0.4).sin() * band;
        place([rr * th.cos(), hh_y, rr * 0.93 * th.sin()])
    };
    let inside_pt = place([0.0, -lift - hh * 0.4, 0.0]);
    let dome = surface(
        view,
        "beret:dome",
        &surf,
        nu,
        nv,
        Inside::Point(inside_pt),
        true,
    );
    let under_surf = |u: f64, v: f64| {
        let th = u * TAU;
        let rr = rb * (1.0 - v);
        place([rr * th.cos(), -0.004 * w * v, rr * 0.93 * th.sin()])
    };
    let under = surface(
        view,
        "beret:under",
        &under_surf,
        24,
        2,
        Inside::Point(inside_pt),
        true,
    );
    let rgb = rgb_of(base, RGB_FALLBACK);
    let band_mid = view.to_screen(place([0.0, 0.02 * w, 0.0]));
    contact(
        ctx,
        body_clip,
        band_mid[0] + 0.02 * w,
        band_mid[1] + 0.045 * w,
        rb * 1.25,
        0.08 * w,
        0.4,
    );
    let binding = ring_mesh(
        view,
        "beret:binding",
        &place,
        rb,
        -0.005 * w,
        0.022 * w,
        0.93,
        segs11(view, TAU * rb, 24, 48),
        5,
    );
    let leather = rgb_of(&shade(base, -0.06, 0.0), RGB_FALLBACK);
    let vis = paint_layers(
        ctx,
        &[
            Layer::new(vec![&under], felt(view, rgb, 0.7)),
            Layer::new(vec![&binding], plastic(view, leather, 0.45)),
            Layer::new(vec![&dome], felt(view, rgb, 1.0)),
        ],
        view.px,
    );
    let outline = hull(&vis);
    grain_over(ctx, &outline, 0.5, 0.14);
    fuzz(ctx, &outline, base, 11.0, 0.4, view.px);
    let x0 = 0.02 * w;
    let y0 = -lift - hh + 0.006 * w;
    let stalk_path = |u: f64| place([x0 + 0.014 * w * u * u, y0 - 0.045 * w * u, 0.0]);
    let stalk_prof = |u: f64, ph: f64| {
        let k = 0.016 * w * (1.0 - 0.25 * u);
        [k * ph.cos(), k * ph.sin()]
    };
    let stalk = tube(
        view,
        "beret:stalk",
        &stalk_path,
        [0.0, 0.0, 1.0],
        &stalk_prof,
        8,
        3,
        false,
    );
    paint_layers(
        ctx,
        &[Layer::new(vec![&stalk], felt(view, rgb, 0.95))],
        view.px,
    );
    sphere_sprite(
        ctx,
        view,
        place([x0 + 0.014 * w, y0 - 0.047 * w, 0.0]),
        0.019 * w,
        SpriteKind::Rgb(scale(rgb, 1.15)),
    );
}

pub(crate) fn draw_beanie(
    ctx: &mut Canvas2D,
    view: &View,
    m: &Marks,
    base: &str,
    body_clip: Option<&Path>,
) {
    let w = m.right - m.left;
    let head_w = (0.5 * w).max(m.top_r - m.top_l);
    let rd = 0.53 * head_w + 0.08 * w;
    let hd = 0.32 * w;
    let cuff_h = 0.1 * w;
    let rc = rd * 1.05;
    let c = [50.0, seat(m, 0.72) + 0.06 * w, 0.0];
    let roll = 0.0;
    let lean = -0.14;
    let place = |p: V3| add(c, tilt(p, roll, lean));
    let dome_surf = |u: f64, v: f64| {
        let th = u * TAU;
        let phi = v * (PI / 2.0);
        let rr = rd * phi.sin() * (1.0 - 0.03 * (1.0 - v));
        let hh = -(cuff_h + hd * phi.cos());
        place([rr * th.cos(), hh, rr * 0.92 * th.sin()])
    };
    let in_dome = place([0.0, -cuff_h - hd * 0.4, 0.0]);
    let dome = surface(
        view,
        "beanie:dome",
        &dome_surf,
        segs11(view, TAU * rd, 28, 48),
        segs(view, 1.3 * hd, 9, 24, 6.0),
        Inside::Point(in_dome),
        true,
    );
    let cuff_surf = |u: f64, v: f64| {
        let th = u * TAU;
        let e = 2.0 * v - 1.0;
        let rr = rc * (1.0 - 0.07 * e * e * e * e) * 1.01;
        place([rr * th.cos(), -cuff_h * (1.0 - v), rr * 0.92 * th.sin()])
    };
    let cuff = surface(
        view,
        "beanie:cuff",
        &cuff_surf,
        segs11(view, TAU * rc, 28, 56),
        4,
        Inside::Point(place([0.0, -cuff_h / 2.0, 0.0])),
        true,
    );
    let rgb = rgb_of(base, RGB_FALLBACK);
    let cuff_rgb = rgb_of(&shade(base, 0.03, 0.0), RGB_FALLBACK);
    let bb = bounds_of([&cuff]);
    contact(
        ctx,
        body_clip,
        bb.cx,
        bb.y1 - 0.01 * w,
        bb.w * 0.5,
        0.06 * w,
        0.34,
    );
    let vis_dome = paint_layers(
        ctx,
        &[Layer::new(vec![&dome], felt(view, rgb, 0.96))],
        view.px,
    );
    const NU: usize = 40;
    const NV: usize = 11;
    let cell = ((rd * TAU) / (NU as f64)) * view.px;
    if cell >= 2.2 {
        let grid = surface(
            view,
            "beanie:stitch",
            &dome_surf,
            NU,
            NV,
            Inside::Point(in_dome),
            false,
        );
        let mut dark = PathBuilder::new();
        let mut light = PathBuilder::new();
        let gw = NU + 1;
        for c_idx in 0..(NU * NV) {
            if grid.f[c_idx * 3 + 2] < 0.12 {
                continue;
            }
            let k = (c_idx / NU) * gw + (c_idx % NU);
            let a = [grid.x[k], grid.y[k]];
            let b = [grid.x[k + 1], grid.y[k + 1]];
            let cq = [grid.x[k + gw + 1], grid.y[k + gw + 1]];
            let d = [grid.x[k + gw], grid.y[k + gw]];
            let at = |s: f64, t: f64| -> P2 {
                let top = [a[0] + (b[0] - a[0]) * s, a[1] + (b[1] - a[1]) * s];
                let bot = [d[0] + (cq[0] - d[0]) * s, d[1] + (cq[1] - d[1]) * s];
                [
                    top[0] + (bot[0] - top[0]) * t,
                    top[1] + (bot[1] - top[1]) * t,
                ]
            };
            for (pb, dx) in [(&mut dark, 0.0), (&mut light, -0.06)] {
                let pa = at(0.14 + dx, 0.12);
                let pb_pt = at(0.5 + dx, 0.88);
                let pc = at(0.86 + dx, 0.12);
                pb.move_to(pa[0] as f32, pa[1] as f32);
                pb.line_to(pb_pt[0] as f32, pb_pt[1] as f32);
                pb.line_to(pc[0] as f32, pc[1] as f32);
            }
        }
        let cw = (rd * TAU) / (NU as f64);
        let clip = poly_path(&hull(&vis_dome), true);
        ctx.save();
        ctx.clip_path(&clip, FillRule::Winding);
        ctx.set_line_cap(LineCap::Round);
        ctx.set_line_join(LineJoin::Round);
        ctx.set_line_width((cw * 0.2).max(0.35));
        ctx.set_stroke_css(&shade(base, -0.22, 0.03));
        ctx.set_global_alpha(0.5);
        stroke_built(ctx, dark);
        ctx.set_line_width((cw * 0.12).max(0.3));
        ctx.set_stroke_css(&shade(base, 0.2, -0.03));
        ctx.set_global_alpha(0.35);
        stroke_built(ctx, light);
        ctx.restore();
    }
    let vis_cuff = paint_layers(
        ctx,
        &[Layer::new(vec![&cuff], felt(view, cuff_rgb, 0.98))],
        view.px,
    );
    let rib_gap = (TAU * rc) / 30.0;
    if rib_gap * view.px >= 3.0 {
        let mut groove = PathBuilder::new();
        let mut ridge = PathBuilder::new();
        for k in 0..60 {
            let u = (k as f64) / 60.0;
            let th = u * TAU;
            let dir = view.dir(tilt([th.cos(), 0.0, 0.92 * th.sin()], roll, lean));
            if dir[2] < 0.08 {
                continue;
            }
            let a = view.to_screen(cuff_surf(u, 0.1));
            let b = view.to_screen(cuff_surf(u, 0.9));
            let pb = if k % 2 == 1 { &mut groove } else { &mut ridge };
            pb.move_to(a[0] as f32, a[1] as f32);
            pb.line_to(b[0] as f32, b[1] as f32);
        }
        ctx.save();
        ctx.set_line_cap(LineCap::Round);
        ctx.set_line_width(rib_gap * 0.34);
        ctx.set_stroke_css(&shade(base, -0.2, 0.03));
        ctx.set_global_alpha(0.45);
        stroke_built(ctx, groove);
        ctx.set_line_width(rib_gap * 0.22);
        ctx.set_stroke_css(&shade(base, 0.16, -0.02));
        ctx.set_global_alpha(0.4);
        stroke_built(ctx, ridge);
        ctx.restore();
    }
    let mut vis = vis_dome;
    vis.extend_from_slice(&vis_cuff);
    let outline = hull(&vis);
    fuzz(ctx, &outline, base, 23.0, 0.35, view.px);
    yarn_ball(
        ctx,
        view,
        place([0.0, -cuff_h - hd - 0.055 * w, 0.0]),
        0.085 * w,
        &shade(base, 0.06, 0.0),
        31.0,
    );
}

pub(crate) fn draw_party(
    ctx: &mut Canvas2D,
    view: &View,
    m: &Marks,
    base: &str,
    body_clip: Option<&Path>,
) {
    let w = m.right - m.left;
    let c = [50.0 + 0.06 * w, seat(m, 0.3) + 0.04 * w, 0.0];
    let radius = 0.18 * w;
    let h = (0.3 * w).max((0.46 * w).min(c[1] + 31.0 - 0.1 * w));
    let roll = 0.18;
    let lean = -0.12;
    let place = |p: V3| add(c, tilt(p, roll, lean));
    let cone_at = |u: f64, v: f64, grow: f64| {
        let th = u * TAU;
        let rr = radius * (1.0 - v) * (1.0 + 0.04 * (v * PI).sin()) * grow;
        place([rr * th.cos(), -h * v, rr * th.sin()])
    };
    let in_cone = |_u: f64, v: f64| place([0.0, -h * 0.97_f64.min(v), 0.0]);
    let n_around = segs11(view, TAU * radius, 20, 40);
    let n_up = segs(view, h, 10, 24, 6.0);
    let cone = surface(
        view,
        "party:cone",
        &|u, v| cone_at(u, v, 1.0),
        n_around,
        n_up,
        Inside::Fn(&in_cone),
        true,
    );
    let a = rgb_of(base, RGB_FALLBACK);
    let b: Rgb = [252.0, 246.0, 234.0];
    let bb = bounds_of([&cone]);
    contact(ctx, body_clip, bb.cx, bb.y1, radius * 1.15, 0.04 * w, 0.3);
    let trim = ring_mesh(
        view,
        "party:trim",
        &place,
        radius * 1.02,
        0.004 * w,
        0.016 * w,
        1.0,
        segs11(view, TAU * radius, 20, 40),
        5,
    );
    const K: usize = 6;
    let twist = 3.2;
    let mut stripes = Vec::with_capacity(K);
    for k in 0..K {
        let key = format!("party:stripe{k}");
        let kf = k as f64;
        stripes.push(surface(
            view,
            &key,
            &|s, t| {
                cone_at(
                    (kf + 0.5 * t - twist * s * 0.97) / (K as f64),
                    s * 0.97,
                    1.004,
                )
            },
            n_up + 3,
            1,
            Inside::Fn(&in_cone),
            true,
        ));
    }
    let vis = paint_layers(
        ctx,
        &[
            Layer::new(vec![&cone], paper(view, b)),
            Layer::new(vec![&trim], gold(view)),
        ],
        view.px,
    );
    let stripe_refs: Vec<&Proj> = stripes.iter().collect();
    paint_layers(ctx, &[Layer::new(stripe_refs, paper(view, a))], view.px);
    grain_over(ctx, &hull(&vis), 0.18, 0.1);
    yarn_ball(
        ctx,
        view,
        place([0.0, -h - 0.035 * w, 0.0]),
        0.06 * w,
        "#f4efe6",
        37.0,
    );
}

fn crown_facet(view: &View) -> impl Fn(V3) -> Rgb + use<> {
    let facet = metal(
        view,
        [255.0, 196.0, 80.0],
        [92.0, 54.0, 10.0],
        [255.0, 222.0, 150.0],
    );
    move |n: V3| {
        if n[2] < 0.0 {
            facet([-n[0], -n[1], -n[2]])
        } else {
            facet(n)
        }
    }
}

pub(crate) fn draw_crown(ctx: &mut Canvas2D, view: &View, m: &Marks, body_clip: Option<&Path>) {
    let w = m.right - m.left;
    let head_w = (0.45 * w).max(m.top_r - m.top_l);
    let radius = 0.44 * head_w + 0.07 * w;
    let band_h = 0.11 * w;
    let point_h = 0.15 * w;
    let ez = 0.9;
    let c = [50.0, seat(m, 0.42) + 0.05 * w, 0.0];
    let roll = 0.05;
    let lean = -0.22;
    let place = |p: V3| add(c, tilt(p, roll, lean));
    let dir_of = |v: V3| norm(tilt(v, roll, lean));
    let on = |th: f64, rr: f64, y: f64| place([rr * th.cos(), y, rr * ez * th.sin()]);
    let axis = |y: f64| place([0.0, y, 0.0]);
    let nu = segs11(view, TAU * radius, 32, 64);
    let band_inside = |_u: f64, v: f64| axis(-band_h * (1.0 - v));
    let band = surface(
        view,
        "crown:band",
        &|u, v| {
            on(
                u * TAU,
                radius * (1.0 - 0.018 * (v * PI).sin()),
                -band_h * (1.0 - v),
            )
        },
        nu,
        2,
        Inside::Fn(&band_inside),
        true,
    );
    let rim_t = 0.017 * w;
    let rim_top = ring_mesh(
        view,
        "crown:rimTop",
        &place,
        radius + 0.004 * w,
        -band_h,
        rim_t,
        ez,
        nu,
        5,
    );
    let rim_bot = ring_mesh(
        view,
        "crown:rimBot",
        &place,
        radius + 0.004 * w,
        0.0,
        rim_t * 1.15,
        ez,
        nu,
        5,
    );
    let cap_base = -band_h * 0.55;
    let cap_h = (0.12 * w).max(c[1] - m.top + 0.04 * w + cap_base);
    let p0 = PI / 2.0;
    let cap_nu = segs11(view, TAU * radius, 24, 48);
    let cap_nv = segs(view, 1.4 * cap_h, 8, 20, 6.0);
    let cap = surface(
        view,
        "crown:cap",
        &|u, v| {
            let th = u * TAU;
            let phi = v * (PI / 2.0);
            let puff =
                0.08 * (2.0 * phi).sin() * (0.55 + 0.45 * (5.0 * (th - p0 - PI / 5.0)).cos());
            on(
                th,
                radius * (0.9 * phi.sin() + puff),
                cap_base - cap_h * phi.cos(),
            )
        },
        cap_nu,
        cap_nv,
        Inside::Point(axis(cap_base - cap_h * 0.4)),
        true,
    );
    let mut points = Vec::with_capacity(5);
    let mut buds = Vec::with_capacity(5);
    let mut pearls = Vec::with_capacity(5);
    let mut bud_tops = Vec::with_capacity(5);
    for i in 0..5 {
        let a0 = p0 + (i as f64 / 5.0) * TAU;
        let point_key = format!("crown:point{i}");
        let point_h_i = point_h;
        let point_inside = |_u: f64, v: f64| on(a0, radius * 0.5, -band_h - point_h_i * v);
        points.push(surface(
            view,
            &point_key,
            &|u, v| {
                let s = u * 2.0 - 1.0;
                let th = a0 + s * ((TAU / 5.0) * 0.34) * (1.0 - v).powf(1.3);
                let rr = radius
                    + 0.004 * w
                    + 0.05 * w * (1.0 - s.abs()) * (1.0 - 0.5 * v)
                    + 0.03 * w * v * v;
                on(th, rr, -band_h - point_h_i * v)
            },
            2,
            segs(view, point_h_i, 3, 5, 11.0),
            Inside::Fn(&point_inside),
            false,
        ));
        pearls.push(on(
            a0,
            radius + 0.004 * w + 0.03 * w + 0.02 * w,
            -band_h - point_h - 0.018 * w,
        ));
        let b0 = a0 + PI / 5.0;
        let bud_key = format!("crown:bud{i}");
        let bud_h = point_h * 0.36;
        let bud_inside = |_u: f64, v: f64| on(b0, radius * 0.5, -band_h - bud_h * v);
        buds.push(surface(
            view,
            &bud_key,
            &|u, v| {
                let s = u * 2.0 - 1.0;
                let th = b0 + s * ((TAU / 5.0) * 0.12) * (1.0 - v).powf(1.3);
                let rr = radius
                    + 0.004 * w
                    + 0.02 * w * (1.0 - s.abs()) * (1.0 - 0.5 * v)
                    + 0.01 * w * v * v;
                on(th, rr, -band_h - bud_h * v)
            },
            2,
            segs(view, bud_h, 3, 5, 11.0),
            Inside::Fn(&bud_inside),
            false,
        ));
        bud_tops.push(on(
            b0,
            radius + 0.004 * w + 0.012 * w,
            -band_h - point_h * 0.36 - 0.012 * w,
        ));
    }
    let bb = bounds_of([&band]);
    contact(ctx, body_clip, bb.cx, bb.y1, bb.w * 0.5, 0.045 * w, 0.34);
    let mid = view.depth(axis(cap_base));
    let pearl_r = 0.024 * w;
    let bud_r = 0.019 * w;
    for p in &pearls {
        if view.depth(*p) < mid {
            sphere_sprite(ctx, view, *p, pearl_r, SpriteKind::Pearl);
        }
    }
    for p in &bud_tops {
        if view.depth(*p) < mid {
            sphere_sprite(ctx, view, *p, bud_r, SpriteKind::Gold);
        }
    }
    let point_refs: Vec<&Proj> = points.iter().collect();
    let bud_refs: Vec<&Proj> = buds.iter().collect();
    paint_layers(
        ctx,
        &[
            Layer::new(vec![&cap], velvet(view, [150.0, 22.0, 44.0])),
            Layer::new(vec![&band], gold(view)).with_bias(0.01 * w),
            Layer::new(vec![&rim_top], gold(view)).with_bias(0.012 * w),
            Layer::new(vec![&rim_bot], gold(view)).with_bias(0.012 * w),
            Layer::new(point_refs, crown_facet(view)).uncull().flat(),
            Layer::new(bud_refs, crown_facet(view)).uncull().flat(),
        ],
        view.px,
    );
    let stones: [Rgb; 5] = [
        [208.0, 20.0, 60.0],
        [30.0, 150.0, 92.0],
        [36.0, 90.0, 220.0],
        [36.0, 90.0, 220.0],
        [30.0, 150.0, 92.0],
    ];
    for i in 0..5 {
        for (th, stone) in [
            (p0 + (i as f64 / 5.0) * TAU, true),
            (p0 + (i as f64 / 5.0) * TAU + PI / 5.0, false),
        ] {
            let n = dir_of([th.cos(), 0.0, ez * th.sin()]);
            if view.dir(n)[2] < 0.12 {
                continue;
            }
            let centre = on(th, radius + 0.01 * w, -band_h * 0.5);
            if !stone {
                sphere_sprite(ctx, view, centre, 0.013 * w, SpriteKind::Gold);
                continue;
            }
            let size = (if i == 0 { 0.034 } else { 0.027 }) * w;
            sphere_sprite(ctx, view, centre, size * 1.3, SpriteKind::Gold);
            let t = dir_of([-th.sin(), 0.0, ez * th.cos()]);
            cut_gem(
                ctx,
                view,
                along(centre, n, 0.006 * w),
                n,
                t,
                size,
                stones[i],
            );
        }
    }
    for p in &pearls {
        if view.depth(*p) >= mid {
            sphere_sprite(ctx, view, *p, pearl_r, SpriteKind::Pearl);
        }
    }
    for p in &bud_tops {
        if view.depth(*p) >= mid {
            sphere_sprite(ctx, view, *p, bud_r, SpriteKind::Gold);
        }
    }
    let orb_r = 0.034 * w;
    let orb = axis(cap_base - cap_h - orb_r * 0.75);
    let up = dir_of([0.0, -1.0, 0.0]);
    let bar_up = dir_of([0.0, 0.0, 1.0]);
    let prof = section(0.009 * w, 0.009 * w, 0.5);
    let c0 = along(orb, up, orb_r * 0.8);
    let c1 = along(orb, up, orb_r * 0.8 + 0.075 * w);
    let cm = along(orb, up, orb_r * 0.8 + 0.05 * w);
    let side = dir_of([1.0, 0.0, 0.0]);
    let h0 = along(cm, side, -0.026 * w);
    let h1 = along(cm, side, 0.026 * w);
    let bar_v_path = |u: f64| {
        [
            c0[0] + (c1[0] - c0[0]) * u,
            c0[1] + (c1[1] - c0[1]) * u,
            c0[2] + (c1[2] - c0[2]) * u,
        ]
    };
    let bar_h_path = |u: f64| {
        [
            h0[0] + (h1[0] - h0[0]) * u,
            h0[1] + (h1[1] - h0[1]) * u,
            h0[2] + (h1[2] - h0[2]) * u,
        ]
    };
    let bar_v = tube(view, "crown:barV", &bar_v_path, bar_up, &prof, 2, 6, false);
    let bar_h = tube(view, "crown:barH", &bar_h_path, bar_up, &prof, 2, 6, false);
    sphere_sprite(ctx, view, orb, orb_r, SpriteKind::Gold);
    paint_layers(
        ctx,
        &[
            Layer::new(vec![&bar_v], gold(view)),
            Layer::new(vec![&bar_h], gold(view)),
        ],
        view.px,
    );
}

struct PhoneSide {
    cushion: Proj,
    shell: Proj,
    ring: Proj,
    plate: Proj,
    yoke: Proj,
}

struct PhoneBuilt {
    owner: u64,
    key: String,
    phones: Phones,
    sides: [PhoneSide; 2],
    band_shell: Proj,
    band_pad: Proj,
    sliders: [Proj; 2],
    shell_rgb: Rgb,
    pad_rgb: Rgb,
    plate_rgb: Rgb,
}

fn phone_key(view: &View, base: &str) -> String {
    let mut s = String::with_capacity(160 + base.len());
    for (i, v) in view.m.iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        s.push_str(&format!("{v}"));
    }
    s.push('|');
    s.push_str(&format!("{}", view.px));
    s.push('|');
    s.push_str(&format!("{},{},{}", view.l[0], view.l[1], view.l[2]));
    s.push('|');
    s.push_str(base);
    s
}

fn top_at(hull_pts: &[P2], x: f64) -> f64 {
    let mut y = f64::INFINITY;
    if hull_pts.is_empty() {
        return y;
    }
    for i in 0..hull_pts.len() {
        let p = hull_pts[i];
        let q = hull_pts[(i + 1) % hull_pts.len()];
        if p[0] == q[0] || x < p[0].min(q[0]) || x > p[0].max(q[0]) {
            continue;
        }
        y = y.min(p[1] + ((q[1] - p[1]) * (x - p[0])) / (q[0] - p[0]));
    }
    y
}

fn arch_at(u: f64, cx: f64, ax: f64, y_end: f64, lift: f64, zb: f64, e: f64) -> V3 {
    let t = PI * (1.0 - u);
    [
        cx + ax * spow(t.cos(), e),
        y_end - lift * t.sin().abs().powf(e),
        zb,
    ]
}

fn pad_t(u: f64, w: f64) -> f64 {
    0.005 * w + 0.022 * w * (PI * u).sin().powf(0.35)
}

fn build_phones(view: &View, m: &Marks, base: &str, owner: u64, key: String) -> PhoneBuilt {
    let w = m.right - m.left;
    let rc = 0.15 * w;
    let oval = 1.1;
    let pad = 0.05 * w;
    let tc = 0.068 * w;
    let yc = m.mid - 0.07 * w;
    let tilt_a: f64 = 0.5;
    let shell_rgb = rgb_of(base, RGB_FALLBACK);
    let pad_rgb = rgb_of(&shade(base, -0.12, 0.0), RGB_FALLBACK);
    let plate_rgb = rgb_of(&shade(base, 0.08, 0.0), RGB_FALLBACK);
    let nu = segs11(view, TAU * rc, 20, 40);
    let mut pivots = Vec::with_capacity(4);
    let mut tops = Vec::with_capacity(2);
    let mut seats = Vec::with_capacity(2);
    let mut sides = Vec::with_capacity(2);
    for s in [-1.0_f64, 1.0] {
        let a = [s * tilt_a.cos(), 0.0, tilt_a.sin()];
        let e1 = [0.0, 1.0, 0.0];
        let e2 = [-tilt_a.sin(), 0.0, s * tilt_a.cos()];
        let o = [
            (if s < 0.0 { m.left } else { m.right }) - s * 0.02 * w,
            yc,
            0.01 * w,
        ];
        let at = |t: f64, rho: f64, th: f64| -> V3 {
            let c = th.cos() * oval * rho;
            let sn = th.sin() * rho;
            [
                o[0] + a[0] * t + e1[0] * c + e2[0] * sn,
                o[1] + a[1] * t + e1[1] * c + e2[1] * sn,
                o[2] + a[2] * t + e1[2] * c + e2[2] * sn,
            ]
        };
        let axis = |t: f64| along(o, a, t);
        seats.push(PhoneSeat {
            c: o,
            ry: rc * oval,
        });
        let rho0 = rc * 0.9 - pad * 0.5;
        let creases = if nu >= 48 { 1.0 } else { 0.0 };
        let cush_key = format!("phones:cushion{s}");
        let cush_inside = |_u: f64, v: f64| axis(pad * 0.5 * (1.0 - (v * PI).cos()));
        let cushion = surface(
            view,
            &cush_key,
            &|u, v| {
                let th = u * TAU;
                let ang = v * PI;
                let crease = 1.0 + 0.006 * creases * (th * 24.0).sin() * ang.sin();
                at(
                    pad * 0.5 * (1.0 - ang.cos()),
                    (rho0 + pad * 0.62 * ang.sin()) * crease,
                    th,
                )
            },
            nu,
            5,
            Inside::Fn(&cush_inside),
            true,
        );
        let shell_inside_pt = axis(pad + tc * 0.5);
        let shell_key = format!("phones:shell{s}");
        let shell = surface(
            view,
            &shell_key,
            &|u, v| {
                let ang = v * (PI / 2.0);
                at(
                    pad + tc * ang.sin().powf(0.55),
                    rc * (0.7 + 0.3 * ang.cos().powf(0.55)),
                    u * TAU,
                )
            },
            nu,
            5,
            Inside::Point(shell_inside_pt),
            true,
        );
        let ring_r = 0.66 * rc;
        let ring_t = 0.017 * w;
        let ring_key = format!("phones:ring{s}");
        let ring_inside = |u: f64, _v: f64| at(pad + tc, ring_r, u * TAU);
        let ring = surface(
            view,
            &ring_key,
            &|u, v| {
                at(
                    pad + tc + ring_t * 0.45 * (v * TAU).sin(),
                    ring_r + ring_t * (v * TAU).cos(),
                    u * TAU,
                )
            },
            nu,
            5,
            Inside::Fn(&ring_inside),
            true,
        );
        let plate_r = ring_r - ring_t * 0.5;
        let plate_key = format!("phones:plate{s}");
        let plate = surface(
            view,
            &plate_key,
            &|u, v| at(pad + tc + 0.013 * w * (1.0 - v * v), plate_r * v, u * TAU),
            nu,
            3,
            Inside::Point(axis(pad + tc - 0.02 * w)),
            true,
        );
        let ty = pad + tc * 0.45;
        let ry = rc * 1.04 + 0.022 * w;
        let yoke_at = |u: f64| {
            let ang = u * PI;
            let c = -ang.sin() * oval * ry;
            let sn = ang.cos() * ry;
            [
                o[0] + a[0] * ty + e1[0] * c + e2[0] * sn,
                o[1] + a[1] * ty + e1[1] * c + e2[1] * sn,
                o[2] + a[2] * ty + e1[2] * c + e2[2] * sn,
            ]
        };
        let yoke_prof = section(0.013 * w, 0.008 * w, 0.6);
        let yoke_key = format!("phones:yoke{s}");
        let yoke = tube(
            view,
            &yoke_key,
            &yoke_at,
            a,
            &yoke_prof,
            segs11(view, PI * ry, 12, 24),
            6,
            false,
        );
        pivots.push(yoke_at(0.0));
        pivots.push(yoke_at(1.0));
        tops.push(yoke_at(0.5));
        sides.push(PhoneSide {
            cushion,
            shell,
            ring,
            plate,
            yoke,
        });
    }
    let pl = tops[0];
    let pr = tops[1];
    let zb = (pl[2] + pr[2]) / 2.0;
    let cx = (pl[0] + pr[0]) / 2.0;
    let ax = (pr[0] - pl[0]) / 2.0;
    let y_end = (pl[1] + pr[1]) / 2.0;
    let e = 2.0 / 2.2;
    let mut lift = y_end - m.top;
    for i in 1..64 {
        let t = PI * (1.0 - (i as f64) / 64.0);
        let s = t.sin().abs().powf(e);
        let yt = top_at(&m.hull, cx + ax * spow(t.cos(), e));
        if s > 0.05 && yt.is_finite() {
            lift = lift.max((y_end - yt + 2.0 * pad_t((i as f64) / 64.0, w) + 0.006 * w) / s);
        }
    }
    let us = 0.09;
    let shell_off = 0.016 * w;
    let shell_u0 = us;
    let shell_u1 = 1.0 - us;
    let shell_path = offset_path(
        Box::new(move |u| {
            arch_at(
                shell_u0 + (shell_u1 - shell_u0) * u,
                cx,
                ax,
                y_end,
                lift,
                zb,
                e,
            )
        }),
        [0.0, 0.0, 1.0],
        Box::new(move |_| shell_off),
    );
    let nb = segs11(view, ax * 2.0 + lift * 2.0, 24, 48);
    let shell_prof = move |u: f64, ph: f64| {
        let k = smooth01(u.min(1.0 - u) / 0.07);
        [
            (0.024 + 0.016 * k) * w * spow(ph.cos(), 0.4),
            (0.008 + 0.008 * k) * w * spow(ph.sin(), 0.4),
        ]
    };
    let band_shell = tube(
        view,
        "phones:bandShell",
        shell_path.as_ref(),
        [0.0, 0.0, 1.0],
        &shell_prof,
        nb,
        6,
        false,
    );
    let cush_u0 = us + 0.02;
    let cush_u1 = 1.0 - us - 0.02;
    let span_u = cush_u1 - cush_u0;
    let cushion_path = offset_path(
        Box::new(move |u| arch_at(cush_u0 + span_u * u, cx, ax, y_end, lift, zb, e)),
        [0.0, 0.0, 1.0],
        Box::new(move |u| -pad_t(cush_u0 + span_u * u, w)),
    );
    let pad_prof = move |u: f64, ph: f64| {
        let along_u = cush_u0 + span_u * u;
        [
            0.034 * w * spow(ph.cos(), 0.5),
            pad_t(along_u, w) * spow(ph.sin(), 0.7),
        ]
    };
    let band_pad = tube(
        view,
        "phones:bandPad",
        cushion_path.as_ref(),
        [0.0, 0.0, 1.0],
        &pad_prof,
        nb,
        6,
        false,
    );
    let slider_prof = section(0.022 * w, 0.006 * w, 0.4);
    let slider_u1 = us + 0.03;
    let slider0 = tube(
        view,
        "phones:slider0",
        &|u: f64| arch_at(slider_u1 * u, cx, ax, y_end, lift, zb, e),
        [0.0, 0.0, 1.0],
        &slider_prof,
        4,
        6,
        false,
    );
    let slider_u0 = 1.0 - us - 0.03;
    let slider1 = tube(
        view,
        "phones:slider1",
        &|u: f64| {
            arch_at(
                slider_u0 + (1.0 - slider_u0) * u,
                cx,
                ax,
                y_end,
                lift,
                zb,
                e,
            )
        },
        [0.0, 0.0, 1.0],
        &slider_prof,
        4,
        6,
        false,
    );
    let mut stitch = Vec::with_capacity(49);
    for i in 0..=48 {
        stitch.push(along(
            cushion_path(0.04 + (0.92 * (i as f64)) / 48.0),
            [0.0, 0.0, 1.0],
            0.034 * w + 0.001 * w,
        ));
    }
    let side_r = sides.pop().unwrap();
    let side_l = sides.pop().unwrap();
    PhoneBuilt {
        owner,
        key,
        phones: Phones {
            pivots,
            pivot_r: 0.017 * w,
            stitch,
            seats,
        },
        sides: [side_l, side_r],
        band_shell,
        band_pad,
        sliders: [slider0, slider1],
        shell_rgb,
        pad_rgb,
        plate_rgb,
    }
}

fn load_phones(view: &View, m: &Marks, base: &str) -> Phones {
    let key = phone_key(view, base);
    let owner = owner_id();
    let hit = LAST_PHONES.with(|c| {
        c.borrow()
            .as_ref()
            .is_some_and(|p| p.owner == owner && p.key == key)
    });
    if !hit {
        let built = build_phones(view, m, base, owner, key);
        let phones = built.phones.clone();
        LAST_PHONES.with(|c| *c.borrow_mut() = Some(built));
        phones
    } else {
        LAST_PHONES.with(|c| c.borrow().as_ref().unwrap().phones.clone())
    }
}

pub(crate) fn phones_of(view: &View, m: &Marks, base: &str) -> Phones {
    load_phones(view, m, base)
}

fn paint_phone_pass(
    ctx: &mut Canvas2D,
    view: &View,
    m: &Marks,
    base: &str,
    pass: PhonePass,
    body_clip: Option<&Path>,
    ph: &PhoneBuilt,
) {
    let w = m.right - m.left;
    let front = matches!(pass, PhonePass::Front);
    if front {
        for s in &ph.phones.seats {
            if view.depth(s.c) < 0.0 {
                continue;
            }
            let c = view.to_screen(s.c);
            contact(
                ctx,
                body_clip,
                c[0],
                c[1] + 0.01 * w,
                0.06 * w,
                s.ry * 1.05,
                0.26,
            );
        }
    }
    let near = move |d: f64| if front { d >= 0.0 } else { d < 0.0 };
    let mut layers = Vec::with_capacity(12);
    for side in &ph.sides {
        layers.push(
            Layer::new(vec![&side.cushion], plastic(view, ph.pad_rgb, 0.35)).with_keep(&near),
        );
        layers
            .push(Layer::new(vec![&side.shell], plastic(view, ph.shell_rgb, 0.8)).with_keep(&near));
        layers.push(
            Layer::new(vec![&side.ring], silver(view))
                .with_bias(0.02 * w)
                .with_keep(&near),
        );
        layers.push(
            Layer::new(vec![&side.plate], plastic(view, ph.plate_rgb, 1.3))
                .with_bias(0.01 * w)
                .with_keep(&near),
        );
        layers.push(Layer::new(vec![&side.yoke], silver(view)).with_keep(&near));
    }
    layers
        .push(Layer::new(vec![&ph.band_shell], plastic(view, ph.shell_rgb, 0.9)).with_keep(&near));
    layers.push(Layer::new(vec![&ph.band_pad], plastic(view, ph.pad_rgb, 0.3)).with_keep(&near));
    layers.push(Layer::new(vec![&ph.sliders[0], &ph.sliders[1]], silver(view)).with_keep(&near));
    paint_layers(ctx, &layers, view.px);
    for p in &ph.phones.pivots {
        if near(view.depth(*p)) {
            sphere_sprite(ctx, view, *p, ph.phones.pivot_r, SpriteKind::Silver);
        }
    }
    if front && w * view.px > 120.0 && view.dir([0.0, 0.0, 1.0])[2] > 0.3 {
        let pts: Vec<P2> = ph
            .phones
            .stitch
            .iter()
            .map(|p| view.to_screen(*p))
            .collect();
        ctx.save();
        ctx.set_line_dash(&[0.022 * w, 0.014 * w]);
        ctx.set_line_cap(LineCap::Round);
        ctx.set_line_width(0.0045 * w);
        ctx.set_stroke_css(&shade(base, 0.12, 0.0));
        ctx.set_global_alpha(0.55);
        ctx.stroke_path(&poly_path(&pts, false));
        ctx.restore();
    }
}

pub(crate) fn draw_phones(
    ctx: &mut Canvas2D,
    view: &View,
    m: &Marks,
    base: &str,
    pass: PhonePass,
    body_clip: Option<&Path>,
) {
    let _ = phones_of(view, m, base);
    LAST_PHONES.with(|c| {
        let slot = c.borrow();
        let ph = slot.as_ref().expect("phones cached");
        paint_phone_pass(ctx, view, m, base, pass, body_clip, ph);
    });
}

fn js_fixed2(n: f64) -> String {
    if n.is_nan() {
        return "NaN".to_string();
    }
    if n.is_infinite() {
        return if n.is_sign_negative() {
            "-Infinity".to_string()
        } else {
            "Infinity".to_string()
        };
    }
    let s = format!("{n:.2}");
    if s == "-0.00" { "0.00".to_string() } else { s }
}

fn bow_layers<'a>(
    view: &'a View,
    parts: &'a [(Proj, bool)],
    rgb: Rgb,
    soft: bool,
    range: std::ops::Range<usize>,
) -> Vec<Layer<'a>> {
    let mut layers = Vec::with_capacity(range.len());
    for (proj, knot) in &parts[range] {
        if *knot && soft {
            layers.push(Layer::new(vec![proj], felt(view, rgb, 0.9)));
        } else if *knot {
            layers.push(Layer::new(vec![proj], satin(view, scale(rgb, 0.9))));
        } else if soft {
            layers.push(Layer::new(vec![proj], felt(view, rgb, 1.0)));
        } else {
            layers.push(Layer::new(vec![proj], satin(view, rgb)));
        }
    }
    layers
}

/// `fabric_bow` defaults to false (`drawBowTie`, 1029–1119).
pub(crate) fn draw_bow_tie(
    ctx: &mut Canvas2D,
    view: &View,
    r: &WearRig,
    m: &Marks,
    base: &str,
    body_z: &dyn Fn(f64, f64) -> f64,
    body_clip: Option<&Path>,
    face: FaceAnchor,
    fabric_bow: bool,
) {
    if r.facing < 0.05 {
        return;
    }
    let w = m.right - m.left;
    let clear = face.y + 22.0 * face.scale;
    let cy_b = clear.max(m.bottom - 0.15 * w);
    let room = (m.bottom - 3.0 - cy_b).max(0.0);
    let k = w.min(100.0) * (room / 15.0).clamp(0.5, 1.0);
    let z_front = body_z(50.0, cy_b);
    let q = |x: f64, y: f64, z: f64| -> V3 { [50.0 + x * k, cy_b + y * k, z_front + z * k] };
    let rgb = rgb_of(base, RGB_FALLBACK);
    let corners = [
        q(-0.36, -0.18, 0.0),
        q(0.36, 0.18, 0.0),
        q(-0.36, 0.18, 0.0),
        q(0.36, -0.18, 0.0),
    ]
    .map(|p| view.to_screen(p));
    let bb = bounds(&corners);
    let sh = view.to_screen(q(0.01, 0.08, 0.0));
    contact(ctx, body_clip, sh[0], sh[1], bb.w * 0.52, bb.h * 0.5, 0.3);
    let nu = segs11(view, 0.32 * k, 8, 16);
    let nv = segs(view, 0.3 * k, 8, 16, 6.0);
    let at = format!(
        "{},{},{}",
        js_fixed2(z_front),
        js_fixed2(cy_b),
        js_fixed2(k)
    );
    let soft = fabric_bow;
    let mut parts: Vec<(Proj, bool)> = Vec::with_capacity(5);
    for dir in [-1.0_f64, 1.0] {
        let wing = |u: f64, v: f64| {
            let x = 0.058 + 0.3 * u - 0.035 * u.powi(4) * (PI * v).sin();
            let h = 0.05 + 0.12 * u.powf(0.8);
            let y = -h * (PI * v).cos() + 0.012 * u * u;
            let bulge = 0.018 + 0.058 * (PI * (0.12 + 0.88 * u)).sin();
            let fold = 0.016 * (3.0 * PI * v).sin() * (1.0 - u).powf(1.3);
            q(dir * x, y, (bulge + fold) * (PI * v).sin() + 0.004)
        };
        let inside = |u: f64, _v: f64| q(dir * (0.058 + 0.3 * u), 0.0, -0.03);
        parts.push((
            surface(
                view,
                &format!("bow:wing{dir}@{at}"),
                &wing,
                nu,
                nv,
                Inside::Fn(&inside),
                true,
            ),
            false,
        ));
        let end_surf = |s: f64, t: f64| {
            let p = wing(1.0, t);
            [p[0], p[1], p[2] - (p[2] - z_front) * s]
        };
        parts.push((
            surface(
                view,
                &format!("bow:end{dir}@{at}"),
                &end_surf,
                1,
                nv,
                Inside::Point(q(dir * 0.2, 0.0, -0.03)),
                true,
            ),
            false,
        ));
    }
    let knot = surface(
        view,
        &format!("bow:knot@{at}"),
        &|u, v| {
            let x = 0.068 * (2.0 * u - 1.0) * (1.0 - 0.12 * (PI * v).sin());
            let y = 0.075 * (2.0 * v - 1.0);
            let zx = (1.0 - (2.0 * u - 1.0).abs().powi(3)).max(0.0).powf(0.5);
            let zy = (1.0 - (2.0 * v - 1.0).abs().powi(5)).max(0.0).powf(0.4);
            q(x, y, 0.012 + 0.075 * zx * zy)
        },
        6,
        6,
        Inside::Point(q(0.0, 0.0, -0.02)),
        true,
    );
    parts.push((knot, true));
    if !soft {
        let layers = bow_layers(view, &parts, rgb, soft, 0..parts.len());
        paint_layers(ctx, &layers, view.px);
        return;
    }
    let mut i = 0;
    while i < parts.len() {
        let end = (i + 2).min(parts.len());
        let layers = bow_layers(view, &parts, rgb, soft, i..end);
        let outline = hull(&paint_layers(ctx, &layers, view.px));
        grain_over(ctx, &outline, 0.5, 0.1);
        fuzz(ctx, &outline, base, (41 + i) as f64, 0.3, view.px);
        i += 2;
    }
}

fn lens_ring(
    view: &View,
    e: EyeSpot,
    f: &FrameSpec,
    z_at: &dyn Fn(f64) -> f64,
    grow: f64,
    n: usize,
) -> Vec<P2> {
    let e2 = 2.0 / f.n;
    let mut pts = Vec::with_capacity(n);
    for i in 0..n {
        let t = (i as f64) / (n as f64) * TAU;
        let x = e.ox + (f.rx + grow) * spow(t.cos(), e2);
        let y = e.oy + (f.ry + grow) * spow(t.sin(), e2);
        pts.push(view.to_screen([x, y, z_at(x)]));
    }
    pts
}

/// `drawGlasses` (1150–1272).
pub fn draw_glasses(
    ctx: &mut Canvas2D,
    kind: BotAvatarGlasses,
    eyes: &[EyeSpot; 2],
    light: ScreenLight,
    pass: GlassesPass,
    turn: HeadTurn,
) {
    let Some(f) = frame_spec(kind) else {
        return;
    };
    set_owner(GLASSES_OWNER);
    let (sy, cy) = turn.yaw.sin_cos();
    let (sp, cp) = turn.pitch.sin_cos();
    let light_v = light_of(light.lx, light.ly);
    let view = make_view(
        [cy, 0.0, sy, sp * sy, cp, -sp * cy, -cp * sy, sp, cp * cy],
        [0.0, 0.0, 0.0],
        light_v.l,
        light_v.h,
        px_of(ctx),
    );
    let z_plane = eyes[0].fz.max(eyes[1].fz) + 3.2;
    let z_at = |x: f64| z_plane - 0.0045 * x * x;
    let vis = [(eyes[0].z - 0.18) * 3.2, (eyes[1].z - 0.18) * 3.2].map(|v| v.clamp(0.0, 1.0));
    if pass == GlassesPass::Shadow {
        for i in 0..2 {
            if vis[i] <= 0.0 {
                continue;
            }
            ctx.save();
            ctx.set_global_alpha(0.22 * vis[i]);
            ctx.translate(-light.lx * 1.6, -light.ly * 1.6 + 0.8);
            ctx.set_line_width(f.tr * 2.0 + 0.6);
            ctx.set_line_join(LineJoin::Round);
            ctx.set_stroke_css("#000");
            ctx.stroke_path(&poly_path(
                &lens_ring(&view, eyes[i], &f, &z_at, 0.0, 48),
                true,
            ));
            ctx.restore();
        }
        return;
    }
    let frame: Rgb = [22.0, 22.0, 26.0];
    for i in 0..2 {
        if vis[i] <= 0.0 {
            continue;
        }
        let lens = poly_path(&lens_ring(&view, eyes[i], &f, &z_at, -f.tr * 0.5, 48), true);
        let b = bounds(&lens_ring(&view, eyes[i], &f, &z_at, 0.0, 16));
        ctx.save();
        ctx.set_global_alpha(vis[i]);
        if kind == BotAvatarGlasses::Shades {
            ctx.set_fill_style(PaintStyle::linear_gradient(
                b.cx,
                b.y0,
                b.cx,
                b.y1,
                &[
                    (0.0, "rgba(58,60,78,0.97)"),
                    (0.55, "rgba(18,18,26,0.98)"),
                    (1.0, "rgba(30,26,44,0.98)"),
                ],
            ));
        } else {
            ctx.set_fill_style(PaintStyle::linear_gradient(
                b.cx,
                b.y0,
                b.cx,
                b.y1,
                &[
                    (0.0, "rgba(236,244,255,0.26)"),
                    (0.45, "rgba(214,230,255,0.08)"),
                    (1.0, "rgba(214,230,255,0.14)"),
                ],
            ));
        }
        ctx.fill_path(&lens);
        ctx.clip_path(&lens, FillRule::Winding);
        let bw = b.w;
        let bh = b.h;
        let streak = |ctx: &mut Canvas2D, x0: f64, width: f64, a: f64| {
            ctx.set_global_alpha(vis[i] * a);
            ctx.set_fill_css("#fff");
            let mut pb = PathBuilder::new();
            pb.move_to((b.x0 + bw * x0) as f32, (b.y1 + bh * 0.1) as f32);
            pb.line_to((b.x0 + bw * (x0 + 0.55)) as f32, (b.y0 - bh * 0.1) as f32);
            pb.line_to(
                (b.x0 + bw * (x0 + 0.55 + width)) as f32,
                (b.y0 - bh * 0.1) as f32,
            );
            pb.line_to((b.x0 + bw * (x0 + width)) as f32, (b.y1 + bh * 0.1) as f32);
            pb.close();
            if let Some(path) = pb.finish() {
                ctx.fill_path(&path);
            }
        };
        let kk = if kind == BotAvatarGlasses::Shades {
            1.4
        } else {
            1.0
        };
        streak(ctx, -0.28, 0.26, 0.16 * kk);
        streak(ctx, 0.06, 0.07, 0.3 * kk);
        ctx.restore();
    }
    let nr = segs(&view, TAU * f.rx * 1.1, 28, 56, 8.0);
    let e2 = 2.0 / f.n;
    let rim_at = |e: EyeSpot| {
        move |u: f64| {
            let t = u * TAU;
            let x = e.ox + f.rx * spow(t.cos(), e2);
            let y = e.oy + f.ry * spow(t.sin(), e2);
            [x, y, z_at(x)]
        }
    };
    let l = eyes[0];
    let r = eyes[1];
    let prof_rim = section(f.td, f.tr, 0.55);
    let rim_l_path = rim_at(l);
    let rim_r_path = rim_at(r);
    let rim_l = tube(
        &view,
        &format!("rim:{:?}:{},{},{}", kind, l.ox, l.oy, js_fixed2(l.fz)),
        &rim_l_path,
        [0.0, 0.0, 1.0],
        &prof_rim,
        nr,
        6,
        true,
    );
    let rim_r = tube(
        &view,
        &format!("rim:{:?}:{},{},{}", kind, r.ox, r.oy, js_fixed2(r.fz)),
        &rim_r_path,
        [0.0, 0.0, 1.0],
        &prof_rim,
        nr,
        6,
        true,
    );
    let round = kind == BotAvatarGlasses::Round;
    let a_pt = [
        l.ox + f.rx * if round { 0.9 } else { 0.98 },
        l.oy - f.ry * if round { 0.32 } else { 0.42 },
        0.0,
    ];
    let b_pt = [
        r.ox - f.rx * if round { 0.9 } else { 0.98 },
        r.oy - f.ry * if round { 0.32 } else { 0.42 },
        0.0,
    ];
    let c_pt = [
        0.0,
        (l.oy + r.oy) / 2.0 - f.ry * if round { 0.8 } else { 0.6 },
        0.0,
    ];
    let bridge_at = |u: f64| {
        let m1 = 1.0 - u;
        let x = m1 * m1 * a_pt[0] + 2.0 * m1 * u * c_pt[0] + u * u * b_pt[0];
        let y = m1 * m1 * a_pt[1] + 2.0 * m1 * u * c_pt[1] + u * u * b_pt[1];
        [x, y, z_at(x) + 0.3]
    };
    let prof_bridge = section(f.td * 0.8, f.tr * 0.75, 0.7);
    let bridge = tube(
        &view,
        &format!(
            "bridge:{:?}:{},{},{},{}",
            kind,
            l.ox,
            l.oy,
            r.ox,
            js_fixed2(z_plane)
        ),
        &bridge_at,
        [0.0, 0.0, 1.0],
        &prof_bridge,
        8,
        6,
        false,
    );
    let mut order = [
        (0_usize, vis[0], view.depth([l.ox, l.oy, z_plane])),
        (1, vis[1], view.depth([r.ox, r.oy, z_plane])),
        (2, vis[0].min(vis[1]), view.depth([0.0, l.oy, z_plane])),
    ];
    order.sort_by(|p, q| p.2.partial_cmp(&q.2).unwrap_or(std::cmp::Ordering::Equal));
    let rims = [&rim_l, &rim_r, &bridge];
    for (idx, a, _) in order {
        if a <= 0.0 {
            continue;
        }
        ctx.save();
        ctx.set_global_alpha(a);
        paint_layers(
            ctx,
            &[Layer::new(vec![rims[idx]], plastic(&view, frame, 1.1))],
            view.px,
        );
        ctx.restore();
    }
    for (e, s, a) in [(l, -1.0, vis[0]), (r, 1.0, vis[1])] {
        if a <= 0.0 {
            continue;
        }
        ctx.save();
        ctx.set_global_alpha(a);
        let x = e.ox + s * f.rx * 0.93;
        sphere_sprite(
            ctx,
            &view,
            [x, e.oy - f.ry * 0.5, z_at(x) + f.td],
            0.75,
            SpriteKind::Silver,
        );
        ctx.restore();
    }
}

/// Behind the body: the far half of the headphones (`drawWearBehind`, 1277–1283).
pub fn draw_wear_behind(ctx: &mut Canvas2D, path: &Path, r: &WearRig, wear: &Wear) {
    if !wear.headphones {
        return;
    }
    let Some(m) = marks_for(path) else {
        return;
    };
    set_owner(path_owner(path));
    draw_phones(
        ctx,
        &view_of(r, px_of(ctx)),
        &m,
        &wear.color,
        PhonePass::Behind,
        None,
    );
}

/// Hat, near headphones, and bow tie (`drawWearFront`, 1288–1317).
pub fn draw_wear_front(
    ctx: &mut Canvas2D,
    path: &Path,
    r: &WearRig,
    wear: &Wear,
    body_clip: Option<&Path>,
    body_z: &dyn Fn(f64, f64) -> f64,
    face: FaceAnchor,
) {
    let Some(m) = marks_for(path) else {
        return;
    };
    set_owner(path_owner(path));
    let view = view_of(r, px_of(ctx));
    if wear.bow_tie {
        draw_bow_tie(
            ctx,
            &view,
            r,
            &m,
            &wear.color,
            body_z,
            body_clip,
            face,
            wear.fabric,
        );
    }
    if wear.headphones {
        draw_phones(ctx, &view, &m, &wear.color, PhonePass::Front, body_clip);
    }
    match wear.hat {
        BotAvatarHat::None => {}
        BotAvatarHat::Beret => draw_beret(ctx, &view, &m, &wear.color, body_clip),
        BotAvatarHat::Beanie => draw_beanie(ctx, &view, &m, &wear.color, body_clip),
        BotAvatarHat::Party => draw_party(ctx, &view, &m, &wear.color, body_clip),
        BotAvatarHat::Crown => draw_crown(ctx, &view, &m, body_clip),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        EL, FRAME_ROUND, FRAME_SHADES, FRAME_SQUARE, FaceAnchor, GlassesPass, HeadTurn, Marks,
        PhonePass, RGB_FALLBACK, ScreenLight, WearRig, add, along, bounds, depth_of, draw_beanie,
        draw_beret, draw_bow_tie, draw_crown, draw_glasses, draw_party, draw_phones, frame_spec,
        grain_tile, keep, light_of, project, rnd, seat, set_owner, smooth01, spow, tilt, view_of,
    };
    use crate::canvas::Canvas2D;
    use crate::mesh::Mesh;
    use crate::shapes::body_path_for;
    use crate::types::{BotAvatarGlasses, BotAvatarType};

    fn close(a: f64, b: f64) {
        assert!((a - b).abs() < 1e-9, "{a} != {b}");
    }

    fn rig() -> WearRig {
        WearRig {
            cy: 0.6,
            sy: 0.8,
            cp: 0.9,
            sp: -0.1,
            facing: 1.0,
            half_depth: 8.0,
            lx: 0.2,
            ly: -0.4,
        }
    }

    fn head() -> Marks {
        let mut widths = vec![0.0_f32; 128];
        for w in &mut widths {
            *w = 80.0;
        }
        Marks {
            top: 12.0,
            top_l: 30.0,
            top_r: 70.0,
            mid: 50.0,
            left: 10.0,
            right: 90.0,
            bottom: 92.0,
            widths,
            step: 106.0 / 128.0,
            hull: vec![[10.0, 12.0], [90.0, 12.0], [90.0, 92.0], [10.0, 92.0]],
        }
    }

    fn eyes() -> [super::EyeSpot; 2] {
        [
            super::EyeSpot {
                x: -12.0,
                y: 0.0,
                sx: 8.0,
                sy: 8.0,
                z: 0.9,
                ox: -14.0,
                oy: -2.0,
                fz: 8.0,
            },
            super::EyeSpot {
                x: 12.0,
                y: 0.0,
                sx: 8.0,
                sy: 8.0,
                z: 0.85,
                ox: 14.0,
                oy: -2.0,
                fz: 8.0,
            },
        ]
    }

    #[test]
    fn frames_match_wear_ts() {
        assert_eq!(frame_spec(BotAvatarGlasses::None), None);
        assert_eq!(frame_spec(BotAvatarGlasses::Round), Some(FRAME_ROUND));
        assert_eq!(frame_spec(BotAvatarGlasses::Square), Some(FRAME_SQUARE));
        assert_eq!(frame_spec(BotAvatarGlasses::Shades), Some(FRAME_SHADES));
        assert_eq!(FRAME_ROUND.rx, 10.8);
        assert_eq!(FRAME_SQUARE.n, 4.2);
        assert_eq!(FRAME_SHADES.td, 1.5);
    }

    #[test]
    fn numerics_match_wear_ts() {
        let samples = [
            (0, 0, 0.0),
            (1, 0, 0.5421679092105478),
            (2, 5, 0.9770136345177889),
            (7, 3, 0.36823442997410893),
            (100, 11, 0.992571905022487),
            (-3, 1, 0.3823187865782529),
            (360, 4, 0.2729304125532508),
        ];
        for (i, k, expect) in samples {
            close(rnd(i, k), expect);
        }
        let r = rig();
        let p = project(&r, [60.0, 40.0, 5.0]);
        close(p[0], 10.0);
        close(p[1], -9.5);
        close(depth_of(&r, [60.0, 40.0, 5.0]), -3.5);
        let t = tilt([1.0, 2.0, 3.0], 0.3, -0.2);
        close(t[0], 0.3642960758029269);
        close(t[1], 2.7582241971790973);
        close(t[2], 2.5018968098725236);
        close(spow(-0.4, 0.4), -0.6931448431551464);
        close(spow(0.0, 0.5), 0.0);
        close(smooth01(0.3), 0.216);
        close(smooth01(-1.0), 0.0);
        close(smooth01(2.0), 1.0);
        assert_eq!(add([1.0, 2.0, 3.0], [4.0, 5.0, 6.0]), [5.0, 7.0, 9.0]);
        assert_eq!(
            along([1.0, 1.0, 1.0], [2.0, 0.0, -1.0], 3.0),
            [7.0, 1.0, -2.0]
        );
        let light = light_of(0.2, -0.4);
        close(light.l[0], 0.16704642374680725);
        close(light.l[1], -0.3340928474936145);
        close(light.l[2], 0.927619243853323);
        close(light.h[0], 0.08507688061199535);
        close(light.h[1], -0.1701537612239907);
        close(light.h[2], 0.9817380617693608);
        let _ = EL;
        let b = bounds(&[[1.0, 4.0], [3.0, 2.0], [-1.0, 8.0]]);
        close(b.x0, -1.0);
        close(b.y0, 2.0);
        close(b.x1, 3.0);
        close(b.y1, 8.0);
        close(b.cx, 1.0);
        close(b.w, 4.0);
        close(b.h, 6.0);
        let mut m = head();
        m.widths = vec![0.0, 10.0, 30.0, 80.0];
        m.step = 1.0;
        m.left = 0.0;
        m.right = 100.0;
        m.top = 5.0;
        close(seat(&m, 0.5), 0.0);
        m.widths = vec![1.0, 2.0];
        close(seat(&m, 0.9), 5.0);
    }

    #[test]
    fn mesh_and_grain_cache_hit() {
        set_owner(42);
        let mut builds = 0;
        let mesh = || Mesh {
            nu: 1,
            nv: 1,
            closed: false,
            pos: vec![0.0; 12],
            face_normals: vec![0.0; 3],
            vert_normals: None,
        };
        let a = keep("unit", || {
            builds += 1;
            mesh()
        });
        let b = keep("unit", || {
            builds += 1;
            mesh()
        });
        assert_eq!(builds, 1);
        assert_eq!(a.pos, b.pos);
        set_owner(43);
        let _ = keep("unit", || {
            builds += 1;
            mesh()
        });
        assert_eq!(builds, 2);
        set_owner(42);
        let _ = keep("unit", || {
            builds += 1;
            mesh()
        });
        assert_eq!(builds, 2);
        let g1 = grain_tile().expect("grain");
        let g2 = grain_tile().expect("grain");
        assert_eq!(g1.data(), g2.data());
        assert_eq!(g1.width(), 48);
        let px = |i: usize| g1.data()[i * 4];
        assert_eq!(
            [px(0), px(1), px(48), px(100), px(48 * 48 - 1)],
            [239, 255, 250, 240, 230]
        );
    }

    #[test]
    fn rgb_fallback_and_parse() {
        assert_eq!(super::rgb_of("#ff0000", RGB_FALLBACK), [255.0, 0.0, 0.0]);
        assert_eq!(super::rgb_of("not-a-color", RGB_FALLBACK), RGB_FALLBACK);
    }

    #[test]
    fn pieces_draw_for_every_avatar_type() {
        let m = head();
        let view = view_of(&rig(), 1.0);
        let mut ctx = Canvas2D::new(32, 32);
        set_owner(7);
        draw_beret(&mut ctx, &view, &m, "#c45b4a", None);
        draw_beanie(&mut ctx, &view, &m, "#3355aa", None);
        draw_party(&mut ctx, &view, &m, "#e23d4a", None);
        draw_crown(&mut ctx, &view, &m, None);
        draw_phones(&mut ctx, &view, &m, "#222228", PhonePass::Behind, None);
        draw_phones(&mut ctx, &view, &m, "#222228", PhonePass::Front, None);
        let z = |_: f64, _: f64| 4.0;
        draw_bow_tie(
            &mut ctx,
            &view,
            &rig(),
            &m,
            "#882233",
            &z,
            None,
            FaceAnchor {
                y: 48.0,
                scale: 1.0,
            },
            false,
        );
        draw_bow_tie(
            &mut ctx,
            &view,
            &rig(),
            &m,
            "#882233",
            &z,
            None,
            FaceAnchor {
                y: 48.0,
                scale: 1.0,
            },
            true,
        );
        let light = ScreenLight { lx: 0.2, ly: -0.4 };
        for kind in [
            BotAvatarGlasses::None,
            BotAvatarGlasses::Round,
            BotAvatarGlasses::Square,
            BotAvatarGlasses::Shades,
        ] {
            draw_glasses(
                &mut ctx,
                kind,
                &eyes(),
                light,
                GlassesPass::Shadow,
                HeadTurn::default(),
            );
            draw_glasses(
                &mut ctx,
                kind,
                &eyes(),
                light,
                GlassesPass::Frame,
                HeadTurn {
                    yaw: 0.4,
                    pitch: -0.2,
                },
            );
        }
        for kind in BotAvatarType::ALL {
            let _path = body_path_for(kind);
            let _ = seat(&m, 0.5);
            let _ = project(&rig(), [40.0, 30.0, 1.0]);
            let _ = kind;
        }
        // second pass hits the phone projection cache
        draw_phones(&mut ctx, &view, &m, "#222228", PhonePass::Behind, None);
    }
}
