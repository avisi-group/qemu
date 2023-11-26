use {
    crate::{
        hardware::{
            decoder::sync::{find_sync_range, ParseError},
            ordered_queue::Sender,
            ring_buffer::RingBufferAux,
            thread_handle::{Context, ThreadHandle},
            PacketParser, NUM_THREADS,
        },
        Mode,
    },
    libipt::{packet::PacketDecoder, ConfigBuilder, PtErrorCode},
    perf_event_open_sys::{
        bindings::{perf_event_attr, perf_event_mmap_page},
        perf_event_open,
    },
    rayon::{ThreadPool, ThreadPoolBuilder},
    std::{
        fs::File,
        io::Read,
        process,
        sync::{
            atomic::{AtomicI32, AtomicU32, Ordering},
            Arc,
        },
    },
};

/// Path to the value of the current Intel PT type
const INTEL_PT_TYPE_PATH: &str = "/sys/bus/event_source/devices/intel_pt/type";

const NR_AUX_PAGES: usize = 16 * 1024;
const NR_DATA_PAGES: usize = 256;

pub struct Reader {
    handle: ThreadHandle,
}

impl Reader {
    pub fn init<P: PacketParser>(
        mode: Mode,
        queue: Sender<Vec<P::ProcessedPacket>>,
        task_count: Arc<AtomicU32>,
    ) -> (Self, Arc<AtomicI32>) {
        let perf_file_descriptor = Arc::new(AtomicI32::new(-1));
        let fd = perf_file_descriptor.clone();
        (
            Self {
                handle: ThreadHandle::spawn(move |ctx| {
                    read_pt_data::<P>(ctx, fd, queue, mode, task_count)
                }),
            },
            perf_file_descriptor,
        )
    }

    pub fn exit(self) {
        self.handle.exit();
    }
}

fn read_pt_data<P: PacketParser>(
    ctx: Context,
    perf_file_descriptor: Arc<AtomicI32>,
    queue: Sender<Vec<P::ProcessedPacket>>,
    mode: Mode,
    task_count: Arc<AtomicU32>,
) {
    let mut pea = perf_event_attr::default();

    // perf event type
    pea.type_ = get_intel_pt_perf_type();

    // Event should start disabled, and not operate in kernel-mode.
    pea.set_disabled(1);
    pea.set_exclude_kernel(1);
    pea.set_exclude_hv(1);
    pea.set_precise_ip(2);

    // 0 pt
    // 1 cyc
    // 2
    // 3

    // 4 pwr_evt
    // 5 fup_on_ptw
    // 7
    // 8

    // 9 mtc
    // 10 tsc
    // 11 noretcomp
    // 12 ptw

    // 13 branch
    // 14-17 mtc_period

    // 19-22 cyc_thresh

    // 24-27 psb_period

    // 31 event

    // 55 notnt
    pea.config = if mode == Mode::PtWrite {
        0b0001_0000_0000_0001
    } else {
        0b0010_0000_0000_0001
    };

    pea.size = std::mem::size_of::<perf_event_attr>() as u32;

    {
        let result = unsafe {
            perf_event_open(
                (&mut pea) as *mut _,
                i32::try_from(process::id()).unwrap(),
                -1,
                -1,
                0,
            )
        };
        if result < 0 {
            println!("last OS error: {:?}", std::io::Error::last_os_error());
            panic!("perf_event_open failed {result}");
        }
        perf_file_descriptor.store(result, Ordering::Relaxed);
    }

    let mmap = memmap2::MmapOptions::new()
        .len((NR_DATA_PAGES + 1) * page_size::get())
        .map_raw(perf_file_descriptor.load(Ordering::Relaxed))
        .unwrap();

    let header = unsafe { &mut *(mmap.as_mut_ptr() as *mut perf_event_mmap_page) };

    header.aux_offset = header.data_offset + header.data_size;
    header.aux_size = (NR_AUX_PAGES * page_size::get()) as u64;

    let aux_area = memmap2::MmapOptions::new()
        .len(header.aux_size as usize)
        .offset(header.aux_offset)
        .map_raw(perf_file_descriptor.load(Ordering::Relaxed))
        .unwrap();

    let mut ring_buffer_aux = RingBufferAux::new(mmap, aux_area);

    let mut task_manager = TaskManager::<P>::new(queue, task_count);
    // let mut w = std::io::BufWriter::new(File::create("/tmp/pt/ptdata.raw").unwrap());

    let mut terminating = false;

    ctx.ready();

    loop {
        // let consumed = ring_buffer_aux.next_data(|buf| {
        //     use std::io::Write;
        //     w.write_all(buf).unwrap();
        //     buf.len()
        // });

        let consumed = ring_buffer_aux.next_data(task_manager.callback(terminating));

        // only perform additional logic if we consumed 0, otherwise immediately call
        // next data
        if consumed == 0 {
            // if we consumed nothing and are terminating, exit
            if terminating {
                log::trace!("read terminating");
                return;
            }

            // if we consumed nothing and are not terminating, run callback with terminating
            // = true
            if ctx.received_exit() {
                log::trace!("reader received exit");
                terminating = true;
                continue;
            }
        }
    }
}

