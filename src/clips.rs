//! The clip library: every authored motion the director picks from.
//!
//! Each clip starts and ends at rest unless it holds a pose for its state (a droop, a nod
//! off), so a loop or a bridge into the next clip stays short. Full turns end on a multiple
//! of 360°.

use crate::clip::{Clip, f, fb, p, r, s};
use crate::ease::Bezier;
use crate::expression::Expression as E;

const IO: Bezier = Bezier::IN_OUT;
const OUT: Bezier = Bezier::OUT;
const IN: Bezier = Bezier::IN;
const SINE: Bezier = Bezier::SINE;
const POP: Bezier = Bezier::OVERSHOOT;
const ANT: Bezier = Bezier::ANTICIPATE;
const SNAP: Bezier = Bezier::SNAP;
const LIN: Bezier = Bezier::LINEAR;

// Idle.

/// A soft sway, two blinks.
pub const IDLE_BREEZE: Clip = Clip {
    name: "idle_breeze",
    duration: 4.0,
    rot: &[
        r(0.0, 0.0, 0.0, 0.0, SINE),
        r(1.3, -3.0, 8.0, 2.0, SINE),
        r(2.7, 2.0, -7.0, -2.0, SINE),
        r(4.0, 0.0, 0.0, 0.0, SINE),
    ],
    pos: &[],
    squash: &[],
    faces: &[],
    blinks: &[1.2, 3.3],
    whirls: &[],
};

/// Looks to one side, then the other, blinking as it turns.
pub const IDLE_GLANCE: Clip = Clip {
    name: "idle_glance",
    duration: 6.0,
    rot: &[
        r(0.0, 0.0, 0.0, 0.0, IO),
        r(0.55, -4.0, 30.0, 4.0, POP),
        r(2.2, -4.0, 30.0, 4.0, IO),
        r(2.8, 6.0, -26.0, -3.0, POP),
        r(4.6, 6.0, -26.0, -3.0, IO),
        r(5.3, 0.0, 0.0, 0.0, OUT),
        r(6.0, 0.0, 0.0, 0.0, LIN),
    ],
    pos: &[],
    squash: &[],
    faces: &[],
    blinks: &[0.4, 2.65, 5.15],
    whirls: &[],
};

/// Crouches, hops into a full turn, lands with a squash.
pub const IDLE_TURN: Clip = Clip {
    name: "idle_turn",
    duration: 6.0,
    rot: &[
        r(0.0, 0.0, 0.0, 0.0, IO),
        r(1.0, 0.0, 0.0, 0.0, ANT),
        r(2.3, -6.0, 360.0, 0.0, OUT),
        r(2.8, 0.0, 360.0, 0.0, LIN),
    ],
    pos: &[
        p(1.05, 0.0, 0.0, OUT),
        p(1.55, 0.0, -14.0, IN),
        p(2.05, 0.0, 0.0, LIN),
    ],
    squash: &[
        s(0.75, 1.0, 1.0, IO),
        s(1.0, 1.09, 0.9, OUT),
        s(1.3, 0.95, 1.07, IO),
        s(2.0, 1.0, 1.0, OUT),
        s(2.12, 1.08, 0.92, OUT),
        s(2.45, 1.0, 1.0, LIN),
    ],
    faces: &[f(1.05, E::Happy), f(2.5, E::Neutral)],
    blinks: &[4.3],
    whirls: &[(1.1, 2.3)],
};

/// Tilts the head and peeks up, curious.
pub const IDLE_PEEK: Clip = Clip {
    name: "idle_peek",
    duration: 5.0,
    rot: &[
        r(0.0, 0.0, 0.0, 0.0, IO),
        r(0.8, -10.0, 6.0, 12.0, POP),
        r(2.4, -10.0, 6.0, 12.0, IO),
        r(3.2, 0.0, 0.0, 0.0, OUT),
        r(5.0, 0.0, 0.0, 0.0, LIN),
    ],
    pos: &[],
    squash: &[],
    faces: &[f(0.6, E::Inspecting), f(3.1, E::Neutral)],
    blinks: &[1.9, 4.3],
    whirls: &[],
};

// Listening.

/// Leans in and nods along.
pub const LISTEN_LEAN: Clip = Clip {
    name: "listen_lean",
    duration: 4.0,
    rot: &[
        r(0.0, 0.0, 0.0, 0.0, OUT),
        r(0.6, 10.0, 0.0, 0.0, IO),
        r(1.1, 4.0, 0.0, 0.0, IO),
        r(1.6, 10.0, 0.0, 0.0, IO),
        r(2.1, 4.0, 0.0, 0.0, SINE),
        r(3.2, 8.0, -6.0, -3.0, SINE),
        r(4.0, 0.0, 0.0, 0.0, LIN),
    ],
    pos: &[],
    squash: &[],
    faces: &[],
    blinks: &[2.6],
    whirls: &[],
};

// Streaming.

