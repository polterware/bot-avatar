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

/// The eighteen body shapes.
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
}

/// Alias for [`BotAvatarType`].
pub type AvatarKind = BotAvatarType;

impl BotAvatarType {
    pub const ALL: [BotAvatarType; 18] = [
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
        Err(format!("unknown avatar kind: {s}"))
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum BotAvatarState {
    #[default]
    Default,
    Working,
    Sleeping,
}

/// Alias for [`BotAvatarState`].
pub type AvatarState = BotAvatarState;

impl BotAvatarState {
    pub const ALL: [BotAvatarState; 3] = [
        BotAvatarState::Default,
        BotAvatarState::Working,
        BotAvatarState::Sleeping,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            BotAvatarState::Default => "default",
            BotAvatarState::Working => "working",
            BotAvatarState::Sleeping => "sleeping",
        }
    }

    pub const fn index(self) -> usize {
        match self {
            BotAvatarState::Default => 0,
            BotAvatarState::Working => 1,
            BotAvatarState::Sleeping => 2,
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
        match s.trim().to_ascii_lowercase().as_str() {
            "default" | "idle" => Ok(BotAvatarState::Default),
            "working" => Ok(BotAvatarState::Working),
            "sleeping" => Ok(BotAvatarState::Sleeping),
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
