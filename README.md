# bot-avatar

Rust port of the Polterware bot avatar renderer. It draws the fabric body, fur, and face into a premultiplied RGBA pixmap with `tiny-skia`. The same time always renders the same bytes.

## API

```rust
use bot_avatar::Avatar;
use bot_avatar::types::{BotAvatarState, BotAvatarType};

let mut avatar = Avatar::new(BotAvatarType::Star, BotAvatarState::Working, 64, 2.0);
let pixmap = avatar.render(0.4);

let mut claude = Avatar::for_agent("claude", "running", 28, 2.0);
let _ = claude.render(0.0);
```

`Avatar::new(kind, state, size_px, scale)` uses the library preset. `Avatar::for_agent(agent_id, status, size_px, scale)` picks the body and brand colour the way Dispatch `agent-avatar.tsx` does (`claude`, `codex`, `grok`, `cursor`, and the other marks), with fabric shading and the stock fur. `status` may be a bot state (`working`, `sleeping`, `default`) or a run status (`running`, `failed`, `idle`). `kind_for_agent` and `state_for_status` are the same lookups.

The device canvas is `round(size_px * 1.5 * scale)` pixels on a side. At 64px scale 2 that is 192.

## Example

```shell
cargo run --release --example render -- star working /tmp/frames
cargo run --release --example render -- claude working /tmp/frames
```

The example writes 12 PNGs across 2 seconds at 128px scale 2, then prints milliseconds per frame for 600 frames at 64px scale 2. A kind name wins over an agent id, so `droid` stays the droid body.

## Bench

```shell
cargo run --release --example bench
```

Prints the cold first frame and the steady ms/frame for every kind and state at 64px scale 2 (600 frames after 60 warm-up frames).

## Determinism

`render(t)` depends only on kind, colour, state, size, scale, and time. Rewinding to an earlier time replays from t = 0. Fur, matcaps, and body forms are baked once and reused. At sidebar size (device width up to 224, which is 64px at scale 2) the body is one front-view card plus a darkened depth rim, blitted with the pose. A hard turn does not rebuild fur and does not show slice parallax or a moving highlight. Larger sizes keep the full slice painter.

## License

Apache-2.0. The renderer is a port of bot-avatars by Jakub Antalik (MIT); see [NOTICE](NOTICE).
