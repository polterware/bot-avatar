use std::f64::consts::{PI, TAU};

use tiny_skia::{LineJoin, PathBuilder};

use crate::canvas::{Canvas2D, PaintStyle, RgbaColor, circle_path};

pub type V3 = [f64; 3];
pub type P2 = [f64; 2];
pub type Rgb = [f64; 3];

pub const WHITE: Rgb = [255.0, 255.0, 255.0];

#[inline]
pub fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

#[inline]
pub fn cross(a: V3, b: V3) -> V3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

#[inline]
pub fn dot(a: V3, b: V3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

#[inline]
pub fn norm(v: V3) -> V3 {
    let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    let d = if l > 0.0 { l } else { 1.0 };
    [v[0] / d, v[1] / d, v[2] / d]
}

#[inline]
pub fn clamp01(x: f64) -> f64 {
    x.clamp(0.0, 1.0)
}

#[inline]
pub fn mix(a: Rgb, b: Rgb, t: f64) -> Rgb {
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
    ]
}

#[inline]
pub fn scale(a: Rgb, k: f64) -> Rgb {
    [a[0] * k, a[1] * k, a[2] * k]
}

#[inline]
fn clamp_byte(v: f64) -> u8 {
    if v < 0.0 {
        0
    } else if v > 255.0 {
        255
    } else {
        v.round() as u8
    }
}

#[inline]
pub fn rgb_to_rgba_color(c: Rgb) -> RgbaColor {
    RgbaColor::from_rgba8(clamp_byte(c[0]), clamp_byte(c[1]), clamp_byte(c[2]), 1.0)
}

pub fn css(c: Rgb) -> String {
    format!(
        "rgb({},{},{})",
        clamp_byte(c[0]),
        clamp_byte(c[1]),
        clamp_byte(c[2])
    )
}

/// Linear map from an accessory's local 3D space (about origin `o`) to view space.
#[derive(Debug, Clone, Copy)]
pub struct View {
    pub m: [f64; 9],
    pub o: V3,
    pub l: V3,
    pub h: V3,
    pub px: f64,
}

pub fn make_view(m: [f64; 9], o: V3, l: V3, h: V3, px: f64) -> View {
    View { m, o, l, h, px }
}

impl View {
    #[inline]
    pub fn to_screen(&self, p: V3) -> P2 {
        let x = p[0] - self.o[0];
        let y = p[1] - self.o[1];
        let z = p[2] - self.o[2];
        [
            self.m[0] * x + self.m[1] * y + self.m[2] * z,
            self.m[3] * x + self.m[4] * y + self.m[5] * z,
        ]
    }

    #[inline]
    pub fn depth(&self, p: V3) -> f64 {
        let x = p[0] - self.o[0];
        let y = p[1] - self.o[1];
        let z = p[2] - self.o[2];
        self.m[6] * x + self.m[7] * y + self.m[8] * z
    }

    #[inline]
    pub fn dir(&self, v: V3) -> V3 {
        norm([
            self.m[0] * v[0] + self.m[1] * v[1] + self.m[2] * v[2],
            self.m[3] * v[0] + self.m[4] * v[1] + self.m[5] * v[2],
            self.m[6] * v[0] + self.m[7] * v[1] + self.m[8] * v[2],
        ])
    }
}

pub fn segs(view: &View, len: f64, lo: usize, hi: usize, quad: f64) -> usize {
    let raw = ((len * view.px) / quad).round() as isize;
    (raw.max(lo as isize).min(hi as isize)) as usize
}

/// Parametric surface sampled on an `nu x nv` grid in object space.
#[derive(Debug, Clone)]
pub struct Mesh {
    pub nu: usize,
    pub nv: usize,
    pub closed: bool,
    pub pos: Vec<f64>,
    pub face_normals: Vec<f64>,
    pub vert_normals: Option<Vec<f64>>,
}

pub enum Inside<'a> {
    Point(V3),
    Fn(&'a dyn Fn(f64, f64) -> V3),
}

