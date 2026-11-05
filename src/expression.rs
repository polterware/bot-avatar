//! Faces. An expression is a pair of eye shapes in the same terms [`crate::draw`] strokes an
//! eye with: a quadratic from `(-x0, y0)` to `(x0, y0)` bowed toward `cy`, `w` wide, with
//! round caps. Moving between expressions is moving those numbers.
//!
//! [`FaceAnim`] morphs from one expression to the next (or closes the eyes, swaps the shape
//! and opens them again), plays blinks that open with a small overshoot, and gives the
//! resolved pair to the pose.

use std::fmt;
use std::str::FromStr;

use crate::ease::{Bezier, smoothstep};

/// One eye's stroke. `dx`/`dy` move it from its place, `tilt` turns it (radians, clockwise),
/// and `cross` from 0 to 1 splits it into two strokes turned ±45° (an X).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EyeShape {
    pub x0: f64,
    pub y0: f64,
    pub cy: f64,
    pub w: f64,
    pub dx: f64,
    pub dy: f64,
    pub tilt: f64,
    pub cross: f64,
}

impl EyeShape {
    const fn new(x0: f64, y0: f64, cy: f64, w: f64) -> Self {
        Self {
            x0,
            y0,
            cy,
            w,
            dx: 0.0,
            dy: 0.0,
            tilt: 0.0,
            cross: 0.0,
        }
    }

    const fn at(self, dx: f64, dy: f64) -> Self {
        Self { dx, dy, ..self }
    }

    const fn tilted(self, tilt: f64) -> Self {
        Self { tilt, ..self }
    }

    /// The classic open eye.
    pub const OPEN: EyeShape = EyeShape::new(0.01, 1.1, -3.3, 12.6);
    /// A closed lid: a short flat stroke.
    pub const SHUT: EyeShape = EyeShape::new(5.4, 0.6, 0.6, 2.8);

    /// `self` toward `other` by `k`.
    pub fn lerp(self, other: EyeShape, k: f64) -> EyeShape {
        let l = |a: f64, b: f64| a + (b - a) * k;
        EyeShape {
            x0: l(self.x0, other.x0),
            y0: l(self.y0, other.y0),
            cy: l(self.cy, other.cy),
            w: l(self.w, other.w),
            dx: l(self.dx, other.dx),
            dy: l(self.dy, other.dy),
            tilt: l(self.tilt, other.tilt),
            cross: l(self.cross, other.cross),
        }
    }

    /// The same eye with its lid down by `lid` (0 open, 1 shut). Keeps place and tilt.
    pub fn with_lid(self, lid: f64) -> EyeShape {
        if lid <= 0.0 {
            return self;
        }
        let shut = EyeShape {
            x0: self.x0.max(EyeShape::SHUT.x0),
            dx: self.dx,
            dy: self.dy,
            tilt: self.tilt,
            cross: 0.0,
            ..EyeShape::SHUT
        };
        self.lerp(shut, lid.min(1.0))
    }

    /// Grows the open part of the eye by `k` (1 is as drawn), for the pop after a blink.
    pub fn popped(self, k: f64) -> EyeShape {
        EyeShape {
            w: self.w * k,
            y0: self.y0 * k,
            cy: self.cy * k,
            ..self
        }
    }
}

/// A face the eyes can make.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Expression {
    #[default]
    Neutral,
    Happy,
    Surprised,
    Sad,
    Sleepy,
    Squinted,
    Calm,
    Angry,
    Confused,
    /// One eye larger than the other, the larger on the side the head turns to.
    Inspecting,
    Wink,
    Dizzy,
}