pub const STREAM_SWAY: Clip = Clip {
    name: "stream_sway",
    duration: 2.4,
    rot: &[
        r(0.0, 0.0, 0.0, 0.0, SINE),
        r(0.6, 0.0, -10.0, -9.0, SINE),
        r(1.8, 0.0, 10.0, 9.0, SINE),
        r(2.4, 0.0, 0.0, 0.0, LIN),
    ],
    pos: &[
        p(0.0, 0.0, 0.0, SINE),
        p(0.6, -8.0, 0.0, SINE),
        p(1.8, 8.0, 0.0, SINE),
        p(2.4, 0.0, 0.0, LIN),
    ],
    squash: &[],
    faces: &[],
    blinks: &[1.2],
    whirls: &[],
};

pub const STREAM_BOB: Clip = Clip {
    name: "stream_bob",
    duration: 2.0,
    rot: &[
        r(0.0, 0.0, 0.0, 0.0, OUT),
        r(0.25, 9.0, 0.0, 0.0, IO),
        r(0.6, 0.0, 0.0, 0.0, OUT),
        r(1.0, 9.0, 4.0, 3.0, IO),
        r(1.4, 0.0, 0.0, 0.0, LIN),
    ],
    pos: &[
        p(0.0, 0.0, 0.0, OUT),
        p(0.25, 0.0, 2.5, IO),
        p(0.6, 0.0, -2.0, OUT),
        p(1.0, 0.0, 2.5, IO),
        p(1.4, 0.0, 0.0, LIN),
    ],
    squash: &[],
    faces: &[],
    blinks: &[1.75],
    whirls: &[],
};

pub const STREAM_TWIRL: Clip = Clip {
    name: "stream_twirl",
    duration: 2.6,
    rot: &[
        r(0.0, 0.0, 0.0, 0.0, ANT),
        r(1.0, -8.0, 360.0, 6.0, OUT),
        r(1.5, 0.0, 360.0, 0.0, LIN),
    ],
    pos: &[
        p(0.0, 0.0, 0.0, OUT),
        p(0.4, 0.0, -8.0, IN),
        p(0.95, 0.0, 0.0, LIN),
    ],
    squash: &[
        s(0.95, 1.0, 1.0, OUT),
        s(1.08, 1.06, 0.94, OUT),
        s(1.4, 1.0, 1.0, LIN),
    ],
    faces: &[f(0.05, E::Squinted), f(1.3, E::Neutral)],
    blinks: &[],
    whirls: &[(0.15, 1.0)],
};

// Working.

/// Reads a page: head down, scanning left to right.
pub const WORK_SCAN: Clip = Clip {
    name: "work_scan",
    duration: 5.0,
    rot: &[
        r(0.0, 0.0, 0.0, 0.0, OUT),
        r(0.6, 12.0, -22.0, 0.0, SINE),
        r(1.6, 12.0, 22.0, 0.0, SINE),
        r(2.6, 12.0, -22.0, 0.0, SINE),
        r(3.6, 12.0, 22.0, 0.0, IO),
        r(4.4, 0.0, 0.0, 0.0, LIN),
    ],
    pos: &[],
    squash: &[],
    faces: &[f(0.5, E::Inspecting), f(4.3, E::Neutral)],
    blinks: &[2.1, 4.6],
    whirls: &[],
};

/// Crouches and somersaults.
pub const WORK_FLIP: Clip = Clip {
    name: "work_flip",
    duration: 4.0,
    rot: &[r(0.55, 0.0, 0.0, 0.0, IO), r(1.5, -360.0, 0.0, 0.0, LIN)],
    pos: &[
        p(0.55, 0.0, 0.0, OUT),
        p(1.0, 0.0, -24.0, IN),
        p(1.5, 0.0, 0.0, LIN),
    ],
    squash: &[
        s(0.0, 1.0, 1.0, IO),
        s(0.5, 1.12, 0.86, OUT),
        s(0.7, 0.92, 1.1, IO),
        s(1.3, 1.0, 1.0, OUT),
        s(1.55, 1.12, 0.88, OUT),
        s(1.95, 1.0, 1.0, LIN),
    ],
    faces: &[f(0.6, E::Surprised), f(2.4, E::Neutral)],
    blinks: &[3.2],
    whirls: &[(0.6, 1.5)],
};

/// Fiddles: quick tilts left and right, focused.
pub const WORK_TINKER: Clip = Clip {
    name: "work_tinker",
    duration: 4.0,
    rot: &[
        r(0.0, 0.0, 0.0, 0.0, OUT),
        r(0.4, 6.0, -8.0, -14.0, IO),
        r(0.9, 6.0, 8.0, 12.0, IO),
        r(1.4, 6.0, -6.0, -10.0, IO),
        r(1.9, 6.0, 4.0, 6.0, IO),
        r(2.6, 0.0, 0.0, 0.0, LIN),
    ],
    pos: &[],
    squash: &[],
    faces: &[f(0.3, E::Squinted), f(2.6, E::Neutral)],
    blinks: &[3.3],
    whirls: &[],
};

