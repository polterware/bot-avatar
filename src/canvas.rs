use std::f64::consts::TAU;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_CANVAS: AtomicU64 = AtomicU64::new(1);

fn next_canvas_id() -> u64 {
    NEXT_CANVAS.fetch_add(1, Ordering::Relaxed)
}

use tiny_skia::{
    BlendMode, Color, FillRule, FilterQuality, GradientStop, LineCap, LineJoin, LinearGradient,
    Mask, Paint, Path, PathBuilder, Pattern, Pixmap, PixmapMut, Point, RadialGradient, Rect,
    SpreadMode, Stroke, StrokeDash, Transform,
};

use crate::color::parse_color_rgba;

/// Multiply two 2D affine matrices `[a, b, c, d, e, f]` (`m1 * m2`).
#[inline]
pub fn mul_affine(m1: [f64; 6], m2: [f64; 6]) -> [f64; 6] {
    [
        m1[0] * m2[0] + m1[2] * m2[1],
        m1[1] * m2[0] + m1[3] * m2[1],
        m1[0] * m2[2] + m1[2] * m2[3],
        m1[1] * m2[2] + m1[3] * m2[3],
        m1[0] * m2[4] + m1[2] * m2[5] + m1[4],
        m1[1] * m2[4] + m1[3] * m2[5] + m1[5],
    ]
}

#[inline]
pub fn to_ts_transform(m: [f64; 6]) -> Transform {
    Transform::from_row(
        m[0] as f32,
        m[1] as f32,
        m[2] as f32,
        m[3] as f32,
        m[4] as f32,
        m[5] as f32,
    )
}

/// Straight (unpremultiplied) RGBA color in `0.0..=1.0`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RgbaColor {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl RgbaColor {
    pub const TRANSPARENT: Self = Self {
        r: 0.0,
        g: 0.0,
        b: 0.0,
        a: 0.0,
    };
    pub const BLACK: Self = Self {
        r: 0.0,
        g: 0.0,
        b: 0.0,
        a: 1.0,
    };
    pub const WHITE: Self = Self {
        r: 1.0,
        g: 1.0,
        b: 1.0,
        a: 1.0,
    };

    #[inline]
    pub fn from_rgba8(r: u8, g: u8, b: u8, a: f64) -> Self {
        Self {
            r: (r as f32) / 255.0,
            g: (g as f32) / 255.0,
            b: (b as f32) / 255.0,
            a: (a as f32).clamp(0.0, 1.0),
        }
    }

    #[inline]
    pub fn from_css(s: &str) -> Self {
        if s.eq_ignore_ascii_case("transparent") {
            return Self::TRANSPARENT;
        }
        if let Some([r, g, b, a]) = parse_color_rgba(s) {
            Self {
                r: (r / 255.0).clamp(0.0, 1.0) as f32,
                g: (g / 255.0).clamp(0.0, 1.0) as f32,
                b: (b / 255.0).clamp(0.0, 1.0) as f32,
                a: a.clamp(0.0, 1.0) as f32,
            }
        } else {
            Self::BLACK
        }
    }

    #[inline]
    pub fn with_alpha_factor(self, factor: f32) -> Color {
        Color::from_rgba(
            self.r.clamp(0.0, 1.0),
            self.g.clamp(0.0, 1.0),
            self.b.clamp(0.0, 1.0),
            (self.a * factor).clamp(0.0, 1.0),
        )
        .unwrap_or(Color::TRANSPARENT)
    }
}

#[derive(Debug, Clone)]
pub enum PaintStyle {
    Solid(RgbaColor),
    LinearGradient {
        x0: f64,
        y0: f64,
        x1: f64,
        y1: f64,
        stops: Vec<(f32, RgbaColor)>,
    },
    RadialGradient {
        x0: f64,
        y0: f64,
        r0: f64,
        x1: f64,
        y1: f64,
        r1: f64,
        stops: Vec<(f32, RgbaColor)>,
    },
    Pattern {
        pixmap: Arc<Pixmap>,
        spread: SpreadMode,
    },
}

impl Default for PaintStyle {
    fn default() -> Self {
        PaintStyle::Solid(RgbaColor::BLACK)
    }
}

impl PaintStyle {
    #[inline]
    pub fn css(color: &str) -> Self {
        PaintStyle::Solid(RgbaColor::from_css(color))
    }

    #[inline]
    pub fn rgba(r: u8, g: u8, b: u8, a: f64) -> Self {
        PaintStyle::Solid(RgbaColor::from_rgba8(r, g, b, a))
    }

    pub fn linear_gradient(x0: f64, y0: f64, x1: f64, y1: f64, stops: &[(f64, &str)]) -> Self {
        PaintStyle::LinearGradient {
            x0,
            y0,
            x1,
            y1,
            stops: stops
                .iter()
                .map(|&(pos, col)| (pos.clamp(0.0, 1.0) as f32, RgbaColor::from_css(col)))
                .collect(),
        }
    }

