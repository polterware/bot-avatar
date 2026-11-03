pub const DARK_INK: &str = "#1E1A33";
pub const LIGHT_INK: &str = "#F7F5F2";

#[inline]
fn clamp01(v: f64) -> f64 {
    v.clamp(0.0, 1.0)
}

fn split_color_args(inner: &str) -> Vec<&str> {
    inner
        .split(|c: char| c.is_ascii_whitespace() || c == ',' || c == '/')
        .filter(|s| !s.is_empty())
        .collect()
}

/// Parse `#rgb` / `#rrggbb` / `rgb()` / `rgba()` / `hsl()` / `hsla()` into
/// 0..=255 RGB channels and 0..=1 alpha.
pub fn parse_color_rgba(input: &str) -> Option<[f64; 4]> {
    let s = input.trim();
    if let Some(hex) = s.strip_prefix('#') {
        return match hex.len() {
            3 => {
                let r = u8::from_str_radix(&hex[0..1], 16).ok()?;
                let g = u8::from_str_radix(&hex[1..2], 16).ok()?;
                let b = u8::from_str_radix(&hex[2..3], 16).ok()?;
                Some([(r * 17) as f64, (g * 17) as f64, (b * 17) as f64, 1.0])
            }
            4 => {
                let r = u8::from_str_radix(&hex[0..1], 16).ok()?;
                let g = u8::from_str_radix(&hex[1..2], 16).ok()?;
                let b = u8::from_str_radix(&hex[2..3], 16).ok()?;
                let a = u8::from_str_radix(&hex[3..4], 16).ok()?;
                Some([
                    (r * 17) as f64,
                    (g * 17) as f64,
                    (b * 17) as f64,
                    (a * 17) as f64 / 255.0,
                ])
            }
            6 => {
                let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
                let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
                let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
                Some([r as f64, g as f64, b as f64, 1.0])
            }
            8 => {
                let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
                let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
                let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
                let a = u8::from_str_radix(&hex[6..8], 16).ok()?;
                Some([r as f64, g as f64, b as f64, a as f64 / 255.0])
            }
            _ => None,
        };
    }

    let lower = s.to_ascii_lowercase();
    if (lower.starts_with("hsl(") || lower.starts_with("hsla(")) && lower.ends_with(')') {
        let open = lower.find('(')?;
        let inner = &lower[open + 1..lower.len() - 1];
        let args = split_color_args(inner);
        if args.len() < 3 {
            return None;
        }
        let h_str = args[0].strip_suffix("deg").unwrap_or(args[0]);
        let s_str = args[1].strip_suffix('%')?;
        let l_str = args[2].strip_suffix('%')?;
        let h: f64 = h_str.parse().ok()?;
        let sat: f64 = s_str.parse().ok()?;
        let lig: f64 = l_str.parse().ok()?;
        let a = if args.len() >= 4 {
            parse_alpha_token(args[3])?
        } else {
            1.0
        };
        let [r, g, b] = hsl_to_rgb([h / 360.0, sat / 100.0, lig / 100.0]);
        return Some([r, g, b, a]);
    }

    if (lower.starts_with("rgb(") || lower.starts_with("rgba(")) && lower.ends_with(')') {
        let open = lower.find('(')?;
        let inner = &lower[open + 1..lower.len() - 1];
        let args = split_color_args(inner);
        if args.len() < 3 {
            return None;
        }
        let r: f64 = args[0].parse().ok()?;
        let g: f64 = args[1].parse().ok()?;
        let b: f64 = args[2].parse().ok()?;
        let a = if args.len() >= 4 {
            parse_alpha_token(args[3])?
        } else {
            1.0
        };
        return Some([r, g, b, a]);
    }

    None
}

fn parse_alpha_token(tok: &str) -> Option<f64> {
    if let Some(pct) = tok.strip_suffix('%') {
        Some(clamp01(pct.parse::<f64>().ok()? / 100.0))
    } else {
        Some(clamp01(tok.parse::<f64>().ok()?))
    }
}

/// Parse `#rgb` / `#rrggbb` / `rgb()` / `hsl()` into 0..=255 channels; `None` for anything else.
pub fn parse_color(input: &str) -> Option<[f64; 3]> {
    let [r, g, b, _] = parse_color_rgba(input)?;
    Some([r, g, b])
}