pub fn make_mesh(
    surf: impl Fn(f64, f64) -> V3,
    nu: usize,
    nv: usize,
    inside: Inside<'_>,
    smooth: bool,
) -> Mesh {
    let w = nu + 1;
    let n_verts = w * (nv + 1);
    let mut pos = vec![0.0; n_verts * 3];
    let mut ext: f64 = 0.0;
    let mut k = 0;
    for j in 0..=nv {
        for i in 0..=nu {
            let p = surf((i as f64) / (nu as f64), (j as f64) / (nv as f64));
            pos[k * 3] = p[0];
            pos[k * 3 + 1] = p[1];
            pos[k * 3 + 2] = p[2];
            let d = (p[0] - pos[0]).abs() + (p[1] - pos[1]).abs() + (p[2] - pos[2]).abs();
            if d > ext {
                ext = d;
            }
            k += 1;
        }
    }

    let get_p = |idx: usize| -> V3 { [pos[idx * 3], pos[idx * 3 + 1], pos[idx * 3 + 2]] };
    let mut face_normals = vec![0.0; nu * nv * 3];
    for j in 0..nv {
        for i in 0..nu {
            let a = j * w + i;
            let pa = get_p(a);
            let pb = get_p(a + 1);
            let pc = get_p(a + w + 1);
            let pd = get_p(a + w);
            let mut n = cross(sub(pc, pa), sub(pd, pb));
            let mid: V3 = [
                (pa[0] + pb[0] + pc[0] + pd[0]) * 0.25,
                (pa[1] + pb[1] + pc[1] + pd[1]) * 0.25,
                (pa[2] + pb[2] + pc[2] + pd[2]) * 0.25,
            ];
            let ins = match inside {
                Inside::Point(pt) => pt,
                Inside::Fn(f) => f(
                    ((i as f64) + 0.5) / (nu as f64),
                    ((j as f64) + 0.5) / (nv as f64),
                ),
            };
            if dot(n, sub(mid, ins)) < 0.0 {
                n = [-n[0], -n[1], -n[2]];
            }
            n = norm(n);
            let f_idx = (j * nu + i) * 3;
            face_normals[f_idx] = n[0];
            face_normals[f_idx + 1] = n[1];
            face_normals[f_idx + 2] = n[2];
        }
    }

    let eps = 1e-6 + ext * 1e-5;
    let same = |p: usize, q: usize| -> bool {
        (pos[p * 3] - pos[q * 3]).abs()
            + (pos[p * 3 + 1] - pos[q * 3 + 1]).abs()
            + (pos[p * 3 + 2] - pos[q * 3 + 2]).abs()
            < eps
    };

    let mut closed = true;
    for j in 0..=nv {
        if !same(j * w, j * w + nu) {
            closed = false;
            break;
        }
    }

    let vert_normals = if smooth {
        let mut acc = vec![0.0; n_verts * 3];
        for j in 0..nv {
            for i in 0..nu {
                let f_idx = (j * nu + i) * 3;
                let a = j * w + i;
                for v_idx in [a, a + 1, a + w + 1, a + w] {
                    acc[v_idx * 3] += face_normals[f_idx];
                    acc[v_idx * 3 + 1] += face_normals[f_idx + 1];
                    acc[v_idx * 3 + 2] += face_normals[f_idx + 2];
                }
            }
        }
        let join = |acc: &mut [f64], ks: &[usize]| {
            let mut x = 0.0;
            let mut y = 0.0;
            let mut z = 0.0;
            for &idx in ks {
                x += acc[idx * 3];
                y += acc[idx * 3 + 1];
                z += acc[idx * 3 + 2];
            }
            for &idx in ks {
                acc[idx * 3] = x;
                acc[idx * 3 + 1] = y;
                acc[idx * 3 + 2] = z;
            }
        };
        if closed {
            for j in 0..=nv {
                join(&mut acc, &[j * w, j * w + nu]);
            }
        }
        for j in [0, nv] {
            let mut pole = true;
            for i in 1..=nu {
                if !same(j * w, j * w + i) {
                    pole = false;
                    break;
                }
            }
            if pole {
                let row: Vec<usize> = (0..w).map(|i| j * w + i).collect();
                join(&mut acc, &row);
            }
        }
        for idx in 0..n_verts {
            let v = norm([acc[idx * 3], acc[idx * 3 + 1], acc[idx * 3 + 2]]);
            acc[idx * 3] = v[0];
            acc[idx * 3 + 1] = v[1];
            acc[idx * 3 + 2] = v[2];
        }
        Some(acc)
    } else {
        None
    };

    Mesh {
        nu,
        nv,
        closed,
        pos,
        face_normals,
        vert_normals,
    }
}

