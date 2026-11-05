//! The motion layer over the classic rig.
//!
//! For each [`BotAvatarState`] past the classic three, a [`Spec`] names the clips to play
//! (shuffled per bot, never the same twice in a row, the first from a random point so bots
//! in one state never move in step), the face to hold, the glyph to fold into, and where
//! the state goes next. [`Stage`] plays it: clips with bridges between them, the face with
//! its blinks, pokes, a held look, and the turn toward the pointer. Pokes, looks, and a set
//! expression work over the classic states too.

use crate::clip::{ClipPlayer, ClipPose};
use crate::clips::*;
use crate::ease::{Bezier, Spring};
use crate::engine::Calm;
use crate::expression::{Expression, EyeShape, FaceAnim};
use crate::glyph::{Glyph, GlyphAnim, GlyphFrame};
use crate::poke::{Poke, Pokes};
use crate::types::BotAvatarState;

use crate::clip::Clip;

const DEG: f64 = std::f64::consts::PI / 180.0;

/// How one state moves.
#[derive(Debug, Clone, Copy)]
pub struct Spec {
    pub clips: &'static [&'static Clip],
    /// Clips for a bot under [`COMPACT_PX`], when the full set is too busy that small.
    pub compact: Option<&'static [&'static Clip]>,
    /// A face held for the whole state; the clips' own faces are skipped.
    pub face: Option<Expression>,
    pub glyph: Option<Glyph>,
    pub next: Option<BotAvatarState>,
    /// Seconds before `next`. Without it, `next` follows one pass through `clips`.
    pub hold: Option<f64>,
}

const fn spec(clips: &'static [&'static Clip]) -> Spec {
    Spec {
        clips,
        compact: None,
        face: None,
        glyph: None,
        next: None,
        hold: None,
    }
}

const fn faced(clips: &'static [&'static Clip], face: Expression) -> Spec {
    Spec {
        face: Some(face),
        ..spec(clips)
    }
}

const fn glyph(g: Glyph) -> Spec {
    Spec {
        glyph: Some(g),
        ..spec(&[&IDLE_BREEZE])
    }
}

/// Below this size a state may use its calmer compact clips.
pub const COMPACT_PX: u32 = 40;

const WORK: &[&Clip] = &[
    &WORK_SCAN,
    &WORK_FLIP,
    &WORK_TINKER,
    &WORK_SPIN,
    &WORK_PONDER,
    &WORK_HOP,
];
const CALM_WORK: &[&Clip] = &[&WORK_SCAN, &WORK_TINKER, &WORK_PONDER];

