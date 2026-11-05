//! Public avatar. The picture comes from [`crate::draw::draw`]; the clock is
//! [`crate::engine::Sim`] stepped at [`crate::ticker::FIXED_DT`].
//!
//! `Avatar` follows `BotAvatar.tsx` `model` (173–251) and `paint` (320–342):
//! fabric shading, the preset colour pipeline, stock fur, `LIGHT_DEFAULTS.fabric`,
//! and a device canvas of `round(size_px * OVERSCAN * scale)`. There is no DOM,
//! so the theme is dark and `still` is false.
//!
//! `kind_for_agent` and `state_for_status` follow
//! `packages/features/dispatch/src/agent-avatar.tsx` (9–32): a trimmed id, or
//! `"agent"` when empty; the mark's body when one matches, otherwise the hash;
//! pending/running work, failed/cancelled/timed_out sleep, anything else idle.
//! [`Avatar::for_agent`] also takes that component's motion: a bot whose run is not working
//! makes no idle jumps and turns its head 0.35 as far.
//!
//! [`Avatar::mascot`] is `EmptyBot` (`packages/frontend/design-system/src/empty-bot.tsx`):
//! `PolterMascot`, the ghost with the stock fur, asleep, sad (the mouth face), or lost (a full
//! turn), never jumping.

use tiny_skia::Pixmap;

use crate::agent_marks::{agent_mark_for, bot_type_for, dispatch_bot_state, seed_for};
use crate::canvas::Canvas2D;
use crate::color::{auto_ink, richer, shade};
use crate::draw::{DrawConfig, LIGHT_DEFAULTS, OVERSCAN, OutlineParts, RISE, WhirlKnobs, draw};
use crate::engine::{Calm, JUMP_DEFAULTS, Pose, Sim};
use crate::expression::Expression;
use crate::glyph::{GlyphPaint, draw_glyph};
use crate::plastic::FurStyle;
use crate::poke::Poke;
use crate::presets::bot_avatar_preset;
use crate::shapes::{body_path_for, part_paths_for, shape_part_colors};
use crate::stage::{COMPACT_PX, Layer, Look, Stage};
use crate::ticker::FIXED_DT;
use crate::types::{
    BotAvatarFace, BotAvatarShading, BotAvatarState, BotAvatarTheme, BotAvatarType,
};
use crate::wear::Wear;

/// Same seed for every avatar, so a time maps to one pose.
const SEED: f64 = 0.0;

/// `EmptyBotMood`: how the mascot looks at an empty page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MascotMood {
    /// Asleep, as a page that has nothing yet.
    Sleeping,
    /// Awake with the mouth face, as a page that found nothing it needs.
    Sad,
    /// Awake and looking all around, as a search that matched nothing.
    Lost,
}

/// How the rig idles between poses.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Motion {
    /// The library's defaults: a jump every 8s and a full turn.
    Library,
    /// `DispatchAgentAvatar`: still and 0.35 of a turn unless the run works.
    Agent,
    /// No idle jumps, and this much of a turn.
    Held(f64),
}

/// A bot avatar rendered to a premultiplied RGBA pixmap.
pub struct Avatar {
    kind: BotAvatarType,
    state: BotAvatarState,
    size_px: u32,
    scale: f32,
    /// Brand colour, in place of the preset colour. `None` keeps the preset.
    color: Option<String>,
    seed: f64,
    /// The preset's face unless this replaces it.
    face: Option<BotAvatarFace>,
    motion: Motion,
    sim: Sim,
    /// Fixed steps applied since t = 0.
    steps: u64,
    canvas: Canvas2D,
    cfg: Option<DrawConfig>,
    /// Where the pointer is, as the rig reads it: x and y from the bot's centre to its edge
    /// (-1 to 1, held to ±1.2 past it) and how strongly the bot looks at it (0 to 1).
    /// `(0, 0, 0)` is no pointer.
    pointer: (f64, f64, f64),
    calm: Calm,
    /// The motion layer: clips, faces, pokes, looks, glyphs.
    stage: Stage,
    /// The rig's pose with the motion layer over it, as drawn.
    pose: Pose,
    glyph: crate::glyph::GlyphFrame,
    glyph_paint: Option<GlyphPaint>,
    /// Pokes, looks, and faces asked for, with the step they came at, so a rewind replays
    /// them where they happened.
    events: Vec<(u64, Event)>,
}

