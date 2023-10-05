use std::sync::atomic::{AtomicU8, Ordering};

static CURRENT_MODE: AtomicU8 = AtomicU8::new(Mode::Invalid as u8);

/// Intel PT tracing modes
#[derive(PartialEq, Eq, Debug)]
pub enum Mode {
    Invalid = 0,
    Simple = 1,
    IntelPt = 2,
    PtWrite = 3,
}

/// Get the current mode
pub fn get_mode() -> Mode {
    match CURRENT_MODE.load(Ordering::Relaxed) {
        0 => Mode::Invalid,
        1 => Mode::Simple,
        2 => Mode::IntelPt,
        3 => Mode::PtWrite,
        _ => unreachable!(),
    }
}

/// Set the current mode
pub fn set_mode(mode: Mode) {
    CURRENT_MODE.store(mode as u8, Ordering::Relaxed)
}