/// Winds up, spins on the spot, lands.
pub const WORK_SPIN: Clip = Clip {
    name: "work_spin",
    duration: 4.0,
    rot: &[
        r(0.0, 0.0, 0.0, 0.0, IO),
        r(0.4, 0.0, -20.0, -8.0, OUT),
        r(1.6, 0.0, 360.0, 0.0, LIN),
    ],
    pos: &[
        p(0.4, 0.0, 0.0, OUT),
        p(0.9, 0.0, -14.0, IN),
        p(1.4, 0.0, 0.0, LIN),
    ],
    squash: &[
        s(1.4, 1.0, 1.0, OUT),
        s(1.55, 1.1, 0.9, OUT),
        s(1.9, 1.0, 1.0, LIN),
    ],
    faces: &[f(0.4, E::Happy), f(1.9, E::Neutral)],
    blinks: &[3.2],
    whirls: &[(0.5, 1.5)],
};

/// Looks up to think, gets an idea, hops.
pub const WORK_PONDER: Clip = Clip {
    name: "work_ponder",
    duration: 5.0,
    rot: &[
        r(0.0, 0.0, 0.0, 0.0, OUT),
        r(0.8, -14.0, -18.0, -6.0, SINE),
        r(2.6, -14.0, -18.0, -6.0, SNAP),
        r(3.0, 0.0, 0.0, 0.0, LIN),
    ],
    pos: &[
        p(3.0, 0.0, 0.0, OUT),
        p(3.3, 0.0, -9.0, IN),
        p(3.65, 0.0, 0.0, LIN),
    ],
    squash: &[
        s(3.6, 1.0, 1.0, OUT),
        s(3.72, 1.06, 0.94, OUT),
        s(4.0, 1.0, 1.0, LIN),
    ],
    faces: &[f(0.8, E::Calm), f(2.95, E::Surprised), f(4.2, E::Neutral)],
    blinks: &[1.8],
    whirls: &[],
};

/// Two hops, leaning one way then the other.
pub const WORK_HOP: Clip = Clip {
    name: "work_hop",
    duration: 3.0,
    rot: &[
        r(0.0, 0.0, 0.0, 0.0, OUT),
        r(0.3, 0.0, -10.0, -10.0, IN),
        r(0.6, 0.0, -6.0, -4.0, OUT),
        r(1.0, 0.0, 10.0, 10.0, IN),
        r(1.3, 0.0, 6.0, 4.0, IO),
        r(1.8, 0.0, 0.0, 0.0, LIN),
    ],
    pos: &[
        p(0.0, 0.0, 0.0, OUT),
        p(0.3, -6.0, -14.0, IN),
        p(0.6, -6.0, 0.0, OUT),
        p(1.0, 6.0, -14.0, IN),
        p(1.3, 6.0, 0.0, IO),
        p(1.8, 0.0, 0.0, LIN),
    ],
    squash: &[
        s(0.55, 1.0, 1.0, OUT),
        s(0.65, 1.1, 0.9, OUT),
        s(0.85, 1.0, 1.0, LIN),
        s(1.25, 1.0, 1.0, OUT),
        s(1.35, 1.1, 0.9, OUT),
        s(1.55, 1.0, 1.0, LIN),
    ],
    faces: &[f(0.2, E::Happy), f(2.0, E::Neutral)],
    blinks: &[2.5],
    whirls: &[],
};

// Trouble.

/// Bumps into something, reels, shakes it off.
pub const BLOCKED_BONK: Clip = Clip {
    name: "blocked_bonk",
    duration: 2.6,
    rot: &[
        r(0.4, 0.0, 0.0, 0.0, OUT),
        r(0.55, 0.0, -6.0, 8.0, IO),
        r(0.95, 0.0, 10.0, -8.0, IO),
        r(1.35, 0.0, -8.0, 6.0, IO),
        r(1.75, 0.0, 0.0, 0.0, LIN),
    ],
    pos: &[
        p(0.0, 0.0, 0.0, IN),
        p(0.32, 7.0, 0.0, OUT),
        p(0.48, -4.0, 0.0, IO),
        p(1.0, 0.0, 0.0, LIN),
    ],
    squash: &[
        s(0.3, 1.0, 1.0, OUT),
        s(0.38, 0.86, 1.08, OUT),
        s(0.75, 1.0, 1.0, LIN),
    ],
    faces: &[f(0.36, E::Dizzy), fb(1.9, E::Confused)],
    blinks: &[],
    whirls: &[],
};

/// Head down, swaying slowly.
pub const SAD_DROOP: Clip = Clip {
    name: "sad_droop",
    duration: 3.4,
    rot: &[
        r(0.0, 18.0, -4.0, -3.0, SINE),
        r(1.7, 18.0, 4.0, 3.0, SINE),
        r(3.4, 18.0, -4.0, -3.0, LIN),
    ],
    pos: &[p(0.0, 0.0, 6.0, LIN)],
    squash: &[s(0.0, 1.03, 0.96, LIN)],
    faces: &[],
    blinks: &[2.3],
    whirls: &[],
};

