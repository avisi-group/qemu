use crate::Mode;

use {
    crate::{
        intel_pt::{notify::Notify, ring_buffer::RingBuffer, thread_handle::ThreadHandle},
        OUT_DIR,
    },
    bbqueue::{BBBuffer, Consumer, Producer},
    libipt::{
        packet::{Compression, Packet, PacketDecoder},
        ConfigBuilder, PtErrorCode,
    },
    parking_lot::RwLock,
    perf_event_open_sys::{
        bindings::{perf_event_attr, perf_event_mmap_page},
        perf_event_open,
    },
    std::{
        collections::HashMap,
        fs::File,
        hash::BuildHasherDefault,
        io::{Read, Write},
        process,
        sync::{mpsc::Receiver, Arc},
        time::Duration,
    },
    twox_hash::XxHash64,
};

mod decoder;
mod notify;
mod ring_buffer;
mod thread_handle;

type SharedPcMap = Arc<RwLock<HashMap<u64, u64, BuildHasherDefault<XxHash64>>>>;

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
    pc_map: SharedPcMap,
    empty_buffer_notifier: Notify,
    /// Handle for PT reading thread
    read_handle: ThreadHandle,
    /// Handle for PT parsing thread
    parse_handle: ThreadHandle,
    /// Handle for trace writing thread
    write_handle: ThreadHandle,
}

impl PtTracer {
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
        let pc_map_clone = pc_map.clone();

        let buffer = Box::leak(Box::new(BBBuffer::new()));
        let (producer, consumer) = buffer.try_split().unwrap();

        let empty_buffer_notifier = Notify::new();
        let empty_buffer_notifier_clone = empty_buffer_notifier.clone();

        let write_handle = ThreadHandle::spawn(move |rx| write_pt_data(rx));

        let parse_handle = ThreadHandle::spawn(move |rx| {
            parse_pt_data(
                rx,
                pc_map_clone,
                empty_buffer_notifier_clone,
                consumer,
                mode,
            )
        });

        let read_handle =
            ThreadHandle::spawn(move |rx| read_pt_data(rx, perf_file_descriptor, producer));

        Self {
            perf_file_descriptor,
            pc_map,
            empty_buffer_notifier,
            read_handle,
            parse_handle,
            write_handle,
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
            read_handle,
            parse_handle,
            write_handle,
            ..
        } = self;

        read_handle.exit();
        parse_handle.exit();
        write_handle.exit();
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
    mapping: SharedPcMap,
    empty_buffer_notifier: Notify,
    mut consumer: Consumer<DATA_BUFFER_SIZE>,
    mode: Mode,
) {
    let mut writer = File::create(OUT_DIR.to_owned() + "intelpt.trace").unwrap();

    let mut terminating = false;

    let mut last_ip = 0;

    loop {
        if let Ok(()) = shutdown_receiver.try_recv() {
            log::trace!("parse terminating");
            terminating = true;
        }

        // read data from consumer, checking for shutdown if empty
        let mut read = match consumer.read() {
            Ok(read) => read,
            Err(bbqueue::Error::InsufficientSize) => {
                if terminating {
                    log::trace!("insufficient size, terminating");
                    return;
                } else {
                    // log::trace!("notify");
                    // empty_buffer_notifier.notify();
                    continue;
                }
            }
            Err(_) => {
                continue;
            }
        };

        let len = read.buf().len();
        log::trace!("read {}", len);

        match parse_single_sync(
            &mut writer,
            mapping.clone(),
            read.buf_mut(),
            &mut last_ip,
            mode,
        ) {
            Ok(idx) => {
                log::trace!("finished, idx: {idx}");
                read.release(idx);
            }
            Err(ParseError::NoSync) => {
                // found no sync points, skip
                log::trace!("skipping {len}");
                read.release(len);
            }
            Err(ParseError::OneSync(start)) => {
                if terminating {
                    // parse all remaining bytes even if there isn't a final sync point
                    log::trace!("parsing remaining bytes from {start}");
                    parse_slice(
                        &mut writer,
                        mapping,
                        &mut read.buf_mut()[start..],
                        &mut last_ip,
                        mode,
                    );
                    return;
                }

                // this should only occur once at the beginning
                log::trace!("releasing {start} initial bytes");
                read.release(start);

                // log::trace!("notify");
                // empty_buffer_notifier.notify();
            }
        }
    }
}

fn find_next_sync_point(slice: &mut [u8]) -> Option<usize> {
    let mut decoder = PacketDecoder::new(&ConfigBuilder::new(slice).unwrap().finish()).unwrap();

    decoder
        .sync_forward()
        .ok()
        .map(|_| decoder.sync_offset().unwrap() as usize)
}

enum ParseError {
    /// Failed to parse slice as no sync points were found
    NoSync,
    /// Failed to parse slice as only a single sync point ({0:?}) was found
    OneSync(usize),
}

/// Parse a single sync range
fn parse_single_sync<W: Write>(
    w: &mut W,
    mapping: SharedPcMap,
    slice: &mut [u8],
    last_ip: &mut u64,
    mode: Mode,
) -> Result<usize, ParseError> {
    let Some(start) = find_next_sync_point(slice) else {
        // slice did not contain any sync points
        return Err(ParseError::NoSync);
    };

    let Some(end) = find_next_sync_point(&mut slice[start + 1..]).map(|idx| idx + start + 1) else {
        // slice only contained a single sync point
        return Err(ParseError::OneSync(start));
    };

    log::trace!("parsing range {start}..{end}");
    parse_slice(w, mapping, &mut slice[start..end], last_ip, mode);

    Ok(end)
}

///
fn parse_slice<W: Write>(
    writer: &mut W,
    mapping: SharedPcMap,
    slice: &mut [u8],
    last_ip: &mut u64,
    mode: Mode,
) {
    let mut decoder = PacketDecoder::new(&ConfigBuilder::new(slice).unwrap().finish()).unwrap();
    decoder.sync_forward().unwrap();
    assert_eq!(decoder.sync_offset().unwrap(), 0);

    loop {
        match decoder.next() {
            Ok(p) => match mode {
                Mode::Tip => match p {
                    Packet::Tip(inner) => {
                        let ip = match inner.compression() {
                            Compression::Suppressed => continue,
                            Compression::Update16 => (*last_ip >> 16) << 16 | inner.tip(),
                            Compression::Update32 => (*last_ip >> 32) << 32 | inner.tip(),
                            Compression::Update48 => (*last_ip >> 32) << 32 | inner.tip(),
                            Compression::Sext48 => (((inner.tip() as i64) << 16) >> 16) as u64,
                            Compression::Full => inner.tip(),
                        };

                        *last_ip = ip;

                        if let Some(guest_pc) = mapping.read().get(&(ip - 9)) {
                            writeln!(writer, "{:x}", guest_pc).unwrap();
                        }
                    }
                    _ => (),
                },
                Mode::Fup => match p {
                    Packet::Fup(inner) => {
                        let ip = inner.fup();

                        if let Some(guest_pc) = mapping.read().get(&(ip - 9)) {
                            writeln!(writer, "{:x}", guest_pc).unwrap();
                        }
                    }
                    _ => (),
                },
                Mode::PtWrite => match p {
                    Packet::Ptw(inner) => {
                        writeln!(writer, "{:x}", inner.payload()).unwrap();
                    }
                    _ => (),
                },
                _ => unreachable!(),
            },

            Err(e) => {
                if e.code() == PtErrorCode::Eos {
                    log::trace!("reached eos");
                    return;
                } else {
                    panic!("{:?}", e);
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