pub fn make_tube(
    path: impl Fn(f64) -> V3,
    up: V3,
    prof: impl Fn(f64, f64) -> [f64; 2],
    nu: usize,
    nv: usize,
    closed: bool,
) -> Mesh {
    let e = 1e-3;
    let at = |u: f64| -> V3 {
        let uu = if closed {
            ((u % 1.0) + 1.0) % 1.0
        } else {
            u.clamp(0.0, 1.0)
        };
        path(uu)
    };
    let inside_fn = |u: f64, _v: f64| path(u);
    make_mesh(
        |u, v| {
            let c = path(u);
            let t = norm(sub(at(u + e), at(u - e)));
            let k = dot(up, t);
            let nn = norm([up[0] - t[0] * k, up[1] - t[1] * k, up[2] - t[2] * k]);
            let bn = cross(t, nn);
            let [a, b] = prof(u, v * TAU);
            [
                c[0] + nn[0] * a + bn[0] * b,
                c[1] + nn[1] * a + bn[1] * b,
                c[2] + nn[2] * a + bn[2] * b,
            ]
        },
        nu,
        nv,
        Inside::Fn(&inside_fn),
        true,
    )
}

#[inline]
fn js_sign(x: f64) -> f64 {
    if x > 0.0 {
        1.0
    } else if x < 0.0 {
        -1.0
    } else {
        0.0
    }
}

pub fn section(a: f64, b: f64, p: f64) -> impl Fn(f64, f64) -> [f64; 2] {
    move |_u: f64, ph: f64| {
        let (s, c) = ph.sin_cos();
        [
            a * js_sign(c) * c.abs().powf(p),
            b * js_sign(s) * s.abs().powf(p),
        ]
    }
}

/// Projected mesh for one frame.
#[derive(Debug, Clone)]
pub struct Proj {
    pub nu: usize,
    pub nv: usize,
    pub closed: bool,
    pub x: Vec<f64>,
    pub y: Vec<f64>,
    pub d: Vec<f64>,
    pub n: Option<Vec<f64>>,
    pub f: Vec<f64>,
    pub cd: Vec<f64>,
}

pub fn project_mesh(view: &View, mesh: &Mesh) -> Proj {
    let nu = mesh.nu;
    let nv = mesh.nv;
    let w = nu + 1;
    let n_verts = w * (nv + 1);
    let n_cells = nu * nv;
    let [m0, m1, m2, m3, m4, m5, m6, m7, m8] = view.m;
    let [ox, oy, oz] = view.o;

    let mut x = vec![0.0; n_verts];
    let mut y = vec![0.0; n_verts];
    let mut d = vec![0.0; n_verts];
    for k in 0..n_verts {
        let px = mesh.pos[k * 3] - ox;
        let py = mesh.pos[k * 3 + 1] - oy;
        let pz = mesh.pos[k * 3 + 2] - oz;
        x[k] = m0 * px + m1 * py + m2 * pz;
        y[k] = m3 * px + m4 * py + m5 * pz;
        d[k] = m6 * px + m7 * py + m8 * pz;
    }

    let turn = |src: &[f64], count: usize| -> Vec<f64> {
        let mut dst = vec![0.0; count * 3];
        for k in 0..count {
            let a = src[k * 3];
            let b = src[k * 3 + 1];
            let c = src[k * 3 + 2];
            let vx = m0 * a + m1 * b + m2 * c;
            let vy = m3 * a + m4 * b + m5 * c;
            let vz = m6 * a + m7 * b + m8 * c;
            let l = (vx * vx + vy * vy + vz * vz).sqrt();
            let denom = if l > 0.0 { l } else { 1.0 };
            dst[k * 3] = vx / denom;
            dst[k * 3 + 1] = vy / denom;
            dst[k * 3 + 2] = vz / denom;
        }
        dst
    };

    let n = mesh.vert_normals.as_ref().map(|vn| turn(vn, n_verts));
    let f = turn(&mesh.face_normals, n_cells);
    let mut cd = vec![0.0; n_cells];
    let mut c = 0;
    for j in 0..nv {
        for i in 0..nu {
            let a = j * w + i;
            cd[c] = (d[a] + d[a + 1] + d[a + w] + d[a + w + 1]) * 0.25;
            c += 1;
        }
    }

    Proj {
        nu,
        nv,
        closed: mesh.closed,
        x,
        y,
        d,
        n,
        f,
        cd,
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Bounds {
    pub x0: f64,
    pub y0: f64,
    pub x1: f64,
    pub y1: f64,
    pub cx: f64,
    pub cy: f64,
    pub w: f64,
    pub h: f64,
}

pub fn bounds_of<'a>(ps: impl IntoIterator<Item = &'a Proj>) -> Bounds {
    let mut x0 = f64::INFINITY;
    let mut y0 = f64::INFINITY;
    let mut x1 = f64::NEG_INFINITY;
    let mut y1 = f64::NEG_INFINITY;
    for p in ps {
        for (&px, &py) in p.x.iter().zip(p.y.iter()) {
            if px < x0 {
                x0 = px;
            }
            if px > x1 {
                x1 = px;
            }
            if py < y0 {
                y0 = py;
            }
            if py > y1 {
                y1 = py;
            }
        }
    }
    Bounds {
        x0,
        y0,
        x1,
        y1,
        cx: (x0 + x1) * 0.5,
        cy: (y0 + y1) * 0.5,
        w: x1 - x0,
        h: y1 - y0,
    }
}

pub struct Layer<'a> {
    pub parts: Vec<&'a Proj>,
    pub shade: Box<dyn Fn(V3) -> Rgb + 'a>,
    pub cull: bool,
    pub flat: bool,
    pub bias: f64,
    pub keep: Option<Box<dyn Fn(f64) -> bool + 'a>>,
}