/// Relative luminance (WCAG), 0..=1. Unparseable colors count as mid-grey.
pub fn luminance(color: &str) -> f64 {
    let Some(c) = parse_color(color) else {
        return 0.5;
    };
    let lin = |v: f64| {
        let x = v / 255.0;
        if x <= 0.03928 {
            x / 12.92
        } else {
            ((x + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * lin(c[0]) + 0.7152 * lin(c[1]) + 0.0722 * lin(c[2])
}

/// Face ink for a body color: dark ink, or light ink on a dark body.
pub fn auto_ink(color: &str) -> &'static str {
    if luminance(color) < 0.13 {
        LIGHT_INK
    } else {
        DARK_INK
    }
}

pub fn rgb_to_hsl([mut r, mut g, mut b]: [f64; 3]) -> [f64; 3] {
    r /= 255.0;
    g /= 255.0;
    b /= 255.0;
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;
    if (max - min).abs() < 1e-12 {
        return [0.0, 0.0, l];
    }
    let d = max - min;
    let s = if l > 0.5 {
        d / (2.0 - max - min)
    } else {
        d / (max + min)
    };
    let h = if (max - r).abs() < 1e-12 {
        (g - b) / d + if g < b { 6.0 } else { 0.0 }
    } else if (max - g).abs() < 1e-12 {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    };
    [h / 6.0, s, l]
}

pub fn hsl_to_rgb([h, s, l]: [f64; 3]) -> [f64; 3] {
    if s == 0.0 {
        return [l * 255.0, l * 255.0, l * 255.0];
    }
    let q = if l < 0.5 {
        l * (1.0 + s)
    } else {
        l + s - l * s
    };
    let p = 2.0 * l - q;
    let f = |mut t: f64| {
        t = ((t % 1.0) + 1.0) % 1.0;
        if t < 1.0 / 6.0 {
            p + (q - p) * 6.0 * t
        } else if t < 0.5 {
            q
        } else if t < 2.0 / 3.0 {
            p + (q - p) * (2.0 / 3.0 - t) * 6.0
        } else {
            p
        }
    };
    [
        f(h + 1.0 / 3.0) * 255.0,
        f(h) * 255.0,
        f(h - 1.0 / 3.0) * 255.0,
    ]
}

pub fn hsl_to_css([h, s, l]: [f64; 3]) -> String {
    format!("hsl({:.1} {:.1}% {:.1}%)", h * 360.0, s * 100.0, l * 100.0)
}

/// A shade of a color: `dl` moves lightness (-1..1), `ds` moves saturation.
/// Darker shades get a touch more saturation so they stay rich instead of going grey.
pub fn shade(color: &str, dl: f64, ds: f64) -> String {
    let Some(c) = parse_color(color) else {
        return color.to_string();
    };
    let [h, s, l] = rgb_to_hsl(c);
    let extra = if dl < 0.0 { -dl * 0.25 } else { 0.0 };
    hsl_to_css([h, clamp01(s + ds + extra), clamp01(l + dl)])
}

#[inline]
fn to_linear(u: f64) -> f64 {
    if u <= 0.04045 {
        u / 12.92
    } else {
        ((u + 0.055) / 1.055).powf(2.4)
    }
}

#[inline]
fn to_srgb(u: f64) -> f64 {
    if u <= 0.0031308 {
        12.92 * u
    } else {
        1.055 * u.powf(1.0 / 2.4) - 0.055
    }
}

fn oklab(r: f64, g: f64, b: f64) -> [f64; 3] {
    let l = (0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b).cbrt();
    let m = (0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b).cbrt();
    let s = (0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b).cbrt();
    [
        0.2104542553 * l + 0.793617785 * m - 0.0040720468 * s,
        1.9779984951 * l - 2.428592205 * m + 0.4505937099 * s,
        0.0259040371 * l + 0.7827717662 * m - 0.808675766 * s,
    ]
}

fn from_oklab(l_cap: f64, a: f64, b: f64) -> [f64; 3] {
    let l = (l_cap + 0.3963377774 * a + 0.2158037573 * b).powi(3);
    let m = (l_cap - 0.1055613458 * a - 0.0638541728 * b).powi(3);
    let s = (l_cap - 0.0894841775 * a - 1.291485548 * b).powi(3);
    [
        4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s,
        -1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s,
        -0.0041960863 * l - 0.7034186147 * m + 1.707614701 * s,
    ]
}

#[inline]
fn in_gamut(c: [f64; 3]) -> bool {
    c.into_iter().all(|v| (-1e-4..=1.0 + 1e-4).contains(&v))
}

/// A more vivid version of a color (`t` in 0..=1): chroma raised along its own
/// hue in OKLab by up to half again, deepening by at most 15% of lightness if needed.
pub fn richer(color: &str, t: f64) -> String {
    let Some(c) = parse_color(color) else {
        return color.to_string();
    };
    if !(t > 0.0) {
        return color.to_string();
    }
    let [l_cap, a, b] = oklab(
        to_linear(c[0] / 255.0),
        to_linear(c[1] / 255.0),
        to_linear(c[2] / 255.0),
    );
    let chroma = a.hypot(b);
    if chroma < 1e-4 {
        return color.to_string();
    }
    let ua = a / chroma;
    let ub = b / chroma;
    let want = chroma * (1.0 + 0.5 * t);
    let mut best: Option<[f64; 3]> = None;
    let mut best_c = chroma;
    for k in 0..=6 {
        let lk = l_cap * (1.0 - (0.15 * t * (k as f64)) / 6.0);
        let mut lo = 0.0;
        let mut hi = want;
        for _ in 0..18 {
            let mid = (lo + hi) / 2.0;
            if in_gamut(from_oklab(lk, ua * mid, ub * mid)) {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        if lo > best_c + 0.004 * (k as f64) {
            best_c = lo;
            best = Some(from_oklab(lk, ua * lo, ub * lo));
        }
        if lo >= want - 1e-4 {
            break;
        }
    }
    let Some(best_rgb) = best else {
        return color.to_string();
    };
    let out = best_rgb.map(|v| (255.0 * to_srgb(v.clamp(0.0, 1.0))).round() as i32);
    format!("rgb({} {} {})", out[0], out[1], out[2])
}
