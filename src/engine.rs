use std::f64::consts::{PI, TAU};

use crate::expression::EyeShape;
use crate::types::{BotAvatarSquashEase, BotAvatarState};

pub const STATES: [BotAvatarState; 3] = [
    BotAvatarState::Default,
    BotAvatarState::Working,
    BotAvatarState::Sleeping,
];

const HOP_T: f64 = 0.68;
const HOP_SPIN_H: f64 = 26.0;
const FLIP_PRE: f64 = 0.2;
#[allow(dead_code)]
const FLIP_POST: f64 = 0.66;

const DEG: f64 = PI / 180.0;
const SWITCH_FROM_SLEEP: f64 = 1.0;

const GAZE_YAW: f64 = 35.0 * DEG;
const GAZE_PITCH: f64 = 14.0 * DEG;
const GAZE_ROLL: f64 = 3.2 * DEG;
const GAZE_HOLD_MIN: f64 = 2.6;
const GAZE_HOLD_MAX: f64 = 4.4;

/// Configuration for idle flips and click jumps.
#[derive(Debug, Clone, Copy)]
pub struct JumpConfig {
    pub height: f64,
    pub time: f64,
    pub stretch: f64,
    pub squash: f64,
    pub squash_time: f64,
    pub squash_ease: BotAvatarSquashEase,
    pub ground_time: f64,
    pub ground_ease: BotAvatarSquashEase,
    pub rise_time: f64,
    pub rise_ease: BotAvatarSquashEase,
    pub click_squash_time: f64,
    pub spin: f64,
    pub lean: f64,
    pub every: f64,
    pub land: f64,
}

pub const JUMP_DEFAULTS: JumpConfig = JumpConfig {
    height: HOP_SPIN_H,
    time: HOP_T,
    stretch: 1.0,
    squash: 1.15,
    squash_time: 0.37,
    squash_ease: BotAvatarSquashEase::Pulse,
    ground_time: 0.11,
    ground_ease: BotAvatarSquashEase::Pulse,
    rise_time: 0.33,
    rise_ease: BotAvatarSquashEase::Pulse,
    click_squash_time: 0.24,
    spin: 1.0,
    lean: 6.0,
    every: 8.0,
    land: 0.0,
};

impl Default for JumpConfig {
    fn default() -> Self {
        JUMP_DEFAULTS
    }
}

#[inline]
const fn switch_to(state: BotAvatarState) -> f64 {
    match state.base() {
        BotAvatarState::Working => 0.7,
        BotAvatarState::Sleeping => 1.4,
        _ => 1.2,
    }
}

#[inline]
const fn squash_peak(ease: BotAvatarSquashEase) -> f64 {
    match ease {
        BotAvatarSquashEase::Sharp => 0.0,
        BotAvatarSquashEase::Pulse => 2.0 / 7.0,
        BotAvatarSquashEase::Soft => 0.5,
        BotAvatarSquashEase::Bouncy => 0.144,
    }
}

#[inline]
fn flip_pre(j: &JumpConfig, poked: bool) -> f64 {
    if poked {
        j.click_squash_time
    } else {
        FLIP_PRE * j.time
    }
}

#[inline]
fn flip_duration(j: &JumpConfig, poked: bool) -> f64 {
    flip_pre(j, poked)
        + j.time
        + squash_peak(j.squash_ease)
            * if poked {
                j.click_squash_time
            } else {
                j.squash_time
            }
        + j.ground_time.max(0.0)
        + j.rise_time
        + j.land.max(0.0)
        + 0.05
}

fn rise_pulse(v: f64, ease: BotAvatarSquashEase) -> f64 {
    if v <= 0.0 {
        return 1.0;
    }
    if v >= 1.0 {
        return 0.0;
    }
    match ease {
        BotAvatarSquashEase::Sharp => (1.0 - v) * (1.0 - v),
        BotAvatarSquashEase::Soft => 0.5 + 0.5 * (PI * v).cos(),
        BotAvatarSquashEase::Bouncy => (-3.2 * v).exp() * (5.4 * v).cos() - v * v * v * 0.026,
        BotAvatarSquashEase::Pulse => {
            let k = 4.2 * v;
            (1.0 + k) * (-k).exp() - v * v * v * 0.078
        }
    }
}

fn squash_pulse(u: f64, ease: BotAvatarSquashEase) -> f64 {
    if u <= 0.0 || u >= 1.0 || !u.is_finite() {
        return 0.0;
    }
    let x = match ease {
        BotAvatarSquashEase::Sharp => (1.0 - u) * (1.0 - u),
        BotAvatarSquashEase::Soft => {
            let s = (PI * u).sin();
            s * s
        }
        BotAvatarSquashEase::Bouncy => ((-3.15 * u).exp() * (8.43 * u).sin()) / 0.596,
        BotAvatarSquashEase::Pulse => {
            let k = 7.0 * u;
            (k * k * (2.0 - k).exp()) / 4.0
        }
    };
    let tail = if u > 0.85 {
        1.0 - (u - 0.85) / 0.15
    } else {
        1.0
    };
    x * tail * tail * (3.0 - 2.0 * tail)
}

#[inline]
fn ground_shape(v: f64, ease: BotAvatarSquashEase) -> f64 {
    1.0 + 0.25 * squash_pulse(v, ease)
}

