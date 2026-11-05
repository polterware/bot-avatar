//! Glyphs: small signs the body folds into while it waits, thinks, or sends. The body
//! shrinks away with a half turn and the glyph grows in its place; going back, the glyph
//! shrinks and the body returns turning the other way.
//!
//! Glyphs are drawn in the body's own colour as soft cushioned balls and strokes, lit from
//! the same side as the fabric, so they read as made of the same stuff.

use std::f64::consts::{PI, TAU};

use tiny_skia::{LineCap, Path, PathBuilder};

use crate::canvas::{Canvas2D, PaintStyle};
use crate::color::shade;
use crate::ease::{Bezier, Spring, bump, smoothstep};

/// A sign the body can fold into.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Glyph {
    /// Three dots bouncing in turn.
    Dots,
    /// A ball with rings sweeping out.
    Radar,
    /// A ball orbited by two moons.
    Orbit,
    /// A ring that fills.
    Progress,
    /// Little balls that gather and pop into one.
    Gather,
    /// A row of dots waving like a voice.
    Wave,
    /// A ball that shoots off up and to the right.
    Send,
    /// A ball that drops into a tray.
    Receive,
    /// A ball that rises into a dock.
    Dock,
    /// A ball bouncing on its shadow.
    Ball,
    /// A pencil scribbling a line.
    Pencil,
    /// A bouncing exclamation mark.
    Bang,
    /// A power sign, dimming.
    Standby,
    /// A map pin that drops and pulses.
    Pin,
}

impl Glyph {
    pub const ALL: [Glyph; 14] = [
        Glyph::Dots,
        Glyph::Radar,
        Glyph::Orbit,
        Glyph::Progress,
        Glyph::Gather,
        Glyph::Wave,
        Glyph::Send,
        Glyph::Receive,
        Glyph::Dock,
        Glyph::Ball,
        Glyph::Pencil,
        Glyph::Bang,
        Glyph::Standby,
        Glyph::Pin,
    ];

    /// For glyphs that come back to the body now and then: how long they hold, and how
    /// long the body shows before they return.
    const fn breath(self) -> Option<(f64, f64)> {
        match self {
            Glyph::Progress => Some((2.8, 1.4)),
            Glyph::Gather => Some((2.2, 1.4)),
            _ => None,
        }
    }
}

const AMOUNT_OMEGA: f64 = 13.0;
const SWAP_OMEGA: f64 = 11.0;
const SPIN_OMEGA: f64 = 13.0;

/// The glyph over time: how far the body has folded into it, which one, and the turn.
#[derive(Debug, Clone)]
pub struct GlyphAnim {
    target: Option<Glyph>,
    shown: Option<Glyph>,
    outgoing: Option<Glyph>,
    amount: Spring,
    swap: Spring,
    spin: Spring,
    turn_dir: f64,
    folded: bool,
    clock: f64,
    out_clock: f64,
    since: f64,
    breathing_out: bool,
    flip: u32,
}

/// One frame of a glyph.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GlyphFrame {
    /// 0 the body, 1 fully the glyph.
    pub amount: f64,
    /// Extra yaw on the body, radians, as it folds.
    pub spin: f64,
    pub glyph: Option<Glyph>,
    pub clock: f64,
    pub outgoing: Option<Glyph>,
    pub out_clock: f64,
    /// 0 all outgoing, 1 all the current glyph.
    pub swap: f64,
}

impl GlyphFrame {
    pub const NONE: GlyphFrame = GlyphFrame {
        amount: 0.0,
        spin: 0.0,
        glyph: None,
        clock: 0.0,
        outgoing: None,
        out_clock: 0.0,
        swap: 1.0,
    };
}

impl Default for GlyphAnim {
    fn default() -> Self {
        Self::new()
    }
}

impl GlyphAnim {
    pub const fn new() -> Self {
        Self {
            target: None,
            shown: None,
            outgoing: None,
            amount: Spring::at(0.0),
            swap: Spring::at(1.0),
            spin: Spring::at(0.0),
            turn_dir: 1.0,
            folded: false,
            clock: 0.0,
            out_clock: 0.0,
            since: 0.0,
            breathing_out: false,
            flip: 0,
        }
    }

