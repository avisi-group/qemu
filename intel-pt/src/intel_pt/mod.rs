use crate::intel_pt::notify::Notify;
use parking_lot::RwLock;
use std::{sync::Arc, time::Duration};
use {
    crate::intel_pt::ring_buffer::RingBuffer,
    crate::{intel_pt::thread_handle::ThreadHandle, OUT_DIR, STATE},
    bbqueue::{BBBuffer, Consumer, Producer},
    libipt::{
        packet::{Compression, Packet, PacketDecoder},
        ConfigBuilder, PtErrorCode,
    },
    perf_event_open_sys::{
        bindings::{perf_event_attr, perf_event_mmap_page},
        perf_event_open,
    },
    std::{
        collections::HashMap,
        fs::File,
        hash::BuildHasherDefault,
        io::{BufWriter, Read, Write},
        process,
        sync::{
            mpsc,
            mpsc::{Receiver, Sender},
        },
        thread::JoinHandle,
    },
    twox_hash::XxHash64,
};

mod decoder;
mod notify;
mod ring_buffer;
mod thread_handle;

/// Path to the value of the current Intel PT type
const INTEL_PT_TYPE_PATH: &str = "/sys/bus/event_source/devices/intel_pt/type";

const NR_AUX_PAGES: usize = 1024;
const NR_DATA_PAGES: usize = 256;

/// Number of Intel PT synchronisation points included in each work item
const SYNC_POINTS_PER_JOB: usize = 32;

/// Size of the Intel PT data buffer in bytes
const DATA_BUFFER_SIZE: usize = 4 * 1024 * 1024 * 1024;

pub struct PtTracer {
    pub perf_file_descriptor: i32,
    /// Host to guest address mapping
    mapping: Arc<RwLock<HashMap<u64, u64, BuildHasherDefault<XxHash64>>>>,

    /// Handle for PT reading thread
    read_handle: ThreadHandle,
    /// Handle for PT parsing thread
    parse_handle: ThreadHandle,
    /// Handle for trace writing thread
    write_handle: ThreadHandle,
}