    pub fn radial_gradient(
        x0: f64,
        y0: f64,
        r0: f64,
        x1: f64,
        y1: f64,
        r1: f64,
        stops: &[(f64, &str)],
    ) -> Self {
        PaintStyle::RadialGradient {
            x0,
            y0,
            r0,
            x1,
            y1,
            r1,
            stops: stops
                .iter()
                .map(|&(pos, col)| (pos.clamp(0.0, 1.0) as f32, RgbaColor::from_css(col)))
                .collect(),
        }
    }
}

#[derive(Clone)]
struct CanvasState {
    ctm: [f64; 6],
    global_alpha: f32,
    blend_mode: BlendMode,
    fill_style: PaintStyle,
    stroke_style: PaintStyle,
    line_width: f64,
    line_cap: LineCap,
    line_join: LineJoin,
    miter_limit: f32,
    line_dash: Option<Vec<f32>>,
    line_dash_offset: f32,
    smoothing_enabled: bool,
    smoothing_quality: FilterQuality,
    clip: Option<Arc<Mask>>,
}

impl Default for CanvasState {
    fn default() -> Self {
        Self {
            ctm: [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
            global_alpha: 1.0,
            blend_mode: BlendMode::SourceOver,
            fill_style: PaintStyle::default(),
            stroke_style: PaintStyle::default(),
            line_width: 1.0,
            line_cap: LineCap::Butt,
            line_join: LineJoin::Miter,
            miter_limit: 10.0,
            line_dash: None,
            line_dash_offset: 0.0,
            smoothing_enabled: true,
            smoothing_quality: FilterQuality::Bilinear,
            clip: None,
        }
    }
}

/// HTML5 Canvas 2D-compatible drawing context backed by a [`tiny_skia::Pixmap`].
pub struct Canvas2D {
    pub pixmap: Pixmap,
    /// Stable id. Cap state is keyed by this, not by the pixmap allocation.
    pub id: u64,
    state: CanvasState,
    stack: Vec<CanvasState>,
}

impl Canvas2D {
    pub fn new(width: u32, height: u32) -> Self {
        let w = width.max(1);
        let h = height.max(1);
        Self {
            pixmap: Pixmap::new(w, h).expect("valid pixmap dimensions"),
            id: next_canvas_id(),
            state: CanvasState::default(),
            stack: Vec::with_capacity(16),
        }
    }

    pub fn ensure_size(&mut self, width: u32, height: u32) {
        let w = width.max(1);
        let h = height.max(1);
        if self.pixmap.width() != w || self.pixmap.height() != h {
            self.pixmap = Pixmap::new(w, h).expect("valid pixmap dimensions");
        }
        self.reset_state();
    }

    pub fn ensure_min_size(&mut self, min_width: u32, min_height: u32) {
        let w = min_width.max(1);
        let h = min_height.max(1);
        if self.pixmap.width() < w || self.pixmap.height() < h {
            let nw = self.pixmap.width().max(w);
            let nh = self.pixmap.height().max(h);
            self.pixmap = Pixmap::new(nw, nh).expect("valid pixmap dimensions");
        }
        self.reset_state();
    }

    pub fn reset_state(&mut self) {
        self.stack.clear();
        self.state = CanvasState::default();
    }

    #[inline]
    pub fn width(&self) -> u32 {
        self.pixmap.width()
    }

    #[inline]
    pub fn height(&self) -> u32 {
        self.pixmap.height()
    }

    #[inline]
    pub fn clear_all(&mut self) {
        self.pixmap.fill(Color::TRANSPARENT);
    }

    /// Clear pixel rectangle `[0..w, 0..h]` in device space (plus a 1px guard edge if inside buffer).
    pub fn clear_device_rect(&mut self, w: u32, h: u32) {
        let pw = self.pixmap.width();
        let ph = self.pixmap.height();
        let cw = (w + 1).min(pw) as usize;
        let ch = (h + 1).min(ph) as usize;
        if cw == pw as usize && ch == ph as usize {
            self.pixmap.fill(Color::TRANSPARENT);
            return;
        }
        let stride = pw as usize * 4;
        let row_bytes = cw * 4;
        let data = self.pixmap.data_mut();
        for y in 0..ch {
            let off = y * stride;
            data[off..off + row_bytes].fill(0);
        }
    }

    #[inline]
    pub fn save(&mut self) {
        self.stack.push(self.state.clone());
    }

    #[inline]
    pub fn restore(&mut self) {
        if let Some(prev) = self.stack.pop() {
            self.state = prev;
        }
    }

    #[inline]
    pub fn get_transform(&self) -> [f64; 6] {
        self.state.ctm
    }

    #[inline]
    pub fn set_transform(&mut self, a: f64, b: f64, c: f64, d: f64, e: f64, f: f64) {
        self.state.ctm = [a, b, c, d, e, f];
    }

    #[inline]
    pub fn reset_transform(&mut self) {
        self.state.ctm = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];
    }

