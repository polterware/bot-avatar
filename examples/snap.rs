//! Temporary pixel baseline. Deleted after the comparison.
use bot_avatar::Avatar;
use bot_avatar::types::{BotAvatarState, BotAvatarType};

fn main() {
    let dir = "/tmp/avatar-base";
    std::fs::create_dir_all(dir).unwrap();
    let kinds = [
        BotAvatarType::Star,
        BotAvatarType::Droid,
        BotAvatarType::Ghost,
        BotAvatarType::Clover,
    ];
    for kind in kinds {
        for state in BotAvatarState::ALL {
            let mut avatar = Avatar::new(kind, state, 64, 2.0);
            for (i, t) in [0.0, 0.4, 1.0, 2.5].into_iter().enumerate() {
                let pm = avatar.render(t);
                let path = format!("{dir}/{kind}_{state}_{i}.png");
                pm.save_png(path).unwrap();
            }
        }
    }
}