/// Nodding off and catching itself.
pub const DROWSY_NOD: Clip = Clip {
    name: "drowsy_nod",
    duration: 3.0,
    rot: &[
        r(0.0, 20.0, 0.0, 4.0, IO),
        r(1.6, 28.0, 0.0, 6.0, IN),
        r(1.9, 12.0, 0.0, 2.0, OUT),
        r(3.0, 20.0, 0.0, 4.0, LIN),
    ],
    pos: &[p(0.0, 0.0, 4.0, LIN)],
    squash: &[],
    faces: &[],
    blinks: &[],
    whirls: &[],
};

/// A long sigh and a slump.
pub const BORED_SIGH: Clip = Clip {
    name: "bored_sigh",
    duration: 4.5,
    rot: &[
        r(0.0, 6.0, 0.0, 0.0, SINE),
        r(1.4, -4.0, 6.0, 3.0, SINE),
        r(2.6, 12.0, -8.0, -4.0, SINE),
        r(4.5, 6.0, 0.0, 0.0, LIN),
    ],
    pos: &[
        p(0.0, 0.0, 2.0, SINE),
        p(1.4, 0.0, -1.0, SINE),
        p(2.6, 0.0, 4.0, SINE),
        p(4.5, 0.0, 2.0, LIN),
    ],
    squash: &[
        s(0.0, 1.0, 1.0, SINE),
        s(1.4, 0.97, 1.05, SINE),
        s(2.6, 1.07, 0.92, SINE),
        s(4.5, 1.0, 1.0, LIN),
    ],
    faces: &[],
    blinks: &[],
    whirls: &[],
};

// Done.

/// A squash, a jump, a squash.
pub const DONE_HOP: Clip = Clip {
    name: "done_hop",
    duration: 2.0,
    rot: &[
        r(0.35, 0.0, 0.0, 0.0, IO),
        r(0.65, -10.0, 0.0, 8.0, IO),
        r(0.95, 0.0, 0.0, 0.0, LIN),
    ],
    pos: &[
        p(0.35, 0.0, 0.0, OUT),
        p(0.65, 0.0, -20.0, IN),
        p(0.95, 0.0, 0.0, LIN),
    ],
    squash: &[
        s(0.0, 1.0, 1.0, IO),
        s(0.25, 1.12, 0.86, OUT),
        s(0.42, 0.9, 1.12, IO),
        s(0.8, 1.0, 1.0, OUT),
        s(0.97, 1.1, 0.9, OUT),
        s(1.3, 1.0, 1.0, LIN),
    ],
    faces: &[],
    blinks: &[1.7],
    whirls: &[],
};

/// Two little bounces.
pub const DONE_BOUNCE: Clip = Clip {
    name: "done_bounce",
    duration: 1.6,
    rot: &[],
    pos: &[
        p(0.0, 0.0, 0.0, OUT),
        p(0.2, 0.0, -10.0, IN),
        p(0.4, 0.0, 0.0, OUT),
        p(0.6, 0.0, -7.0, IN),
        p(0.8, 0.0, 0.0, LIN),
    ],
    squash: &[
        s(0.0, 1.0, 1.0, OUT),
        s(0.05, 1.08, 0.92, OUT),
        s(0.15, 0.95, 1.06, IO),
        s(0.4, 1.1, 0.9, OUT),
        s(0.55, 0.96, 1.05, IO),
        s(0.8, 1.08, 0.92, OUT),
        s(1.1, 1.0, 1.0, LIN),
    ],
    faces: &[],
    blinks: &[],
    whirls: &[],
};

/// Happy wiggles.
pub const DONE_WIGGLE: Clip = Clip {
    name: "done_wiggle",
    duration: 2.0,
    rot: &[
        r(0.0, 0.0, 0.0, 0.0, IO),
        r(0.2, -4.0, -8.0, -12.0, IO),
        r(0.45, -4.0, 8.0, 12.0, IO),
        r(0.7, -4.0, -8.0, -12.0, IO),
        r(0.95, -4.0, 8.0, 12.0, IO),
        r(1.3, 0.0, 0.0, 0.0, LIN),
    ],
    pos: &[],
    squash: &[],
    faces: &[],
    blinks: &[],
    whirls: &[],
};

/// Two full twirls with a hop.
pub const CELEBRATE_TWIRL: Clip = Clip {
    name: "celebrate_twirl",
    duration: 3.0,
    rot: &[
        r(0.0, 0.0, 0.0, 0.0, ANT),
        r(1.4, -8.0, 720.0, 0.0, OUT),
        r(1.9, 0.0, 720.0, 0.0, LIN),
    ],
    pos: &[
        p(0.2, 0.0, 0.0, OUT),
        p(0.7, 0.0, -18.0, IN),
        p(1.25, 0.0, 0.0, LIN),
    ],
    squash: &[
        s(0.0, 1.0, 1.0, OUT),
        s(0.15, 1.1, 0.9, IO),
        s(0.35, 0.93, 1.08, IO),
        s(1.2, 1.0, 1.0, OUT),
        s(1.32, 1.1, 0.9, OUT),
        s(1.65, 1.0, 1.0, LIN),
    ],
    faces: &[],
    blinks: &[2.4],
    whirls: &[(0.25, 1.4)],
};

