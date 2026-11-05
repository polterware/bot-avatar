//! Pokes: short moves laid over whatever the bot is doing, without stopping it. Several
//! can play at once; they add up.

use std::f64::consts::{PI, TAU};
use std::fmt;
use std::str::FromStr;

use crate::ease::{Bezier, bump, smoothstep};

/// A short move to lay over the current motion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Poke {
    /// A full turn (or more) with a small hop and the spin ring.
    Spin,
    /// A hop with a squash on takeoff and landing.
    Bounce,
    /// A nod.
    Nod,
    /// A quick head shake, no.
    Shake,
    /// A side to side wiggle.
    Wiggle,
    /// A somersault.
    Flip,
}

impl Poke {
    pub const ALL: [Poke; 6] = [
        Poke::Spin,
        Poke::Bounce,
        Poke::Nod,
        Poke::Shake,
        Poke::Wiggle,
        Poke::Flip,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Poke::Spin => "spin",
            Poke::Bounce => "bounce",
            Poke::Nod => "nod",
            Poke::Shake => "shake",
            Poke::Wiggle => "wiggle",
            Poke::Flip => "flip",
        }
    }

    /// How long it takes; a spin takes longer for each extra turn.
    pub fn duration(self, turns: u32) -> f64 {
        match self {
            Poke::Spin => 0.9 + 0.4 * f64::from(turns.max(1) - 1),
            Poke::Bounce => 0.62,
            Poke::Nod => 0.5,
            Poke::Shake => 0.6,
            Poke::Wiggle => 0.7,
            Poke::Flip => 0.95,
        }
    }
}

impl fmt::Display for Poke {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for Poke {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let lower = s.trim().to_ascii_lowercase();
        Self::ALL
            .into_iter()
            .find(|p| p.as_str() == lower)
            .ok_or_else(|| format!("unknown poke: {s}"))
    }
}

/// The sum of the pokes playing now.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PokeOffset {
    /// Degrees.
    pub pitch: f64,
    pub yaw: f64,
    pub roll: f64,
    /// Rig units, up is negative.
    pub y: f64,
    pub sx: f64,
    pub sy: f64,
    pub whirl: f64,
    pub whirl_angle: f64,
    pub laugh: f64,
}

impl PokeOffset {
    pub const NONE: PokeOffset = PokeOffset {
        pitch: 0.0,
        yaw: 0.0,
        roll: 0.0,
        y: 0.0,
        sx: 1.0,
        sy: 1.0,
        whirl: 0.0,
        whirl_angle: 0.0,
        laugh: 0.0,
    };
}

#[derive(Debug, Clone, Copy)]
struct Running {
    poke: Poke,
    turns: u32,
    t: f64,
}

/// The pokes in flight.
#[derive(Debug, Clone, Default)]
pub struct Pokes {
    running: Vec<Running>,
}

const SPIN_EASE: Bezier = Bezier::new(0.5, -0.22, 0.3, 1.0);

/// How far a [`Poke::Spin`] of `turns` has turned the bot, in degrees, `t` seconds after it
/// started: 0 before, `360 × turns` once it is over. Two bots spun at the same moment turn
/// alike, so one can hand over to the other while both face sideways.
pub fn spin_yaw(t: f64, turns: u32) -> f64 {
    let turns = turns.max(1);
    let u = (t / Poke::Spin.duration(turns)).clamp(0.0, 1.0);
    360.0 * f64::from(turns) * SPIN_EASE.at(u)
}

