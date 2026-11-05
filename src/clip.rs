//! Keyframed motion. A [`Clip`] holds tracks of keys; [`ClipPlayer`] plays one, loops it,
//! and bridges to the next from wherever the body is.
//!
//! Units: rotation in degrees as `[pitch, yaw, roll]` (pitch down, yaw right, roll
//! clockwise), position in the 100-unit rig box (`y` down), squash as `[sx, sy]` anchored
//! at the base.

use crate::ease::Bezier;
use crate::expression::Expression;

/// A value at time `t`, eased toward the next key by `ease`.
#[derive(Debug, Clone, Copy)]
pub struct Key<const N: usize> {
    pub t: f64,
    pub v: [f64; N],
    pub ease: Bezier,
}

/// A rotation key.
pub const fn r(t: f64, pitch: f64, yaw: f64, roll: f64, ease: Bezier) -> Key<3> {
    Key {
        t,
        v: [pitch, yaw, roll],
        ease,
    }
}

/// A position key.
pub const fn p(t: f64, x: f64, y: f64, ease: Bezier) -> Key<2> {
    Key { t, v: [x, y], ease }
}

/// A squash key.
pub const fn s(t: f64, sx: f64, sy: f64, ease: Bezier) -> Key<2> {
    Key {
        t,
        v: [sx, sy],
        ease,
    }
}

/// A face change at `t`. `through_blink` closes the eyes for the swap.
#[derive(Debug, Clone, Copy)]
pub struct FaceKey {
    pub t: f64,
    pub expr: Expression,
    pub through_blink: bool,
}

pub const fn f(t: f64, expr: Expression) -> FaceKey {
    FaceKey {
        t,
        expr,
        through_blink: false,
    }
}

pub const fn fb(t: f64, expr: Expression) -> FaceKey {
    FaceKey {
        t,
        expr,
        through_blink: true,
    }
}

/// One authored motion.
#[derive(Debug)]
pub struct Clip {
    pub name: &'static str,
    pub duration: f64,
    pub rot: &'static [Key<3>],
    pub pos: &'static [Key<2>],
    pub squash: &'static [Key<2>],
    pub faces: &'static [FaceKey],
    pub blinks: &'static [f64],
    /// Windows (start, end) where the spin ring shows and the eyes laugh.
    pub whirls: &'static [(f64, f64)],
}

impl Clip {
    pub const EMPTY: Clip = Clip {
        name: "rest",
        duration: 1.0,
        rot: &[],
        pos: &[],
        squash: &[],
        faces: &[],
        blinks: &[],
        whirls: &[],
    };

    /// The pose at `t` seconds into the clip.
    pub fn sample(&self, t: f64) -> ClipPose {
        let rot = sample(self.rot, t).unwrap_or([0.0; 3]);
        let pos = sample(self.pos, t).unwrap_or([0.0; 2]);
        let squash = sample(self.squash, t).unwrap_or([1.0; 2]);
        let mut whirl = 0.0_f64;
        for &(a, b) in self.whirls {
            if t > a && t < b {
                let q = (t - a) / (b - a);
                whirl = whirl.max(
                    crate::ease::smoothstep(q / 0.2)
                        * (1.0 - crate::ease::smoothstep((q - 0.7) / 0.3)),
                );
            }
        }
        ClipPose {
            rot,
            pos,
            squash,
            whirl,
        }
    }
}

/// Where a clip puts the body.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ClipPose {
    pub rot: [f64; 3],
    pub pos: [f64; 2],
    pub squash: [f64; 2],
    pub whirl: f64,
}

impl ClipPose {
    pub const REST: ClipPose = ClipPose {
        rot: [0.0; 3],
        pos: [0.0; 2],
        squash: [1.0; 2],
        whirl: 0.0,
    };

