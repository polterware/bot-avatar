use std::fmt;
use std::str::FromStr;

/// Numeric value accepted for pixel sizes, display scale, and timestamps.
pub trait Scalar: Copy {
    fn to_f64(self) -> f64;
}

impl Scalar for f32 {
    #[inline]
    fn to_f64(self) -> f64 {
        self as f64
    }
}
impl Scalar for f64 {
    #[inline]
    fn to_f64(self) -> f64 {
        self
    }
}
impl Scalar for i32 {
    #[inline]
    fn to_f64(self) -> f64 {
        self as f64
    }
}
impl Scalar for u32 {
    #[inline]
    fn to_f64(self) -> f64 {
        self as f64
    }
}
impl Scalar for u16 {
    #[inline]
    fn to_f64(self) -> f64 {
        self as f64
    }
}
impl Scalar for usize {
    #[inline]
    fn to_f64(self) -> f64 {
        self as f64
    }
}

/// The body shapes: the eighteen classic ones, then the newer ones.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum BotAvatarType {
    #[default]
    Clover,
    Flower,
    Triangle,
    Square,
    Blob,
    Ghost,
    Circle,
    Drop,
    Star,
    Droid,
    Mech,
    Alien,
    Hexagon,
    Cat,
    Cloud,
    Pill,
    Pebble,
    Puddle,
    Sparkle,
    Heart,
    Tablet,
    Wedge,
    House,
    Burst,
    Bean,
    Egg,
    Squircle,
    Capsule,
    Cylinder,
    Gem,
    Crystal,
    Shield,
    Dome,
    Arch,
    Leaf,
    Pumpkin,
}

/// Alias for [`BotAvatarType`].
pub type AvatarKind = BotAvatarType;

impl BotAvatarType {
    /// Every body shape.
    pub const ALL: [BotAvatarType; 36] = [
        BotAvatarType::Clover,
        BotAvatarType::Flower,
        BotAvatarType::Triangle,
        BotAvatarType::Square,
        BotAvatarType::Blob,
        BotAvatarType::Ghost,
        BotAvatarType::Circle,
        BotAvatarType::Drop,
        BotAvatarType::Star,
        BotAvatarType::Droid,
        BotAvatarType::Mech,
        BotAvatarType::Alien,
        BotAvatarType::Hexagon,
        BotAvatarType::Cat,
        BotAvatarType::Cloud,
        BotAvatarType::Pill,
        BotAvatarType::Pebble,
        BotAvatarType::Puddle,
        BotAvatarType::Sparkle,
        BotAvatarType::Heart,
        BotAvatarType::Tablet,
        BotAvatarType::Wedge,
        BotAvatarType::House,
        BotAvatarType::Burst,
        BotAvatarType::Bean,
        BotAvatarType::Egg,
        BotAvatarType::Squircle,
        BotAvatarType::Capsule,
        BotAvatarType::Cylinder,
        BotAvatarType::Gem,
        BotAvatarType::Crystal,
        BotAvatarType::Shield,
        BotAvatarType::Dome,
        BotAvatarType::Arch,
        BotAvatarType::Leaf,
        BotAvatarType::Pumpkin,
    ];

    /// The eighteen classic shapes. An agent without a mark hashes to one of these, so a
    /// new shape never changes an existing agent's body.
    pub const CLASSIC: [BotAvatarType; 18] = [
        BotAvatarType::Clover,
        BotAvatarType::Flower,
        BotAvatarType::Triangle,
        BotAvatarType::Square,
        BotAvatarType::Blob,
        BotAvatarType::Ghost,
        BotAvatarType::Circle,
        BotAvatarType::Drop,
        BotAvatarType::Star,
        BotAvatarType::Droid,
        BotAvatarType::Mech,
        BotAvatarType::Alien,
        BotAvatarType::Hexagon,
        BotAvatarType::Cat,
        BotAvatarType::Cloud,
        BotAvatarType::Pill,
        BotAvatarType::Pebble,
        BotAvatarType::Puddle,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            BotAvatarType::Clover => "clover",
            BotAvatarType::Flower => "flower",
            BotAvatarType::Triangle => "triangle",
            BotAvatarType::Square => "square",
            BotAvatarType::Blob => "blob",
            BotAvatarType::Ghost => "ghost",
            BotAvatarType::Circle => "circle",
            BotAvatarType::Drop => "drop",
            BotAvatarType::Star => "star",
            BotAvatarType::Droid => "droid",
            BotAvatarType::Mech => "mech",
            BotAvatarType::Alien => "alien",
            BotAvatarType::Hexagon => "hexagon",
            BotAvatarType::Cat => "cat",
            BotAvatarType::Cloud => "cloud",
            BotAvatarType::Pill => "pill",
            BotAvatarType::Pebble => "pebble",
            BotAvatarType::Puddle => "puddle",
            BotAvatarType::Sparkle => "sparkle",
            BotAvatarType::Heart => "heart",
            BotAvatarType::Tablet => "tablet",
            BotAvatarType::Wedge => "wedge",
            BotAvatarType::House => "house",
            BotAvatarType::Burst => "burst",
            BotAvatarType::Bean => "bean",
            BotAvatarType::Egg => "egg",
            BotAvatarType::Squircle => "squircle",
            BotAvatarType::Capsule => "capsule",
            BotAvatarType::Cylinder => "cylinder",
            BotAvatarType::Gem => "gem",
            BotAvatarType::Crystal => "crystal",
            BotAvatarType::Shield => "shield",
            BotAvatarType::Dome => "dome",
            BotAvatarType::Arch => "arch",
            BotAvatarType::Leaf => "leaf",
            BotAvatarType::Pumpkin => "pumpkin",
        }
    }
}

