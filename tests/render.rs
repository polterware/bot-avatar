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

#[test]
fn a_poke_replays_where_it_happened_after_a_rewind() {
    use bot_avatar::Poke;
    let mut a = Avatar::new(BotAvatarType::Circle, BotAvatarState::Coding, 48, 1.0);
    a.render(0.5);
    a.poke(Poke::Spin, 1);
    let first = a.render(0.9).data().to_vec();
    let _later = a.render(2.0);
    let rewound = a.render(0.9).data().to_vec();
    assert_eq!(first, rewound);
    let mut plain = Avatar::new(BotAvatarType::Circle, BotAvatarState::Coding, 48, 1.0);
    assert_ne!(first, plain.render(0.9).data().to_vec());
}

#[test]
fn finished_moves_on_to_idle_by_itself() {
    let mut a = Avatar::new(BotAvatarType::Blob, BotAvatarState::Default, 48, 1.0);
    a.render(0.2);
    a.set_state(BotAvatarState::Finished);
    a.render(1.0);
    assert_eq!(a.state(), BotAvatarState::Finished);
    a.render(8.0);
    assert_eq!(a.state(), BotAvatarState::Default);
}

#[test]
fn a_glyph_state_draws_its_glyph_not_the_body() {
    let mut body = Avatar::new(BotAvatarType::Square, BotAvatarState::Default, 64, 1.0);
    let mut dots = Avatar::new(BotAvatarType::Square, BotAvatarState::Thinking, 64, 1.0);
    let lit = |data: &[u8]| data.chunks_exact(4).filter(|px| px[3] > 128).count();
    let body_px = lit(body.render(1.0).data());
    let dots_px = lit(dots.render(1.0).data());
    assert!(dots_px > 50, "the dots show: {dots_px}");
    assert!(
        dots_px * 4 < body_px,
        "and the body is gone: {dots_px} vs {body_px}"
    );
}
