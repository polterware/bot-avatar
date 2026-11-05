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

`Avatar::new(kind, state, size_px, scale)` uses the library preset. There are 36 body shapes (`BotAvatarType::ALL`): the 18 classic ones and sparkle, heart, tablet, wedge, house, burst, bean, egg, squircle, capsule, cylinder, gem, crystal, shield, dome, arch, leaf, and pumpkin. An agent without a mark still hashes to one of the 18 classic shapes (`BotAvatarType::CLASSIC`), so new shapes never change an existing agent's body. `Avatar::for_agent(agent_id, status, size_px, scale)` picks the body and brand colour the way Dispatch `agent-avatar.tsx` does (`claude`, `codex`, `grok`, `cursor`, and the other marks), with fabric shading and the stock fur. `status` may be a bot state (`working`, `sleeping`, `default`) or a run status (`running`, `failed`, `idle`). `kind_for_agent` and `state_for_status` are the same lookups.

## Motion

`default`, `working`, and `sleeping` are the classic rig. Every other state (`BotAvatarState::ALL`, 54 in all) is played by the motion layer in `stage.rs`:

- **Clips** (`clips.rs`): keyframed rotation, position, and squash with per-key cubic béziers, face changes, blinks, and spin-ring windows. A state lists its clips; the director shuffles them per bot (seeded), never plays one twice in a row, and starts the first at a random point so bots in one state never move in step. Switching clips or states bridges from the current pose in 0.36 s and never unwinds a full turn.
- **Flow**: `finished`, `failed`, and `approving` play once and go idle; `waking` goes idle after 1.8 s; `blocked` turns `drowsy` after two minutes. `Avatar::state()` reports where it went.
- **Faces** (`expression.rs`): neutral, happy, surprised, sad, sleepy, squinted, calm, angry, confused, inspecting (the larger eye on the side the head turns to), wink, and dizzy. They are the classic eye stroke with other numbers, so they morph into each other (0.28 s) or swap behind a blink. Blinks open with a small overshoot. `set_expression(Some(face), through_blink)` holds a face over any state.
- **Glyphs** (`glyph.rs`): thinking, searching, locating, waiting, progress, spawning, dictating, sending, receiving, uploading, writing, alerting, bouncing, and powering-down fold the body into a sign (dots, radar, pin, orbit, ring, gathering balls, voice wave, send, tray, dock, pencil, `!`, bouncing ball, power) drawn in the body's colour. The body shrinks away with a half turn; progress and spawning come back to the body for a breath now and then.
- **Pokes** (`poke.rs`): `poke(Poke::Spin | Bounce | Nod | Shake | Wiggle | Flip, turns)` lays a short move over anything, the classic states included. `look(Look::Left | Right | Up | Down | Center)` holds a turn.
- **Pointer**: `set_pointer` turns the body toward the pointer in the motion states and moves the eyes, as in the classic rig. `Calm::Gaze` holds the body at rest; `Calm::Busy` plays the clips at about half size.

Pokes, looks, and faces are logged with the step they came at, so a rewind replays them where they happened and `render(t)` stays deterministic.

```rust
use bot_avatar::{Avatar, Expression, Look, Poke};
use bot_avatar::types::{BotAvatarState, BotAvatarType};

let mut bot = Avatar::new(BotAvatarType::Heart, BotAvatarState::Thinking, 64, 2.0);
bot.render(0.5);
bot.set_state(BotAvatarState::Streaming);
bot.poke(Poke::Spin, 1);
bot.look(Look::Left);
bot.set_expression(Some(Expression::Wink), false);
let _ = bot.render(1.0);
```

## Gallery

```shell
cargo run --release --example gallery -- /tmp/shapes.png all default 1.0 64
cargo run --release --example gallery -- film /tmp/frames blob all 12 64
cargo run --release --example gallery -- timeline /tmp/life blob,heart 22 "3:thinking,6:streaming,10:coding,16:finished,19.5:poke-spin" 96
```

`film` writes a frame per 1/30 s with every listed state side by side; `timeline` plays a list of `seconds:state`, `seconds:poke-<poke>`, `seconds:look-<look>`, or `seconds:face-<expression>` events.

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