impl Expression {
    pub const ALL: [Expression; 12] = [
        Expression::Neutral,
        Expression::Happy,
        Expression::Surprised,
        Expression::Sad,
        Expression::Sleepy,
        Expression::Squinted,
        Expression::Calm,
        Expression::Angry,
        Expression::Confused,
        Expression::Inspecting,
        Expression::Wink,
        Expression::Dizzy,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Expression::Neutral => "neutral",
            Expression::Happy => "happy",
            Expression::Surprised => "surprised",
            Expression::Sad => "sad",
            Expression::Sleepy => "sleepy",
            Expression::Squinted => "squinted",
            Expression::Calm => "calm",
            Expression::Angry => "angry",
            Expression::Confused => "confused",
            Expression::Inspecting => "inspecting",
            Expression::Wink => "wink",
            Expression::Dizzy => "dizzy",
        }
    }

    /// The left and right eye. `bias` (-1 to 1) picks which eye of [`Expression::Inspecting`]
    /// is the larger: 1 the right.
    pub fn eyes(self, bias: f64) -> [EyeShape; 2] {
        const HAPPY: EyeShape = EyeShape::new(6.2, 2.2, -11.4, 4.4);
        const SAD: EyeShape = EyeShape::new(5.6, 1.4, -5.6, 4.2).at(0.0, 1.0);
        const SQUINT: EyeShape = EyeShape::new(5.2, 0.4, 0.4, 4.6);
        match self {
            Expression::Neutral => [EyeShape::OPEN; 2],
            Expression::Happy => [HAPPY; 2],
            Expression::Surprised => [EyeShape::new(0.01, 2.8, -8.4, 13.6).at(0.0, -0.6); 2],
            Expression::Sad => [SAD.tilted(-0.32), SAD.tilted(0.32)],
            Expression::Sleepy => [EyeShape::new(6.0, -0.9, 6.4, 4.0).at(0.0, 0.8); 2],
            Expression::Squinted => [SQUINT; 2],
            Expression::Calm => [EyeShape::new(0.01, 0.3, -0.9, 9.6); 2],
            Expression::Angry => {
                let a = SQUINT.at(0.0, 0.6);
                [a.tilted(0.38), a.tilted(-0.38)]
            }
            Expression::Confused => [
                EyeShape::new(0.01, 1.4, -4.2, 11.4),
                SQUINT.tilted(0.26).at(0.0, -1.8),
            ],
            Expression::Inspecting => {
                let big = EyeShape::new(0.01, 1.7, -5.1, 14.8);
                let small = EyeShape::new(0.01, 0.6, -1.8, 9.0);
                let k = (bias.clamp(-1.0, 1.0) + 1.0) / 2.0;
                [big.lerp(small, k), small.lerp(big, k)]
            }
            Expression::Wink => [HAPPY, EyeShape::OPEN],
            Expression::Dizzy => {
                [EyeShape {
                    cross: 1.0,
                    ..EyeShape::new(5.0, 0.0, 0.0, 3.6)
                }; 2]
            }
        }
    }

    /// Faces whose shape has no open eye to blink with.
    pub const fn is_closed(self) -> bool {
        matches!(
            self,
            Expression::Happy | Expression::Sleepy | Expression::Squinted | Expression::Dizzy
        )
    }
}

impl fmt::Display for Expression {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for Expression {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let lower = s.trim().to_ascii_lowercase();
        Self::ALL
            .into_iter()
            .find(|e| e.as_str() == lower)
            .ok_or_else(|| format!("unknown expression: {s}"))
    }
}

/// How long a face takes to change shape.
pub const MORPH_SECONDS: f64 = 0.28;
/// How long a face takes to change through a blink.
pub const LID_MORPH_SECONDS: f64 = 0.36;
/// How long a blink takes.
pub const BLINK_SECONDS: f64 = 0.24;
/// How much larger the eyes open after a blink before they settle.
const BLINK_POP: f64 = 0.06;
const MORPH_EASE: Bezier = Bezier::new(0.4, 0.06, 0.18, 1.0);

#[derive(Debug, Clone, Copy)]
enum Change {
    Morph { from: [EyeShape; 2], t: f64 },
    ThroughBlink { from: [EyeShape; 2], t: f64 },
}

/// The face over time: the current expression, a change in flight, and a blink.
#[derive(Debug, Clone)]
pub struct FaceAnim {
    expr: Expression,
    bias: f64,
    change: Option<Change>,
    blink: Option<f64>,
    /// A change asked for while a blink was closing waits for it.
    queued: Option<(Expression, bool)>,
}

impl Default for FaceAnim {
    fn default() -> Self {
        Self::new(Expression::Neutral)
    }
}

impl FaceAnim {
    pub fn new(expr: Expression) -> Self {
        Self {
            expr,
            bias: 0.0,
            change: None,
            blink: None,
            queued: None,
        }
    }

    pub fn expression(&self) -> Expression {
        self.expr
    }

    /// Sets the turn the inspecting face reads, -1 (left) to 1 (right).
    pub fn set_bias(&mut self, bias: f64) {
        self.bias = bias.clamp(-1.0, 1.0);
    }

    pub fn is_settled(&self) -> bool {
        self.change.is_none() && self.blink.is_none() && self.queued.is_none()
    }

    /// Moves to `expr`. `through_blink` closes the eyes for the swap instead of morphing.
    pub fn set(&mut self, expr: Expression, through_blink: bool) {
        if expr == self.expr && self.queued.is_none() {
            return;
        }
        if self.blink.is_some() {
            self.queued = Some((expr, through_blink));
            return;
        }
        let from = self.shapes();
        self.expr = expr;
        self.change = Some(if through_blink {
            Change::ThroughBlink { from, t: 0.0 }
        } else {
            Change::Morph { from, t: 0.0 }
        });
    }