    pub fn target(&self) -> Option<Glyph> {
        self.target
    }

    /// Folds into `glyph`, or back into the body with `None`. `coin` (0 to 1) picks the
    /// direction of the half turn.
    pub fn set(&mut self, glyph: Option<Glyph>, coin: f64) {
        if glyph == self.target {
            return;
        }
        self.target = glyph;
        self.since = 0.0;
        self.breathing_out = false;
        if let Some(g) = glyph {
            if self.shown.is_some() && self.amount.x > 0.02 {
                self.outgoing = self.shown;
                self.out_clock = self.clock;
                self.swap.x = 0.0;
                self.swap.v = 0.0;
            } else {
                self.outgoing = None;
                self.swap.x = 1.0;
            }
            self.swap.target = 1.0;
            if self.shown != Some(g) {
                self.clock = 0.0;
            }
            self.shown = Some(g);
        }
        self.turn_dir = if coin < 0.5 { 1.0 } else { -1.0 };
    }

    /// Jumps to the end of any change.
    pub fn settle(&mut self) {
        self.amount.target = if self.target.is_some() { 1.0 } else { 0.0 };
        self.update_fold();
        self.amount.settle();
        self.swap.settle();
        self.spin.settle();
        self.outgoing = None;
    }

    fn update_fold(&mut self) {
        let want = self.target.is_some() && !self.breathing_out;
        if want != self.folded {
            self.folded = want;
            self.flip += 1;
            self.spin.target += PI * self.turn_dir;
        }
        self.amount.target = if want { 1.0 } else { 0.0 };
    }

    pub fn update(&mut self, dt: f64) {
        self.since += dt;
        if let Some((hold, rest)) = self.target.and_then(Glyph::breath) {
            if !self.breathing_out && self.since > hold {
                self.breathing_out = true;
                self.since = 0.0;
            } else if self.breathing_out && self.since > rest {
                self.breathing_out = false;
                self.since = 0.0;
                self.clock = 0.0;
            }
        }
        self.update_fold();
        let steps = (dt / (1.0 / 240.0)).ceil().max(1.0) as u32;
        let h = dt / f64::from(steps);
        for _ in 0..steps {
            self.amount.step(AMOUNT_OMEGA, h);
            self.swap.step(SWAP_OMEGA, h);
            self.spin.step(SPIN_OMEGA, h);
        }
        if self.swap.x > 0.996 {
            self.outgoing = None;
        }
        if self.target.is_none() && self.amount.x < 0.004 {
            self.shown = None;
            self.outgoing = None;
        }
        self.clock += dt;
        self.out_clock += dt;
    }

    pub fn is_settled(&self) -> bool {
        self.target.is_none() && self.amount.is_settled() && self.spin.is_settled()
    }

    pub fn frame(&self) -> GlyphFrame {
        GlyphFrame {
            amount: self.amount.x.clamp(0.0, 1.0),
            spin: self.spin.x,
            glyph: self.shown,
            clock: self.clock,
            outgoing: self.outgoing,
            out_clock: self.out_clock,
            swap: self.swap.x.clamp(0.0, 1.0),
        }
    }
}

/// The colours a glyph is made of, from the body colour.
pub struct GlyphPaint {
    light: String,
    base: String,
    dark: String,
    deep: String,
    ink: String,
}

impl GlyphPaint {
    pub fn new(color: &str, ink: &str) -> Self {
        Self {
            light: shade(color, 0.16, 0.02),
            base: color.to_string(),
            dark: shade(color, -0.16, 0.04),
            deep: shade(color, -0.3, 0.06),
            ink: ink.to_string(),
        }
    }
}