impl<'a> Layer<'a> {
    pub fn new(parts: Vec<&'a Proj>, shade: impl Fn(V3) -> Rgb + 'a) -> Self {
        Self {
            parts,
            shade: Box::new(shade),
            cull: true,
            flat: false,
            bias: 0.0,
            keep: None,
        }
    }

    pub fn uncull(mut self) -> Self {
        self.cull = false;
        self
    }

    pub fn flat(mut self) -> Self {
        self.flat = true;
        self
    }

    pub fn with_bias(mut self, bias: f64) -> Self {
        self.bias = bias;
        self
    }

    pub fn with_keep(mut self, keep: impl Fn(f64) -> bool + 'a) -> Self {
        self.keep = Some(Box::new(keep));
        self
    }
}

fn monotone(mx: &[f64], my: &[f64], s: usize, e: usize) -> bool {
    let ax = mx[e] - mx[s];
    let ay = my[e] - my[s];
    let mut prev = f64::NEG_INFINITY;
    for k in s..=e {
        let t = (mx[k] - mx[s]) * ax + (my[k] - my[s]) * ay;
        if t < prev - 1e-9 {
            return false;
        }
        prev = t;
    }
    true
}

struct Piece<'a> {
    d: f64,
    layer_idx: usize,
    p: &'a Proj,
    j: usize,
    cols: Vec<usize>,
    cells: Vec<usize>,
}

