use {crate::state::State, std::str::FromStr};

pub mod ffi;
pub mod hardware;
pub mod state;

static STATE: State = State::new();

/// Tracing mode
#[derive(PartialEq, Eq, Clone, Copy)]
#[repr(u8)]
pub enum Mode {
    Uninitialized = 0,
    Simple = 1,
    Tip = 2,
    PtWrite = 4,
}

impl From<u8> for Mode {
    fn from(value: u8) -> Self {
        match value {
            1 => Mode::Simple,
            2 => Mode::Tip,
            4 => Mode::PtWrite,
            _ => Mode::Uninitialized,
        }
    }
}

impl From<Mode> for u8 {
    fn from(value: Mode) -> Self {
        value as u8
    }
}

impl FromStr for Mode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "simple" => Ok(Mode::Simple),
            "tip" => Ok(Mode::Tip),
            "ptw" => Ok(Mode::PtWrite),
            _ => Err(s.to_owned()),
        }
    }
}