/// Two nods.
pub const APPROVE_NOD: Clip = Clip {
    name: "approve_nod",
    duration: 1.8,
    rot: &[
        r(0.0, 0.0, 0.0, 0.0, OUT),
        r(0.25, 14.0, 0.0, 0.0, IO),
        r(0.55, -2.0, 0.0, 0.0, OUT),
        r(0.8, 14.0, 0.0, 0.0, IO),
        r(1.15, 0.0, 0.0, 0.0, LIN),
    ],
    pos: &[],
    squash: &[],
    faces: &[f(0.1, E::Happy), f(1.3, E::Neutral)],
    blinks: &[],
    whirls: &[],
};

// Moods.

/// Chin up, puffed, a slow look around.
pub const PROUD_PUFF: Clip = Clip {
    name: "proud_puff",
    duration: 4.0,
    rot: &[
        r(0.0, 0.0, 0.0, 0.0, POP),
        r(0.6, -12.0, -6.0, 0.0, SINE),
        r(2.0, -12.0, 8.0, 2.0, SINE),
        r(3.3, -12.0, -6.0, 0.0, IO),
        r(4.0, 0.0, 0.0, 0.0, LIN),
    ],
    pos: &[],
    squash: &[
        s(0.0, 1.0, 1.0, POP),
        s(0.6, 1.05, 1.06, SINE),
        s(3.3, 1.05, 1.06, IO),
        s(4.0, 1.0, 1.0, LIN),
    ],
    faces: &[],
    blinks: &[2.6],
    whirls: &[],
};

/// Bops side to side.
pub const PLAY_BOP: Clip = Clip {
    name: "play_bop",
    duration: 2.4,
    rot: &[
        r(0.0, 0.0, 0.0, 0.0, OUT),
        r(0.3, 0.0, -20.0, -8.0, IO),
        r(0.9, 0.0, -20.0, -8.0, OUT),
        r(1.2, 0.0, 20.0, 8.0, IO),
        r(1.8, 0.0, 20.0, 8.0, OUT),
        r(2.4, 0.0, 0.0, 0.0, LIN),
    ],
    pos: &[
        p(0.0, 0.0, 0.0, OUT),
        p(0.15, -4.0, -8.0, IN),
        p(0.3, -8.0, 0.0, IO),
        p(0.9, -8.0, 0.0, OUT),
        p(1.05, 0.0, -8.0, IN),
        p(1.2, 8.0, 0.0, IO),
        p(1.8, 8.0, 0.0, OUT),
        p(2.1, 0.0, -6.0, IN),
        p(2.4, 0.0, 0.0, LIN),
    ],
    squash: &[
        s(0.2, 1.0, 1.0, IN),
        s(0.3, 1.08, 0.93, OUT),
        s(0.45, 1.0, 1.0, LIN),
        s(1.2, 1.08, 0.93, OUT),
        s(1.35, 1.0, 1.0, LIN),
    ],
    faces: &[],
    blinks: &[],
    whirls: &[],
};

/// Turns away and looks down, peeks back.
pub const SHY_HIDE: Clip = Clip {
    name: "shy_hide",
    duration: 4.0,
    rot: &[
        r(0.0, 0.0, 0.0, 0.0, OUT),
        r(0.6, 12.0, 34.0, 8.0, SINE),
        r(2.2, 12.0, 30.0, 8.0, IO),
        r(2.6, 4.0, 10.0, 2.0, IO),
        r(3.2, 12.0, 32.0, 8.0, IO),
        r(4.0, 0.0, 0.0, 0.0, LIN),
    ],
    pos: &[
        p(0.0, 0.0, 0.0, OUT),
        p(0.6, 3.0, 2.0, IO),
        p(3.2, 3.0, 2.0, IO),
        p(4.0, 0.0, 0.0, LIN),
    ],
    squash: &[],
    faces: &[],
    blinks: &[1.4, 2.9],
    whirls: &[],
};

/// Shivers, crouched.
pub const SCARED_SHIVER: Clip = Clip {
    name: "scared_shiver",
    duration: 1.2,
    rot: &[
        r(0.0, 6.0, -3.0, -3.0, LIN),
        r(0.1, 6.0, 3.0, 3.0, LIN),
        r(0.2, 6.0, -3.0, -3.0, LIN),
        r(0.3, 6.0, 3.0, 3.0, LIN),
        r(0.4, 6.0, -3.0, -3.0, LIN),
        r(0.5, 6.0, 3.0, 3.0, LIN),
        r(0.6, 6.0, -3.0, -3.0, LIN),
        r(0.7, 6.0, 3.0, 3.0, LIN),
        r(0.8, 6.0, -3.0, -3.0, LIN),
        r(0.9, 6.0, 3.0, 3.0, LIN),
        r(1.0, 6.0, -3.0, -3.0, LIN),
        r(1.1, 6.0, 3.0, 3.0, LIN),
        r(1.2, 6.0, -3.0, -3.0, LIN),
    ],
    pos: &[
        p(0.0, -1.5, 4.0, LIN),
        p(0.05, 1.5, 4.0, LIN),
        p(0.1, -1.5, 4.0, LIN),
        p(0.15, 1.5, 4.0, LIN),
        p(0.2, -1.5, 4.0, LIN),
        p(0.25, 1.5, 4.0, LIN),
        p(0.3, -1.5, 4.0, LIN),
        p(0.35, 1.5, 4.0, LIN),
        p(0.4, -1.5, 4.0, LIN),
        p(0.45, 1.5, 4.0, LIN),
        p(0.5, -1.5, 4.0, LIN),
        p(0.55, 1.5, 4.0, LIN),
        p(0.6, -1.5, 4.0, LIN),
        p(0.65, 1.5, 4.0, LIN),
        p(0.7, -1.5, 4.0, LIN),
        p(0.75, 1.5, 4.0, LIN),
        p(0.8, -1.5, 4.0, LIN),
        p(0.85, 1.5, 4.0, LIN),
        p(0.9, -1.5, 4.0, LIN),
        p(0.95, 1.5, 4.0, LIN),
        p(1.0, -1.5, 4.0, LIN),
        p(1.05, 1.5, 4.0, LIN),
        p(1.1, -1.5, 4.0, LIN),
        p(1.15, 1.5, 4.0, LIN),
        p(1.2, -1.5, 4.0, LIN),
    ],
    squash: &[s(0.0, 1.05, 0.92, LIN)],
    faces: &[],
    blinks: &[],
    whirls: &[],
};