fn get_intel_pt_perf_type() -> u32 {
    let mut intel_pt_type = File::open(INTEL_PT_TYPE_PATH).unwrap();

    let mut buf = String::new();
    intel_pt_type.read_to_string(&mut buf).unwrap();

    buf.trim().parse().unwrap()
}

pub struct TaskManager<P: PacketParser> {
    queue: Sender<Vec<P::ProcessedPacket>>,
    sequence_number: u64,
    pool: ThreadPool,
    task_count: Arc<AtomicU32>,
}

impl<P: PacketParser> TaskManager<P> {
    pub fn new(queue: Sender<Vec<P::ProcessedPacket>>, task_count: Arc<AtomicU32>) -> Self {
        Self {
            queue,
            sequence_number: 0,
            pool: ThreadPoolBuilder::new()
                .num_threads(NUM_THREADS)
                .build()
                .unwrap(),
            task_count,
        }
    }

    pub fn callback(&mut self, terminating: bool) -> impl FnOnce(&[u8]) -> usize + '_ {
        let queue = self.queue.clone();
        let task_count = self.task_count.clone();
        move |buf| {
            let consumed = sync_spawn_task::<P>(
                buf,
                &self.pool,
                terminating,
                queue,
                self.sequence_number,
                task_count,
            );

            if consumed > 0 {
                self.sequence_number += 1;
            }

            consumed
        }
    }
}

/// Find a range of sync points and spawn a task to parse PT data in that range,
/// returning the number of bytes consumed
fn sync_spawn_task<P: PacketParser>(
    buf: &[u8],
    pool: &ThreadPool,
    terminating: bool,
    queue: Sender<Vec<P::ProcessedPacket>>,
    sequence_number: u64,
    task_count: Arc<AtomicU32>,
) -> usize {
    // find the range of bytes alinged to sync points
    let range = match find_sync_range(buf) {
        Ok(range) => range,

        Err(ParseError::NoSync) => {
            panic!("found no sync points");
        }

        // only find a single sync point
        Err(ParseError::OneSync(start)) => {
            if terminating {
                // parse all remaining bytes even if there isn't a final sync point
                let range = start..buf.len();
                log::warn!("parsing remaining bytes in {range:?}");
                range
            } else {
                // we don't have enough data so release 0 bytes and try again
                return 0;
            }
        }
    };

    // not technically necessary, but since it always holds, not holding is probably
    // a bug?
    assert_eq!(0, range.start);

    let data = buf[range.clone()].to_owned();

    task_count.fetch_add(1, Ordering::Relaxed);
    let tc_clone = task_count.clone();
    pool.spawn(move || task_fn::<P>(queue, data, sequence_number, tc_clone));

    range.len()
}

fn task_fn<P: PacketParser>(
    queue: Sender<Vec<P::ProcessedPacket>>,
    mut data: Vec<u8>,
    sequence_number: u64,
    task_count: Arc<AtomicU32>,
) {
    // create a new decoder, synchronise it, and then assert that it synchronised to
    // byte 0
    let mut decoder = PacketDecoder::new(&ConfigBuilder::new(&mut data).unwrap().finish()).unwrap();
    decoder.sync_forward().unwrap();
    assert_eq!(decoder.sync_offset().unwrap(), 0);

    let mut packet_handler = P::new();

    let result = decoder
        .map(|r| r.map(|p| packet_handler.process(p)))
        .collect::<Result<(), _>>();

    if let Err(e) = result {
        if e.code() != PtErrorCode::Eos {
            panic!("error while decoding: {e}");
        }
    }

    // push processed data into queue to be picked up by writer
    queue.send(sequence_number, packet_handler.finish());

    task_count.fetch_sub(1, Ordering::Relaxed);
}
