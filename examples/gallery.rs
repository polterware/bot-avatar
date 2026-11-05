//! `cargo run --release --example gallery -- <out.png> [kinds] [states] [time] [size]`
//!
//! Renders a grid into one PNG: a row per state, a column per kind. `kinds` and `states`
//! are comma lists, or `all`. Defaults: every kind, `default`, t = 1.0, 64px at scale 2.
//!
//! `cargo run --release --example gallery -- film <out_dir> <kind> <states> <seconds> [size]`
//! writes one PNG per frame at 30 fps, a column per state, for a video.
//!
//! `cargo run --release --example gallery -- timeline <out_dir> <kinds> <seconds> <events> [size]`
//! films one bot per kind through `events`, a comma list of `seconds:state`,
//! `seconds:poke-<poke>`, `seconds:look-<look>`, or `seconds:face-<expression>`.

use std::process::ExitCode;

use bot_avatar::types::{BotAvatarState, BotAvatarType};
use bot_avatar::{Avatar, Expression, Look, Poke};
use tiny_skia::{Pixmap, PixmapPaint, Transform};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("film") {
        return film(&args[1..]);
    }
    if args.first().map(String::as_str) == Some("timeline") {
        return timeline(&args[1..]);
    }
    let Some(out) = args.first() else {
        eprintln!("Usage: gallery <out.png> [kinds] [states] [time] [size]");
        return ExitCode::from(2);
    };
    let kinds = parse_list(args.get(1), &BotAvatarType::ALL);
    let states = parse_list(args.get(2), &[BotAvatarState::Default]);
    let time: f64 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(1.0);
    let size: u32 = args.get(4).and_then(|s| s.parse().ok()).unwrap_or(64);
    let (Some(kinds), Some(states)) = (kinds, states) else {
        return ExitCode::from(2);
    };
    let mut cells = Vec::new();
    for state in &states {
        for kind in &kinds {
            let mut avatar = Avatar::new(*kind, *state, size, 2.0);
            cells.push(avatar.render(time).clone());
        }
    }
    let sheet = tile(&cells, kinds.len());
    if let Err(err) = sheet.save_png(out) {
        eprintln!("save {out}: {err}");
        return ExitCode::from(1);
    }
    ExitCode::SUCCESS
}

fn film(args: &[String]) -> ExitCode {
    let (Some(dir), Some(kind), Some(states), Some(seconds)) =
        (args.first(), args.get(1), args.get(2), args.get(3))
    else {
        eprintln!("Usage: gallery film <out_dir> <kind> <states> <seconds> [size]");
        return ExitCode::from(2);
    };
    let Ok(kind) = kind.parse::<BotAvatarType>() else {
        eprintln!("unknown kind {kind}");
        return ExitCode::from(2);
    };
    let Some(states) = parse_list(Some(states), &BotAvatarState::ALL) else {
        return ExitCode::from(2);
    };
    let seconds: f64 = seconds.parse().unwrap_or(4.0);
    let size: u32 = args.get(4).and_then(|s| s.parse().ok()).unwrap_or(64);
    if let Err(err) = std::fs::create_dir_all(dir) {
        eprintln!("create {dir}: {err}");
        return ExitCode::from(1);
    }
    let mut avatars: Vec<Avatar> = states
        .iter()
        .map(|state| Avatar::new(kind, *state, size, 2.0))
        .collect();
    let frames = (seconds * 30.0).round() as u32;
    let cols = avatars.len().min(8);
    for frame in 0..frames {
        let t = f64::from(frame) / 30.0;
        let cells: Vec<Pixmap> = avatars.iter_mut().map(|a| a.render(t).clone()).collect();
        let sheet = tile(&cells, cols);
        let path = format!("{dir}/f{frame:04}.png");
        if let Err(err) = sheet.save_png(&path) {
            eprintln!("save {path}: {err}");
            return ExitCode::from(1);
        }
    }
    ExitCode::SUCCESS
}