    #[inline]
    pub fn transform(&mut self, a: f64, b: f64, c: f64, d: f64, e: f64, f: f64) {
        self.state.ctm = mul_affine(self.state.ctm, [a, b, c, d, e, f]);
    }

    #[inline]
    pub fn translate(&mut self, tx: f64, ty: f64) {
        self.transform(1.0, 0.0, 0.0, 1.0, tx, ty);
    }

    #[inline]
    pub fn scale(&mut self, sx: f64, sy: f64) {
        self.transform(sx, 0.0, 0.0, sy, 0.0, 0.0);
    }

    #[inline]
    pub fn rotate(&mut self, angle_rad: f64) {
        let (s, c) = angle_rad.sin_cos();
        self.transform(c, s, -s, c, 0.0, 0.0);
    }

    #[inline]
    pub fn global_alpha(&self) -> f64 {
        self.state.global_alpha as f64
    }

    #[inline]
    pub fn set_global_alpha(&mut self, alpha: f64) {
        if alpha.is_finite() {
            self.state.global_alpha = alpha.clamp(0.0, 1.0) as f32;
        }
    }

    #[inline]
    pub fn set_blend_mode(&mut self, mode: BlendMode) {
        self.state.blend_mode = mode;
    }

    #[inline]
    pub fn set_fill_style(&mut self, style: PaintStyle) {
        self.state.fill_style = style;
    }

    #[inline]
    pub fn set_fill_css(&mut self, color: &str) {
        self.state.fill_style = PaintStyle::css(color);
    }

    #[inline]
    pub fn set_stroke_style(&mut self, style: PaintStyle) {
        self.state.stroke_style = style;
    }

    #[inline]
    pub fn set_stroke_css(&mut self, color: &str) {
        self.state.stroke_style = PaintStyle::css(color);
    }

    #[inline]
    pub fn set_line_width(&mut self, width: f64) {
        if width.is_finite() && width > 0.0 {
            self.state.line_width = width;
        }
    }

    #[inline]
    pub fn set_line_cap(&mut self, cap: LineCap) {
        self.state.line_cap = cap;
    }

    #[inline]
    pub fn set_line_join(&mut self, join: LineJoin) {
        self.state.line_join = join;
    }

    #[inline]
    pub fn set_miter_limit(&mut self, limit: f64) {
        if limit.is_finite() && limit > 0.0 {
            self.state.miter_limit = limit as f32;
        }
    }

    #[inline]
    pub fn set_line_dash(&mut self, segments: &[f64]) {
        if segments.is_empty() {
            self.state.line_dash = None;
        } else {
            let mut v: Vec<f32> = segments.iter().map(|&x| (x.max(0.0)) as f32).collect();
            if v.len() % 2 == 1 {
                let copy = v.clone();
                v.extend_from_slice(&copy);
            }
            if v.iter().all(|&x| x <= 0.0) {
                self.state.line_dash = None;
            } else {
                self.state.line_dash = Some(v);
            }
        }
    }

    #[inline]
    pub fn set_image_smoothing(&mut self, enabled: bool) {
        self.state.smoothing_enabled = enabled;
    }

    #[inline]
    pub fn set_image_smoothing_quality(&mut self, quality: FilterQuality) {
        self.state.smoothing_quality = quality;
    }

    pub fn clip_path(&mut self, path: &Path, fill_rule: FillRule) {
        let ts = to_ts_transform(self.state.ctm);
        let w = self.pixmap.width();
        let h = self.pixmap.height();
        match &self.state.clip {
            None => {
                let mut mask = Mask::new(w, h).expect("valid mask dimensions");
                mask.fill_path(path, fill_rule, true, ts);
                self.state.clip = Some(Arc::new(mask));
            }
            Some(existing) => {
                let mut mask = (**existing).clone();
                mask.intersect_path(path, fill_rule, true, ts);
                self.state.clip = Some(Arc::new(mask));
            }
        }
    }

    pub fn clip_rect(&mut self, x: f64, y: f64, w: f64, h: f64) {
        if let Some(path) = rect_path(x, y, w, h) {
            self.clip_path(&path, FillRule::Winding);
        }
    }

