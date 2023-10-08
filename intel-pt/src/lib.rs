use {
    enum_kinds::EnumKind,
    intel_pt::PtTracer,
    parking_lot::{lock_api::RawMutex, Mutex},
    std::{
        fs::File,
        io::{BufWriter, Write},
        mem,
        path::Path,
        time::Duration,
    },
};

mod ffi;
mod intel_pt;

const OUT_DIR: &str = "/home/fm208/data/";

static STATE: State = State::new();

struct State {
    inner: Mutex<InnerState>,
}

impl State {
    const fn new() -> Self {
        State {
            inner: Mutex::const_new(RawMutex::INIT, InnerState::Uninitialized),
        }
    }

    fn init_simple(&self) {
        let f = File::create(OUT_DIR.to_owned() + "simple.trace").unwrap();
        let writer = BufWriter::with_capacity(65536, f);
        *self.inner.lock() = InnerState::Simple(writer)
    }

    fn init_intelpt(&self) {
        *self.inner.lock() = InnerState::IntelPt(PtTracer::init());
    }

    fn mode(&self) -> Mode {
        (&*self.inner.lock()).into()
    }

    /// Returns whether direct chaining should be enabled in QEMU
    fn enable_direct_chaining(&self) -> bool {
        // disable direct chaining for simple mode
        if let Mode::Simple = STATE.mode() {
            false
        } else {
            true
        }
    }

    /// Returns whether jmx should be inserted at the start of blocks (generates
    /// a TIP packet)
    fn insert_jmx_at_block_start(&self) -> bool {
        if let Mode::IntelPt = STATE.mode() {
            true
        } else {
            false
        }
    }

    fn insert_chain_count_check(&self) -> bool {
        if let Mode::IntelPt = STATE.mode() {
            true
        } else {
            false
        }
    }

    fn trace_guest_pc(&self, pc: u64) {
        let InnerState::Simple(f) = &mut *self.inner.lock() else {
            return;
        };

        write!(f, "{pc:X}\n").unwrap();
    }

    fn insert(&self, host_pc: u64, guest_pc: u64) {
        let InnerState::IntelPt(tracer) = &mut *self.inner.lock() else {
            return;
        };
        tracer.insert_mapping(host_pc, guest_pc);
    }

    fn start_recording(&self) {
        let InnerState::IntelPt(tracer) = &mut *self.inner.lock() else {
            return;
        };

        tracer.start_recording();
    }

    fn stop_recording(&self) {
        let InnerState::IntelPt(tracer) = &mut *self.inner.lock() else {
            return;
        };

        tracer.stop_recording();
    }

    fn exit(&self) {
        match mem::take(&mut *self.inner.lock()) {
            InnerState::Uninitialized => (),
            InnerState::Simple(mut w) => w.flush().unwrap(),
            InnerState::IntelPt(tracer) => tracer.terminate(),
        }
    }
}

#[derive(EnumKind)]
#[enum_kind(Mode)]
enum InnerState {
    Uninitialized,
    Simple(BufWriter<File>),
    IntelPt(PtTracer),
    //PtWrite
    //Kernel
}

impl Default for InnerState {
    fn default() -> Self {
        Self::Uninitialized
    }
}