impl fmt::Display for BotAvatarType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for BotAvatarType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let lower = s.trim().to_ascii_lowercase();
        for kind in Self::ALL {
            if kind.as_str() == lower {
                return Ok(kind);
            }
        }
        let alias = match lower.as_str() {
            "sphere" => Some(BotAvatarType::Circle),
            "teardrop" => Some(BotAvatarType::Drop),
            "hex" => Some(BotAvatarType::Hexagon),
            "star6" | "star-6" => Some(BotAvatarType::Burst),
            "horseshoe" => Some(BotAvatarType::Arch),
            "spark" => Some(BotAvatarType::Sparkle),
            _ => None,
        };
        alias.ok_or_else(|| format!("unknown avatar kind: {s}"))
    }
}

/// What the face is made of. The eyes alone by default; `Mouth` adds a small
/// mouth that changes with the state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum BotAvatarFace {
    #[default]
    Eyes,
    Mouth,
}

impl BotAvatarFace {
    pub const ALL: [BotAvatarFace; 2] = [BotAvatarFace::Eyes, BotAvatarFace::Mouth];

    pub const fn as_str(self) -> &'static str {
        match self {
            BotAvatarFace::Eyes => "eyes",
            BotAvatarFace::Mouth => "mouth",
        }
    }
}

/// What the bot is doing. Each state is a pose plus its own motion.
///
/// The first three are the classic rig. The others are played by the motion layer:
/// keyframed clips picked per state, an expression, and for some a glyph the body folds into.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum BotAvatarState {
    #[default]
    /// Idle: the classic rig looking around, with a flip now and then.
    Default,
    /// At work: the classic rig with hops and a laugh.
    Working,
    /// Asleep: the classic rig, nodding.
    Sleeping,
    /// Leaning in, calm, nodding along.
    Listening,
    /// The body folds into three bouncing dots.
    Thinking,
    /// Swaying in time while a reply streams in.
    Streaming,
    /// The body folds into a radar sweep.
    Searching,
    /// The body folds into a map pin that drops and pulses.
    Locating,
    /// Scanning left to right, head down.
    Reading,
    /// Busy at work: turns, flips, and tilts.
    Coding,
    /// Running a task: the working clips.
    Executing,
    /// Crunching: the working clips.
    Computing,
    /// Driving a computer: the working clips.
    Controlling,
    /// The body folds into a ball orbited by two moons.
    Waiting,
    /// Bumped into something; drowsy after two minutes.
    Blocked,
    /// A happy hop, then idle.
    Finished,
    /// Droops, sad, then idle.
    Failed,
    /// Stretches awake, surprised, then idle.
    Waking,
    /// Happy bounces.
    Happy,
    /// Quick bounces, eyes wide.
    Excited,
    /// Shakes with laughter.
    Laughing,
    /// Chin up, puffed.
    Proud,
    /// Tilts and leans in, one eye bigger.
    Curious,
    /// Bops side to side, winking.
    Playful,
    /// Turns away, looking down.
    Shy,
    /// Startled jump, eyes wide.
    Surprised,
    /// A slow side-eye, squinting.
    Suspicious,
    /// Stomps and shakes.
    Angry,
    /// Tilts one way, then the other.
    Confused,
    /// Droops and sways.
    Sad,
    /// Nodding off.
    Drowsy,
    /// Sighs and slumps.
    Bored,
    /// Shivers, crouched.
    Scared,
    /// Twirls twice with a hop.
    Celebrate,
    /// Two nods, then idle.
    Approving,
    /// The orbit glyph.
    Orbit,
    /// The radar glyph.
    Radar,
    /// The body folds into a ring that fills, then comes back for a breath.
    Progress,
    /// Little balls gather and pop into one, then the body comes back.
    Spawning,
    /// The progress glyph.
    Loading,
    /// The body folds into a row of dots waving like a voice.
    Dictating,
    /// The body folds into a ball that shoots off and comes back.
    Sending,
    /// The body folds into a ball that drops into a tray.
    Receiving,
    /// The body folds into a ball that rises into a dock.
    Uploading,
    /// The body folds into a pencil scribbling.
    Writing,
    /// The body folds into a bouncing exclamation mark.
    Alerting,
    /// The body folds into a bouncing ball.
    Bouncing,
    /// The body folds into a dimming power sign.
    PoweringDown,
    /// A slow calm sway.
    Humming,
    /// A jump with wide eyes.
    Notifying,
    /// Dangling like a pendulum.
    Dragging,
    /// A clean turn for a logo.
    Logo,
    /// A tour of poses.
    Showcase,
}