    fn build_paint<'a>(&self, style: &'a PaintStyle) -> Option<Paint<'a>> {
        let alpha = self.state.global_alpha;
        if alpha <= 0.0
            && !matches!(
                self.state.blend_mode,
                BlendMode::Clear
                    | BlendMode::Source
                    | BlendMode::DestinationIn
                    | BlendMode::DestinationOut
            )
        {
            return None;
        }
        let mut paint = Paint {
            blend_mode: self.state.blend_mode,
            anti_alias: true,
            ..Paint::default()
        };

        match style {
            PaintStyle::Solid(c) => {
                paint.set_color(c.with_alpha_factor(alpha));
            }
            PaintStyle::LinearGradient {
                x0,
                y0,
                x1,
                y1,
                stops,
            } => {
                if stops.is_empty() {
                    return None;
                }
                let gstops: Vec<GradientStop> = stops
                    .iter()
                    .map(|&(pos, col)| GradientStop::new(pos, col.with_alpha_factor(alpha)))
                    .collect();
                if let Some(shader) = LinearGradient::new(
                    Point::from_xy(*x0 as f32, *y0 as f32),
                    Point::from_xy(*x1 as f32, *y1 as f32),
                    gstops,
                    SpreadMode::Pad,
                    Transform::identity(),
                ) {
                    paint.shader = shader;
                } else {
                    paint.set_color(stops.last().unwrap().1.with_alpha_factor(alpha));
                }
            }
            PaintStyle::RadialGradient {
                x0,
                y0,
                r0,
                x1,
                y1,
                r1,
                stops,
            } => {
                if stops.is_empty() {
                    return None;
                }
                if *r1 <= 1e-6 {
                    paint.set_color(stops.last().unwrap().1.with_alpha_factor(alpha));
                    return Some(paint);
                }
                let concentric = (*x0 - *x1).abs() < 1e-5 && (*y0 - *y1).abs() < 1e-5;
                let gstops: Vec<GradientStop> = if concentric && *r0 > 1e-6 && *r0 < *r1 {
                    let ratio = (*r0 / *r1) as f32;
                    let span = (1.0 - ratio).max(1e-6);
                    let mut remapped = Vec::with_capacity(stops.len() + 1);
                    remapped.push(GradientStop::new(0.0, stops[0].1.with_alpha_factor(alpha)));
                    for &(pos, col) in stops {
                        remapped.push(GradientStop::new(
                            (ratio + pos * span).clamp(0.0, 1.0),
                            col.with_alpha_factor(alpha),
                        ));
                    }
                    remapped
                } else {
                    stops
                        .iter()
                        .map(|&(pos, col)| GradientStop::new(pos, col.with_alpha_factor(alpha)))
                        .collect()
                };
                if let Some(shader) = RadialGradient::new(
                    Point::from_xy(*x0 as f32, *y0 as f32),
                    Point::from_xy(*x1 as f32, *y1 as f32),
                    *r1 as f32,
                    gstops,
                    SpreadMode::Pad,
                    Transform::identity(),
                ) {
                    paint.shader = shader;
                } else {
                    paint.set_color(stops.last().unwrap().1.with_alpha_factor(alpha));
                }
            }
            PaintStyle::Pattern { pixmap, spread } => {
                let quality = if self.state.smoothing_enabled {
                    self.state.smoothing_quality
                } else {
                    FilterQuality::Nearest
                };
                paint.shader = Pattern::new(
                    (**pixmap).as_ref(),
                    *spread,
                    quality,
                    alpha,
                    Transform::identity(),
                );
            }
        }

        Some(paint)
    }

    pub fn fill_path(&mut self, path: &Path) {
        self.fill_path_rule(path, FillRule::Winding);
    }

    pub fn fill_path_evenodd(&mut self, path: &Path) {
        self.fill_path_rule(path, FillRule::EvenOdd);
    }

    pub fn fill_path_rule(&mut self, path: &Path, rule: FillRule) {
        let style = self.state.fill_style.clone();
        if let Some(paint) = self.build_paint(&style) {
            let ts = to_ts_transform(self.state.ctm);
            let mask = self.state.clip.as_deref();
            self.pixmap.fill_path(path, &paint, rule, ts, mask);
        }
    }

    pub fn stroke_path(&mut self, path: &Path) {
        if self.state.line_width <= 0.0 {
            return;
        }
        let style = self.state.stroke_style.clone();
        let Some(paint) = self.build_paint(&style) else {
            return;
        };

        let mut stroke = Stroke {
            width: self.state.line_width as f32,
            miter_limit: self.state.miter_limit,
            line_cap: self.state.line_cap,
            line_join: self.state.line_join,
            dash: None,
        };
        if let Some(ref segs) = self.state.line_dash {
            stroke.dash = StrokeDash::new(segs.clone(), self.state.line_dash_offset);
        }

        let m = self.state.ctm;
        let ts = to_ts_transform(m);
        let mask = self.state.clip.as_deref();

        // Check whether CTM is a uniform similarity transform (no skew or anisotropic scale).
        let s1 = m[0] * m[0] + m[1] * m[1];
        let s2 = m[2] * m[2] + m[3] * m[3];
        let dot = m[0] * m[2] + m[1] * m[3];
        let det = m[0] * m[3] - m[1] * m[2];
        if det.abs() < 1e-10 {
            return;
        }
        let uniform = (s1 - s2).abs() <= 1e-4 * s1.max(s2).max(1.0)
            && dot.abs() <= 1e-4 * s1.max(s2).max(1.0)
            && det > 0.0;

        if uniform {
            self.pixmap.stroke_path(path, &paint, &stroke, ts, mask);
        } else {
            // Match HTML5 Canvas 2D: stroke in user coordinates, then transform outline by CTM.
            let max_scale = s1.max(s2).sqrt().max(1.0) as f32;
            if let Some(stroked) = path.stroke(&stroke, max_scale) {
                self.pixmap
                    .fill_path(&stroked, &paint, FillRule::Winding, ts, mask);
            }
        }
    }

    pub fn fill_rect(&mut self, x: f64, y: f64, w: f64, h: f64) {
        if w == 0.0 || h == 0.0 {
            return;
        }
        let style = self.state.fill_style.clone();
        let Some(paint) = self.build_paint(&style) else {
            return;
        };
        let (rx, rw) = if w >= 0.0 { (x, w) } else { (x + w, -w) };
        let (ry, rh) = if h >= 0.0 { (y, h) } else { (y + h, -h) };
        if let Some(rect) = Rect::from_xywh(rx as f32, ry as f32, rw as f32, rh as f32) {
            let ts = to_ts_transform(self.state.ctm);
            let mask = self.state.clip.as_deref();
            self.pixmap.fill_rect(rect, &paint, ts, mask);
        }
    }

    pub fn clear_rect(&mut self, x: f64, y: f64, w: f64, h: f64) {
        if w == 0.0 || h == 0.0 {
            return;
        }
        let (rx, rw) = if w >= 0.0 { (x, w) } else { (x + w, -w) };
        let (ry, rh) = if h >= 0.0 { (y, h) } else { (y + h, -h) };
        if let Some(rect) = Rect::from_xywh(rx as f32, ry as f32, rw as f32, rh as f32) {
            let paint = Paint {
                blend_mode: BlendMode::Clear,
                anti_alias: false,
                ..Paint::default()
            };
            let ts = to_ts_transform(self.state.ctm);
            let mask = self.state.clip.as_deref();
            self.pixmap.fill_rect(rect, &paint, ts, mask);
        }
    }

    #[inline]
    pub fn can_fast_blit_identity(&self) -> bool {
        self.state.clip.is_none()
            && self.state.blend_mode == BlendMode::SourceOver
            && (self.state.global_alpha - 1.0).abs() < 1e-6
            && (self.state.ctm[0] - 1.0).abs() < 1e-6
            && self.state.ctm[1].abs() < 1e-6
            && self.state.ctm[2].abs() < 1e-6
            && (self.state.ctm[3] - 1.0).abs() < 1e-6
            && self.state.ctm[4].abs() < 1e-6
            && self.state.ctm[5].abs() < 1e-6
    }

    /// Equivalent to `ctx.drawImage(src, dx, dy)`.
    pub fn draw_image_at(&mut self, src: &Pixmap, dx: f64, dy: f64) {
        let sw = src.width() as f64;
        let sh = src.height() as f64;
        self.draw_image_sub(src, 0.0, 0.0, sw, sh, dx, dy, sw, sh);
    }

    /// Equivalent to `ctx.drawImage(src, dx, dy, dw, dh)`.
    pub fn draw_image(&mut self, src: &Pixmap, dx: f64, dy: f64, dw: f64, dh: f64) {
        let sw = src.width() as f64;
        let sh = src.height() as f64;
        self.draw_image_sub(src, 0.0, 0.0, sw, sh, dx, dy, dw, dh);
    }

    /// Equivalent to `ctx.drawImage(src, sx, sy, sw, sh, dx, dy, dw, dh)`.
    #[allow(clippy::too_many_arguments)]
    pub fn draw_image_sub(
        &mut self,
        src: &Pixmap,
        sx: f64,
        sy: f64,
        sw: f64,
        sh: f64,
        dx: f64,
        dy: f64,
        dw: f64,
        dh: f64,
    ) {
        if sw.abs() < 1e-9 || sh.abs() < 1e-9 || dw.abs() < 1e-9 || dh.abs() < 1e-9 {
            return;
        }
        let alpha = self.state.global_alpha;
        if alpha <= 0.0
            && !matches!(
                self.state.blend_mode,
                BlendMode::Clear
                    | BlendMode::Source
                    | BlendMode::DestinationIn
                    | BlendMode::DestinationOut
            )
        {
            return;
        }
        let quality = if self.state.smoothing_enabled {
            self.state.smoothing_quality
        } else {
            FilterQuality::Nearest
        };

        let local = [dw / sw, 0.0, 0.0, dh / sh, dx, dy];
        let full = mul_affine(self.state.ctm, local);
        let det = full[0] * full[3] - full[1] * full[2];
        if det.abs() < 1e-10 {
            return;
        }

        let Some(rect) = Rect::from_xywh(0.0, 0.0, sw.abs() as f32, sh.abs() as f32) else {
            return;
        };
        let pat_transform = Transform::from_translate(-sx as f32, -sy as f32);
        let paint = Paint {
            blend_mode: self.state.blend_mode,
            anti_alias: true,
            shader: Pattern::new(src.as_ref(), SpreadMode::Pad, quality, alpha, pat_transform),
            ..Paint::default()
        };
        let ts = to_ts_transform(full);
        let mask = self.state.clip.as_deref();
        self.pixmap.fill_rect(rect, &paint, ts, mask);
    }
}