impl PtTracer {
    pub fn insert_mapping(&mut self, host_pc: u64, guest_pc: u64) {
        self.mapping.write().insert(host_pc, guest_pc);
        //  println!("mapping {host_pc:x} {guest_pc:x}");
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

    pub fn init() -> Self {
        let mut pea: perf_event_attr = perf_event_attr::default();

        // perf event type
        pea.type_ = get_intel_pt_perf_type();

        // Event should start disabled, and not operate in kernel-mode.
        pea.set_disabled(1);
        pea.set_exclude_kernel(1);
        pea.set_exclude_hv(1);
        pea.set_precise_ip(2);

        // 2401 to disable return compression
        pea.config = 0x2001; // 0010000000000001

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

        let mapping = Arc::new(RwLock::new(HashMap::default()));
        let mapping_clone = mapping.clone();

        let buffer = Box::leak(Box::new(BBBuffer::new()));
        let (producer, consumer) = buffer.try_split().unwrap();

        let write_handle = ThreadHandle::spawn(move |rx| write_pt_data(rx));

        let parse_handle =
            ThreadHandle::spawn(move |rx| parse_pt_data(rx, mapping_clone, consumer));

        let read_handle =
            ThreadHandle::spawn(move |rx| read_pt_data(rx, perf_file_descriptor, producer));

        Self {
            perf_file_descriptor,
            mapping,

            read_handle,
            parse_handle,
            write_handle,
        }
    }

    /// Waits for the internal ring buffer to be empty
    pub fn wait_for_empty(&self) {
        // self.pt_buffer_empty.wait();
        // self.data_buffer_empty.wait();
    }

    pub fn terminate(self) {
        log::trace!("terminating");

        let Self {
            read_handle,
            parse_handle,
            write_handle,
            ..
        } = self;

        read_handle.terminate();
        parse_handle.terminate();
        write_handle.terminate();
    }
}

fn get_intel_pt_perf_type() -> u32 {
    let mut intel_pt_type = File::open(INTEL_PT_TYPE_PATH).unwrap();

    let mut buf = String::new();
    intel_pt_type.read_to_string(&mut buf).unwrap();

    buf.trim().parse().unwrap()
}

fn read_pt_data(
    shutdown_receiver: Receiver<()>,
    perf_file_descriptor: i32,

    mut producer: Producer<DATA_BUFFER_SIZE>,
) {
    let mmap = memmap2::MmapOptions::new()
        .len((NR_DATA_PAGES + 1) * page_size::get())
        .map_raw(perf_file_descriptor)
        .unwrap();

    let header = unsafe { &mut *(mmap.as_mut_ptr() as *mut perf_event_mmap_page) };

    header.aux_offset = header.data_offset + header.data_size;
    header.aux_size = (NR_AUX_PAGES * page_size::get()) as u64;

    let aux_area = memmap2::MmapOptions::new()
        .len(header.aux_size as usize)
        .offset(header.aux_offset)
        .map_raw(perf_file_descriptor)
        .unwrap();

    let mut ring_buffer = RingBuffer::new(mmap, aux_area);

    loop {
        match ring_buffer.next_record() {
            Some(record) => {
                let mut grant = producer
                    .grant_exact(record.data().len())
                    .expect(&format!("failed to grant {}", record.data().len()));
                record.data().copy_to_slice(grant.buf());
                grant.commit(record.data().len());
            }
            None => {
                if let Ok(()) = shutdown_receiver.try_recv() {
                    log::trace!("read terminating");
                    return;
                }
            }
        }
    }
}

fn parse_pt_data(
    shutdown_receiver: Receiver<()>,
    mapping: Arc<RwLock<HashMap<u64, u64, BuildHasherDefault<XxHash64>>>>,
    mut consumer: Consumer<DATA_BUFFER_SIZE>,
) {
    let mut writer = File::create(OUT_DIR.to_owned() + "intelpt.trace").unwrap();

    loop {
        // read data from consumer, checking for shutdown if empty
        let mut read = match consumer.read() {
            Ok(read) => read,
            Err(_) => {
                continue;
            }
        };

        if let Ok(()) = shutdown_receiver.try_recv() {
            log::trace!("parse terminating");

            parse_slice(&mut writer, mapping.clone(), read.buf_mut());

            return;
        }

        log::trace!("read {}", read.buf().len());

        let mut decoder =
            PacketDecoder::new(&ConfigBuilder::new(read.buf_mut()).unwrap().finish()).unwrap();

        if decoder.sync_forward().is_err() {
            // insufficient data in buffer to find next sync point so allow more data to be written
            continue;
        }

        let (begin, end) = {
            let offset = decoder.sync_offset().unwrap();
            decoder.sync_set(offset + 1).unwrap();
            if decoder.sync_forward().is_err() {
                continue;
            }
            let next_offset = decoder.sync_offset().unwrap();
            log::trace!("offsets {offset} {next_offset}");
            (offset as usize, next_offset as usize)
        };

        parse_slice(
            &mut writer,
            mapping.clone(),
            &mut read.buf_mut()[begin..end],
        );

        let offset = decoder.offset().unwrap() as usize;
        decoder.sync_backward().unwrap();

        log::trace!(
            "finished, offset: {}, sync_offset: {}",
            offset,
            decoder.sync_offset().unwrap()
        );

        read.release(end);
    }
}

fn parse_slice<W: Write>(
    writer: &mut W,
    mapping: Arc<RwLock<HashMap<u64, u64, BuildHasherDefault<XxHash64>>>>,
    slice: &mut [u8],
) {
    let mut decoder = PacketDecoder::new(&ConfigBuilder::new(slice).unwrap().finish()).unwrap();

    let mut last_ip = 0;

    loop {
        match decoder.next() {
            Ok(p) => match p {
                Packet::Tip(inner) => {
                    let ip = match inner.compression() {
                        Compression::Suppressed => continue,
                        Compression::Update16 => (last_ip >> 16) << 16 | inner.tip(),
                        Compression::Update32 => (last_ip >> 32) << 32 | inner.tip(),
                        Compression::Update48 => (last_ip >> 32) << 32 | inner.tip(),
                        Compression::Sext48 => (((inner.tip() as i64) << 16) >> 16) as u64,
                        Compression::Full => inner.tip(),
                    };

                    last_ip = ip;

                    if let Some(guest_pc) = mapping.read().get(&(ip - 9)) {
                        writeln!(writer, "{:X}", guest_pc).unwrap();
                    }
                }
                _ => (),
            },
            Err(e) => {
                log::trace!("packet err: {e:?}");
                if let Err(e) = decoder.sync_forward() {
                    if e.code() == PtErrorCode::Eos {
                        log::trace!("packet err: got eos while syncing after packet error");
                        break;
                    } else {
                        log::trace!("packet err: got error while syncing {e:?}");
                    }
                }
            }
        }
    }
}

fn write_pt_data(shutdown_receiver: Receiver<()>) {
    loop {
        if let Ok(()) = shutdown_receiver.try_recv() {
            log::trace!("write terminating");
            return;
        } else {
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}
