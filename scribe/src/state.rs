use {
    crate::{hardware::HardwareTracer, Mode},
    parking_lot::{lock_api::RawMutex, Mutex},
    std::{
        fs::File,
        io::{BufWriter, Write},
        mem,
        path::PathBuf,
        sync::atomic::{AtomicU8, Ordering},
    },
};

pub struct State {
    mode: AtomicU8,
    inner: Mutex<InnerState>,
}

impl State {
    pub const fn new() -> Self {
        State {
            mode: AtomicU8::new(Mode::Uninitialized as u8),
            inner: Mutex::const_new(RawMutex::INIT, InnerState::Uninitialized),
        }
    }

    pub fn handle_arg(&self, arg: &str) {
        pretty_env_logger::formatted_timed_builder()
            .filter_level(log::LevelFilter::Trace)
            .try_init()
            .unwrap();

        // parse arguments
        let (mode, path) = {
            let split = arg.split(',').collect::<Vec<_>>();

            assert_eq!(split.len(), 2);

            let mode = split[0]
                .parse::<Mode>()
                .expect("unrecognized command line argument");

            let path = PathBuf::from(split[1]);

            (mode, path)
        };

        *self.inner.lock() = match mode {
            Mode::Simple => InnerState::Simple(BufWriter::new(
                File::create(path.join("simple.trace")).unwrap(),
            )),
            Mode::Tip | Mode::Fup | Mode::PtWrite => {
                InnerState::Hardware(HardwareTracer::init(mode, path))
            }
            Mode::Uninitialized => unreachable!(),
        };
        self.mode.store(mode.into(), Ordering::Relaxed);
    }

    fn mode(&self) -> Mode {
        self.mode.load(Ordering::Relaxed).into()
    }

    pub fn enable_simple_tracing(&self) -> bool {
        Mode::Simple == self.mode()
    }

    /// Returns whether jmx should be inserted at the start of blocks (generates
    /// a TIP packet)
    pub fn insert_jmx_at_block_start(&self) -> bool {
        self.mode() == Mode::Tip || self.mode() == Mode::Fup
    }

    pub fn insert_pt_write(&self) -> bool {
        self.mode() == Mode::PtWrite
    }

    pub fn insert_chain_count_check(&self) -> bool {
        self.mode() == Mode::Tip || self.mode() == Mode::Fup || self.mode() == Mode::PtWrite
    }

    pub fn trace_guest_pc(&self, pc: u64) {
        if self.mode() != Mode::Simple {
            return;
        }

        let InnerState::Simple(f) = &mut *self.inner.lock() else {
            unreachable!();
        };

        f.write_all(&pc.to_le_bytes()).unwrap();
    }

    pub fn pc_mapping(&self, host_pc: u64, guest_pc: u64) {
        let InnerState::Hardware(tracer) = &mut *self.inner.lock() else {
            return;
        };

        tracer.insert_mapping(host_pc, guest_pc);
    }

    pub fn start_recording(&self) {
        let InnerState::Hardware(tracer) = &mut *self.inner.lock() else {
            return;
        };

        tracer.start_recording();
    }

    pub fn stop_recording(&self) {
        let InnerState::Hardware(tracer) = &mut *self.inner.lock() else {
            return;
        };

        tracer.stop_recording();
    }

    pub fn exit(&self) {
        match mem::take(&mut *self.inner.lock()) {
            InnerState::Uninitialized => (),
            InnerState::Simple(mut w) => w.flush().unwrap(),
            InnerState::Hardware(tracer) => tracer.exit(),
        }
    }
}

enum InnerState {
    Uninitialized,
    Simple(BufWriter<File>),
    Hardware(HardwareTracer),
}

impl Default for InnerState {
    fn default() -> Self {
        Self::Uninitialized
    }
}