/// Upload straight (unpremultiplied) RGBA bytes `[r, g, b, a, ...]` into a [`Pixmap`].
pub fn upload_straight_rgba(dst: &mut Pixmap, src_rgba: &[u8]) {
    let dst_bytes = dst.data_mut();
    let len = dst_bytes.len().min(src_rgba.len());
    for (d, s) in dst_bytes[..len]
        .chunks_exact_mut(4)
        .zip(src_rgba[..len].chunks_exact(4))
    {
        let a = s[3] as u32;
        if a == 0 {
            d.copy_from_slice(&[0, 0, 0, 0]);
        } else if a == 255 {
            d.copy_from_slice(s);
        } else {
            d[0] = ((s[0] as u32 * a + 127) / 255) as u8;
            d[1] = ((s[1] as u32 * a + 127) / 255) as u8;
            d[2] = ((s[2] as u32 * a + 127) / 255) as u8;
            d[3] = a as u8;
        }
    }
}

/// Create a new [`Pixmap`] from straight (unpremultiplied) RGBA bytes.
pub fn pixmap_from_straight_rgba(width: u32, height: u32, src_rgba: &[u8]) -> Pixmap {
    let mut pm = Pixmap::new(width.max(1), height.max(1)).expect("valid pixmap size");
    upload_straight_rgba(&mut pm, src_rgba);
    pm
}

