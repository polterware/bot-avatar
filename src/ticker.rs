/// Fixed simulation step (`1 / 120` s, <= `0.05` s max chunk inside `Sim::step`)
/// used to drive `Avatar::render(time_seconds)` deterministically across frame rates.
pub const FIXED_DT: f64 = 1.0 / 120.0;

/// Maximum per-frame delta clamped by the browser ticker (`0.1` s).
pub const MAX_FRAME_DT: f64 = 0.1;

/// Clamp a frame delta to [`MAX_FRAME_DT`], matching `ticker.ts`.
#[inline]
pub fn clamp_dt(dt: f64) -> f64 {
    if dt <= 0.0 || !dt.is_finite() {
        0.0
    } else {
        dt.min(MAX_FRAME_DT)
    }
}

/// Pointer position in avatar-local coordinates (`NaN` while away).
#[derive(Debug, Clone, Copy)]
pub struct Pointer {
    pub x: f64,
    pub y: f64,
}

impl Default for Pointer {
    fn default() -> Self {
        Self {
            x: f64::NAN,
            y: f64::NAN,
        }
    }
}

impl Pointer {
    pub const AWAY: Self = Self {
        x: f64::NAN,
        y: f64::NAN,
    };

    #[inline]
    pub fn is_present(self) -> bool {
        self.x.is_finite() && self.y.is_finite()
    }
}