/// Draws `frame` with the rig box `box_px` wide, centred where the body sits. The context
/// is already scaled by the device pixel ratio.
pub fn draw_glyph(
    ctx: &mut Canvas2D,
    full: f64,
    center_y: f64,
    box_px: f64,
    frame: &GlyphFrame,
    paint: &GlyphPaint,
) {
    if frame.amount <= 0.002 {
        return;
    }
    let s = box_px / 100.0;
    let grow = Bezier::OVERSHOOT.at(smoothstep((frame.amount - 0.25) / 0.75));
    if grow <= 0.001 {
        return;
    }
    ctx.save();
    ctx.translate(full / 2.0, center_y);
    ctx.scale(s, s);
    ctx.set_line_cap(LineCap::Round);
    if let Some(out) = frame.outgoing {
        let k = 1.0 - frame.swap;
        if k > 0.002 {
            ctx.save();
            ctx.scale(grow * k, grow * k);
            draw_one(ctx, out, frame.out_clock, paint);
            ctx.restore();
        }
    }
    if let Some(glyph) = frame.glyph {
        let k = if frame.outgoing.is_some() {
            frame.swap
        } else {
            1.0
        };
        ctx.save();
        ctx.scale(grow * k, grow * k);
        draw_one(ctx, glyph, frame.clock, paint);
        ctx.restore();
    }
    ctx.restore();
}