/// Something asked of the motion layer at a moment.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Event {
    Poke(Poke, u32),
    Look(Look),
    Face(Option<Expression>, bool),
    Blink,
}

impl Avatar {
    pub fn new(kind: BotAvatarType, state: BotAvatarState, size_px: u32, scale: f32) -> Self {
        Self::build(kind, state, size_px, scale, None, SEED, Motion::Library)
    }

    /// `EmptyBot`: the Polterware mascot as an empty-state face.
    pub fn mascot(mood: MascotMood, size_px: u32, scale: f32) -> Self {
        let state = match mood {
            MascotMood::Sleeping => BotAvatarState::Sleeping,
            MascotMood::Sad | MascotMood::Lost => BotAvatarState::Default,
        };
        let turn = if mood == MascotMood::Lost { 1.0 } else { 0.35 };
        let mut avatar = Self::build(
            BotAvatarType::Ghost,
            state,
            size_px,
            scale,
            None,
            SEED,
            Motion::Held(turn),
        );
        if mood == MascotMood::Sad {
            avatar.face = Some(BotAvatarFace::Mouth);
        }
        avatar
    }

    /// An agent from Dispatch `agent-avatar.tsx`: mark body and colour, fabric shading,
    /// stock fur. `status` is a bot state (`working`) or a run status (`running`).
    /// Out of `working` it makes no idle jumps and turns its head 0.35 as far.
    pub fn for_agent(agent_id: &str, status: &str, size_px: u32, scale: f32) -> Self {
        let trimmed = agent_id.trim();
        let key = if trimmed.is_empty() { "agent" } else { trimmed };
        let kind = kind_for_agent(key);
        let state = agent_state(status);
        let color = agent_mark_for(key).map(|mark| mark.color.to_string());
        Self::build(
            kind,
            state,
            size_px,
            scale,
            color,
            seed_for(key),
            Motion::Agent,
        )
    }

    fn build(
        kind: BotAvatarType,
        state: BotAvatarState,
        size_px: u32,
        scale: f32,
        color: Option<String>,
        seed: f64,
        motion: Motion,
    ) -> Self {
        let side = device_side(size_px, scale);
        Self {
            kind,
            state,
            size_px,
            scale,
            color,
            seed,
            face: None,
            motion,
            sim: new_sim(seed, state, motion),
            steps: 0,
            canvas: Canvas2D::new(side, side),
            cfg: None,
            pointer: (0.0, 0.0, 0.0),
            calm: Calm::Full,
            stage: Stage::new(seed, state, size_px < COMPACT_PX),
            pose: Pose::default(),
            glyph: crate::glyph::GlyphFrame::NONE,
            glyph_paint: None,
            events: Vec::new(),
        }
    }

    /// Point the rig at `state`. A later rewind replays from t = 0 in that state.
    ///
    /// The classic states run the classic rig. The others hand the body to the motion
    /// layer, which bridges from the current pose into the new state's clips.
    pub fn set_state(&mut self, state: BotAvatarState) {
        self.state = state;
        self.sim.set_state(state, false);
        self.sim.set_quiet(!state.is_classic());
        apply_motion(&mut self.sim, state, self.motion);
        self.stage.set_state(state);
    }

    /// The state now. A state with a `next` (finished, waking, blocked...) moves on by
    /// itself, so this can differ from the last [`Avatar::set_state`].
    pub fn state(&self) -> BotAvatarState {
        self.state
    }

    /// The face the eyes show now.
    pub fn expression(&self) -> Expression {
        self.stage.expression()
    }