pub fn paint_layers(ctx: &mut Canvas2D, layers: &[Layer<'_>], px: f64) -> Vec<P2> {
    let mut pieces: Vec<Piece<'_>> = Vec::new();
    let mut corners: Vec<P2> = Vec::new();

    for (layer_idx, l) in layers.iter().enumerate() {
        let bias = l.bias;
        let cull = l.cull;
        for &p in &l.parts {
            let nu = p.nu;
            let nv = p.nv;
            let w = nu + 1;
            let vis = |c: usize| -> bool {
                (!cull || p.f[c * 3 + 2] > -0.04)
                    && l.keep.as_ref().is_none_or(|keep_fn| keep_fn(p.cd[c]))
            };
            for j in 0..nv {
                if l.flat {
                    for i in 0..nu {
                        let c = j * nu + i;
                        if !vis(c) {
                            continue;
                        }
                        pieces.push(Piece {
                            d: p.cd[c] + bias,
                            layer_idx,
                            p,
                            j,
                            cols: vec![i, i + 1],
                            cells: vec![c],
                        });
                        corners.push([p.x[j * w + i], p.y[j * w + i]]);
                        corners.push([p.x[(j + 1) * w + i + 1], p.y[(j + 1) * w + i + 1]]);
                    }
                    continue;
                }

                let mut runs: Vec<Vec<usize>> = Vec::new();
                let mut in_run = false;
                for i in 0..nu {
                    if vis(j * nu + i) {
                        if !in_run {
                            runs.push(Vec::new());
                            in_run = true;
                        }
                        runs.last_mut().unwrap().push(i);
                    } else {
                        in_run = false;
                    }
                }
                if p.closed
                    && runs.len() > 1
                    && runs[0][0] == 0
                    && *runs.last().unwrap().last().unwrap() == nu - 1
                {
                    let mut tail = runs.pop().unwrap();
                    tail.extend_from_slice(&runs[0]);
                    runs[0] = tail;
                }

                for run in runs {
                    let mut cols = Vec::with_capacity(run.len() + 1);
                    cols.extend_from_slice(&run);
                    cols.push(*run.last().unwrap() + 1);

                    let mut mx = Vec::with_capacity(cols.len());
                    let mut my = Vec::with_capacity(cols.len());
                    for (k, &col_i) in cols.iter().enumerate() {
                        let t = j * w + col_i;
                        let b = t + w;
                        mx.push((p.x[t] + p.x[b]) * 0.5);
                        my.push((p.y[t] + p.y[b]) * 0.5);
                        if k == 0 || k + 1 == cols.len() {
                            corners.push([p.x[t], p.y[t]]);
                            corners.push([p.x[b], p.y[b]]);
                        } else if j == 0 {
                            corners.push([p.x[t], p.y[t]]);
                        } else if j + 1 == nv {
                            corners.push([p.x[b], p.y[b]]);
                        }
                    }

                    let mut s = 0;
                    while s + 1 < cols.len() {
                        let mut e = s + 1;
                        while e + 1 < cols.len() && monotone(&mx, &my, s, e + 1) {
                            e += 1;
                        }
                        let mut dd = 0.0;
                        for &r_col in &run[s..e] {
                            dd += p.cd[j * nu + r_col];
                        }
                        pieces.push(Piece {
                            d: dd / ((e - s) as f64) + bias,
                            layer_idx,
                            p,
                            j,
                            cols: cols[s..=e].to_vec(),
                            cells: run[s..e].to_vec(),
                        });
                        s = e;
                    }
                }
            }
        }
    }

    pieces.sort_by(|a, b| a.d.total_cmp(&b.d));
    let grow = if px > 0.0 { 0.5 / px } else { 0.0 };

    let mut tx = Vec::new();
    let mut ty = Vec::new();
    let mut bx = Vec::new();
    let mut by = Vec::new();

    for pc in pieces {
        let l = &layers[pc.layer_idx];
        let p = pc.p;
        let j = pc.j;
        let cols = &pc.cols;
        let cells = &pc.cells;
        let w = p.nu + 1;
        let nc = cols.len();

        tx.clear();
        ty.clear();
        bx.clear();
        by.clear();

        let t0 = j * w + cols[0];
        let tn = j * w + cols[nc - 1];
        let m0x = (p.x[t0] + p.x[t0 + w]) * 0.5;
        let m0y = (p.y[t0] + p.y[t0 + w]) * 0.5;
        let mnx = (p.x[tn] + p.x[tn + w]) * 0.5;
        let mny = (p.y[tn] + p.y[tn + w]) * 0.5;
        let ax = mnx - m0x;
        let ay = mny - m0y;
        let al = ax.hypot(ay);
        let (ux, uy) = if al > 1e-9 {
            (ax / al, ay / al)
        } else {
            (0.0, 0.0)
        };

        for (k, &col_i) in cols.iter().enumerate() {
            let t = j * w + col_i;
            let b = t + w;
            let mut dx = p.x[t] - p.x[b];
            let mut dy = p.y[t] - p.y[b];
            let dl = dx.hypot(dy);
            if dl > 1e-9 {
                dx /= dl;
                dy /= dl;
            } else {
                dx = 0.0;
                dy = 0.0;
            }
            let e = if k == 0 {
                -grow
            } else if k + 1 == nc {
                grow
            } else {
                0.0
            };
            tx.push(p.x[t] + dx * grow + ux * e);
            ty.push(p.y[t] + dy * grow + uy * e);
            bx.push(p.x[b] - dx * grow + ux * e);
            by.push(p.y[b] - dy * grow + uy * e);
        }

        let colour = |k: usize| -> Rgb {
            if let Some(ref vn) = p.n
                && !l.flat
            {
                let t = (j * w + cols[k]) * 3;
                let b = t + w * 3;
                return (l.shade)(norm([
                    vn[t] + vn[b],
                    vn[t + 1] + vn[b + 1],
                    vn[t + 2] + vn[b + 2],
                ]));
            }
            let idx0 = k.saturating_sub(1).min(cells.len() - 1);
            let idx1 = k.min(cells.len() - 1);
            let c0 = cells[idx0] * 3;
            let c1 = cells[idx1] * 3;
            if l.flat {
                (l.shade)([p.f[c1], p.f[c1 + 1], p.f[c1 + 2]])
            } else {
                (l.shade)(norm([
                    p.f[c0] + p.f[c1],
                    p.f[c0 + 1] + p.f[c1 + 1],
                    p.f[c0 + 2] + p.f[c1 + 2],
                ]))
            }
        };

        let style = if l.flat || al < 1e-6 {
            PaintStyle::Solid(rgb_to_rgba_color(colour(0)))
        } else {
            let mut stops = Vec::with_capacity(nc);
            let mut last: Option<Rgb> = None;
            let mut prev_off: f32 = 0.0;
            let inv_al2 = 1.0 / (al * al);
            for (k, &col_i) in cols.iter().enumerate() {
                let c = colour(k);
                if let Some(prev_c) = last
                    && k + 1 < nc
                    && (c[0] - prev_c[0]).abs()
                        + (c[1] - prev_c[1]).abs()
                        + (c[2] - prev_c[2]).abs()
                        < 5.0
                {
                    continue;
                }
                let t = j * w + col_i;
                let mx_k = (p.x[t] + p.x[t + w]) * 0.5;
                let my_k = (p.y[t] + p.y[t + w]) * 0.5;
                let raw_off =
                    (((mx_k - m0x) * ax + (my_k - m0y) * ay) * inv_al2).clamp(0.0, 1.0) as f32;
                let off = raw_off.max(prev_off);
                prev_off = off;
                stops.push((off, rgb_to_rgba_color(c)));
                last = Some(c);
            }
            PaintStyle::LinearGradient {
                x0: m0x,
                y0: m0y,
                x1: mnx,
                y1: mny,
                stops,
            }
        };

        let mut pb = PathBuilder::new();
        pb.move_to(tx[0] as f32, ty[0] as f32);
        for k in 1..nc {
            pb.line_to(tx[k] as f32, ty[k] as f32);
        }
        for k in (0..nc).rev() {
            pb.line_to(bx[k] as f32, by[k] as f32);
        }
        pb.close();
        if let Some(path) = pb.finish() {
            ctx.set_fill_style(style);
            ctx.fill_path(&path);
        }
    }

    corners
}

pub fn hull(pts: &[P2]) -> Vec<P2> {
    let mut p = pts.to_vec();
    p.sort_by(|a, b| a[0].total_cmp(&b[0]).then_with(|| a[1].total_cmp(&b[1])));
    if p.len() < 3 {
        return p;
    }
    let cr = |o: P2, a: P2, b: P2| -> f64 {
        (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0])
    };
    let mut lo: Vec<P2> = Vec::new();
    for &q in &p {
        while lo.len() >= 2 && cr(lo[lo.len() - 2], lo[lo.len() - 1], q) <= 0.0 {
            lo.pop();
        }
        lo.push(q);
    }
    let mut up: Vec<P2> = Vec::new();
    for &q in p.iter().rev() {
        while up.len() >= 2 && cr(up[up.len() - 2], up[up.len() - 1], q) <= 0.0 {
            up.pop();
        }
        up.push(q);
    }
    lo.pop();
    up.pop();
    lo.extend_from_slice(&up);
    lo
}

