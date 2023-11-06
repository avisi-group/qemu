use {
    crate::{
        hardware::{notify::Notify, reader::Reader, writer::Writer},
        Mode,
    },
    libipt::packet::Packet,
    parking_lot::RwLock,
    std::{
        collections::HashMap,
        hash::BuildHasherDefault,
        path::PathBuf,
        sync::{
            atomic::{AtomicI32, AtomicU32, Ordering},
            Arc,
        },
    },
    twox_hash::XxHash64,
};

pub mod decoder;
pub mod notify;
pub mod ordered_queue;
pub mod reader;
pub mod ring_buffer;
pub mod thread_handle;
pub mod writer;

type SharedPcMap = Arc<RwLock<HashMap<u64, u64, BuildHasherDefault<XxHash64>>>>;

/// Number of Intel PT synchronisation points included in each work item
const _SYNC_POINTS_PER_JOB: usize = 32;

pub struct HardwareTracer {
    pub perf_file_descriptor: Arc<AtomicI32>,
    /// Host to guest address mapping
    pc_map: Option<SharedPcMap>,

    empty_buffer_notifier: Notify,

    /// PT reader
    reader: Reader,
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

    pub fn init(mode: Mode, path: PathBuf) -> Self {
        let empty_buffer_notifier = Notify::new();
        let task_count = Arc::new(AtomicU32::new(0));

        match mode {
            Mode::Uninitialized | Mode::Simple => unreachable!(),
            Mode::Tip => {
                // let pc_map = Arc::new(RwLock::new(HashMap::default()));

                // let (sender, receiver) = ordered_queue::new();

                // let writer = Writer::init::<TipWriter, _>(
                //     path.join("tip.trace"),
                //     pc_map.clone(),
                //     receiver,
                //     empty_buffer_notifier.clone(),
                //     task_count.clone(),
                // );

                // let (reader, perf_file_descriptor) =
                //     Reader::init::<TipParser>(mode, sender, task_count);

                // Self {
                //     perf_file_descriptor,
                //     pc_map: Some(pc_map),
                //     empty_buffer_notifier,
                //     reader,
                //     writer,
                // }
                todo!()
            }
            Mode::PtWrite => {
                let (sender, receiver) = ordered_queue::new();

                let writer = Writer::init::<PtwWriter, _>(
                    path.join("ptw.trace"),
                    (),
                    receiver,
                    empty_buffer_notifier.clone(),
                    task_count.clone(),
                );

                let (reader, perf_file_descriptor) =
                    Reader::init::<PtwParser>(mode, sender, task_count);

                Self {
                    perf_file_descriptor,
                    pc_map: None,
                    empty_buffer_notifier,
                    reader,
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

        //{
        // use std::io::Write;
        // let mut w = std::fs::File::create("/mnt/tmp/pcmap.txt").unwrap();
        // self.pc_map
        //     .unwrap()
        //     .read()
        //     .iter()
        //     .for_each(|(k, v)| writeln!(&mut w, "{k:x}: {v:x}").unwrap());
        //}

        let Self { reader, writer, .. } = self;

        reader.exit();
        writer.exit();
    }
}

/// Processes packets
pub trait PacketParser {
    type ProcessedPacket: Send + 'static + std::fmt::Debug;

    fn new() -> Self;

    fn process(&mut self, packet: Packet<()>);

    fn finish(self) -> Vec<Self::ProcessedPacket>;
}

/// Transforms processed packets into Program Counter values
pub trait PacketWriter {
    type ProcessedPacket: Send + 'static;
    type Ctx: Send + 'static;

    fn new(ctx: Self::Ctx) -> Self;

    fn calculate_pc(&mut self, data: Self::ProcessedPacket) -> Option<u64>;
}

pub struct PtwParser {
    buf: Vec<u64>,
}

impl PacketParser for PtwParser {
    type ProcessedPacket = u64;

    fn new() -> Self {
        Self { buf: Vec::new() }
    }

    fn process(&mut self, packet: Packet<()>) {
        if let Packet::Ptw(inner) = packet {
            self.buf.push(inner.payload());
        }
    }

    fn finish(self) -> Vec<Self::ProcessedPacket> {
        self.buf
    }
}

pub struct PtwWriter;

impl PacketWriter for PtwWriter {
    type ProcessedPacket = u64;
    type Ctx = ();

    fn new(_: Self::Ctx) -> Self {
        Self
    }

    fn calculate_pc(&mut self, data: Self::ProcessedPacket) -> Option<u64> {
        Some(data)
    }
}

#[derive(Debug)]
pub enum Kind {
    /// Payload: 16 bits. Update last IP
    Update16,
    /// Payload: 32 bits. Update last IP
    Update32,
    /// Payload: 48 bits. Update last IP
    Update48,
    /// Payload: 64 bits. Full address
    Update64,
    /// Payload: 48 bits. Sign extend to full address
    SignExtend48,
    /// Full address, but do not emit a PC
    Update64NoEmit,
}

pub struct TipParser {
    buf: Vec<(Kind, u64)>,
}

impl PacketParser for TipParser {
    type ProcessedPacket = (Kind, u64);

    fn new() -> Self {
        Self { buf: Vec::new() }
    }

    fn process(&mut self, packet: Packet<()>) {
        match packet {
            Packet::Tip(inner) => {
                let compression = match inner.compression() {
                    libipt::packet::Compression::Suppressed => return,
                    libipt::packet::Compression::Update16 => Kind::Update16,
                    libipt::packet::Compression::Update32 => Kind::Update32,
                    libipt::packet::Compression::Sext48 => Kind::SignExtend48,
                    libipt::packet::Compression::Update48 => Kind::Update48,
                    libipt::packet::Compression::Full => Kind::Update64,
                };
                self.buf.push((compression, inner.tip()));
            }
            Packet::Fup(inner) => {
                self.buf.push((Kind::Update64NoEmit, inner.fup()));
            }
            _ => (),
        }
    }

    fn finish(self) -> Vec<Self::ProcessedPacket> {
        self.buf
    }
}

pub struct TipWriter {
    last_ip: u64,
    pc_map: SharedPcMap,
}

impl PacketWriter for TipWriter {
    type ProcessedPacket = (Kind, u64);

    type Ctx = SharedPcMap;

    fn new(ctx: Self::Ctx) -> Self {
        Self {
            last_ip: 0,
            pc_map: ctx,
        }
    }

    fn calculate_pc(&mut self, data: Self::ProcessedPacket) -> Option<u64> {
        let ip = match data.0 {
            Kind::Update16 => (self.last_ip >> 16) << 16 | data.1,
            Kind::Update32 => (self.last_ip >> 32) << 32 | data.1,
            Kind::Update48 => (self.last_ip >> 48) << 48 | data.1,
            Kind::SignExtend48 => (((data.1 as i64) << 16) >> 16) as u64,
            Kind::Update64 | Kind::Update64NoEmit => data.1,
        };

        self.last_ip = ip;

        if matches!(data.0, Kind::Update64NoEmit) {
            None
        } else {
            self.pc_map.read().get(&(ip - 9)).copied()
        }
    }
}