/// Alias for [`BotAvatarState`].
pub type AvatarState = BotAvatarState;

impl BotAvatarState {
    /// Every state.
    pub const ALL: [BotAvatarState; 53] = [
        BotAvatarState::Default,
        BotAvatarState::Working,
        BotAvatarState::Sleeping,
        BotAvatarState::Listening,
        BotAvatarState::Thinking,
        BotAvatarState::Streaming,
        BotAvatarState::Searching,
        BotAvatarState::Locating,
        BotAvatarState::Reading,
        BotAvatarState::Coding,
        BotAvatarState::Executing,
        BotAvatarState::Computing,
        BotAvatarState::Controlling,
        BotAvatarState::Waiting,
        BotAvatarState::Blocked,
        BotAvatarState::Finished,
        BotAvatarState::Failed,
        BotAvatarState::Waking,
        BotAvatarState::Happy,
        BotAvatarState::Excited,
        BotAvatarState::Laughing,
        BotAvatarState::Proud,
        BotAvatarState::Curious,
        BotAvatarState::Playful,
        BotAvatarState::Shy,
        BotAvatarState::Surprised,
        BotAvatarState::Suspicious,
        BotAvatarState::Angry,
        BotAvatarState::Confused,
        BotAvatarState::Sad,
        BotAvatarState::Drowsy,
        BotAvatarState::Bored,
        BotAvatarState::Scared,
        BotAvatarState::Celebrate,
        BotAvatarState::Approving,
        BotAvatarState::Orbit,
        BotAvatarState::Radar,
        BotAvatarState::Progress,
        BotAvatarState::Spawning,
        BotAvatarState::Loading,
        BotAvatarState::Dictating,
        BotAvatarState::Sending,
        BotAvatarState::Receiving,
        BotAvatarState::Uploading,
        BotAvatarState::Writing,
        BotAvatarState::Alerting,
        BotAvatarState::Bouncing,
        BotAvatarState::PoweringDown,
        BotAvatarState::Humming,
        BotAvatarState::Notifying,
        BotAvatarState::Dragging,
        BotAvatarState::Logo,
        BotAvatarState::Showcase,
    ];