    /// The clip the motion layer plays now (`"rest"` for the classic states).
    pub fn clip_name(&self) -> &'static str {
        self.stage.clip_name()
    }

    /// Lays a short move over whatever the bot is doing. `turns` counts the turns of a
    /// [`Poke::Spin`]; the others ignore it.
    pub fn poke(&mut self, poke: Poke, turns: u32) {
        self.push(Event::Poke(poke, turns));
    }

    /// Turns the head to `look` and holds it there; [`Look::Center`] lets it go.
    pub fn look(&mut self, look: Look) {
        self.push(Event::Look(look));
    }

    /// Holds `face` over every state, or gives the face back to the state with `None`.
    /// `through_blink` swaps the shape behind a blink instead of morphing it.
    pub fn set_expression(&mut self, face: Option<Expression>, through_blink: bool) {
        self.push(Event::Face(face, through_blink));
    }

    /// Blinks once, unless the face is one without open eyes.
    pub fn blink(&mut self) {
        self.push(Event::Blink);
    }

    /// Whether frames would change on their own: false once a classic bot in a calm rig has
    /// nothing left to play.
    pub fn is_animating(&self) -> bool {
        !(self.calm != Calm::Full && self.state.is_classic() && self.stage.is_settled())
    }

    fn push(&mut self, event: Event) {
        self.events.push((self.steps, event));
        apply_event(&mut self.stage, event);
    }

    /// Sets how much of the rig moves; see [`Calm`]. A change replays the rig from t = 0.
    pub fn set_calm(&mut self, calm: Calm) {
        if calm == self.calm {
            return;
        }
        // Replayed from t = 0, so two bots in the same calm at the same time share a pose.
        self.calm = calm;
        self.restart();
    }

    /// Back to t = 0 in the current state; the asked-for events replay as the clock passes them.
    fn restart(&mut self) {
        self.sim = new_sim(self.seed, self.state, self.motion);
        self.sim.set_calm(self.calm);
        self.stage = Stage::new(self.seed, self.state, self.size_px < COMPACT_PX);
        self.steps = 0;
    }

    /// Makes the bot look at the pointer: `x` and `y` from its centre to its edge (-1 to 1),
    /// `strength` from 0 (looks ahead) to 1. The head and eyes ease toward it over the next
    /// steps; `(0, 0, 0)` lets them go back.
    pub fn set_pointer(&mut self, x: f64, y: f64, strength: f64) {
        let finite = |v: f64| if v.is_finite() { v } else { 0.0 };
        self.pointer = (finite(x), finite(y), finite(strength));
    }

    /// Pose and paint at `time_seconds`.
    ///
    /// The sim is rebuilt and stepped from t = 0 when `time_seconds` is earlier
    /// than the last call. The same time is the same step count, so with no pointer the
    /// pixmap bytes depend only on kind, state, size, scale, and time.
    pub fn render(&mut self, time_seconds: f64) -> &Pixmap {
        self.advance(time_seconds);
        if self.cfg.is_none() {
            self.cfg = Some(self.build_config());
        }
        let box_px = f64::from(self.size_px);
        let scale = finite_scale(self.scale);
        let side = device_side(self.size_px, self.scale);
        self.canvas.ensure_size(side, side);
        self.canvas.set_transform(scale, 0.0, 0.0, scale, 0.0, 0.0);
        let cfg = self.cfg.as_ref().expect("config");
        let glyph = self.glyph;
        let mut pose = self.pose;
        if glyph.amount > 0.0 {
            // The body shrinks toward its centre and turns away as the glyph grows.
            let k = (1.0 - glyph.amount).powf(1.3);
            pose.y -= 50.0 * pose.sy * (1.0 - k);
            pose.sx *= k;
            pose.sy *= k;
            pose.yaw += glyph.spin;
        } else {
            pose.yaw += glyph.spin;
        }
        if pose.sx > 0.015 {
            draw(&mut self.canvas, box_px, &pose, cfg);
        } else {
            let full = box_px * OVERSCAN;
            self.canvas.clear_rect(0.0, 0.0, full, full);
        }
        if glyph.amount > 0.0 {
            let paint = self
                .glyph_paint
                .get_or_insert_with(|| GlyphPaint::new(&cfg.color, &cfg.ink));
            let full = box_px * OVERSCAN;
            draw_glyph(
                &mut self.canvas,
                full,
                full / 2.0 + RISE * box_px,
                box_px,
                &glyph,
                paint,
            );
        }
        &self.canvas.pixmap
    }

    fn advance(&mut self, time_seconds: f64) {
        let target = step_count(time_seconds);
        let replay = target < self.steps;
        if replay {
            self.restart();
        }
        while self.steps < target {
            if replay || self.steps < self.events.last().map_or(0, |e| e.0) {
                for &(at, event) in &self.events {
                    if at == self.steps {
                        apply_event(&mut self.stage, event);
                    }
                }
            }
            let (x, y, strength) = self.pointer;
            self.sim.set_pointer(x, y, strength);
            self.sim.update(FIXED_DT);
            let layer = self.stage.update(FIXED_DT, self.calm, self.pointer);
            let moved = self.stage.state();
            if moved != self.state {
                // The director moved on by itself (a finished bot goes idle).
                self.state = moved;
                self.sim.set_state(moved, false);
                self.sim.set_quiet(!moved.is_classic());
                apply_motion(&mut self.sim, moved, self.motion);
            }
            self.pose = compose(&self.sim.pose, &layer);
            self.glyph = layer.glyph;
            self.steps += 1;
        }
        if self.steps == 0 {
            self.pose = self.sim.pose;
        }
    }

    fn build_config(&self) -> DrawConfig {
        let preset = bot_avatar_preset(self.kind);
        let (color, ink, vivid) = body_ink(self.kind, self.color.as_deref());
        let lit = LIGHT_DEFAULTS.fabric;
        let parts = part_paths_for(self.kind).map(OutlineParts::Many);
        let parts_colors = shape_part_colors(self.kind).map(|cols| {
            cols.iter()
                .map(|c| c.map(str::to_string))
                .collect::<Vec<_>>()
        });
        DrawConfig {
            path: body_path_for(self.kind),
            face: self.face.unwrap_or(preset.face),
            face_x: preset.face_x,
            face_y: preset.face_y,
            face_scale: preset.face_scale,
            color,
            ink,
            shading: BotAvatarShading::Fabric,
            shadow: Some(clamp_num(lit.shadow, 0.0, 2.0)),
            highlight: Some(clamp_num(lit.highlight, 0.0, 2.0)),
            depth: Some(clamp_num(0.65, 0.2, 2.0)),
            light: Some(lit.light),
            rim: Some(clamp_num(lit.rim, 0.0, 2.0)),
            spread: Some(clamp_num(lit.spread, 0.4, 2.5)),
            roundness: Some(clamp_num(1.0, 0.0, 1.0)),
            fur: Some(stock_fur()),
            vivid: Some(vivid),
            back_light: None,
            light_front: None,
            shine: None,
            sheen: None,
            back_softness: None,
            type_key: Some(self.kind.as_str().to_string()),
            still: false,
            fur_on_idle: true,
            parts,
            parts_depth: None,
            parts_colors,
            theme: Some(BotAvatarTheme::Dark),
            dpr: Some(finite_scale(self.scale)),
            sides: crate::draw::SideMode::Auto,
            whirl: Some(WhirlKnobs {
                strength: clamp_num(0.0, 0.0, 2.0),
                size: clamp_num(1.0, 0.6, 1.6),
                width: clamp_num(1.0, 0.4, 2.0),
                length: clamp_num(1.0, 0.4, 1.6),
                tilt: clamp_num(1.0, 0.5, 1.8),
            }),
            wear: Some(Wear {
                hat: crate::types::BotAvatarHat::None,
                glasses: crate::types::BotAvatarGlasses::None,
                headphones: false,
                bow_tie: false,
                color: "#27272b".to_string(),
                fabric: true,
            }),
        }
    }
}