fn draw_one(ctx: &mut Canvas2D, glyph: Glyph, t: f64, p: &GlyphPaint) {
    match glyph {
        Glyph::Dots => {
            for i in 0..3 {
                let ph = ((t / 1.1) - f64::from(i) * 0.16).rem_euclid(1.0);
                let b = bump(ph, 0.0, 0.5);
                let x = (f64::from(i) - 1.0) * 19.0;
                ball(ctx, x, -7.0 * b, 7.2 * (1.0 + 0.14 * b), p);
            }
        }
        Glyph::Radar => {
            for i in 0..2 {
                let q = ((t / 1.8) + f64::from(i) * 0.5).rem_euclid(1.0);
                let r = 9.0 + 27.0 * Bezier::OUT.at(q);
                ctx.set_global_alpha((1.0 - q).powf(1.4) * 0.9);
                ring(ctx, 0.0, 0.0, r, 3.0 - 1.6 * q, &p.base);
            }
            ctx.set_global_alpha(0.35);
            ring(ctx, 0.0, 0.0, 30.0, 1.2, &p.dark);
            ctx.set_global_alpha(1.0);
            let a = t * TAU / 1.8;
            arc_stroke(ctx, (0.0, 0.0, 30.0), a - 0.9, a, 3.2, &p.light);
            let pulse = 1.0 + 0.08 * (t * TAU / 0.9).sin();
            ball(ctx, 0.0, 0.0, 9.0 * pulse, p);
        }
        Glyph::Orbit => {
            let moons = |ctx: &mut Canvas2D, behind: bool| {
                for i in 0..2 {
                    let a = t * TAU / 2.2 + f64::from(i) * PI;
                    let z = a.sin();
                    if (z < 0.0) != behind {
                        continue;
                    }
                    let (x, y) = (31.0 * a.cos(), 9.0 * z);
                    let (cx, cy) = rot(x, y, -0.26);
                    ball(ctx, cx, cy, 4.6 * (1.0 + 0.22 * z), p);
                }
            };
            ctx.set_global_alpha(0.3);
            ellipse_stroke(ctx, 31.0, 9.0, -0.26, 1.2, &p.dark);
            ctx.set_global_alpha(1.0);
            moons(ctx, true);
            ball(ctx, 0.0, 0.0, 11.0, p);
            moons(ctx, false);
        }
        Glyph::Progress => {
            ctx.set_global_alpha(0.32);
            ring(ctx, 0.0, 0.0, 24.0, 6.5, &p.deep);
            ctx.set_global_alpha(1.0);
            let fill = Bezier::IN_OUT.at((t / 2.6).clamp(0.0, 1.0));
            let a0 = -PI / 2.0;
            let a1 = a0 + TAU * fill.max(0.02);
            arc_stroke(ctx, (0.0, 0.0, 24.0), a0, a1, 6.5, &p.base);
            let (hx, hy) = (24.0 * a1.cos(), 24.0 * a1.sin());
            ball(ctx, hx, hy, 4.4, p);
            ball(ctx, 0.0, 0.0, 7.0 + 1.6 * fill, p);
        }
        Glyph::Gather => {
            let q = (t / 2.2).clamp(0.0, 1.0);
            let pull = Bezier::IN.at((q / 0.62).min(1.0));
            let n = 6;
            if q < 0.62 {
                for i in 0..n {
                    let a = f64::from(i) * TAU / f64::from(n) + 0.8 * pull + 0.3;
                    let r = 32.0 * (1.0 - pull);
                    ball(ctx, r * a.cos(), r * a.sin(), 5.0 * (1.0 - 0.3 * pull), p);
                }
            }
            let pop = if q < 0.62 {
                6.0 * pull
            } else {
                12.0 * Bezier::OVERSHOOT.at(((q - 0.62) / 0.3).min(1.0))
            };
            if pop > 0.1 {
                ball(ctx, 0.0, 0.0, pop.max(3.0), p);
            }
        }
        Glyph::Wave => {
            for i in 0..5 {
                let fi = f64::from(i);
                let amp = 0.45 + 0.55 * ((t * 3.1 + fi * 1.7).sin() * 0.5 + 0.5);
                let h = 12.0 * amp * (t * TAU * 1.3 - fi * 0.7).sin().abs();
                let x = (fi - 2.0) * 12.0;
                capsule(ctx, x, -h / 2.0 - 1.0, x, h / 2.0 + 1.0, 8.4, p);
            }
        }
        Glyph::Send => {
            let q = (t / 1.4).rem_euclid(1.0);
            let fly = Bezier::IN.at((q / 0.62).min(1.0));
            if q < 0.62 {
                for k in (1..4).rev() {
                    let lag = (fly - f64::from(k) * 0.07).max(0.0);
                    ctx.set_global_alpha(0.22 * (1.0 - f64::from(k) / 4.0) * (1.0 - fly * 0.5));
                    ball(ctx, 30.0 * lag, -30.0 * lag, 8.0 * (1.0 - 0.6 * lag), p);
                }
                ctx.set_global_alpha(1.0 - smoothstep((fly - 0.8) / 0.2));
                ball(ctx, 30.0 * fly, -30.0 * fly, 8.0 * (1.0 - 0.6 * fly), p);
                ctx.set_global_alpha(1.0);
            } else {
                let g = Bezier::OVERSHOOT.at(((q - 0.62) / 0.3).min(1.0));
                ball(ctx, 0.0, 0.0, 8.0 * g, p);
            }
        }
        Glyph::Receive => {
            ctx.set_global_alpha(0.9);
            tray(ctx, 0.0, 20.0, 40.0, &p.dark);
            ctx.set_global_alpha(1.0);
            let q = (t / 1.5).rem_euclid(1.0);
            let (y, sx, sy, a) = if q < 0.45 {
                let d = Bezier::IN.at(q / 0.45);
                (-34.0 + 46.0 * d, 1.0, 1.0, smoothstep(q / 0.12))
            } else if q < 0.62 {
                let k = bump((q - 0.45) / 0.17, 0.0, 1.0);
                (12.0 + 1.5 * k, 1.0 + 0.2 * k, 1.0 - 0.18 * k, 1.0)
            } else {
                (12.0, 1.0, 1.0, 1.0 - smoothstep((q - 0.82) / 0.18))
            };
            ctx.set_global_alpha(a);
            ctx.save();
            ctx.translate(0.0, y);
            ctx.scale(sx, sy);
            ball(ctx, 0.0, 0.0, 8.0, p);
            ctx.restore();
            ctx.set_global_alpha(1.0);
        }
        Glyph::Dock => {
            let q = (t / 1.4).rem_euclid(1.0);
            let rise = Bezier::IN_OUT.at((q / 0.6).min(1.0));
            let glow = bump(q, 0.55, 0.85);
            capsule(ctx, -17.0, -26.0, 17.0, -26.0, 6.0 + 1.5 * glow, p);
            let y = 22.0 - 42.0 * rise;
            let r = 8.0 * (1.0 - 0.5 * smoothstep((q - 0.5) / 0.12));
            if q < 0.62 {
                ball(ctx, 0.0, y, r, p);
            }
            ctx.set_global_alpha(0.5 * (1.0 - rise));
            chevron(ctx, 0.0, 12.0 - 10.0 * rise, 7.0, &p.light);
            ctx.set_global_alpha(1.0);
        }
        Glyph::Ball => {
            let q = (t / 0.9).rem_euclid(1.0);
            let h = (PI * q).sin();
            let y = 13.0 - 32.0 * h;
            let contact = 1.0 - smoothstep(h / 0.18);
            ctx.set_global_alpha(0.28 + 0.2 * contact);
            shadow(ctx, 0.0, 25.0, 13.0 - 5.0 * h, 3.2, &p.deep);
            ctx.set_global_alpha(1.0);
            ctx.save();
            ctx.translate(0.0, y + 11.0);
            ctx.scale(1.0 + 0.18 * contact, 1.0 - 0.16 * contact);
            ball(ctx, 0.0, -11.0, 11.0, p);
            ctx.restore();
        }
        Glyph::Pencil => {
            let q = (t / 2.0).rem_euclid(1.0);
            let wx = -18.0 + 36.0 * q;
            let wy = 12.0 + 3.0 * (q * TAU * 3.0).sin();
            ctx.set_global_alpha(0.7 * (1.0 - smoothstep((q - 0.85) / 0.15)));
            scribble(ctx, q, &p.dark);
            ctx.set_global_alpha(1.0);
            ctx.save();
            ctx.translate(wx, wy);
            ctx.rotate(0.62 + 0.1 * (q * TAU * 3.0).cos());
            pencil(ctx, p);
            ctx.restore();
        }
        Glyph::Bang => {
            let q = (t / 1.1).rem_euclid(1.0);
            let hop = bump(q, 0.0, 0.42);
            let land = bump(q, 0.38, 0.55);
            let tilt = 0.14 * (q * TAU * 2.0).sin() * (1.0 - q);
            ctx.save();
            ctx.translate(0.0, -10.0 * hop + 22.0);
            ctx.scale(1.0 + 0.1 * land, 1.0 - 0.1 * land);
            ctx.rotate(tilt);
            capsule(ctx, 0.0, -42.0, 0.0, -18.0, 11.0, p);
            ball(ctx, 0.0, -5.0, 6.2, p);
            ctx.restore();
        }
        Glyph::Standby => {
            // The ring dims and brightens like a sleeping light; the bar stays lit.
            let dim = 0.4 + 0.6 * (0.5 + 0.5 * (t * TAU / 2.4).cos());
            ctx.set_global_alpha(dim);
            arc_stroke(
                ctx,
                (0.0, 2.0, 20.0),
                -PI / 2.0 + 0.62,
                1.5 * PI - 0.62,
                7.0,
                &p.base,
            );
            ctx.set_global_alpha(1.0);
            capsule(ctx, 0.0, -24.0, 0.0, -4.0, 7.0, p);
        }
        Glyph::Pin => {
            let q = (t / 2.2).rem_euclid(1.0);
            let drop = Bezier::OVERSHOOT.at((q / 0.32).min(1.0));
            let y = -22.0 * (1.0 - drop);
            for k in 0..2 {
                let r = ((q - 0.3 - f64::from(k) * 0.18) / 0.5).clamp(0.0, 1.0);
                if r > 0.0 && r < 1.0 {
                    ctx.set_global_alpha(0.6 * (1.0 - r));
                    ellipse_at(ctx, 0.0, 24.0, 6.0 + 22.0 * r, 2.0 + 6.0 * r, 1.8, &p.base);
                }
            }
            ctx.set_global_alpha(0.3);
            shadow(ctx, 0.0, 24.0, 8.0 * (0.6 + 0.4 * drop), 2.5, &p.deep);
            ctx.set_global_alpha(1.0);
            pin(ctx, 0.0, y, p);
        }
    }
}