/// Three quick bounces.
pub const EXCITED_BOUNCE: Clip = Clip {
    name: "excited_bounce",
    duration: 1.5,
    rot: &[
        r(0.0, 0.0, 0.0, 0.0, IO),
        r(0.2, -6.0, 0.0, -6.0, IO),
        r(0.5, -6.0, 0.0, 6.0, IO),
        r(0.8, -6.0, 0.0, -6.0, IO),
        r(1.1, 0.0, 0.0, 0.0, LIN),
    ],
    pos: &[
        p(0.0, 0.0, 0.0, OUT),
        p(0.15, 0.0, -12.0, IN),
        p(0.3, 0.0, 0.0, OUT),
        p(0.45, 0.0, -12.0, IN),
        p(0.6, 0.0, 0.0, OUT),
        p(0.75, 0.0, -12.0, IN),
        p(0.9, 0.0, 0.0, LIN),
    ],
    squash: &[
        s(0.0, 1.0, 1.0, OUT),
        s(0.04, 1.1, 0.9, OUT),
        s(0.1, 0.95, 1.07, IO),
        s(0.3, 1.1, 0.9, OUT),
        s(0.4, 0.95, 1.07, IO),
        s(0.6, 1.1, 0.9, OUT),
        s(0.7, 0.95, 1.07, IO),
        s(0.9, 1.1, 0.9, OUT),
        s(1.15, 1.0, 1.0, LIN),
    ],
    faces: &[],
    blinks: &[],
    whirls: &[],
};

/// Shakes with laughter.
pub const LAUGH_SHAKE: Clip = Clip {
    name: "laugh_shake",
    duration: 1.6,
    rot: &[
        r(0.0, -6.0, 0.0, -4.0, SINE),
        r(0.2, -8.0, 0.0, 4.0, SINE),
        r(0.4, -6.0, 0.0, -4.0, SINE),
        r(0.6, -8.0, 0.0, 4.0, SINE),
        r(0.8, -6.0, 0.0, -4.0, SINE),
        r(1.0, -8.0, 0.0, 4.0, SINE),
        r(1.2, -6.0, 0.0, -4.0, SINE),
        r(1.4, -8.0, 0.0, 4.0, SINE),
        r(1.6, -6.0, 0.0, -4.0, LIN),
    ],
    pos: &[
        p(0.0, 0.0, 0.0, SINE),
        p(0.1, 0.0, -2.5, SINE),
        p(0.2, 0.0, 0.0, SINE),
        p(0.3, 0.0, -2.5, SINE),
        p(0.4, 0.0, 0.0, SINE),
        p(0.5, 0.0, -2.5, SINE),
        p(0.6, 0.0, 0.0, SINE),
        p(0.7, 0.0, -2.5, SINE),
        p(0.8, 0.0, 0.0, SINE),
        p(0.9, 0.0, -2.5, SINE),
        p(1.0, 0.0, 0.0, SINE),
        p(1.1, 0.0, -2.5, SINE),
        p(1.2, 0.0, 0.0, SINE),
        p(1.3, 0.0, -2.5, SINE),
        p(1.4, 0.0, 0.0, SINE),
        p(1.5, 0.0, -2.5, SINE),
        p(1.6, 0.0, 0.0, LIN),
    ],
    squash: &[],
    faces: &[],
    blinks: &[],
    whirls: &[],
};

