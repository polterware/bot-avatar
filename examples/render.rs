//! `cargo run --release --example render -- <kind> <state> <out_dir>`
//!
//! Writes 12 PNGs across 2 seconds at 128px, scale 2, then times 600 frames
//! at 64px, scale 2, and prints milliseconds per frame.

use std::process::ExitCode;
use std::time::Instant;

use bot_avatar::Avatar;
use bot_avatar::types::{BotAvatarState, BotAvatarType};

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let Some(kind_s) = args.next() else {
        return usage();
    };
    let Some(state_s) = args.next() else {
        return usage();
    };
    let Some(out_dir) = args.next() else {
        return usage();
    };
    let state = match state_s.parse::<BotAvatarState>() {
        Ok(state) => state,
        Err(err) => {
            eprintln!("{err}");
            return usage();
        }
    };
    if let Err(err) = std::fs::create_dir_all(&out_dir) {
        eprintln!("create {out_dir}: {err}");
        return ExitCode::from(1);
    }

    let mut avatar = avatar_for(&kind_s, state, 128, 2.0);
    for i in 0..12 {
        let t = i as f64 * 2.0 / 12.0;
        let pixmap = avatar.render(t);
        let path = std::path::PathBuf::from(&out_dir).join(format!("{kind_s}_{state}_{i:02}.png"));
        if let Err(err) = pixmap.save_png(&path) {
            eprintln!("save {}: {err}", path.display());
            return ExitCode::from(1);
        }
    }

    let mut bench = avatar_for(&kind_s, state, 64, 2.0);
    let started = Instant::now();
    for i in 0..600 {
        let _pixmap = bench.render(i as f64 / 60.0);
    }
    let ms = started.elapsed().as_secs_f64() * 1000.0 / 600.0;
    println!("{ms:.3} ms/frame");
    ExitCode::SUCCESS
}

fn avatar_for(kind_s: &str, state: BotAvatarState, size: u32, scale: f32) -> Avatar {
    if let Ok(kind) = kind_s.parse::<BotAvatarType>() {
        Avatar::new(kind, state, size, scale)
    } else {
        Avatar::for_agent(kind_s, state.as_str(), size, scale)
    }
}

fn usage() -> ExitCode {
    eprintln!("Usage: render <kind-or-agent> <state> <out_dir>");
    let kinds: Vec<&str> = BotAvatarType::ALL.iter().map(|k| k.as_str()).collect();
    eprintln!("kind: {}", kinds.join(", "));
    eprintln!("agent: claude, codex, grok, cursor, gemini, copilot, ...");
    let states: Vec<&str> = BotAvatarState::ALL.iter().map(|s| s.as_str()).collect();
    eprintln!("state: {}", states.join(", "));
    ExitCode::from(2)
}