/// Copy device sub-rectangle `[0..bw, 0..bh]` from `src` to `dst` (`copy` blend mode).
pub fn copy_subrect(dst: &mut Pixmap, src: &Pixmap, bw: u32, bh: u32) {
    let w = bw.min(dst.width()).min(src.width()) as usize;
    let h = bh.min(dst.height()).min(src.height()) as usize;
    let dst_stride = dst.width() as usize * 4;
    let src_stride = src.width() as usize * 4;
    let row_bytes = w * 4;
    let d_data = dst.data_mut();
    let s_data = src.data();
    for y in 0..h {
        let doff = y * dst_stride;
        let soff = y * src_stride;
        d_data[doff..doff + row_bytes].copy_from_slice(&s_data[soff..soff + row_bytes]);
    }
}

/// Unbounded Canvas 2D `destination-atop` over `[0..bw, 0..bh]`:
/// `out = dst * src.a + src * (1 - dst.a)` (all channels premultiplied).
/// Also clears pixels where `src.a == 0`!
pub fn composite_destination_atop_subrect(dst: &mut Pixmap, src: &Pixmap, bw: u32, bh: u32) {
    let w = bw.min(dst.width()).min(src.width()) as usize;
    let h = bh.min(dst.height()).min(src.height()) as usize;
    let dst_stride = dst.width() as usize * 4;
    let src_stride = src.width() as usize * 4;
    let row_bytes = w * 4;
    let d_data = dst.data_mut();
    let s_data = src.data();
    for y in 0..h {
        let d_row = &mut d_data[y * dst_stride..y * dst_stride + row_bytes];
        let s_row = &s_data[y * src_stride..y * src_stride + row_bytes];
        for (d, s) in d_row.chunks_exact_mut(4).zip(s_row.chunks_exact(4)) {
            let sa = s[3] as u32;
            let inv_da = 255 - (d[3] as u32);
            if sa == 0 {
                d.copy_from_slice(&[0, 0, 0, 0]);
            } else {
                d[0] = ((d[0] as u32 * sa + s[0] as u32 * inv_da + 127) / 255) as u8;
                d[1] = ((d[1] as u32 * sa + s[1] as u32 * inv_da + 127) / 255) as u8;
                d[2] = ((d[2] as u32 * sa + s[2] as u32 * inv_da + 127) / 255) as u8;
                d[3] = sa as u8;
            }
        }
    }
}

/// Exact cross-fade `dst = (1 - t) * dst + t * src` over `[0..bw, 0..bh]`
/// (equivalent to `destination-in(1 - t)` followed by `lighter(t * src)` without quantization loss).
pub fn lerp_subrect(dst: &mut Pixmap, src: &Pixmap, bw: u32, bh: u32, t: f64) {
    let t = t.clamp(0.0, 1.0);
    if t <= 0.0 {
        return;
    }
    if t >= 1.0 {
        copy_subrect(dst, src, bw, bh);
        return;
    }
    let w_src = (t * 256.0).round() as u32;
    let w_dst = 256 - w_src;
    let w = bw.min(dst.width()).min(src.width()) as usize;
    let h = bh.min(dst.height()).min(src.height()) as usize;
    let dst_stride = dst.width() as usize * 4;
    let src_stride = src.width() as usize * 4;
    let row_bytes = w * 4;
    let d_data = dst.data_mut();
    let s_data = src.data();
    for y in 0..h {
        let d_row = &mut d_data[y * dst_stride..y * dst_stride + row_bytes];
        let s_row = &s_data[y * src_stride..y * src_stride + row_bytes];
        for (d, s) in d_row.chunks_exact_mut(4).zip(s_row.chunks_exact(4)) {
            d[0] = ((d[0] as u32 * w_dst + s[0] as u32 * w_src + 128) >> 8).min(255) as u8;
            d[1] = ((d[1] as u32 * w_dst + s[1] as u32 * w_src + 128) >> 8).min(255) as u8;
            d[2] = ((d[2] as u32 * w_dst + s[2] as u32 * w_src + 128) >> 8).min(255) as u8;
            d[3] = ((d[3] as u32 * w_dst + s[3] as u32 * w_src + 128) >> 8).min(255) as u8;
        }
    }
}

