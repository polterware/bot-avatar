use crate::presets::BOT_AVATAR_TYPES;
use crate::types::{BotAvatarState, BotAvatarType};

/// Known agents use a library body that reads under the fur, plus the brand color.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AgentMark {
    pub id: &'static str,
    pub kind: BotAvatarType,
    pub color: &'static str,
}

struct MarkEntry {
    mark: AgentMark,
    aliases: &'static [&'static str],
}

const MARKS: &[MarkEntry] = &[
    MarkEntry {
        mark: AgentMark {
            id: "claude",
            kind: BotAvatarType::Star,
            color: "#D97757",
        },
        aliases: &["claude", "claudecode", "anthropic"],
    },
    MarkEntry {
        mark: AgentMark {
            id: "codex",
            kind: BotAvatarType::Flower,
            color: "#19C37D",
        },
        aliases: &["codex", "openai", "chatgpt"],
    },
    MarkEntry {
        mark: AgentMark {
            id: "cursor",
            kind: BotAvatarType::Square,
            color: "#F4F4F5",
        },
        aliases: &["cursor", "cursoragent"],
    },
    MarkEntry {
        mark: AgentMark {
            id: "gemini",
            kind: BotAvatarType::Triangle,
            color: "#8AB4F8",
        },
        aliases: &["gemini", "antigravity", "agy"],
    },
    MarkEntry {
        mark: AgentMark {
            id: "grok",
            kind: BotAvatarType::Hexagon,
            color: "#E4E4E7",
        },
        aliases: &["grok", "xai"],
    },
    MarkEntry {
        mark: AgentMark {
            id: "copilot",
            kind: BotAvatarType::Cat,
            color: "#EDEDED",
        },
        aliases: &["copilot", "githubcopilot"],
    },
    MarkEntry {
        mark: AgentMark {
            id: "windsurf",
            kind: BotAvatarType::Cloud,
            color: "#5EEAD4",
        },
        aliases: &["windsurf", "codeium"],
    },
    MarkEntry {
        mark: AgentMark {
            id: "continue",
            kind: BotAvatarType::Pill,
            color: "#F5C16C",
        },
        aliases: &["continue"],
    },
    MarkEntry {
        mark: AgentMark {
            id: "cline",
            kind: BotAvatarType::Ghost,
            color: "#7DD3FC",
        },
        aliases: &["cline"],
    },
    MarkEntry {
        mark: AgentMark {
            id: "roo",
            kind: BotAvatarType::Alien,
            color: "#E9D5FF",
        },
        aliases: &["roo", "roocode"],
    },
    MarkEntry {
        mark: AgentMark {
            id: "aider",
            kind: BotAvatarType::Pebble,
            color: "#86EFAC",
        },
        aliases: &["aider"],
    },
    MarkEntry {
        mark: AgentMark {
            id: "amp",
            kind: BotAvatarType::Drop,
            color: "#FDBA74",
        },
        aliases: &["amp"],
    },
    MarkEntry {
        mark: AgentMark {
            id: "zed",
            kind: BotAvatarType::Mech,
            color: "#93C5FD",
        },
        aliases: &["zed"],
    },
    MarkEntry {
        mark: AgentMark {
            id: "opencode",
            kind: BotAvatarType::Droid,
            color: "#CBD5E1",
        },
        aliases: &["opencode"],
    },
    MarkEntry {
        mark: AgentMark {
            id: "goose",
            kind: BotAvatarType::Blob,
            color: "#FDE68A",
        },
        aliases: &["goose"],
    },
    MarkEntry {
        mark: AgentMark {
            id: "junie",
            kind: BotAvatarType::Clover,
            color: "#FB923C",
        },
        aliases: &["junie", "jetbrains"],
    },
    MarkEntry {
        mark: AgentMark {
            id: "kiro",
            kind: BotAvatarType::Puddle,
            color: "#FCD34D",
        },
        aliases: &["kiro", "amazonq"],
    },
    MarkEntry {
        mark: AgentMark {
            id: "devin",
            kind: BotAvatarType::Circle,
            color: "#A5B4FC",
        },
        aliases: &["devin"],
    },
    MarkEntry {
        mark: AgentMark {
            id: "augment",
            kind: BotAvatarType::Hexagon,
            color: "#67E8F9",
        },
        aliases: &["augment"],
    },
    MarkEntry {
        mark: AgentMark {
            id: "cody",
            kind: BotAvatarType::Droid,
            color: "#C4B5FD",
        },
        aliases: &["cody", "sourcegraph"],
    },
    MarkEntry {
        mark: AgentMark {
            id: "qwen",
            kind: BotAvatarType::Drop,
            color: "#6EE7B7",
        },
        aliases: &["qwen", "qoder"],
    },
    MarkEntry {
        mark: AgentMark {
            id: "factory",
            kind: BotAvatarType::Mech,
            color: "#D6D3D1",
        },
        aliases: &["factory", "droid"],
    },
    MarkEntry {
        mark: AgentMark {
            id: "trae",
            kind: BotAvatarType::Triangle,
            color: "#FDA4AF",
        },
        aliases: &["trae"],
    },
    MarkEntry {
        mark: AgentMark {
            id: "crush",
            kind: BotAvatarType::Pebble,
            color: "#F9A8D4",
        },
        aliases: &["crush"],
    },
    MarkEntry {
        mark: AgentMark {
            id: "pi",
            kind: BotAvatarType::Circle,
            color: "#FDE047",
        },
        aliases: &["pi"],
    },
    MarkEntry {
        mark: AgentMark {
            id: "kimi",
            kind: BotAvatarType::Pill,
            color: "#A5B4FC",
        },
        aliases: &["kimi", "moonshot", "moonshotai"],
    },
];