fn rot(x: f64, y: f64, a: f64) -> (f64, f64) {
    let (s, c) = a.sin_cos();
    (x * c - y * s, x * s + y * c)
}

/// A cushioned ball: lit from the upper left, darker toward the rim.
fn ball(ctx: &mut Canvas2D, x: f64, y: f64, r: f64, p: &GlyphPaint) {
    if r <= 0.05 {
        return;
    }
    let Some(path) = crate::canvas::circle_path(x, y, r) else {
        return;
    };
    ctx.set_fill_style(PaintStyle::radial_gradient(
        x - 0.38 * r,
        y - 0.42 * r,
        0.0,
        x,
        y,
        r,
        &[(0.0, &p.light), (0.55, &p.base), (1.0, &p.dark)],
    ));
    ctx.fill_path(&path);
}

/// A rounded bar from one point to another, shaded like a ball.
fn capsule(ctx: &mut Canvas2D, x0: f64, y0: f64, x1: f64, y1: f64, w: f64, p: &GlyphPaint) {
    let mut pb = PathBuilder::new();
    pb.move_to(x0 as f32, y0 as f32);
    pb.line_to(x1 as f32, (y1 + 1e-3) as f32);
    let Some(path) = pb.finish() else {
        return;
    };
    let (nx, ny) = {
        let (dx, dy) = (x1 - x0, y1 - y0);
        let l = dx.hypot(dy).max(1e-6);
        (-dy / l, dx / l)
    };
    let mx = (x0 + x1) / 2.0;
    let my = (y0 + y1) / 2.0;
    let (gx, gy) = if nx * -0.6 + ny * -0.8 >= 0.0 {
        (nx, ny)
    } else {
        (-nx, -ny)
    };
    ctx.set_stroke_style(PaintStyle::linear_gradient(
        mx + gx * w / 2.0,
        my + gy * w / 2.0,
        mx - gx * w / 2.0,
        my - gy * w / 2.0,
        &[(0.0, &p.light), (0.45, &p.base), (1.0, &p.dark)],
    ));
    ctx.set_line_width(w);
    ctx.stroke_path(&path);
}