    /// Starts a blink unless one is playing or the face has no open eyes.
    pub fn blink(&mut self) {
        if self.blink.is_none() && self.change.is_none() && !self.expr.is_closed() {
            self.blink = Some(0.0);
        }
    }

    pub fn update(&mut self, dt: f64) {
        if let Some(t) = self.blink.as_mut() {
            *t += dt;
            if *t >= BLINK_SECONDS {
                self.blink = None;
                if let Some((expr, through)) = self.queued.take() {
                    self.set(expr, through);
                }
            }
        }
        if let Some(change) = self.change.as_mut() {
            let (t, len) = match change {
                Change::Morph { t, .. } => (t, MORPH_SECONDS),
                Change::ThroughBlink { t, .. } => (t, LID_MORPH_SECONDS),
            };
            *t += dt;
            if *t >= len {
                self.change = None;
            }
        }
    }

    /// The two eyes as they look now.
    pub fn shapes(&self) -> [EyeShape; 2] {
        let target = self.expr.eyes(self.bias);
        let mut eyes = match self.change {
            None => target,
            Some(Change::Morph { from, t }) => {
                let k = MORPH_EASE.at((t / MORPH_SECONDS).clamp(0.0, 1.0));
                [from[0].lerp(target[0], k), from[1].lerp(target[1], k)]
            }
            Some(Change::ThroughBlink { from, t }) => {
                let p = (t / LID_MORPH_SECONDS).clamp(0.0, 1.0);
                if p < 0.45 {
                    let lid = smoothstep(p / 0.45);
                    [from[0].with_lid(lid), from[1].with_lid(lid)]
                } else {
                    let lid = 1.0 - smoothstep((p - 0.45) / 0.55);
                    [target[0].with_lid(lid), target[1].with_lid(lid)]
                }
            }
        };
        if let Some(t) = self.blink {
            let (lid, pop) = blink_curve(t / BLINK_SECONDS);
            eyes = [
                eyes[0].with_lid(lid).popped(pop),
                eyes[1].with_lid(lid).popped(pop),
            ];
        }
        eyes
    }
}

/// A blink at `p` (0 to 1): how shut the lid is, and how much larger the eye opens past it.
/// It closes in the first 40%, stays shut a moment, and opens with a small overshoot.
fn blink_curve(p: f64) -> (f64, f64) {
    let p = p.clamp(0.0, 1.0);
    if p < 0.4 {
        (smoothstep(p / 0.4), 1.0)
    } else if p < 0.52 {
        (1.0, 1.0)
    } else {
        let q = (p - 0.52) / 0.48;
        let lid = 1.0 - smoothstep((q / 0.6).min(1.0));
        let swell = ((q - 0.3) / 0.7).clamp(0.0, 1.0);
        let pop = 1.0 + BLINK_POP * (std::f64::consts::PI * swell).sin();
        (lid, pop)
    }
}

#[cfg(test)]
mod tests {
    use super::{BLINK_SECONDS, Expression, EyeShape, FaceAnim, MORPH_SECONDS};

    #[test]
    fn every_expression_parses_by_name() {
        for e in Expression::ALL {
            assert_eq!(e.as_str().parse::<Expression>(), Ok(e));
        }
    }

    #[test]
    fn a_morph_lands_on_the_target_shape() {
        let mut face = FaceAnim::new(Expression::Neutral);
        face.set(Expression::Surprised, false);
        let mid = face.shapes();
        assert_ne!(mid, Expression::Surprised.eyes(0.0));
        for _ in 0..40 {
            face.update(MORPH_SECONDS / 30.0);
        }
        assert_eq!(face.shapes(), Expression::Surprised.eyes(0.0));
        assert!(face.is_settled());
    }

    #[test]
    fn a_blink_closes_and_reopens() {
        let mut face = FaceAnim::new(Expression::Neutral);
        face.blink();
        let mut shut = false;
        for _ in 0..60 {
            face.update(BLINK_SECONDS / 50.0);
            let eye = face.shapes()[0];
            shut |= (eye.w - EyeShape::SHUT.w).abs() < 0.2;
        }
        assert!(shut);
        assert_eq!(face.shapes(), [EyeShape::OPEN; 2]);
    }

    #[test]
    fn a_change_during_a_blink_waits_for_it() {
        let mut face = FaceAnim::new(Expression::Neutral);
        face.blink();
        face.update(0.05);
        face.set(Expression::Happy, false);
        assert_eq!(face.expression(), Expression::Neutral);
        for _ in 0..80 {
            face.update(1.0 / 60.0);
        }
        assert_eq!(face.expression(), Expression::Happy);
        assert!(face.is_settled());
    }
}