/// The motion for `state`, or `None` for the classic three.
pub fn spec_for(state: BotAvatarState) -> Option<Spec> {
    use BotAvatarState as S;
    use Expression as E;
    let work = Spec {
        compact: Some(CALM_WORK),
        ..spec(WORK)
    };
    Some(match state {
        S::Default | S::Working | S::Sleeping => return None,
        S::Listening => faced(&[&LISTEN_LEAN], E::Calm),
        S::Thinking => glyph(Glyph::Dots),
        S::Streaming => Spec {
            compact: Some(&[&STREAM_SWAY, &STREAM_BOB]),
            ..spec(&[&STREAM_SWAY, &STREAM_BOB, &STREAM_TWIRL])
        },
        S::Searching | S::Radar => glyph(Glyph::Radar),
        S::Locating => glyph(Glyph::Pin),
        S::Reading => spec(&[&WORK_SCAN]),
        S::Coding | S::Executing | S::Computing | S::Controlling => work,
        S::Waiting | S::Orbit => glyph(Glyph::Orbit),
        S::Blocked => Spec {
            next: Some(S::Drowsy),
            hold: Some(120.0),
            ..spec(&[&BLOCKED_BONK])
        },
        S::Finished => Spec {
            face: Some(E::Happy),
            next: Some(S::Default),
            ..spec(&[&DONE_BOUNCE, &DONE_HOP])
        },
        S::Failed => Spec {
            face: Some(E::Sad),
            next: Some(S::Default),
            ..spec(&[&SAD_DROOP])
        },
        S::Waking => Spec {
            face: Some(E::Surprised),
            next: Some(S::Default),
            hold: Some(1.8),
            ..spec(&[&WAKE_STRETCH])
        },
        S::Happy => faced(&[&DONE_HOP, &DONE_WIGGLE, &DONE_BOUNCE], E::Happy),
        S::Excited => faced(&[&EXCITED_BOUNCE], E::Surprised),
        S::Laughing => faced(&[&LAUGH_SHAKE], E::Happy),
        S::Proud => faced(&[&PROUD_PUFF], E::Calm),
        S::Curious => faced(&[&CURIOUS_TILT], E::Inspecting),
        S::Playful => faced(&[&PLAY_BOP], E::Wink),
        S::Shy => faced(&[&SHY_HIDE], E::Calm),
        S::Surprised => faced(&[&NOTIFY_JUMP], E::Surprised),
        S::Suspicious => faced(&[&SUSPICIOUS_SIDE], E::Squinted),
        S::Angry => faced(&[&ANGRY_STOMP], E::Angry),
        S::Confused => faced(&[&CONFUSED_TILT], E::Confused),
        S::Sad => faced(&[&SAD_DROOP], E::Sad),
        S::Drowsy => faced(&[&DROWSY_NOD], E::Sleepy),
        S::Bored => faced(&[&BORED_SIGH], E::Sleepy),
        S::Scared => faced(&[&SCARED_SHIVER], E::Surprised),
        S::Celebrate => faced(&[&CELEBRATE_TWIRL], E::Happy),
        S::Approving => Spec {
            next: Some(S::Default),
            ..spec(&[&APPROVE_NOD])
        },
        S::Progress | S::Loading => glyph(Glyph::Progress),
        S::Spawning => glyph(Glyph::Gather),
        S::Dictating => glyph(Glyph::Wave),
        S::Sending => glyph(Glyph::Send),
        S::Receiving => glyph(Glyph::Receive),
        S::Uploading => glyph(Glyph::Dock),
        S::Writing => glyph(Glyph::Pencil),
        S::Alerting => glyph(Glyph::Bang),
        S::Bouncing => glyph(Glyph::Ball),
        S::PoweringDown => glyph(Glyph::Standby),
        S::Humming => faced(&[&HUM_SWAY], E::Calm),
        S::Notifying => faced(&[&NOTIFY_JUMP], E::Surprised),
        S::Dragging => faced(&[&DRAG_DANGLE], E::Surprised),
        S::Logo => spec(&[&LOGO_SPIN]),
        S::Showcase => spec(&[
            &IDLE_GLANCE,
            &CELEBRATE_TWIRL,
            &PROUD_PUFF,
            &PLAY_BOP,
            &IDLE_PEEK,
        ]),
        S::Turntable => spec(&[&TURNTABLE]),
    })
}

/// Where to point a held look.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Look {
    #[default]
    Center,
    Left,
    Right,
    Up,
    Down,
}

impl Look {
    /// `[pitch, yaw]` in degrees and the `[x, y]` lean in rig units.
    const fn target(self) -> ([f64; 2], [f64; 2]) {
        match self {
            Look::Center => ([0.0, 0.0], [0.0, 0.0]),
            Look::Left => ([0.0, -24.0], [-6.0, 0.0]),
            Look::Right => ([0.0, 24.0], [6.0, 0.0]),
            Look::Up => ([-18.0, 0.0], [0.0, -4.0]),
            Look::Down => ([18.0, 0.0], [0.0, 4.0]),
        }
    }
}

/// The layer [`Stage`] lays over the rig's pose for one step.
#[derive(Debug, Clone, Copy)]
pub struct Layer {
    /// Radians.
    pub pitch: f64,
    pub yaw: f64,
    pub roll: f64,
    /// Rig units.
    pub x: f64,
    pub y: f64,
    pub sx: f64,
    pub sy: f64,
    pub eyes: [EyeShape; 2],
    /// 0 the classic eyes, 1 `eyes`.
    pub eyes_mix: f64,
    pub whirl: f64,
    pub whirl_angle: f64,
    pub laugh: f64,
    pub glyph: GlyphFrame,
}