    pub fn lerp(self, other: ClipPose, k: f64) -> ClipPose {
        let l = |a: f64, b: f64| a + (b - a) * k;
        ClipPose {
            rot: [
                l(self.rot[0], other.rot[0]),
                l(self.rot[1], other.rot[1]),
                l(self.rot[2], other.rot[2]),
            ],
            pos: [l(self.pos[0], other.pos[0]), l(self.pos[1], other.pos[1])],
            squash: [
                l(self.squash[0], other.squash[0]),
                l(self.squash[1], other.squash[1]),
            ],
            whirl: l(self.whirl, other.whirl),
        }
    }

    /// Each angle wrapped to the turn nearest `near`'s, so a bridge never unwinds a spin.
    fn wrapped_near(self, near: ClipPose) -> ClipPose {
        let w = |a: f64, b: f64| a - 360.0 * ((a - b) / 360.0).round();
        ClipPose {
            rot: [
                w(self.rot[0], near.rot[0]),
                w(self.rot[1], near.rot[1]),
                w(self.rot[2], near.rot[2]),
            ],
            ..self
        }
    }

    /// Scales the motion by `k` toward rest.
    pub fn scaled(self, k: f64) -> ClipPose {
        ClipPose::REST.lerp(self, k)
    }
}

fn sample<const N: usize>(keys: &[Key<N>], t: f64) -> Option<[f64; N]> {
    let first = keys.first()?;
    if t <= first.t {
        return Some(first.v);
    }
    let last = keys.last()?;
    if t >= last.t {
        return Some(last.v);
    }
    let i = keys.windows(2).position(|w| t < w[1].t)?;
    let (a, b) = (&keys[i], &keys[i + 1]);
    let span = (b.t - a.t).max(1e-9);
    let k = a.ease.at((t - a.t) / span);
    let mut out = a.v;
    for (j, o) in out.iter_mut().enumerate() {
        *o = a.v[j] + (b.v[j] - a.v[j]) * k;
    }
    Some(out)
}

/// How long a bridge from one clip into the next takes.
pub const BRIDGE_SECONDS: f64 = 0.36;
const BRIDGE_EASE: Bezier = Bezier::new(0.4, 0.0, 0.2, 1.0);

/// What happened during one [`ClipPlayer::advance`].
#[derive(Debug, Default)]
pub struct Cues {
    pub faces: Vec<FaceKey>,
    pub blinks: u32,
    pub looped: bool,
}

/// Plays clips: loops the current one and bridges into the next.
#[derive(Debug)]
pub struct ClipPlayer {
    clip: &'static Clip,
    t: f64,
    bridge: Option<(ClipPose, f64)>,
    loops: u64,
}

impl Default for ClipPlayer {
    fn default() -> Self {
        Self::new()
    }
}

impl ClipPlayer {
    pub const fn new() -> Self {
        Self {
            clip: &Clip::EMPTY,
            t: 0.0,
            bridge: None,
            loops: 0,
        }
    }