/// When a [`Poke::Spin`] of `turns` has turned the bot `degrees`, in seconds after it
/// started, for `degrees` above zero.
pub fn spin_time_at(degrees: f64, turns: u32) -> f64 {
    let duration = Poke::Spin.duration(turns.max(1));
    let (mut lo, mut hi) = (0.0_f64, duration);
    if spin_yaw(hi, turns) < degrees {
        return duration;
    }
    // The wind-up only turns back below zero, so the yaw is under any positive target
    // until it crosses it once.
    for _ in 0..48 {
        let mid = 0.5 * (lo + hi);
        if spin_yaw(mid, turns) < degrees {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    hi
}
const FLIP_EASE: Bezier = Bezier::new(0.45, -0.2, 0.35, 1.0);

impl Pokes {
    pub fn poke(&mut self, poke: Poke, turns: u32) {
        self.running.push(Running {
            poke,
            turns: turns.max(1),
            t: 0.0,
        });
    }

    pub fn clear(&mut self) {
        self.running.clear();
    }

    pub fn is_empty(&self) -> bool {
        self.running.is_empty()
    }

    pub fn update(&mut self, dt: f64) {
        for r in &mut self.running {
            r.t += dt;
        }
        self.running.retain(|r| r.t < r.poke.duration(r.turns));
    }

    pub fn offset(&self) -> PokeOffset {
        let mut o = PokeOffset::NONE;
        for r in &self.running {
            let u = (r.t / r.poke.duration(r.turns)).clamp(0.0, 1.0);
            match r.poke {
                Poke::Spin => {
                    o.yaw += spin_yaw(r.t, r.turns);
                    let hop = bump(u, 0.08, 0.7);
                    o.y -= 7.0 * hop;
                    o.sx *= 1.0 - 0.03 * hop;
                    o.sy *= 1.0 + 0.04 * hop;
                    let ring = smoothstep(u / 0.2) * (1.0 - smoothstep((u - 0.65) / 0.3));
                    if ring > o.whirl {
                        o.whirl = ring;
                        o.whirl_angle =
                            TAU * (1.5 * u + 0.9 * SPIN_EASE.at(u)) * f64::from(r.turns);
                    }
                    o.laugh = o.laugh.max(bump(u, 0.05, 0.9));
                }
                Poke::Bounce => {
                    let ground = bump(u, 0.0, 0.2) + bump(u, 0.8, 1.0);
                    let air = ((u - 0.16) / 0.68).clamp(0.0, 1.0);
                    o.y -= 18.0 * (PI * air).sin();
                    let stretch = bump(u, 0.18, 0.82);
                    o.sx *= 1.0 + 0.1 * ground - 0.05 * stretch;
                    o.sy *= 1.0 - 0.12 * ground + 0.07 * stretch;
                }
                Poke::Nod => {
                    o.pitch += 16.0 * bump(u, 0.0, 1.0);
                }
                Poke::Shake => {
                    o.yaw += 16.0 * (TAU * 2.5 * u).sin() * (1.0 - u);
                }
                Poke::Wiggle => {
                    o.roll += 13.0 * (TAU * 2.5 * u).sin() * (1.0 - u);
                    o.y -= 2.0 * bump(u, 0.0, 1.0);
                }
                Poke::Flip => {
                    let e = FLIP_EASE.at(u);
                    o.pitch -= 360.0 * e;
                    let air = ((u - 0.12) / 0.7).clamp(0.0, 1.0);
                    o.y -= 22.0 * (PI * air).sin();
                    let ground = bump(u, 0.0, 0.16) + bump(u, 0.82, 1.0);
                    o.sx *= 1.0 + 0.1 * ground;
                    o.sy *= 1.0 - 0.12 * ground;
                    let ring = bump(u, 0.12, 0.85);
                    if ring > o.whirl {
                        o.whirl = ring;
                        o.whirl_angle = TAU * 2.0 * u;
                    }
                }
            }
        }
        o
    }
}

#[cfg(test)]
mod tests {
    use super::{Poke, PokeOffset, Pokes, spin_time_at, spin_yaw};

    #[test]
    fn a_spin_reaches_a_turn_at_the_time_it_says() {
        for turns in [1, 2] {
            let full = 360.0 * f64::from(turns);
            assert_eq!(spin_yaw(0.0, turns), 0.0);
            assert!((spin_yaw(Poke::Spin.duration(turns), turns) - full).abs() < 1e-9);
            for degrees in [90.0, 270.0, full - 90.0] {
                let at = spin_time_at(degrees, turns);
                assert!(
                    (spin_yaw(at, turns) - degrees).abs() < 0.01,
                    "{degrees} at {at}"
                );
            }
        }
    }

    #[test]
    fn a_spin_turns_once_and_ends_at_rest() {
        let mut pokes = Pokes::default();
        pokes.poke(Poke::Spin, 1);
        let mut max_yaw: f64 = 0.0;
        for _ in 0..200 {
            pokes.update(1.0 / 120.0);
            max_yaw = max_yaw.max(pokes.offset().yaw);
        }
        assert!(max_yaw > 350.0);
        assert!(pokes.is_empty());
        assert_eq!(pokes.offset(), PokeOffset::NONE);
    }

    #[test]
    fn a_bounce_leaves_the_ground_and_squashes() {
        let mut pokes = Pokes::default();
        pokes.poke(Poke::Bounce, 1);
        let (mut up, mut squashed) = (false, false);
        for _ in 0..80 {
            pokes.update(1.0 / 120.0);
            let o = pokes.offset();
            up |= o.y < -15.0;
            squashed |= o.sy < 0.95;
        }
        assert!(up && squashed);
    }

    #[test]
    fn pokes_parse_by_name() {
        for p in Poke::ALL {
            assert_eq!(p.as_str().parse::<Poke>(), Ok(p));
        }
    }
}
