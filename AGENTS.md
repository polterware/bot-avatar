# bot-avatar Agent Instructions

## Communication and editing

- Speak to Erick in Brazilian Portuguese.
- Write code, UI strings, technical documentation, commits, logs, and identifiers in English. No Portuguese text anywhere in the repository: code, comments, UI strings, error and log messages, tests, fixtures, docs, and skills.
- Preserve unrelated work and make the smallest safe change.
- Do not create branches or commits unless explicitly requested.
- Do not run browsers, previews, or development servers unless the current task asks for them. Automated tests and builds may run by default.

## What this is

The Rust port of bot-avatars, the TypeScript bot avatar renderer by Jakub Antalik (MIT; see NOTICE), drawn with `tiny-skia`. `polterware-ui` draws the mascot and the agent bots with it.

- Rendering is deterministic: the same kind, colour, state, size, scale, and time render the same bytes. Keep it so, and keep the tests that hold it.
- `Avatar::for_agent` picks the body and brand colour the way Dispatch does; a new agent adds its mark there.
- Speed matters at sidebar size (64px at scale 2): run `cargo run --release --example bench` before and after a change to the painter.

## Verification

Run `cargo fmt --check`, `cargo clippy --all-targets`, and `cargo test` after relevant changes. Maintain risk-proportional tests.