/// Source-over composite `src` (scaled by `alpha`) onto `dst` over `[0..bw, 0..bh]` at 1:1 device pixels.
pub fn composite_source_over_subrect(dst: &mut Pixmap, src: &Pixmap, bw: u32, bh: u32, alpha: f64) {
    let a_scale = (alpha.clamp(0.0, 1.0) * 255.0).round() as u32;
    if a_scale == 0 {
        return;
    }
    let w = bw.min(dst.width()).min(src.width()) as usize;
    let h = bh.min(dst.height()).min(src.height()) as usize;
    let dst_stride = dst.width() as usize * 4;
    let src_stride = src.width() as usize * 4;
    let row_bytes = w * 4;
    let d_data = dst.data_mut();
    let s_data = src.data();
    for y in 0..h {
        let d_row = &mut d_data[y * dst_stride..y * dst_stride + row_bytes];
        let s_row = &s_data[y * src_stride..y * src_stride + row_bytes];
        for (d, s) in d_row.chunks_exact_mut(4).zip(s_row.chunks_exact(4)) {
            if s[3] == 0 {
                continue;
            }
            let sr = (s[0] as u32 * a_scale + 127) / 255;
            let sg = (s[1] as u32 * a_scale + 127) / 255;
            let sb = (s[2] as u32 * a_scale + 127) / 255;
            let sa = (s[3] as u32 * a_scale + 127) / 255;
            let inv_sa = 255 - sa;
            d[0] = (sr + (d[0] as u32 * inv_sa + 127) / 255).min(255) as u8;
            d[1] = (sg + (d[1] as u32 * inv_sa + 127) / 255).min(255) as u8;
            d[2] = (sb + (d[2] as u32 * inv_sa + 127) / 255).min(255) as u8;
            d[3] = (sa + (d[3] as u32 * inv_sa + 127) / 255).min(255) as u8;
        }
    }
}

/// Integer-aligned 1:1 premultiplied `source-over` blit of `src[0..bw, 0..bh]` onto `dst` at `(ox, oy)`.
pub fn blit_source_over_at(dst: &mut Pixmap, src: &Pixmap, bw: u32, bh: u32, ox: i32, oy: i32) {
    let sw = (bw.min(src.width())) as i32;
    let sh = (bh.min(src.height())) as i32;
    let dw = dst.width() as i32;
    let dh = dst.height() as i32;

    let x_start = 0.max(-ox);
    let y_start = 0.max(-oy);
    let x_end = sw.min(dw - ox);
    let y_end = sh.min(dh - oy);
    if x_start >= x_end || y_start >= y_end {
        return;
    }

    let copy_w = (x_end - x_start) as usize;
    let dst_stride = dw as usize * 4;
    let src_stride = src.width() as usize * 4;
    let d_data = dst.data_mut();
    let s_data = src.data();

    for sy in y_start..y_end {
        let dy = (sy + oy) as usize;
        let dx0 = (x_start + ox) as usize;
        let sx0 = x_start as usize;
        let d_row = &mut d_data[dy * dst_stride + dx0 * 4..dy * dst_stride + (dx0 + copy_w) * 4];
        let s_row = &s_data
            [sy as usize * src_stride + sx0 * 4..sy as usize * src_stride + (sx0 + copy_w) * 4];
        for (d, s) in d_row.chunks_exact_mut(4).zip(s_row.chunks_exact(4)) {
            let sa = s[3] as u32;
            if sa == 0 {
                continue;
            } else if sa == 255 {
                d.copy_from_slice(s);
            } else {
                let inv_sa = 255 - sa;
                d[0] = (s[0] as u32 + (d[0] as u32 * inv_sa + 127) / 255).min(255) as u8;
                d[1] = (s[1] as u32 + (d[1] as u32 * inv_sa + 127) / 255).min(255) as u8;
                d[2] = (s[2] as u32 + (d[2] as u32 * inv_sa + 127) / 255).min(255) as u8;
                d[3] = (sa + (d[3] as u32 * inv_sa + 127) / 255).min(255) as u8;
            }
        }
    }
}