#[inline]
fn hop_squash(a: f64) -> f64 {
    let d = a.abs().min((a - 1.0).abs()) / 0.11;
    (-d * d).exp()
}

/// Complete instant pose of the avatar rig.
#[derive(Debug, Clone, Copy)]
pub struct Pose {
    pub yaw: f64,
    pub pitch: f64,
    pub roll: f64,
    pub x: f64,
    pub y: f64,
    pub sx: f64,
    pub sy: f64,
    pub eye_open: f64,
    pub blink_l: f64,
    pub blink_r: f64,
    pub look_x: f64,
    pub look_y: f64,
    pub breath: f64,
    pub laugh: f64,
    pub whirl: f64,
    pub whirl_angle: f64,
    pub w: [f64; 3],
    /// Expression eyes from the motion layer, mixed over the classic ones by `eyes_mix`.
    pub eyes: [EyeShape; 2],
    /// 0 the classic eyes, 1 [`Pose::eyes`].
    pub eyes_mix: f64,
}

impl Default for Pose {
    fn default() -> Self {
        rest_pose(BotAvatarState::Default)
    }
}

/// Mulberry32 deterministic PRNG matching `rng()` in `engine.ts`.
#[derive(Debug, Clone, Copy)]
struct Rng {
    a: u32,
}

impl Rng {
    fn new(seed: i64) -> Self {
        let raw = (seed.wrapping_mul(0x9e37_79b1_i64)) as u32;
        Self {
            a: if raw == 0 { 1 } else { raw },
        }
    }

    #[inline]
    fn next(&mut self) -> f64 {
        self.a = self.a.wrapping_add(0x6d2b_79f5);
        let mut t = self.a;
        t = (t ^ (t >> 15)).wrapping_mul(t | 1);
        t ^= t.wrapping_add((t ^ (t >> 7)).wrapping_mul(t | 61));
        ((t ^ (t >> 14)) as f64) / 4_294_967_296.0
    }
}

#[inline]
fn approach(cur: f64, target: f64, rate: f64, dt: f64) -> f64 {
    cur + (target - cur) * (1.0 - (-rate * dt).exp())
}

#[inline]
fn ease_in_out(p: f64) -> f64 {
    if p < 0.5 {
        4.0 * p * p * p
    } else {
        1.0 - (-2.0 * p + 2.0).powi(3) / 2.0
    }
}

#[inline]
fn ease_sine(p: f64) -> f64 {
    0.5 - 0.5 * (PI * p).cos()
}

#[derive(Debug, Clone, Copy)]
struct Wander {
    value: f64,
    vel: f64,
    target: f64,
    next: f64,
    amp: f64,
    hold_min: f64,
    hold_max: f64,
    rate: f64,
    spring: bool,
}

impl Wander {
    fn new(amp: f64, hold_min: f64, hold_max: f64, rate: f64, spring: bool) -> Self {
        Self {
            value: 0.0,
            vel: 0.0,
            target: 0.0,
            next: 0.0,
            amp,
            hold_min,
            hold_max,
            rate,
            spring,
        }
    }

    fn update(&mut self, rand: &mut Rng, t: f64, dt: f64) {
        if t >= self.next {
            self.target = (rand.next() * 2.0 - 1.0) * self.amp;
            self.next = t + self.hold_min + rand.next() * (self.hold_max - self.hold_min);
        }
        if self.spring {
            let w = self.rate * 1.6;
            let z = 0.9;
            self.vel += (w * w * (self.target - self.value) - 2.0 * z * w * self.vel) * dt;
            self.value += self.vel * dt;
        } else {
            self.value = approach(self.value, self.target, self.rate, dt);
        }
    }

    fn aim(&mut self, v: f64) {
        self.target = v;
        self.next = f64::INFINITY;
    }

    fn set(&mut self, amp: f64, hold_min: f64, hold_max: f64, rate: f64) {
        self.amp = amp;
        self.hold_min = hold_min;
        self.hold_max = hold_max;
        self.rate = rate;
        self.next = 0.0;
    }
}

#[derive(Debug, Clone, Copy)]
struct Event {
    p: f64,
    duration: f64,
}

impl Event {
    fn new(duration: f64) -> Self {
        Self { p: -1.0, duration }
    }

    #[inline]
    fn fire(&mut self) {
        self.p = 0.0;
    }

    #[inline]
    fn active(&self) -> bool {
        self.p >= 0.0
    }