fn new_sim(seed: f64, state: BotAvatarState, motion: Motion) -> Sim {
    let mut sim = Sim::new(seed, state);
    sim.set_quiet(!state.is_classic());
    apply_motion(&mut sim, state, motion);
    sim
}

fn apply_event(stage: &mut Stage, event: Event) {
    match event {
        Event::Poke(poke, turns) => stage.poke(poke, turns),
        Event::Look(look) => stage.look(look),
        Event::Face(face, through) => stage.set_expression(face, through),
        Event::Blink => stage.blink(),
    }
}

/// The rig's pose with the motion layer laid over it. A layer at rest leaves the pose as it is.
fn compose(base: &Pose, l: &Layer) -> Pose {
    let mut p = *base;
    p.pitch += l.pitch;
    p.yaw += l.yaw;
    p.roll += l.roll;
    p.x += l.x;
    p.y += l.y;
    p.sx *= l.sx;
    p.sy *= l.sy;
    p.eyes = l.eyes;
    p.eyes_mix = l.eyes_mix;
    if l.whirl > p.whirl {
        p.whirl = l.whirl;
        p.whirl_angle = l.whirl_angle;
    }
    p
}

/// `DispatchAgentAvatar`: `jumpEvery: 0, turn: 0.35` unless the run works, else the
/// library's defaults (a jump every 8s, a full turn). `EmptyBot`: `jumpEvery: 0` and its turn.
fn apply_motion(sim: &mut Sim, state: BotAvatarState, motion: Motion) {
    let (turn, every) = match motion {
        Motion::Library => return,
        Motion::Agent if state == BotAvatarState::Working => (1.0, JUMP_DEFAULTS.every),
        Motion::Agent => (0.35, 0.0),
        Motion::Held(turn) => (turn, 0.0),
    };
    sim.set_turn(turn);
    let mut jump = sim.jump_config();
    jump.every = every;
    sim.set_jump(jump);
}