fn arc_path(cx: f64, cy: f64, r: f64, a0: f64, a1: f64) -> Option<Path> {
    let steps = (((a1 - a0).abs() / (PI / 24.0)).ceil() as usize).max(2);
    let mut pb = PathBuilder::new();
    for i in 0..=steps {
        let a = a0 + (a1 - a0) * i as f64 / steps as f64;
        let (x, y) = (cx + r * a.cos(), cy + r * a.sin());
        if i == 0 {
            pb.move_to(x as f32, y as f32);
        } else {
            pb.line_to(x as f32, y as f32);
        }
    }
    pb.finish()
}

fn arc_stroke(
    ctx: &mut Canvas2D,
    (cx, cy, r): (f64, f64, f64),
    a0: f64,
    a1: f64,
    w: f64,
    color: &str,
) {
    if let Some(path) = arc_path(cx, cy, r, a0, a1) {
        ctx.set_stroke_css(color);
        ctx.set_line_width(w);
        ctx.stroke_path(&path);
    }
}

fn ring(ctx: &mut Canvas2D, cx: f64, cy: f64, r: f64, w: f64, color: &str) {
    arc_stroke(ctx, (cx, cy, r), 0.0, TAU, w, color);
}

fn ellipse_stroke(ctx: &mut Canvas2D, rx: f64, ry: f64, tilt: f64, w: f64, color: &str) {
    ctx.save();
    ctx.rotate(tilt);
    ellipse_at(ctx, 0.0, 0.0, rx, ry, w, color);
    ctx.restore();
}

fn ellipse_at(ctx: &mut Canvas2D, cx: f64, cy: f64, rx: f64, ry: f64, w: f64, color: &str) {
    let mut pb = PathBuilder::new();
    for i in 0..=48 {
        let a = f64::from(i) * TAU / 48.0;
        let (x, y) = (cx + rx * a.cos(), cy + ry * a.sin());
        if i == 0 {
            pb.move_to(x as f32, y as f32);
        } else {
            pb.line_to(x as f32, y as f32);
        }
    }
    if let Some(path) = pb.finish() {
        ctx.set_stroke_css(color);
        ctx.set_line_width(w);
        ctx.stroke_path(&path);
    }
}