    fn update(&mut self, dt: f64) {
        if self.p < 0.0 {
            return;
        }
        if self.duration <= 0.0 {
            self.p = -1.0;
            return;
        }
        self.p += dt / self.duration;
        if self.p >= 1.0 {
            self.p = -1.0;
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct Rest {
    pitch: f64,
    roll: f64,
    y: f64,
    look_x: f64,
    look_y: f64,
}

const fn rest_for(state: BotAvatarState) -> Rest {
    match state.base() {
        BotAvatarState::Working => Rest {
            pitch: 5.0 * DEG,
            roll: 0.0,
            y: 0.0,
            look_x: 0.0,
            look_y: 0.0,
        },
        BotAvatarState::Sleeping => Rest {
            pitch: -16.0 * DEG,
            roll: 6.0 * DEG,
            y: 3.0,
            look_x: 0.0,
            look_y: 1.0,
        },
        _ => Rest {
            pitch: 0.0,
            roll: 0.0,
            y: 0.0,
            look_x: 0.0,
            look_y: 0.0,
        },
    }
}

/// How much of the rig moves. `Full` is the whole rig: wander, gaze turns, hops, flips. The
/// calm ones hold the body in its state's rest pose and move only the face.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Calm {
    #[default]
    Full,
    /// Only the eyes move, toward the pointer when there is one.
    Gaze,
    /// At work: the face plays [`Beat`]s one after another (glances around, happy closed
    /// eyes, a double blink, a little head tilt, a rare short hop, a rest), while the body breathes and the bot blinks now and then.
    /// A pointer takes the eyes over.
    Busy,
}

/// One beat of a working bot's face in [`Calm::Busy`].
#[derive(Debug, Clone, Copy, PartialEq)]
enum Beat {
    /// The eyes look to (x, y) and hold there.
    Glance(f64, f64),
    /// The eyes close into happy arcs (`laugh`), then open.
    Happy,
    /// Two quick blinks, eyes ahead.
    Blinks,
    /// The head tilts a little to one side (-1 or 1), the eyes with it, as if curious.
    Tilt(f64),
    /// A short low hop, with a little stretch, at most once every `HOP_GAP` seconds.
    Hop,
    /// Eyes ahead.
    Rest,
}

/// A busy bot's hop: its height in rig units (up is negative y), its length, and the least
/// time between two hops, so it stays rare.
const HOP_HEIGHT: f64 = 3.0;
const HOP_SECONDS: f64 = 0.4;
const HOP_GAP: f64 = 12.0;

impl Beat {
    fn same_kind(self, other: Beat) -> bool {
        std::mem::discriminant(&self) == std::mem::discriminant(&other)
    }
}

/// Rig simulator advancing a [`Pose`] through time.
#[derive(Debug, Clone)]
pub struct Sim {
    pub pose: Pose,
    pub state: BotAvatarState,

    rand: Rng,
    t: f64,
    w_from: [f64; 3],
    tr: f64,
    tr_duration: f64,
    yaw_w: Wander,
    pitch_w: Wander,
    roll_w: Wander,
    look_x_w: Wander,
    look_y_w: Wander,
    blink: Event,
    blink_at: f64,
    blink_again: bool,
    dart: Event,
    dart_at: f64,
    dart_x: f64,
    dart_y: f64,
    flip: Event,
    flip_poked: bool,
    jump: JumpConfig,
    flip_at: f64,
    flip_side: f64,
    nod: Event,
    nod_at: f64,
    hop_phase: f64,
    hop_count: u64,
    hop_gain: f64,
    laugh_ev: Event,
    laugh_at: f64,
    prev_yaw: f64,
    jelly: f64,
    jelly_v: f64,
    gaze_lead: f64,
    gaze_dir: [f64; 2],
    gaze_at: f64,
    turn_k: f64,
    breath_phase: f64,
    ptr_x: f64,
    ptr_y: f64,
    calm: Calm,
    beat: Beat,
    /// When the current beat ends.
    beat_end: f64,
    /// When the second blink of `Beat::Blinks` is due.
    blink_again_at: Option<f64>,
    /// The eyes and the happy arcs as the busy face eases them.
    busy_look: (f64, f64),
    busy_laugh: f64,
    busy_roll: f64,
    /// When the last hop began.
    hop_at: f64,
    ptr_s: f64,
    ptr_target_x: f64,
    ptr_target_y: f64,
    ptr_target_s: f64,
    base_yaw: f64,
    /// The motion layer drives the body: hold the rest pose and only breathe.
    quiet: bool,
}

impl Sim {
    pub fn new(seed: f64, state: BotAvatarState) -> Self {
        let seed_int = (seed * 1e6).floor() as i64 + 1;
        let mut rand = Rng::new(seed_int);
        let yaw_w = Wander::new(36.0 * DEG, 1.1, 2.6, 3.0, true);
        let pitch_w = Wander::new(10.0 * DEG, 1.1, 2.6, 2.6, true);
        let roll_w = Wander::new(5.0 * DEG, 1.6, 3.2, 2.0, true);
        let look_x_w = Wander::new(3.6, 0.5, 2.0, 14.0, false);
        let look_y_w = Wander::new(2.4, 0.5, 2.0, 14.0, false);

        let t = rand.next() * 10.0;
        let hop_phase = rand.next();
        let breath_phase = rand.next();
        let blink_at = t + 1.0 + rand.next() * 3.0;
        let jump = JUMP_DEFAULTS;
        let flip_at = if jump.every > 0.0 {
            t + jump.every * (0.625 + rand.next() * 0.75)
        } else {
            f64::INFINITY
        };
        let nod_at = t + 3.0 + rand.next() * 4.0;
        let dart_at = t + 1.0 + rand.next() * 2.0;
        let laugh_at = t + 0.6 + rand.next() * 1.5;

        let mut sim = Self {
            pose: Pose {
                yaw: 0.0,
                pitch: 0.0,
                roll: 0.0,
                x: 0.0,
                y: 0.0,
                sx: 1.0,
                sy: 1.0,
                eye_open: 1.0,
                blink_l: 0.0,
                blink_r: 0.0,
                look_x: 0.0,
                look_y: 0.0,
                breath: 0.0,
                laugh: 0.0,
                whirl: 0.0,
                whirl_angle: 0.0,
                w: [1.0, 0.0, 0.0],
                eyes: [EyeShape::OPEN; 2],
                eyes_mix: 0.0,
            },
            state: BotAvatarState::Default,
            rand,
            t,
            w_from: [1.0, 0.0, 0.0],
            tr: 1.0,
            tr_duration: 1.2,
            yaw_w,
            pitch_w,
            roll_w,
            look_x_w,
            look_y_w,
            blink: Event::new(0.17),
            blink_at,
            blink_again: false,
            dart: Event::new(0.12),
            dart_at,
            dart_x: 0.0,
            dart_y: 0.0,
            flip: Event::new(flip_duration(&JUMP_DEFAULTS, false)),
            flip_poked: false,
            jump,
            flip_at,
            flip_side: 1.0,
            nod: Event::new(1.7),
            nod_at,
            hop_phase,
            hop_count: 0,
            hop_gain: 0.0,
            laugh_ev: Event::new(0.8),
            laugh_at,
            prev_yaw: 0.0,
            jelly: 0.0,
            jelly_v: 0.0,
            gaze_lead: 0.0,
            gaze_dir: [0.0, 0.0],
            gaze_at: 0.0,
            turn_k: 1.0,
            breath_phase,
            ptr_x: 0.0,
            ptr_y: 0.0,
            calm: Calm::Full,
            beat: Beat::Rest,
            beat_end: 0.0,
            blink_again_at: None,
            busy_look: (0.0, 0.0),
            busy_laugh: 0.0,
            busy_roll: 0.0,
            hop_at: f64::NEG_INFINITY,
            ptr_s: 0.0,
            ptr_target_x: 0.0,
            ptr_target_y: 0.0,
            ptr_target_s: 0.0,
            base_yaw: 0.0,
            quiet: false,
        };
        sim.set_state(state, true);
        sim
    }

    /// Points the rig at `next`'s classic base ([`BotAvatarState::base`]).
    pub fn set_state(&mut self, next: BotAvatarState, immediate: bool) {
        let next = next.base();
        if next == self.state && !immediate {
            return;
        }
        let from = self.state;
        self.state = next;
        if immediate {
            for (i, &s) in STATES.iter().enumerate() {
                self.pose.w[i] = if s == next { 1.0 } else { 0.0 };
            }
            self.tr = 1.0;
        } else {
            self.w_from = self.pose.w;
            self.tr = 0.0;
            self.tr_duration = if from == BotAvatarState::Sleeping {
                SWITCH_FROM_SLEEP
            } else {
                switch_to(next)
            };
        }
        match next {
            BotAvatarState::Working => {
                self.yaw_w.set(16.0 * DEG, 0.9, 1.8, 4.0);
                self.pitch_w.set(3.0 * DEG, 1.2, 2.4, 3.0);
                self.roll_w.set(0.0, 1.0, 2.0, 3.0);
                self.look_x_w.set(2.0, 0.5, 1.2, 12.0);
                self.look_y_w.set(1.0, 0.5, 1.2, 12.0);
                self.hop_phase = 0.0;
                self.hop_count = 0;
                self.laugh_at = self.t + 0.5 + self.rand.next() * 1.2;
            }
            BotAvatarState::Sleeping => {
                self.yaw_w.set(7.0 * DEG, 3.0, 6.0, 0.7);
                self.pitch_w.set(3.0 * DEG, 3.0, 6.0, 0.7);
                self.roll_w.set(2.0 * DEG, 3.0, 6.0, 0.6);
                self.look_x_w.set(0.0, 2.0, 4.0, 2.0);
                self.look_y_w.set(0.0, 2.0, 4.0, 2.0);
                self.nod_at = self.t + 2.5 + self.rand.next() * 4.0;
            }
            _ => {
                self.yaw_w.set(GAZE_YAW, 2.6, 5.4, 2.0);
                self.pitch_w.set(GAZE_PITCH, 2.8, 5.8, 1.8);
                self.roll_w.set(GAZE_ROLL, 3.4, 6.6, 1.5);
                self.gaze_at = 0.0;
                self.gaze_dir = [0.0, 0.0];
                self.look_x_w.set(3.6, 0.6, 2.2, 13.0);
                self.look_y_w.set(2.4, 0.6, 2.2, 13.0);
                self.flip_at = self.next_flip(self.t, 0.6);
            }
        }
    }

    /// Sets how much of the rig moves; see [`Calm`].
    pub fn set_calm(&mut self, calm: Calm) {
        self.calm = calm;
    }

    /// Hands the body to the motion layer: the rig holds its rest pose, breathes, and lets
    /// the eyes follow the pointer.
    pub fn set_quiet(&mut self, quiet: bool) {
        self.quiet = quiet;
    }

    fn update_quiet(&mut self, dt: f64) {
        self.ptr_s = approach(self.ptr_s, self.ptr_target_s, 8.0, dt);
        self.ptr_x = approach(self.ptr_x, self.ptr_target_x, 14.0, dt);
        self.ptr_y = approach(self.ptr_y, self.ptr_target_y, 14.0, dt);
        let ps = self.ptr_s;
        let mut pose = rest_pose(self.state);
        self.breath_phase += dt / 3.6;
        let breath = (self.breath_phase * TAU).sin();
        pose.breath = breath;
        pose.sx += breath * 0.008;
        pose.sy += breath * 0.012;
        pose.look_x += 4.5 * self.ptr_x * ps;
        pose.look_y += 3.0 * self.ptr_y * ps;
        self.pose = pose;
    }

    /// The calm rig: the state's rest pose with the face alone moving.
    fn update_calm(&mut self, t: f64, dt: f64) {
        self.ptr_s = approach(self.ptr_s, self.ptr_target_s, 8.0, dt);
        self.ptr_x = approach(self.ptr_x, self.ptr_target_x, 14.0, dt);
        self.ptr_y = approach(self.ptr_y, self.ptr_target_y, 14.0, dt);
        let ps = self.ptr_s;
        let mut pose = rest_pose(self.state);
        if self.calm == Calm::Busy {
            if t >= self.beat_end {
                self.next_beat(t);
            }
            let (look, laugh, roll) = match self.beat {
                Beat::Glance(x, y) => ((x, y), 0.0, 0.0),
                Beat::Happy => ((0.0, 0.0), 1.0, 0.0),
                Beat::Tilt(side) => ((2.0 * side, -0.5), 0.0, 5.0 * DEG * side),
                Beat::Hop => ((0.0, -1.0), 0.0, 0.0),
                Beat::Blinks | Beat::Rest => ((0.0, 0.0), 0.0, 0.0),
            };
            let (x, y) = self.busy_look;
            self.busy_look = (approach(x, look.0, 9.0, dt), approach(y, look.1, 9.0, dt));
            self.busy_laugh = approach(self.busy_laugh, laugh, 10.0, dt);
            self.busy_roll = approach(self.busy_roll, roll, 6.0, dt);
            pose.look_x += self.busy_look.0 * (1.0 - ps);
            pose.look_y += self.busy_look.1 * (1.0 - ps);
            pose.laugh = self.busy_laugh;
            pose.roll += self.busy_roll;
            let q = (t - self.hop_at) / HOP_SECONDS;
            if (0.0..1.0).contains(&q) {
                let arc = (PI * q).sin();
                pose.y -= HOP_HEIGHT * arc;
                pose.sx *= 1.0 - 0.025 * arc;
                pose.sy *= 1.0 + 0.04 * arc;
            }

            self.breath_phase += dt / 3.6;
            let breath = (self.breath_phase * TAU).sin();
            pose.breath = breath;
            pose.sx += breath * 0.008;
            pose.sy += breath * 0.012;

            // Blinks now and then, and the second of a double blink; none while happy.
            let due = t >= self.blink_at || self.blink_again_at.is_some_and(|at| t >= at);
            if due && !self.blink.active() && self.beat != Beat::Happy {
                self.blink.fire();
                if self.blink_again_at.is_some_and(|at| t >= at) {
                    self.blink_again_at = None;
                } else {
                    self.blink_at = t + 2.2 + self.rand.next() * 2.6;
                }
            }
            self.blink.update(dt);
            if self.blink.active() {
                let close = (PI * self.blink.p).sin();
                pose.blink_l = close;
                pose.blink_r = close;
            }
        }
        pose.look_x += 4.5 * self.ptr_x * ps;
        pose.look_y += 3.0 * self.ptr_y * ps;
        self.pose = pose;
    }

    /// Picks the busy face's next beat at `t`: never the same kind twice in a row, and a
    /// glance looks to the other side from the last one.
    fn next_beat(&mut self, t: f64) {
        let last = self.beat;
        let next = loop {
            let p = self.rand.next();
            let beat = if p < 0.45 {
                let side = match last {
                    Beat::Glance(x, _) if x > 0.0 => -1.0,
                    Beat::Glance(..) => 1.0,
                    _ if self.rand.next() < 0.5 => -1.0,
                    _ => 1.0,
                };
                let x = side * (2.0 + self.rand.next() * 2.0);
                let y = (self.rand.next() * 2.0 - 1.0) * 1.5;
                Beat::Glance(x, y)
            } else if p < 0.66 {
                Beat::Happy
            } else if p < 0.76 {
                Beat::Blinks
            } else if p < 0.86 {
                Beat::Tilt(if self.rand.next() < 0.5 { -1.0 } else { 1.0 })
            } else if p < 0.93 {
                if t - self.hop_at < HOP_GAP {
                    continue;
                }
                Beat::Hop
            } else {
                Beat::Rest
            };
            if !beat.same_kind(last) || matches!(beat, Beat::Glance(..)) {
                break beat;
            }
        };
        let length = match next {
            Beat::Glance(..) => 0.9 + self.rand.next() * 0.7,
            Beat::Happy => 1.0 + self.rand.next() * 0.6,
            Beat::Blinks => 0.8,
            Beat::Tilt(_) => 1.2 + self.rand.next() * 0.6,
            Beat::Hop => HOP_SECONDS + 0.3,
            Beat::Rest => 0.6 + self.rand.next() * 0.6,
        };
        if next == Beat::Hop {
            self.hop_at = t;
        }
        if next == Beat::Blinks {
            self.blink_at = t;
            self.blink_again_at = Some(t + 0.28);
        }
        self.beat = next;
        self.beat_end = t + length;
    }

    pub fn set_pointer(&mut self, x: f64, y: f64, strength: f64) {
        self.ptr_target_x = x.clamp(-1.2, 1.2);
        self.ptr_target_y = y.clamp(-1.2, 1.2);
        self.ptr_target_s = strength.clamp(0.0, 1.0);
    }

    pub fn poke(&mut self) {
        if self.flip.active() && self.flip.p < 0.6 {
            return;
        }
        self.flip_poked = true;
        self.flip.duration = flip_duration(&self.jump, true);
        self.flip_side = if self.rand.next() < 0.5 { -1.0 } else { 1.0 };
        self.flip.fire();
        self.flip_at = self.next_flip(self.t, 1.1);
    }

    pub fn set_turn(&mut self, k: f64) {
        let next = k.max(0.0);
        if (next - self.turn_k).abs() < 1e-12 {
            return;
        }
        self.turn_k = next;
        if self.state == BotAvatarState::Default {
            self.gaze_at = 0.0;
        }
    }

    pub fn set_jump(&mut self, j: JumpConfig) {
        let prev_every = self.jump.every;
        self.jump = j;
        if (j.every - prev_every).abs() > 1e-12 {
            self.flip_at = self.next_flip(self.t, 1.0);
        }
    }

    pub fn jump_config(&self) -> JumpConfig {
        self.jump
    }

    fn next_gaze(&mut self) -> [f64; 2] {
        let [px, py] = self.gaze_dir;
        if px != 0.0 || py != 0.0 {
            let p = self.rand.next();
            if p < 0.66 {
                return [-px, -py];
            }
            if p < 0.85 {
                return [-px, py];
            }
            return [0.0, 0.0];
        }
        const CORNERS: [[f64; 2]; 4] = [[1.0, -1.0], [-1.0, 1.0], [-1.0, -1.0], [1.0, 1.0]];
        let idx = ((self.rand.next() * 4.0).floor() as usize).min(3);
        CORNERS[idx]
    }

    fn next_flip(&mut self, t: f64, k: f64) -> f64 {
        let every = self.jump.every;
        if every > 0.0 {
            t + every * k * (0.625 + self.rand.next() * 0.75)
        } else {
            f64::INFINITY
        }
    }

    pub fn update(&mut self, mut dt: f64) {
        if !dt.is_finite() || dt < 0.0 {
            dt = 0.0;
        }
        dt = dt.min(0.05);
        self.t += dt;
        let t = self.t;
        if self.quiet {
            self.update_quiet(dt);
            return;
        }
        if self.calm != Calm::Full {
            self.update_calm(t, dt);
            return;
        }

        if self.tr < 1.0 {
            self.tr = (self.tr + dt / self.tr_duration).min(1.0);
            let e = ease_sine(self.tr);
            for (i, &s) in STATES.iter().enumerate() {
                let target = if s == self.state { 1.0 } else { 0.0 };
                self.pose.w[i] = self.w_from[i] + (target - self.w_from[i]) * e;
            }
        }
        let [wd, ww, ws] = self.pose.w;

        let mut rest = Rest {
            pitch: 0.0,
            roll: 0.0,
            y: 0.0,
            look_x: 0.0,
            look_y: 0.0,
        };
        for (i, &s) in STATES.iter().enumerate() {
            let r = rest_for(s);
            let wi = self.pose.w[i];
            rest.pitch += r.pitch * wi;
            rest.roll += r.roll * wi;
            rest.y += r.y * wi;
            rest.look_x += r.look_x * wi;
            rest.look_y += r.look_y * wi;
        }

        if self.state == BotAvatarState::Default && t >= self.gaze_at {
            let [gx, gy] = self.next_gaze();
            self.gaze_dir = [gx, gy];
            let reach = 0.84 + self.rand.next() * 0.16;
            self.yaw_w.aim(gx * GAZE_YAW * reach * self.turn_k);
            self.pitch_w.aim(gy * GAZE_PITCH * reach);
            self.roll_w.aim(gx * GAZE_ROLL * reach * self.turn_k);
            self.gaze_at = t + GAZE_HOLD_MIN + self.rand.next() * (GAZE_HOLD_MAX - GAZE_HOLD_MIN);
        }

        self.yaw_w.update(&mut self.rand, t, dt);
        self.pitch_w.update(&mut self.rand, t, dt);
        self.roll_w.update(&mut self.rand, t, dt);
        self.look_x_w.update(&mut self.rand, t, dt);
        self.look_y_w.update(&mut self.rand, t, dt);

        self.ptr_s = approach(self.ptr_s, self.ptr_target_s, 8.0, dt);
        self.ptr_x = approach(self.ptr_x, self.ptr_target_x, 14.0, dt);
        self.ptr_y = approach(self.ptr_y, self.ptr_target_y, 14.0, dt);
        let ps = self.ptr_s;
        let quiet = 1.0 - 0.75 * ps;

        self.base_yaw = approach(
            self.base_yaw,
            self.yaw_w.value * quiet + 22.0 * DEG * self.ptr_x * ps,
            5.0,
            dt,
        );
        let base_pitch = rest.pitch + self.pitch_w.value * quiet - 12.0 * DEG * self.ptr_y * ps;
        let base_roll = rest.roll + self.roll_w.value * quiet;
        let base_y = rest.y;
        let base_look_x = rest.look_x + self.look_x_w.value * quiet + 4.5 * self.ptr_x * ps;
        let base_look_y = rest.look_y + self.look_y_w.value * quiet + 3.0 * self.ptr_y * ps;

        let mut spin = 0.0;
        let mut hop_y = 0.0;
        let mut sx = 1.0;
        let mut sy = 1.0;
        let mut pitch_add = 0.0;
        let mut roll_add = 0.0;
        let mut blink_close = 0.0;
        let mut look_x_add = 0.0;
        let mut look_y_add = 0.0;
        let mut laugh: f64 = 0.0;
        let mut whirl: f64 = 0.0;
        let mut whirl_angle = 0.0;

        let smooth = |a: f64, b: f64, v: f64| -> f64 {
            let x = ((v - a) / (b - a)).clamp(0.0, 1.0);
            x * x * (3.0 - 2.0 * x)
        };
        let envelope = |q: f64| smooth(0.1, 0.26, q) * (1.0 - smooth(0.66, 0.9, q));
        let ring_angle = |q: f64| TAU * (1.5 * q + 0.9 * ease_in_out(q));

        if t >= self.blink_at && !self.blink.active() && wd + ww > 0.5 {
            self.blink.fire();
            self.blink_again = !self.blink_again && self.rand.next() < 0.22;
            self.blink_at = t + if self.blink_again {
                0.28
            } else {
                2.2 + self.rand.next() * 2.6
            };
        }
        self.blink.update(dt);
        if self.blink.active() {
            blink_close = (PI * self.blink.p).sin();
        }

        if t >= self.dart_at && !self.dart.active() && wd + ww > 0.5 {
            self.dart.fire();
            self.dart_x = (self.rand.next() * 2.0 - 1.0) * 4.0;
            self.dart_y = (self.rand.next() * 2.0 - 1.0) * 2.0;
            self.dart.duration = 0.25 + self.rand.next() * 0.45;
            self.dart_at = t + 1.2 + self.rand.next() * 2.6;
        }
        self.dart.update(dt);
        if self.dart.active() {
            let q = self.dart.p;
            let hold = if q < 0.15 {
                q / 0.15
            } else if q > 0.8 {
                (1.0 - q) / 0.2
            } else {
                1.0
            };
            look_x_add += self.dart_x * hold * (wd + ww);
            look_y_add += self.dart_y * hold * (wd + ww);
        }

        if self.state == BotAvatarState::Default && t >= self.flip_at && !self.flip.active() {
            self.flip_poked = false;
            self.flip.duration = flip_duration(&self.jump, false);
            self.flip_side = if self.rand.next() < 0.5 { -1.0 } else { 1.0 };
            self.flip.fire();
            self.flip_at = self.next_flip(t, 1.0);
        }
        self.flip.update(dt);
        if self.flip.active() {
            let j = self.jump;
            let pre_s = flip_pre(&j, self.flip_poked);
            let a = (self.flip.p * self.flip.duration - pre_s) / j.time.max(1e-6);
            let q = a.clamp(0.0, 1.0);
            let arc = (PI * q).sin();
            spin += TAU * j.spin * ease_in_out(q);
            hop_y -= j.height * arc;

            let tl = (a - 1.0) * j.time - j.land;
            let squash_time = (if self.flip_poked {
                j.click_squash_time
            } else {
                j.squash_time
            })
            .max(1e-6);
            let crouch = |u: f64| {
                if self.flip_poked {
                    u * u * (3.0 - 2.0 * u)
                } else {
                    hop_squash(u - 1.0)
                }
            };
            let hold = j.ground_time.max(0.0);
            let peak_t = squash_peak(j.squash_ease) * squash_time;
            let rise = tl - peak_t - hold;
            let depth = if tl <= peak_t {
                squash_pulse(tl / squash_time, j.squash_ease)
            } else if rise <= 0.0 && hold > 0.0 {
                ground_shape((tl - peak_t) / hold, j.ground_ease)
            } else {
                rise_pulse(rise / j.rise_time.max(1e-6), j.rise_ease)
            };
            let land = (if a < 0.0 {
                let u = if pre_s > 0.0 {
                    (1.0 + (a * j.time) / pre_s).max(0.0)
                } else {
                    0.0
                };
                crouch(u)
            } else if tl > 0.0 {
                depth
            } else if a < 0.2 {
                hop_squash(a)
            } else {
                0.0
            }) * j.squash;

            sx += 0.16 * land - 0.06 * arc * j.stretch;
            sy += -0.18 * land + 0.09 * arc * j.stretch;
            roll_add += self.flip_side * j.lean * DEG * arc;
            if j.spin > 0.0 {
                laugh = laugh.max(arc);
                whirl = whirl.max(envelope(q));
                whirl_angle = ring_angle(q);
            }
        }

        let j = self.jump;
        let exit_hold = j.ground_time.max(0.0);
        let exit_for = exit_hold + j.rise_time;
        if self.state == BotAvatarState::Working {
            self.hop_gain = ww;
        }
        if self.hop_gain > 0.02 && (self.state == BotAvatarState::Working || self.hop_phase > 0.0) {
            self.hop_phase += dt / HOP_T;
            if self.hop_phase >= 1.0 {
                if self.state == BotAvatarState::Working {
                    self.hop_phase -= 1.0;
                    self.hop_count += 1;
                } else if (self.hop_phase - 1.0) * HOP_T >= exit_for {
                    self.hop_phase = 0.0;
                    self.hop_gain = 0.0;
                }
            }
            let g = self.hop_gain;
            let q = self.hop_phase.min(1.0);
            let arc = (PI * q).sin();
            let spinning = self.hop_count % 3 == 2;
            let h = if spinning { HOP_SPIN_H } else { 18.0 };
            hop_y -= h * arc * g;

            let exit_t = if self.state != BotAvatarState::Working && self.hop_phase > 1.0 {
                (self.hop_phase - 1.0) * HOP_T
            } else {
                -1.0
            };
            let land = if exit_t < 0.0 {
                hop_squash(self.hop_phase)
            } else if exit_t < exit_hold && exit_hold > 0.0 {
                ground_shape(exit_t / exit_hold, j.ground_ease)
            } else {
                rise_pulse((exit_t - exit_hold) / j.rise_time.max(1e-6), j.rise_ease)
            };
            sx += (0.16 * land - 0.06 * arc) * g;
            sy += (-0.18 * land + 0.09 * arc) * g;
            if spinning {
                spin += TAU * ease_in_out(q) * g;
                laugh = laugh.max(arc * g);
                let ew = envelope(q) * g;
                if ew > whirl {
                    whirl = ew;
                    whirl_angle = ring_angle(q);
                }
            }
            let lean_dir = if self.hop_count % 2 == 0 { 1.0 } else { -1.0 };
            roll_add += lean_dir * 6.0 * DEG * arc * g;
        }

        if self.state == BotAvatarState::Working && t >= self.laugh_at && !self.laugh_ev.active() {
            self.laugh_ev.fire();
            self.laugh_ev.duration = 0.6 + self.rand.next() * 0.5;
            self.laugh_at = t + 1.6 + self.rand.next() * 2.2;
        }
        self.laugh_ev.update(dt);
        if self.laugh_ev.active() {
            let q = self.laugh_ev.p;
            let l = if q < 0.18 {
                q / 0.18
            } else if q > 0.78 {
                (1.0 - q) / 0.22
            } else {
                1.0
            };
            laugh = laugh.max(l);
        }

        if self.state == BotAvatarState::Sleeping && t >= self.nod_at && !self.nod.active() {
            self.nod.fire();
            self.nod_at = t + 4.0 + self.rand.next() * 4.0;
        }
        self.nod.update(dt);
        if self.nod.active() {
            let q = self.nod.p;
            let dip = if q < 0.72 {
                ease_in_out(q / 0.72)
            } else {
                1.0 - ease_in_out((q - 0.72) / 0.28)
            };
            pitch_add -= 13.0 * DEG * dip * ws;
        }

        self.breath_phase += dt / (3.6 + 1.2 * ws);
        let breath = (self.breath_phase * TAU).sin();
        self.pose.breath = breath;
        sx += breath * (0.008 + 0.014 * ws);
        sy += breath * (0.012 + 0.02 * ws);
        let bob = (t * TAU / 3.4).sin() * 2.0 * (1.0 - ws);

        self.pose.yaw = self.base_yaw + spin;

        let mut dyaw = self.base_yaw - self.prev_yaw;
        dyaw = (((dyaw + PI) % TAU) + TAU) % TAU - PI;
        self.prev_yaw = self.base_yaw;
        let rate = if dt > 0.0 { dyaw.abs() / dt } else { 0.0 };
        let lead_target = if dt > 0.0 {
            ((dyaw / dt) * 2.4).clamp(-2.2, 2.2)
        } else {
            0.0
        };
        self.gaze_lead = approach(self.gaze_lead, lead_target, 9.0, dt);
        let jelly_target = (0.055 * rate).min(0.22);
        let omega = 16.0;
        let zeta = 0.45;
        self.jelly_v +=
            (omega * omega * (jelly_target - self.jelly) - 2.0 * zeta * omega * self.jelly_v) * dt;
        self.jelly += self.jelly_v * dt;
        let jelly = self.jelly.clamp(-0.08, 0.28) * 0.6;
        sx *= 1.0 + jelly;
        sy *= 1.0 - 0.55 * jelly;

        self.pose.pitch = base_pitch + pitch_add;
        self.pose.roll = base_roll + roll_add;
        self.pose.x = 0.0;
        self.pose.y = base_y + hop_y + bob;
        self.pose.sx = sx;
        self.pose.sy = sy;
        self.pose.eye_open = 1.0;
        self.pose.laugh = approach(self.pose.laugh, laugh, 30.0, dt);
        self.pose.blink_l = blink_close;
        self.pose.blink_r = blink_close;
        self.pose.look_x = base_look_x + look_x_add + self.gaze_lead;
        self.pose.look_y = base_look_y + look_y_add;
        self.pose.whirl = whirl;
        self.pose.whirl_angle = whirl_angle;
    }
}

/// The still pose of a state, for reduced motion and the first paint.
pub const fn rest_pose(state: BotAvatarState) -> Pose {
    let r = rest_for(state);
    let w = match state.base() {
        BotAvatarState::Working => [0.0, 1.0, 0.0],
        BotAvatarState::Sleeping => [0.0, 0.0, 1.0],
        _ => [1.0, 0.0, 0.0],
    };
    Pose {
        yaw: 0.0,
        pitch: r.pitch,
        roll: r.roll,
        x: 0.0,
        y: r.y,
        sx: 1.0,
        sy: 1.0,
        eye_open: 1.0,
        blink_l: 0.0,
        blink_r: 0.0,
        look_x: r.look_x,
        look_y: r.look_y,
        breath: 0.0,
        laugh: 0.0,
        whirl: 0.0,
        whirl_angle: 0.0,
        w,
        eyes: [EyeShape::OPEN; 2],
        eyes_mix: 0.0,
    }
}