/// Mark body, or the hashed library body. Blank trims to `"agent"`.
pub fn kind_for_agent(agent_id: &str) -> BotAvatarType {
    let trimmed = agent_id.trim();
    let key = if trimmed.is_empty() { "agent" } else { trimmed };
    agent_mark_for(key)
        .map(|mark| mark.kind)
        .unwrap_or_else(|| bot_type_for(key))
}

/// A run status maps the way Dispatch does; any other word is a state's name, and an unknown
/// word is idle.
fn agent_state(status: &str) -> BotAvatarState {
    const RUN_STATUSES: [&str; 6] = [
        "pending",
        "running",
        "failed",
        "cancelled",
        "canceled",
        "timed_out",
    ];
    let lower = status.trim().to_ascii_lowercase();
    if RUN_STATUSES.contains(&lower.as_str()) {
        return state_for_status(&lower);
    }
    lower
        .parse::<BotAvatarState>()
        .unwrap_or_else(|_| state_for_status(status))
}

/// Dispatch run status. `"idle"` is not a run state, so it is [`BotAvatarState::Default`].
pub fn state_for_status(status: &str) -> BotAvatarState {
    dispatch_bot_state(Some(status))
}

fn step_count(time_seconds: f64) -> u64 {
    if !time_seconds.is_finite() || time_seconds <= 0.0 {
        return 0;
    }
    let steps = time_seconds / FIXED_DT;
    if !steps.is_finite() || steps < 1.0 {
        return 0;
    }
    let capped = steps.min(u64::MAX as f64);
    capped.floor() as u64
}

fn device_side(size_px: u32, scale: f32) -> u32 {
    let n = (f64::from(size_px) * OVERSCAN * finite_scale(scale)).round();
    if n.is_finite() && n >= 1.0 {
        n as u32
    } else {
        1
    }
}

fn finite_scale(scale: f32) -> f64 {
    let scale = f64::from(scale);
    if scale.is_finite() && scale > 0.0 {
        scale
    } else {
        1.0
    }
}

/// `BotAvatar.tsx` `clamp`: a non-finite value becomes 1, then the range.
fn clamp_num(v: f64, lo: f64, hi: f64) -> f64 {
    let v = if v.is_finite() { v } else { 1.0 };
    v.clamp(lo, hi)
}

/// `q` in `BotAvatar.tsx`: `round(clamp * 20) / 20`.
fn quantize_fur(v: f64, lo: f64, hi: f64) -> f64 {
    (clamp_num(v, lo, hi) * 20.0).round() / 20.0
}

fn stock_fur() -> FurStyle {
    FurStyle {
        length: quantize_fur(1.0, 0.3, 2.5),
        density: quantize_fur(1.6, 0.3, 2.0),
        fuzz: quantize_fur(0.9, 0.0, 1.0),
        clumps: quantize_fur(0.4, 0.0, 1.0),
        curl: quantize_fur(0.7, 0.0, 1.0),
        gravity: quantize_fur(0.9, 0.0, 1.0),
    }
}

fn body_ink(kind: BotAvatarType, color: Option<&str>) -> (String, String, f64) {
    let preset = bot_avatar_preset(kind);
    let brightness = preset.brightness.unwrap_or(1.0);
    let saturation = preset.saturation.unwrap_or(1.5);
    let vivid = clamp_num(saturation, 1.5, 2.5) - 1.5;
    let picked = color.unwrap_or(preset.color);
    let shaded = if brightness == 1.0 && saturation == 1.0 {
        picked.to_string()
    } else {
        shade(
            picked,
            (clamp_num(brightness, 0.0, 2.0) - 1.0) * 0.35,
            (clamp_num(saturation, 0.0, 1.5) - 1.0) * 0.5,
        )
    };
    let body = richer(&shaded, vivid);
    let ink = auto_ink(&body).to_string();
    (body, ink, vivid)
}