/// Stomps twice and shakes its head.
pub const ANGRY_STOMP: Clip = Clip {
    name: "angry_stomp",
    duration: 1.8,
    rot: &[
        r(0.0, 6.0, 0.0, 0.0, IO),
        r(0.9, 6.0, 0.0, 0.0, IO),
        r(1.0, 6.0, -12.0, 0.0, IO),
        r(1.15, 6.0, 12.0, 0.0, IO),
        r(1.3, 6.0, -10.0, 0.0, IO),
        r(1.45, 6.0, 8.0, 0.0, IO),
        r(1.6, 6.0, 0.0, 0.0, LIN),
    ],
    pos: &[
        p(0.0, 0.0, 0.0, OUT),
        p(0.15, 0.0, -6.0, IN),
        p(0.3, 0.0, 0.0, OUT),
        p(0.45, 0.0, -6.0, IN),
        p(0.6, 0.0, 0.0, LIN),
    ],
    squash: &[
        s(0.28, 1.0, 1.0, OUT),
        s(0.32, 1.14, 0.84, OUT),
        s(0.44, 1.0, 1.0, OUT),
        s(0.58, 1.0, 1.0, OUT),
        s(0.62, 1.14, 0.84, OUT),
        s(0.8, 1.0, 1.0, LIN),
    ],
    faces: &[],
    blinks: &[],
    whirls: &[],
};

/// Tilts one way, then the other.
pub const CONFUSED_TILT: Clip = Clip {
    name: "confused_tilt",
    duration: 4.0,
    rot: &[
        r(0.0, 0.0, 0.0, 0.0, POP),
        r(0.5, -4.0, -8.0, -14.0, SINE),
        r(1.7, -4.0, -8.0, -14.0, POP),
        r(2.2, -4.0, 8.0, 14.0, SINE),
        r(3.4, -4.0, 8.0, 14.0, IO),
        r(4.0, 0.0, 0.0, 0.0, LIN),
    ],
    pos: &[],
    squash: &[],
    faces: &[],
    blinks: &[1.0, 2.8],
    whirls: &[],
};

/// Tilts and leans in.
pub const CURIOUS_TILT: Clip = Clip {
    name: "curious_tilt",
    duration: 4.0,
    rot: &[
        r(0.0, 0.0, 0.0, 0.0, POP),
        r(0.6, -8.0, 12.0, 16.0, SINE),
        r(2.6, -6.0, 16.0, 14.0, IO),
        r(3.4, 0.0, 0.0, 0.0, LIN),
    ],
    pos: &[
        p(0.0, 0.0, 0.0, POP),
        p(0.6, 4.0, -2.0, SINE),
        p(2.6, 4.0, -2.0, IO),
        p(3.4, 0.0, 0.0, LIN),
    ],
    squash: &[],
    faces: &[],
    blinks: &[1.5, 3.6],
    whirls: &[],
};

/// A slow side-eye.
pub const SUSPICIOUS_SIDE: Clip = Clip {
    name: "suspicious_side",
    duration: 5.0,
    rot: &[
        r(0.0, 0.0, 0.0, 0.0, SINE),
        r(1.4, 4.0, -24.0, -3.0, SINE),
        r(2.4, 4.0, -24.0, -3.0, SINE),
        r(3.8, 4.0, 18.0, 2.0, SINE),
        r(5.0, 0.0, 0.0, 0.0, LIN),
    ],
    pos: &[],
    squash: &[],
    faces: &[],
    blinks: &[],
    whirls: &[],
};

/// A slow, calm sway.
pub const HUM_SWAY: Clip = Clip {
    name: "hum_sway",
    duration: 6.0,
    rot: &[
        r(0.0, 0.0, 0.0, 0.0, SINE),
        r(1.5, -3.0, 6.0, -7.0, SINE),
        r(4.5, -3.0, -6.0, 7.0, SINE),
        r(6.0, 0.0, 0.0, 0.0, LIN),
    ],
    pos: &[
        p(0.0, 0.0, 0.0, SINE),
        p(1.5, -4.0, 0.0, SINE),
        p(4.5, 4.0, 0.0, SINE),
        p(6.0, 0.0, 0.0, LIN),
    ],
    squash: &[],
    faces: &[],
    blinks: &[3.0],
    whirls: &[],
};

/// A startled jump.
pub const NOTIFY_JUMP: Clip = Clip {
    name: "notify_jump",
    duration: 2.0,
    rot: &[
        r(0.0, 0.0, 0.0, 0.0, OUT),
        r(0.3, -10.0, 0.0, 0.0, IO),
        r(0.8, 0.0, 0.0, 0.0, LIN),
    ],
    pos: &[
        p(0.0, 0.0, 0.0, OUT),
        p(0.28, 0.0, -18.0, IN),
        p(0.56, 0.0, 0.0, LIN),
    ],
    squash: &[
        s(0.0, 1.0, 1.0, IO),
        s(0.12, 0.92, 1.1, IO),
        s(0.3, 1.0, 1.0, OUT),
        s(0.58, 1.12, 0.88, OUT),
        s(0.9, 1.0, 1.0, LIN),
    ],
    faces: &[],
    blinks: &[1.4],
    whirls: &[],
};

/// Dangles like a pendulum.
pub const DRAG_DANGLE: Clip = Clip {
    name: "drag_dangle",
    duration: 2.4,
    rot: &[
        r(0.0, 0.0, 0.0, -18.0, SINE),
        r(1.2, 0.0, 0.0, 18.0, SINE),
        r(2.4, 0.0, 0.0, -18.0, LIN),
    ],
    pos: &[
        p(0.0, -6.0, -6.0, SINE),
        p(1.2, 6.0, -6.0, SINE),
        p(2.4, -6.0, -6.0, LIN),
    ],
    squash: &[s(0.0, 0.94, 1.08, LIN)],
    faces: &[],
    blinks: &[],
    whirls: &[],
};

