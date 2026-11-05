//! Timing curves shared by the motion layer: CSS-style cubic béziers, smoothstep, and a
//! critically damped spring.

/// A CSS `cubic-bezier(x1, y1, x2, y2)`. The y values may leave 0..1 to overshoot.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bezier {
    pub x1: f64,
    pub y1: f64,
    pub x2: f64,
    pub y2: f64,
}

impl Bezier {
    pub const fn new(x1: f64, y1: f64, x2: f64, y2: f64) -> Self {
        Self { x1, y1, x2, y2 }
    }

    pub const LINEAR: Bezier = Bezier::new(0.0, 0.0, 1.0, 1.0);
    /// Slow at both ends.
    pub const IN_OUT: Bezier = Bezier::new(0.42, 0.0, 0.58, 1.0);
    /// Fast start, long settle.
    pub const OUT: Bezier = Bezier::new(0.16, 1.0, 0.3, 1.0);
    /// Slow start, fast end.
    pub const IN: Bezier = Bezier::new(0.5, 0.0, 0.9, 0.4);
    /// Shoots past the end and comes back.
    pub const OVERSHOOT: Bezier = Bezier::new(0.34, 1.45, 0.64, 1.0);
    /// Dips back before it goes.
    pub const ANTICIPATE: Bezier = Bezier::new(0.36, -0.4, 0.66, 1.0);
    /// Dips back, then shoots past.
    pub const SNAP: Bezier = Bezier::new(0.6, -0.3, 0.3, 1.35);
    /// Soft and even.
    pub const SINE: Bezier = Bezier::new(0.37, 0.0, 0.63, 1.0);

    /// Progress at time `x` (0 to 1).
    pub fn at(self, x: f64) -> f64 {
        if x <= 0.0 {
            return 0.0;
        }
        if x >= 1.0 {
            return 1.0;
        }
        let bx = |t: f64| {
            let u = 1.0 - t;
            3.0 * u * u * t * self.x1 + 3.0 * u * t * t * self.x2 + t * t * t
        };
        let dbx = |t: f64| {
            let u = 1.0 - t;
            3.0 * u * u * self.x1
                + 6.0 * u * t * (self.x2 - self.x1)
                + 3.0 * t * t * (1.0 - self.x2)
        };
        let mut t = x;
        for _ in 0..8 {
            let err = bx(t) - x;
            if err.abs() < 1e-7 {
                break;
            }
            let d = dbx(t);
            if d.abs() < 1e-6 {
                break;
            }
            t = (t - err / d).clamp(0.0, 1.0);
        }
        if (bx(t) - x).abs() > 1e-4 {
            let (mut lo, mut hi) = (0.0, 1.0);
            for _ in 0..30 {
                t = (lo + hi) / 2.0;
                if bx(t) < x {
                    lo = t;
                } else {
                    hi = t;
                }
            }
        }
        let u = 1.0 - t;
        3.0 * u * u * t * self.y1 + 3.0 * u * t * t * self.y2 + t * t * t
    }
}

#[inline]
pub fn smoothstep(x: f64) -> f64 {
    let x = x.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

/// `sin(πx)²` inside `a..b`, 0 outside: a soft bump.
#[inline]
pub fn bump(x: f64, a: f64, b: f64) -> f64 {
    if x <= a || x >= b {
        0.0
    } else {
        (std::f64::consts::PI * (x - a) / (b - a)).sin().powi(2)
    }
}

/// A critically damped spring toward `target`. `omega` is its speed (rad/s).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Spring {
    pub x: f64,
    pub v: f64,
    pub target: f64,
}

impl Spring {
    pub const fn at(x: f64) -> Self {
        Self {
            x,
            v: 0.0,
            target: x,
        }
    }

    pub fn step(&mut self, omega: f64, dt: f64) {
        let a = -2.0 * omega * self.v - omega * omega * (self.x - self.target);
        self.v += a * dt;
        self.x += self.v * dt;
        if !self.x.is_finite() || !self.v.is_finite() {
            self.x = self.target;
            self.v = 0.0;
        }
    }

    pub fn settle(&mut self) {
        self.x = self.target;
        self.v = 0.0;
    }

    pub fn is_settled(&self) -> bool {
        (self.x - self.target).abs() < 1e-3 && self.v.abs() < 1e-2
    }
}

#[cfg(test)]
mod tests {
    use super::{Bezier, Spring};

    #[test]
    fn bezier_hits_its_ends_and_matches_linear() {
        for b in [Bezier::IN_OUT, Bezier::OUT, Bezier::SNAP, Bezier::OVERSHOOT] {
            assert_eq!(b.at(0.0), 0.0);
            assert_eq!(b.at(1.0), 1.0);
        }
        assert!((Bezier::LINEAR.at(0.3) - 0.3).abs() < 1e-6);
        assert!((Bezier::IN_OUT.at(0.5) - 0.5).abs() < 1e-6);
        assert!(Bezier::OVERSHOOT.at(0.6) > 1.0);
        assert!(Bezier::ANTICIPATE.at(0.15) < 0.0);
    }

    #[test]
    fn a_spring_settles_without_overshoot() {
        let mut s = Spring::at(0.0);
        s.target = 1.0;
        let mut peak: f64 = 0.0;
        for _ in 0..240 {
            s.step(12.0, 1.0 / 120.0);
            peak = peak.max(s.x);
        }
        assert!(peak <= 1.0 + 1e-6, "{peak}");
        assert!((s.x - 1.0).abs() < 1e-3);
    }
}