#[cfg(test)]
fn same_pose(a: &Pose, b: &Pose) -> bool {
    a.yaw == b.yaw
        && a.pitch == b.pitch
        && a.roll == b.roll
        && a.x == b.x
        && a.y == b.y
        && a.sx == b.sx
        && a.sy == b.sy
        && a.eye_open == b.eye_open
        && a.blink_l == b.blink_l
        && a.blink_r == b.blink_r
        && a.look_x == b.look_x
        && a.look_y == b.look_y
        && a.breath == b.breath
        && a.laugh == b.laugh
        && a.whirl == b.whirl
        && a.whirl_angle == b.whirl_angle
        && a.w == b.w
}

#[cfg(test)]
mod tests {
    use super::{Avatar, MascotMood, kind_for_agent, same_pose, state_for_status, stock_fur};
    use crate::agent_marks::{agent_mark_for, bot_type_for, seed_for};
    use crate::draw::OutlineParts;
    use crate::engine::JUMP_DEFAULTS;
    use crate::plastic::FUR_STOCK;
    use crate::types::{
        BotAvatarFace, BotAvatarShading, BotAvatarState, BotAvatarTheme, BotAvatarType,
    };

    #[test]
    fn kind_for_agent_prefers_marks() {
        assert_eq!(kind_for_agent(""), bot_type_for("agent"));
        assert_eq!(kind_for_agent("   "), bot_type_for("agent"));
        assert!(agent_mark_for("agent").is_none());
        assert_eq!(kind_for_agent("claude"), BotAvatarType::Star);
        assert_eq!(kind_for_agent("  Claude  "), BotAvatarType::Star);
        assert_eq!(kind_for_agent("OpenAI"), BotAvatarType::Flower);
        assert_eq!(kind_for_agent("xai"), BotAvatarType::Hexagon);
        assert_eq!(kind_for_agent("cursor"), BotAvatarType::Square);
        assert_eq!(kind_for_agent("antigravity"), BotAvatarType::Triangle);
        assert_eq!(kind_for_agent("github-copilot"), BotAvatarType::Cat);
        let unknown = "zzzz-not-an-agent";
        assert!(agent_mark_for(unknown).is_none());
        assert_eq!(kind_for_agent(unknown), bot_type_for(unknown));
    }

    #[test]
    fn state_for_status_matches_dispatch() {
        assert_eq!(state_for_status("pending"), BotAvatarState::Working);
        assert_eq!(state_for_status("running"), BotAvatarState::Working);
        assert_eq!(state_for_status("Running"), BotAvatarState::Working);
        assert_eq!(state_for_status("failed"), BotAvatarState::Sleeping);
        assert_eq!(state_for_status("cancelled"), BotAvatarState::Sleeping);
        assert_eq!(state_for_status("canceled"), BotAvatarState::Sleeping);
        assert_eq!(state_for_status("timed_out"), BotAvatarState::Sleeping);
        assert_eq!(state_for_status("succeeded"), BotAvatarState::Default);
        assert_eq!(state_for_status(" "), BotAvatarState::Default);
        assert_eq!(state_for_status("blocked"), BotAvatarState::Default);
        assert_eq!(state_for_status("idle"), BotAvatarState::Default);
    }

    #[test]
    fn stock_fur_is_on_the_quantized_grid() {
        assert_eq!(stock_fur(), FUR_STOCK);
    }

