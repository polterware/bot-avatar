//! Temporary: compare sidebar frames to /tmp/avatar-base. Deleted after the check.
use bot_avatar::Avatar;
use bot_avatar::types::{BotAvatarState, BotAvatarType};
use tiny_skia::Pixmap;

fn main() {
    let kinds = [
        BotAvatarType::Star,
        BotAvatarType::Droid,
        BotAvatarType::Ghost,
        BotAvatarType::Clover,
    ];
    let times = [0.0_f64, 0.4, 1.0, 2.5];
    let mut sum_mae = 0.0;
    let mut sum_ink = 0.0;
    let mut n = 0u32;
    let mut worst_mae = 0.0;
    let mut worst_ink = 0.0;
    let mut worst_max = 0u8;
    let mut worst_name = String::new();
    let mut changed = 0u64;
    let mut total = 0u64;
    for kind in kinds {
        for state in BotAvatarState::ALL {
            let mut avatar = Avatar::new(kind, state, 64, 2.0);
            for (i, t) in times.into_iter().enumerate() {
                let pm = avatar.render(t);
                let path = format!("/tmp/avatar-base/{kind}_{state}_{i}.png");
                let _ = std::fs::create_dir_all("/tmp/avatar-new");
                let _ = pm.save_png(format!("/tmp/avatar-new/{kind}_{state}_{i}.png"));
                let base = Pixmap::load_png(&path).unwrap_or_else(|e| panic!("{path}: {e:?}"));
                assert_eq!(pm.width(), base.width(), "{path}");
                assert_eq!(pm.height(), base.height(), "{path}");
                let mut max_d = 0u8;
                let mut acc = 0u64;
                let mut ink_acc = 0u64;
                let mut ink_n = 0u64;
                for (a, b) in pm.data().chunks_exact(4).zip(base.data().chunks_exact(4)) {
                    let mut pix = 0u32;
                    for ch in 0..4 {
                        let d = a[ch].abs_diff(b[ch]) as u32;
                        pix += d;
                        max_d = max_d.max(d as u8);
                        acc += d as u64;
                    }
                    if pix != 0 {
                        changed += 1;
                    }
                    total += 1;
                    if a[3] > 16 || b[3] > 16 {
                        ink_n += 1;
                        for ch in 0..4 {
                            ink_acc += a[ch].abs_diff(b[ch]) as u64;
                        }
                    }
                }
                let pixels = (pm.width() as u64 * pm.height() as u64) as f64;
                let mae = acc as f64 / (pixels * 4.0);
                let ink = if ink_n > 0 {
                    ink_acc as f64 / (ink_n as f64 * 4.0)
                } else {
                    0.0
                };
                println!("{kind} {state} t={t:.1} mae={mae:.3} ink={ink:.3} max={max_d}");
                if ink > worst_ink {
                    worst_ink = ink;
                    worst_mae = mae;
                    worst_name = format!("{kind} {state} t={t:.1}");
                }
                worst_max = worst_max.max(max_d);
                sum_mae += mae;
                sum_ink += ink;
                n += 1;
            }
        }
    }
    let nf = n as f64;
    println!(
        "frames={n} mean_mae={:.3} mean_ink={:.3} worst_ink={worst_ink:.3} ({worst_name} mae={worst_mae:.3}) max_channel={worst_max} changed_px={changed}/{total}",
        sum_mae / nf,
        sum_ink / nf
    );
}
