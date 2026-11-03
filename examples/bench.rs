//! `cargo run --release --example bench`
//!
//! Prints the cold first frame and the steady milliseconds per frame for every
//! kind and state at 64px, scale 2. Each steady figure is 600 frames after 60
//! warm-up frames.

use std::time::Instant;

use bot_avatar::Avatar;
use bot_avatar::types::{BotAvatarState, BotAvatarType};

fn main() {
    println!("kind state cold_ms steady_ms");
    for kind in BotAvatarType::ALL {
        for state in BotAvatarState::ALL {
            let mut avatar = Avatar::new(kind, state, 64, 2.0);
            let cold_at = Instant::now();
            let _ = avatar.render(0.0);
            let cold_ms = cold_at.elapsed().as_secs_f64() * 1000.0;
            for i in 0..60 {
                let _ = avatar.render((i + 1) as f64 / 60.0);
            }
            let started = Instant::now();
            for i in 0..600 {
                let _ = avatar.render((60 + i) as f64 / 60.0);
            }
            let steady = started.elapsed().as_secs_f64() * 1000.0 / 600.0;
            println!("{kind} {state} {cold_ms:.3} {steady:.3}");
        }
    }
}