    #[test]
    fn new_matches_the_component_defaults() {
        let avatar = Avatar::new(BotAvatarType::Clover, BotAvatarState::Working, 64, 2.0);
        assert_eq!(avatar.canvas.width(), 192);
        assert_eq!(avatar.canvas.height(), 192);
        assert_eq!(avatar.sim.state, BotAvatarState::Working);
        let cfg = avatar.build_config();
        assert_eq!(cfg.shading, BotAvatarShading::Fabric);
        assert_eq!(cfg.depth, Some(0.65));
        assert_eq!(cfg.roundness, Some(1.0));
        assert_eq!(cfg.light, Some(295.0));
        assert_eq!(cfg.fur, Some(FUR_STOCK));
        assert!(!cfg.still);
        assert!(cfg.fur_on_idle);
        assert_eq!(cfg.dpr, Some(2.0));
        assert_eq!(cfg.theme, Some(BotAvatarTheme::Dark));
        assert_eq!(cfg.type_key.as_deref(), Some("clover"));
        assert!(cfg.parts.is_none());
        assert!(cfg.parts_colors.is_none());
        assert_eq!(cfg.parts_depth, None);
        let whirl = cfg.whirl.unwrap();
        assert_eq!(whirl.strength, 0.0);
        assert_eq!(whirl.size, 1.0);
        let wear = cfg.wear.unwrap();
        assert_eq!(wear.color, "#27272b");
        assert!(wear.fabric);
        assert!(!wear.headphones);
        assert!(!wear.bow_tie);

        let ghost = Avatar::new(BotAvatarType::Ghost, BotAvatarState::Default, 32, 1.0);
        let g = ghost.build_config();
        assert!(matches!(g.parts, Some(OutlineParts::Many(_))));
        let colors = g.parts_colors.unwrap();
        assert_eq!(colors[0].as_deref(), Some("#8f8f8f"));
        assert_eq!(colors[1].as_deref(), Some("#4ade80"));
        assert_eq!(ghost.canvas.width(), 48);
    }

    #[test]
    fn for_agent_uses_the_mark_color_and_fabric() {
        let claude = Avatar::for_agent("claude", "running", 64, 2.0);
        assert_eq!(claude.kind, BotAvatarType::Star);
        assert_eq!(claude.state, BotAvatarState::Working);
        assert_eq!(claude.seed, seed_for("claude"));
        let marked = claude.build_config();
        let plain =
            Avatar::new(BotAvatarType::Star, BotAvatarState::Working, 64, 2.0).build_config();
        assert_eq!(marked.shading, BotAvatarShading::Fabric);
        assert_eq!(marked.fur, Some(FUR_STOCK));
        assert_ne!(marked.color, plain.color);
        let cursor = Avatar::for_agent("cursor", "idle", 28, 1.0);
        assert_eq!(cursor.kind, BotAvatarType::Square);
        assert_eq!(cursor.state, BotAvatarState::Default);
        let grok = Avatar::for_agent("grok", "failed", 28, 1.0);
        assert_eq!(grok.kind, BotAvatarType::Hexagon);
        assert_eq!(grok.state, BotAvatarState::Sleeping);
    }

    #[test]
    fn an_agent_holds_still_unless_its_run_works() {
        let mut avatar = Avatar::for_agent("claude", "completed", 32, 1.0);
        assert_eq!(avatar.sim.jump_config().every, 0.0);
        avatar.set_state(BotAvatarState::Working);
        assert_eq!(avatar.sim.jump_config().every, JUMP_DEFAULTS.every);
        avatar.set_state(BotAvatarState::Sleeping);
        assert_eq!(avatar.sim.jump_config().every, 0.0);
        let plain = Avatar::new(BotAvatarType::Star, BotAvatarState::Default, 32, 1.0);
        assert_eq!(plain.sim.jump_config().every, JUMP_DEFAULTS.every);
    }

    #[test]
    fn the_mascot_is_the_ghost_in_each_empty_bot_mood() {
        let sleeping = Avatar::mascot(MascotMood::Sleeping, 64, 2.0);
        assert_eq!(sleeping.kind, BotAvatarType::Ghost);
        assert_eq!(sleeping.state, BotAvatarState::Sleeping);
        assert_eq!(sleeping.build_config().face, BotAvatarFace::Eyes);
        assert_eq!(sleeping.build_config().fur, Some(FUR_STOCK));
        let sad = Avatar::mascot(MascotMood::Sad, 64, 2.0);
        assert_eq!(sad.state, BotAvatarState::Default);
        assert_eq!(sad.build_config().face, BotAvatarFace::Mouth);
        let lost = Avatar::mascot(MascotMood::Lost, 64, 2.0);
        assert_eq!(lost.build_config().face, BotAvatarFace::Eyes);
        for mut avatar in [sleeping, sad, lost] {
            assert_eq!(avatar.sim.jump_config().every, 0.0);
            avatar.set_state(BotAvatarState::Working);
            assert_eq!(avatar.sim.jump_config().every, 0.0);
        }
    }