fn shadow(ctx: &mut Canvas2D, cx: f64, cy: f64, rx: f64, ry: f64, color: &str) {
    let mut pb = PathBuilder::new();
    if let Some(rect) = tiny_skia::Rect::from_xywh(
        (cx - rx) as f32,
        (cy - ry) as f32,
        (2.0 * rx) as f32,
        (2.0 * ry) as f32,
    ) {
        pb.push_oval(rect);
    }
    if let Some(path) = pb.finish() {
        ctx.set_fill_css(color);
        ctx.fill_path(&path);
    }
}

fn tray(ctx: &mut Canvas2D, cx: f64, cy: f64, w: f64, color: &str) {
    let h = w / 2.0;
    let mut pb = PathBuilder::new();
    pb.move_to((cx - h) as f32, (cy - 8.0) as f32);
    pb.line_to((cx - h) as f32, cy as f32);
    pb.quad_to(
        (cx - h) as f32,
        (cy + 8.0) as f32,
        (cx - h + 8.0) as f32,
        (cy + 8.0) as f32,
    );
    pb.line_to((cx + h - 8.0) as f32, (cy + 8.0) as f32);
    pb.quad_to(
        (cx + h) as f32,
        (cy + 8.0) as f32,
        (cx + h) as f32,
        cy as f32,
    );
    pb.line_to((cx + h) as f32, (cy - 8.0) as f32);
    if let Some(path) = pb.finish() {
        ctx.set_stroke_css(color);
        ctx.set_line_width(5.0);
        ctx.stroke_path(&path);
    }
}

fn chevron(ctx: &mut Canvas2D, cx: f64, cy: f64, w: f64, color: &str) {
    let mut pb = PathBuilder::new();
    pb.move_to((cx - w) as f32, (cy + w * 0.6) as f32);
    pb.line_to(cx as f32, (cy - w * 0.4) as f32);
    pb.line_to((cx + w) as f32, (cy + w * 0.6) as f32);
    if let Some(path) = pb.finish() {
        ctx.set_stroke_css(color);
        ctx.set_line_width(3.2);
        ctx.stroke_path(&path);
    }
}

fn scribble(ctx: &mut Canvas2D, q: f64, color: &str) {
    let mut pb = PathBuilder::new();
    let n = ((q * 48.0).ceil() as usize).max(2);
    for i in 0..=n {
        let u = (i as f64 / 48.0).min(q);
        let x = -18.0 + 36.0 * u;
        let y = 12.0 + 3.0 * (u * TAU * 3.0).sin() + 6.0;
        if i == 0 {
            pb.move_to(x as f32, y as f32);
        } else {
            pb.line_to(x as f32, y as f32);
        }
    }
    if let Some(path) = pb.finish() {
        ctx.set_stroke_css(color);
        ctx.set_line_width(2.6);
        ctx.stroke_path(&path);
    }
}

/// A pencil pointing down its own +y, the tip at the origin.
fn pencil(ctx: &mut Canvas2D, p: &GlyphPaint) {
    capsule(ctx, 0.0, -40.0, 0.0, -14.0, 12.0, p);
    let mut pb = PathBuilder::new();
    pb.move_to(-6.0, -14.0);
    pb.line_to(6.0, -14.0);
    pb.line_to(1.2, -2.4);
    pb.quad_to(0.0, 0.0, -1.2, -2.4);
    pb.close();
    if let Some(path) = pb.finish() {
        ctx.set_fill_style(PaintStyle::linear_gradient(
            -6.0,
            -8.0,
            6.0,
            -8.0,
            &[(0.0, &p.light), (1.0, &p.base)],
        ));
        ctx.fill_path(&path);
    }
    let mut tip = PathBuilder::new();
    tip.move_to(-2.4, -5.8);
    tip.line_to(2.4, -5.8);
    tip.line_to(0.6, -1.2);
    tip.quad_to(0.0, 0.0, -0.6, -1.2);
    tip.close();
    if let Some(path) = tip.finish() {
        ctx.set_fill_css(&p.ink);
        ctx.fill_path(&path);
    }
    ctx.set_global_alpha(0.55);
    capsule(
        ctx,
        -4.0,
        -44.0,
        4.0,
        -44.0,
        6.0,
        &GlyphPaint {
            light: p.light.clone(),
            base: p.deep.clone(),
            dark: p.deep.clone(),
            deep: p.deep.clone(),
            ink: p.ink.clone(),
        },
    );
    ctx.set_global_alpha(1.0);
}