/* ── materials ─────────────────────────────────────────────────────── */

pub fn felt(view: &View, base: Rgb, k: f64) -> impl Fn(V3) -> Rgb + use<> {
    let l = view.l;
    move |n: V3| {
        let nl = dot(n, l);
        let dif = clamp01((nl + 0.4) / 1.4).powf(1.35);
        let graze = (1.0 - clamp01(n[2])).powi(2);
        let lit = (0.22 + 0.95 * dif + 0.22 * graze * clamp01(nl + 0.6)) * k;
        let floor = 26.0 * dif * k;
        [
            base[0] * lit + floor,
            base[1] * lit + floor,
            base[2] * lit + floor * 1.05,
        ]
    }
}

pub fn velvet(view: &View, base: Rgb) -> impl Fn(V3) -> Rgb + use<> {
    let l = view.l;
    let sheen = mix(base, WHITE, 0.3);
    move |n: V3| {
        let nl = dot(n, l);
        let dif = clamp01((nl + 0.3) / 1.3);
        let graze = (1.0 - clamp01(n[2])).powf(1.6);
        mix(
            scale(base, 0.2 + 0.7 * dif),
            sheen,
            0.6 * graze * (0.4 + 0.6 * dif),
        )
    }
}

pub fn paper(view: &View, base: Rgb) -> impl Fn(V3) -> Rgb + use<> {
    let l = view.l;
    let h = view.h;
    move |n: V3| {
        let dif = clamp01((dot(n, l) + 0.2) / 1.2);
        let spec = clamp01(dot(n, h)).powi(22) * 0.3;
        mix(scale(base, 0.36 + 0.7 * dif), WHITE, spec)
    }
}

pub fn plastic(view: &View, base: Rgb, gloss: f64) -> impl Fn(V3) -> Rgb + use<> {
    let l = view.l;
    let h = view.h;
    move |n: V3| {
        let dif = clamp01((dot(n, l) + 0.25) / 1.25);
        let nh = clamp01(dot(n, h));
        let spec = (nh.powi(16) * 0.2 + nh.powi(80) * 0.6) * gloss;
        let rim = (1.0 - clamp01(n[2])).powi(3) * 0.14 * gloss;
        mix(scale(base, 0.28 + 0.78 * dif), WHITE, clamp01(spec + rim))
    }
}

