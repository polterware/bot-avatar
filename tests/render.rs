//! Pixel checks for the full renderer.

use bot_avatar::Avatar;
use bot_avatar::types::{BotAvatarState, BotAvatarType};

#[test]
fn every_type_and_state_has_opaque_pixels() {
    for kind in BotAvatarType::ALL {
        for state in BotAvatarState::ALL {
            let mut avatar = Avatar::new(kind, state, 64, 2.0);
            let pixmap = avatar.render(0.25);
            assert_eq!(pixmap.width(), 192, "{kind} {state}");
            assert_eq!(pixmap.height(), 192, "{kind} {state}");
            let opaque = pixmap.data().chunks_exact(4).any(|px| px[3] == 255);
            assert!(opaque, "{kind} {state} has no opaque pixel");
        }
    }
}

#[test]
fn same_time_is_identical() {
    let mut a = Avatar::new(BotAvatarType::Clover, BotAvatarState::Default, 64, 2.0);
    let mut b = Avatar::new(BotAvatarType::Clover, BotAvatarState::Default, 64, 2.0);
    let first = a.render(0.4).data().to_vec();
    let second = b.render(0.4).data().to_vec();
    assert_eq!(first, second);
    let again = a.render(0.4).data().to_vec();
    assert_eq!(first, again);
    let _later = a.render(1.0);
    let rewound = a.render(0.4).data().to_vec();
    assert_eq!(first, rewound);
}