/// Deterministic 0..1 numbers (mulberry32).
#[derive(Debug, Clone, Copy)]
struct Rng(u32);

impl Rng {
    fn new(seed: f64) -> Self {
        let s = ((seed * 1e6).floor() as i64).wrapping_mul(0x2545_f491) as u32 ^ 0x9e37_79b9;
        Self(if s == 0 { 1 } else { s })
    }

    fn next(&mut self) -> f64 {
        self.0 = self.0.wrapping_add(0x6d2b_79f5);
        let mut t = self.0;
        t = (t ^ (t >> 15)).wrapping_mul(t | 1);
        t ^= t.wrapping_add((t ^ (t >> 7)).wrapping_mul(t | 61));
        f64::from(t ^ (t >> 14)) / 4_294_967_296.0
    }
}

/// Plays the motion layer.
#[derive(Debug)]
pub struct Stage {
    state: BotAvatarState,
    spec: Option<Spec>,
    rng: Rng,
    deck: Vec<&'static Clip>,
    compact: bool,
    order: usize,
    elapsed: f64,
    player: ClipPlayer,
    face: FaceAnim,
    /// A face asked for from outside; it wins over the state's.
    held: Option<Expression>,
    eyes_mix: Spring,
    blink_at: f64,
    clock: f64,
    pokes: Pokes,
    glyph: GlyphAnim,
    look: Look,
    look_from: ([f64; 2], [f64; 2]),
    look_t: f64,
    amp: Spring,
    gaze: (Spring, Spring),
}

const LOOK_SECONDS: f64 = 0.5;
const POINTER_YAW: f64 = 24.0;
const POINTER_PITCH: f64 = 14.0;

impl Stage {
    pub fn new(seed: f64, state: BotAvatarState, compact: bool) -> Self {
        let mut stage = Self {
            state,
            spec: None,
            rng: Rng::new(seed + 0.618),
            deck: Vec::new(),
            compact,
            order: 0,
            elapsed: 0.0,
            player: ClipPlayer::new(),
            face: FaceAnim::new(Expression::Neutral),
            held: None,
            eyes_mix: Spring::at(0.0),
            blink_at: 0.0,
            clock: 0.0,
            pokes: Pokes::default(),
            glyph: GlyphAnim::new(),
            look: Look::Center,
            look_from: ([0.0; 2], [0.0; 2]),
            look_t: LOOK_SECONDS,
            amp: Spring::at(1.0),
            gaze: (Spring::at(0.0), Spring::at(0.0)),
        };
        stage.blink_at = 1.0 + stage.rng.next() * 2.5;
        stage.enter(state, true);
        stage
    }

    pub fn state(&self) -> BotAvatarState {
        self.state
    }