/// Build a rectangle [`Path`].
pub fn rect_path(x: f64, y: f64, w: f64, h: f64) -> Option<Path> {
    let (rx, rw) = if w >= 0.0 { (x, w) } else { (x + w, -w) };
    let (ry, rh) = if h >= 0.0 { (y, h) } else { (y + h, -h) };
    let rect = Rect::from_xywh(rx as f32, ry as f32, rw as f32, rh as f32)?;
    Some(PathBuilder::from_rect(rect))
}

/// Build a full circle [`Path`].
pub fn circle_path(cx: f64, cy: f64, r: f64) -> Option<Path> {
    if r <= 0.0 {
        return None;
    }
    PathBuilder::from_circle(cx as f32, cy as f32, r as f32)
}

/// Append a Canvas 2D `ellipse(cx, cy, rx, ry, rotation, start_angle, end_angle, anticlockwise)`
/// segment to `pb`. If `move_first` is true, starts with `move_to`; otherwise `line_to`.
#[allow(clippy::too_many_arguments)]
pub fn push_ellipse_arc(
    pb: &mut PathBuilder,
    cx: f64,
    cy: f64,
    rx: f64,
    ry: f64,
    rotation: f64,
    start_angle: f64,
    end_angle: f64,
    anticlockwise: bool,
    move_first: bool,
) {
    let rx = rx.abs();
    let ry = ry.abs();
    let (sin_rot, cos_rot) = rotation.sin_cos();

    let map_pt = |theta: f64| -> (f32, f32) {
        let (st, ct) = theta.sin_cos();
        let ex = rx * ct;
        let ey = ry * st;
        (
            (cos_rot * ex - sin_rot * ey + cx) as f32,
            (sin_rot * ex + cos_rot * ey + cy) as f32,
        )
    };

    // Compute signed sweep matching HTML5 Canvas 2D arc/ellipse specification.
    let mut dtheta = end_angle - start_angle;
    if !anticlockwise {
        if dtheta >= TAU {
            dtheta = TAU;
        } else {
            while dtheta < 0.0 {
                dtheta += TAU;
            }
        }
    } else if dtheta <= -TAU {
        dtheta = -TAU;
    } else {
        while dtheta > 0.0 {
            dtheta -= TAU;
        }
    }

    let (sx, sy) = map_pt(start_angle);
    if move_first {
        pb.move_to(sx, sy);
    } else {
        pb.line_to(sx, sy);
    }

    if dtheta.abs() < 1e-9 || rx < 1e-9 || ry < 1e-9 {
        return;
    }

    let segs = ((dtheta.abs() / (std::f64::consts::FRAC_PI_2 + 1e-9)).ceil() as usize).max(1);
    let delta = dtheta / (segs as f64);
    let k = (4.0 / 3.0) * (delta * 0.25).tan();

    let map_vec = |ux: f64, uy: f64| -> (f32, f32) {
        let ex = rx * ux;
        let ey = ry * uy;
        (
            (cos_rot * ex - sin_rot * ey + cx) as f32,
            (sin_rot * ex + cos_rot * ey + cy) as f32,
        )
    };

    for i in 0..segs {
        let a0 = start_angle + (i as f64) * delta;
        let a1 = a0 + delta;
        let (s0, c0) = a0.sin_cos();
        let (s1, c1) = a1.sin_cos();
        let (c1x, c1y) = map_vec(c0 - k * s0, s0 + k * c0);
        let (c2x, c2y) = map_vec(c1 + k * s1, s1 - k * c1);
        let (ex, ey) = map_vec(c1, s1);
        pb.cubic_to(c1x, c1y, c2x, c2y, ex, ey);
    }
}

/// Build an open or closed circular arc [`Path`] (`ctx.arc(...)`).
pub fn arc_path(
    cx: f64,
    cy: f64,
    r: f64,
    start_angle: f64,
    end_angle: f64,
    anticlockwise: bool,
    close: bool,
) -> Option<Path> {
    if r <= 0.0 {
        return None;
    }
    let mut pb = PathBuilder::new();
    push_ellipse_arc(
        &mut pb,
        cx,
        cy,
        r,
        r,
        0.0,
        start_angle,
        end_angle,
        anticlockwise,
        true,
    );
    if close {
        pb.close();
    }
    pb.finish()
}

/// Build an ellipse [`Path`] (`ctx.ellipse(...)`).
pub fn ellipse_path(
    cx: f64,
    cy: f64,
    rx: f64,
    ry: f64,
    rotation: f64,
    start_angle: f64,
    end_angle: f64,
    anticlockwise: bool,
    close: bool,
) -> Option<Path> {
    if rx <= 0.0 || ry <= 0.0 {
        return None;
    }
    let mut pb = PathBuilder::new();
    push_ellipse_arc(
        &mut pb,
        cx,
        cy,
        rx,
        ry,
        rotation,
        start_angle,
        end_angle,
        anticlockwise,
        true,
    );
    if close {
        pb.close();
    }
    pb.finish()
}

/// Unused-warning suppressor for `PixmapMut` re-export convenience.
pub type CanvasPixmapMut<'a> = PixmapMut<'a>;
