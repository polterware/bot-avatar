//! `cargo run --release --example bench [kinds] [states] [frames]`
//!
//! Prints the cold first frame and the steady milliseconds per frame for every
//! kind and state at 64px, scale 2. Each steady figure is `frames` (600) frames after 60
//! warm-up frames. `kinds` and `states` are comma lists, or `all`; `classic` is the 18
//! classic bodies or the three classic states. The last line is the mean and the max.

use std::time::Instant;

use bot_avatar::Avatar;
use bot_avatar::types::{BotAvatarState, BotAvatarType};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let kinds: Vec<BotAvatarType> = match args.first().map(String::as_str) {
        None | Some("all") => BotAvatarType::ALL.to_vec(),
        Some("classic") => BotAvatarType::CLASSIC.to_vec(),
        Some(list) => list.split(',').map(|k| k.parse().expect("kind")).collect(),
    };
    let states: Vec<BotAvatarState> = match args.get(1).map(String::as_str) {
        None | Some("all") => BotAvatarState::ALL.to_vec(),
        Some("classic") => BotAvatarState::CLASSIC.to_vec(),
        Some(list) => list.split(',').map(|s| s.parse().expect("state")).collect(),
    };
    let frames: usize = args.get(2).map_or(600, |n| n.parse().expect("frames"));
    let (mut sum, mut max, mut n) = (0.0_f64, 0.0_f64, 0.0_f64);
    println!("kind state cold_ms steady_ms");
    for &kind in &kinds {
        for &state in &states {
            let mut avatar = Avatar::new(kind, state, 64, 2.0);
            let cold_at = Instant::now();
            let _ = avatar.render(0.0);
            let cold_ms = cold_at.elapsed().as_secs_f64() * 1000.0;
            for i in 0..60 {
                let _ = avatar.render((i + 1) as f64 / 60.0);
            }
            let started = Instant::now();
            for i in 0..frames {
                let _ = avatar.render((60 + i) as f64 / 60.0);
            }
            let steady = started.elapsed().as_secs_f64() * 1000.0 / frames as f64;
            println!("{kind} {state} {cold_ms:.3} {steady:.3}");
            sum += steady;
            max = max.max(steady);
            n += 1.0;
        }
    }
    println!("mean {:.3} max {max:.3}", sum / n.max(1.0));
}