    /// The three states of the classic rig.
    pub const CLASSIC: [BotAvatarState; 3] = [
        BotAvatarState::Default,
        BotAvatarState::Working,
        BotAvatarState::Sleeping,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            BotAvatarState::Default => "default",
            BotAvatarState::Working => "working",
            BotAvatarState::Sleeping => "sleeping",
            BotAvatarState::Listening => "listening",
            BotAvatarState::Thinking => "thinking",
            BotAvatarState::Streaming => "streaming",
            BotAvatarState::Searching => "searching",
            BotAvatarState::Locating => "locating",
            BotAvatarState::Reading => "reading",
            BotAvatarState::Coding => "coding",
            BotAvatarState::Executing => "executing",
            BotAvatarState::Computing => "computing",
            BotAvatarState::Controlling => "controlling",
            BotAvatarState::Waiting => "waiting",
            BotAvatarState::Blocked => "blocked",
            BotAvatarState::Finished => "finished",
            BotAvatarState::Failed => "failed",
            BotAvatarState::Waking => "waking",
            BotAvatarState::Happy => "happy",
            BotAvatarState::Excited => "excited",
            BotAvatarState::Laughing => "laughing",
            BotAvatarState::Proud => "proud",
            BotAvatarState::Curious => "curious",
            BotAvatarState::Playful => "playful",
            BotAvatarState::Shy => "shy",
            BotAvatarState::Surprised => "surprised",
            BotAvatarState::Suspicious => "suspicious",
            BotAvatarState::Angry => "angry",
            BotAvatarState::Confused => "confused",
            BotAvatarState::Sad => "sad",
            BotAvatarState::Drowsy => "drowsy",
            BotAvatarState::Bored => "bored",
            BotAvatarState::Scared => "scared",
            BotAvatarState::Celebrate => "celebrate",
            BotAvatarState::Approving => "approving",
            BotAvatarState::Orbit => "orbit",
            BotAvatarState::Radar => "radar",
            BotAvatarState::Progress => "progress",
            BotAvatarState::Spawning => "spawning",
            BotAvatarState::Loading => "loading",
            BotAvatarState::Dictating => "dictating",
            BotAvatarState::Sending => "sending",
            BotAvatarState::Receiving => "receiving",
            BotAvatarState::Uploading => "uploading",
            BotAvatarState::Writing => "writing",
            BotAvatarState::Alerting => "alerting",
            BotAvatarState::Bouncing => "bouncing",
            BotAvatarState::PoweringDown => "powering-down",
            BotAvatarState::Humming => "humming",
            BotAvatarState::Notifying => "notifying",
            BotAvatarState::Dragging => "dragging",
            BotAvatarState::Logo => "logo",
            BotAvatarState::Showcase => "showcase",
        }
    }

    /// Whether the classic rig plays this state on its own (no motion layer).
    pub const fn is_classic(self) -> bool {
        matches!(
            self,
            BotAvatarState::Default | BotAvatarState::Working | BotAvatarState::Sleeping
        )
    }

    /// The classic state whose rig runs under this one.
    pub const fn base(self) -> BotAvatarState {
        match self {
            BotAvatarState::Working => BotAvatarState::Working,
            BotAvatarState::Sleeping => BotAvatarState::Sleeping,
            _ => BotAvatarState::Default,
        }
    }

    /// Index of [`Self::base`] in [`Self::CLASSIC`].
    pub const fn index(self) -> usize {
        match self.base() {
            BotAvatarState::Working => 1,
            BotAvatarState::Sleeping => 2,
            _ => 0,
        }
    }
}

impl fmt::Display for BotAvatarState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for BotAvatarState {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let lower = s.trim().to_ascii_lowercase();
        if lower == "idle" {
            return Ok(BotAvatarState::Default);
        }
        let lower = lower.replace('_', "-");
        for state in Self::ALL {
            if state.as_str() == lower {
                return Ok(state);
            }
        }
        match lower.as_str() {
            "poweringdown" | "power-down" => Ok(BotAvatarState::PoweringDown),
            "done" => Ok(BotAvatarState::Finished),
            _ => Err(format!("unknown avatar state: {s}")),
        }
    }
}

/// How the landing squash of a jump plays out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum BotAvatarSquashEase {
    Sharp,
    #[default]
    Pulse,
    Soft,
    Bouncy,
}

/// How the body is lit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum BotAvatarShading {
    Crisp,
    Smooth,
    Plastic,
    Flat,
    #[default]
    Fabric,
}

impl BotAvatarShading {
    pub const ALL: [BotAvatarShading; 5] = [
        BotAvatarShading::Crisp,
        BotAvatarShading::Smooth,
        BotAvatarShading::Plastic,
        BotAvatarShading::Flat,
        BotAvatarShading::Fabric,
    ];

    pub const fn is_material(self) -> bool {
        matches!(self, BotAvatarShading::Plastic | BotAvatarShading::Fabric)
    }
}

/// Something to wear on the head.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum BotAvatarHat {
    #[default]
    None,
    Beret,
    Beanie,
    Party,
    Crown,
}

/// Something to wear over the eyes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum BotAvatarGlasses {
    #[default]
    None,
    Round,
    Square,
    Shades,
}

/// Surface theme for backdrop-dependent touches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum BotAvatarTheme {
    #[default]
    Dark,
    Light,
}

/// Static preset metadata for one body shape.
#[derive(Debug, Clone, Copy)]
pub struct BotAvatarPreset {
    pub label: &'static str,
    pub color: &'static str,
    pub face: BotAvatarFace,
    pub face_x: f64,
    pub face_y: f64,
    pub face_scale: f64,
    pub brightness: Option<f64>,
    pub saturation: Option<f64>,
}

/// Fixed orientation override in radians.
#[derive(Debug, Clone, Copy, Default)]
pub struct HeldPose {
    pub yaw: Option<f64>,
    pub pitch: Option<f64>,
    pub roll: Option<f64>,
}
