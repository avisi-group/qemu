use {
    crate::{
        intel_pt::{parser::Parser, reader::Reader, writer::Writer},
        Mode, OUT_DIR,
    },
    bbqueue::BBBuffer,
    parking_lot::RwLock,
    perf_event_open_sys::{bindings::perf_event_attr, perf_event_open},
    std::{collections::HashMap, fs::File, hash::BuildHasherDefault, io::Read, process, sync::Arc},
    twox_hash::XxHash64,
};

mod decoder;
mod notify;
mod parser;
mod reader;
mod ring_buffer;
mod thread_handle;
mod writer;

type SharedPcMap = Arc<RwLock<HashMap<u64, u64, BuildHasherDefault<XxHash64>>>>;

/// Path to the value of the current Intel PT type
const INTEL_PT_TYPE_PATH: &str = "/sys/bus/event_source/devices/intel_pt/type";

/// Number of Intel PT synchronisation points included in each work item
const _SYNC_POINTS_PER_JOB: usize = 32;

/// Size of the Intel PT data buffer in bytes
const BUFFER_SIZE: usize = 512 * 1024 * 1024;

static BUFFER: BBBuffer<BUFFER_SIZE> = BBBuffer::new();

pub struct HardwareTracer {
    pub perf_file_descriptor: i32,
    /// Host to guest address mapping
    pc_map: SharedPcMap,
    /// PT reader
    reader: Reader,
    /// Handle for PT parsing thread
    parser: Parser,
    /// Handle for trace writing thread
    writer: Writer,
}

impl HardwareTracer {
    pub fn insert_mapping(&mut self, host_pc: u64, guest_pc: u64) {
        self.pc_map.write().insert(host_pc, guest_pc);
    }

    pub fn start_recording(&self) {
        self.wait_for_empty();

        if unsafe { perf_event_open_sys::ioctls::ENABLE(self.perf_file_descriptor, 0) } < 0 {
            panic!("failed to start recording");
        }
    }

    pub fn stop_recording(&self) {
        if unsafe { perf_event_open_sys::ioctls::DISABLE(self.perf_file_descriptor, 0) } < 0 {
            panic!("failed to stop recording");
        }
    }

    pub fn init(mode: Mode) -> Self {
        let mut pea: perf_event_attr = perf_event_attr::default();

        // perf event type
        pea.type_ = get_intel_pt_perf_type();

        // Event should start disabled, and not operate in kernel-mode.
        pea.set_disabled(1);
        pea.set_exclude_kernel(1);
        pea.set_exclude_hv(1);
        pea.set_precise_ip(2);

        // 2401 to disable return compression

        pea.config = if mode == Mode::PtWrite {
            0b0011_0000_0000_0001
        } else {
            0x2001 // 0010000000000001
        };

        pea.size = std::mem::size_of::<perf_event_attr>() as u32;

        let perf_file_descriptor = unsafe {
            perf_event_open(
                (&mut pea) as *mut _,
                i32::try_from(process::id()).unwrap(),
                -1,
                -1,
                0,
            )
        };
        if perf_file_descriptor < 0 {
            println!("last OS error: {:?}", std::io::Error::last_os_error());
            panic!("perf_event_open failed {perf_file_descriptor}");
        }

        let pc_map = Arc::new(RwLock::new(HashMap::default()));
        let (producer, consumer) = BUFFER.try_split().unwrap();

        let (writer, queue) =
            Writer::init(OUT_DIR.to_owned() + "intelpt.trace", pc_map.clone(), mode);
        let parser = Parser::init(consumer, queue, mode);
        let reader = Reader::init(perf_file_descriptor, producer);

        Self {
            perf_file_descriptor,
            pc_map,
            reader,
            parser,
            writer,
        }
    }

    /// Waits for the internal ring buffer to be empty
    pub fn wait_for_empty(&self) {
        // log::trace!("waiting");
        // self.empty_buffer_notifier.wait();
    }

    pub fn exit(self) {
        log::trace!("terminating");

        let Self {
            reader,
            parser,
            writer,
            ..
        } = self;

        reader.exit();
        parser.exit();
        writer.exit();
    }
}

fn get_intel_pt_perf_type() -> u32 {
    let mut intel_pt_type = File::open(INTEL_PT_TYPE_PATH).unwrap();

    let mut buf = String::new();
    intel_pt_type.read_to_string(&mut buf).unwrap();

    buf.trim().parse().unwrap()
}

pub struct ParsedData {
    pub sequence_number: u32,
    pub data: Vec<u64>,
}

impl PartialEq for ParsedData {
    fn eq(&self, other: &Self) -> bool {
        self.sequence_number == other.sequence_number
    }
}

impl Eq for ParsedData {}

impl PartialOrd for ParsedData {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        other.sequence_number.partial_cmp(&self.sequence_number)
    }
}

impl Ord for ParsedData {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        other.sequence_number.cmp(&self.sequence_number)
    }
}