fn timeline(args: &[String]) -> ExitCode {
    let (Some(dir), Some(kinds), Some(seconds), Some(spec)) =
        (args.first(), args.get(1), args.get(2), args.get(3))
    else {
        eprintln!("Usage: gallery timeline <out_dir> <kinds> <seconds> <events> [size]");
        return ExitCode::from(2);
    };
    let Some(kinds) = parse_list::<BotAvatarType>(Some(kinds), &BotAvatarType::ALL) else {
        return ExitCode::from(2);
    };
    let seconds: f64 = seconds.parse().unwrap_or(10.0);
    let size: u32 = args.get(4).and_then(|s| s.parse().ok()).unwrap_or(64);
    let mut events: Vec<(f64, String)> = Vec::new();
    for item in spec.split(',').filter(|s| !s.is_empty()) {
        let Some((t, what)) = item.split_once(':') else {
            eprintln!("bad event {item}");
            return ExitCode::from(2);
        };
        events.push((t.parse().unwrap_or(0.0), what.to_string()));
    }
    if let Err(err) = std::fs::create_dir_all(dir) {
        eprintln!("create {dir}: {err}");
        return ExitCode::from(1);
    }
    let mut avatars: Vec<Avatar> = kinds
        .iter()
        .map(|kind| Avatar::new(*kind, BotAvatarState::Default, size, 2.0))
        .collect();
    let mut next = 0;
    let frames = (seconds * 30.0).round() as u32;
    for frame in 0..frames {
        let t = f64::from(frame) / 30.0;
        while next < events.len() && events[next].0 <= t {
            let what = events[next].1.as_str();
            for avatar in &mut avatars {
                if let Some(p) = what.strip_prefix("poke-") {
                    if let Ok(p) = p.parse::<Poke>() {
                        avatar.poke(p, 1);
                    }
                } else if let Some(l) = what.strip_prefix("look-") {
                    let look = match l {
                        "left" => Look::Left,
                        "right" => Look::Right,
                        "up" => Look::Up,
                        "down" => Look::Down,
                        _ => Look::Center,
                    };
                    avatar.look(look);
                } else if let Some(f) = what.strip_prefix("face-") {
                    avatar.set_expression(f.parse::<Expression>().ok(), false);
                } else if let Ok(state) = what.parse::<BotAvatarState>() {
                    avatar.set_state(state);
                }
            }
            next += 1;
        }
        let cells: Vec<Pixmap> = avatars.iter_mut().map(|a| a.render(t).clone()).collect();
        let sheet = tile(&cells, cells.len().min(8));
        let path = format!("{dir}/f{frame:04}.png");
        if let Err(err) = sheet.save_png(&path) {
            eprintln!("save {path}: {err}");
            return ExitCode::from(1);
        }
    }
    ExitCode::SUCCESS
}

fn parse_list<T: std::str::FromStr + Copy>(arg: Option<&String>, all: &[T]) -> Option<Vec<T>>
where
    T::Err: std::fmt::Display,
{
    match arg.map(String::as_str) {
        None => Some(all.to_vec()),
        Some("all") => Some(all.to_vec()),
        Some(list) => {
            let mut out = Vec::new();
            for item in list.split(',') {
                match item.parse::<T>() {
                    Ok(v) => out.push(v),
                    Err(err) => {
                        eprintln!("{err}");
                        return None;
                    }
                }
            }
            Some(out)
        }
    }
}

fn tile(cells: &[Pixmap], cols: usize) -> Pixmap {
    let w = cells.first().map_or(1, Pixmap::width);
    let h = cells.first().map_or(1, Pixmap::height);
    let cols = cols.max(1);
    let rows = cells.len().div_ceil(cols).max(1);
    let mut sheet = Pixmap::new(w * cols as u32, h * rows as u32).expect("sheet size");
    sheet.fill(tiny_skia::Color::from_rgba8(22, 22, 22, 255));
    for (i, cell) in cells.iter().enumerate() {
        let x = (i % cols) as i32 * w as i32;
        let y = (i / cols) as i32 * h as i32;
        sheet.draw_pixmap(
            x,
            y,
            cell.as_ref(),
            &PixmapPaint::default(),
            Transform::identity(),
            None,
        );
    }
    sheet
}