    #[test]
    fn set_state_and_rewind_share_one_clock() {
        let mut a = Avatar::new(BotAvatarType::Star, BotAvatarState::Default, 64, 2.0);
        let mut b = Avatar::new(BotAvatarType::Star, BotAvatarState::Default, 64, 2.0);
        a.advance(0.4);
        b.advance(0.4);
        assert_eq!(a.steps, b.steps);
        assert!(a.steps > 0);
        assert!(same_pose(&a.sim.pose, &b.sim.pose));
        let steps = a.steps;
        let pose = a.sim.pose;
        a.advance(0.4);
        assert_eq!(a.steps, steps);
        assert!(same_pose(&a.sim.pose, &pose));
        a.advance(1.0);
        assert!(a.steps > steps);
        a.advance(0.4);
        assert_eq!(a.steps, steps);
        assert!(same_pose(&a.sim.pose, &pose));
        a.set_state(BotAvatarState::Sleeping);
        assert_eq!(a.state, BotAvatarState::Sleeping);
        assert_eq!(a.sim.state, BotAvatarState::Sleeping);
        a.advance(-1.0);
        assert_eq!(a.steps, 0);
        assert_eq!(a.sim.state, BotAvatarState::Sleeping);
        a.advance(f64::NAN);
        assert_eq!(a.steps, 0);
    }

    #[test]
    fn a_pointer_turns_the_bot_toward_it() {
        // The pose, not a paint: painting fills the shared form and fur caches other tests count.
        let mut ahead = Avatar::for_agent("codex", "idle", 28, 1.0);
        ahead.advance(1.0);
        let mut looking = Avatar::for_agent("codex", "idle", 28, 1.0);
        looking.set_pointer(1.0, 0.0, 1.0);
        looking.advance(1.0);
        assert!(!same_pose(&looking.sim.pose, &ahead.sim.pose));
        looking.set_pointer(f64::NAN, 0.0, 1.0);
        assert_eq!(looking.pointer, (0.0, 0.0, 1.0));
    }

    #[test]
    fn a_gazing_bot_moves_only_its_eyes() {
        let rest = crate::engine::rest_pose(BotAvatarState::Working);
        let mut bot = Avatar::for_agent("codex", "running", 28, 1.0);
        bot.set_calm(crate::engine::Calm::Gaze);
        bot.set_pointer(1.0, 1.0, 1.0);
        bot.advance(3.0);
        let pose = bot.sim.pose;
        assert_eq!(
            (pose.yaw, pose.pitch, pose.roll),
            (rest.yaw, rest.pitch, rest.roll)
        );
        assert_eq!((pose.y, pose.sx, pose.sy), (rest.y, rest.sx, rest.sy));
        assert!(
            pose.look_x > rest.look_x + 3.0,
            "looks right: {}",
            pose.look_x
        );
        assert!(
            pose.look_y > rest.look_y + 2.0,
            "looks down: {}",
            pose.look_y
        );
    }

    #[test]
    fn a_busy_bot_glances_smiles_and_blinks_with_at_most_a_low_hop() {
        let rest = crate::engine::rest_pose(BotAvatarState::Working);
        let mut bot = Avatar::for_agent("codex", "running", 28, 1.0);
        bot.set_calm(crate::engine::Calm::Busy);
        let (mut glanced, mut smiled, mut blinked) = (false, false, false);
        for i in 1..=400 {
            bot.advance(f64::from(i) * 0.05);
            let pose = bot.sim.pose;
            // No turn or nod; a hop is short and low.
            assert_eq!((pose.yaw, pose.pitch), (rest.yaw, rest.pitch));
            assert!(
                (pose.y - rest.y).abs() <= 3.0 + 1e-9,
                "hop height {}",
                pose.y
            );
            glanced |= (pose.look_x - rest.look_x).abs() > 1.5;
            smiled |= pose.laugh > 0.8;
            blinked |= pose.blink_l > 0.8;
        }
        assert!(glanced && smiled && blinked, "{glanced} {smiled} {blinked}");
    }
}
