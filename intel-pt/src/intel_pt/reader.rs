use {
    crate::{
        intel_pt::{
            ring_buffer::RingBuffer,
            thread_handle::{Context, ThreadHandle},
            BUFFER_SIZE,
        },
        Mode,
    },
    bbqueue::Producer,
    perf_event_open_sys::{
        bindings::{perf_event_attr, perf_event_mmap_page},
        perf_event_open,
    },
    std::{
        fs::File,
        io::Read,
        process,
        sync::{
            atomic::{AtomicI32, Ordering},
            Arc,
        },
    },
};

/// Path to the value of the current Intel PT type
const INTEL_PT_TYPE_PATH: &str = "/sys/bus/event_source/devices/intel_pt/type";

const NR_AUX_PAGES: usize = 1024;
const NR_DATA_PAGES: usize = 256;

pub struct Reader {
    handle: ThreadHandle,
}

impl Reader {
    pub fn init(producer: Producer<'static, BUFFER_SIZE>, mode: Mode) -> (Self, Arc<AtomicI32>) {
        let perf_file_descriptor = Arc::new(AtomicI32::new(-1));
        let fd = perf_file_descriptor.clone();
        (
            Self {
                handle: ThreadHandle::spawn(move |ctx| read_pt_data(ctx, fd, producer, mode)),
            },
            perf_file_descriptor,
        )
    }

    pub fn exit(self) {
        self.handle.exit();
    }
}

fn read_pt_data(
    ctx: Context,
    perf_file_descriptor: Arc<AtomicI32>,
    mut producer: Producer<BUFFER_SIZE>,
    mode: Mode,
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

    let mut ring_buffer = RingBuffer::new(mmap, aux_area);

    ctx.ready();

    loop {
        let had_record = ring_buffer.next_record(|buf| {
            let mut grant = producer
                .grant_exact(buf.len())
                .expect(&format!("failed to grant {}", buf.len()));
            grant.buf().copy_from_slice(buf);
            grant.commit(buf.len());
        });

        if !had_record {
            if ctx.received_exit() {
                log::trace!("read terminating");
                return;
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