/// A map pin with its point at (x, y + 24).
fn pin(ctx: &mut Canvas2D, x: f64, y: f64, p: &GlyphPaint) {
    let (cx, cy, r) = (x, y - 6.0, 15.0);
    let mut pb = PathBuilder::new();
    let tipy = y + 24.0;
    let a = 0.62_f64;
    pb.move_to(x as f32, tipy as f32);
    pb.quad_to(
        (cx - r * 0.95) as f32,
        (cy + r * 0.95) as f32,
        (cx - r * a.cos()) as f32,
        (cy + r * a.sin()) as f32,
    );
    let steps = 32;
    for i in 1..=steps {
        let t = PI - a + (PI + 2.0 * a) * f64::from(i) / f64::from(steps);
        pb.line_to((cx + r * t.cos()) as f32, (cy + r * t.sin()) as f32);
    }
    pb.quad_to(
        (cx + r * 0.95) as f32,
        (cy + r * 0.95) as f32,
        x as f32,
        tipy as f32,
    );
    pb.close();
    if let Some(path) = pb.finish() {
        ctx.set_fill_style(PaintStyle::radial_gradient(
            cx - 0.4 * r,
            cy - 0.45 * r,
            0.0,
            cx,
            cy + 4.0,
            r * 1.6,
            &[(0.0, &p.light), (0.5, &p.base), (1.0, &p.dark)],
        ));
        ctx.fill_path(&path);
    }
    if let Some(hole) = crate::canvas::circle_path(cx, cy, 5.6) {
        ctx.set_fill_css(&p.deep);
        ctx.fill_path(&hole);
    }
}

#[cfg(test)]
mod tests {
    use super::{Glyph, GlyphAnim};

    #[test]
    fn folding_turns_half_way_and_back() {
        let mut g = GlyphAnim::new();
        g.set(Some(Glyph::Dots), 0.2);
        for _ in 0..240 {
            g.update(1.0 / 120.0);
        }
        let f = g.frame();
        assert!(f.amount > 0.99);
        assert!((f.spin - std::f64::consts::PI).abs() < 0.02, "{}", f.spin);
        g.set(None, 0.2);
        for _ in 0..240 {
            g.update(1.0 / 120.0);
        }
        let f = g.frame();
        assert!(f.amount < 0.01);
        assert!((f.spin - 2.0 * std::f64::consts::PI).abs() < 0.02);
        assert!(g.is_settled());
        assert_eq!(f.glyph, None);
    }

    #[test]
    fn a_breathing_glyph_comes_back_to_the_body() {
        let mut g = GlyphAnim::new();
        g.set(Some(Glyph::Progress), 0.7);
        let mut low = false;
        for i in 0..(120 * 5) {
            g.update(1.0 / 120.0);
            if i > 120 * 3 {
                low |= g.frame().amount < 0.1;
            }
        }
        assert!(low);
    }

    #[test]
    fn switching_glyphs_crossfades() {
        let mut g = GlyphAnim::new();
        g.set(Some(Glyph::Dots), 0.2);
        for _ in 0..240 {
            g.update(1.0 / 120.0);
        }
        g.set(Some(Glyph::Radar), 0.2);
        g.update(1.0 / 120.0);
        let f = g.frame();
        assert_eq!(f.outgoing, Some(Glyph::Dots));
        assert_eq!(f.glyph, Some(Glyph::Radar));
        assert!(f.amount > 0.99);
    }
}