pub fn satin(view: &View, base: Rgb) -> impl Fn(V3) -> Rgb + use<> {
    let l = view.l;
    let h = view.h;
    let deep = scale(base, 0.42);
    let sheen = mix(base, WHITE, 0.55);
    move |n: V3| {
        let dif = clamp01((dot(n, l) + 0.35) / 1.35);
        let nh = clamp01(dot(n, h));
        let s = nh.powi(6) * 0.5 + nh.powi(40) * 0.35;
        mix(mix(deep, base, dif), sheen, clamp01(s))
    }
}

pub fn metal(view: &View, tint: Rgb, dark: Rgb, glint: Rgb) -> impl Fn(V3) -> Rgb + use<> {
    let h = view.h;
    let l = view.l;
    move |n: V3| {
        let rz = 2.0 * n[2] * n[2] - 1.0;
        let ry = 2.0 * n[2] * n[1];
        let up = -ry;
        let sky = clamp01(0.45 + up * 0.85 + rz * 0.1);
        let hd = (up + 0.12) / 0.16;
        let horizon = (-hd * hd).exp() * 0.55;
        let nl = clamp01(dot(n, l));
        let c = mix(dark, tint, clamp01(0.1 + sky * 0.72 + nl * 0.3));
        let spec = clamp01(dot(n, h)).powi(70) + horizon * 0.4;
        mix(c, glint, clamp01(spec))
    }
}

pub fn gold(view: &View) -> impl Fn(V3) -> Rgb + use<> {
    metal(
        view,
        [255.0, 200.0, 88.0],
        [86.0, 50.0, 8.0],
        [255.0, 247.0, 220.0],
    )
}

pub fn silver(view: &View) -> impl Fn(V3) -> Rgb + use<> {
    metal(
        view,
        [232.0, 236.0, 242.0],
        [58.0, 62.0, 72.0],
        [255.0, 255.0, 255.0],
    )
}

pub fn pearl(view: &View) -> impl Fn(V3) -> Rgb + use<> {
    let l = view.l;
    let h = view.h;
    move |n: V3| {
        let dif = clamp01((dot(n, l) + 0.4) / 1.4);
        let base = mix(
            [238.0, 228.0, 228.0],
            [212.0, 222.0, 242.0],
            1.0 - clamp01(n[2]),
        );
        mix(
            scale(base, 0.62 + 0.42 * dif),
            WHITE,
            clamp01(dot(n, h)).powi(40) * 0.8,
        )
    }
}

pub fn gem(view: &View, base: Rgb) -> impl Fn(V3) -> Rgb + use<> {
    let l = view.l;
    let h = view.h;
    let deep = scale(base, 0.22);
    let clear = mix(base, WHITE, 0.3);
    move |n: V3| {
        let c = mix(deep, clear, clamp01(clamp01(n[2]) * 0.75 + dot(n, l) * 0.4));
        mix(c, WHITE, clamp01(dot(n, h)).powi(24) * 0.95)
    }
}

#[derive(Debug, Clone, Copy)]
pub enum SpriteKind {
    Gold,
    Silver,
    Pearl,
    Rgb(Rgb),
}

const SPRITE_GOLD: &[(f64, &str)] = &[
    (0.0, "#fffbe8"),
    (0.08, "#ffe7a0"),
    (0.3, "#eeb546"),
    (0.62, "#ad7214"),
    (0.87, "#5a3806"),
    (1.0, "#8c5c18"),
];
const SPRITE_SILVER: &[(f64, &str)] = &[
    (0.0, "#ffffff"),
    (0.08, "#f3f5f9"),
    (0.32, "#c6ccd6"),
    (0.64, "#7a808c"),
    (0.88, "#3d4149"),
    (1.0, "#6b717c"),
];
const SPRITE_PEARL: &[(f64, &str)] = &[
    (0.0, "#ffffff"),
    (0.1, "#fdf9f7"),
    (0.45, "#ebe4e6"),
    (0.8, "#c6bccb"),
    (1.0, "#a89db5"),
];

