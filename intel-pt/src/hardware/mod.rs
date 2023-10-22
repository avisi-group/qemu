use {
    crate::{
        hardware::{notify::Notify, parser::Parser, reader::Reader, writer::Writer},
        Mode, OUT_DIR,
    },
    bbqueue::BBBuffer,
    parking_lot::RwLock,
    std::{
        collections::HashMap,
        hash::BuildHasherDefault,
        sync::{
            atomic::{AtomicI32, Ordering},
            Arc,
        },
    },
    twox_hash::XxHash64,
};

pub mod decoder;
pub mod notify;
pub mod ordered_queue;
pub mod parser;
pub mod reader;
pub mod ring_buffer;
pub mod thread_handle;
pub mod writer;

type SharedPcMap = Arc<RwLock<HashMap<u64, u64, BuildHasherDefault<XxHash64>>>>;

/// Number of Intel PT synchronisation points included in each work item
const _SYNC_POINTS_PER_JOB: usize = 32;

/// Size of the Intel PT data buffer in bytes
pub const BUFFER_SIZE: usize = 1024 * 1024 * 1024;

static BUFFER: BBBuffer<BUFFER_SIZE> = BBBuffer::new();

pub struct HardwareTracer {
    pub perf_file_descriptor: Arc<AtomicI32>,
    /// Host to guest address mapping
    pc_map: SharedPcMap,

    empty_buffer_notifier: Notify,
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

        if unsafe {
            perf_event_open_sys::ioctls::ENABLE(
                self.perf_file_descriptor.load(Ordering::Relaxed),
                0,
            )
        } < 0
        {
            panic!("failed to start recording");
        }
    }

    pub fn stop_recording(&self) {
        if unsafe {
            perf_event_open_sys::ioctls::DISABLE(
                self.perf_file_descriptor.load(Ordering::Relaxed),
                0,
            )
        } < 0
        {
            panic!("failed to stop recording");
        }
    }

    pub fn init(mode: Mode) -> Self {
        let pc_map = Arc::new(RwLock::new(HashMap::default()));
        let (producer, consumer) = BUFFER.try_split().unwrap();
        let empty_buffer_notifier = Notify::new();
        let (sender, receiver) = ordered_queue::new();

        let writer = Writer::init(
            OUT_DIR.to_owned() + "intelpt.trace",
            pc_map.clone(),
            receiver,
            mode,
        );
        let parser = Parser::init(empty_buffer_notifier.clone(), consumer, sender, mode);
        let (reader, perf_file_descriptor) = Reader::init(producer, mode);

        Self {
            perf_file_descriptor,
            pc_map,
            empty_buffer_notifier,
            reader,
            parser,
            writer,
        }
    }

    /// Waits for the internal ring buffer to be empty
    pub fn wait_for_empty(&self) {
        log::trace!("waiting");
        self.empty_buffer_notifier.wait();
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