    pub fn clip(&self) -> &'static Clip {
        self.clip
    }

    pub fn loops(&self) -> u64 {
        self.loops
    }

    pub fn is_bridging(&self) -> bool {
        self.bridge.is_some()
    }

    /// Starts `clip` at `phase` (0 to 1 of its length). The body eases there from its
    /// current pose; `cut` jumps instead.
    pub fn play(&mut self, clip: &'static Clip, phase: f64, cut: bool) {
        let from = self.pose();
        self.clip = clip;
        self.t = phase.clamp(0.0, 0.999) * clip.duration;
        self.bridge = if cut { None } else { Some((from, 0.0)) };
    }

    /// Steps the clip, returning the face changes and blinks it passed.
    pub fn advance(&mut self, dt: f64) -> Cues {
        let mut cues = Cues::default();
        if let Some((from, t)) = self.bridge.as_mut() {
            *t += dt;
            if *t < BRIDGE_SECONDS {
                return cues;
            }
            let _ = from;
            self.bridge = None;
            self.pass(-1e-9, self.t, &mut cues);
            return cues;
        }
        let clip = self.clip;
        let mut next = self.t + dt;
        if next >= clip.duration {
            self.pass(self.t, clip.duration, &mut cues);
            next -= clip.duration;
            if next >= clip.duration {
                next = 0.0;
            }
            self.loops += 1;
            cues.looped = true;
            self.pass(-1e-9, next, &mut cues);
        } else {
            self.pass(self.t, next, &mut cues);
        }
        self.t = next;
        cues
    }

    fn pass(&self, a: f64, b: f64, cues: &mut Cues) {
        for face in self.clip.faces {
            if face.t > a && face.t <= b {
                cues.faces.push(*face);
            }
        }
        for &blink in self.clip.blinks {
            if blink > a && blink <= b {
                cues.blinks += 1;
            }
        }
    }

    /// The pose now, mid-bridge included.
    pub fn pose(&self) -> ClipPose {
        let here = self.clip.sample(self.t);
        match self.bridge {
            None => here,
            Some((from, t)) => {
                let k = BRIDGE_EASE.at((t / BRIDGE_SECONDS).clamp(0.0, 1.0));
                from.wrapped_near(here).lerp(here, k)
            }
        }
    }

    /// The face the clip shows at its current time, if it sets one.
    pub fn face_now(&self) -> Option<Expression> {
        self.clip
            .faces
            .iter()
            .rfind(|f| f.t <= self.t + 1e-9)
            .map(|f| f.expr)
    }
}

#[cfg(test)]
mod tests {
    use super::{BRIDGE_SECONDS, Clip, ClipPlayer, ClipPose, f, r};
    use crate::ease::Bezier;
    use crate::expression::Expression;

    static TURN: Clip = Clip {
        name: "turn",
        duration: 2.0,
        rot: &[
            r(0.0, 0.0, 0.0, 0.0, Bezier::IN_OUT),
            r(1.0, 0.0, 360.0, 0.0, Bezier::LINEAR),
            r(2.0, 0.0, 360.0, 0.0, Bezier::LINEAR),
        ],
        pos: &[],
        squash: &[],
        faces: &[f(0.5, Expression::Happy)],
        blinks: &[1.5],
        whirls: &[],
    };

    #[test]
    fn samples_hold_outside_the_keys_and_ease_between() {
        assert_eq!(TURN.sample(-1.0).rot, [0.0, 0.0, 0.0]);
        assert_eq!(TURN.sample(5.0).rot, [0.0, 360.0, 0.0]);
        assert!((TURN.sample(0.5).rot[1] - 180.0).abs() < 1e-6);
        assert_eq!(TURN.sample(0.5).squash, [1.0, 1.0]);
    }

    #[test]
    fn the_player_loops_and_reports_cues() {
        let mut player = ClipPlayer::new();
        player.play(&TURN, 0.0, true);
        let mut faces = 0;
        let mut blinks = 0;
        for _ in 0..250 {
            let cues = player.advance(1.0 / 60.0);
            faces += cues.faces.len();
            blinks += cues.blinks;
        }
        assert_eq!(player.loops(), 2);
        assert_eq!(faces, 2);
        assert_eq!(blinks, 2);
    }

    #[test]
    fn a_bridge_starts_where_the_body_is_and_never_unwinds() {
        let mut player = ClipPlayer::new();
        player.play(&TURN, 0.99, true);
        let before = player.pose();
        assert!((before.rot[1] - 360.0).abs() < 1e-6);
        player.play(&Clip::EMPTY, 0.0, false);
        let start = player.pose();
        assert!(
            start.rot[1].abs() < 1e-6,
            "wrapped to the near turn: {:?}",
            start.rot
        );
        for _ in 0..((BRIDGE_SECONDS * 60.0) as usize + 2) {
            player.advance(1.0 / 60.0);
        }
        assert_eq!(player.pose(), ClipPose::REST);
    }
}