pub fn sphere_sprite(ctx: &mut Canvas2D, view: &View, c: V3, rad: f64, kind: SpriteKind) {
    let Some(circle) = circle_path(view.to_screen(c)[0], view.to_screen(c)[1], rad) else {
        return;
    };
    let s = view.to_screen(c);
    let h = view.h;
    let fx = s[0] + h[0] * rad * 0.85;
    let fy = s[1] + h[1] * rad * 0.85;
    let style = match kind {
        SpriteKind::Gold => PaintStyle::radial_gradient(fx, fy, 0.0, s[0], s[1], rad, SPRITE_GOLD),
        SpriteKind::Silver => {
            PaintStyle::radial_gradient(fx, fy, 0.0, s[0], s[1], rad, SPRITE_SILVER)
        }
        SpriteKind::Pearl => {
            PaintStyle::radial_gradient(fx, fy, 0.0, s[0], s[1], rad, SPRITE_PEARL)
        }
        SpriteKind::Rgb(rgb) => PaintStyle::RadialGradient {
            x0: fx,
            y0: fy,
            r0: 0.0,
            x1: s[0],
            y1: s[1],
            r1: rad,
            stops: vec![
                (0.0, rgb_to_rgba_color(mix(rgb, WHITE, 0.8))),
                (0.1, rgb_to_rgba_color(mix(rgb, WHITE, 0.3))),
                (0.35, rgb_to_rgba_color(rgb)),
                (0.85, rgb_to_rgba_color(scale(rgb, 0.5))),
                (1.0, rgb_to_rgba_color(scale(rgb, 0.62))),
            ],
        },
    };
    ctx.set_fill_style(style);
    ctx.fill_path(&circle);
}

#[allow(clippy::too_many_arguments)]
pub fn cut_gem(ctx: &mut Canvas2D, view: &View, c: V3, n_surf: V3, t_surf: V3, rad: f64, col: Rgb) {
    let b = cross(n_surf, t_surf);
    let at = |r: f64, a: f64, h: f64| -> V3 {
        let (sa, ca) = a.sin_cos();
        let x = r * ca;
        let y = r * sa;
        [
            c[0] + x * t_surf[0] + y * b[0] + h * n_surf[0],
            c[1] + x * t_surf[1] + y * b[1] + h * n_surf[1],
            c[2] + x * t_surf[2] + y * b[2] + h * n_surf[2],
        ]
    };

    const K: usize = 8;
    let tr = 0.55;
    let th = 0.36 * rad;
    let mut girdle = [[0.0; 3]; K];
    let mut table = [[0.0; 3]; K];
    for k in 0..K {
        let a = (((k as f64) + 0.5) / (K as f64)) * TAU;
        girdle[k] = at(rad, a, 0.0);
        table[k] = at(rad * tr, a, th);
    }

    let shade = gem(view, col);
    ctx.set_line_width(0.3_f64.min(if view.px > 0.0 { 1.0 / view.px } else { 0.3 }));
    ctx.set_line_join(LineJoin::Round);

    let mut draw_poly = |pts: &[V3], normal: V3| {
        let nv = view.dir(normal);
        if nv[2] <= 0.02 {
            return;
        }
        let col_rgba = rgb_to_rgba_color(shade(nv));
        let mut pb = PathBuilder::new();
        for (i, &p) in pts.iter().enumerate() {
            let s = view.to_screen(p);
            if i == 0 {
                pb.move_to(s[0] as f32, s[1] as f32);
            } else {
                pb.line_to(s[0] as f32, s[1] as f32);
            }
        }
        pb.close();
        if let Some(path) = pb.finish() {
            ctx.set_fill_style(PaintStyle::Solid(col_rgba));
            ctx.set_stroke_style(PaintStyle::Solid(col_rgba));
            ctx.fill_path(&path);
            ctx.stroke_path(&path);
        }
    };

    for k in 0..K {
        let g0 = girdle[k];
        let g1 = girdle[(k + 1) % K];
        let t1 = table[(k + 1) % K];
        let t0 = table[k];
        let mut fn_n = cross(sub(t1, g0), sub(t0, g1));
        if dot(fn_n, n_surf) < 0.0 {
            fn_n = [-fn_n[0], -fn_n[1], -fn_n[2]];
        }
        draw_poly(&[g0, g1, t1, t0], norm(fn_n));
    }
    draw_poly(&table, n_surf);

    let nv = view.dir(n_surf);
    if nv[2] > 0.1 {
        let s0 = view.to_screen(at(0.0, 0.0, th));
        let l = view.l;
        let ang = (-l[1]).atan2(-l[0]);
        let s1 = view.to_screen(at(rad * tr * 0.8, ang + 2.2, th));
        let s2 = view.to_screen(at(rad * tr * 0.9, ang + PI, th));
        let s3 = view.to_screen(at(rad * tr * 0.8, ang + PI + 0.94, th));
        let mut pb = PathBuilder::new();
        pb.move_to(s0[0] as f32, s0[1] as f32);
        pb.line_to(s1[0] as f32, s1[1] as f32);
        pb.line_to(s2[0] as f32, s2[1] as f32);
        pb.line_to(s3[0] as f32, s3[1] as f32);
        pb.close();
        if let Some(path) = pb.finish() {
            ctx.set_fill_style(PaintStyle::rgba(255, 255, 255, 0.3));
            ctx.fill_path(&path);
        }
    }
}