/// The mark for a known agent label, or `None` so the caller keeps a stable hashed body.
pub fn agent_mark_for(label: &str) -> Option<AgentMark> {
    let lower = label.to_ascii_lowercase();
    let tokens: Vec<&str> = lower
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|s| !s.is_empty())
        .collect();
    if tokens.is_empty() {
        return None;
    }
    let joined: String = tokens.concat();
    for entry in MARKS {
        for &alias in entry.aliases {
            if tokens.contains(&alias) || joined == alias {
                return Some(entry.mark);
            }
            if alias.len() >= 5 && joined.contains(alias) {
                return Some(entry.mark);
            }
        }
    }
    None
}

/// FNV-1a 32-bit hash matching `hash32` / `hashSeed` in TypeScript.
pub fn hash32(id: &str) -> u32 {
    let mut h: u32 = 2_166_136_261;
    for code in id.encode_utf16() {
        h = (h ^ (code as u32)).wrapping_mul(16_777_619);
    }
    h
}

/// Fallback body shape hashed from an agent identifier (from Dispatch `agent-avatar.tsx`).
pub fn bot_type_for(id: &str) -> BotAvatarType {
    BOT_AVATAR_TYPES[(hash32(id) as usize) % BOT_AVATAR_TYPES.len()]
}

/// Deterministic 0..1 seed hashed from an identifier (from Dispatch `agent-avatar.tsx`).
pub fn seed_for(id: &str) -> f64 {
    ((hash32(id) % 1000) as f64) / 1000.0
}

/// Map a Dispatch run status string to a [`BotAvatarState`] (`agent-avatar.tsx`).
pub fn dispatch_bot_state(status: Option<&str>) -> BotAvatarState {
    let Some(s) = status else {
        return BotAvatarState::Default;
    };
    match s.to_ascii_lowercase().as_str() {
        "pending" | "running" => BotAvatarState::Working,
        "failed" | "cancelled" | "canceled" | "timed_out" => BotAvatarState::Sleeping,
        _ => BotAvatarState::Default,
    }
}