/// Stretches awake.
pub const WAKE_STRETCH: Clip = Clip {
    name: "wake_stretch",
    duration: 2.0,
    rot: &[
        r(0.0, 16.0, 0.0, 4.0, POP),
        r(0.5, -10.0, 0.0, 0.0, IO),
        r(1.2, 0.0, 0.0, 0.0, LIN),
    ],
    pos: &[
        p(0.0, 0.0, 3.0, POP),
        p(0.5, 0.0, -4.0, IO),
        p(1.2, 0.0, 0.0, LIN),
    ],
    squash: &[
        s(0.0, 1.0, 1.0, OUT),
        s(0.5, 0.92, 1.14, IO),
        s(1.0, 1.04, 0.96, IO),
        s(1.4, 1.0, 1.0, LIN),
    ],
    faces: &[],
    blinks: &[1.5],
    whirls: &[],
};

// Showing off.

/// One clean turn, then still.
pub const LOGO_SPIN: Clip = Clip {
    name: "logo_spin",
    duration: 4.0,
    rot: &[
        r(0.0, -8.0, 0.0, 0.0, IO),
        r(1.8, -8.0, 360.0, 0.0, LIN),
        r(4.0, -8.0, 360.0, 0.0, LIN),
    ],
    pos: &[],
    squash: &[],
    faces: &[],
    blinks: &[2.6],
    whirls: &[],
};

/// Every clip, for galleries and tests.
pub const ALL: &[&Clip] = &[
    &IDLE_BREEZE,
    &IDLE_GLANCE,
    &IDLE_TURN,
    &IDLE_PEEK,
    &LISTEN_LEAN,
    &STREAM_SWAY,
    &STREAM_BOB,
    &STREAM_TWIRL,
    &WORK_SCAN,
    &WORK_FLIP,
    &WORK_TINKER,
    &WORK_SPIN,
    &WORK_PONDER,
    &WORK_HOP,
    &BLOCKED_BONK,
    &SAD_DROOP,
    &DROWSY_NOD,
    &BORED_SIGH,
    &DONE_HOP,
    &DONE_BOUNCE,
    &DONE_WIGGLE,
    &CELEBRATE_TWIRL,
    &APPROVE_NOD,
    &PROUD_PUFF,
    &PLAY_BOP,
    &SHY_HIDE,
    &SCARED_SHIVER,
    &EXCITED_BOUNCE,
    &LAUGH_SHAKE,
    &ANGRY_STOMP,
    &CONFUSED_TILT,
    &CURIOUS_TILT,
    &SUSPICIOUS_SIDE,
    &HUM_SWAY,
    &NOTIFY_JUMP,
    &DRAG_DANGLE,
    &WAKE_STRETCH,
    &LOGO_SPIN,
];

#[cfg(test)]
mod tests {
    use super::ALL;

    #[test]
    fn every_clip_is_well_formed() {
        for clip in ALL {
            assert!(clip.duration > 0.0, "{}", clip.name);
            for keys in [clip.pos, clip.squash] {
                assert!(
                    keys.windows(2).all(|w| w[0].t < w[1].t),
                    "{} keys out of order",
                    clip.name
                );
                assert!(
                    keys.iter().all(|k| k.t <= clip.duration + 1e-9),
                    "{} key past its end",
                    clip.name
                );
            }
            assert!(
                clip.rot.windows(2).all(|w| w[0].t < w[1].t),
                "{} rot out of order",
                clip.name
            );
            assert!(
                clip.rot.iter().all(|k| k.t <= clip.duration + 1e-9),
                "{} rot past its end",
                clip.name
            );
            for k in clip.rot {
                for v in k.v {
                    assert!(
                        (v % 360.0).abs() < 1e-9 || v.abs() < 90.0,
                        "{} turn must end whole",
                        clip.name
                    );
                }
            }
            // Played once into another state, so it may start away from where it ends.
            if clip.name == "wake_stretch" {
                continue;
            }
            let end = clip.sample(clip.duration);
            let start = clip.sample(0.0);
            for i in 0..3 {
                let d = (end.rot[i] - start.rot[i]).rem_euclid(360.0);
                assert!(
                    d < 1e-6 || (360.0 - d) < 1e-6,
                    "{} loops on a seam: {:?} -> {:?}",
                    clip.name,
                    start.rot,
                    end.rot
                );
            }
            for i in 0..2 {
                assert!(
                    (end.pos[i] - start.pos[i]).abs() < 1e-6,
                    "{} pos seam",
                    clip.name
                );
                assert!(
                    (end.squash[i] - start.squash[i]).abs() < 1e-6,
                    "{} squash seam",
                    clip.name
                );
            }
            for &b in clip.blinks {
                assert!(b > 0.0 && b < clip.duration, "{} blink outside", clip.name);
            }
        }
    }
}
