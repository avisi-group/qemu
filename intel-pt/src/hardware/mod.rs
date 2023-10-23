use {
    crate::{
        hardware::{notify::Notify, parser::Parser, reader::Reader, writer::Writer},
        Mode, OUT_DIR,
    },
    bbqueue::BBBuffer,
    libipt::packet::Packet,
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
    pc_map: Option<SharedPcMap>,

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
        if let Some(pc_map) = &self.pc_map {
            pc_map.write().insert(host_pc, guest_pc);
        }
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
        let (producer, consumer) = BUFFER.try_split().unwrap();
        let empty_buffer_notifier = Notify::new();

        match mode {
            Mode::Uninitialized | Mode::Simple => unreachable!(),
            Mode::Tip => {
                let pc_map = Arc::new(RwLock::new(HashMap::default()));

                let (sender, receiver) = ordered_queue::new();

                let writer = Writer::init::<TipDecoder, _>(
                    OUT_DIR.to_owned() + "intelpt.trace",
                    pc_map.clone(),
                    receiver,
                );

                let parser =
                    Parser::init::<TipHandler>(empty_buffer_notifier.clone(), consumer, sender);
                let (reader, perf_file_descriptor) = Reader::init(producer, mode);

                Self {
                    perf_file_descriptor,
                    pc_map: Some(pc_map),
                    empty_buffer_notifier,
                    reader,
                    parser,
                    writer,
                }
            }
            Mode::Fup => todo!(),
            Mode::PtWrite => {
                let (sender, receiver) = ordered_queue::new();

                let writer = Writer::init::<PtwriteHandler, _>(
                    OUT_DIR.to_owned() + "intelpt.trace",
                    (),
                    receiver,
                );

                let parser =
                    Parser::init::<PtwriteHandler>(empty_buffer_notifier.clone(), consumer, sender);
                let (reader, perf_file_descriptor) = Reader::init(producer, mode);

                Self {
                    perf_file_descriptor,
                    pc_map: None,
                    empty_buffer_notifier,
                    reader,
                    parser,
                    writer,
                }
            }
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

/// Processes packets
pub trait PacketHandler {
    type ProcessedPacket: Send + 'static;

    fn new() -> Self;

    fn process_packet(&mut self, packet: Packet<()>);

    fn finish(self) -> Vec<Self::ProcessedPacket>;
}

/// Transforms processed packets into Program Counter values
pub trait ProcessedPacketHandler {
    type ProcessedPacket: Send + 'static;
    type Ctx: Send + 'static;

    fn new(ctx: Self::Ctx) -> Self;

    fn calculate_pc(&mut self, data: Self::ProcessedPacket) -> Option<u64>;
}

struct PtwriteHandler {
    buf: Vec<u64>,
}

impl PacketHandler for PtwriteHandler {
    type ProcessedPacket = u64;

    fn new() -> Self {
        Self { buf: Vec::new() }
    }

    fn process_packet(&mut self, packet: Packet<()>) {
        if let Packet::Ptw(inner) = packet {
            self.buf.push(inner.payload());
        }
    }

    fn finish(self) -> Vec<Self::ProcessedPacket> {
        self.buf
    }
}

impl ProcessedPacketHandler for PtwriteHandler {
    type ProcessedPacket = u64;
    type Ctx = ();

    fn new(_: Self::Ctx) -> Self {
        Self { buf: Vec::new() }
    }

    fn calculate_pc(&mut self, data: Self::ProcessedPacket) -> Option<u64> {
        Some(data)
    }
}

enum Compression {
    /// Payload: 16 bits. Update last IP
    Update16,
    /// Payload: 32 bits. Update last IP
    Update32,
    /// Payload: 48 bits. Sign extend to full address
    Sext48,
    /// Payload: 48 bits. Update last IP
    Update48,
    /// Payload: 64 bits. Full address
    Full,
}

struct TipHandler {
    buf: Vec<(Compression, u64)>,
}

impl PacketHandler for TipHandler {
    type ProcessedPacket = (Compression, u64);

    fn new() -> Self {
        Self { buf: Vec::new() }
    }

    fn process_packet(&mut self, packet: Packet<()>) {
        if let Packet::Tip(inner) = packet {
            let compression = match inner.compression() {
                libipt::packet::Compression::Suppressed => return,
                libipt::packet::Compression::Update16 => Compression::Update16,
                libipt::packet::Compression::Update32 => Compression::Update32,
                libipt::packet::Compression::Sext48 => Compression::Sext48,
                libipt::packet::Compression::Update48 => Compression::Update48,
                libipt::packet::Compression::Full => Compression::Full,
            };
            self.buf.push((compression, inner.tip()));
        }
    }

    fn finish(self) -> Vec<Self::ProcessedPacket> {
        self.buf
    }
}

struct TipDecoder {
    last_ip: u64,
    pc_map: SharedPcMap,
}

impl ProcessedPacketHandler for TipDecoder {
    type ProcessedPacket = (Compression, u64);

    type Ctx = SharedPcMap;

    fn new(ctx: Self::Ctx) -> Self {
        Self {
            last_ip: 0,
            pc_map: ctx,
        }
    }

    fn calculate_pc(&mut self, data: Self::ProcessedPacket) -> Option<u64> {
        let ip = match data.0 {
            Compression::Update16 => (self.last_ip >> 16) << 16 | data.1,
            Compression::Update32 => (self.last_ip >> 32) << 32 | data.1,
            Compression::Update48 => (self.last_ip >> 48) << 48 | data.1,
            Compression::Sext48 => (((data.1 as i64) << 16) >> 16) as u64,
            Compression::Full => data.1,
        };

        self.last_ip = ip;

        self.pc_map.read().get(&(ip - 9)).copied()
    }
}
