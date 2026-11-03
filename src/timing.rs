//! Temporary frame timers. Removed once the steady path is under a millisecond.
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

pub struct Slot {
    ns: AtomicU64,
    n: AtomicU64,
}

impl Slot {
    pub const fn new() -> Self {
        Self {
            ns: AtomicU64::new(0),
            n: AtomicU64::new(0),
        }
    }

    pub fn add(&self, t0: Instant) {
        self.ns.fetch_add(t0.elapsed().as_nanos() as u64, Ordering::Relaxed);
        self.n.fetch_add(1, Ordering::Relaxed);
    }

    pub fn reset(&self) {
        self.ns.store(0, Ordering::Relaxed);
        self.n.store(0, Ordering::Relaxed);
    }

    fn line(&self, name: &str) {
        let n = self.n.load(Ordering::Relaxed);
        let ms = if n == 0 {
            0.0
        } else {
            self.ns.load(Ordering::Relaxed) as f64 / n as f64 / 1e6
        };
        eprintln!("  {name} {ms:.3} ms x {n}");
    }
}

pub static CLEAR: Slot = Slot::new();
pub static DRAW: Slot = Slot::new();
pub static PLASTIC: Slot = Slot::new();
pub static BLIT: Slot = Slot::new();
pub static FACE: Slot = Slot::new();
pub static WHIRL: Slot = Slot::new();
pub static FORM: Slot = Slot::new();
pub static FUR: Slot = Slot::new();
pub static REBUILD: Slot = Slot::new();

pub fn reset() {
    for slot in [&CLEAR, &DRAW, &PLASTIC, &BLIT, &FACE, &WHIRL, &FORM, &FUR, &REBUILD] {
        slot.reset();
    }
}

pub fn report() {
    eprintln!("timing per call:");
    CLEAR.line("clear");
    DRAW.line("draw");
    FORM.line("form");
    FUR.line("fur");
    REBUILD.line("rebuild");
    BLIT.line("blit");
    PLASTIC.line("plastic");
    FACE.line("face");
    WHIRL.line("whirl");
}