    /// The clip playing now.
    pub fn clip_name(&self) -> &'static str {
        self.player.clip().name
    }

    pub fn expression(&self) -> Expression {
        self.face.expression()
    }

    /// Moves to `state`, bridging from the current pose.
    pub fn set_state(&mut self, state: BotAvatarState) {
        if state != self.state {
            self.enter(state, false);
        }
    }

    pub fn set_compact(&mut self, compact: bool) {
        if compact != self.compact {
            self.compact = compact;
            self.deck.clear();
        }
    }

    pub fn poke(&mut self, poke: Poke, turns: u32) {
        self.pokes.poke(poke, turns);
    }

    pub fn look(&mut self, look: Look) {
        if look == self.look {
            return;
        }
        self.look_from = self.look_now();
        self.look = look;
        self.look_t = 0.0;
    }

    /// Holds `face` over any state, or lets the state choose again with `None`.
    pub fn set_expression(&mut self, face: Option<Expression>, through_blink: bool) {
        self.held = face;
        let want = self.wanted_face();
        self.face.set(want, through_blink);
    }

    pub fn blink(&mut self) {
        self.face.blink();
    }

    /// Whether nothing is moving and nothing will until something changes.
    pub fn is_settled(&self) -> bool {
        self.spec.is_none()
            && self.pokes.is_empty()
            && self.glyph.is_settled()
            && self.face.is_settled()
            && self.look_t >= LOOK_SECONDS
            && self.eyes_mix.is_settled()
    }

    fn wanted_face(&self) -> Expression {
        self.held
            .or_else(|| self.spec.and_then(|s| s.face))
            .or_else(|| self.player.face_now())
            .unwrap_or(Expression::Neutral)
    }

    fn list(&self) -> &'static [&'static Clip] {
        match self.spec {
            None => &[],
            Some(s) => match (self.compact, s.compact) {
                (true, Some(c)) => c,
                _ => s.clips,
            },
        }
    }

    fn enter(&mut self, state: BotAvatarState, first: bool) {
        self.state = state;
        self.spec = spec_for(state);
        self.elapsed = 0.0;
        self.order = 0;
        self.deck.clear();
        let coin = self.rng.next();
        self.glyph.set(self.spec.and_then(|s| s.glyph), coin);
        if first {
            self.glyph.settle();
        }
        self.eyes_mix.target = if self.spec.is_some() || self.held.is_some() {
            1.0
        } else {
            0.0
        };
        if first {
            self.eyes_mix.settle();
        }
        match self.spec {
            None => self.player.play(&Clip::EMPTY, 0.0, first),
            Some(_) => {
                let clip = self.pick();
                let phase = if first { self.rng.next() } else { 0.0 };
                self.player.play(clip, phase, first);
            }
        }
        let want = self.wanted_face();
        if first {
            self.face = FaceAnim::new(want);
        } else {
            self.face.set(want, false);
        }
    }

    fn sequential(&self) -> bool {
        self.spec
            .is_some_and(|s| s.next.is_some() && s.hold.is_none())
    }

    fn pick(&mut self) -> &'static Clip {
        let list = self.list();
        if list.is_empty() {
            return &Clip::EMPTY;
        }
        if self.sequential() {
            return list[self.order.min(list.len() - 1)];
        }
        if self.deck.is_empty() {
            let mut deck = list.to_vec();
            for i in (1..deck.len()).rev() {
                let j = ((self.rng.next() * (i + 1) as f64) as usize).min(i);
                deck.swap(i, j);
            }
            let last = self.player.clip().name;
            if deck.len() > 1 && deck[0].name == last {
                deck.swap(0, 1);
            }
            self.deck = deck;
        }
        self.deck.remove(0)
    }

    fn look_now(&self) -> ([f64; 2], [f64; 2]) {
        let (r1, p1) = self.look.target();
        let (r0, p0) = self.look_from;
        let k = Bezier::IN_OUT.at((self.look_t / LOOK_SECONDS).clamp(0.0, 1.0));
        let l = |a: f64, b: f64| a + (b - a) * k;
        (
            [l(r0[0], r1[0]), l(r0[1], r1[1])],
            [l(p0[0], p1[0]), l(p0[1], p1[1])],
        )
    }

    /// Steps the layer. `pointer` is `(x, y, strength)` as [`crate::Avatar::set_pointer`]
    /// takes it.
    pub fn update(&mut self, dt: f64, calm: Calm, pointer: (f64, f64, f64)) -> Layer {
        self.clock += dt;
        self.elapsed += dt;
        if let Some(Spec {
            next: Some(next),
            hold: Some(hold),
            ..
        }) = self.spec
            && self.elapsed >= hold
        {
            self.enter(next, false);
        }

        let playing = self.spec.is_some();
        self.amp.target = match calm {
            Calm::Gaze => 0.0,
            Calm::Busy => 0.55,
            Calm::Full => 1.0,
        };
        self.amp.step(6.0, dt);
        if playing {
            let cues = self
                .player
                .advance(dt * if calm == Calm::Gaze { 0.0 } else { 1.0 });
            let held = self.held.is_some() || self.spec.and_then(|s| s.face).is_some();
            if !held {
                for face in &cues.faces {
                    self.face.set(face.expr, face.through_blink);
                }
            }
            for _ in 0..cues.blinks {
                self.face.blink();
            }
            if cues.looped {
                if self.sequential() {
                    self.order += 1;
                    if self.order >= self.list().len() {
                        let next = self
                            .spec
                            .and_then(|s| s.next)
                            .unwrap_or(BotAvatarState::Default);
                        self.enter(next, false);
                    } else {
                        let clip = self.pick();
                        self.player.play(clip, 0.0, false);
                    }
                } else if self.list().len() > 1 {
                    let clip = self.pick();
                    self.player.play(clip, 0.0, false);
                }
            }
        } else {
            self.player.advance(dt);
        }

        // Our own blinks between the clips', unless the clip blinks for itself.
        if self.clock >= self.blink_at {
            if self.spec.is_some() && self.player.clip().blinks.is_empty() {
                self.face.blink();
            }
            self.blink_at = self.clock + 2.4 + self.rng.next() * 2.8;
        }

        let clip_pose = self.player.pose();
        let yaw_deg = (clip_pose.rot[1] + 180.0).rem_euclid(360.0) - 180.0;
        self.face.set_bias((yaw_deg / 20.0).clamp(-1.0, 1.0));
        self.face.update(dt);
        self.eyes_mix.step(14.0, dt);
        self.pokes.update(dt);
        self.glyph.update(dt);
        self.look_t += dt;

        let (px, py, ps) = pointer;
        let follow = if playing && calm == Calm::Full {
            ps
        } else {
            0.0
        };
        self.gaze.0.target = px.clamp(-1.0, 1.0) * follow;
        self.gaze.1.target = py.clamp(-1.0, 1.0) * follow;
        self.gaze.0.step(12.0, dt);
        self.gaze.1.step(12.0, dt);

        let amp = self.amp.x.clamp(0.0, 1.0);
        let c: ClipPose = clip_pose.scaled(amp);
        let poke = self.pokes.offset();
        let (lr, lp) = self.look_now();
        let gx = self.gaze.0.x;
        let gy = self.gaze.1.x;
        Layer {
            pitch: (c.rot[0] + poke.pitch + lr[0] - POINTER_PITCH * gy) * DEG,
            yaw: (c.rot[1] + poke.yaw + lr[1] + POINTER_YAW * gx) * DEG,
            roll: (c.rot[2] + poke.roll) * DEG,
            x: c.pos[0] + lp[0] + 5.0 * gx,
            y: c.pos[1] + poke.y + lp[1] + 3.0 * gy,
            sx: c.squash[0] * poke.sx,
            sy: c.squash[1] * poke.sy,
            eyes: self.face.shapes(),
            eyes_mix: self.eyes_mix.x.clamp(0.0, 1.0),
            whirl: c.whirl.max(poke.whirl),
            whirl_angle: if poke.whirl >= c.whirl {
                poke.whirl_angle
            } else {
                self.clock * std::f64::consts::TAU * 1.6
            },
            laugh: poke.laugh.max(c.whirl),
            glyph: self.glyph.frame(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Look, Stage, spec_for};
    use crate::engine::Calm;
    use crate::expression::Expression;
    use crate::glyph::Glyph;
    use crate::poke::Poke;
    use crate::types::BotAvatarState;

    fn run(stage: &mut Stage, seconds: f64) -> super::Layer {
        let mut layer = stage.update(0.0, Calm::Full, (0.0, 0.0, 0.0));
        for _ in 0..(seconds * 120.0) as usize {
            layer = stage.update(1.0 / 120.0, Calm::Full, (0.0, 0.0, 0.0));
        }
        layer
    }

    #[test]
    fn every_new_state_has_motion_and_the_classic_ones_none() {
        for state in BotAvatarState::ALL {
            assert_eq!(spec_for(state).is_none(), state.is_classic(), "{state}");
            if let Some(spec) = spec_for(state) {
                assert!(!spec.clips.is_empty(), "{state}");
            }
        }
    }

    #[test]
    fn a_classic_state_lays_nothing_over_the_rig() {
        let mut stage = Stage::new(0.3, BotAvatarState::Default, false);
        let layer = run(&mut stage, 2.0);
        assert_eq!(
            (layer.pitch, layer.yaw, layer.roll, layer.x, layer.y),
            (0.0, 0.0, 0.0, 0.0, 0.0)
        );
        assert_eq!((layer.sx, layer.sy, layer.eyes_mix), (1.0, 1.0, 0.0));
        assert!(stage.is_settled());
    }

    #[test]
    fn a_poke_plays_over_a_classic_state_and_ends() {
        let mut stage = Stage::new(0.3, BotAvatarState::Default, false);
        stage.poke(Poke::Spin, 1);
        let mid = run(&mut stage, 0.45);
        assert!(mid.yaw.abs() > 0.5);
        let end = run(&mut stage, 1.0);
        assert_eq!(end.yaw, 0.0);
    }

    #[test]
    fn thinking_folds_into_dots() {
        let mut stage = Stage::new(0.3, BotAvatarState::Default, false);
        stage.set_state(BotAvatarState::Thinking);
        let layer = run(&mut stage, 1.5);
        assert_eq!(layer.glyph.glyph, Some(Glyph::Dots));
        assert!(layer.glyph.amount > 0.98);
    }

    #[test]
    fn finished_plays_once_then_goes_idle() {
        let mut stage = Stage::new(0.3, BotAvatarState::Default, false);
        stage.set_state(BotAvatarState::Finished);
        assert_eq!(stage.expression(), Expression::Happy);
        run(&mut stage, 6.0);
        assert_eq!(stage.state(), BotAvatarState::Default);
        assert_eq!(stage.expression(), Expression::Neutral);
    }

    #[test]
    fn blocked_turns_drowsy_after_its_hold() {
        let mut stage = Stage::new(0.3, BotAvatarState::Blocked, false);
        run(&mut stage, 121.0);
        assert_eq!(stage.state(), BotAvatarState::Drowsy);
    }

    #[test]
    fn working_clips_never_repeat_back_to_back() {
        let mut stage = Stage::new(0.7, BotAvatarState::Coding, false);
        let mut last = stage.clip_name();
        let mut changes = 0;
        for _ in 0..(60 * 120) {
            stage.update(1.0 / 120.0, Calm::Full, (0.0, 0.0, 0.0));
            let now = stage.clip_name();
            if now != last {
                changes += 1;
                last = now;
            }
        }
        assert!(changes >= 8, "{changes}");
    }

    #[test]
    fn two_seeds_start_out_of_step() {
        let mut a = Stage::new(0.1, BotAvatarState::Streaming, false);
        let mut b = Stage::new(0.9, BotAvatarState::Streaming, false);
        let la = run(&mut a, 0.1);
        let lb = run(&mut b, 0.1);
        assert!((la.yaw - lb.yaw).abs() + (la.x - lb.x).abs() > 1e-3);
    }

    #[test]
    fn a_held_look_turns_and_stays() {
        let mut stage = Stage::new(0.3, BotAvatarState::Default, false);
        stage.look(Look::Left);
        let layer = run(&mut stage, 1.0);
        assert!((layer.yaw.to_degrees() + 24.0).abs() < 1e-6);
    }

    #[test]
    fn gaze_calm_holds_the_body_still() {
        let mut stage = Stage::new(0.3, BotAvatarState::Coding, false);
        let mut layer = stage.update(0.0, Calm::Gaze, (0.0, 0.0, 0.0));
        for _ in 0..600 {
            layer = stage.update(1.0 / 120.0, Calm::Gaze, (0.0, 0.0, 0.0));
        }
        assert!(
            layer.yaw.abs() < 0.01 && layer.y.abs() < 0.1,
            "{} {}",
            layer.yaw,
            layer.y
        );
    }
}
